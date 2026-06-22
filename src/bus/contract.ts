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
export const EVENT_AGENT_SESSION_OUTPUT = "tinto://agent-session-output";
export const EVENT_AGENT_SESSION_CHANGE_LOG = "tinto://agent-session-change-log";

// ---- Agent console sessions (ACI-001) ----
export type AgentSessionStatus =
  | "starting"
  | "running"
  | "exited"
  | "error"
  | "completed"
  | "failed"
  | "reverted";

export interface AgentSessionError {
  category: string;
  message: string;
}

export type AgentSessionCheckpointType = "git_ref" | "fs_snapshot";

export interface AgentSessionCheckpoint {
  checkpoint_type: AgentSessionCheckpointType;
  git_hash?: string | null;
  snapshot_files: string[];
}

export type AgentSessionChangeKind = "created" | "modified" | "removed";

export interface AgentSessionChange {
  path: string;
  kind: AgentSessionChangeKind;
  timestamp_ms: number;
}

export interface AgentSessionChangeLog {
  session_id: string;
  changes: AgentSessionChange[];
}

export interface AgentSessionLimits {
  max_sessions: number;
  max_sessions_per_repo: number;
  max_lifetime_ms: number;
}

export interface AgentSession {
  id: string;
  repo: string;
  agent_type: string;
  status: AgentSessionStatus;
  pid: number | null;
  started_at_ms: number;
  ended_at_ms?: number | null;
  exit_code: number | null;
  error: AgentSessionError | null;
  checkpoint?: AgentSessionCheckpoint | null;
  change_log?: AgentSessionChange[];
  reverted_at_ms?: number | null;
  active_sessions: number;
  age_ms: number;
  output_bytes_per_second?: number | null;
}

export interface AgentSessionOutput {
  session_id: string;
  chunk_base64: string;
  timestamp_ms: number;
}

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

// ---- Passive signals (RDM-011) ----
export type SignalSeverity = "info" | "warning" | "critical";

export type PassiveSignalKind =
  | "sensitive_path"
  | "possible_secret"
  | "large_delete"
  | "config_change"
  | "test_change";

export interface PassiveSignal {
  kind: PassiveSignalKind;
  severity: SignalSeverity;
  path?: string | null;
  message: string;
}

export interface RepoMetrics {
  changed_files: number;
  lines_added: number;
  lines_removed: number;
}

// ---- Diff types (consumed by RDM-008) ----
// serde serializes the Rust enum variants as-is: PascalCase (no rename_all).
export type DiffLineKind = "Added" | "Removed" | "Context";

export interface DiffLine {
  kind: DiffLineKind;
  content: string;
  old_lineno: number | null; // null for Added
  new_lineno: number | null; // null for Removed
}

export interface DiffHunk {
  old_start: number;
  new_start: number;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  old_path: string | null;
  is_binary: boolean;
  hunks: DiffHunk[];
}

export interface RepoDelta {
  repo: string; // canonical path (opaque identity)
  revision: number; // monotonic per repo
  status: RepoStatus;
  branch: BranchInfo | null;
  head: CommitInfo | null;
  last_activity_ms: number; // epoch ms
  error: RepoErrorState | null;
  metrics?: RepoMetrics; // RDM-011 additive
  signals?: PassiveSignal[]; // RDM-011 additive
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
  signals?: PassiveSignal[]; // RDM-011 additive
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

// ---- Subscriptions / tree / file content (RDM-008) ----
export interface SubscriptionTarget {
  repo: string;
  path?: string | null; // null/omitted = whole-repo (RDM-008 uses file targets only)
}

export interface TreeEntry {
  path: string; // relative to the repo root, with "/" separators
  is_dir: boolean;
}

export interface RepoTree {
  entries: TreeEntry[];
  truncated: boolean; // true when the 20k-entry cap was hit
}

export type FileEncoding = "utf8" | "base64";

export interface FileContent {
  encoding: FileEncoding;
  content: string;
  truncated: boolean; // true when the command-specific read guard cut the content
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

// ---- File operations (drag/drop/paste/move) ----
export type FileConflictKind =
  | "file_exists"
  | "dir_exists"
  | "source_missing"
  | "overwrite";

export interface FileConflict {
  /** Destino relativo al repo. */
  dest_rel: string;
  kind: FileConflictKind;
}

export interface CopyResult {
  /** Paths relativos al repo de los archivos/actualizados. */
  copied: string[];
  /** Conflictos detectados (vacío si todo OK). */
  conflicts: FileConflict[];
}
