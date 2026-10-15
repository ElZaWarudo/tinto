//! The Delivery engine: the job queue, the runner, job-boundary snapshots,
//! result evaluation and recovery after a restart.
//!
//! A job is written ahead (`pending`) before its process starts, snapshots
//! the worktree when it starts and when it ends, and ends when its process
//! exits. There are no turns.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::adapters::{self, JobLaunch, JobOutcome, JobPaths};
use super::decisions;
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryChange, DeliveryCoordinatorEndpoint, DeliveryDecision,
    DeliveryDecisionKind, DeliveryDecisionStatus, DeliveryFrozenCopy, DeliveryJob, DeliveryJobLog,
    DeliveryJobStatus, DeliveryLease, DeliveryLeaseState, DeliveryLeaseWaiter, DeliveryOverview,
    DeliveryPlatform, DeliveryResultState, DeliverySettings, DeliveryTask,
};
use super::prs::{self, GitHub};
use super::store::DeliveryStore;
use super::tasks::{self, plain_path, Place};
use super::{now_ms, DeliveryError};
use crate::agent_console::checkpoint::{self, CheckpointConfig, WorktreeSnapshot};
use crate::bus::contract::AgentSessionChangeKind;
use crate::wsl_agent::protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION};

pub type Notify = Arc<dyn Fn() + Send + Sync>;
pub type Launcher = Arc<
    dyn Fn(&DeliveryJob, &DeliveryTask, &JobPaths) -> Result<JobLaunch, DeliveryError>
        + Send
        + Sync,
>;
pub type PathsFor = Arc<dyn Fn(&str) -> Result<JobPaths, DeliveryError> + Send + Sync>;
/// Says how Codex's sandbox has to run for a task's job to start the given
/// programs, and which of them cannot run at all.
pub type SandboxPlanner =
    Arc<dyn Fn(&DeliveryTask, &[String]) -> adapters::SandboxPlan + Send + Sync>;

const CODEX_MODEL_SETTING: &str = "codex_model";
/// Written next to a WSL job's files so a restart can stop what is left.
const WSL_JOB_FILE: &str = "wsl-job.json";
/// How long a WSL job gets to exit after `TERM` before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// The stage that gets QA tools: network, the main checkout, QA commands
/// and, when the repo enables it, a browser.
pub const QA_ROLE: &str = "qa";

/// Roles of the built-in flow (backlog-delivery's stages) and their defaults.
pub fn role_defaults(role: &str) -> (bool, Option<&'static str>, u32) {
    match role {
        "tests" => (true, None, 45),
        "implementation" => (true, None, 60),
        "review" => (false, None, 30),
        QA_ROLE => (false, Some("qa"), 60),
        "bootstrap" => (true, None, 30),
        _ => (true, None, 60),
    }
}

#[derive(Clone)]
pub struct DeliveryService {
    pub(super) inner: Arc<Inner>,
}

pub(super) struct Inner {
    pub(super) store: Mutex<DeliveryStore>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    notify: Mutex<Option<Notify>>,
    pub(super) coordinator: Mutex<Option<DeliveryCoordinatorEndpoint>>,
    sandbox_planner: Mutex<SandboxPlanner>,
    pub(super) github: Mutex<Arc<dyn GitHub>>,
    pub(super) jira: Mutex<Arc<dyn super::jira::Jira>>,
    launcher: Launcher,
    paths_for: PathsFor,
    checkpoint_config: CheckpointConfig,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub repo: PathBuf,
    pub distro: Option<String>,
    pub key: String,
    pub title: String,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub task_id: String,
    pub role: String,
    pub agent: DeliveryAgent,
    pub model: Option<String>,
    pub access: DeliveryAccess,
    pub prompt: String,
    pub writes: Option<bool>,
    pub lease: Option<String>,
    pub timeout_minutes: Option<u32>,
    /// Reasoning effort (`low`, `medium`, `high`…); the CLI's default when
    /// `None`.
    pub reasoning: Option<String>,
    /// Where a Claude job on a Windows repo runs; Tinto chooses when `None`.
    pub platform: Option<DeliveryPlatform>,
}

/// How a job's process ended, before its result is evaluated.
struct Ending {
    exit_ok: bool,
    exit_code: Option<i32>,
    cancelled: bool,
    timed_out: bool,
    /// The process ran, so an exclusive resource may be in an unknown state.
    touched: bool,
}

impl DeliveryService {
    pub fn open_default() -> Result<Self, DeliveryError> {
        let claude_distro = std::sync::OnceLock::<Option<String>>::new();
        let claude_distro = Arc::new(claude_distro);
        let launcher: Launcher = Arc::new(move |job, task, paths| {
            let distro = claude_distro.get_or_init(adapters::default_wsl_distro);
            adapters::launch(job, task, paths, distro.as_deref())
        });
        let service = Self::with_parts(
            DeliveryStore::open_default()?,
            launcher,
            Arc::new(JobPaths::for_job),
            CheckpointConfig::default(),
        )?;
        service.set_sandbox_planner(Arc::new(adapters::codex_sandbox_plan));
        Ok(service)
    }

    pub fn set_sandbox_planner(&self, planner: SandboxPlanner) {
        if let Ok(mut slot) = self.inner.sandbox_planner.lock() {
            *slot = planner;
        }
    }

    pub(super) fn sandbox_plan(
        &self,
        task: &DeliveryTask,
        programs: &[String],
    ) -> adapters::SandboxPlan {
        let planner = self
            .inner
            .sandbox_planner
            .lock()
            .ok()
            .map(|slot| slot.clone());
        planner.map_or_else(Default::default, |planner| planner(task, programs))
    }

    pub fn with_parts(
        store: DeliveryStore,
        launcher: Launcher,
        paths_for: PathsFor,
        checkpoint_config: CheckpointConfig,
    ) -> Result<Self, DeliveryError> {
        let service = Self {
            inner: Arc::new(Inner {
                store: Mutex::new(store),
                cancels: Mutex::new(HashMap::new()),
                notify: Mutex::new(None),
                coordinator: Mutex::new(None),
                // Nothing is probed unless the real planner is set.
                sandbox_planner: Mutex::new(Arc::new(|_, _| adapters::SandboxPlan::default())),
                // Nothing reaches GitHub unless the app sets `gh`.
                github: Mutex::new(Arc::new(prs::Offline)),
                jira: Mutex::new(Arc::new(super::jira::JiraHttp)),
                launcher,
                paths_for,
                checkpoint_config,
            }),
        };
        service.reconcile()?;
        service.stage_legacy_jira()?;
        Ok(service)
    }

    pub fn set_notify(&self, notify: Notify) {
        if let Ok(mut slot) = self.inner.notify.lock() {
            *slot = Some(notify);
        }
    }

    pub(super) fn notify(&self) {
        let notify = self.inner.notify.lock().ok().and_then(|slot| slot.clone());
        if let Some(notify) = notify {
            notify();
        }
    }

    pub(super) fn store(&self) -> Result<MutexGuard<'_, DeliveryStore>, DeliveryError> {
        self.inner.store.lock().map_err(|_| {
            DeliveryError::new("delivery_lock_poisoned", "Delivery state is unavailable")
        })
    }

    /// Jobs that were running when Tinto stopped died with it (KTD9). They
    /// become `interrupted`, never restarted on their own, and any resource
    /// they held is quarantined until the user releases it.
    fn reconcile(&self) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let now = now_ms();
        let mut leftovers = Vec::new();
        for mut job in store.jobs()? {
            if !job.status.is_active() {
                continue;
            }
            job.status = DeliveryJobStatus::Interrupted;
            job.ended_at_ms = Some(now);
            job.error = Some("Tinto se cerró mientras el trabajo corría.".to_string());
            store.put_job(&job)?;
            settle_lease(&store, &job, true)?;
            store.record_event(
                now,
                "job_interrupted",
                Some(&job.task_id),
                Some(&job.id),
                "",
            )?;
            let wsl_job = (self.inner.paths_for)(&job.id)
                .ok()
                .and_then(|paths| std::fs::read(paths.dir.join(WSL_JOB_FILE)).ok())
                .and_then(|data| serde_json::from_slice::<adapters::WslJob>(&data).ok());
            leftovers.extend(wsl_job);
        }
        // WSL jobs outlive Tinto; stop what is left without delaying startup.
        if !leftovers.is_empty() {
            std::thread::spawn(move || {
                for wsl_job in leftovers {
                    adapters::stop_wsl_job(&wsl_job, "KILL");
                }
            });
        }
        Ok(())
    }

    pub fn overview(&self) -> Result<DeliveryOverview, DeliveryError> {
        // Never show a commit step as done when its commits are gone.
        if let Err(error) = self.reconcile_commits() {
            eprintln!("tinto: delivery could not check the commit steps: {error}");
        }
        let store = self.store()?;
        let tasks = store.tasks()?;
        let live: HashSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
        let jobs = store
            .jobs()?
            .into_iter()
            .filter(|job| live.contains(job.task_id.as_str()))
            .collect();
        let approvals = store
            .approvals()?
            .into_iter()
            .filter(|approval| live.contains(approval.task_id.as_str()))
            .collect();
        let decisions = store
            .decisions()?
            .into_iter()
            .filter(|decision| live.contains(decision.task_id.as_str()))
            .collect();
        let prs = store
            .prs()?
            .into_iter()
            .filter(|pr| live.contains(pr.task_id.as_str()))
            .collect();
        Ok(DeliveryOverview {
            runs: store.runs()?,
            jobs,
            approvals,
            decisions,
            prs,
            leases: store.leases()?,
            settings: store.settings()?,
            coordinator: self
                .inner
                .coordinator
                .lock()
                .ok()
                .and_then(|slot| slot.clone()),
            tasks,
        })
    }

    pub fn update_settings(
        &self,
        settings: DeliverySettings,
    ) -> Result<DeliverySettings, DeliveryError> {
        let settings = DeliverySettings {
            capacity: settings.capacity.clamp(1, 8),
        };
        self.store()?.set_settings(&settings)?;
        self.tick();
        Ok(settings)
    }

    pub fn set_codex_model(&self, model: Option<String>) -> Result<(), DeliveryError> {
        let model = model
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty());
        self.store()?.set_setting(CODEX_MODEL_SETTING, &model)
    }

    // ---- tasks ----

    pub fn create_task(&self, mut request: NewTask) -> Result<DeliveryTask, DeliveryError> {
        // The view passes `\\?\` paths and coordinators plain ones; a task
        // records the plain form so both see the same repo.
        request.repo = PathBuf::from(plain_path(&request.repo));
        let key = tasks::validate_key(&request.key)?;
        let place = Place::of(request.distro.as_deref());
        let repo_text = plain_path(&request.repo);
        let settings = {
            let store = self.store()?;
            if let Some(existing) = store.tasks()?.into_iter().find(|task| {
                plain_path(&task.repo) == repo_text && task.key.eq_ignore_ascii_case(&key)
            }) {
                return Err(DeliveryError::new(
                    "task_exists",
                    format!(
                        "{} ya está en curso en {}",
                        key,
                        plain_path(&existing.worktree)
                    ),
                ));
            }
            if let Some(run_id) = request.run_id.as_deref() {
                let run = store
                    .run(run_id)?
                    .ok_or_else(|| DeliveryError::not_found("la ejecución", run_id))?;
                if run.status != "active" {
                    return Err(DeliveryError::new(
                        "run_closed",
                        "la ejecución está cerrada",
                    ));
                }
            }
            store.repo_settings(&request.repo)?
        };
        let root = match (&settings.worktree_root, place) {
            (Some(root), _) => root.clone(),
            (None, Place::Native) => tasks::default_worktree_root(&request.repo),
            (None, Place::Wsl(_)) => {
                PathBuf::from(format!("{}-wt", repo_text.trim_end_matches('/')))
            }
        };
        let worktree = tasks::join_place(place, &root, &key);
        let (base_ref, base_commit) = tasks::resolve_base(
            place,
            &request.repo,
            request.base.as_deref(),
            settings.default_base.as_deref(),
        )?;
        let branch = request
            .branch
            .filter(|branch| !branch.trim().is_empty())
            .map(|branch| branch.trim().to_string())
            .unwrap_or_else(|| tasks::default_branch(&key));
        if place == Place::Native {
            std::fs::create_dir_all(&root).map_err(DeliveryError::io)?;
        }
        let start_commit =
            tasks::create_worktree(place, &request.repo, &worktree, &branch, &base_commit)?;
        let now = now_ms();
        let task = DeliveryTask {
            id: uuid::Uuid::new_v4().to_string(),
            run_id: request.run_id,
            repo: request.repo,
            distro: request.distro,
            key,
            title: request.title.trim().to_string(),
            worktree,
            branch,
            base_ref,
            base_commit: start_commit,
            state: "intake".to_string(),
            contract_version: 1,
            created_at_ms: now,
            updated_at_ms: now,
            removed_at_ms: None,
            archived_at_ms: None,
        };
        {
            let store = self.store()?;
            store.put_task(&task)?;
            store.record_event(now, "task_created", Some(&task.id), None, &task.branch)?;
        }
        if let Some(command) = settings
            .bootstrap
            .filter(|command| !command.trim().is_empty())
        {
            self.dispatch(NewJob {
                task_id: task.id.clone(),
                role: "bootstrap".to_string(),
                agent: DeliveryAgent::Shell,
                model: None,
                access: DeliveryAccess::Workspace,
                prompt: command,
                writes: Some(true),
                lease: None,
                timeout_minutes: None,
                reasoning: None,
                platform: None,
            })?;
        } else {
            self.notify();
        }
        Ok(task)
    }

    pub fn task(&self, task_id: &str) -> Result<DeliveryTask, DeliveryError> {
        self.store()?
            .task(task_id)?
            .filter(|task| task.removed_at_ms.is_none())
            .ok_or_else(|| DeliveryError::not_found("la tarea", task_id))
    }

    /// Removes the worktree and the local branch. Refuses when there is work
    /// that would be lost (uncommitted changes or unpublished commits),
    /// unless `force`.
    pub fn remove_task(&self, task_id: &str, force: bool) -> Result<(), DeliveryError> {
        let task = self.task(task_id)?;
        if self
            .store()?
            .task_jobs(task_id)?
            .iter()
            .any(|job| job.status.is_open())
        {
            return Err(DeliveryError::new(
                "task_busy",
                "la tarea tiene trabajos en cola o en marcha; cancélalos antes de eliminarla",
            ));
        }
        let place = Place::of(task.distro.as_deref());
        // A removal that failed halfway (e.g. a folder in use on Windows)
        // leaves the folder without its `.git` link, and a worktree deleted
        // by hand leaves no folder at all: nothing left to keep.
        let present = match place {
            Place::Native => {
                let folder = PathBuf::from(plain_path(&task.worktree));
                let linked = folder.join(".git").exists();
                if !linked {
                    let _ = std::fs::remove_dir(&folder);
                }
                linked
            }
            Place::Wsl(_) => tasks::is_worktree(place, &task.worktree),
        };
        if !present {
            tasks::prune_worktrees(place, &task.repo);
        }
        if present {
            if !force {
                let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
                if state.dirty || state.unpublished > 0 {
                    let mut reasons = Vec::new();
                    if state.dirty {
                        reasons.push("cambios sin confirmar".to_string());
                    }
                    if state.unpublished > 0 {
                        reasons.push(format!("{} commits sin publicar", state.unpublished));
                    }
                    return Err(DeliveryError::new(
                        "task_unsaved",
                        format!("la tarea tiene {}", reasons.join(" y ")),
                    ));
                }
            }
            tasks::remove_worktree(place, &task.repo, &task.worktree, force)?;
        }
        // Published work is on the remote; what was not, the user chose to
        // drop (`force`). A branch git keeps checked out elsewhere stays.
        let _ = tasks::git(place, &task.repo, &["branch", "-D", &task.branch]);
        let store = self.store()?;
        let now = now_ms();
        let mut task = task;
        task.removed_at_ms = Some(now);
        task.updated_at_ms = now;
        store.put_task(&task)?;
        store.record_event(
            now,
            "task_removed",
            Some(&task.id),
            None,
            if force { "force" } else { "" },
        )?;
        drop(store);
        self.notify();
        Ok(())
    }

    pub fn update_task(
        &self,
        task_id: &str,
        state: Option<String>,
        contract_version: Option<u32>,
        title: Option<String>,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        if let Some(state) = state.map(|state| state.trim().to_string()) {
            if state.is_empty() || state.len() > 40 {
                return Err(DeliveryError::new(
                    "invalid_state",
                    "estado de tarea inválido",
                ));
            }
            task.state = state;
        }
        if let Some(version) = contract_version {
            if version < task.contract_version {
                return Err(DeliveryError::new(
                    "invalid_contract_version",
                    "la versión del contrato solo puede aumentar",
                ));
            }
            task.contract_version = version;
        }
        if let Some(title) = title {
            task.title = title.trim().to_string();
        }
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "task_updated",
            Some(&task.id),
            None,
            &task.state,
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    // ---- jobs ----

    pub fn dispatch(&self, request: NewJob) -> Result<DeliveryJob, DeliveryError> {
        let task = self.task(&request.task_id)?;
        let role = request.role.trim().to_ascii_lowercase();
        if role.is_empty() || role.len() > 40 {
            return Err(DeliveryError::new(
                "invalid_role",
                "el rol del trabajo es obligatorio",
            ));
        }
        if request.prompt.trim().is_empty() {
            return Err(DeliveryError::new(
                "invalid_prompt",
                "el encargo está vacío",
            ));
        }
        let (writes, lease, timeout) = role_defaults(&role);
        let reasoning = request
            .reasoning
            .map(|effort| effort.trim().to_ascii_lowercase())
            .filter(|effort| !effort.is_empty());
        if let Some(effort) = reasoning.as_deref() {
            if !effort.chars().all(|c| c.is_ascii_lowercase()) {
                return Err(DeliveryError::new(
                    "invalid_reasoning",
                    format!("nivel de razonamiento desconocido: {effort}"),
                ));
            }
        }
        let store = self.store()?;
        let model = match request.model.filter(|model| !model.trim().is_empty()) {
            None if request.agent == DeliveryAgent::Codex => store
                .setting::<Option<String>>(CODEX_MODEL_SETTING)?
                .flatten(),
            model => model,
        };
        let settings = store.repo_settings(&task.repo)?;
        let qa = role == QA_ROLE;
        let decisions = store.task_decisions(&task.id)?;
        if request.agent != DeliveryAgent::Shell {
            let pending = decisions
                .iter()
                .filter(|decision| decision.status == DeliveryDecisionStatus::Pending)
                .count();
            if pending > 0 {
                return Err(DeliveryError::new(
                    "decisions_pending",
                    format!(
                        "la tarea tiene {pending} decisiones pendientes; el usuario las contesta en Tinto"
                    ),
                ));
            }
        }
        // What the user allowed for this task's QA, on top of the repo's.
        let permitted: Vec<_> = decisions
            .iter()
            .filter(|decision| qa && allowed_permission(decision))
            .collect();
        // A Claude job may run these and nothing else. Codex's sandbox lets
        // it run anything, so for it they say which programs that sandbox
        // must be able to start.
        let allowed_commands = match (request.agent, request.access) {
            (DeliveryAgent::Claude | DeliveryAgent::Codex, DeliveryAccess::Workspace) => {
                let mut commands = settings.checks;
                if qa {
                    commands.extend(settings.qa_commands);
                    commands.extend(permitted.iter().filter_map(|d| d.command.clone()));
                }
                commands
            }
            _ => Vec::new(),
        };
        let mut write_dirs: Vec<String> = Vec::new();
        for dir in permitted.iter().flat_map(|decision| &decision.write_dirs) {
            if !write_dirs.contains(dir) {
                write_dirs.push(dir.clone());
            }
        }
        let mut read_dirs: Vec<String> = Vec::new();
        for dir in permitted.iter().flat_map(|decision| &decision.read_dirs) {
            if !read_dirs.contains(dir) {
                read_dirs.push(dir.clone());
            }
        }
        let decision_notes = decisions
            .iter()
            .filter_map(decisions::decision_note)
            .collect();
        let qa_browser = qa && settings.qa_browser;
        let qa_environment = if qa {
            settings.qa_environment.clone()
        } else {
            String::new()
        };
        let attempt = store
            .task_jobs(&task.id)?
            .iter()
            .filter(|job| job.role == role)
            .map(|job| job.attempt)
            .max()
            .unwrap_or(0)
            + 1;
        let now = now_ms();
        let job = DeliveryJob {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            role,
            attempt,
            agent: request.agent,
            model,
            access: request.access,
            prompt: request.prompt,
            contract_version: task.contract_version,
            writes: request.writes.unwrap_or(writes),
            lease: request
                .lease
                .filter(|lease| !lease.trim().is_empty())
                .or_else(|| lease.map(str::to_string)),
            timeout_minutes: request.timeout_minutes.unwrap_or(timeout).clamp(1, 24 * 60),
            status: DeliveryJobStatus::Queued,
            created_at_ms: now,
            started_at_ms: None,
            ended_at_ms: None,
            pid: None,
            provider_session_id: None,
            exit_code: None,
            error: None,
            start_candidate: None,
            end_candidate: None,
            changes: Vec::new(),
            result: None,
            result_state: None,
            result_note: None,
            undone_at_ms: None,
            allowed_commands,
            qa_browser,
            decisions: decision_notes,
            qa_environment,
            reasoning,
            platform: request.platform,
            ran_on: None,
            read_dirs,
            write_dirs,
            frozen_scripts: Vec::new(),
        };
        store.put_job(&job)?;
        store.record_event(
            now,
            "job_queued",
            Some(&job.task_id),
            Some(&job.id),
            &job.role,
        )?;
        drop(store);
        if job.agent != DeliveryAgent::Shell {
            // A stage on the task is the round that answers the PR's news.
            let _ = self.mark_pr_seen(&job.task_id);
        }
        self.tick();
        Ok(job)
    }

    pub fn job(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        self.store()?
            .job(job_id)?
            .ok_or_else(|| DeliveryError::not_found("el trabajo", job_id))
    }

    pub fn cancel(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let mut job = self.job(job_id)?;
        match job.status {
            DeliveryJobStatus::Queued => {
                let store = self.store()?;
                job.status = DeliveryJobStatus::Cancelled;
                job.ended_at_ms = Some(now_ms());
                store.put_job(&job)?;
                settle_lease(&store, &job, false)?;
                store.record_event(
                    now_ms(),
                    "job_cancelled",
                    Some(&job.task_id),
                    Some(&job.id),
                    "",
                )?;
                drop(store);
                self.tick();
                Ok(job)
            }
            DeliveryJobStatus::Pending | DeliveryJobStatus::Started => {
                let flag = self
                    .inner
                    .cancels
                    .lock()
                    .ok()
                    .and_then(|cancels| cancels.get(job_id).cloned());
                match flag {
                    Some(flag) => {
                        flag.store(true, Ordering::SeqCst);
                        Ok(job)
                    }
                    None => Err(DeliveryError::new(
                        "job_not_running",
                        "el trabajo no está en marcha en esta sesión de Tinto",
                    )),
                }
            }
            _ => Err(DeliveryError::new("job_not_open", "el trabajo ya terminó")),
        }
    }

    /// A new attempt with the same assignment, on the task's current
    /// contract version.
    pub fn retry(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let job = self.job(job_id)?;
        if job.status.is_open() {
            return Err(DeliveryError::new(
                "job_open",
                "el trabajo todavía no terminó",
            ));
        }
        self.dispatch(NewJob {
            task_id: job.task_id,
            role: job.role,
            agent: job.agent,
            model: job.model,
            access: job.access,
            prompt: job.prompt,
            writes: Some(job.writes),
            lease: job.lease,
            timeout_minutes: Some(job.timeout_minutes),
            reasoning: job.reasoning,
            platform: job.platform,
        })
    }

    /// Restores the files a job changed to how they were when it started.
    /// Only the latest job that changed files can be undone.
    pub fn undo(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let mut job = self.job(job_id)?;
        if job.agent == DeliveryAgent::Shell || job.status.is_open() || job.undone_at_ms.is_some() {
            return Err(DeliveryError::new(
                "undo_unavailable",
                "este trabajo no se puede deshacer",
            ));
        }
        let task = self.task(&job.task_id)?;
        let (snapshot, later_changes) = {
            let store = self.store()?;
            let jobs = store.task_jobs(&task.id)?;
            if jobs.iter().any(|other| other.status.is_open()) {
                return Err(DeliveryError::new(
                    "task_busy",
                    "espera a que terminen los trabajos de la tarea",
                ));
            }
            let later = jobs
                .iter()
                .skip_while(|other| other.id != job.id)
                .skip(1)
                .any(|other| {
                    other.agent != DeliveryAgent::Shell
                        && other.undone_at_ms.is_none()
                        && !other.changes.is_empty()
                });
            (store.start_snapshot(&job.id)?, later)
        };
        if later_changes {
            return Err(DeliveryError::new(
                "undo_out_of_order",
                "deshaz primero los trabajos posteriores de esta tarea",
            ));
        }
        let snapshot = snapshot.ok_or_else(|| {
            DeliveryError::new(
                "undo_unavailable",
                "el trabajo no tiene punto de control inicial",
            )
        })?;
        if job.changes.is_empty() && job.status == DeliveryJobStatus::Interrupted {
            let now =
                self.snapshot(&task, &format!("job-{}-undo", job.id), Some(&snapshot.tree))?;
            job.changes = to_changes(&now);
        }
        for change in &job.changes {
            self.revert_file(&task, &snapshot, Path::new(&change.path))?;
        }
        job.undone_at_ms = Some(now_ms());
        let store = self.store()?;
        store.put_job(&job)?;
        store.record_event(
            now_ms(),
            "job_undone",
            Some(&job.task_id),
            Some(&job.id),
            "",
        )?;
        drop(store);
        self.notify();
        Ok(job)
    }

    pub fn job_log(&self, job_id: &str, from_line: usize) -> Result<DeliveryJobLog, DeliveryError> {
        let job = self.job(job_id)?;
        let paths = (self.inner.paths_for)(job_id)?;
        let events = std::fs::read_to_string(&paths.events).unwrap_or_default();
        let lines: Vec<&str> = events.lines().collect();
        let entries = lines
            .iter()
            .skip(from_line)
            .flat_map(|line| adapters::log_entries(job.agent, line))
            .collect();
        let stderr = std::fs::read_to_string(&paths.stderr).unwrap_or_default();
        let tail_start = stderr
            .char_indices()
            .rev()
            .nth(2000)
            .map(|(index, _)| index)
            .unwrap_or(0);
        Ok(DeliveryJobLog {
            entries,
            next_line: lines.len(),
            stderr_tail: stderr[tail_start..].to_string(),
        })
    }

    pub fn release_lease(&self, name: &str, note: &str) -> Result<DeliveryLease, DeliveryError> {
        let store = self.store()?;
        let mut lease = store
            .lease(name)?
            .ok_or_else(|| DeliveryError::not_found("el recurso", name))?;
        match lease.state {
            DeliveryLeaseState::Quarantined => {}
            DeliveryLeaseState::Active => {
                return Err(DeliveryError::new(
                    "lease_active",
                    "un trabajo lo está usando; cancélalo primero",
                ))
            }
            DeliveryLeaseState::Free => return Ok(lease),
        }
        lease.state = DeliveryLeaseState::Free;
        lease.holder_job_id = None;
        lease.holder_task_id = None;
        lease.acquired_at_ms = None;
        lease.generation += 1;
        lease.note = Some(format!("Liberado por el usuario: {}", note.trim()));
        store.put_lease(&lease)?;
        store.record_event(now_ms(), "lease_released", None, None, name)?;
        drop(store);
        self.tick();
        Ok(lease)
    }

    // ---- scheduling ----

    /// Starts every queued job that can run now, in FIFO order: one job per
    /// task at a time, up to the capacity, and exclusive resources in the
    /// order they were requested.
    pub fn tick(&self) {
        match self.select_jobs() {
            Ok(started) => {
                for job_id in started {
                    let service = self.clone();
                    std::thread::spawn(move || service.run_job(&job_id));
                }
            }
            Err(error) => eprintln!("tinto: delivery scheduling failed: {error}"),
        }
        self.notify();
    }

    fn select_jobs(&self) -> Result<Vec<String>, DeliveryError> {
        let store = self.store()?;
        let capacity = store.settings()?.capacity as usize;
        let jobs = store.jobs()?;
        let mut running = jobs.iter().filter(|job| job.status.is_active()).count();
        let mut busy: HashSet<String> = jobs
            .iter()
            .filter(|job| job.status.is_active())
            .map(|job| job.task_id.clone())
            .collect();
        let mut started = Vec::new();
        for mut job in jobs
            .into_iter()
            .filter(|job| job.status == DeliveryJobStatus::Queued)
        {
            if busy.contains(&job.task_id) {
                continue;
            }
            if running >= capacity {
                break;
            }
            if let Some(name) = job.lease.clone() {
                let mut lease = store.lease(&name)?.unwrap_or(DeliveryLease {
                    name: name.clone(),
                    state: DeliveryLeaseState::Free,
                    generation: 0,
                    holder_job_id: None,
                    holder_task_id: None,
                    acquired_at_ms: None,
                    note: None,
                    queue: Vec::new(),
                });
                let first_in_line = lease
                    .queue
                    .first()
                    .is_none_or(|waiter| waiter.job_id == job.id);
                if lease.state == DeliveryLeaseState::Free && first_in_line {
                    lease.queue.retain(|waiter| waiter.job_id != job.id);
                    lease.state = DeliveryLeaseState::Active;
                    lease.holder_job_id = Some(job.id.clone());
                    lease.holder_task_id = Some(job.task_id.clone());
                    lease.acquired_at_ms = Some(now_ms());
                    lease.generation += 1;
                    lease.note = None;
                    store.put_lease(&lease)?;
                } else {
                    if !lease.queue.iter().any(|waiter| waiter.job_id == job.id) {
                        lease.queue.push(DeliveryLeaseWaiter {
                            job_id: job.id.clone(),
                            task_id: job.task_id.clone(),
                            requested_at_ms: now_ms(),
                        });
                        store.put_lease(&lease)?;
                    }
                    continue;
                }
            }
            job.status = DeliveryJobStatus::Pending;
            store.put_job(&job)?;
            if let Ok(mut cancels) = self.inner.cancels.lock() {
                cancels.insert(job.id.clone(), Arc::new(AtomicBool::new(false)));
            }
            busy.insert(job.task_id.clone());
            running += 1;
            started.push(job.id);
        }
        Ok(started)
    }

    // ---- running a job ----

    fn run_job(&self, job_id: &str) {
        let outcome = self.run_job_inner(job_id);
        if let Err(error) = outcome {
            let _ = self.finish_failed(job_id, &error.message);
        }
        if let Ok(mut cancels) = self.inner.cancels.lock() {
            cancels.remove(job_id);
        }
        self.tick();
    }

    fn run_job_inner(&self, job_id: &str) -> Result<(), DeliveryError> {
        let mut job = self.job(job_id)?;
        let task = self.task(&job.task_id)?;
        let start = if job.agent == DeliveryAgent::Shell {
            None
        } else {
            Some(
                self.snapshot(&task, &format!("job-{}-start", job.id), None)
                    .map_err(|error| {
                        DeliveryError::new(
                            error.category,
                            format!(
                                "no se pudo crear el punto de control inicial: {}",
                                error.message
                            ),
                        )
                    })?,
            )
        };
        // Recorded before anything can refuse the job, so a job that never
        // starts still says which code it found.
        if let Some(start) = &start {
            job.start_candidate = Some(start.candidate_id());
            self.store()?.put_job(&job)?;
        }
        let paths = (self.inner.paths_for)(&job.id)?;
        paths.prepare()?;
        if job.role == QA_ROLE {
            job.frozen_scripts = self.prepare_permissions(&task, &paths, start.as_ref())?;
            // A sandbox cannot create the folder it is given as writable.
            if task.distro.is_none() {
                for dir in &job.write_dirs {
                    std::fs::create_dir_all(dir).map_err(|error| {
                        DeliveryError::new(
                            "write_dir_unavailable",
                            format!("no se pudo preparar la carpeta de escritura {dir}: {error}"),
                        )
                    })?;
                }
            }
        }
        let launch = (self.inner.launcher)(&job, &task, &paths)?;
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &launch.cwd {
            command.current_dir(cwd);
        }
        #[cfg(target_os = "windows")]
        crate::windows_process::hide_console(&mut command);
        let mut child = command.spawn().map_err(|error| {
            DeliveryError::new(
                "spawn_failed",
                format!("no se pudo iniciar {}: {error}", launch.program.display()),
            )
        })?;
        #[cfg(target_os = "windows")]
        let process_tree = crate::windows_process::KillOnCloseJob::attach(&child).ok();
        if let Some(wsl_job) = &launch.wsl_job {
            if let Ok(data) = serde_json::to_vec(wsl_job) {
                let _ = std::fs::write(paths.dir.join(WSL_JOB_FILE), data);
            }
        }

        {
            let store = self.store()?;
            let mut current = store
                .job(&job.id)?
                .ok_or_else(|| DeliveryError::not_found("el trabajo", &job.id))?;
            current.status = DeliveryJobStatus::Started;
            current.started_at_ms = Some(now_ms());
            current.pid = Some(child.id());
            current.provider_session_id = launch.provider_session_id.clone();
            current.ran_on = Some(ran_on(&launch));
            current.frozen_scripts = job.frozen_scripts.clone();
            current.start_candidate = start.as_ref().map(WorktreeSnapshot::candidate_id);
            store.put_job(&current)?;
            if let Some(start) = &start {
                store.set_start_snapshot(&job.id, start)?;
            }
            store.record_event(
                now_ms(),
                "job_started",
                Some(&job.task_id),
                Some(&job.id),
                "",
            )?;
        }
        self.notify();

        if let Some(mut stdin) = child.stdin.take() {
            let input = launch.stdin.clone();
            std::thread::spawn(move || {
                let _ = stdin.write_all(input.as_bytes());
            });
        }
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let events_path = paths.events.clone();
        let agent = job.agent;
        let session_known = launch.provider_session_id.is_some();
        let service = self.clone();
        let stdout_job = job.id.clone();
        let stdout_reader = std::thread::spawn(move || {
            let Some(stdout) = stdout else { return };
            let Ok(mut file) = std::fs::File::create(&events_path) else {
                return;
            };
            let mut session_recorded = session_known;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let _ = writeln!(file, "{line}");
                if !session_recorded {
                    if let Some(id) = adapters::session_id_from_line(agent, &line) {
                        session_recorded = true;
                        service.record_session_id(&stdout_job, id);
                    }
                }
            }
        });
        let stderr_path = paths.stderr.clone();
        let stderr_reader = std::thread::spawn(move || {
            let Some(mut stderr) = stderr else { return };
            let mut buffer = Vec::new();
            let _ = stderr.read_to_end(&mut buffer);
            let _ = std::fs::write(&stderr_path, buffer);
        });

        let cancel = self
            .inner
            .cancels
            .lock()
            .ok()
            .and_then(|cancels| cancels.get(&job.id).cloned())
            .unwrap_or_default();
        let deadline = Instant::now() + Duration::from_secs(u64::from(job.timeout_minutes) * 60);
        let mut cancelled = false;
        let mut timed_out = false;
        // When the job is being stopped: kill it outright once this passes.
        let mut kill_at: Option<Instant> = None;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(_) => break None,
            }
            if !cancelled && !timed_out {
                if cancel.load(Ordering::SeqCst) {
                    cancelled = true;
                } else if Instant::now() >= deadline {
                    timed_out = true;
                }
                if cancelled || timed_out {
                    // Closing `wsl.exe` would leave the Linux processes
                    // running, so a WSL job is asked to stop first.
                    kill_at = Some(match &launch.wsl_job {
                        Some(wsl_job) => {
                            adapters::stop_wsl_job(wsl_job, "TERM");
                            Instant::now() + STOP_GRACE
                        }
                        None => Instant::now(),
                    });
                }
            }
            if kill_at.is_some_and(|at| Instant::now() >= at) {
                kill_at = None;
                if let Some(wsl_job) = &launch.wsl_job {
                    adapters::stop_wsl_job(wsl_job, "KILL");
                }
                let _ = child.kill();
                #[cfg(target_os = "windows")]
                if let Some(tree) = &process_tree {
                    let _ = tree.kill_all();
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        #[cfg(target_os = "windows")]
        drop(process_tree);
        let _ = stdout_reader.join();
        let _ = stderr_reader.join();

        let events = std::fs::read_to_string(&paths.events).unwrap_or_default();
        let result_file = std::fs::read_to_string(&paths.result).ok();
        let outcome = adapters::read_outcome(job.agent, &events, result_file.as_deref());
        let end = match &start {
            Some(start) => {
                Some(self.snapshot(&task, &format!("job-{}-end", job.id), Some(&start.tree))?)
            }
            None => None,
        };
        let ending = Ending {
            exit_ok: status.is_some_and(|status| status.success()),
            exit_code: status.and_then(|status| status.code()),
            cancelled,
            timed_out,
            // A job the user cancelled before it did more than read left
            // its resource as it was.
            touched: !cancelled || adapters::acted(&job, &events),
        };
        let stderr_text = std::fs::read_to_string(&paths.stderr).unwrap_or_default();
        self.finish(&job.id, ending, outcome, end, &stderr_text)
    }

    /// Before a QA job starts: refuses it when a permission that runs the
    /// task's code was allowed for other worktree content, and writes the
    /// scripts the user approved, as frozen, into the job's folder.
    fn prepare_permissions(
        &self,
        task: &DeliveryTask,
        paths: &JobPaths,
        start: Option<&WorktreeSnapshot>,
    ) -> Result<Vec<DeliveryFrozenCopy>, DeliveryError> {
        let decisions = self.store()?.task_decisions(&task.id)?;
        let mut copies = Vec::new();
        for (index, decision) in decisions
            .iter()
            .filter(|decision| allowed_permission(decision))
            .enumerate()
        {
            if let (true, Some(start)) = (decision.runs_candidate, start) {
                if decision.candidate.as_deref() != Some(start.tree.as_str()) {
                    return Err(DeliveryError::new(
                        "permission_stale",
                        format!(
                            "El código de la tarea cambió desde que permitiste «{}». Vuelve a permitirlo en Decisiones y relanza la QA.",
                            decision.question
                        ),
                    ));
                }
            }
            let dir = paths.dir.join("frozen").join((index + 1).to_string());
            for script in &decision.scripts {
                let name = script.path.rsplit(['/', '\\']).next().unwrap_or_default();
                let copy = dir.join(name);
                std::fs::create_dir_all(&dir).map_err(DeliveryError::io)?;
                std::fs::write(&copy, &script.content).map_err(DeliveryError::io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o755))
                        .map_err(DeliveryError::io)?;
                }
                copies.push(DeliveryFrozenCopy {
                    original: script.path.clone(),
                    copy: plain_path(&copy),
                });
            }
        }
        Ok(copies)
    }

    fn record_session_id(&self, job_id: &str, session_id: String) {
        if let Ok(store) = self.store() {
            if let Ok(Some(mut job)) = store.job(job_id) {
                job.provider_session_id = Some(session_id);
                let _ = store.put_job(&job);
            }
        }
        self.notify();
    }

    fn finish(
        &self,
        job_id: &str,
        ending: Ending,
        outcome: JobOutcome,
        end: Option<WorktreeSnapshot>,
        stderr: &str,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let mut job = store
            .job(job_id)?
            .ok_or_else(|| DeliveryError::not_found("el trabajo", job_id))?;
        let task = store.task(&job.task_id)?;
        job.ended_at_ms = Some(now_ms());
        job.exit_code = ending.exit_code;
        if outcome.provider_session_id.is_some() {
            job.provider_session_id = outcome.provider_session_id.clone();
        }
        if let Some(end) = &end {
            job.end_candidate = Some(end.candidate_id());
            job.changes = to_changes(end);
        }
        if ending.cancelled {
            job.status = DeliveryJobStatus::Cancelled;
            job.error = Some("Cancelado por el usuario.".to_string());
        } else if ending.timed_out {
            job.status = DeliveryJobStatus::Failed;
            job.error = Some(format!(
                "Se agotó el tiempo máximo de {} minutos.",
                job.timeout_minutes
            ));
        } else if !ending.exit_ok {
            job.status = DeliveryJobStatus::Failed;
            job.error = Some(outcome.error.clone().unwrap_or_else(|| {
                let tail = stderr.trim().lines().last().unwrap_or_default();
                match ending.exit_code {
                    Some(code) if tail.is_empty() => {
                        format!("El proceso terminó con código {code}.")
                    }
                    Some(code) => format!("El proceso terminó con código {code}: {tail}"),
                    None => "El proceso terminó de forma anormal.".to_string(),
                }
            }));
        } else if job.agent == DeliveryAgent::Shell {
            job.status = DeliveryJobStatus::Finished;
        } else {
            match outcome.result {
                Some(result) => {
                    job.status = DeliveryJobStatus::Finished;
                    job.result = Some(result);
                    let (state, note) = result_state(&store, &job, task.as_ref())?;
                    job.result_state = Some(state);
                    job.result_note = note;
                }
                None => {
                    job.status = DeliveryJobStatus::Failed;
                    job.error = outcome
                        .error
                        .clone()
                        .or(outcome.result_error.clone())
                        .or_else(|| Some("El agente no entregó un resultado.".to_string()));
                }
            }
        }
        if job.agent == DeliveryAgent::Claude {
            if let Some(error) = job.error.as_mut().filter(|error| error.contains("/login")) {
                error.push_str(&format!(
                    " — Claude Code no tiene sesión iniciada en {}: iníciala allí con `claude auth login` o elige otra plataforma al lanzar.",
                    job.ran_on.as_deref().unwrap_or("esta plataforma")
                ));
            }
        }
        store.put_job(&job)?;
        settle_lease(&store, &job, ending.touched)?;
        let detail = format!("{:?}", job.status).to_ascii_lowercase();
        store.record_event(
            now_ms(),
            "job_ended",
            Some(&job.task_id),
            Some(&job.id),
            &detail,
        )?;
        Ok(())
    }

    /// A job that failed outside its process (snapshot, launch, store). If
    /// its process had started, its resource may be in an unknown state.
    fn finish_failed(&self, job_id: &str, message: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let Some(mut job) = store.job(job_id)? else {
            return Ok(());
        };
        if !job.status.is_open() {
            return Ok(());
        }
        let touched = job.status == DeliveryJobStatus::Started;
        job.status = DeliveryJobStatus::Failed;
        job.ended_at_ms = Some(now_ms());
        job.error = Some(message.to_string());
        store.put_job(&job)?;
        settle_lease(&store, &job, touched)?;
        store.record_event(
            now_ms(),
            "job_ended",
            Some(&job.task_id),
            Some(&job.id),
            "failed",
        )?;
        Ok(())
    }

    // ---- snapshots ----

    pub(super) fn snapshot(
        &self,
        task: &DeliveryTask,
        name: &str,
        compare_to: Option<&str>,
    ) -> Result<WorktreeSnapshot, DeliveryError> {
        match task.distro.as_deref() {
            None => Ok(checkpoint::snapshot_worktree(
                Path::new(&plain_path(&task.worktree)),
                name,
                now_ms(),
                &self.inner.checkpoint_config,
                compare_to,
            )?),
            Some(distro) => {
                let response = crate::wsl_agent::launcher::request_wsl_agent_with_timeout(
                    distro,
                    &AgentRequest::WorktreeSnapshot {
                        protocol_version: PROTOCOL_VERSION,
                        repo: task.worktree.clone(),
                        allowed_repos: vec![task.worktree.clone()],
                        name: name.to_string(),
                        created_at_ms: now_ms(),
                        compare_to: compare_to.map(str::to_string),
                    },
                    crate::wsl_agent::launcher::CHECKPOINT_CREATE_TIMEOUT,
                )?;
                match response {
                    AgentResponse::WorktreeSnapshot { snapshot } => Ok(snapshot),
                    AgentResponse::Error { category, message } => {
                        Err(DeliveryError::new(category, message))
                    }
                    _ => Err(DeliveryError::new(
                        "malformed_response",
                        "respuesta inesperada del agente WSL",
                    )),
                }
            }
        }
    }

    fn revert_file(
        &self,
        task: &DeliveryTask,
        snapshot: &WorktreeSnapshot,
        path: &Path,
    ) -> Result<(), DeliveryError> {
        match task.distro.as_deref() {
            None => Ok(checkpoint::revert_checkpoint_file(
                &snapshot.checkpoint,
                path,
            )?),
            Some(distro) => {
                let response = crate::wsl_agent::launcher::request_wsl_agent(
                    distro,
                    &AgentRequest::AgentCheckpointRevertFile {
                        protocol_version: PROTOCOL_VERSION,
                        allowed_repos: vec![snapshot.checkpoint.repo.clone()],
                        checkpoint: snapshot.checkpoint.clone(),
                        path: path.to_path_buf(),
                    },
                )?;
                match response {
                    AgentResponse::Unit => Ok(()),
                    AgentResponse::Error { category, message } => {
                        Err(DeliveryError::new(category, message))
                    }
                    _ => Err(DeliveryError::new(
                        "malformed_response",
                        "respuesta inesperada del agente WSL",
                    )),
                }
            }
        }
    }
}

/// Whether a finished job's result can advance its task.
fn result_state(
    store: &DeliveryStore,
    job: &DeliveryJob,
    task: Option<&DeliveryTask>,
) -> Result<(DeliveryResultState, Option<String>), DeliveryError> {
    let superseded = store
        .task_jobs(&job.task_id)?
        .iter()
        .any(|other| other.role == job.role && other.attempt > job.attempt);
    if superseded {
        return Ok((
            DeliveryResultState::Stale,
            Some("Hay un intento más reciente de este rol.".to_string()),
        ));
    }
    if let Some(task) = task {
        if task.contract_version != job.contract_version {
            return Ok((
                DeliveryResultState::Stale,
                Some(format!(
                    "El contrato cambió de v{} a v{} durante el trabajo.",
                    job.contract_version, task.contract_version
                )),
            ));
        }
    }
    if !job.writes && job.start_candidate != job.end_candidate {
        return Ok((
            DeliveryResultState::Invalid,
            Some("Un trabajo de solo lectura cambió archivos.".to_string()),
        ));
    }
    Ok((DeliveryResultState::Accepted, None))
}

/// Releases the job's exclusive resource, or quarantines it when the job
/// ran and did not finish cleanly. Also drops the job from any queue.
fn settle_lease(
    store: &DeliveryStore,
    job: &DeliveryJob,
    touched: bool,
) -> Result<(), DeliveryError> {
    let Some(name) = job.lease.as_deref() else {
        return Ok(());
    };
    let Some(mut lease) = store.lease(name)? else {
        return Ok(());
    };
    lease.queue.retain(|waiter| waiter.job_id != job.id);
    if lease.holder_job_id.as_deref() == Some(job.id.as_str()) {
        if job.status == DeliveryJobStatus::Finished || !touched {
            lease.state = DeliveryLeaseState::Free;
            lease.holder_job_id = None;
            lease.holder_task_id = None;
            lease.acquired_at_ms = None;
            lease.generation += 1;
            lease.note = None;
        } else {
            lease.state = DeliveryLeaseState::Quarantined;
            let how = match job.status {
                DeliveryJobStatus::Cancelled => "se canceló cuando ya había ejecutado comandos",
                DeliveryJobStatus::Interrupted => "seguía en marcha cuando Tinto se cerró",
                _ => "falló a medias",
            };
            let key = store
                .task(&job.task_id)?
                .map_or_else(|| job.task_id.clone(), |task| task.key);
            lease.note = Some(format!(
                "El trabajo {} de {key} {how}, así que el entorno puede haber quedado a medio usar. Nadie más puede usarlo hasta que lo liberes: comprueba que no queden procesos ni datos de prueba suyos y pulsa Liberar.",
                job.role
            ));
        }
    }
    store.put_lease(&lease)
}

impl DeliveryService {
    /// Whether the user allowed this task's QA to run with full access.
    /// Coordinators cannot choose full access; only this lets their QA job
    /// have it.
    pub fn qa_full_access(&self, task_id: &str) -> Result<bool, DeliveryError> {
        Ok(self
            .store()?
            .task_decisions(task_id)?
            .iter()
            .any(|decision| decision.full_access && allowed_permission(decision)))
    }
}

fn allowed_permission(decision: &DeliveryDecision) -> bool {
    decision.kind == DeliveryDecisionKind::Permission
        && decision.answer.as_deref() == Some(decisions::ALLOWED)
}

/// Where a job ran, as its stage shows it.
fn ran_on(launch: &JobLaunch) -> String {
    let place = match &launch.wsl_job {
        Some(wsl_job) => format!("WSL · {}", wsl_job.distro),
        None => match std::env::consts::OS {
            "windows" => "Windows",
            "macos" => "macOS",
            _ => "Linux",
        }
        .to_string(),
    };
    match &launch.note {
        Some(note) => format!("{place} · {note}"),
        None => place,
    }
}

fn to_changes(snapshot: &WorktreeSnapshot) -> Vec<DeliveryChange> {
    snapshot
        .changes
        .iter()
        .map(|change| DeliveryChange {
            path: change.path.to_string_lossy().replace('\\', "/"),
            kind: match change.kind {
                AgentSessionChangeKind::Created => "created",
                AgentSessionChangeKind::Modified => "modified",
                AgentSessionChangeKind::Removed => "removed",
            }
            .to_string(),
        })
        .collect()
}
