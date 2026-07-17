// Wires the live bus into the store: loads the initial snapshot + workbench
// config and attaches the delta/fs-events/watching-state listeners. StrictMode-
// safe via the `active` guard + unlisten cleanup (KTD6). Call once near the app
// root. `reloadActiveWorkbench` re-seeds after a workbench switch.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { busStore, type BusConnectionChannel } from "./store";
import type { FsEventBatch, RepoDelta, WorkbenchConfig } from "./contract";
import { agentSessionStore } from "../agent/sessionStore";
import { repoTreeStore } from "../workspace/repoTreeStore";
import {
  getWorkbenchSnapshot,
  listAgentSessions,
  listWorkbenches,
  onAgentSessionsChanged,
  onAgentSessionChangeLog,
  onAgentSessionOutput,
  onAgentSessionTimeline,
  onFsEvents,
  onWatchingState,
  onWorkbenchDelta,
} from "./client";

let activeReloadGeneration = 0;
const LISTENER_RETRY_MS = 1_000;

function nextReloadGeneration(): number {
  activeReloadGeneration += 1;
  return activeReloadGeneration;
}

function isCurrentReload(generation: number): boolean {
  return generation === activeReloadGeneration;
}

export async function reloadActiveWorkbench(): Promise<void> {
  const generation = nextReloadGeneration();
  const previous = busStore.getState();
  const hadUsableWorkbench = previous.config !== null && previous.loaded;
  busStore.beginConfigLoad();
  busStore.beginSnapshotLoad();
  let config: WorkbenchConfig;
  try {
    config = await listWorkbenches();
    if (!isCurrentReload(generation)) return;
    if (!hadUsableWorkbench) {
      busStore.setConfig(config);
    }
  } catch (error) {
    if (!isCurrentReload(generation)) return;
    const reason = snapshotFailureReason(error);
    busStore.setConfigError(reason);
    busStore.setSnapshotError("No se actualizó la instantánea porque falló la configuración.");
    return;
  }
  try {
    const snapshot = await getWorkbenchSnapshot();
    if (!isCurrentReload(generation)) return;
    busStore.loadWorkbench(config, snapshot.repos, snapshot.watching);
  } catch (error) {
    if (!isCurrentReload(generation)) return;
    const reason = snapshotFailureReason(error);
    if (!hadUsableWorkbench) {
      busStore.loadWorkbench(config, [], {
        available: false,
        reason,
      });
    } else {
      busStore.finishConfigLoad();
    }
    busStore.setSnapshotError(reason);
  }
  try {
    const sessions = await listAgentSessions();
    if (!isCurrentReload(generation)) return;
    agentSessionStore.setSessions(sessions);
    busStore.clearConnectionError("agent-session-list");
  } catch (error) {
    if (!isCurrentReload(generation)) return;
    busStore.setConnectionError(
      "agent-session-list",
      connectionFailureReason("Listado de sesiones Agent", error),
    );
  }
}

function snapshotFailureReason(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("transformCallback")) {
    return "El puente de Tauri no está disponible; abre Tinto como aplicación de escritorio para cargar los datos de los repositorios.";
  }
  return message || "No se pudieron cargar los datos de los repositorios.";
}

function connectionFailureReason(channelLabel: string, error: unknown): string {
  return `${channelLabel}: ${snapshotFailureReason(error)}`;
}

export function useBusConnection(): void {
  useEffect(() => {
    let active = true;
    const unlisteners = new Set<UnlistenFn>();
    const retryTimers = new Set<number>();
    let agentSessionListRetryTimer: number | null = null;

    const scheduleAgentSessionListRetry = (): void => {
      if (!active || agentSessionListRetryTimer !== null) return;
      agentSessionListRetryTimer = window.setTimeout(() => {
        agentSessionListRetryTimer = null;
        void listAgentSessions().then(
          (sessions) => {
            if (!active) return;
            agentSessionStore.setSessions(sessions);
            busStore.clearConnectionError("agent-session-list");
          },
          (error) => {
            if (!active) return;
            busStore.setConnectionError(
              "agent-session-list",
              connectionFailureReason("Listado de sesiones Agent", error),
            );
            scheduleAgentSessionListRetry();
          },
        );
      }, LISTENER_RETRY_MS);
    };

    const syncAgentSessionListRetry = (): void => {
      if (busStore.getState().connectionErrors["agent-session-list"]) {
        scheduleAgentSessionListRetry();
        return;
      }
      if (agentSessionListRetryTimer !== null) {
        window.clearTimeout(agentSessionListRetryTimer);
        agentSessionListRetryTimer = null;
      }
    };
    const unsubscribeAgentSessionRetry = busStore.subscribe(syncAgentSessionListRetry);
    syncAgentSessionListRetry();

    const attachListener = (
      channel: BusConnectionChannel,
      label: string,
      subscribe: () => Promise<UnlistenFn>,
    ): void => {
      const scheduleRetry = (error: unknown) => {
        if (!active) return;
        busStore.setConnectionError(channel, connectionFailureReason(label, error));
        const timer = window.setTimeout(() => {
          retryTimers.delete(timer);
          attachListener(channel, label, subscribe);
        }, LISTENER_RETRY_MS);
        retryTimers.add(timer);
      };

      let pending: Promise<UnlistenFn>;
      try {
        pending = subscribe();
      } catch (error) {
        scheduleRetry(error);
        return;
      }

      void pending.then((unlisten) => {
        if (!active) {
          unlisten();
          return;
        }
        unlisteners.add(unlisten);
        busStore.clearConnectionError(channel);
      }, scheduleRetry);
    };

    const applyDelta = (delta: RepoDelta) => {
      if (!active) return;
      busStore.applyDelta(delta);
    };
    const applyFsEvents = (batch: FsEventBatch) => {
      if (!active) return;
      busStore.applyFsEvents(batch);
      repoTreeStore.refresh(batch.repo);
    };

    attachListener("repo-deltas", "Canal de cambios de repositorios", () =>
      onWorkbenchDelta(applyDelta),
    );
    attachListener("file-events", "Canal de eventos de archivos", () => onFsEvents(applyFsEvents));
    attachListener("watching-state", "Canal de estado de supervisión", () =>
      onWatchingState((watching) => active && busStore.setWatching(watching)),
    );
    attachListener("agent-sessions", "Canal de sesiones Agent", () =>
      onAgentSessionsChanged((sessions) => {
        if (!active) return;
        agentSessionStore.setSessions(sessions);
        busStore.clearConnectionError("agent-session-list");
      }),
    );
    attachListener("agent-changes", "Canal de cambios de Agent", () =>
      onAgentSessionChangeLog(
        (log) => active && agentSessionStore.applyChangeLog(log.session_id, log.changes),
      ),
    );
    attachListener("agent-output", "Canal de salida de Agent", () =>
      onAgentSessionOutput((output) => active && agentSessionStore.appendOutput(output)),
    );
    attachListener("agent-timeline", "Canal de cronología de Agent", () =>
      onAgentSessionTimeline((item) => active && agentSessionStore.appendTimelineItem(item)),
    );
    void reloadActiveWorkbench();
    return () => {
      active = false;
      unsubscribeAgentSessionRetry();
      if (agentSessionListRetryTimer !== null) {
        window.clearTimeout(agentSessionListRetryTimer);
      }
      retryTimers.forEach((timer) => window.clearTimeout(timer));
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);
}
