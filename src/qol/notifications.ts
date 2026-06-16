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
      body: "Watching degraded. Data remains available on demand.",
    });
  }

  for (const delta of Object.values(state.repos)) {
    const name = displayName(delta.repo);

    if (delta.error?.class === "terminal") {
      notifications.push({
        key: `repo-error:${delta.repo}:${delta.revision}:${delta.error.category}`,
        title: repoTitle(name),
        body: `Terminal repo error: ${label(delta.error.category)}.`,
      });
    }

    for (const signal of criticalSignals(delta)) {
      notifications.push({
        key: `signal:${delta.repo}:${delta.revision}:${signal.kind}:${signal.path ?? "repo"}`,
        title: repoTitle(name),
        body: `Critical ${label(signal.kind)} signal detected.`,
      });
    }

    for (const event of state.fsEventsByRepo[delta.repo] ?? []) {
      for (const signal of warningFsSignals(event)) {
        notifications.push({
          key: `fs-signal:${delta.repo}:${event.timestamp_ms}:${event.kind}:${event.path}:${signal.kind}`,
          title: repoTitle(name),
          body: `${label(signal.severity)} watched-file signal detected.`,
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
      message: "Notifications denied by the OS.",
    });
    return false;
  }
  qualityStore.setNotificationState({
    enabled: false,
    status: "unavailable",
    message: "Notifications unavailable in this runtime.",
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
          message: "Notifications failed in this runtime.",
        });
      }
    }
  }, [adapter, busState, notificationsEnabled]);

  return null;
}
