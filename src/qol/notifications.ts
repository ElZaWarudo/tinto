import { useEffect, useRef } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { BusState } from "../bus/store";
import type { AgentSession, FsEvent, PassiveSignal, RepoDelta } from "../bus/contract";
import { busStore, useBusState } from "../bus/store";
import { sessionSignal } from "../agent/sessionSignal";
import { useAgentSessionState } from "../agent/sessionStore";
import { qualityStore, useQualityState } from "./state";

export interface NotificationAdapter {
  ensurePermission: () => Promise<"granted" | "denied" | "unavailable">;
  send: (notification: RedactedNotification) => Promise<void> | void;
}

export interface RedactedNotification {
  key: string;
  title: string;
  body: string;
}

export const tauriNotificationAdapter: NotificationAdapter = {
  async ensurePermission() {
    try {
      if (await isPermissionGranted()) return "granted";
      return (await requestPermission()) === "granted" ? "granted" : "denied";
    } catch {
      return "unavailable";
    }
  },
  send(notification) {
    sendNotification({ title: notification.title, body: notification.body });
  },
};

function label(value: string): string {
  const labels: Record<string, string> = {
    critical: "crítica",
    warning: "advertencia",
    info: "informativa",
    sensitive_path: "archivo sensible",
    possible_secret: "posible secreto",
    large_delete: "borrado grande",
    config_change: "cambio de configuración",
    test_change: "cambio de pruebas",
  };
  if (labels[value]) return labels[value];
  return value.replace(/_/g, " ");
}

function repoTitle(repoName: string): string {
  return `Tinto: ${repoName}`;
}

function criticalSignals(delta: RepoDelta): PassiveSignal[] {
  return (delta.signals ?? []).filter((signal) => signal.severity === "critical");
}

function warningFsSignals(event: FsEvent): PassiveSignal[] {
  return (event.signals ?? []).filter(
    (signal) => signal.severity === "warning" || signal.severity === "critical",
  );
}

export function collectRelevantNotifications(
  state: BusState,
  displayName: (repo: string) => string,
): RedactedNotification[] {
  const notifications: RedactedNotification[] = [];

  if (!state.watching.available) {
    notifications.push({
      key: `watching:degraded:${state.watching.reason ?? "unknown"}`,
      title: "Tinto: Workbench",
      body: "Observación degradada. Los datos siguen disponibles bajo demanda.",
    });
  }

  for (const delta of Object.values(state.repos)) {
    const name = displayName(delta.repo);

    if (delta.error?.class === "terminal") {
      notifications.push({
        key: `repo-error:${delta.repo}:${delta.error.category}`,
        title: repoTitle(name),
        body: `Error terminal del repositorio: ${label(delta.error.category)}.`,
      });
    }

    for (const signal of criticalSignals(delta)) {
      notifications.push({
        key: `signal:${delta.repo}:${signal.kind}:${signal.path ?? "repo"}`,
        title: repoTitle(name),
        body: `Se detectó una señal crítica: ${label(signal.kind)}.`,
      });
    }

    for (const event of state.fsEventsByRepo[delta.repo] ?? []) {
      for (const signal of warningFsSignals(event)) {
        notifications.push({
          key: `fs-signal:${delta.repo}:${event.timestamp_ms}:${event.kind}:${event.path}:${signal.kind}`,
          title: repoTitle(name),
          body: `Se detectó una señal de archivo observado: ${label(signal.severity)}.`,
        });
      }
    }
  }

  return notifications;
}

/** One notification per agent session that starts needing the user (a
 *  permission or an approval waits), and the sessions that need them now:
 *  pass those back next time so a session is announced again only after it
 *  stopped needing you. */
export function agentNeedsNotifications(
  sessions: AgentSession[],
  needingBefore: ReadonlySet<string>,
  displayName: (repo: string) => string,
): { notifications: RedactedNotification[]; needing: Set<string> } {
  const needing = new Set(
    sessions.filter((session) => sessionSignal(session) === "needs_you").map(({ id }) => id),
  );
  const notifications = sessions
    .filter((session) => needing.has(session.id) && !needingBefore.has(session.id))
    .map((session) => ({
      key: `agent-needs:${session.id}`,
      title: repoTitle(displayName(session.repo)),
      body: `Un Agent (${session.agent_type}) te necesita: tiene un permiso pendiente.`,
    }));
  return { notifications, needing };
}

/** The user's choice in Ajustes → Notificaciones, kept across restarts. */
const PREFERENCE_KEY = "tinto.notifications";

function rememberChoice(enabled: boolean) {
  try {
    localStorage.setItem(PREFERENCE_KEY, enabled ? "on" : "off");
  } catch {
    // Without storage the switch only lasts this session.
  }
}

/** Whether the user left notifications on last time. */
export function notificationsWereOn(): boolean {
  try {
    return localStorage.getItem(PREFERENCE_KEY) === "on";
  } catch {
    return false;
  }
}

export async function enableNotifications(adapter: NotificationAdapter = tauriNotificationAdapter) {
  qualityStore.setNotificationState({ status: "checking", message: null });
  const permission = await adapter.ensurePermission();
  if (permission === "granted") {
    qualityStore.setNotificationState({ enabled: true, status: "ready", message: null });
    rememberChoice(true);
    return true;
  }
  if (permission === "denied") {
    qualityStore.setNotificationState({
      enabled: false,
      status: "denied",
      message: "El sistema operativo denegó las notificaciones.",
    });
    return false;
  }
  qualityStore.setNotificationState({
    enabled: false,
    status: "unavailable",
    message: "Las notificaciones no están disponibles en este entorno.",
  });
  return false;
}

export function disableNotifications() {
  qualityStore.setNotificationState({ enabled: false, status: "idle", message: null });
  rememberChoice(false);
}

function sendOrMarkUnavailable(
  adapter: NotificationAdapter,
  notification: RedactedNotification,
  onFailure: () => void = () => {},
) {
  const markUnavailable = () => {
    onFailure();
    qualityStore.setNotificationState({
      enabled: false,
      status: "unavailable",
      message: "Las notificaciones fallaron en este entorno.",
    });
  };
  try {
    void Promise.resolve(adapter.send(notification)).catch(markUnavailable);
  } catch {
    markUnavailable();
  }
}

export function NotificationWatcher({
  adapter = tauriNotificationAdapter,
}: {
  adapter?: NotificationAdapter;
}) {
  const busState = useBusState();
  const { sessions } = useAgentSessionState();
  const { notificationsEnabled } = useQualityState();
  const sent = useRef(new Set<string>());
  const agentsNeeding = useRef<ReadonlySet<string>>(new Set());

  // Turned on last time: on again at startup, once the system still allows it.
  useEffect(() => {
    if (notificationsWereOn()) void enableNotifications(adapter);
  }, [adapter]);

  useEffect(() => {
    if (!notificationsEnabled) return;
    const current = collectRelevantNotifications(busState, (repo) => busStore.displayName(repo));
    // A condition that cleared is forgotten, so it is announced again if it returns.
    const currentKeys = new Set(current.map(({ key }) => key));
    for (const key of sent.current) if (!currentKeys.has(key)) sent.current.delete(key);
    for (const notification of current) {
      if (sent.current.has(notification.key)) continue;
      sent.current.add(notification.key);
      sendOrMarkUnavailable(adapter, notification, () => sent.current.delete(notification.key));
    }
  }, [adapter, busState, notificationsEnabled]);

  // Followed with notifications off too, so turning them on does not announce
  // what already needed you.
  useEffect(() => {
    const { notifications, needing } = agentNeedsNotifications(
      Object.values(sessions),
      agentsNeeding.current,
      (repo) => busStore.displayName(repo),
    );
    agentsNeeding.current = needing;
    if (!notificationsEnabled) return;
    for (const notification of notifications) sendOrMarkUnavailable(adapter, notification);
  }, [adapter, sessions, notificationsEnabled]);

  return null;
}
