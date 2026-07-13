// Wires the live bus into the store: loads the initial snapshot + workbench
// config and attaches the delta/fs-events/watching-state listeners. StrictMode-
// safe via the `active` guard + unlisten cleanup (KTD6). Call once near the app
// root. `reloadActiveWorkbench` re-seeds after a workbench switch.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { busStore } from "./store";
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
  } catch {
    /* agent sessions are best-effort and ephemeral */
  }
}

function snapshotFailureReason(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("transformCallback")) {
    return "El puente de Tauri no está disponible; abre Tinto como aplicación de escritorio para cargar los datos de los repositorios.";
  }
  return message || "No se pudieron cargar los datos de los repositorios.";
}

export function useBusConnection(): void {
  useEffect(() => {
    let active = true;
    const applyDelta = (delta: RepoDelta) => {
      if (!active) return;
      busStore.applyDelta(delta);
    };
    const applyFsEvents = (batch: FsEventBatch) => {
      if (!active) return;
      busStore.applyFsEvents(batch);
      repoTreeStore.refresh(batch.repo);
    };
    const pending: Promise<UnlistenFn>[] = [
      onWorkbenchDelta(applyDelta),
      onFsEvents(applyFsEvents),
      onWatchingState((w) => active && busStore.setWatching(w)),
      onAgentSessionsChanged((sessions) => active && agentSessionStore.setSessions(sessions)),
      onAgentSessionChangeLog(
        (log) => active && agentSessionStore.applyChangeLog(log.session_id, log.changes),
      ),
      onAgentSessionOutput((output) => active && agentSessionStore.appendOutput(output)),
      onAgentSessionTimeline((item) => active && agentSessionStore.appendTimelineItem(item)),
    ];
    void reloadActiveWorkbench();
    return () => {
      active = false;
      pending.forEach((p) => void p.then((fn) => fn()));
    };
  }, []);
}
