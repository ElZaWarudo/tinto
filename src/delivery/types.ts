// Delivery mode's contract with the backend (src-tauri/src/delivery/model.rs).
// Kept apart from the frozen bus contract: Delivery evolves on its own.

export type DeliveryJobStatus =
  "queued" | "pending" | "started" | "finished" | "failed" | "cancelled" | "interrupted";

export type DeliveryAgent = "codex" | "claude" | "shell";
export type DeliveryAccess = "workspace" | "full";
export type DeliveryResultState = "accepted" | "stale" | "invalid";
export type DeliveryLeaseState = "free" | "active" | "quarantined";
export type DeliveryRung = "commit" | "push" | "pr" | "jira";
export type DeliveryApprovalStatus = "pending" | "approved" | "rejected" | "executed" | "failed";

export interface DeliveryTask {
  id: string;
  run_id: string | null;
  repo: string;
  distro: string | null;
  key: string;
  title: string;
  worktree: string;
  branch: string;
  base_ref: string;
  base_commit: string;
  state: string;
  contract_version: number;
  created_at_ms: number;
  updated_at_ms: number;
  removed_at_ms?: number | null;
}

export interface DeliveryCheck {
  command: string;
  result: string;
}

export interface DeliveryJobResult {
  status: "pass" | "findings" | "blocked" | string;
  summary: string;
  changed_paths: string[];
  checks: DeliveryCheck[];
  findings: string[];
  handoff: string;
}

export interface DeliveryChange {
  path: string;
  kind: "created" | "modified" | "removed" | string;
}

export interface DeliveryJob {
  id: string;
  task_id: string;
  role: string;
  attempt: number;
  agent: DeliveryAgent;
  model: string | null;
  access: DeliveryAccess;
  prompt: string;
  contract_version: number;
  writes: boolean;
  lease: string | null;
  timeout_minutes: number;
  status: DeliveryJobStatus;
  created_at_ms: number;
  started_at_ms: number | null;
  ended_at_ms: number | null;
  pid: number | null;
  provider_session_id: string | null;
  exit_code: number | null;
  error: string | null;
  start_candidate: string | null;
  end_candidate: string | null;
  changes: DeliveryChange[];
  result: DeliveryJobResult | null;
  result_state: DeliveryResultState | null;
  result_note: string | null;
  undone_at_ms: number | null;
  /** Commands a Claude job without full access may run. */
  allowed_commands?: string[];
  /** QA jobs: whether it had a browser. */
  qa_browser?: boolean;
  /** The task's answered decisions as the job received them. */
  decisions?: string[];
}

export interface DeliveryLeaseWaiter {
  job_id: string;
  task_id: string;
  requested_at_ms: number;
}

export interface DeliveryLease {
  name: string;
  state: DeliveryLeaseState;
  generation: number;
  holder_job_id: string | null;
  holder_task_id: string | null;
  acquired_at_ms: number | null;
  note: string | null;
  queue: DeliveryLeaseWaiter[];
}

export interface DeliveryApproval {
  id: string;
  task_id: string;
  rung: DeliveryRung;
  title: string;
  body: string;
  status: DeliveryApprovalStatus;
  requested_by: string;
  requested_at_ms: number;
  decided_at_ms: number | null;
  executed_at_ms: number | null;
  outcome: string | null;
}

export type DeliveryDecisionKind = "choice" | "text" | "permission";

export interface DeliveryDecisionOption {
  label: string;
  consequence: string;
  recommended: boolean;
}

/** Something the user settles before a task's stages run; answers are final. */
export interface DeliveryDecision {
  id: string;
  task_id: string;
  kind: DeliveryDecisionKind;
  /** Plain language, for the user. */
  question: string;
  /** Technical context, folded. */
  detail: string;
  options: DeliveryDecisionOption[];
  /** `text`: the proposed text, exactly as it will be shown. */
  text: string;
  /** `permission`: the command QA may run once allowed. */
  command: string | null;
  /** `permission`: how to undo it. */
  undo: string;
  status: "pending" | "answered";
  /** The chosen option, the approved text, or "allowed"/"denied". */
  answer: string | null;
  requested_by: string;
  requested_at_ms: number;
  decided_by: string | null;
  decided_at_ms: number | null;
}

export interface DeliveryRun {
  id: string;
  repo: string;
  title: string;
  status: "active" | "closed" | string;
  generation: number;
  owner: string | null;
  created_at_ms: number;
  updated_at_ms: number;
  /** Post QA verdicts as Jira comments; null until the user sets it. */
  qa_jira_comment?: boolean | null;
}

export interface DeliverySettings {
  capacity: number;
}

export interface DeliveryRepoSettings {
  worktree_root: string | null;
  bootstrap: string | null;
  default_base: string | null;
  /** Verification commands; Claude jobs without full access may run only these. */
  checks?: string[];
  /** More commands QA jobs may run (e.g. the CLI an issue changes). */
  qa_commands?: string[];
  /** Whether QA jobs get a headless browser. */
  qa_browser?: boolean;
  /** Where QA runs on this machine, in the user's words. */
  qa_environment?: string;
}

export interface DeliveryCoordinatorEndpoint {
  url: string;
  token: string;
}

export interface DeliveryOverview {
  runs: DeliveryRun[];
  tasks: DeliveryTask[];
  jobs: DeliveryJob[];
  leases: DeliveryLease[];
  approvals: DeliveryApproval[];
  decisions: DeliveryDecision[];
  settings: DeliverySettings;
  coordinator: DeliveryCoordinatorEndpoint | null;
}

export interface DeliveryLogEntry {
  kind: "message" | "command" | "error" | "info" | string;
  text: string;
}

export interface DeliveryJobLog {
  entries: DeliveryLogEntry[];
  next_line: number;
  stderr_tail: string;
}

export interface DeliveryConversation {
  session_id: string;
  repo: string;
  agent_type: string;
}
