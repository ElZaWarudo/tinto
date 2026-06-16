// Live workbench store: holds per-repo state keyed by canonical path, applies
// the contract revision rule (apply a delta only if its revision is newer),
// tracks activity for the live indicator, and joins display names from the
// workbench config. The state object is replaced on each mutation but unchanged
// per-repo delta references are preserved, so memoized cards re-render only when
// their own repo changes.

import { useSyncExternalStore } from "react";
import type {
  FileDiff,
  FsEvent,
  FsEventBatch,
  PassiveSignal,
  RepoDelta,
  RepoMetrics,
  SignalSeverity,
  WatchingState,
  WorkbenchConfig,
} from "./contract";

export const MAX_FS_EVENTS_PER_REPO = 50;

export const EMPTY_METRICS: RepoMetrics = {
  changed_files: 0,
  lines_added: 0,
  lines_removed: 0,
};

export interface BusState {
  /** Latest delta per canonical repo path. */
  repos: Record<string, RepoDelta>;
  /** Last activity (epoch ms) per repo: max of delta + fs-event timestamps. */
  activity: Record<string, number>;
  /** Live diffs per repo, keyed by file path (RDM-008). A repo key present
   * (even `{}`) means a diff computation has occurred for it — see KTD2. */
  diffs: Record<string, Record<string, FileDiff>>;
  /** Recent Plane-2 filesystem events per repo, newest first (RDM-009). */
  fsEventsByRepo: Record<string, FsEvent[]>;
  watching: WatchingState;
  /** Workbench config (names/aliases/active); null until loaded. */
  config: WorkbenchConfig | null;
  /** True once the first snapshot has been applied (distinguishes loading). */
  loaded: boolean;
}

const EMPTY: BusState = {
  repos: {},
  activity: {},
  diffs: {},
  fsEventsByRepo: {},
  watching: { available: true },
  config: null,
  loaded: false,
};

export function basename(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : path;
}

/** Build a repo's diff map from a delta's `subscribed_diffs`, or null when the
 * delta omits them (`null`/`undefined`) — the caller retains on null (KTD2). */
function sliceFromDelta(delta: RepoDelta): Record<string, FileDiff> | null {
  const sub = delta.subscribed_diffs;
  if (sub == null) return null;
  const map: Record<string, FileDiff> = {};
  for (const fd of sub) map[fd.path] = fd;
  return map;
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
    const diffs: Record<string, Record<string, FileDiff>> = {};
    const fsEventsByRepo: Record<string, FsEvent[]> = {};
    for (const d of repos) {
      const existing = this.state.repos[d.repo];
      const winner = existing && existing.revision > d.revision ? existing : d;
      repoMap[d.repo] = winner;
      const prevActivity = this.state.activity[d.repo] ?? 0;
      activity[d.repo] = Math.max(prevActivity, winner.last_activity_ms);
      // Diffs: a subscribed snapshot carries them; else retain any in-flight diff
      // for a repo that survives membership. Repos absent from the snapshot drop.
      const sliced = sliceFromDelta(winner);
      if (sliced) diffs[d.repo] = sliced;
      else if (this.state.diffs[d.repo]) diffs[d.repo] = this.state.diffs[d.repo];
      if (this.state.fsEventsByRepo[d.repo]) {
        fsEventsByRepo[d.repo] = this.state.fsEventsByRepo[d.repo];
      }
    }
    this.set({
      ...this.state,
      repos: repoMap,
      activity,
      diffs,
      fsEventsByRepo,
      watching,
      loaded: true,
    });
  }

  /** Apply a delta, honoring the monotonic-revision rule.
   *
   * Diff rule (KTD2): `subscribed_diffs == null` ⇒ retain the repo's diffs (a
   * status-only / error delta must not blank an open diff — R5a); a non-null
   * array is authoritative for the repo's subscribed targets ⇒ replace-set the
   * repo's diff map (a reverted file omitted from the array clears, an empty
   * array sets `{}` which still marks "computed"). */
  applyDelta(delta: RepoDelta) {
    const existing = this.state.repos[delta.repo];
    if (existing && existing.revision >= delta.revision) return; // stale
    const prevActivity = this.state.activity[delta.repo] ?? 0;
    const sliced = sliceFromDelta(delta);
    const diffs =
      sliced === null
        ? this.state.diffs // retain
        : { ...this.state.diffs, [delta.repo]: sliced }; // replace-set
    this.set({
      ...this.state,
      repos: { ...this.state.repos, [delta.repo]: delta },
      activity: {
        ...this.state.activity,
        [delta.repo]: Math.max(prevActivity, delta.last_activity_ms),
      },
      diffs,
    });
  }

  /** Drop a single target's diff (on diff-panel close). */
  dropDiff(repo: string, path: string) {
    const repoDiffs = this.state.diffs[repo];
    if (!repoDiffs || !(path in repoDiffs)) return;
    const next = { ...repoDiffs };
    delete next[path];
    this.set({ ...this.state, diffs: { ...this.state.diffs, [repo]: next } });
  }

  /** Plane-2 events bump the repo's activity without a git recompute. */
  applyFsEvents(batch: FsEventBatch) {
    if (!this.state.repos[batch.repo] || batch.events.length === 0) return;
    const maxTs = batch.events.reduce((m, e) => Math.max(m, e.timestamp_ms), 0);
    const prev = this.state.activity[batch.repo] ?? 0;
    const nextEvents = [...batch.events, ...(this.state.fsEventsByRepo[batch.repo] ?? [])]
      .sort((a, b) => b.timestamp_ms - a.timestamp_ms)
      .slice(0, MAX_FS_EVENTS_PER_REPO);
    this.set({
      ...this.state,
      activity:
        maxTs > prev ? { ...this.state.activity, [batch.repo]: maxTs } : this.state.activity,
      fsEventsByRepo: { ...this.state.fsEventsByRepo, [batch.repo]: nextEvents },
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
    this.set({ ...this.state, repos: {}, activity: {}, diffs: {}, fsEventsByRepo: {} });
  }

  /** Full reset to the empty state (primarily for tests). */
  resetAll() {
    this.set({ ...EMPTY });
  }

  /** Display name for a repo path: alias from config, else the basename. */
  displayName(path: string): string {
    const wb = (this.state.config?.workbenches ?? []).find((w) =>
      w.repos.some((r) => r.path === path),
    );
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

/** The live diff for a target, or undefined if none is held (RDM-008). */
export function getDiff(state: BusState, repo: string, path: string): FileDiff | undefined {
  return state.diffs[repo]?.[path];
}

/** Recent Plane-2 events for a repo, newest first (RDM-009). */
export function getFsEvents(state: BusState, repo: string): FsEvent[] {
  return state.fsEventsByRepo[repo] ?? [];
}

export function getRepoMetrics(delta: RepoDelta | undefined): RepoMetrics {
  return delta?.metrics ?? EMPTY_METRICS;
}

export function getRepoSignals(delta: RepoDelta | undefined): PassiveSignal[] {
  return delta?.signals ?? [];
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

export function getPathSignals(delta: RepoDelta | undefined, path: string): PassiveSignal[] {
  const target = normalizePath(path);
  return getRepoSignals(delta).filter(
    (signal) => signal.path && normalizePath(signal.path) === target,
  );
}

function severityRank(severity: SignalSeverity): number {
  if (severity === "critical") return 0;
  if (severity === "warning") return 1;
  return 2;
}

export function sortSignals(signals: PassiveSignal[]): PassiveSignal[] {
  return [...signals].sort((a, b) => {
    const bySeverity = severityRank(a.severity) - severityRank(b.severity);
    if (bySeverity !== 0) return bySeverity;
    return `${a.path ?? ""}:${a.kind}`.localeCompare(`${b.path ?? ""}:${b.kind}`);
  });
}

export function signalCounts(signals: PassiveSignal[]): Record<SignalSeverity, number> {
  return {
    critical: signals.filter((s) => s.severity === "critical").length,
    warning: signals.filter((s) => s.severity === "warning").length,
    info: signals.filter((s) => s.severity === "info").length,
  };
}

/** True once a diff computation has occurred for a repo (lets a panel tell
 * "loading" from "clean / no-renderable" — KTD2 / R7). */
export function hasComputedDiffs(state: BusState, repo: string): boolean {
  return state.diffs[repo] !== undefined;
}

/** Singleton store for the app. */
export const busStore = new BusStore();

/** Subscribe a component to the whole bus state. */
export function useBusState(): BusState {
  return useSyncExternalStore(busStore.subscribe, busStore.getState);
}
