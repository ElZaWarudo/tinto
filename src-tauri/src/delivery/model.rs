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
    /// A delivered task put away: its worktree and local branch are gone
    /// (all of it is on GitHub) and it can be brought back.
    #[serde(default)]
    pub archived_at_ms: Option<u64>,
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
    /// The paused job this one continues, in the same agent session.
    #[serde(default)]
    pub resumes: Option<String>,
    /// What the agent did last, as its log says it, and when: the view
    /// shows it while the job runs and tells when it has gone quiet.
    #[serde(default)]
    pub activity: Option<String>,
    #[serde(default)]
    pub activity_at_ms: Option<u64>,
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
    /// Why the job runs with another agent than the one asked for (it was
    /// not installed on this machine).
    #[serde(default)]
    pub substituted: Option<String>,
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
    /// `jira` only: the statuses Tinto walks the issue through, in order,
    /// when the repo's Jira is set in Tinto. Empty: the coordinator does the
    /// step and reports it.
    #[serde(default)]
    pub jira_statuses: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qa_browser: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<String>,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliverySettings {
    /// Agents running at once. 0 is automatic: one per active task (each
    /// issue), up to [`MAX_CAPACITY`].
    pub capacity: u32,
}

pub const MAX_CAPACITY: u32 = 8;

impl DeliverySettings {
    /// How many agents may run now, given the active tasks.
    pub fn effective_capacity(&self, active_tasks: usize) -> u32 {
        match self.capacity {
            0 => (active_tasks as u32).clamp(1, MAX_CAPACITY),
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
}

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
    /// Settings changes coordinators asked for and the user has not decided.
    #[serde(default)]
    pub settings_proposals: Vec<DeliverySettingsProposal>,
    /// The tasks' pull requests Tinto follows.
    pub prs: Vec<super::prs::DeliveryPr>,
    pub settings: DeliverySettings,
    /// Agents that may run now: the setting, or one per active task.
    pub capacity: u32,
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

/// Decisions from before `blocking` existed were all the user's.
fn blocking_by_default() -> bool {
    true
}
