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
    pub worktree: PathBuf,
    pub branch: String,
    pub base_ref: String,
    pub base_commit: String,
    /// Free-form stage label (`intake`, `tests`, `review`, `qa_ready`…).
    pub state: String,
    pub contract_version: u32,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub removed_at_ms: Option<u64>,
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
    /// A plain shell command (bootstrap, checks); no snapshot, no result.
    Shell,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryAccess {
    /// Codex `workspace-write`; Claude `acceptEdits` (cannot run commands).
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
    pub model: Option<String>,
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
    /// Shell commands a Claude job without full access may run, taken from
    /// the repo's verification commands (and, for QA, its QA commands and
    /// the user's permissions) when the job was dispatched. A Codex job gets
    /// the verification commands: its sandbox must reach their programs.
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// QA jobs only: a browser through Playwright's MCP server.
    #[serde(default)]
    pub qa_browser: bool,
    /// The task's answered decisions when the job was dispatched, as the
    /// job's instructions state them.
    #[serde(default)]
    pub decisions: Vec<String>,
    /// QA jobs only: the repo's QA environment when the job was dispatched.
    #[serde(default)]
    pub qa_environment: String,
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
    /// QA jobs only: folders outside the worktree the user's permissions
    /// let the job read and list.
    #[serde(default)]
    pub read_dirs: Vec<String>,
    /// QA jobs only: folders outside the worktree the user's permissions
    /// let the job write in.
    #[serde(default)]
    pub write_dirs: Vec<String>,
    /// QA jobs only: the frozen copies of the scripts the user approved,
    /// written to the job's folder when it started.
    #[serde(default)]
    pub frozen_scripts: Vec<DeliveryFrozenCopy>,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRung {
    Commit,
    Push,
    Pr,
    /// Merge the PR on GitHub and delete its branch there.
    Merge,
    Jira,
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

/// One commit of a commit step's plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCommit {
    /// The exact message: subject, then an optional body after a blank line.
    pub message: String,
    /// The files it includes, relative to the worktree, with `/`.
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
    /// `commit` only: the commits Tinto created, in order.
    #[serde(default)]
    pub created: Vec<String>,
    /// `commit` only: where the branch was before them, to undo the step.
    #[serde(default)]
    pub before: Option<String>,
    /// `push` only: the commit the remote branch ended at once published.
    #[serde(default)]
    pub tip: Option<String>,
    /// What Tinto noticed when it checked the step against git and GitHub,
    /// such as a recorded PR URL that is not the branch's PR.
    #[serde(default)]
    pub notice: Option<String>,
    /// The user's corrections of `outcome`, oldest first. `outcome` is the
    /// latest; each correction keeps the text it replaced.
    #[serde(default)]
    pub corrections: Vec<DeliveryCorrection>,
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
    pub status: DeliveryDecisionStatus,
    /// The chosen option's label, the approved text, or `allowed`/`denied`.
    pub answer: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySettings {
    pub capacity: u32,
}

impl Default for DeliverySettings {
    fn default() -> Self {
        Self { capacity: 3 }
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
    /// Commands jobs use to verify their work (e.g. `npm test`). Claude jobs
    /// without full access may run these and nothing else.
    #[serde(default)]
    pub checks: Vec<String>,
    /// More commands QA jobs may run, such as the CLI an issue changes.
    #[serde(default)]
    pub qa_commands: Vec<String>,
    /// Whether QA jobs get a browser.
    #[serde(default)]
    pub qa_browser: bool,
    /// Where QA runs on this machine (platforms, scope, clients), in the
    /// user's words. QA jobs get it; coordinators read it instead of asking.
    #[serde(default)]
    pub qa_environment: String,
    /// The agent each stage (`tests`, `review`…) runs with, fixed by the
    /// user. The launch dialog starts from it and coordinators that name no
    /// agent get it.
    #[serde(default)]
    pub stages: BTreeMap<String, DeliveryStageAgent>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryCoordinatorEndpoint {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryOverview {
    pub runs: Vec<DeliveryRun>,
    pub tasks: Vec<DeliveryTask>,
    pub jobs: Vec<DeliveryJob>,
    pub leases: Vec<DeliveryLease>,
    pub approvals: Vec<DeliveryApproval>,
    pub decisions: Vec<DeliveryDecision>,
    /// The tasks' pull requests Tinto follows.
    pub prs: Vec<super::prs::DeliveryPr>,
    pub settings: DeliverySettings,
    pub coordinator: Option<DeliveryCoordinatorEndpoint>,
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
