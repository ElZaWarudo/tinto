// Wires the live bus into the store: loads the initial snapshot + workbench
// config and attaches the delta/fs-events/watching-state listeners. StrictMode-
// safe via the `active` guard + unlisten cleanup (KTD6). Call once near the app
// root. `reloadActiveWorkbench` re-seeds after a workbench switch.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { busStore } from "./store";
import type { FsEventBatch, RepoDelta } from "./contract";
import { agentSessionStore } from "../agent/sessionStore";
import { repoTreeStore } from "../workspace/repoTreeStore";
import {
  getWorkbenchSnapshot,
  listAgentSessions,
  listWorkbenches,
  onAgentSessionChangeLog,
  onAgentSessionOutput,
  onFsEvents,
  onWatchingState,
  onWorkbenchDelta,
} from "./client";

export async function reloadActiveWorkbench(): Promise<void> {
  try {
    const config = await listWorkbenches();
    busStore.setConfig(config);
  } catch {
    /* config is best-effort; names fall back to basenames */
  }
  try {
    const snapshot = await getWorkbenchSnapshot();
    busStore.loadSnapshot(snapshot.repos, snapshot.watching);
  } catch {
    /* leave prior state; a watching banner / errors surface separately */
  }
  try {
    const sessions = await listAgentSessions();
    agentSessionStore.setSessions(sessions);
  } catch {
    /* agent sessions are best-effort and ephemeral */
  }
}

export function useBusConnection(): void {
  useEffect(() => {
    let active = true;
    const applyDelta = (delta: RepoDelta) => {
      if (!active) return;
      if (busStore.applyDelta(delta)) {
        repoTreeStore.ensureLoaded(delta.repo);
      }
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
      onAgentSessionChangeLog(
        (log) => active && agentSessionStore.applyChangeLog(log.session_id, log.changes),
      ),
      onAgentSessionOutput((output) => active && agentSessionStore.appendOutput(output)),
    ];
    void reloadActiveWorkbench();
    return () => {
      active = false;
      pending.forEach((p) => void p.then((fn) => fn()));
    };
  }, []);
}
