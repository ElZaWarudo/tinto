//! Delivery mode's own contract with the frontend. It is deliberately kept
//! out of the frozen bus contract: Delivery is a separate way of working and
//! evolves on its own (`src/delivery/types.ts` mirrors these types).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryTask {
    pub id: String,
    pub run_id: Option<String>,
    /// Source repository the worktree was created from.
    pub repo: PathBuf,
    /// WSL distro when `repo` is a Linux path.
    pub distro: Option<String>,
    pub key: String,
    pub title: String,
    /// One of the ordered pull requests its issue is delivered in, when the
    /// issue is split; other tasks of the same key are the other packages.
    #[serde(default)]
    pub package: Option<DeliveryPackage>,
    pub worktree: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub base_commit: String,
    /// Free-form stage label (`intake`, `tests`, `review`, `qa_ready`…).
    pub state: String,
    pub contract_version: u32,
    /// Folders outside the worktree every job of the task may read and list,
    /// such as the issue's planning folder in the main checkout.
    #[serde(default)]
    pub read_dirs: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub removed_at_ms: Option<u64>,
    /// A delivered task put away: its worktree and local branch are gone
    /// (all of it is on GitHub) and it can be brought back.
    #[serde(default)]
    pub archived_at_ms: Option<u64>,
    /// What the task waits for before it can go on. While one is open the
    /// task is blocked: no stage starts on it.
    #[serde(default)]
    pub blockers: Vec<DeliveryBlocker>,
    /// Why it is blocked, in the coordinator's words; empty once no
    /// blocker is open (the cleared reason moves to `unblocked`).
    #[serde(default)]
    pub blocked_reason: String,
    /// Its last open blocker cleared without the coordinator acting on it
    /// (the user cleared it, or it resolved): the task waits on the
    /// coordinator until it dispatches, asks for a step, sets its state or
    /// blocks it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unblocked: Option<DeliveryUnblock>,
    /// The repo's checks Tinto runs on what each stage changes (the tests
    /// stage aside), as the coordinator chose them for this issue.
    #[serde(default)]
    pub checks: Vec<String>,
    /// The repo's setup commands (installs, generators) Tinto runs once in
    /// the task's worktree before its stages, as the coordinator chose them.
    #[serde(default)]
    pub setup: Vec<String>,
    /// The coordinator judged its change one CI has nothing to verify (only
    /// docs): its PR gets no CI label and merges without CI having run.
    #[serde(default)]
    pub skip_ci: bool,
    /// Its issue's parent in Jira (an epic, a story), as last read.
    #[serde(default)]
    pub parent: Option<super::jira::DeliveryIssueParent>,
    /// When `parent` was last read from Jira; never, while `None`.
    #[serde(default)]
    pub parent_read_at_ms: Option<u64>,
    /// The tip of the branch it was based on, which merged with its commits
    /// rewritten (squash or rebase): the branch still carries them, so it is
    /// rebased onto `base_ref` from here (its sync step restacks it) before
    /// it is published again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restack_from: Option<String>,
}

/// A task's place among the pull requests of its issue: "2 of 3".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryPackage {
    /// From 1, in delivery order.
    pub index: u32,
    /// How many packages the issue has.
    pub of: u32,
    /// What this package delivers ("Backend", "UI").
    #[serde(default)]
    pub title: String,
    /// The package (task id) whose branch this one starts from; its PR
    /// targets that branch until that package merges.
    #[serde(default)]
    pub builds_on: Option<String>,
}

impl DeliveryPackage {
    pub fn is_last(&self) -> bool {
        self.index >= self.of
    }
}

impl DeliveryTask {
    /// "AGOS-432 · 2/3" for a package, the key otherwise.
    pub fn label(&self) -> String {
        match &self.package {
            Some(package) => format!("{} · {}/{}", self.key, package.index, package.of),
            None => self.key.clone(),
        }
    }

    /// Another package of the same issue.
    pub fn is_sibling(&self, other: &DeliveryTask) -> bool {
        match (&self.package, &other.package) {
            (Some(mine), Some(theirs)) => {
                mine.index != theirs.index
                    && self.repo == other.repo
                    && self.key.eq_ignore_ascii_case(&other.key)
            }
            _ => false,
        }
    }
}

/// How a task's last open blocker cleared.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryUnblock {
    pub at_ms: u64,
    /// The blockers that cleared last, in a few words.
    pub cleared: String,
    /// `user` (cleared by hand in Tinto) or `tinto` (it resolved).
    pub by: String,
    /// Why it was blocked, as the coordinator said.
    #[serde(default)]
    pub reason: String,
}

/// Something a blocked task waits for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryBlocker {
    pub id: String,
    #[serde(flatten)]
    pub kind: DeliveryBlockerKind,
    /// Where it stands now, in a few words ("PR abierto", "fusionado").
    #[serde(default)]
    pub status: String,
    /// When it cleared; until then it blocks the task.
    #[serde(default)]
    pub cleared_at_ms: Option<u64>,
}

impl DeliveryBlocker {
    pub fn is_open(&self) -> bool {
        self.cleared_at_ms.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeliveryBlockerKind {
    /// Another task, in this batch or another, reaching `until`.
    Task {
        task_id: String,
        key: String,
        until: DeliveryTaskMilestone,
    },
    /// A GitHub PR, Tinto's or someone else's, merged (or just closed).
    Pr {
        url: String,
        until: DeliveryPrMilestone,
        /// Its state as GitHub last reported it: `open`, `merged`, `closed`.
        #[serde(default)]
        state: Option<String>,
    },
    /// A decision of another task, waiting on the user.
    Decision { decision_id: String, key: String },
    /// Anything else: the user clears it by hand.
    Note { text: String },
}

/// How far another task got, in delivery order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryTaskMilestone {
    PrOpened,
    Merged,
    Delivered,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPrMilestone {
    Merged,
    Closed,
}

/// A blocker as a coordinator or the user asks for it: a task by id or
/// issue key, a PR by URL, a decision by id, or a note.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NewBlocker {
    Task {
        task: String,
        #[serde(default)]
        until: Option<DeliveryTaskMilestone>,
    },
    Pr {
        url: String,
        #[serde(default)]
        until: Option<DeliveryPrMilestone>,
    },
    Decision {
        decision_id: String,
    },
    Note {
        text: String,
    },
}

/// An archived task as a search finds it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryArchivedTask {
    pub task: DeliveryTask,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryJobStatus {
    /// Waiting for capacity, its task or an exclusive resource.
    Queued,
    /// Written ahead: about to start.
    Pending,
    Started,
    Finished,
    Failed,
    Cancelled,
    /// Tinto stopped while the job was running.
    Interrupted,
    /// The user stopped it to continue later in the same agent session; it
    /// holds its task (and its exclusive resource) until resumed or cancelled.
    Paused,
}

impl DeliveryJobStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Started)
    }

    pub fn is_open(self) -> bool {
        matches!(self, Self::Queued | Self::Pending | Self::Started)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryAgent {
    Codex,
    Claude,
    /// Muse Code, headless through `muse exec --json`.
    Muse,
    /// OpenCode, headless through `opencode run --format json`.
    Opencode,
    /// A plain shell command (bootstrap, checks); no snapshot, no result.
    Shell,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryAccess {
    /// Codex `workspace-write`; Claude any command but Tinto's own steps
    /// (`acceptEdits` for jobs that write).
    #[default]
    Workspace,
    /// Codex `danger-full-access`; Claude `bypassPermissions`.
    Full,
}

/// Where a Claude Code job on a Windows repository runs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPlatform {
    Windows,
    Wsl,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryResultState {
    Accepted,
    /// From a superseded attempt or an older contract version.
    Stale,
    /// Breaks the job's own rules (e.g. a read-only job changed files).
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCheck {
    pub command: String,
    pub result: String,
}

/// The structured result every agent job must end with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryJobResult {
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub checks: Vec<DeliveryCheck>,
    #[serde(default)]
    pub findings: Vec<String>,
    #[serde(default)]
    pub handoff: String,
    /// What stopped the job, if anything: `sandbox` (its sandbox or
    /// permission rules refused something a decision allows),
    /// `environment` (something outside them failed) or `none`.
    #[serde(default)]
    pub blocked_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryChange {
    pub path: String,
    /// `created`, `modified` or `removed`.
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryJob {
    pub id: String,
    pub task_id: String,
    pub role: String,
    pub attempt: u32,
    pub agent: DeliveryAgent,
    /// The model asked for; `None` leaves the CLI's default.
    pub model: Option<String>,
    /// The model the agent's session says it runs (Claude Code reports it
    /// when it starts), so a default is shown by its name.
    #[serde(default)]
    pub session_model: Option<String>,
    pub access: DeliveryAccess,
    pub prompt: String,
    pub contract_version: u32,
    /// Read-only jobs (review, QA) must not change the candidate.
    pub writes: bool,
    /// Exclusive resource the job holds while it runs (e.g. `qa`).
    pub lease: Option<String>,
    pub timeout_minutes: u32,
    pub status: DeliveryJobStatus,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub pid: Option<u32>,
    /// When the job's process started, as the system tells it: a restarted
    /// Tinto adopts the process with `pid` only while this still matches.
    #[serde(default)]
    pub process_started: Option<String>,
    /// Codex thread id or Claude session id, for "Abrir en Agents".
    pub provider_session_id: Option<String>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub start_candidate: Option<String>,
    pub end_candidate: Option<String>,
    #[serde(default)]
    pub changes: Vec<DeliveryChange>,
    pub result: Option<DeliveryJobResult>,
    pub result_state: Option<DeliveryResultState>,
    pub result_note: Option<String>,
    pub undone_at_ms: Option<u64>,
    /// A stage that failed or was blocked, settled without running it again
    /// because its work was done another way: when, and the one-line note
    /// saying how. It then counts as passed.
    #[serde(default)]
    pub resolved_at_ms: Option<u64>,
    #[serde(default)]
    pub resolved_note: Option<String>,
    /// The checks a job without full access is given, taken from the
    /// repo's verification commands (and, for QA, its QA commands and the
    /// user's permissions) when the job was dispatched. A Codex job's
    /// sandbox must reach their programs.
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// QA jobs only: a browser through Playwright's MCP server.
    #[serde(default)]
    pub qa_browser: bool,
    /// The task's answered decisions when the job was dispatched, as the
    /// job's instructions state them.
    #[serde(default)]
    pub decisions: Vec<String>,
    /// QA jobs only: the problems to check again and the ones not to
    /// report, when the job was dispatched.
    #[serde(default)]
    pub qa_followup: Vec<String>,
    /// QA jobs only: the repo's QA environment when the job was dispatched.
    #[serde(default)]
    pub qa_environment: String,
    /// Browser QA jobs only: the repo's QA stack when the job was
    /// dispatched, which Tinto prepares before the agent starts.
    #[serde(default)]
    pub qa_stack: Option<DeliveryQaStack>,
    /// What Tinto's preparation of the QA stack did, once it ran.
    #[serde(default)]
    pub prepare: Option<DeliveryQaPrepare>,
    /// QA jobs only: files of the worktree the job changed, which Tinto put
    /// back as they were when it started.
    #[serde(default)]
    pub reverted: Vec<String>,
    /// The task's checks when the job started and how they did on the code
    /// it started from, as the job's instructions state them.
    #[serde(default)]
    pub task_checks: Vec<String>,
    /// The task's round when the job was dispatched in one: why its work
    /// came back and what to address, as the job's instructions state them.
    #[serde(default)]
    pub round: Vec<String>,
    /// The repo's approved guide rules for the job's stage when it was
    /// dispatched, as its instructions state them.
    #[serde(default)]
    pub guide_rules: Vec<String>,
    /// It needs the repo's environment (Docker, the probes' services): it
    /// waits queued while the "Entorno" check fails instead of failing.
    #[serde(default)]
    pub needs_environment: bool,
    /// Why it waits on the environment, while it does.
    #[serde(default)]
    pub waits_environment: Option<String>,
    /// Reasoning effort passed to the CLI; its default when `None`.
    #[serde(default)]
    pub reasoning: Option<String>,
    /// Where a Claude job on a Windows repo was asked to run. When `None`,
    /// Windows if Claude Code is installed there, otherwise WSL.
    #[serde(default)]
    pub platform: Option<DeliveryPlatform>,
    /// Where the job ran, as the stage shows it (`Windows`, `WSL · Ubuntu`).
    #[serde(default)]
    pub ran_on: Option<String>,
    /// Folders outside the worktree the job may read and list: the ones its
    /// coordinator named and, for QA, the ones the user's permissions name.
    #[serde(default)]
    pub read_dirs: Vec<String>,
    /// Whether the model and reasoning came from the stage's settings (the
    /// coordinator named no model): a retry takes the stage's current ones.
    #[serde(default)]
    pub stage_default: bool,
    /// The repo's environment variables for checks when the job was
    /// dispatched; the agent's process runs with them.
    #[serde(default)]
    pub check_env: BTreeMap<String, String>,
    /// QA jobs only: folders outside the worktree the user's permissions
    /// let the job write in.
    #[serde(default)]
    pub write_dirs: Vec<String>,
    /// QA jobs only: the frozen copies of the scripts the user approved,
    /// written to the job's folder when it started.
    #[serde(default)]
    pub frozen_scripts: Vec<DeliveryFrozenCopy>,
    /// The paused job this one continues, in the same agent session.
    #[serde(default)]
    pub resumes: Option<String>,
    /// What the agent is told when it continues a job that stopped (its time
    /// ran out, Tinto restarted); `None` when the user resumed a pause.
    #[serde(default)]
    pub resume_note: Option<String>,
    /// A writing job that starts while a sync step's merge is stopped on
    /// conflicts: the base being merged and the files to resolve, which
    /// its instructions name.
    #[serde(default)]
    pub merge_conflicts: Option<DeliveryMergeConflicts>,
    /// What the agent did last, as its log says it, and when: the view
    /// shows it while the job runs and tells when it has gone quiet.
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub activity_at_ms: Option<u64>,
    /// The command or tool call running now, and since when: a long command
    /// is not a quiet agent.
    #[serde(default)]
    pub current_command: Option<String>,
    #[serde(default)]
    pub current_command_at_ms: Option<u64>,
    /// Where the job's time went, once it ended.
    #[serde(default)]
    pub timing: Option<DeliveryTiming>,
    /// The agent's own task list, as it last wrote it.
    #[serde(default)]
    pub plan: Vec<DeliveryPlanItem>,
    /// What the job used, once the agent reports it.
    #[serde(default)]
    pub usage: Option<DeliveryUsage>,
    /// QA jobs: the page Tinto rendered from the visual changes record the
    /// job wrote, for the user to open, or why it could not.
    #[serde(default)]
    pub visual_record: Option<String>,
    #[serde(default)]
    pub visual_record_error: Option<String>,
    /// The file the record came from (`<KEY>.md`, a `<KEY>.html` the agent
    /// wrote, or a `<KEY>-*.md`), the other candidates beside it, and what
    /// the record lacks of the QA record guide.
    #[serde(default)]
    pub visual_record_source: Option<String>,
    #[serde(default)]
    pub visual_record_extras: Vec<String>,
    #[serde(default)]
    pub visual_record_warnings: Vec<String>,
    /// Why the job runs with another agent than the one asked for (it was
    /// not installed on this machine).
    #[serde(default)]
    pub substituted: Option<String>,
    /// The interrupted job this one repeats, queued by Tinto when it
    /// restarted ("Reintentar al reiniciar"). Such a retry is never repeated
    /// again.
    #[serde(default)]
    pub retry_of: Option<String>,
}

/// A pass of work sent back on a task: review or QA findings, QA problems
/// the user sent to fix, a failing check of Tinto, changes asked on the PR,
/// conflicts of a sync. Round 1, the initial delivery, is never stored.
/// Recorded when it happens, so it keeps what the cause was then.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryRound {
    pub id: String,
    pub task_id: String,
    /// From 2.
    pub number: u32,
    /// What sent the work back: `review`, `qa`, `qa_problems`, `checks`,
    /// `pr` or `sync`.
    pub kind: String,
    /// In a few words, for the task list: "5 hallazgos de QA", "PR".
    pub short: String,
    /// One line: "Cambios pedidos en el PR #308 por ana (3 comentarios)".
    pub cause: String,
    /// What the round has to address, one per line.
    #[serde(default)]
    pub items: Vec<String>,
    /// The job or step whose outcome opened it.
    #[serde(default)]
    pub source: Option<String>,
    pub opened_at_ms: u64,
    /// The branch was already published when it opened: its commit and
    /// push are done again in it.
    #[serde(default)]
    pub after_push: bool,
    #[serde(default)]
    pub closed_at_ms: Option<u64>,
    /// How it ended: "corregidos", "aceptados", "siguió en la ronda 4"…
    #[serde(default)]
    pub outcome: Option<String>,
    /// Opened while a sync's merge was in progress: it waits for the
    /// commit that concludes the merge, so its changes are not mixed in.
    #[serde(default)]
    pub waits_merge_commit: bool,
}

impl DeliveryRound {
    pub fn is_open(&self) -> bool {
        self.closed_at_ms.is_none()
    }

    /// Open and not waiting for a merge commit: the round stages work in.
    pub fn is_current(&self) -> bool {
        self.is_open() && !self.waits_merge_commit
    }
}

/// One item of an agent's task list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryPlanItem {
    pub text: String,
    /// `pending`, `in_progress` or `completed`.
    pub status: String,
}

/// Tokens a job used and, when the agent says it, what they cost.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryUsage {
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    /// Millionths of a dollar (Claude Code reports the cost; Codex does not).
    pub cost_micro_usd: Option<u64>,
}

impl DeliveryUsage {
    /// Both together: an agent that reports each step's use.
    pub fn plus(&self, other: &DeliveryUsage) -> DeliveryUsage {
        DeliveryUsage {
            input_tokens: self.input_tokens + other.input_tokens,
            cached_tokens: self.cached_tokens + other.cached_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cost_micro_usd: match (self.cost_micro_usd, other.cost_micro_usd) {
                (None, None) => None,
                (left, right) => Some(left.unwrap_or(0) + right.unwrap_or(0)),
            },
        }
    }
}

/// Where a job's time went, from its agent's output.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryTiming {
    /// Time in model calls, when the agent says it (Claude Code).
    pub model_ms: Option<u64>,
    /// Time with at least one command or tool call running.
    pub commands_ms: u64,
    /// The agent's turns, when it says them (Claude Code).
    pub turns: Option<u32>,
    /// The commands that took longest, all their runs added up.
    pub slowest: Vec<DeliverySlowCommand>,
    /// QA: the time Tinto spent preparing the QA stack before the agent
    /// started, outside the job's time limit.
    #[serde(default)]
    pub prepare_ms: Option<u64>,
}

/// How Tinto prepares a repo's QA stack before a browser QA job, so the
/// agent starts on a stack that serves the task's worktree.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryQaStack {
    /// Its name, such as the compose project (`agentos-dev`). Browser QA on
    /// it holds the resource `qa:browser:<name>`.
    pub name: String,
    /// Run in the task's worktree to rebuild and restart services;
    /// `{services}` becomes the ones to rebuild, space-separated.
    pub up: String,
    /// The repo's folders and the services built from each.
    #[serde(default)]
    pub services: BTreeMap<String, Vec<String>>,
    /// Answers once the stack is up.
    pub ready_url: String,
    /// The app's paths QA visits, requested before it starts so the dev
    /// server has compiled them.
    #[serde(default)]
    pub warm_paths: Vec<String>,
    /// Run once the stack answers: test data, test users.
    #[serde(default)]
    pub seed: Vec<String>,
    /// Run after `up` to serve a production build of the frontend instead
    /// of its dev server; nothing when empty.
    #[serde(default)]
    pub preview: String,
}

/// What preparing the QA stack did for a job.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryQaPrepare {
    pub ms: u64,
    /// The services it rebuilt.
    pub services: Vec<String>,
    /// The stack already served this task's worktree as it is: nothing was
    /// rebuilt.
    pub reused: bool,
    /// What it did, in a few lines, or why it failed.
    pub note: String,
}

/// What the QA stack serves since Tinto last prepared it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryPreparedStack {
    pub task_id: String,
    /// The worktree's candidate (commit and tree) it was built from.
    pub candidate: String,
    /// The services built from that task's worktree.
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySlowCommand {
    pub command: String,
    pub total_ms: u64,
    pub runs: u32,
    /// Runs Claude Code's Bash left at its 10-minute ceiling.
    pub at_ceiling: u32,
}

/// A script as named in a permission and the frozen copy a job runs instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryFrozenCopy {
    pub original: String,
    pub copy: String,
}

/// A script a permission's command runs, as it was when the coordinator
/// asked. Jobs run this text, whatever happens to the file afterwards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryFrozenScript {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryLeaseState {
    Free,
    Active,
    /// Its holder ended uncleanly; only the user can release it.
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryLeaseWaiter {
    pub job_id: String,
    pub task_id: String,
    pub requested_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryLease {
    pub name: String,
    pub state: DeliveryLeaseState,
    pub generation: u64,
    pub holder_job_id: Option<String>,
    pub holder_task_id: Option<String>,
    pub acquired_at_ms: Option<u64>,
    pub note: Option<String>,
    #[serde(default)]
    pub queue: Vec<DeliveryLeaseWaiter>,
    /// The task that held it last: its environment (a QA stack built from
    /// that task's branch) may still be up, so its next job goes first.
    #[serde(default)]
    pub last_task_id: Option<String>,
    /// A QA stack's resource: what Tinto last prepared on it.
    #[serde(default)]
    pub prepared: Option<DeliveryPreparedStack>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRung {
    Commit,
    /// Merge the task's base into its branch, in its worktree.
    Sync,
    Push,
    Pr,
    /// Merge the PR on GitHub and delete its branch there.
    Merge,
    Jira,
    /// Reply to a reviewer's comment on the PR. Asked once per comment,
    /// outside the ladder: it neither counts toward delivery nor holds the
    /// stages.
    PrReply,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Executed,
    Failed,
    /// A commit step whose commits were taken back, by the user in Tinto or
    /// outside it (reset, amend, rebase). The step is pending again.
    Undone,
    /// A done step that git or GitHub no longer bear out: the remote branch
    /// is gone or moved, or the PR was closed. `notice` says what changed;
    /// it counts as done again if what it recorded holds again.
    Drifted,
}

impl DeliveryApprovalStatus {
    /// Done, whether or not it still matches what git and GitHub show.
    pub fn was_done(self) -> bool {
        matches!(self, Self::Executed | Self::Drifted)
    }
}

/// A merge of the task's base stopped on conflicts, as a job is told.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryMergeConflicts {
    /// The base branch being merged, such as `origin/develop`.
    pub base: String,
    pub files: Vec<String>,
}

/// One commit of a commit step's plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCommit {
    /// The exact message: subject, then an optional body after a blank line.
    pub message: String,
    /// The files it includes, relative to the worktree, with `/`. A commit
    /// that concludes a merge takes every change and needs none.
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryApproval {
    pub id: String,
    pub task_id: String,
    pub rung: DeliveryRung,
    /// Commit subject or PR title: the exact text being approved.
    pub title: String,
    pub body: String,
    pub status: DeliveryApprovalStatus,
    /// `user`, the coordinator's owner id, or `github` for a step done
    /// outside Tinto that Tinto found on GitHub.
    pub requested_by: String,
    pub requested_at_ms: u64,
    pub decided_at_ms: Option<u64>,
    pub executed_at_ms: Option<u64>,
    pub outcome: Option<String>,
    /// `merge` only: `squash`, `merge` or `rebase`.
    #[serde(default)]
    pub merge_method: Option<String>,
    /// `jira` only: when it happens, `after_pr` (the PR is open, the issue
    /// goes to review) or `after_merge` (the issue is closed).
    #[serde(default)]
    pub stage: Option<String>,
    /// `jira` only: the statuses Tinto walks the issue through, in order,
    /// when the repo's Jira is set in Tinto. Empty: the coordinator does the
    /// step and reports it.
    #[serde(default)]
    pub jira_statuses: Vec<String>,
    /// `jira` only: the step links the PR to an issue another package of it
    /// already moved, without changing the issue's status.
    #[serde(default)]
    pub jira_link_only: bool,
    /// `pr` only: the GitHub logins the user chose to review it. Tinto asks
    /// them on GitHub once the PR is linked.
    #[serde(default)]
    pub reviewers: Vec<String>,
    /// `pr` only: whether Tinto already asked `reviewers` on GitHub.
    #[serde(default)]
    pub reviewers_requested: bool,
    /// `commit` only: the commits to create, in order.
    #[serde(default)]
    pub commits: Vec<DeliveryCommit>,
    /// `commit` only: changed files the plan leaves uncommitted on purpose.
    #[serde(default)]
    pub left_out: Vec<String>,
    /// `commit` only: why one commit holds a change that spans several
    /// areas or is large, which the user reads with the plan.
    #[serde(default)]
    pub single_commit_reason: Option<String>,
    /// `commit`: the commit being merged when the step concludes a merge in
    /// progress; Tinto then makes one commit of the whole merge, with the
    /// branch and this commit as parents. `sync`: the base commit it merged.
    #[serde(default)]
    pub merge_head: Option<String>,
    /// `sync` only: the files left in conflict, when the merge stopped on
    /// them. The merge stays in progress until a commit step concludes it.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// `commit`: the commits Tinto created, in order. `sync`: its merge
    /// commit, when the merge had no conflicts.
    #[serde(default)]
    pub created: Vec<String>,
    /// `commit`: where the branch was before them, to undo the step.
    /// `sync` that restacked the branch: where it was before the rebase.
    #[serde(default)]
    pub before: Option<String>,
    /// `push` only: the commit the remote branch ended at once published.
    #[serde(default)]
    pub tip: Option<String>,
    /// `push` only: the branch's HEAD when the push was asked, the commit
    /// the user approves publishing.
    #[serde(default)]
    pub head: Option<String>,
    /// `commit` only: the author git records on the commits (`Name
    /// <email>`), from the worktree's git config, as the user approves it.
    #[serde(default)]
    pub author: Option<String>,
    /// What Tinto noticed when it checked the step against git and GitHub,
    /// such as a recorded PR URL that is not the branch's PR.
    #[serde(default)]
    pub notice: Option<String>,
    /// The user's corrections of `outcome`, oldest first. `outcome` is the
    /// latest; each correction keeps the text it replaced.
    #[serde(default)]
    pub corrections: Vec<DeliveryCorrection>,
    /// `pr_reply` only: the review comment answered
    /// (`…/pull/<n>#discussion_r<id>`), or the PR for a general comment.
    #[serde(default)]
    pub comment_url: Option<String>,
    /// `pr_reply` only: resolve the review thread once replied.
    #[serde(default)]
    pub resolve_thread: bool,
    /// `pr_reply` only: QA screenshots the user chose to show in the reply.
    #[serde(default)]
    pub images: Vec<DeliveryReplyImage>,
    /// `pr_reply` only: the review comment and its diff, as Tinto read them
    /// from GitHub when the reply was asked.
    #[serde(default)]
    pub thread: Option<DeliveryCodeThread>,
    /// `pr_reply` only: the user's latest replies to review comments in the
    /// same repository, newest first, for the reply to match their style.
    #[serde(default)]
    pub style_examples: Vec<String>,
    /// `pr_reply` only: the branch's commits that changed the commented file
    /// since the review comment, newest first: the fix the reply is about.
    #[serde(default)]
    pub fix_commits: Vec<DeliveryFixCommit>,
}

/// A commit of the task's branch, as a reply to a reviewer refers to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryFixCommit {
    pub sha: String,
    pub subject: String,
    /// The commit in the PR on GitHub.
    pub url: String,
}

/// A screenshot of one of the task's QA jobs, shown in a reply to a
/// reviewer: `file` is its name in the job's QA folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryReplyImage {
    pub job_id: String,
    pub file: String,
}

/// A result the user corrected by hand, for what Tinto cannot check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCorrection {
    /// The result before this correction.
    pub previous: Option<String>,
    pub outcome: String,
    pub reason: String,
    pub at_ms: u64,
}

/// What a decision asks of the user before work starts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryDecisionKind {
    /// Pick one of a few options.
    Choice,
    /// Approve or edit a text exactly as end users will see it.
    Text,
    /// Allow something outside the worktree, such as a QA command.
    Permission,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryDecisionStatus {
    Pending,
    Answered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryDecisionOption {
    pub label: String,
    /// One line on what choosing it means.
    #[serde(default)]
    pub consequence: String,
    #[serde(default)]
    pub recommended: bool,
}

/// A problem QA found (or the user reported) and what the user decided.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryQaProblem {
    pub id: String,
    pub task_id: String,
    /// The QA job whose record lists it; `None` when the user reported it.
    pub job_id: Option<String>,
    /// That QA job's attempt.
    #[serde(default)]
    pub attempt: Option<u32>,
    /// `#n` in the task.
    pub number: u32,
    /// `P3` as the record names it.
    #[serde(default)]
    pub label: String,
    /// The change of the record it is about.
    #[serde(default)]
    pub change: String,
    /// Where it happens.
    #[serde(default)]
    pub place: String,
    pub text: String,
    /// QA's proposals, the recommended first.
    #[serde(default)]
    pub suggestions: Vec<String>,
    pub status: DeliveryQaProblemStatus,
    /// The fix the user chose: a suggestion or their own words.
    #[serde(default)]
    pub solution: Option<String>,
    /// Why the user dismissed it.
    #[serde(default)]
    pub reason: Option<String>,
    /// What the coordinator said about it, oldest first.
    #[serde(default)]
    pub notes: Vec<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryQaProblemStatus {
    /// Waiting for the user.
    Open,
    /// The user sent it to fix.
    Sent,
    /// The coordinator is fixing it.
    Fixing,
    /// QA checked it is fixed.
    Verified,
    /// QA found it again after the fix.
    StillFailing,
    /// The user decided it is not a problem.
    Dismissed,
    /// A later QA replaced it before the user decided.
    Superseded,
}

/// Code a decision is about, as Tinto read it from GitHub or the task's
/// worktree: what the user reads before choosing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCodeThread {
    /// `pr_comment` (a review comment and the diff it is anchored to) or
    /// `code` (lines of the task's worktree).
    pub kind: String,
    /// The file, relative to the repository.
    pub path: String,
    /// The line it is about, when known.
    #[serde(default)]
    pub line: Option<u32>,
    /// The comment on GitHub.
    #[serde(default)]
    pub url: Option<String>,
    /// The commented line changed after the comment.
    #[serde(default)]
    pub outdated: bool,
    #[serde(default)]
    pub lines: Vec<DeliveryCodeLine>,
    #[serde(default)]
    pub comment: Option<DeliveryThreadComment>,
    /// Why it matters, in the coordinator's words.
    #[serde(default)]
    pub note: String,
    /// Why Tinto could not show it.
    #[serde(default)]
    pub error: Option<String>,
}

/// One line of a code thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCodeLine {
    /// `add`, `del` or `ctx`.
    pub kind: String,
    /// Its number before and after the change; `None` on the side it is not.
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
    /// The line the comment is on.
    #[serde(default)]
    pub target: bool,
}

/// A review comment as GitHub has it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryThreadComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub replies: u32,
}

/// A question the coordinator needs the user to answer before a task's
/// stages run. Answers are final: changing one is a new contract version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryDecision {
    pub id: String,
    pub task_id: String,
    pub kind: DeliveryDecisionKind,
    /// Plain language, for the user.
    pub question: String,
    /// Technical context, folded in the interface.
    #[serde(default)]
    pub detail: String,
    /// The code the question is about, shown above the options.
    #[serde(default)]
    pub references: Vec<DeliveryCodeThread>,
    /// `choice` only.
    #[serde(default)]
    pub options: Vec<DeliveryDecisionOption>,
    /// `text` only: the proposed text, exactly as it will be shown.
    #[serde(default)]
    pub text: String,
    /// `permission` only: the command QA may run once allowed, if any.
    #[serde(default)]
    pub command: Option<String>,
    /// `permission` only: how to undo its effect.
    #[serde(default)]
    pub undo: String,
    /// `permission` only: the scripts the command runs, frozen.
    #[serde(default)]
    pub scripts: Vec<DeliveryFrozenScript>,
    /// `permission` only: folders outside the worktree QA may read and list.
    #[serde(default)]
    pub read_dirs: Vec<String>,
    /// `permission` only: folders outside the worktree QA may write in,
    /// the folders of `clients` included.
    #[serde(default)]
    pub write_dirs: Vec<String>,
    /// `permission` only: the user's real clients (`claude`, `codex`) the
    /// command starts; each needs to write in its own folder.
    #[serde(default)]
    pub clients: Vec<String>,
    /// `permission` only: once allowed, the QA jobs a coordinator
    /// dispatches on this task run with full access (no sandbox, no
    /// command limits). The exception for QA that a sandbox cannot run.
    #[serde(default)]
    pub full_access: bool,
    /// `permission` only: why it may not work as asked (for example a
    /// program that cannot start in Codex's sandbox), found when it was
    /// asked and shown to the coordinator and the user.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// `permission` only: the command or its scripts run code from the
    /// task's worktree, so the permission holds for one candidate.
    #[serde(default)]
    pub runs_candidate: bool,
    /// `permission` only: files agents can edit that the frozen scripts
    /// name and Tinto did not freeze; the user is told before allowing.
    #[serde(default)]
    pub unfrozen: Vec<String>,
    /// The worktree content (tree id) the user allowed `runs_candidate` for.
    #[serde(default)]
    pub candidate: Option<String>,
    /// Whether the stages wait for the user. A decision that is not
    /// blocking is the coordinator's own call on something with a clear
    /// recommendation: it is answered when asked, with that recommendation,
    /// and the user can change it at any time.
    #[serde(default = "blocking_by_default")]
    pub blocking: bool,
    pub status: DeliveryDecisionStatus,
    /// The chosen option's label, the approved text, or `allowed`/`denied`.
    pub answer: Option<String>,
    pub requested_by: String,
    pub requested_at_ms: u64,
    pub decided_by: Option<String>,
    pub decided_at_ms: Option<u64>,
}

/// Changes to a repo's settings. A list is added to or removed from; a
/// value given replaces the current one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySettingsChanges {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks_remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qa_commands_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup_remove: Vec<String>,
    /// Commands that tell whether a service the jobs need is up: they
    /// become environment probes (and checks, which probes must be).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes_add: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qa_browser: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
    /// Variables for checks, by name, each set to the value given.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub check_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliverySettingsProposalStatus {
    Pending,
    Approved,
    Rejected,
}

/// Changes to a repo's settings a coordinator asks for. Nothing changes
/// until the user approves it in Tinto, in whole or in part.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySettingsProposal {
    pub id: String,
    pub repo: PathBuf,
    /// The task it was asked for.
    pub task_id: String,
    pub run_id: String,
    /// One plain line, for the user.
    pub reason: String,
    pub changes: DeliverySettingsChanges,
    pub status: DeliverySettingsProposalStatus,
    /// What the user approved: all of `changes` or a part.
    #[serde(default)]
    pub approved: Option<DeliverySettingsChanges>,
    pub requested_by: String,
    pub requested_at_ms: u64,
    pub decided_by: Option<String>,
    pub decided_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryRun {
    pub id: String,
    pub repo: std::path::PathBuf,
    pub title: String,
    /// `active` or `closed`.
    pub status: String,
    /// Increases on every takeover; older generations are fenced.
    pub generation: u64,
    pub owner: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    /// The user's standing choice for the batch: post each QA verdict as a
    /// comment on its Jira issue. `None` until the user sets it in Tinto.
    #[serde(default)]
    pub qa_jira_comment: Option<bool>,
    /// The coordinator ended the batch's brainstorm: from then on Tinto
    /// settles its decisions itself (recommendations, texts, permissions)
    /// and approves its commits and pushes, so nothing stops before the PR,
    /// which still waits for the user.
    #[serde(default)]
    pub unattended: bool,
    /// The user paused the batch: its queued jobs wait, the running ones go on.
    #[serde(default)]
    pub paused: bool,
    /// The user asked the coordinator to create no more tasks in the batch:
    /// it finishes the ones it has, and create_task refuses new ones.
    #[serde(default)]
    pub no_new_tasks: bool,
    /// The Agents conversation coordinating the batch, as its Delivery
    /// tool calls say: Tinto wakes it when something waits on it.
    #[serde(default)]
    pub coordinator_session: Option<String>,
    /// The event cursor the coordinator acknowledged: it called wait_run
    /// with it (or a later one), so it handled every change up to it.
    #[serde(default)]
    pub coordinator_cursor: u64,
    /// The newest cursor wait_run handed it. The changes after
    /// `coordinator_cursor` up to this one were handed over but not
    /// acknowledged: a coordinator interrupted meanwhile gets them again.
    #[serde(default)]
    pub returned_cursor: u64,
    /// The event cursor Tinto last woke the coordinator's conversation up to.
    #[serde(default)]
    pub notified_cursor: u64,
    /// When Tinto last woke it.
    #[serde(default)]
    pub notified_at_ms: Option<u64>,
}

/// One entry of Delivery's audit trail; `seq` is the cursor of wait_run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryEvent {
    pub seq: u64,
    pub at_ms: u64,
    pub kind: String,
    pub task_id: Option<String>,
    pub job_id: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySettings {
    /// Agents running at once. 0 is automatic: one per active task (each
    /// issue), up to what the machine's cores bear ([`auto_capacity_limit`]);
    /// [`UNLIMITED_CAPACITY`] starts every queued job.
    pub capacity: u32,
}

pub const MAX_CAPACITY: u32 = 8;
/// The capacity that never makes a job wait for another task's.
pub const UNLIMITED_CAPACITY: u32 = u32::MAX;

/// Most jobs the automatic capacity runs at once: one per physical core
/// (about half the logical ones), less one for Tinto's checks, at least 2.
/// More jobs than that slow each other down: a type check that takes a
/// minute alone took up to 17 with five jobs on four cores.
pub fn auto_capacity_limit(logical_cores: usize) -> u32 {
    ((logical_cores / 2).saturating_sub(1) as u32).clamp(2, MAX_CAPACITY)
}

impl DeliverySettings {
    /// How many agents may run now, given the active tasks.
    pub fn effective_capacity(&self, active_tasks: usize) -> u32 {
        match self.capacity {
            0 => {
                let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
                (active_tasks as u32).clamp(1, auto_capacity_limit(cores))
            }
            UNLIMITED_CAPACITY => UNLIMITED_CAPACITY,
            fixed => fixed.min(MAX_CAPACITY),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryRepoSettings {
    /// Folder that holds the task worktrees; default `<repo>-wt`.
    pub worktree_root: Option<PathBuf>,
    /// Shell command run in every new worktree (e.g. `npm ci`).
    pub bootstrap: Option<String>,
    /// Base ref for new tasks; default the repo's integration branch.
    pub default_base: Option<String>,
    /// Commands jobs use to verify their work (e.g. `npm test`): jobs are
    /// given them, and a Codex job's sandbox must reach their programs.
    #[serde(default)]
    pub checks: Vec<String>,
    /// More commands QA jobs may run, such as the CLI an issue changes.
    #[serde(default)]
    pub qa_commands: Vec<String>,
    /// Commands that prepare a task's worktree (e.g. `npm --prefix frontend
    /// ci`, a generator): a coordinator picks a task's from them, and Tinto
    /// runs those before its stages.
    #[serde(default)]
    pub setup: Vec<String>,
    /// Environment variables, by name, that jobs run the checks and QA
    /// commands with (e.g. a test database's URL). Test settings, not secrets.
    /// `{task}` in a value becomes the job's task key, lowercase with `_` for
    /// the rest (`agos_474`), so each task can have its own test database.
    #[serde(default)]
    pub check_env: BTreeMap<String, String>,
    /// Whether QA jobs get a browser.
    #[serde(default)]
    pub qa_browser: bool,
    /// Where QA runs on this machine (platforms, scope, clients), in the
    /// user's words. QA jobs get it; coordinators read it instead of asking.
    #[serde(default)]
    pub qa_environment: String,
    /// How Tinto prepares the stack browser QA runs on, if set.
    #[serde(default)]
    pub qa_stack: Option<DeliveryQaStack>,
    /// The agent each stage (`tests`, `review`…) runs with, fixed by the
    /// user. The launch dialog starts from it and coordinators that name no
    /// agent get it.
    #[serde(default)]
    pub stages: BTreeMap<String, DeliveryStageAgent>,
    /// The repo's Jira, for Tinto to run the Jira steps itself. Its token
    /// is stored apart and never returned.
    #[serde(default)]
    pub jira: Option<DeliveryJiraSettings>,
    /// How changes in this repo are verified beyond the stages, in the
    /// user's words (end-to-end runs, browser QA when there is a UI…).
    /// Coordinators read it instead of asking on every issue.
    #[serde(default)]
    pub verification: String,
    /// Whether QA may make paid model calls. `None` until the user sets it.
    #[serde(default)]
    pub paid_model_calls: Option<bool>,
    /// A coordinator's plan of one commit with more files than this must
    /// say why; default [`SINGLE_COMMIT_MAX_FILES`].
    #[serde(default)]
    pub single_commit_max_files: Option<u32>,
    /// Likewise for changed lines (added plus removed); default
    /// [`SINGLE_COMMIT_MAX_LINES`].
    #[serde(default)]
    pub single_commit_max_lines: Option<u32>,
    /// "Reintentar al reiniciar": when Tinto starts, a job it interrupted is
    /// queued again once, unless repeating it is not safe.
    #[serde(default)]
    pub retry_on_restart: bool,
    /// "PR sin revisor": the PR step can be approved without choosing a
    /// reviewer, for repos where nobody else reviews.
    #[serde(default)]
    pub pr_without_reviewer: bool,
    /// The verification commands (`checks`) the user marked as environment
    /// probes: the "Entorno" check runs them (e.g. a database ping).
    #[serde(default)]
    pub environment_probes: Vec<String>,
    /// The reviewers whose comments make the repo's guides.
    #[serde(default)]
    pub guides: Vec<DeliveryGuidePerson>,
    /// Which steps Tinto takes on its own once a batch is past its
    /// brainstorm, and which still wait for the user.
    #[serde(default)]
    pub autonomy: DeliveryAutonomy,
}

/// Per step, `true` when Tinto takes it on its own past the brainstorm (a
/// sync, at any time). The defaults stop only to open and merge the PR and
/// to reply on it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DeliveryAutonomy {
    /// Choices take their recommended option; texts, as proposed.
    pub decisions: bool,
    /// Permissions to run a command, read or write outside the worktree.
    pub permissions: bool,
    /// A QA without its sandbox.
    pub full_access: bool,
    pub settings_proposals: bool,
    /// Each QA problem goes to the coordinator with QA's first suggestion.
    pub qa_problems: bool,
    pub commit: bool,
    pub sync: bool,
    pub push: bool,
    /// Opening the PR, without reviewers unless the coordinator chose some.
    pub pr: bool,
    /// Merging it, once its checks and reviews let it.
    pub merge: bool,
    /// The Jira steps Tinto runs itself.
    pub jira: bool,
    /// Posting a reply on the PR as the user.
    pub pr_reply: bool,
}

impl Default for DeliveryAutonomy {
    fn default() -> Self {
        Self {
            decisions: true,
            permissions: true,
            full_access: true,
            settings_proposals: true,
            qa_problems: true,
            commit: true,
            sync: true,
            push: true,
            pr: false,
            merge: false,
            jira: true,
            pr_reply: false,
        }
    }
}

/// What a guide reviews for: how things look, or how they are built.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryGuideKind {
    Visual,
    Technical,
}

/// A reviewer the user made a guide of the repo, of one kind or both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuidePerson {
    pub login: String,
    pub kinds: Vec<DeliveryGuideKind>,
}

pub const SINGLE_COMMIT_MAX_FILES: u32 = 5;
pub const SINGLE_COMMIT_MAX_LINES: u32 = 300;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryJiraSettings {
    /// `https://jira.example.com`.
    pub host: String,
    /// The project key, such as `AGOS`.
    #[serde(default)]
    pub project: String,
    /// The statuses the issue goes through once the PR is open, in order
    /// (e.g. `En ejecución`, `In Review`).
    #[serde(default)]
    pub after_pr: Vec<String>,
    /// The statuses once the PR is merged (e.g. `Cerrada`).
    #[serde(default)]
    pub after_merge: Vec<String>,
    /// Whether a token is saved. Tinto fills it when it reads the settings.
    #[serde(default)]
    pub has_token: bool,
}

/// Who runs a stage: the agent, its model and its reasoning effort.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryStageAgent {
    pub agent: DeliveryAgent,
    /// The CLI's default when `None`.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning: Option<String>,
    /// The stage's time limit, for a dispatch that gives none; the role's
    /// default when `None`.
    #[serde(default)]
    pub timeout_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCoordinatorEndpoint {
    pub url: String,
    pub token: String,
    /// This executable, for agents outside Tinto to run as a stdio MCP
    /// server that opens Tinto when it is closed.
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryOverview {
    /// Where each task is (by task id): the one answer every surface reads.
    #[serde(default)]
    pub phases: std::collections::BTreeMap<String, super::phase::DeliveryTaskPhase>,
    pub runs: Vec<DeliveryRun>,
    pub tasks: Vec<DeliveryTask>,
    pub jobs: Vec<DeliveryJob>,
    pub leases: Vec<DeliveryLease>,
    pub approvals: Vec<DeliveryApproval>,
    pub decisions: Vec<DeliveryDecision>,
    /// Problems of the QA records, and the user's.
    #[serde(default)]
    pub qa_problems: Vec<DeliveryQaProblem>,
    /// Settings changes coordinators asked for and the user has not decided.
    #[serde(default)]
    pub settings_proposals: Vec<DeliverySettingsProposal>,
    /// Every round of the tasks after their initial delivery, oldest first.
    #[serde(default)]
    pub rounds: Vec<DeliveryRound>,
    /// The tasks' pull requests Tinto follows.
    pub prs: Vec<super::prs::DeliveryPr>,
    pub settings: DeliverySettings,
    /// Agents that may run now: the setting, or one per active task.
    pub capacity: u32,
    pub coordinator: Option<DeliveryCoordinatorEndpoint>,
    /// What each active batch's coordinator has not read yet.
    #[serde(default)]
    pub coordinators: Vec<DeliveryCoordinatorState>,
    /// Issues delivered in several packages, each across its packages.
    #[serde(default)]
    pub issues: Vec<DeliverySplitIssue>,
    /// Why each queued job (by id) has not started.
    #[serde(default)]
    pub queued: std::collections::BTreeMap<String, DeliveryQueueWait>,
}

/// Why a queued job has not started, by the scheduler's own rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryQueueWait {
    pub reason: String,
    pub since_ms: u64,
    /// What it waits for looks stuck: a job far beyond its usual time, a
    /// resource held by a job whose process is gone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// An issue split into packages, as far as its packages got.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySplitIssue {
    pub repo: String,
    pub key: String,
    /// How many packages it has, created or not.
    pub of: u32,
    /// The packages Tinto has, in order.
    pub packages: Vec<DeliveryIssuePackage>,
    /// Every package delivered.
    pub delivered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryIssuePackage {
    pub task_id: String,
    pub index: u32,
    pub title: String,
    pub merged: bool,
    pub delivered: bool,
    /// Put away once delivered: it is no longer among the tasks.
    pub archived: bool,
}

/// Whether a batch's coordinator keeps up with its changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCoordinatorState {
    pub run_id: String,
    /// It is inside wait_run now, so it reads each change as it comes.
    pub waiting: bool,
    /// Changes for it that it has not read and Tinto has not woken it for.
    pub unread: u32,
    /// When the oldest of them happened.
    pub unread_since_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryLogEntry {
    /// `message`, `command`, `error` or `info`.
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryJobLog {
    pub entries: Vec<DeliveryLogEntry>,
    /// Raw lines read so far; pass it back as `from_line` to continue.
    pub next_line: usize,
    pub stderr_tail: String,
}

/// Decisions from before `blocking` existed were all the user's.
fn blocking_by_default() -> bool {
    true
}
