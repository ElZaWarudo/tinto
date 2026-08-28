// Curated TypeScript facade for the FROZEN backend↔frontend contract
// (docs/contracts/bus-contract.md / src-tauri/src/bus/contract.rs).
// `contract.generated.ts` is generated from Rust as the drift guard; this file
// keeps frontend-facing docs, compatibility comments, and non-bus adjunct types.
// Additive-first. serde serializes Rust field names as-is (snake_case); the
// lowercase enums map to lowercase string unions.
//
// Timestamp units (KTD8): `last_activity_ms` and FsEvent `timestamp_ms` are
// epoch MILLISECONDS; `CommitInfo.timestamp` is epoch SECONDS.

// ---- Event names (emit, backend → frontend) ----
export const EVENT_WORKBENCH_DELTA = "tinto://workbench-delta";
export const EVENT_FS_EVENTS = "tinto://fs-events";
export const EVENT_WATCHING_STATE = "tinto://watching-state";
export const EVENT_AGENT_SESSIONS_CHANGED = "tinto://agent-sessions-changed";
export const EVENT_AGENT_SESSION_OUTPUT = "tinto://agent-session-output";
export const EVENT_AGENT_SESSION_CHANGE_LOG = "tinto://agent-session-change-log";
export const EVENT_AGENT_SESSION_TIMELINE = "tinto://agent-session-timeline";

// ---- Agent console sessions (ACI-001) ----
export type AgentProviderSource = "local" | "wsl";

export type AgentProviderReadinessState = "unavailable" | "binary_available";

export interface AgentProviderReadiness {
  agent_type: string;
  source: AgentProviderSource;
  distro?: string | null;
  state: AgentProviderReadinessState;
}

export type AgentInstallPrivilege = "none";

export type AgentInstallOutcomeKind =
  | "verified"
  | "unsupported_recipe"
  | "missing_prerequisite"
  | "authorization_declined"
  | "cancelled"
  | "spawn_failed"
  | "installer_failed"
  | "timeout"
  | "cleanup_failed"
  | "verification_failed"
  | "launch_failed";

export interface AgentInstallPreview {
  attempt_id: string;
  agent_type: string;
  display_name: string;
  source: AgentProviderSource;
  distro?: string | null;
  installer: string;
  command_display: string;
  arguments: string[];
  global_effect: string;
  privilege: AgentInstallPrivilege;
  recipe_revision: string;
  expires_at_ms: number;
}

export interface AgentInstallOutcome {
  outcome: AgentInstallOutcomeKind;
  verified_version?: string | null;
  session_id?: string | null;
  message: string;
}

export type AgentSessionAcpState =
  | "unavailable"
  | "authentication_required"
  | "connecting_acp"
  | "acp_ready"
  | "pty_compatibility"
  | "failed";

export type AgentSessionAcpMode = "acp" | "pty";

export type AgentSessionAcpConfigCategory = "model" | "mode";

export interface AgentSessionAcpConfigValue {
  id: string;
  label: string;
}

export interface AgentSessionAcpConfigOption {
  id: string;
  label: string;
  category: AgentSessionAcpConfigCategory;
  current_value: string;
  values: AgentSessionAcpConfigValue[];
}

export interface AgentSessionAcpRuntime {
  state: AgentSessionAcpState;
  mode?: AgentSessionAcpMode | null;
  detail?: string | null;
  lost_capabilities?: string[];
  retry_available: boolean;
  image_attachments?: boolean;
  config_options?: AgentSessionAcpConfigOption[];
}

export type AgentSessionAcpPermissionKind =
  "allow_once" | "allow_always" | "reject_once" | "reject_always";

export type AgentSessionAcpPermissionState =
  "pending" | "allowed" | "denied" | "cancelled" | "expired" | "invalidated";

export interface AgentSessionAcpPermissionOption {
  id: string;
  label: string;
  kind: AgentSessionAcpPermissionKind;
}

export interface AgentSessionAcpPermission {
  id: string;
  generation: number;
  provider_session_id: string;
  turn_id: string;
  tool_call_id: string;
  title: string;
  options: AgentSessionAcpPermissionOption[];
  state: AgentSessionAcpPermissionState;
  reason?: string | null;
  expires_at_ms: number;
}

export type AgentSessionStatus =
  "starting" | "running" | "exited" | "error" | "completed" | "failed" | "reverted";

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

export type AgentSessionTurnStatus = "waiting" | "working" | "settling";

export interface AgentSessionTurnCheckpoint {
  id: string;
  index: number;
  started_at_ms: number;
  ended_at_ms: number;
  checkpoint: AgentSessionCheckpoint;
  restore_checkpoint?: AgentSessionCheckpoint | null;
  changes: AgentSessionChange[];
}

export interface AgentSessionLimits {
  max_sessions: number;
  max_sessions_per_repo: number;
  max_lifetime_ms: number;
}

export interface AgentSessionRuntimeOptions {
  model?: string | null;
  reasoning_effort?: string | null;
  speed?: string | null;
}

export type AgentSessionPermissionMode = "workspace" | "full_access";

export type AgentRuntimeCatalogStatus = "loading" | "ready" | "error";

export interface AgentRuntimeReasoningEffort {
  value: string;
  description: string;
}

export interface AgentRuntimeServiceTier {
  id: string;
  name: string;
  description: string;
}

export interface AgentRuntimeModel {
  id: string;
  model: string;
  display_name: string;
  description: string;
  supported_reasoning_efforts: AgentRuntimeReasoningEffort[];
  default_reasoning_effort: string;
  service_tiers?: AgentRuntimeServiceTier[];
  default_service_tier?: string | null;
  is_default: boolean;
}

export interface AgentRuntimeCatalog {
  status: AgentRuntimeCatalogStatus;
  source: string;
  models?: AgentRuntimeModel[];
  default_model?: string | null;
  error?: string | null;
  updated_at_ms: number;
}

export type AgentSessionGoalStatus =
  "active" | "paused" | "blocked" | "usage_limited" | "budget_limited" | "complete";

export interface AgentSessionGoal {
  text: string;
  status: AgentSessionGoalStatus;
  token_budget?: number | null;
  tokens_used: number;
  time_used_seconds: number;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface AgentSessionPersonality {
  name: string;
  updated_at_ms: number;
}

export interface AgentSessionPlanMode {
  enabled: boolean;
  updated_at_ms: number;
}

export interface AgentSessionFeedback {
  kind: string;
  text: string;
  created_at_ms: number;
}

export interface AgentSessionContextSummary {
  text: string;
  created_at_ms: number;
  source_events: number;
  source_turns: number;
}

export interface AgentSessionContextUsage {
  used_tokens: number;
  model_context_window: number;
}

export type AgentSessionResumeMode = "native" | "context_bridge";

export interface AgentSessionResumeResult {
  session_id: string;
  mode: AgentSessionResumeMode;
}

export interface AgentSession {
  id: string;
  repo: string;
  agent_type: string;
  permission_mode?: AgentSessionPermissionMode | null;
  permission_mode_change_supported?: boolean;
  acp_runtime?: AgentSessionAcpRuntime | null;
  acp_permissions?: AgentSessionAcpPermission[];
  provider_session_id?: string | null;
  wsl_distro?: string | null;
  status: AgentSessionStatus;
  pid: number | null;
  started_at_ms: number;
  ended_at_ms?: number | null;
  exit_code: number | null;
  error: AgentSessionError | null;
  checkpoint?: AgentSessionCheckpoint | null;
  change_log?: AgentSessionChange[];
  turn_status: AgentSessionTurnStatus;
  turn_checkpoints?: AgentSessionTurnCheckpoint[];
  timeline?: AgentSessionTimelineItem[];
  subagents?: AgentSubagentThread[];
  runtime_options?: AgentSessionRuntimeOptions;
  goal?: AgentSessionGoal | null;
  personality?: AgentSessionPersonality | null;
  plan_mode?: AgentSessionPlanMode | null;
  feedback?: AgentSessionFeedback[];
  context_summary?: AgentSessionContextSummary | null;
  context_usage?: AgentSessionContextUsage | null;
  turn_interrupt_supported: boolean;
  reverted_at_ms?: number | null;
  restored_to_turn_index?: number | null;
  active_sessions: number;
  age_ms: number;
  output_bytes_per_second?: number | null;
}

export interface AgentSubagentCapabilities {
  inspect: boolean;
  direct_input: boolean;
  steer: boolean;
  interrupt: boolean;
  wait: boolean;
  close: boolean;
}

export interface AgentSubagentActivity {
  id: string;
  kind: string;
  status: string;
  text: string;
  timestamp_ms: number;
}

export interface AgentSubagentResult {
  status: string;
  summary?: string | null;
  error?: string | null;
  updated_at_ms: number;
}

export interface AgentSubagentThread {
  id: string;
  parent_id?: string | null;
  source_kind: string;
  depth: number;
  agent_path?: string[];
  nickname?: string | null;
  role?: string | null;
  model?: string | null;
  reasoning_effort?: string | null;
  runtime?: string | null;
  approval_policy?: string | null;
  permission_mode?: string | null;
  capacity?: number | null;
  thread_status: string;
  turn_status: string;
  collaboration_status?: string | null;
  collaboration_tool?: string | null;
  consolidation_id?: string | null;
  runtime_state?: string | null;
  approval_request_id?: string | null;
  prompt?: string | null;
  preview?: string | null;
  capabilities: AgentSubagentCapabilities;
  activities?: AgentSubagentActivity[];
  result?: AgentSubagentResult | null;
  timeline?: AgentSessionTimelineItem[];
  updated_at_ms: number;
}

export interface AgentSessionOutput {
  session_id: string;
  chunk_base64: string;
  timestamp_ms: number;
}

export type AgentSessionTimelineKind =
  | "user_message"
  | "steer_message"
  | "agent_message"
  | "agent_progress"
  | "command_output"
  | "activity"
  | "lifecycle";

export interface AgentSessionTimelineItem {
  session_id: string;
  id: string;
  kind: AgentSessionTimelineKind;
  text: string;
  timestamp_ms: number;
  attachments?: AgentSessionAttachment[];
}

export interface AgentSessionAttachment {
  path: string;
  name: string;
  is_image: boolean;
}

export interface AgentJournalSessionSummary {
  id: string;
  repo: string;
  agent_type: string;
  wsl_distro?: string | null;
  status: AgentSessionStatus;
  started_at_ms: number;
  ended_at_ms?: number | null;
  updated_at_ms: number;
  event_count: number;
  first_user_message?: string | null;
  last_event_kind?: AgentSessionTimelineKind | null;
  last_event_text?: string | null;
  last_event_at_ms?: number | null;
}

export type AgentHostCommandStatus = "completed" | "unavailable";

export interface AgentReviewSummary {
  branch: string;
  changed_files: number;
  working_shortstat?: string | null;
  staged_shortstat?: string | null;
  files: string[];
  truncated_count: number;
}

export interface AgentReviewFinding {
  severity: string;
  title: string;
  detail: string;
  path?: string | null;
  line?: number | null;
}

export interface AgentHostCommandResult {
  command: string;
  status: AgentHostCommandStatus;
  message: string;
  session_id?: string | null;
  repo?: string | null;
  agent_type?: string | null;
  review_summary?: AgentReviewSummary | null;
  review_findings?: AgentReviewFinding[] | null;
}

export interface CommandError {
  category: string;
  message: string;
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
  "sensitive_path" | "possible_secret" | "large_delete" | "config_change" | "test_change";

export interface PassiveSignal {
  kind: PassiveSignalKind;
  severity: SignalSeverity;
  path?: string | null;
  message: string;
}

export interface SecretFinding {
  path: string;
  line: number;
  rule_id: string;
  description: string;
}

export type SecretScanEngine = "gitleaks" | "heuristic";

export type SecretScanState = "not_run" | "clean" | "findings" | "degraded";

export interface SecretScanStatus {
  state: SecretScanState;
  engine?: SecretScanEngine | null;
  version?: string | null;
  failure_category?: string | null;
  message?: string | null;
  checked_at_ms?: number | null;
}

export interface RepoMetrics {
  changed_files: number;
  lines_added: number;
  lines_removed: number;
}

export interface GitleaksSetupStatus {
  installed: boolean;
  version: string | null;
  binary_path: string | null;
}

export interface GitleaksInstallResult {
  installed: boolean;
  version: string | null;
  binary_path: string | null;
  method: string | null;
  message: string;
}

export interface RepoFetchPreview {
  remote: string;
  host: string;
  sanitized_url: string;
}

export interface RepoFetchResult {
  remote: string;
  host: string;
  fetched_at_ms: number;
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
  metrics: RepoMetrics;
  gitleaks_configured: boolean;
  agents_md_configured: boolean;
  signals?: PassiveSignal[]; // RDM-011 additive
  secret_findings?: SecretFinding[]; // additive
  secret_scan_status: SecretScanStatus;
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

export type ContentEncoding = "utf8" | "base64";
export type FileEncoding = ContentEncoding;

export interface FileContent {
  encoding: ContentEncoding;
  content: string;
  truncated: boolean; // true when the command-specific read guard cut the content
}

// ---- Snapshot ----
export interface WorkbenchSnapshot {
  watching: WatchingState;
  repos: RepoDelta[];
}

// ---- Project MCP inventory/profile adjuncts (RDM-024) ----
// These fields are additive to the frozen generated map.  They intentionally
// carry only source-bound identity and command availability; provider args,
// env, headers, paths, and credentials never cross the bus.
export type McpProvider = "codex";
export type McpTarget = "windows_local";
export type McpInventoryStatus = "empty" | "success" | "partial" | "error" | "unsupported";
export type McpDeliveryStatus = "unknown" | "unsupported";

export interface McpDefinition {
  provider: McpProvider;
  target: McpTarget;
  source: string;
  name: string;
  command_available?: boolean | null;
}

export interface McpDefinitionRef {
  provider: McpProvider;
  target: McpTarget;
  source: string;
  name: string;
}

export interface McpInventory {
  provider: McpProvider;
  target: McpTarget;
  status: McpInventoryStatus;
  definitions: McpDefinition[];
  error?: string | null;
  checked_at_ms: number;
}

export interface McpProfile {
  id: string;
  name: string;
  definitions: McpDefinitionRef[];
}

export interface McpProfileState {
  profiles: McpProfile[];
  active_profile_id?: string | null;
  delivery_status: McpDeliveryStatus;
}

// ---- Workbench config (from list_workbenches; source of names/aliases) ----
export type RepoSource = "local" | "wsl";

export interface RepoEntry {
  source?: RepoSource;
  path: string;
  distro?: string | null;
  alias: string | null;
  fs_watch: string[];
}

export interface Workbench {
  name: string;
  repos: RepoEntry[];
  mcp_profiles?: McpProfile[];
  mcp_default_profile?: string | null;
}

export interface WorkbenchConfig {
  version: number;
  active: string | null;
  workbenches: Workbench[];
}

// ---- File operations (drag/drop/paste/move) ----
export type FileConflictKind = "file_exists" | "dir_exists" | "source_missing" | "overwrite";

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
  /** Limpiezas auxiliares pendientes tras una mutación ya completada. */
  warnings?: string[];
}

export interface FileOpOutcome {
  warnings: string[];
}

export interface DeletedEntry {
  path: string;
  is_dir: boolean;
}

export interface DeleteResult {
  token: string;
  entries: DeletedEntry[];
  completed?: boolean;
  recovery_required?: boolean;
  warnings?: string[];
}

// The generated Rust mirror exposes the same map. contract.parity.ts checks
// both key coverage and bidirectional assignability during every TypeScript build.
export interface CuratedBusContractTypeMap {
  RepoStatus: RepoStatus;
  BranchInfo: BranchInfo;
  CommitInfo: CommitInfo;
  DiffLineKind: DiffLineKind;
  DiffLine: DiffLine;
  DiffHunk: DiffHunk;
  FileDiff: FileDiff;
  AgentProviderSource: AgentProviderSource;
  AgentProviderReadinessState: AgentProviderReadinessState;
  AgentProviderReadiness: AgentProviderReadiness;
  AgentInstallPrivilege: AgentInstallPrivilege;
  AgentInstallOutcomeKind: AgentInstallOutcomeKind;
  AgentInstallPreview: AgentInstallPreview;
  AgentInstallOutcome: AgentInstallOutcome;
  AgentSessionAcpState: AgentSessionAcpState;
  AgentSessionAcpMode: AgentSessionAcpMode;
  AgentSessionAcpConfigCategory: AgentSessionAcpConfigCategory;
  AgentSessionAcpConfigValue: AgentSessionAcpConfigValue;
  AgentSessionAcpConfigOption: AgentSessionAcpConfigOption;
  AgentSessionAcpRuntime: AgentSessionAcpRuntime;
  AgentSessionAcpPermissionKind: AgentSessionAcpPermissionKind;
  AgentSessionAcpPermissionState: AgentSessionAcpPermissionState;
  AgentSessionAcpPermissionOption: AgentSessionAcpPermissionOption;
  AgentSessionAcpPermission: AgentSessionAcpPermission;
  AgentSessionStatus: AgentSessionStatus;
  AgentSessionError: AgentSessionError;
  AgentSessionCheckpointType: AgentSessionCheckpointType;
  AgentSessionCheckpoint: AgentSessionCheckpoint;
  AgentSessionChangeKind: AgentSessionChangeKind;
  AgentSessionChange: AgentSessionChange;
  AgentSessionChangeLog: AgentSessionChangeLog;
  AgentSessionTurnStatus: AgentSessionTurnStatus;
  AgentSessionTurnCheckpoint: AgentSessionTurnCheckpoint;
  AgentSessionLimits: AgentSessionLimits;
  AgentSessionRuntimeOptions: AgentSessionRuntimeOptions;
  AgentSessionPermissionMode: AgentSessionPermissionMode;
  AgentRuntimeCatalogStatus: AgentRuntimeCatalogStatus;
  AgentRuntimeReasoningEffort: AgentRuntimeReasoningEffort;
  AgentRuntimeServiceTier: AgentRuntimeServiceTier;
  AgentRuntimeModel: AgentRuntimeModel;
  AgentRuntimeCatalog: AgentRuntimeCatalog;
  AgentSessionGoalStatus: AgentSessionGoalStatus;
  AgentSessionGoal: AgentSessionGoal;
  AgentSessionPersonality: AgentSessionPersonality;
  AgentSessionPlanMode: AgentSessionPlanMode;
  AgentSessionFeedback: AgentSessionFeedback;
  AgentSessionContextSummary: AgentSessionContextSummary;
  AgentSessionContextUsage: AgentSessionContextUsage;
  AgentSubagentCapabilities: AgentSubagentCapabilities;
  AgentSubagentActivity: AgentSubagentActivity;
  AgentSubagentResult: AgentSubagentResult;
  AgentSubagentThread: AgentSubagentThread;
  // The generated wire requires this capability flag; the curated facade keeps
  // it optional so legacy archived sessions remain assignable.
  AgentSession: AgentSession & { permission_mode_change_supported: boolean };
  AgentSessionOutput: AgentSessionOutput;
  AgentSessionTimelineKind: AgentSessionTimelineKind;
  AgentSessionAttachment: AgentSessionAttachment;
  AgentSessionTimelineItem: AgentSessionTimelineItem;
  AgentJournalSessionSummary: AgentJournalSessionSummary;
  AgentHostCommandStatus: AgentHostCommandStatus;
  AgentReviewSummary: AgentReviewSummary;
  AgentReviewFinding: AgentReviewFinding;
  AgentHostCommandResult: AgentHostCommandResult;
  GitleaksSetupStatus: GitleaksSetupStatus;
  GitleaksInstallResult: GitleaksInstallResult;
  RepoFetchPreview: RepoFetchPreview;
  RepoFetchResult: RepoFetchResult;
  CommandError: CommandError;
  RepoErrorClass: RepoErrorClass;
  RepoErrorState: RepoErrorState;
  SignalSeverity: SignalSeverity;
  PassiveSignalKind: PassiveSignalKind;
  PassiveSignal: PassiveSignal;
  RepoMetrics: RepoMetrics;
  SecretFinding: SecretFinding;
  SecretScanEngine: SecretScanEngine;
  SecretScanState: SecretScanState;
  SecretScanStatus: SecretScanStatus;
  RepoDelta: RepoDelta;
  FsEventKind: FsEventKind;
  FsEvent: FsEvent;
  FsEventBatch: FsEventBatch;
  WatchingState: WatchingState;
  SubscriptionTarget: SubscriptionTarget;
  TreeEntry: TreeEntry;
  RepoTree: RepoTree;
  FileContent: FileContent;
  ContentEncoding: ContentEncoding;
  WorkbenchSnapshot: WorkbenchSnapshot;
  McpProvider: McpProvider;
  McpTarget: McpTarget;
  McpInventoryStatus: McpInventoryStatus;
  McpDeliveryStatus: McpDeliveryStatus;
  McpDefinition: McpDefinition;
  McpDefinitionRef: McpDefinitionRef;
  McpInventory: McpInventory;
  McpProfile: McpProfile;
  McpProfileState: McpProfileState;
}
