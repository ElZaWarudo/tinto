// Live workbench store: holds per-repo state keyed by canonical path, applies
// the contract revision rule (apply a delta only if its revision is newer),
// tracks activity for the live indicator, and joins display names from the
// workbench config. The state object is replaced on each mutation but unchanged
// per-repo delta references are preserved, so memoized cards re-render only when
// their own repo changes.

import { useSyncExternalStore } from "react";
import type { FsEventBatch, RepoDelta, WatchingState, WorkbenchConfig } from "./contract";

export interface BusState {
  /** Latest delta per canonical repo path. */
  repos: Record<string, RepoDelta>;
  /** Last activity (epoch ms) per repo: max of delta + fs-event timestamps. */
  activity: Record<string, number>;
  watching: WatchingState;
  /** Workbench config (names/aliases/active); null until loaded. */
  config: WorkbenchConfig | null;
  /** True once the first snapshot has been applied (distinguishes loading). */
  loaded: boolean;
}

const EMPTY: BusState = {
  repos: {},
  activity: {},
  watching: { available: true },
  config: null,
  loaded: false,
};

export function basename(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

export class BusStore {
  private state: BusState = EMPTY;
  private listeners = new Set<() => void>();

  getState = (): BusState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(next: BusState) {
    this.state = next;
    this.listeners.forEach((l) => l());
  }

  /** Seed from a full snapshot (workbench switch / startup).
   *
   * Authoritative for MEMBERSHIP (repos absent from the snapshot are dropped),
   * but honors the contract's monotonic-revision rule per repo: a higher-
   * revision delta that arrived while the snapshot IPC was in flight is kept
   * rather than clobbered by the older snapshot. Activity is preserved as the
   * max of any prior (e.g. fs-event) activity and the snapshot's. */
  loadSnapshot(repos: RepoDelta[], watching: WatchingState) {
    const repoMap: Record<string, RepoDelta> = {};
    const activity: Record<string, number> = {};
    for (const d of repos) {
      const existing = this.state.repos[d.repo];
      const winner = existing && existing.revision > d.revision ? existing : d;
      repoMap[d.repo] = winner;
      const prevActivity = this.state.activity[d.repo] ?? 0;
      activity[d.repo] = Math.max(prevActivity, winner.last_activity_ms);
    }
    this.set({ ...this.state, repos: repoMap, activity, watching, loaded: true });
  }

  /** Apply a delta, honoring the monotonic-revision rule. */
  applyDelta(delta: RepoDelta) {
    const existing = this.state.repos[delta.repo];
    if (existing && existing.revision >= delta.revision) return; // stale
    const prevActivity = this.state.activity[delta.repo] ?? 0;
    this.set({
      ...this.state,
      repos: { ...this.state.repos, [delta.repo]: delta },
      activity: {
        ...this.state.activity,
        [delta.repo]: Math.max(prevActivity, delta.last_activity_ms),
      },
    });
  }

  /** Plane-2 events bump the repo's activity without a git recompute. */
  applyFsEvents(batch: FsEventBatch) {
    if (!this.state.repos[batch.repo] || batch.events.length === 0) return;
    const maxTs = batch.events.reduce((m, e) => Math.max(m, e.timestamp_ms), 0);
    const prev = this.state.activity[batch.repo] ?? 0;
    if (maxTs <= prev) return;
    this.set({
      ...this.state,
      activity: { ...this.state.activity, [batch.repo]: maxTs },
    });
  }

  setWatching(watching: WatchingState) {
    this.set({ ...this.state, watching });
  }

  setConfig(config: WorkbenchConfig) {
    this.set({ ...this.state, config });
  }

  /** Clear live repo state (on workbench switch); config is reloaded separately. */
  reset() {
    this.set({ ...this.state, repos: {}, activity: {} });
  }

  /** Full reset to the empty state (primarily for tests). */
  resetAll() {
    this.set({ ...EMPTY });
  }

  /** Display name for a repo path: alias from config, else the basename. */
  displayName(path: string): string {
    const wb = this.state.config?.workbenches.find((w) => w.repos.some((r) => r.path === path));
    const entry = wb?.repos.find((r) => r.path === path);
    return entry?.alias ?? basename(path);
  }
}

/** Canonical repo paths of the active workbench, ordered by display name. */
export function sortedRepoPaths(store: BusStore, state: BusState): string[] {
  return Object.keys(state.repos).sort((a, b) =>
    store.displayName(a).localeCompare(store.displayName(b)),
  );
}

/** Compact status summary: "clean" or "{m}M {s}S {u}U". */
export function statusSummary(status: {
  modified: string[];
  staged: string[];
  untracked: string[];
}): string {
  const { modified, staged, untracked } = status;
  if (!modified.length && !staged.length && !untracked.length) return "clean";
  return `${modified.length}M ${staged.length}S ${untracked.length}U`;
}

/** Convert a CommitInfo unix-seconds timestamp to a JS Date (KTD8). */
export function commitDate(unixSeconds: number): Date {
  return new Date(unixSeconds * 1000);
}

/** Singleton store for the app. */
export const busStore = new BusStore();

/** Subscribe a component to the whole bus state. */
export function useBusState(): BusState {
  return useSyncExternalStore(busStore.subscribe, busStore.getState);
}
