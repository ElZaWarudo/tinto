// Delivery mode's contract with the backend (src-tauri/src/delivery/model.rs).
// Kept apart from the frozen bus contract: Delivery evolves on its own.

export type DeliveryJobStatus =
  | "queued"
  | "pending"
  | "started"
  | "finished"
  | "failed"
  | "cancelled"
  | "interrupted"
  /** Stopped by the user to continue later in the same session. */
  | "paused";

/** One item of an agent's own task list. */
export interface DeliveryPlanItem {
  text: string;
  status: "pending" | "in_progress" | "completed" | string;
}

/** Tokens a job used and, when the agent says it, their cost. */
export interface DeliveryUsage {
  input_tokens: number;
  cached_tokens: number;
  output_tokens: number;
  /** Millionths of a dollar; Claude Code reports it, Codex does not. */
  cost_micro_usd: number | null;
}

/** Where a job's time went, from its agent's output. */
export interface DeliveryTiming {
  /** Time in model calls, when the agent says it (Claude Code). */
  model_ms: number | null;
  /** Time with at least one command or tool call running. */
  commands_ms: number;
  /** The agent's turns, when it says them (Claude Code). */
  turns: number | null;
  /** The commands that took longest, all their runs added up. */
  slowest: DeliverySlowCommand[];
}

export interface DeliverySlowCommand {
  command: string;
  total_ms: number;
  runs: number;
  /** Runs Claude Code's Bash left at its 10-minute ceiling. */
  at_ceiling: number;
}

export type DeliveryAgent = "codex" | "claude" | "muse" | "shell";
export type DeliveryAccess = "workspace" | "full";
export type DeliveryResultState = "accepted" | "stale" | "invalid";
export type DeliveryLeaseState = "free" | "active" | "quarantined";
export type DeliveryRung = "commit" | "push" | "pr" | "merge" | "jira";
export type DeliveryApprovalStatus =
  /** `undone`: a commit step whose commits were taken back; it is pending again.
   *  `drifted`: a done step git or GitHub no longer bear out; `notice` says why. */
  "pending" | "approved" | "rejected" | "executed" | "failed" | "undone" | "drifted";

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
  /** One of the ordered pull requests its issue is delivered in. */
  package?: DeliveryPackage | null;
  worktree: string;
  branch: string;
  base_ref: string;
  base_commit: string;
  state: string;
  contract_version: number;
  created_at_ms: number;
  updated_at_ms: number;
  removed_at_ms?: number | null;
  /** A delivered task put away; it can be brought back. */
  archived_at_ms?: number | null;
  /** What the task waits for; cleared ones stay, with `cleared_at_ms`. */
  blockers?: DeliveryBlocker[];
  /** Why it is blocked, in the coordinator's words. */
  blocked_reason?: string;
  /** The repo checks Tinto runs on what each stage changes (the tests stage aside). */
  checks?: string[];
}

/** A task's place among its issue's pull requests: "2 of 3". */
export interface DeliveryPackage {
  index: number;
  of: number;
  /** What it delivers ("Backend", "UI"). */
  title: string;
  /** The package (task id) whose branch it starts from. */
  builds_on?: string | null;
}

/** An issue split into packages, as far as its packages got. */
export interface DeliverySplitIssue {
  repo: string;
  key: string;
  of: number;
  packages: {
    task_id: string;
    index: number;
    title: string;
    merged: boolean;
    delivered: boolean;
    /** Put away once delivered: no longer among the tasks. */
    archived: boolean;
  }[];
  delivered: boolean;
}

export type DeliveryTaskMilestone = "pr_opened" | "merged" | "delivered";
export type DeliveryPrMilestone = "merged" | "closed";

/** Something a blocked task waits for (src-tauri/src/delivery/model.rs). */
export type DeliveryBlocker = {
  id: string;
  /** Where it stands now, in a few words ("PR abierto", "fusionado"). */
  status: string;
  cleared_at_ms: number | null;
} & (
  | { type: "task"; task_id: string; key: string; until: DeliveryTaskMilestone }
  | { type: "pr"; url: string; until: DeliveryPrMilestone; state?: string | null }
  | { type: "decision"; decision_id: string; key: string }
  | { type: "note"; text: string }
);

/** A blocker to add: a task by id or key, a PR by URL, or a note. */
export type NewDeliveryBlocker =
  | { type: "task"; task: string; until?: DeliveryTaskMilestone }
  | { type: "pr"; url: string; until?: DeliveryPrMilestone }
  | { type: "note"; text: string };

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
  /** The model asked for; null leaves the CLI's default. */
  model: string | null;
  /** The model the session says it runs (Claude Code reports it at start). */
  session_model?: string | null;
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
  /** What the job was told about the task's checks when it started. */
  task_checks?: string[];
  /** Where a Claude job on a Windows repo was asked to run; Tinto chose when null. */
  platform?: DeliveryPlatform | null;
  /** Where the job ran ("Windows", "WSL · Ubuntu"…), once started. */
  ran_on?: string | null;
  /** Why it runs with another agent than the one asked for (not installed here). */
  substituted?: string | null;
  /** The interrupted job this one repeats, queued by Tinto when it restarted. */
  retry_of?: string | null;
  /** QA jobs: folders outside the worktree the permissions let it read. */
  read_dirs?: string[];
  /** QA jobs: folders outside the worktree the permissions let it write in. */
  write_dirs?: string[];
  /** QA jobs: the approved scripts and the frozen copies it ran instead. */
  frozen_scripts?: { original: string; copy: string }[];
  /** The paused job this one continues, in the same session. */
  resumes?: string | null;
  /** What the agent did last, and when. */
  activity?: string | null;
  activity_at_ms?: number | null;
  /** The command or tool call running now, and since when. */
  current_command?: string | null;
  current_command_at_ms?: number | null;
  /** Where the job's time went, once it ended. */
  timing?: DeliveryTiming | null;
  /** The agent's own task list, as it last wrote it. */
  plan?: DeliveryPlanItem[];
  /** What the job used, once the agent reports it. */
  usage?: DeliveryUsage | null;
  /** QA: the page rendered from its visual changes record, or why it could not be. */
  visual_record?: string | null;
  visual_record_error?: string | null;
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
  /** `jira` only: after_pr (the PR is open) or after_merge (it is merged). */
  stage?: "after_pr" | "after_merge" | null;
  /** `pr` only: who the user chose to review it; Tinto asks them on GitHub. */
  reviewers?: string[];
  /** `jira` only: statuses Tinto walks the issue through; empty when the coordinator does it. */
  jira_statuses?: string[];
  /** `jira` only: links the PR to an issue another package already moved. */
  jira_link_only?: boolean;
  reviewers_requested?: boolean;
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
  /** `commit`: why a single commit holds a change across areas or a large one. */
  single_commit_reason?: string | null;
  /** `commit`: changed files the plan leaves uncommitted on purpose. */
  left_out?: string[];
  /** `commit`: the commits Tinto created. */
  created?: string[];
  /** What Tinto noticed checking the step against git and GitHub. */
  notice?: string | null;
  /** The user's corrections of `outcome`, oldest first; `outcome` is the latest. */
  corrections?: DeliveryCorrection[];
}

/** A result the user corrected by hand, keeping the one it replaced. */
export interface DeliveryCorrection {
  previous: string | null;
  outcome: string;
  reason: string;
  at_ms: number;
}

export type DeliveryDecisionKind = "choice" | "text" | "permission";

export interface DeliveryDecisionOption {
  label: string;
  consequence: string;
  recommended: boolean;
}

/** Something the user settles before a task's stages run; answers are final. */
export type DeliveryQaProblemStatus =
  "open" | "sent" | "fixing" | "verified" | "still_failing" | "dismissed" | "superseded";

/** A problem QA found (or the user reported) and what the user decided. */
export interface DeliveryQaProblem {
  id: string;
  task_id: string;
  /** The QA job whose record lists it; null when the user reported it. */
  job_id: string | null;
  attempt?: number | null;
  /** #n in the task. */
  number: number;
  /** P3 as the record names it. */
  label: string;
  change: string;
  place: string;
  text: string;
  /** QA's proposals, the recommended first. */
  suggestions: string[];
  status: DeliveryQaProblemStatus;
  solution?: string | null;
  reason?: string | null;
  notes: string[];
  created_at_ms: number;
  updated_at_ms: number;
}

/** Code a decision is about: a PR review comment and its diff, or worktree lines. */
export interface DeliveryCodeThread {
  kind: "pr_comment" | "code" | string;
  path: string;
  line?: number | null;
  url?: string | null;
  outdated?: boolean;
  lines: DeliveryCodeLine[];
  comment?: DeliveryThreadComment | null;
  note?: string;
  /** Why Tinto could not show it. */
  error?: string | null;
}

export interface DeliveryCodeLine {
  kind: "add" | "del" | "ctx" | string;
  old: number | null;
  new: number | null;
  text: string;
  target?: boolean;
}

export interface DeliveryThreadComment {
  author: string;
  body: string;
  created_at: string;
  replies: number;
}

export interface DeliveryDecision {
  id: string;
  task_id: string;
  kind: DeliveryDecisionKind;
  /** Plain language, for the user. */
  question: string;
  /** Technical context, folded. */
  detail: string;
  /** The code the question is about, as Tinto read it. */
  references?: DeliveryCodeThread[];
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
  /** False: the coordinator's own call, answered with its recommendation; the user may change it. */
  blocking?: boolean;
  status: "pending" | "answered";
  /** The chosen option, the approved text, or "allowed"/"denied". */
  answer: string | null;
  requested_by: string;
  requested_at_ms: number;
  decided_by: string | null;
  decided_at_ms: number | null;
}

export interface DeliveryRun {
  /** The Agents conversation coordinating it, and when Tinto last woke it. */
  coordinator_session?: string | null;
  notified_at_ms?: number | null;
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
  /** The coordinator ended the brainstorm: decisions, commits and pushes no longer wait for the user. */
  unattended?: boolean;
  /** The user paused the batch: its queued jobs wait. */
  paused?: boolean;
  /** The user asked the coordinator to create no more tasks in the batch. */
  no_new_tasks?: boolean;
}

export interface DeliverySettings {
  /** Agents running at once; 0 is automatic: one per active task, up to 8. */
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
  /** Environment variables jobs run the checks and QA commands with, by name. */
  check_env?: Record<string, string>;
  /** Whether QA jobs get a headless browser. */
  qa_browser?: boolean;
  /** Where QA runs on this machine, in the user's words. */
  qa_environment?: string;
  /** How changes are verified beyond the stages, in the user's words. */
  verification?: string;
  /** Whether QA may make paid model calls; null until set. */
  paid_model_calls?: boolean | null;
  /** A coordinator's single commit with more files or lines must say why (default 5 and 300). */
  single_commit_max_files?: number | null;
  single_commit_max_lines?: number | null;
  /** The agent each stage runs with, by role, fixed by the user. */
  stages?: Record<string, DeliveryStageAgent>;
  /** The repo's Jira, for Tinto to run the Jira steps; the token is kept apart. */
  jira?: DeliveryJiraSettings | null;
  /** "Reintentar al reiniciar": jobs a restart interrupted run again once, when safe. */
  retry_on_restart?: boolean;
  /** The checks the user marked as environment probes, run by the "Entorno" check. */
  environment_probes?: string[];
}

/** One line of a repo's "Entorno" check: Docker or a probe. */
export interface DeliveryEnvironmentCheck {
  name: string;
  ok: boolean;
  /** What it printed (its end), or why it failed. */
  output: string;
}

export interface DeliveryJiraSettings {
  host: string;
  project: string;
  /** Statuses the issue goes through once the PR is open, in order. */
  after_pr: string[];
  /** Statuses once the PR is merged. */
  after_merge: string[];
  /** Whether a token is saved (read only). */
  has_token?: boolean;
}

/** Who runs a stage; the CLI's defaults where null. */
export interface DeliveryStageAgent {
  agent: DeliveryAgent;
  model: string | null;
  reasoning: string | null;
  /** The stage's time limit in minutes; null keeps the role's default. */
  timeout_minutes?: number | null;
}

export interface DeliveryCoordinatorEndpoint {
  url: string;
  token: string;
  /** Tinto's MCP proxy, a copy of tinto.exe that updates leave running, for agents
   *  outside Tinto to run as a stdio MCP server that opens Tinto. */
  command?: string | null;
}

/** Changes to a repo's settings: lists add or remove, values replace. */
export interface DeliverySettingsChanges {
  checks_add?: string[];
  checks_remove?: string[];
  qa_commands_add?: string[];
  bootstrap?: string | null;
  qa_browser?: boolean | null;
  verification?: string | null;
  /** Variables for checks, by name, each set to the value given. */
  check_env?: Record<string, string>;
}

/** Settings changes a coordinator asks for; nothing changes until the user approves. */
export interface DeliverySettingsProposal {
  id: string;
  repo: string;
  task_id: string;
  run_id: string;
  /** One plain line, for the user. */
  reason: string;
  changes: DeliverySettingsChanges;
  status: "pending" | "approved" | "rejected";
  /** What the user approved: all of `changes` or a part. */
  approved: DeliverySettingsChanges | null;
  requested_by: string;
  requested_at_ms: number;
  decided_by: string | null;
  decided_at_ms: number | null;
}

/** Who has to act for a task to move. */
export type DeliveryWaiter =
  "user" | "coordinator" | "agent" | "queue" | "reviewers" | "dependency" | "external" | "nobody";

/** Where a task is, as Tinto derives it (src-tauri/src/delivery/phase.rs). */
export type DeliveryPhase =
  | { kind: "decisions"; count: number }
  | { kind: "approval" | "record" | "drifted"; rung: DeliveryRung; stage: string | null }
  | { kind: "running" | "paused" | "batch_paused"; job_id: string; role: string }
  | { kind: "waits_quarantine"; job_id: string; lease: string }
  | { kind: "queued"; job_id: string; role: string; lease: string | null; position: number }
  | { kind: "quarantined"; lease: string }
  | { kind: "settings_proposal" }
  | { kind: "blocked"; open: number }
  | { kind: "broken"; job_id: string; role: string; attempt: number; blocked: boolean }
  | { kind: "delivered" }
  | { kind: "pr"; number: number }
  | { kind: "next_stage"; role: string }
  | { kind: "next_step"; rung: DeliveryRung; stage: string | null };

export type DeliveryTaskPhase = DeliveryPhase & {
  waits_on: DeliveryWaiter;
  /** The stage to run next, when one is due. */
  next_stage: string | null;
  /** The delivery steps this task climbs, when not the whole ladder (a middle package ends at merge). */
  steps?: { rung: DeliveryRung; stage: string | null }[];
};

export interface DeliveryOverview {
  /** Where each task is, by task id: every surface reads this one answer. */
  phases?: Record<string, DeliveryTaskPhase>;
  runs: DeliveryRun[];
  tasks: DeliveryTask[];
  jobs: DeliveryJob[];
  leases: DeliveryLease[];
  approvals: DeliveryApproval[];
  decisions: DeliveryDecision[];
  /** Problems of the QA records, and the ones the user reported. */
  qa_problems?: DeliveryQaProblem[];
  /** Settings changes coordinators asked for, waiting for the user. */
  settings_proposals?: DeliverySettingsProposal[];
  /** The tasks' pull requests Tinto follows. */
  prs?: DeliveryPr[];
  settings: DeliverySettings;
  /** Agents that may run now: the setting, or one per active task. */
  capacity?: number;
  coordinator: DeliveryCoordinatorEndpoint | null;
  /** What each active batch's coordinator has not read yet. */
  coordinators?: DeliveryCoordinatorState[];
  /** Issues delivered in several packages, each across its packages. */
  issues?: DeliverySplitIssue[];
}

/** Whether a batch's coordinator keeps up with its changes. */
export interface DeliveryCoordinatorState {
  run_id: string;
  /** It is inside wait_run now, reading each change as it comes. */
  waiting: boolean;
  /** Changes it has not read and Tinto has not woken it for. */
  unread: number;
  unread_since_ms: number | null;
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

/** What a task changed since its worktree was created, each file against that commit. */
export interface DeliveryTaskChanges {
  /** The worktree as a repo of the active workbench. */
  repo: string;
  base_ref: string;
  base_commit: string;
  files: import("../bus/contract").FileDiff[];
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
  /** Once merged, when GitHub gives them. */
  merged_by?: string | null;
  merged_at?: string | null;
  merge_commit?: string | null;
  /** Reviewers whose latest review approves it. */
  approved_by?: string[];
  /** The branch it merges into, as GitHub last said. */
  base?: string | null;
}

/** An archived task as a search finds it. */
export interface DeliveryArchivedTask {
  task: DeliveryTask;
  pr_number: number | null;
  pr_url: string | null;
}

/** A task's Jira issue as Tinto reads it; the HTML is Jira's, not yet sanitized. */
export interface DeliveryIssue {
  key: string;
  url: string;
  summary: string;
  status: string;
  type: string;
  priority: string | null;
  assignee: string | null;
  reporter: string | null;
  labels: string[];
  parent: { key: string; summary: string } | null;
  links: { type: string; key: string; summary: string; status: string }[];
  description_html: string;
  comments: { author: string; created: string; body_html: string }[];
}
