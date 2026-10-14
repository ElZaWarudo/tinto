// Delivery mode's contract with the backend (src-tauri/src/delivery/model.rs).
// Kept apart from the frozen bus contract: Delivery evolves on its own.

export type DeliveryJobStatus =
  "queued" | "pending" | "started" | "finished" | "failed" | "cancelled" | "interrupted";

export type DeliveryAgent = "codex" | "claude" | "shell";
export type DeliveryAccess = "workspace" | "full";
export type DeliveryResultState = "accepted" | "stale" | "invalid";
export type DeliveryLeaseState = "free" | "active" | "quarantined";
export type DeliveryRung = "commit" | "push" | "pr" | "merge" | "jira";
export type DeliveryApprovalStatus =
  /** `undone`: a commit step whose commits were taken back; it is pending again. */
  "pending" | "approved" | "rejected" | "executed" | "failed" | "undone";

/** One commit of a commit step's plan. */
export interface DeliveryCommit {
  /** The exact message: subject, then an optional body after a blank line. */
  message: string;
  /** The files it includes, relative to the worktree. */
  paths: string[];
}

/** What the form to prepare a commit starts from. */
export interface DeliveryCommitDraft {
  /** Every file with uncommitted changes. */
  files: string[];
  /** Subjects of the branch's latest commits. */
  recent: string[];
}

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
  /** What stopped the job: its own sandbox or rules, something outside, or nothing. */
  blocked_by?: "none" | "environment" | "sandbox" | string;
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
  /** Reasoning effort passed to the CLI; its default when null. */
  reasoning?: string | null;
  /** QA jobs: whether it had a browser. */
  qa_browser?: boolean;
  /** The task's answered decisions as the job received them. */
  decisions?: string[];
  /** Where a Claude job on a Windows repo was asked to run; Tinto chose when null. */
  platform?: DeliveryPlatform | null;
  /** Where the job ran ("Windows", "WSL · Ubuntu"…), once started. */
  ran_on?: string | null;
  /** QA jobs: folders outside the worktree the permissions let it read. */
  read_dirs?: string[];
  /** QA jobs: folders outside the worktree the permissions let it write in. */
  write_dirs?: string[];
  /** QA jobs: the approved scripts and the frozen copies it ran instead. */
  frozen_scripts?: { original: string; copy: string }[];
}

export type DeliveryPlatform = "windows" | "wsl";

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
  /** `merge` only: squash, merge or rebase. */
  merge_method?: string | null;
  title: string;
  body: string;
  status: DeliveryApprovalStatus;
  requested_by: string;
  requested_at_ms: number;
  decided_at_ms: number | null;
  executed_at_ms: number | null;
  outcome: string | null;
  /** `commit`: the commits to create, in order. */
  commits?: DeliveryCommit[];
  /** `commit`: changed files the plan leaves uncommitted on purpose. */
  left_out?: string[];
  /** `commit`: the commits Tinto created. */
  created?: string[];
  /** What Tinto noticed checking the step against git and GitHub. */
  notice?: string | null;
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
  /** `permission`: the scripts the command runs, frozen when it was asked. */
  scripts?: { path: string; content: string }[];
  /** `permission`: folders outside the worktree QA may read and list. */
  read_dirs?: string[];
  /** `permission`: folders outside the worktree QA may write in. */
  write_dirs?: string[];
  /** `permission`: the user's real clients ("claude", "codex") the command starts. */
  clients?: string[];
  /** `permission`: once allowed, the coordinator's QA on this task runs with full access. */
  full_access?: boolean;
  /** `permission`: why it may not work as asked, found when it was requested. */
  warnings?: string[];
  /** `permission`: it runs the task's code, so it holds for one candidate. */
  runs_candidate?: boolean;
  /** `permission`: agent-editable files its scripts name that are not frozen. */
  unfrozen?: string[];
  /** The worktree content (tree id) a `runs_candidate` permission was allowed for. */
  candidate?: string | null;
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
  /** The agent each stage runs with, by role, fixed by the user. */
  stages?: Record<string, DeliveryStageAgent>;
}

/** Who runs a stage; the CLI's defaults where null. */
export interface DeliveryStageAgent {
  agent: DeliveryAgent;
  model: string | null;
  reasoning: string | null;
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
  /** The tasks' pull requests Tinto follows. */
  prs?: DeliveryPr[];
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

/** A task's pull request as Tinto follows it on GitHub. */
export interface DeliveryPr {
  task_id: string;
  url: string;
  repo: string;
  number: number;
  /** open, merged, closed, or unknown until the first read. */
  state: string;
  draft: boolean;
  conflicts: boolean;
  /** approved, changes_requested, review_required or none. */
  review_decision: string | null;
  checks: { passing: number; failing: number; pending: number };
  /** Reviews and comments by others since the user last saw the PR. */
  new_reviews: number;
  new_comments: number;
  new_authors: string[];
  latest_activity: string | null;
  seen_until: string | null;
  checked_at_ms: number | null;
  error: string | null;
}
