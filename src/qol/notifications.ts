import { useEffect, useRef } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { BusState } from "../bus/store";
import type { FsEvent, PassiveSignal, RepoDelta } from "../bus/contract";
import { busStore, useBusState } from "../bus/store";
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
        key: `repo-error:${delta.repo}:${delta.revision}:${delta.error.category}`,
        title: repoTitle(name),
        body: `Error terminal del repositorio: ${label(delta.error.category)}.`,
      });
    }

    for (const signal of criticalSignals(delta)) {
      notifications.push({
        key: `signal:${delta.repo}:${delta.revision}:${signal.kind}:${signal.path ?? "repo"}`,
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

export async function enableNotifications(adapter: NotificationAdapter = tauriNotificationAdapter) {
  qualityStore.setNotificationState({ status: "checking", message: null });
  const permission = await adapter.ensurePermission();
  if (permission === "granted") {
    qualityStore.setNotificationState({ enabled: true, status: "ready", message: null });
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
}

export function NotificationWatcher({
  adapter = tauriNotificationAdapter,
}: {
  adapter?: NotificationAdapter;
}) {
  const busState = useBusState();
  const { notificationsEnabled } = useQualityState();
  const sent = useRef(new Set<string>());

  useEffect(() => {
    if (!notificationsEnabled) return;
    for (const notification of collectRelevantNotifications(busState, (repo) =>
      busStore.displayName(repo),
    )) {
      if (sent.current.has(notification.key)) continue;
      sent.current.add(notification.key);
      try {
        void adapter.send(notification);
      } catch {
        qualityStore.setNotificationState({
          enabled: false,
          status: "unavailable",
          message: "Las notificaciones fallaron en este entorno.",
        });
      }
    }
  }, [adapter, busState, notificationsEnabled]);

  return null;
}
