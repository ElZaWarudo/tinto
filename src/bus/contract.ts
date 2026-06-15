// TypeScript mirror of the FROZEN backend↔frontend contract
// (docs/contracts/bus-contract.md / src-tauri/src/bus/contract.rs). Hand-kept
// in sync; additive-first. serde serializes Rust field names as-is (snake_case);
// the lowercase enums map to lowercase string unions.
//
// Timestamp units (KTD8): `last_activity_ms` and FsEvent `timestamp_ms` are
// epoch MILLISECONDS; `CommitInfo.timestamp` is epoch SECONDS.

// ---- Event names (emit, backend → frontend) ----
export const EVENT_WORKBENCH_DELTA = "tinto://workbench-delta";
export const EVENT_FS_EVENTS = "tinto://fs-events";
export const EVENT_WATCHING_STATE = "tinto://watching-state";

// ---- Git value types ----
export interface RepoStatus {
  modified: string[];
  staged: string[];
  untracked: string[];
}

export interface BranchInfo {
  name: string | null; // null in detached HEAD
  detached: boolean;
  unborn: boolean;
  ahead: number | null; // null when no upstream
  behind: number | null;
}

export interface CommitInfo {
  id: string;
  summary: string;
  author: string;
  timestamp: number; // unix SECONDS
}

// ---- Repo state ----
export type RepoErrorClass = "transient" | "terminal";

export interface RepoErrorState {
  class: RepoErrorClass;
  category: string;
  message: string;
}

export interface FileDiff {
  path: string;
  old_path: string | null;
  is_binary: boolean;
  hunks: unknown[]; // diff hunks; only consumed by RDM-008
}

export interface RepoDelta {
  repo: string; // canonical path (opaque identity)
  revision: number; // monotonic per repo
  status: RepoStatus;
  branch: BranchInfo | null;
  head: CommitInfo | null;
  last_activity_ms: number; // epoch ms
  error: RepoErrorState | null;
  subscribed_diffs?: FileDiff[] | null; // RDM-008
}

// ---- Plane 2 (watched files) ----
export type FsEventKind = "created" | "modified" | "removed";

export interface FsEvent {
  path: string;
  kind: FsEventKind;
  timestamp_ms: number; // epoch ms
  size: number | null;
  size_delta: number | null;
}

export interface FsEventBatch {
  repo: string;
  events: FsEvent[];
}

// ---- Watching availability ----
export interface WatchingState {
  available: boolean;
  reason?: string | null;
}

// ---- Snapshot ----
export interface WorkbenchSnapshot {
  watching: WatchingState;
  repos: RepoDelta[];
}

// ---- Workbench config (from list_workbenches; source of names/aliases) ----
export interface RepoEntry {
  path: string;
  alias: string | null;
  fs_watch: string[];
}

export interface Workbench {
  name: string;
  repos: RepoEntry[];
}

export interface WorkbenchConfig {
  version: number;
  active: string | null;
  workbenches: Workbench[];
}
