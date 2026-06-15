// Wires the live bus into the store: loads the initial snapshot + workbench
// config and attaches the delta/fs-events/watching-state listeners. StrictMode-
// safe via the `active` guard + unlisten cleanup (KTD6). Call once near the app
// root. `reloadActiveWorkbench` re-seeds after a workbench switch.

import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { busStore } from "./store";
import {
  getWorkbenchSnapshot,
  listWorkbenches,
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
}

export function useBusConnection(): void {
  useEffect(() => {
    let active = true;
    const pending: Promise<UnlistenFn>[] = [
      onWorkbenchDelta((d) => active && busStore.applyDelta(d)),
      onFsEvents((b) => active && busStore.applyFsEvents(b)),
      onWatchingState((w) => active && busStore.setWatching(w)),
    ];
    void reloadActiveWorkbench();
    return () => {
      active = false;
      pending.forEach((p) => void p.then((fn) => fn()));
    };
  }, []);
}
