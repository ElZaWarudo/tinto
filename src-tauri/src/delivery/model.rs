//! Delivery mode's own contract with the frontend. It is deliberately kept
//! out of the frozen bus contract: Delivery is a separate way of working and
//! evolves on its own (`src/delivery/types.ts` mirrors these types).

use serde::{Deserialize, Serialize};
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
    /// `user` or the coordinator's owner id.
    pub requested_by: String,
    pub requested_at_ms: u64,
    pub decided_at_ms: Option<u64>,
    pub executed_at_ms: Option<u64>,
    pub outcome: Option<String>,
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
