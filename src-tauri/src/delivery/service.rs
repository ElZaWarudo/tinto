//! The Delivery engine: the job queue, the runner, job-boundary snapshots,
//! result evaluation and recovery after a restart.
//!
//! A job is written ahead (`pending`) before its process starts, snapshots
//! the worktree when it starts and when it ends, and ends when its process
//! exits. There are no turns.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::adapters::{self, JobLaunch, JobOutcome, JobPaths};
use super::decisions;
use super::environment::{self, DeliveryEnvironmentCheck};
use super::job_process::{self, JobProcess};
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryBlocker, DeliveryChange, DeliveryCoordinatorEndpoint,
    DeliveryDecision, DeliveryDecisionKind, DeliveryDecisionStatus, DeliveryFrozenCopy,
    DeliveryJob, DeliveryJobLog, DeliveryJobStatus, DeliveryLease, DeliveryLeaseState,
    DeliveryOverview, DeliveryPackage, DeliveryPlatform, DeliveryPreparedStack, DeliveryQaPrepare,
    DeliveryQaStack, DeliveryRepoSettings, DeliveryResultState, DeliverySettings,
    DeliverySettingsProposalStatus, DeliveryTask, DeliveryTaskMilestone, DeliveryTiming,
    NewBlocker,
};
use super::phase::{
    check_fails, current_checks, current_setup, has_open_findings, is_broken, last_change,
};
use super::prs::{self, GitHub};
use super::queue::Queue;
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
/// Whether an agent's CLI is installed on this machine, outside WSL.
pub type Installed = Arc<dyn Fn(DeliveryAgent) -> bool + Send + Sync>;
/// Runs a repo's "Entorno" check (Docker, the user's probes).
pub type EnvironmentCheck = Arc<
    dyn Fn(&Path, Option<&str>, &DeliveryRepoSettings) -> Vec<DeliveryEnvironmentCheck>
        + Send
        + Sync,
>;

const CODEX_MODEL_SETTING: &str = "codex_model";
/// Models an account refused in a job ("codex:gpt-…"): not dispatched
/// again until the view reads that agent's catalog anew.
const REFUSED_MODELS_SETTING: &str = "refused_models";
/// How long a job its account's usage limit stopped waits to run again.
const USAGE_LIMIT_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(200)
} else {
    Duration::from_secs(60 * 60)
};

/// A resolution's note in one line, up to 300 characters.
fn resolve_note(note: &str) -> Result<String, DeliveryError> {
    let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
    if note.is_empty() || note.chars().count() > 300 {
        return Err(DeliveryError::new(
            "invalid_note",
            "di en una línea (hasta 300 caracteres) cómo quedó resuelto",
        ));
    }
    Ok(note)
}

fn refused_model(agent: DeliveryAgent, model: &str) -> String {
    format!("{}:{}", agent_name(agent).to_lowercase(), model.trim())
}

/// A check that formats or lints: quick enough for a writing stage to run
/// before it ends.
pub(super) fn style_check(command: &str) -> bool {
    let lower = command.to_lowercase();
    [
        "fmt", "format", "prettier", "lint", "clippy", "ruff", "black", "biome",
    ]
    .iter()
    .any(|word| lower.contains(word))
}

/// How each of the task's checks went the last time it ran, and how long it
/// took, for a stage that starts from other code: agents that did not know
/// a suite takes 12 minutes ran it themselves.
fn latest_check_runs(checks: &[String], jobs: &[&DeliveryJob]) -> Option<String> {
    let runs: Vec<String> = checks
        .iter()
        .filter_map(|check| {
            let run = jobs
                .iter()
                .filter(|job| job.role == CHECKS_ROLE && job.prompt == *check)
                .filter(|job| {
                    matches!(
                        job.status,
                        DeliveryJobStatus::Finished | DeliveryJobStatus::Failed
                    )
                })
                .max_by_key(|job| job.created_at_ms)?;
            let took = run.ended_at_ms?.saturating_sub(run.started_at_ms?) / 1000;
            let took = match took {
                0..=59 => format!("{took} s"),
                _ => format!("{} min", took.div_ceil(60)),
            };
            let how = match run.status {
                DeliveryJobStatus::Finished => "passed",
                _ => "failed",
            };
            Some(format!("`{check}` {how} in {took}"))
        })
        .collect();
    (!runs.is_empty()).then(|| format!("Their latest runs, on earlier code: {}.", runs.join("; ")))
}

/// A provider stopping the account for its usage, not the job's work.
fn usage_limit(error: &str) -> bool {
    let lower = error.to_lowercase();
    ["hit your session limit", "usage limit", "hit your limit"]
        .iter()
        .any(|phrase| lower.contains(phrase))
}

/// Where the models of an agent's catalog, as the view last saw it, are kept.
fn catalog_setting(agent: DeliveryAgent) -> Option<&'static str> {
    match agent {
        DeliveryAgent::Codex => Some("codex_models"),
        DeliveryAgent::Muse => Some("muse_models"),
        DeliveryAgent::Claude | DeliveryAgent::Opencode | DeliveryAgent::Shell => None,
    }
}

/// Written next to a WSL job's files so a restart can stop what is left.
const WSL_JOB_FILE: &str = "wsl-job.json";
/// What the job's process reads as its input.
const STDIN_FILE: &str = "stdin.txt";
/// How long a WSL job gets to exit after `TERM` before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// A stopped job whose processes survive the kill is killed again this
/// often, and stops being waited on after `KILL_GIVE_UP`: its queue goes on.
const KILL_AGAIN: Duration = Duration::from_secs(2);
const KILL_GIVE_UP: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(30)
};

/// The stage that gets QA tools: network, the main checkout, QA commands
/// and, when the repo enables it, a browser.
pub const QA_ROLE: &str = "qa";

/// One of the task's checks, run by Tinto itself as a shell job.
pub const CHECKS_ROLE: &str = "checks";

/// The stage that writes the task's code.
const IMPLEMENTATION_ROLE: &str = "implementation";

/// One of the task's setup commands, run by Tinto itself as a shell job
/// before its stages.
pub const SETUP_ROLE: &str = "setup";

/// The resource of browser QA on a repo's QA stack, followed by its name.
pub const QA_BROWSER_LEASE: &str = "qa:browser:";
/// The resource of a command-line QA on its own isolated project, followed
/// by the project's name: it runs without the browser and the QA stack.
pub const QA_CLI_LEASE: &str = "qa:cli:";

/// How long a passing "Entorno" check stands before a job that needs the
/// environment checks it again.
pub(super) const ENVIRONMENT_FRESH_MS: u64 = 60_000;
/// Tinto starts a repo's environment again only after this long.
const ENVIRONMENT_START_EVERY_MS: u64 = 10 * 60_000;
/// How often a running job's process and output are looked at. Tests look
/// often: each of their jobs would otherwise end up to a tick late.
const MONITOR_TICK: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_millis(200)
};

/// The end of a failed check's output its next stage reads.
const CHECK_TAIL_CHARS: usize = 2_000;

/// The longest time limit a job can have, in minutes.
pub const MAX_TIMEOUT_MINUTES: u32 = 24 * 60;
/// The least time a job resolving a merge's conflicts gets by default.
const MERGE_TIMEOUT_MINUTES: u32 = 120;

/// Roles of the built-in flow (backlog-delivery's stages) and their defaults.
pub fn role_defaults(role: &str) -> (bool, Option<&'static str>, u32) {
    match role {
        "tests" => (true, None, 45),
        "implementation" => (true, None, 60),
        "review" => (false, None, 30),
        // Scoping: reads the code and hands its findings back.
        "explore" => (false, None, 30),
        QA_ROLE => (false, Some("qa"), 60),
        "bootstrap" => (true, None, 30),
        CHECKS_ROLE => (false, None, 30),
        SETUP_ROLE => (true, None, 30),
        _ => (true, None, 60),
    }
}

#[derive(Clone)]
pub struct DeliveryService {
    pub(super) inner: Arc<Inner>,
}

pub(super) struct Inner {
    pub(super) store: Mutex<DeliveryStore>,
    /// Held while a PR is read from GitHub and its steps checked (see
    /// `refresh_pr`).
    pub(super) pr_reads: Mutex<()>,
    cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Jobs being stopped to be paused rather than cancelled.
    pausing: Mutex<HashSet<String>>,
    /// Jobs interrupted by the last restart that "Reintentar al reiniciar"
    /// queues again (see [`DeliveryService::retry_interrupted`]), with what
    /// is left of each in WSL.
    restart_retries: Mutex<Vec<(String, Option<adapters::WslJob>)>>,
    /// Jobs whose process outlived the last Tinto, which this one follows
    /// once the app is wired (see [`DeliveryService::follow_running`]).
    adopted: Mutex<Vec<String>>,
    /// Tests set it to stand for Tinto closing: its jobs keep running and
    /// nothing follows them any more.
    closed: AtomicBool,
    notify: Mutex<Option<Notify>>,
    /// Counts every change; wait_run sleeps on it until the next one.
    pub(super) changes: (Mutex<u64>, Condvar),
    /// Coordinators inside wait_run, by run: Tinto does not wake their
    /// conversation then, the wait hands them the changes.
    pub(super) waiting: Mutex<HashMap<String, usize>>,
    pub(super) coordinator: Mutex<Option<DeliveryCoordinatorEndpoint>>,
    sandbox_planner: Mutex<SandboxPlanner>,
    installed: Mutex<Installed>,
    environment_check: Mutex<EnvironmentCheck>,
    /// Each repo's last "Entorno" check, for the jobs that need it.
    environment: Mutex<HashMap<PathBuf, RepoEnvironment>>,
    pub(super) github: Mutex<Arc<dyn GitHub>>,
    pub(super) jira: Mutex<Arc<dyn super::jira::Jira>>,
    launcher: Launcher,
    pub(super) paths_for: PathsFor,
    checkpoint_config: CheckpointConfig,
}

/// A repo's last "Entorno" check, as the jobs that need it read it.
#[derive(Debug, Clone, Default)]
pub(super) struct RepoEnvironment {
    /// What failed, or `None` when everything answered.
    pub(super) problem: Option<String>,
    /// `None` until a check ends.
    pub(super) checked_at_ms: Option<u64>,
    /// A check is running.
    checking: bool,
    /// When Tinto last ran the repo's commands that start the environment.
    started_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct NewTask {
    pub repo: PathBuf,
    pub distro: Option<String>,
    pub key: String,
    pub title: String,
    /// Folders in the main checkout every job of the task may read.
    pub read_dirs: Vec<String>,
    pub base: Option<String>,
    pub branch: Option<String>,
    pub run_id: Option<String>,
    /// One of the ordered pull requests of a split issue.
    pub package: Option<NewPackage>,
}

/// A package as a coordinator asks for it. With `builds_on` (a package
/// index) it starts from that package's branch and is blocked until that
/// one reaches `until` (its PR opened, by default).
#[derive(Debug, Clone)]
pub struct NewPackage {
    pub index: u32,
    pub of: u32,
    pub title: String,
    pub builds_on: Option<u32>,
    pub until: Option<DeliveryTaskMilestone>,
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
    /// Folders the job may read besides its worktree: absolute paths inside
    /// the repo's main checkout or the task's worktree.
    pub read_dirs: Vec<String>,
    /// The model and reasoning come from the stage's settings.
    pub stage_default: bool,
    /// It needs the repo's environment: it waits while that is down. QA
    /// always does.
    pub needs_environment: bool,
}

/// Why preparing a QA stack stopped short.
enum PrepareStop {
    Cancelled,
    Failed(String),
}

/// How a job's process ended, before its result is evaluated.
struct Ending {
    exit_ok: bool,
    exit_code: Option<i32>,
    cancelled: bool,
    timed_out: bool,
    /// The process ran, so an exclusive resource may be in an unknown state.
    touched: bool,
    /// What it left running once it ended, which Tinto stopped.
    leftovers: Vec<String>,
}

/// Reads a job's output file as its process writes it, a whole line at a
/// time.
#[derive(Default)]
struct EventsTail {
    offset: u64,
    /// The start of a line still being written.
    partial: Vec<u8>,
}

impl EventsTail {
    /// The lines written whole since the last read.
    fn read(&mut self, path: &Path) -> Vec<String> {
        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(self.offset)).is_err() || file.read_to_end(&mut bytes).is_err()
        {
            return Vec::new();
        }
        self.offset += bytes.len() as u64;
        self.partial.extend(bytes);
        let Some(end) = self.partial.iter().rposition(|byte| *byte == b'\n') else {
            return Vec::new();
        };
        let whole: Vec<u8> = self.partial.drain(..=end).collect();
        String::from_utf8_lossy(&whole)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The last line, once the process ended without closing it.
    fn rest(self) -> Option<String> {
        let line = String::from_utf8_lossy(&self.partial)
            .trim_end()
            .to_string();
        (!line.is_empty()).then_some(line)
    }
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
        service.set_installed(Arc::new(adapters::installed_on_windows));
        Ok(service)
    }

    /// Says whether an agent is installed on this machine (outside WSL).
    pub fn set_installed(&self, installed: Installed) {
        if let Ok(mut slot) = self.inner.installed.lock() {
            *slot = installed;
        }
    }

    fn is_installed(&self, agent: DeliveryAgent) -> bool {
        let installed = self.inner.installed.lock().ok().map(|slot| slot.clone());
        installed.is_none_or(|installed| installed(agent))
    }

    /// How the "Entorno" check runs; tests replace it.
    pub fn set_environment_check(&self, check: EnvironmentCheck) {
        if let Ok(mut slot) = self.inner.environment_check.lock() {
            *slot = check;
        }
    }

    /// Runs the repo's "Entorno" check. The jobs that need the environment
    /// read its outcome: they wait while it fails and start once it passes.
    pub fn check_environment(
        &self,
        repo: &Path,
        distro: Option<&str>,
    ) -> Result<Vec<DeliveryEnvironmentCheck>, DeliveryError> {
        let settings = self.store()?.repo_settings(repo)?;
        let check = self
            .inner
            .environment_check
            .lock()
            .map(|slot| slot.clone())
            .map_err(|_| {
                DeliveryError::new("delivery_lock_poisoned", "Delivery state is unavailable")
            })?;
        if let Ok(mut repos) = self.inner.environment.lock() {
            repos.entry(repo.to_path_buf()).or_default().checking = true;
        }
        let mut results = check(repo, distro, &settings);
        // A failing environment the repo knows how to start is started, at
        // most once every few minutes, and checked again.
        let started_at_ms = self
            .inner
            .environment
            .lock()
            .ok()
            .and_then(|repos| repos.get(repo).and_then(|state| state.started_at_ms));
        let due = started_at_ms
            .is_none_or(|at| now_ms().saturating_sub(at) >= ENVIRONMENT_START_EVERY_MS);
        let mut started = started_at_ms;
        if results.iter().any(|result| !result.ok) && !settings.environment_up.is_empty() && due {
            started = Some(now_ms());
            let start = environment::start(repo, distro, &settings.environment_up);
            results = check(repo, distro, &settings);
            results.extend(start.into_iter().filter(|step| !step.ok));
        }
        let failed: Vec<String> = results
            .iter()
            .filter(|result| !result.ok)
            .map(|result| format!("{}: {}", result.name, result.output))
            .collect();
        if let Ok(mut repos) = self.inner.environment.lock() {
            repos.insert(
                repo.to_path_buf(),
                RepoEnvironment {
                    problem: (!failed.is_empty()).then(|| failed.join("\n")),
                    checked_at_ms: Some(now_ms()),
                    checking: false,
                    started_at_ms: started,
                },
            );
        }
        // The jobs waiting on it start, or say why they wait.
        self.tick();
        Ok(results)
    }

    /// Checks again, in the background, the environment of each repo with a
    /// queued job that needs it and no recent check. Notifies only through
    /// the checks it starts, so the blockers' loop can call it.
    pub fn check_due_environments(&self) {
        let due = match self.environments_due() {
            Ok(due) => due,
            Err(error) => {
                eprintln!(
                    "tinto: delivery could not read the jobs waiting on the environment: {error}"
                );
                return;
            }
        };
        for (repo, distro) in due {
            if let Ok(mut repos) = self.inner.environment.lock() {
                let state = repos.entry(repo.clone()).or_default();
                if state.checking {
                    continue;
                }
                state.checking = true;
            }
            let service = self.clone();
            std::thread::spawn(move || {
                if let Err(error) = service.check_environment(&repo, distro.as_deref()) {
                    eprintln!("tinto: delivery could not check the environment: {error}");
                }
            });
        }
    }

    fn environments_due(&self) -> Result<Vec<(PathBuf, Option<String>)>, DeliveryError> {
        let store = self.store()?;
        let tasks = store.tasks()?;
        let repos = self
            .inner
            .environment
            .lock()
            .map(|repos| repos.clone())
            .unwrap_or_default();
        let now = now_ms();
        let mut due: Vec<(PathBuf, Option<String>)> = Vec::new();
        for job in store
            .jobs()?
            .iter()
            .filter(|job| job.status == DeliveryJobStatus::Queued && job.needs_environment)
        {
            let Some(task) = tasks.iter().find(|task| task.id == job.task_id) else {
                continue;
            };
            let fresh = repos.get(&task.repo).is_some_and(|state| {
                state.checking
                    || state
                        .checked_at_ms
                        .is_some_and(|at| now.saturating_sub(at) < ENVIRONMENT_FRESH_MS)
            });
            if !fresh && !due.iter().any(|(repo, _)| repo == &task.repo) {
                due.push((task.repo.clone(), task.distro.clone()));
            }
        }
        Ok(due)
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
                pr_reads: Mutex::new(()),
                cancels: Mutex::new(HashMap::new()),
                pausing: Mutex::new(HashSet::new()),
                restart_retries: Mutex::new(Vec::new()),
                adopted: Mutex::new(Vec::new()),
                closed: AtomicBool::new(false),
                notify: Mutex::new(None),
                changes: (Mutex::new(0), Condvar::new()),
                waiting: Mutex::new(HashMap::new()),
                coordinator: Mutex::new(None),
                // Nothing is probed unless the real planner is set.
                sandbox_planner: Mutex::new(Arc::new(|_, _| adapters::SandboxPlan::default())),
                // Every agent counts as installed unless the app checks.
                installed: Mutex::new(Arc::new(|_| true)),
                environment_check: Mutex::new(Arc::new(environment::check)),
                environment: Mutex::new(HashMap::new()),
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
        if let Err(error) = service.record_missed_qa_problems() {
            eprintln!("tinto: delivery could not read earlier QA problems: {error}");
        }
        Ok(service)
    }

    pub fn set_notify(&self, notify: Notify) {
        if let Ok(mut slot) = self.inner.notify.lock() {
            *slot = Some(notify);
        }
    }

    pub(super) fn notify(&self) {
        let (count, changed) = &self.inner.changes;
        if let Ok(mut count) = count.lock() {
            *count += 1;
            changed.notify_all();
        }
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

    /// A job's process outlives Tinto: the jobs still running when it
    /// starts again are adopted, and so are those whose agent delivered its
    /// result while Tinto was closed; they end as any other. The rest died
    /// (KTD9): they become `interrupted` and any resource they held is
    /// quarantined until the user releases it. With "Reintentar al
    /// reiniciar" in their repo, the ones safe to repeat are queued again
    /// once the app is wired.
    fn reconcile(&self) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let now = now_ms();
        let mut leftovers = Vec::new();
        let mut retries = Vec::new();
        let mut adopted = Vec::new();
        for mut job in store.jobs()? {
            if !job.status.is_active() {
                continue;
            }
            if self.outlived_tinto(&job) {
                if let Ok(mut cancels) = self.inner.cancels.lock() {
                    cancels.insert(job.id.clone(), Arc::new(AtomicBool::new(false)));
                }
                adopted.push(job.id);
                continue;
            }
            // Past its run's brainstorm nothing waits for the user, so the
            // job is repeated as if the repo asked for it. Checks and setup
            // only run commands: they always run again.
            let commands = job.agent == DeliveryAgent::Shell
                && matches!(job.role.as_str(), CHECKS_ROLE | SETUP_ROLE);
            let retry_on_restart = commands
                || match store.task(&job.task_id)? {
                    Some(task) => {
                        let unattended = match task.run_id.as_deref() {
                            Some(run_id) => store.run(run_id)?.is_some_and(|run| run.unattended),
                            None => false,
                        };
                        unattended || store.repo_settings(&task.repo)?.retry_on_restart
                    }
                    None => false,
                };
            let not_retried = if retry_on_restart {
                not_safe_to_repeat(&job)
            } else {
                None
            };
            let retry = retry_on_restart && not_retried.is_none();
            job.status = DeliveryJobStatus::Interrupted;
            job.ended_at_ms = Some(now);
            job.current_command = None;
            job.current_command_at_ms = None;
            job.error = Some(match &not_retried {
                Some(reason) => {
                    format!("Tinto se cerró mientras el trabajo corría. No se reintenta: {reason}.")
                }
                None => "Tinto se cerró mientras el trabajo corría.".to_string(),
            });
            store.put_job(&job)?;
            settle_lease(&store, &job, true, &[])?;
            store.record_event(
                now,
                "job_interrupted",
                Some(&job.task_id),
                Some(&job.id),
                &not_retried
                    .map(|reason| format!("no se reintenta: {reason}"))
                    .unwrap_or_default(),
            )?;
            let wsl_job = (self.inner.paths_for)(&job.id)
                .ok()
                .and_then(|paths| std::fs::read(paths.dir.join(WSL_JOB_FILE)).ok())
                .and_then(|data| serde_json::from_slice::<adapters::WslJob>(&data).ok());
            if retry {
                retries.push((job.id.clone(), wsl_job));
            } else {
                leftovers.extend(wsl_job);
            }
        }
        if let Ok(mut slot) = self.inner.restart_retries.lock() {
            *slot = retries;
        }
        if let Ok(mut slot) = self.inner.adopted.lock() {
            *slot = adopted;
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

    /// Whether a job Tinto had running when it closed goes on: its process
    /// still runs, or its agent delivered its result meanwhile.
    fn outlived_tinto(&self, job: &DeliveryJob) -> bool {
        let (DeliveryJobStatus::Started, Some(pid), Some(started)) =
            (job.status, job.pid, job.process_started.as_deref())
        else {
            return false;
        };
        if job_process::alive(pid, started) {
            return true;
        }
        let Ok(paths) = (self.inner.paths_for)(&job.id) else {
            return false;
        };
        // A command that ended meanwhile left its exit code.
        if job.agent == DeliveryAgent::Shell {
            return adapters::exit_code_left(&paths.dir).is_some();
        }
        let events = std::fs::read_to_string(&paths.events).unwrap_or_default();
        let result = std::fs::read_to_string(&paths.result).ok();
        adapters::read_outcome(job.agent, &events, result.as_deref())
            .result
            .is_some()
    }

    /// Follows, each in its thread, the jobs that outlived the last Tinto.
    /// Called once the app is wired.
    pub fn follow_running(&self) {
        let adopted = self
            .inner
            .adopted
            .lock()
            .map(|mut slot| std::mem::take(&mut *slot))
            .unwrap_or_default();
        for job_id in adopted {
            let service = self.clone();
            std::thread::spawn(move || service.end_job(&job_id, service.follow_adopted(&job_id)));
        }
    }

    /// The jobs running now, each with its task's key: closing Tinto leaves
    /// them running.
    pub fn running_jobs(&self) -> Result<Vec<(String, DeliveryJob)>, DeliveryError> {
        let store = self.store()?;
        let mut running = Vec::new();
        for job in store
            .jobs()?
            .into_iter()
            .filter(|job| job.status.is_active())
        {
            let key = store
                .task(&job.task_id)?
                .map(|task| task.key)
                .unwrap_or_default();
            running.push((key, job));
        }
        Ok(running)
    }

    /// Stands for Tinto closing, in tests: its jobs keep running and
    /// nothing in this service follows them any more.
    #[cfg(test)]
    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
    }

    pub fn overview(&self) -> Result<DeliveryOverview, DeliveryError> {
        // Never show a commit step as done when its commits are gone.
        if let Err(error) = self.reconcile_commits() {
            eprintln!("tinto: delivery could not check the commit steps: {error}");
        }
        let coordinators = self.coordinator_states()?;
        let queued = self.queue_waits()?;
        let mut store = self.store()?;
        let tasks = store.tasks()?;
        let live: HashSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
        let live_jobs = |store: &DeliveryStore| -> Result<Vec<DeliveryJob>, DeliveryError> {
            Ok(store
                .jobs()?
                .into_iter()
                .filter(|job| live.contains(job.task_id.as_str()))
                .collect())
        };
        let mut jobs = live_jobs(&store)?;
        // A QA record copied or written after its job ended is rendered
        // before the view shows that job.
        let appeared: Vec<DeliveryJob> = tasks
            .iter()
            .filter_map(|task| {
                jobs.iter()
                    .filter(|job| job.task_id == task.id && job.role == QA_ROLE)
                    .max_by_key(|job| job.created_at_ms)
                    .filter(|job| self.record_appeared(task, job))
                    .cloned()
            })
            .collect();
        if !appeared.is_empty() {
            drop(store);
            for job in &appeared {
                if let Err(error) = self.refresh_visual_record(job) {
                    eprintln!("tinto: delivery could not render a QA record: {error}");
                }
            }
            store = self.store()?;
            jobs = live_jobs(&store)?;
        }
        let approvals: Vec<_> = store
            .approvals()?
            .into_iter()
            .filter(|approval| live.contains(approval.task_id.as_str()))
            .collect();
        let decisions: Vec<_> = store
            .decisions()?
            .into_iter()
            .filter(|decision| live.contains(decision.task_id.as_str()))
            .collect();
        let settings_proposals: Vec<_> = store
            .settings_proposals()?
            .into_iter()
            .filter(|proposal| proposal.status == DeliverySettingsProposalStatus::Pending)
            .collect();
        let prs: Vec<_> = store
            .prs()?
            .into_iter()
            .filter(|pr| live.contains(pr.task_id.as_str()))
            .collect();
        let qa_problems: Vec<_> = store
            .qa_problems()?
            .into_iter()
            .filter(|problem| live.contains(problem.task_id.as_str()))
            .collect();
        let rounds: Vec<_> = store
            .rounds()?
            .into_iter()
            .filter(|round| live.contains(round.task_id.as_str()))
            .collect();
        let runs = store.runs()?;
        let leases = store.leases()?;
        // Git is asked only about the tasks whose sync stopped on conflicts.
        let merging: HashSet<String> = tasks
            .iter()
            .filter(|task| {
                let steps: Vec<&super::model::DeliveryApproval> = approvals
                    .iter()
                    .filter(|approval| approval.task_id == task.id)
                    .collect();
                super::phase::open_merge(&steps, true).is_some()
                    && tasks::merge_head(Place::of(task.distro.as_deref()), &task.worktree)
                        .is_some()
            })
            .map(|task| task.id.clone())
            .collect();
        // The batches whose coordinator checks a quarantined environment
        // itself: past the brainstorm, or with nothing waiting on the user.
        let releasable_runs: HashSet<String> = runs
            .iter()
            .filter(|run| {
                let asking = tasks.iter().any(|task| {
                    task.run_id.as_deref() == Some(run.id.as_str())
                        && decisions.iter().any(|decision| {
                            decision.task_id == task.id
                                && decision.status == DeliveryDecisionStatus::Pending
                        })
                });
                let proposing = settings_proposals
                    .iter()
                    .any(|proposal| proposal.run_id == run.id);
                run.unattended || !(asking || proposing)
            })
            .map(|run| run.id.clone())
            .collect();
        let review_optional: HashSet<String> = tasks
            .iter()
            .filter(|task| {
                store
                    .repo_settings(&task.repo)
                    .is_ok_and(|settings| settings.pr_without_reviewer)
            })
            .map(|task| task.id.clone())
            .collect();
        let no_qa: HashSet<String> = tasks
            .iter()
            .filter(|task| {
                store
                    .repo_settings(&task.repo)
                    .is_ok_and(|settings| !settings.has_qa())
            })
            .map(|task| task.id.clone())
            .collect();
        // Where each task is, from the same facts the view gets.
        let facts = super::phase::Facts {
            runs: &runs,
            jobs: &jobs,
            leases: &leases,
            approvals: &approvals,
            decisions: &decisions,
            settings_proposals: &settings_proposals,
            prs: &prs,
            rounds: &rounds,
            queued: &queued,
            merging: &merging,
            releasable_runs: &releasable_runs,
            review_optional: &review_optional,
            no_qa: &no_qa,
        };
        let phases = tasks
            .iter()
            .map(|task| (task.id.clone(), super::phase::task_phase(task, &facts)))
            .collect();
        let archived_packages: Vec<DeliveryTask> = store
            .removed_tasks()?
            .into_iter()
            .filter(|task| task.package.is_some() && task.archived_at_ms.is_some())
            .collect();
        let issues = super::phase::split_issues(&tasks, &archived_packages, &phases, &facts);
        Ok(DeliveryOverview {
            issues,
            phases,
            runs,
            jobs,
            approvals,
            decisions,
            qa_problems,
            settings_proposals,
            rounds,
            prs,
            leases,
            capacity: store.settings()?.effective_capacity(tasks.len()),
            settings: store.settings()?,
            coordinator: self
                .inner
                .coordinator
                .lock()
                .ok()
                .and_then(|slot| slot.clone()),
            coordinators,
            queued,
            quotas: super::queue::quotas(&store)?,
            tasks,
        })
    }

    pub fn update_settings(
        &self,
        settings: DeliverySettings,
    ) -> Result<DeliverySettings, DeliveryError> {
        let capacity = match settings.capacity {
            super::model::UNLIMITED_CAPACITY => super::model::UNLIMITED_CAPACITY,
            fixed => fixed.min(super::model::MAX_CAPACITY),
        };
        let settings = DeliverySettings { capacity };
        self.store()?.set_settings(&settings)?;
        self.tick();
        Ok(settings)
    }

    /// The model catalog of Codex or Muse Code as the view last saw it for
    /// the account: its models and its default, which Codex jobs that name
    /// no model use.
    pub fn set_model_catalog(
        &self,
        agent: DeliveryAgent,
        default_model: Option<String>,
        models: Vec<String>,
    ) -> Result<(), DeliveryError> {
        let setting = catalog_setting(agent).ok_or_else(|| {
            DeliveryError::new(
                "invalid_arguments",
                "solo Codex y Muse Code tienen catálogo de modelos",
            )
        })?;
        let mut known: Vec<String> = Vec::new();
        for model in models.iter().map(|model| model.trim()) {
            if !model.is_empty() && !known.iter().any(|seen| seen == model) {
                known.push(model.to_string());
            }
        }
        let store = self.store()?;
        store.set_setting(setting, &known)?;
        // A catalog seen anew: what the account refused may be back.
        let prefix = refused_model(agent, "");
        let refused: Vec<String> = store
            .setting::<Vec<String>>(REFUSED_MODELS_SETTING)?
            .unwrap_or_default()
            .into_iter()
            .filter(|key| !key.starts_with(&prefix))
            .collect();
        store.set_setting(REFUSED_MODELS_SETTING, &refused)?;
        let default_model = default_model
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty());
        if agent == DeliveryAgent::Codex && default_model.is_some() {
            store.set_setting(CODEX_MODEL_SETTING, &default_model)?;
        }
        Ok(())
    }

    /// Refuses a Codex or Muse Code model the account's catalog, as Tinto
    /// last saw it, does not have, naming the ones it has. Claude Code's
    /// models are a fixed list, and before any catalog is seen nothing is
    /// refused.
    pub fn check_model(&self, agent: DeliveryAgent, model: &str) -> Result<(), DeliveryError> {
        let Some(setting) = catalog_setting(agent) else {
            return Ok(());
        };
        let known: Vec<String> = self.store()?.setting(setting)?.unwrap_or_default();
        let model = model.trim();
        if known.is_empty() || known.iter().any(|known| known == model) {
            return Ok(());
        }
        Err(DeliveryError::new(
            "model_unavailable",
            format!(
                "{} no ofrece el modelo «{model}» a esta cuenta (según el catálogo que Tinto vio por última vez); elige uno de estos: {}",
                agent_name(agent),
                known.join(", ")
            ),
        ))
    }

    // ---- tasks ----

    pub fn create_task(&self, mut request: NewTask) -> Result<DeliveryTask, DeliveryError> {
        // The view passes `\\?\` paths and coordinators plain ones; a task
        // records the plain form so both see the same repo.
        request.repo = PathBuf::from(plain_path(&request.repo));
        let key = tasks::validate_key(&request.key)?;
        let place = Place::of(request.distro.as_deref());
        let repo_text = plain_path(&request.repo);
        let package = request.package.as_ref();
        if let Some(package) = package {
            validate_package(package)?;
        }
        let (settings, builds_on) = {
            let store = self.store()?;
            let same_issue: Vec<DeliveryTask> = store
                .tasks()?
                .into_iter()
                .filter(|task| {
                    plain_path(&task.repo) == repo_text && task.key.eq_ignore_ascii_case(&key)
                })
                .collect();
            // Packages of one issue live side by side; anything else is one
            // task per issue.
            if let Some(existing) = same_issue
                .iter()
                .find(|task| match (&task.package, package) {
                    (Some(existing), Some(wanted)) => existing.index == wanted.index,
                    _ => true,
                })
            {
                return Err(DeliveryError::new(
                    "task_exists",
                    format!(
                        "{} ya está en curso en {}",
                        existing.label(),
                        plain_path(&existing.worktree)
                    ),
                ));
            }
            if let (Some(wanted), Some(other)) = (
                package,
                same_issue.iter().find_map(|task| task.package.as_ref()),
            ) {
                if other.of != wanted.of {
                    return Err(DeliveryError::new(
                        "invalid_package",
                        format!(
                            "los paquetes de {key} son {}: este dice {}",
                            other.of, wanted.of
                        ),
                    ));
                }
            }
            let builds_on = match package.and_then(|package| package.builds_on) {
                None => None,
                Some(index) => Some(
                    same_issue
                        .iter()
                        .find(|task| task.package.as_ref().is_some_and(|p| p.index == index))
                        .cloned()
                        .ok_or_else(|| {
                            DeliveryError::new(
                                "invalid_package",
                                format!("{key} no tiene en curso el paquete {index} sobre el que construir"),
                            )
                        })?,
                ),
            };
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
                if run.no_new_tasks {
                    return Err(DeliveryError::new(
                        "run_no_new_tasks",
                        "el usuario pidió no crear más tareas en este lote: termina las que tiene",
                    ));
                }
            }
            (store.repo_settings(&request.repo)?, builds_on)
        };
        let root = match (&settings.worktree_root, place) {
            (Some(root), _) => root.clone(),
            (None, Place::Native) => tasks::default_worktree_root(&request.repo),
            (None, Place::Wsl(_)) => {
                PathBuf::from(format!("{}-wt", repo_text.trim_end_matches('/')))
            }
        };
        // A package's worktree and branch carry its number: `AGOS-432-2`.
        let leaf = match package {
            Some(package) => format!("{key}-{}", package.index),
            None => key.clone(),
        };
        let worktree = tasks::join_place(place, &root, &leaf);
        // Checked before anything is created.
        let read_dirs = readable_dirs(
            &request.read_dirs,
            &request.repo,
            &worktree,
            request.distro.as_deref(),
        )?;
        // A package built on another starts from that package's branch.
        let base = match &builds_on {
            Some(other) => Some(other.branch.as_str()),
            None => request.base.as_deref(),
        };
        let (base_ref, base_commit) =
            tasks::resolve_base(place, &request.repo, base, settings.default_base.as_deref())?;
        let branch = request
            .branch
            .filter(|branch| !branch.trim().is_empty())
            .map(|branch| branch.trim().to_string())
            .unwrap_or_else(|| tasks::default_branch(&leaf));
        let until = package
            .and_then(|package| package.until)
            .unwrap_or(DeliveryTaskMilestone::PrOpened);
        let package = request.package.map(|package| DeliveryPackage {
            index: package.index,
            of: package.of,
            title: package.title.trim().to_string(),
            builds_on: builds_on.as_ref().map(|other| other.id.clone()),
        });
        if place == Place::Native {
            std::fs::create_dir_all(&root).map_err(DeliveryError::io)?;
        }
        let start_commit =
            tasks::create_worktree(place, &request.repo, &worktree, &branch, &base_commit)?;
        // A job that finds them missing checks them out too (see run_job).
        if let Err(error) = tasks::init_submodules(place, &worktree, true) {
            eprintln!("tinto: delivery could not check out the submodules of {key}: {error}");
        }
        let now = now_ms();
        let task = DeliveryTask {
            id: uuid::Uuid::new_v4().to_string(),
            run_id: request.run_id,
            repo: request.repo,
            distro: request.distro,
            key,
            title: request.title.trim().to_string(),
            package,
            worktree,
            branch,
            base_ref,
            base_commit: start_commit,
            state: "intake".to_string(),
            contract_version: 1,
            read_dirs,
            created_at_ms: now,
            updated_at_ms: now,
            removed_at_ms: None,
            archived_at_ms: None,
            blockers: Vec::new(),
            blocked_reason: String::new(),
            unblocked: None,
            checks: Vec::new(),
            setup: Vec::new(),
            parent: None,
            parent_read_at_ms: None,
            restack_from: None,
            skip_ci: false,
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
                read_dirs: Vec::new(),
                stage_default: false,
                needs_environment: false,
            })?;
        } else {
            self.notify();
        }
        // Its stages wait for the package it builds on (after the bootstrap:
        // a blocked task takes no job).
        if let Some(other) = builds_on {
            let index = other.package.as_ref().map_or(0, |package| package.index);
            return self.set_blockers(
                &task.id,
                vec![NewBlocker::Task {
                    task: other.id,
                    until: Some(until),
                }],
                &format!("espera al paquete {index}"),
            );
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

    /// Edits the task. The user's edits wake the coordinator; its own
    /// (`set_task_state`) are recorded apart and do not.
    pub fn update_task(
        &self,
        task_id: &str,
        state: Option<String>,
        contract_version: Option<u32>,
        title: Option<String>,
        by_user: bool,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let before = task.clone();
        if let Some(state) = state.map(|state| state.trim().to_string()) {
            let length = state.chars().count();
            if length == 0 || length > 40 {
                return Err(DeliveryError::new(
                    "invalid_state",
                    format!("el estado de la tarea admite de 1 a 40 caracteres, de cualquier tipo (este tiene {length}); para un bloqueo usa set_task_blocked"),
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
        // What changed, as the coordinator reads it.
        let mut changes = Vec::new();
        if task.state != before.state {
            changes.push(format!("estado «{}»", task.state));
        }
        if task.title != before.title {
            changes.push(format!("título «{}»", task.title));
        }
        if task.contract_version != before.contract_version {
            changes.push(format!("contrato v{}", task.contract_version));
        }
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            if by_user {
                "task_updated"
            } else {
                "task_state_set"
            },
            Some(&task.id),
            None,
            &changes.join(", "),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    /// The checks Tinto runs on what the task's stages change. Each must be
    /// one of the repo's checks as the user wrote it: Tinto runs them outside
    /// any agent sandbox.
    pub fn set_task_checks(
        &self,
        task_id: &str,
        checks: Vec<String>,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let store = self.store()?;
        let allowed = store.repo_settings(&task.repo)?.checks;
        task.checks = pick_commands(&checks, &allowed).map_err(|check| {
            DeliveryError::new(
                "unknown_check",
                format!(
                    "«{check}» no es una de las comprobaciones del repositorio (read_settings: checks); elige entre ellas tal cual"
                ),
            )
        })?;
        task.updated_at_ms = now_ms();
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "task_state_set",
            Some(&task.id),
            None,
            &format!("comprobaciones: {}", task.checks.join("; ")),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    /// Whether the task's PR needs CI, as the coordinator judged its change.
    pub fn set_task_needs_ci(
        &self,
        task_id: &str,
        needs_ci: bool,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        task.skip_ci = !needs_ci;
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "task_state_set",
            Some(&task.id),
            None,
            if needs_ci { "necesita CI" } else { "sin CI" },
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    /// The commands that prepare the task's worktree, each one of the repo's
    /// setup commands as the user wrote it: Tinto runs them outside any
    /// agent sandbox. It queues at once, ahead of anything dispatched
    /// later, each one that has not passed nor is queued or running. One the
    /// task's bootstrap runs too (one of its `&&` parts) is left out: the
    /// worktree has it already, or will.
    pub fn set_task_setup(
        &self,
        task_id: &str,
        setup: Vec<String>,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let jobs = {
            let store = self.store()?;
            let allowed = store.repo_settings(&task.repo)?.setup;
            let picked = pick_commands(&setup, &allowed).map_err(|command| {
                DeliveryError::new(
                    "unknown_setup",
                    format!(
                        "«{command}» no es uno de los comandos de preparación del repositorio (read_settings: setup); elige entre ellos tal cual"
                    ),
                )
            })?;
            let bootstrap = store.task_jobs(&task.id)?.into_iter().rev().find(|job| {
                job.role == "bootstrap"
                    && (job.status.is_open() || job.status == DeliveryJobStatus::Finished)
            });
            let bootstrapped: Vec<String> = bootstrap.map_or(Vec::new(), |job| {
                job.prompt
                    .split("&&")
                    .map(|part| part.trim().to_string())
                    .collect()
            });
            task.setup = picked
                .into_iter()
                .filter(|command| !bootstrapped.contains(command))
                .collect();
            task.updated_at_ms = now_ms();
            store.put_task(&task)?;
            store.record_event(
                task.updated_at_ms,
                "task_state_set",
                Some(&task.id),
                None,
                &format!("preparación: {}", task.setup.join("; ")),
            )?;
            store.task_jobs(&task.id)?
        };
        let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
        let current = current_setup(&task.setup, &jobs);
        for command in &task.setup {
            let settled = current.iter().any(|job| {
                &job.prompt == command
                    && (job.status.is_open() || job.status == DeliveryJobStatus::Finished)
            });
            if settled {
                continue;
            }
            self.dispatch(NewJob {
                task_id: task.id.clone(),
                role: SETUP_ROLE.to_string(),
                agent: DeliveryAgent::Shell,
                model: None,
                access: DeliveryAccess::Workspace,
                prompt: command.clone(),
                writes: None,
                lease: None,
                timeout_minutes: None,
                reasoning: None,
                platform: None,
                read_dirs: Vec::new(),
                stage_default: false,
                needs_environment: false,
            })?;
        }
        // Stages a dropped command held can start.
        self.tick();
        Ok(task)
    }

    /// Replaces the folders every job of the task may read besides its
    /// worktree. The coordinator's own edit: it does not wake it.
    pub fn set_task_read_dirs(
        &self,
        task_id: &str,
        dirs: &[String],
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        task.read_dirs = readable_dirs(dirs, &task.repo, &task.worktree, task.distro.as_deref())?;
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "task_state_set",
            Some(&task.id),
            None,
            &format!("carpetas de lectura: {}", task.read_dirs.join(", ")),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    /// Takes a package off the one it builds on, when it turns out not to
    /// need its code: it starts again from the base that chain started from,
    /// and stops waiting for it. Only while it has nothing of its own (no
    /// commit, no change, nothing published), so nothing is lost.
    pub fn detach_package(&self, task_id: &str) -> Result<DeliveryTask, DeliveryError> {
        let refused = |message: String| DeliveryError::new("package_detach_refused", message);
        let mut task = self.task(task_id)?;
        let Some(mut package) = task.package.clone() else {
            return Err(refused("la tarea no es un paquete".into()));
        };
        let Some(other_id) = package.builds_on.clone() else {
            return Err(refused("el paquete ya parte de la base, no de otro".into()));
        };
        if self
            .store()?
            .task_jobs(task_id)?
            .iter()
            .any(|job| job.status.is_open())
        {
            return Err(refused(
                "tiene trabajos en curso: espera a que terminen o cancélalos".into(),
            ));
        }
        let place = Place::of(task.distro.as_deref());
        let head = tasks::head(place, &task.worktree)?;
        let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
        let other = self.task(&other_id)?;
        if state.dirty || head != task.base_commit {
            return Err(refused(format!(
                "ya tiene trabajo propio sobre la rama de {}: rebásalo a mano sobre la base",
                other.key
            )));
        }
        if state.has_upstream || self.task_pr(task_id)?.is_some() {
            return Err(refused("su rama ya está publicada".into()));
        }
        // The base the chain started from: up the packages, each on the one
        // before, to the first that started elsewhere.
        let mut first = other.clone();
        while let Some(previous) = first.package.as_ref().and_then(|p| p.builds_on.clone()) {
            let previous = self.task(&previous)?;
            if first.base_ref != previous.branch {
                break;
            }
            first = previous;
        }
        let (base_ref, commit) =
            tasks::resolve_base(place, &task.repo, Some(&first.base_ref), None)?;
        tasks::git(place, &task.worktree, &["reset", "-q", "--hard", &commit])?;
        task.base_ref = base_ref;
        task.base_commit = commit;
        package.builds_on = None;
        task.package = Some(package);
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "package_detached",
            Some(&task.id),
            None,
            &format!("ya no espera a {}: parte de {}", other.key, task.base_ref),
        )?;
        drop(store);
        let waits: Vec<String> = task
            .blockers
            .iter()
            .filter(|blocker| {
                blocker.is_open()
                    && matches!(&blocker.kind, super::model::DeliveryBlockerKind::Task { task_id, .. } if *task_id == other.id)
            })
            .map(|blocker| blocker.id.clone())
            .collect();
        let task = if waits.is_empty() {
            task
        } else {
            self.clear_blockers(&task.id, Some(&waits), false)?
        };
        self.notify();
        Ok(task)
    }

    /// Makes a task created as a whole issue one package of it, so the
    /// issue's other packages can follow. It keeps its worktree and base.
    pub fn set_task_package(
        &self,
        task_id: &str,
        package: &NewPackage,
    ) -> Result<DeliveryTask, DeliveryError> {
        validate_package(package)?;
        let invalid = |message: String| DeliveryError::new("invalid_package", message);
        if package.builds_on.is_some() || package.until.is_some() {
            return Err(invalid(
                "una tarea que ya existe conserva su base: package solo admite index, of y title"
                    .into(),
            ));
        }
        let mut task = self.task(task_id)?;
        if let Some(existing) = &task.package {
            return Err(invalid(format!(
                "{} ya es el paquete {} de {}",
                task.key, existing.index, existing.of
            )));
        }
        // A task without a package is the only one of its issue.
        task.package = Some(DeliveryPackage {
            index: package.index,
            of: package.of,
            title: package.title.trim().to_string(),
            builds_on: None,
        });
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "task_state_set",
            Some(&task.id),
            None,
            &format!("paquete {}/{}", package.index, package.of),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    // ---- jobs ----

    pub fn dispatch(&self, request: NewJob) -> Result<DeliveryJob, DeliveryError> {
        self.dispatch_job(request, None, None)
    }

    /// `resumes` is the paused job the new one continues in its session;
    /// `retry_of` the interrupted job it repeats after a restart.
    fn dispatch_job(
        &self,
        mut request: NewJob,
        resumes: Option<&DeliveryJob>,
        retry_of: Option<&str>,
    ) -> Result<DeliveryJob, DeliveryError> {
        let task = self.task(&request.task_id)?;
        // On a Windows repo agents run as installed on Windows, never in WSL
        // unless asked: an agent that is not installed gives way to the
        // other, with that agent's own model.
        let mut substituted = None;
        // A platform asked for (Windows or WSL) keeps the agent asked for.
        let local = task.distro.is_none() && request.platform.is_none();
        if local && request.agent != DeliveryAgent::Shell && resumes.is_none() {
            let other = match request.agent {
                DeliveryAgent::Codex => DeliveryAgent::Claude,
                _ => DeliveryAgent::Codex,
            };
            if !self.is_installed(request.agent) {
                if !self.is_installed(other) {
                    return Err(DeliveryError::new(
                        "binary_not_found",
                        "ni Codex ni Claude Code están instalados en este equipo",
                    ));
                }
                substituted = Some(format!(
                    "{} no está instalado en este equipo: corrió con {}.",
                    agent_name(request.agent),
                    agent_name(other)
                ));
                request.agent = other;
                request.model = None;
                // Only the levels both agents know carry over.
                request.reasoning = request
                    .reasoning
                    .filter(|effort| matches!(effort.trim(), "low" | "medium" | "high" | "xhigh"));
            }
        }
        // A paused stage holds its task: nothing else runs on it until the
        // user resumes or cancels it.
        if let Some(paused) = self.paused_job(&task.id)? {
            if resumes.map(|job| &job.id) != Some(&paused.id) {
                return Err(DeliveryError::new(
                    "job_paused",
                    format!(
                        "{} está en pausa: el usuario la reanuda o la cancela en Tinto",
                        paused.role
                    ),
                ));
            }
        }
        // A blocked task waits for its blockers before any stage; its setup
        // can run meanwhile.
        let setup = request.agent == DeliveryAgent::Shell && request.role == SETUP_ROLE;
        let open: Vec<String> = task
            .blockers
            .iter()
            .filter(|_| !setup)
            .filter(|blocker| blocker.is_open())
            .map(super::blockers::blocker_label)
            .collect();
        if !open.is_empty() {
            return Err(DeliveryError::new(
                "task_blocked",
                format!(
                    "{} está bloqueada por {}: quita el bloqueo antes de lanzar una etapa",
                    task.key,
                    open.join(", ")
                ),
            ));
        }
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
        // Read before the store is held: it reads the store itself.
        let qa_followup = if role == QA_ROLE {
            self.qa_followup(&task.id)?
        } else {
            Vec::new()
        };
        let shared_decisions = self.package_decisions(&task.id)?;
        if request.agent != DeliveryAgent::Shell {
            self.check_merge_stage(&task, &role)?;
        }
        // A sync stopped on conflicts: the job is told which files to resolve.
        let merge_conflicts = match request.agent {
            DeliveryAgent::Shell => None,
            _ => self.merge_conflicts(&task)?,
        };
        // A writing stage on a PR with news answers them in a round, which
        // the stages dispatched in it are told about.
        let store = self.store()?;
        let round = match request.agent {
            DeliveryAgent::Shell => Vec::new(),
            _ => {
                if request.writes.unwrap_or(writes) {
                    super::rounds::for_pr(&store, &task.id)?;
                }
                super::rounds::note(&store, &task.id)?
            }
        };
        let model = match request.model.filter(|model| !model.trim().is_empty()) {
            None if request.agent == DeliveryAgent::Codex => store
                .setting::<Option<String>>(CODEX_MODEL_SETTING)?
                .flatten(),
            model => model,
        };
        if let Some(model) = &model {
            let refused: Vec<String> = store.setting(REFUSED_MODELS_SETTING)?.unwrap_or_default();
            if refused.contains(&refused_model(request.agent, model)) {
                return Err(DeliveryError::new(
                    "model_unavailable",
                    format!(
                        "la cuenta de {} rechazó el modelo «{model}» en un trabajo anterior: elige otro en Ajustes o en el despacho",
                        agent_name(request.agent)
                    ),
                ));
            }
        }
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
        // The time limit given, else the stage's, else the role's. Resolving
        // a merge's conflicts takes longer than a stage's usual work.
        let timeout = request
            .timeout_minutes
            .or_else(|| settings.stages.get(&role)?.timeout_minutes)
            .unwrap_or(match merge_conflicts {
                Some(_) => timeout.max(MERGE_TIMEOUT_MINUTES),
                None => timeout,
            });
        // The checks the job is given. Codex's sandbox must also be
        // able to start their programs; OpenCode's rules allow them.
        let allowed_commands = match (request.agent, request.access) {
            (
                DeliveryAgent::Claude | DeliveryAgent::Codex | DeliveryAgent::Opencode,
                DeliveryAccess::Workspace,
            ) => {
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
        // The task's and the coordinator's, such as the issue's planning
        // folder in the main checkout. A retry passes them again (and the
        // permissions'): already there.
        for dir in task.read_dirs.iter().chain(&request.read_dirs) {
            if !read_dirs.contains(dir) {
                read_dirs.push(readable_dir(
                    dir,
                    &task.repo,
                    &task.worktree,
                    task.distro.as_deref(),
                )?);
            }
        }
        // `{task}` gives each task its own value, such as a test database,
        // and `{job}` each job (a check run, an agent's session): two of
        // the task's jobs running at once never share it.
        let id = uuid::Uuid::new_v4().to_string();
        let slug = task_slug(&task.key);
        let check_env = settings
            .check_env
            .into_iter()
            .map(|(name, value)| {
                let value = value.replace("{task}", &slug).replace("{job}", &id[..8]);
                (name, value)
            })
            .collect();
        let mut decision_notes: Vec<String> = decisions
            .iter()
            .filter_map(decisions::decision_note)
            .collect();
        // The issue's product choices and approved texts hold for each of
        // its packages; permissions stay with the package they were asked for.
        for (package, decision) in &shared_decisions {
            if let Some(note) = decisions::decision_note(decision) {
                decision_notes.push(format!("{note} (decided for {package})"));
            }
        }
        // A QA job holds the resource of the environment it tests on: a
        // command-line QA the coordinator gives its own `qa:cli:<project>`
        // runs beside a browser QA on the repo's stack, without the browser.
        let lease = request
            .lease
            .filter(|lease| !lease.trim().is_empty())
            .or_else(|| match (&settings.qa_stack, qa && settings.qa_browser) {
                (Some(stack), true) => Some(format!("{QA_BROWSER_LEASE}{}", stack.name)),
                _ => lease.map(str::to_string),
            });
        let command_line_qa = lease
            .as_deref()
            .is_some_and(|lease| lease.starts_with(QA_CLI_LEASE));
        let qa_browser = qa && settings.qa_browser && !command_line_qa;
        let qa_environment = if qa {
            settings.qa_environment.clone()
        } else {
            String::new()
        };
        let qa_stack = settings.qa_stack.clone().filter(|_| qa_browser);
        // Checks and setup run commands, not an agent: no rules to follow.
        let guide_rules = if matches!(role.as_str(), CHECKS_ROLE | SETUP_ROLE) {
            Vec::new()
        } else {
            super::guides::rule_notes(&store.guide(&task.repo)?.rules, &role)
        };
        // Each check and setup command counts its own attempts.
        let attempt = store
            .task_jobs(&task.id)?
            .iter()
            .filter(|job| {
                job.role == role
                    && (!matches!(role.as_str(), CHECKS_ROLE | SETUP_ROLE)
                        || job.prompt == request.prompt)
            })
            .map(|job| job.attempt)
            .max()
            .unwrap_or(0)
            + 1;
        let now = now_ms();
        let job = DeliveryJob {
            id,
            task_id: task.id.clone(),
            role,
            attempt,
            agent: request.agent,
            model,
            session_model: None,
            access: request.access,
            prompt: request.prompt,
            contract_version: task.contract_version,
            writes: request.writes.unwrap_or(writes),
            lease,
            timeout_minutes: timeout.clamp(1, MAX_TIMEOUT_MINUTES),
            status: DeliveryJobStatus::Queued,
            created_at_ms: now,
            started_at_ms: None,
            ended_at_ms: None,
            pid: None,
            process_started: None,
            // A resumed job goes on in the paused job's session.
            provider_session_id: resumes.and_then(|paused| paused.provider_session_id.clone()),
            exit_code: None,
            error: None,
            start_candidate: None,
            end_candidate: None,
            changes: Vec::new(),
            result: None,
            result_state: None,
            result_note: None,
            undone_at_ms: None,
            resolved_at_ms: None,
            resolved_note: None,
            allowed_commands,
            qa_browser,
            decisions: decision_notes,
            qa_followup,
            qa_environment,
            qa_stack,
            prepare: None,
            reverted: Vec::new(),
            task_checks: Vec::new(),
            round,
            guide_rules,
            needs_environment: request.needs_environment || qa,
            waits_environment: None,
            reasoning,
            platform: request.platform,
            ran_on: None,
            read_dirs,
            stage_default: request.stage_default,
            check_env,
            write_dirs,
            frozen_scripts: Vec::new(),
            resumes: resumes.map(|paused| paused.id.clone()),
            resume_note: resumes.and_then(continue_note),
            merge_conflicts,
            activity: None,
            activity_at_ms: None,
            current_command: None,
            current_command_at_ms: None,
            timing: None,
            plan: Vec::new(),
            usage: None,
            visual_record: None,
            visual_record_error: None,
            visual_record_source: None,
            visual_record_extras: Vec::new(),
            visual_record_warnings: Vec::new(),
            substituted,
            retry_of: retry_of.map(str::to_string),
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
        if job.role == IMPLEMENTATION_ROLE {
            self.end_brainstorm_once_implementing(&task)?;
        }
        self.tick();
        Ok(job)
    }

    /// Once every task of the run is being implemented, or waits for what
    /// blocks it (another task, a package before it), the brainstorm is
    /// over even if the coordinator did not say so: from then on the run
    /// does not wait for the user (commits, pushes, decisions). It stays
    /// open while a decision or settings proposal of the run is pending.
    fn end_brainstorm_once_implementing(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let Some(run_id) = task.run_id.as_deref() else {
            return Ok(());
        };
        if self.run(run_id)?.unattended {
            return Ok(());
        }
        let store = self.store()?;
        let jobs = store.jobs()?;
        let all_implementing = store
            .tasks()?
            .iter()
            .filter(|other| other.run_id.as_deref() == Some(run_id))
            .all(|other| {
                other.blockers.iter().any(DeliveryBlocker::is_open)
                    || jobs
                        .iter()
                        .any(|job| job.task_id == other.id && job.role == IMPLEMENTATION_ROLE)
            });
        drop(store);
        if all_implementing {
            // Something still waits on the user: the coordinator ends it.
            let _ = self.end_brainstorm(run_id);
        }
        Ok(())
    }

    /// The task's paused job that no later job resumes, if any.
    fn paused_job(&self, task_id: &str) -> Result<Option<DeliveryJob>, DeliveryError> {
        let jobs = self.store()?.task_jobs(task_id)?;
        let resumed: HashSet<&str> = jobs
            .iter()
            .filter_map(|job| job.resumes.as_deref())
            .collect();
        Ok(jobs
            .iter()
            .find(|job| {
                job.status == DeliveryJobStatus::Paused && !resumed.contains(job.id.as_str())
            })
            .cloned())
    }

    /// Stops a running agent to continue later in the same session. Its
    /// changes stay, and so does the resource it holds.
    pub fn pause(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let job = self.job(job_id)?;
        if job.agent == DeliveryAgent::Shell || !job.status.is_active() {
            return Err(DeliveryError::new(
                "job_not_pausable",
                "solo se pausa un agente en marcha",
            ));
        }
        if let Ok(mut pausing) = self.inner.pausing.lock() {
            pausing.insert(job.id.clone());
        }
        self.cancel(job_id)
    }

    /// Continues a paused job in its agent session, as a new attempt that
    /// takes over its resource.
    pub fn resume(&self, job_id: &str) -> Result<DeliveryJob, DeliveryError> {
        let paused = self.job(job_id)?;
        if self.paused_job(&paused.task_id)?.map(|job| job.id) != Some(paused.id.clone()) {
            return Err(DeliveryError::new(
                "job_not_paused",
                "ese trabajo no está en pausa",
            ));
        }
        if paused.provider_session_id.is_none() {
            return Err(DeliveryError::new(
                "session_unknown",
                "no se sabe la sesión del agente; cancélalo y lanza la etapa de nuevo",
            ));
        }
        self.dispatch_job(in_same_session(&paused), Some(&paused), None)
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
                settle_lease(&store, &job, false, &[])?;
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
            // Giving up a pause frees its task; its resource is settled as
            // that of a job that ran.
            DeliveryJobStatus::Paused
                if self.paused_job(&job.task_id)?.map(|paused| paused.id)
                    == Some(job.id.clone()) =>
            {
                let store = self.store()?;
                job.status = DeliveryJobStatus::Cancelled;
                job.error = Some("Pausado y cancelado por el usuario.".to_string());
                store.put_job(&job)?;
                settle_lease(&store, &job, true, &[])?;
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
            _ => Err(DeliveryError::new("job_not_open", "el trabajo ya terminó")),
        }
    }

    /// A new attempt with the same assignment, on the task's current
    /// contract version, and with another time limit if given.
    pub fn retry(
        &self,
        job_id: &str,
        timeout_minutes: Option<u32>,
    ) -> Result<DeliveryJob, DeliveryError> {
        let job = self.job(job_id)?;
        if job.status.is_open() {
            return Err(DeliveryError::new(
                "job_open",
                "el trabajo todavía no terminó",
            ));
        }
        let mut assignment = self.same_assignment(job)?;
        if timeout_minutes.is_some() {
            assignment.timeout_minutes = timeout_minutes;
        }
        self.dispatch(assignment)
    }

    /// Why retrying `job` will fail the same way, if it will: a check that
    /// failed in under a second (a missing file, a submodule never checked
    /// out) with no code, setup or task settings changed since it started.
    pub fn futile_retry(&self, job: &DeliveryJob) -> Result<Option<String>, DeliveryError> {
        let fast = matches!(
            (job.started_at_ms, job.ended_at_ms),
            (Some(start), Some(end)) if end.saturating_sub(start) < 1_000
        );
        if job.agent != DeliveryAgent::Shell
            || job.role != CHECKS_ROLE
            || job.status != DeliveryJobStatus::Failed
            || !fast
        {
            return Ok(None);
        }
        let since = job.started_at_ms.unwrap_or(job.created_at_ms);
        if self.task(&job.task_id)?.updated_at_ms > since {
            return Ok(None);
        }
        let changed = self.store()?.task_jobs(&job.task_id)?.iter().any(|other| {
            other.created_at_ms > job.created_at_ms
                && (other.role == SETUP_ROLE
                    || (other.agent != DeliveryAgent::Shell
                        && other.undone_at_ms.is_none()
                        && !other.changes.is_empty()))
        });
        Ok((!changed).then(|| {
            format!(
                "`{}` falló en menos de un segundo ({}) y desde entonces no cambió el código, el setup ni los ajustes de la tarea: el reintento fallará igual. Arregla antes la causa (una dependencia o un submódulo que falta, un setup nuevo con set_task_state o propose_settings).",
                job.prompt,
                job.error.as_deref().unwrap_or("sin mensaje")
            )
        }))
    }

    /// Goes on with a job that ran out of time or that a restart cut off, in
    /// its agent session: the agent is told why it stopped and keeps what it
    /// left in the worktree. With another time limit if given.
    pub fn continue_job(
        &self,
        job_id: &str,
        timeout_minutes: Option<u32>,
    ) -> Result<DeliveryJob, DeliveryError> {
        let stopped = self.job(job_id)?;
        if continue_note(&stopped).is_none() {
            return Err(DeliveryError::new(
                "job_not_continuable",
                "solo se continúa un trabajo que se quedó sin tiempo o que Tinto interrumpió al cerrarse; para otro, usa retry_job",
            ));
        }
        let later = self
            .store()?
            .task_jobs(&stopped.task_id)?
            .iter()
            .any(|job| job.role == stopped.role && job.attempt > stopped.attempt);
        if later {
            return Err(DeliveryError::new(
                "job_superseded",
                format!(
                    "{} ya tiene un intento posterior: continúa ese",
                    stopped.role
                ),
            ));
        }
        if !self.session_started(&stopped) {
            return Err(DeliveryError::new(
                "session_unknown",
                "la sesión del agente no se puede reanudar (no llegó a empezar): usa retry_job",
            ));
        }
        let mut assignment = in_same_session(&stopped);
        if timeout_minutes.is_some() {
            assignment.timeout_minutes = timeout_minutes;
        }
        self.dispatch_job(assignment, Some(&stopped), None)
    }

    /// Whether the job's agent session can be resumed: its session exists
    /// only once the agent started it, as its own output says.
    fn session_started(&self, job: &DeliveryJob) -> bool {
        job.provider_session_id.is_some()
            && (self.inner.paths_for)(&job.id)
                .ok()
                .and_then(|paths| std::fs::read_to_string(paths.events).ok())
                .is_some_and(|events| {
                    events
                        .lines()
                        .any(|line| adapters::session_id_from_line(job.agent, line).is_some())
                })
    }

    /// A job's assignment again, for a retry. A job on the stage's settings
    /// follows them as they are now: the user may have changed a model that
    /// failed.
    fn same_assignment(&self, job: DeliveryJob) -> Result<NewJob, DeliveryError> {
        // A stage the user configured runs as it is configured now, whatever
        // the attempt being repeated ran with: a model changed in Ajustes
        // applies to the next attempt.
        let (mut agent, mut model, mut reasoning) = (job.agent, job.model, job.reasoning);
        let repo = self.task(&job.task_id)?.repo;
        let stage = self.store()?.repo_settings(&repo)?.stages.remove(&job.role);
        let follows_stage = stage.is_some() || job.stage_default;
        if let Some(stage) = stage {
            (agent, model, reasoning) = (stage.agent, stage.model, stage.reasoning);
        }
        Ok(NewJob {
            task_id: job.task_id,
            role: job.role,
            agent,
            model,
            access: job.access,
            prompt: job.prompt,
            writes: Some(job.writes),
            lease: job.lease,
            timeout_minutes: Some(job.timeout_minutes),
            reasoning,
            platform: job.platform,
            read_dirs: job.read_dirs,
            stage_default: follows_stage,
            needs_environment: job.needs_environment,
        })
    }

    /// "Reintentar al reiniciar": queues again, once, each job the restart
    /// interrupted that is safe to repeat, as a retry of it. Called once the
    /// app is wired; it waits for what a WSL job left to stop first. The
    /// coordinator hears of each retry, or why it could not be queued.
    pub fn retry_interrupted(&self) {
        let retries = self
            .inner
            .restart_retries
            .lock()
            .map(|mut slot| std::mem::take(&mut *slot))
            .unwrap_or_default();
        for (job_id, wsl_job) in retries {
            if let Some(wsl_job) = wsl_job {
                adapters::stop_wsl_job(&wsl_job, "KILL");
            }
            if let Err(error) = self.retry_after_restart(&job_id, "al reiniciar Tinto") {
                eprintln!("tinto: delivery could not retry job {job_id}: {error}");
            }
        }
    }

    /// Queues a stopped job again on its own, `when` saying why in its note
    /// ("al reiniciar Tinto").
    pub(super) fn retry_after_restart(
        &self,
        job_id: &str,
        when: &str,
    ) -> Result<(), DeliveryError> {
        let mut job = self.job(job_id)?;
        // An agent that had started goes on in its session, keeping what it
        // did; anything else starts over.
        let queued = if continue_note(&job).is_some() && self.session_started(&job) {
            self.dispatch_job(in_same_session(&job), Some(&job), Some(&job.id))
        } else {
            self.same_assignment(job.clone())
                .and_then(|assignment| self.dispatch_job(assignment, None, Some(&job.id)))
        };
        let store = self.store()?;
        let note = match &queued {
            Ok(retry) => {
                store.record_event(
                    now_ms(),
                    "job_retried",
                    Some(&retry.task_id),
                    Some(&retry.id),
                    &job.id,
                )?;
                format!("Se reintentó {when}.")
            }
            Err(error) => {
                store.record_event(
                    now_ms(),
                    "job_not_retried",
                    Some(&job.task_id),
                    Some(&job.id),
                    &error.message,
                )?;
                format!("No se pudo reintentar {when}: {}.", error.message)
            }
        };
        job.error = Some(
            format!("{} {note}", job.error.unwrap_or_default())
                .trim()
                .to_string(),
        );
        store.put_job(&job)?;
        drop(store);
        self.notify();
        Ok(())
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

    /// Settles the task's broken stage without running it again, because
    /// its work was done another way (another session finished it, the
    /// coordinator fixed it outside Tinto), or accepts a failing check of
    /// Tinto on the current code. The note, one line, says how; the attempt
    /// then counts as passed, and a newer one replaces it as usual. `by` is
    /// `user` or `coordinator`.
    pub fn resolve(
        &self,
        job_id: &str,
        note: &str,
        by: &str,
    ) -> Result<DeliveryJob, DeliveryError> {
        let note = resolve_note(note)?;
        let store = self.store()?;
        let mut job = store
            .job(job_id)?
            .ok_or_else(|| DeliveryError::not_found("el trabajo", job_id))?;
        let jobs = store.task_jobs(&job.task_id)?;
        let job_refs: Vec<&DeliveryJob> = jobs.iter().collect();
        if job.agent == DeliveryAgent::Shell {
            if job.role != CHECKS_ROLE {
                return Err(DeliveryError::new(
                    "not_resolvable_role",
                    "solo se dan por resueltas las etapas de agente y las comprobaciones de Tinto; un comando de setup se arregla y se reintenta con retry_job",
                ));
            }
            if !check_fails(&job) {
                return Err(DeliveryError::new(
                    "already_passed",
                    "la comprobación no falló o ya está resuelta: no hay nada que aceptar",
                ));
            }
            let checks = store
                .task(&job.task_id)?
                .map(|task| task.checks)
                .unwrap_or_default();
            if !current_checks(&checks, &job_refs)
                .iter()
                .any(|check| check.id == job.id)
            {
                return Err(DeliveryError::new(
                    "not_latest",
                    "no es la última ejecución de esa comprobación sobre el código actual, o la comprobación ya no está entre las de la tarea (entonces no bloquea nada)",
                ));
            }
        } else {
            // A stage that broke, or one whose findings are accepted as
            // they are.
            if !(is_broken(&job) || has_open_findings(&job)) {
                return Err(DeliveryError::new(
                    "already_passed",
                    "la etapa pasó o ya está resuelta: solo se resuelve una que falló, se bloqueó, se interrumpió o dejó hallazgos",
                ));
            }
            let latest = jobs.iter().rev().find(|other| {
                other.agent != DeliveryAgent::Shell
                    && other.result_state != Some(DeliveryResultState::Stale)
                    && other.undone_at_ms.is_none()
            });
            if let Some(latest) = latest.filter(|latest| latest.id != job.id) {
                return Err(DeliveryError::new(
                    "not_latest",
                    format!(
                        "no es la última etapa de la tarea: la última es {} ({})",
                        latest.role, latest.id
                    ),
                ));
            }
        }
        if let Some(open) = jobs.iter().find(|other| other.status.is_open()) {
            return Err(DeliveryError::new(
                "jobs_open",
                format!(
                    "esta tarea tiene un trabajo abierto, {} ({}): espera a que termine o cancélalo",
                    open.role, open.id
                ),
            ));
        }
        job.resolved_at_ms = Some(now_ms());
        job.resolved_note = Some(note.clone());
        store.put_job(&job)?;
        let kind = match by {
            "user" => "job_resolved_by_user",
            _ => "job_resolved",
        };
        store.record_event(now_ms(), kind, Some(&job.task_id), Some(&job.id), &note)?;
        super::rounds::settle_round(&store, &job.task_id)?;
        drop(store);
        self.notify();
        Ok(job)
    }

    /// Closes the task's open round without more work: what it had to
    /// address was settled another way (answered with a reply, a check that
    /// passed when run again with nothing to publish). The note, one line,
    /// is its outcome.
    pub fn resolve_round(
        &self,
        task_id: &str,
        note: &str,
        by: &str,
    ) -> Result<super::model::DeliveryRound, DeliveryError> {
        let note = resolve_note(note)?;
        let store = self.store()?;
        let mut round = super::rounds::open_of(&store, task_id)?.ok_or_else(|| {
            DeliveryError::new("no_open_round", "la tarea no tiene una ronda abierta")
        })?;
        if let Some(open) = store
            .task_jobs(task_id)?
            .into_iter()
            .find(|job| job.status.is_open())
        {
            return Err(DeliveryError::new(
                "jobs_open",
                format!(
                    "esta tarea tiene un trabajo abierto, {} ({}): espera a que termine o cancélalo",
                    open.role, open.id
                ),
            ));
        }
        round.closed_at_ms = Some(now_ms());
        round.outcome = Some(format!("resuelta por {by}: {note}"));
        store.put_round(&round)?;
        store.record_event(
            now_ms(),
            "round_closed",
            Some(task_id),
            None,
            &format!(
                "{} — {}",
                super::rounds::round_text(&round),
                round.outcome.as_deref().unwrap_or_default()
            ),
        )?;
        drop(store);
        self.notify();
        Ok(round)
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

    /// Frees a quarantined resource. `released_by` names who checked the
    /// environment: the user, or a coordinator past its run's brainstorm.
    pub fn release_lease(
        &self,
        name: &str,
        note: &str,
        released_by: &str,
    ) -> Result<DeliveryLease, DeliveryError> {
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
        lease.note = Some(format!("Liberado por {released_by}: {}", note.trim()));
        store.put_lease(&lease)?;
        // What whoever released it found and did, for the user to read.
        store.record_event(
            now_ms(),
            "lease_released",
            None,
            None,
            &format!("{name}: {}", lease.note.as_deref().unwrap_or_default()),
        )?;
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
        self.check_due_environments();
        self.notify();
    }

    fn select_jobs(&self) -> Result<Vec<String>, DeliveryError> {
        let store = self.store()?;
        let mut queue = Queue::read(&store, self.environments())?;
        // A job that needs the environment waits while its last check
        // fails, and says why.
        let now = now_ms();
        let problems: Vec<Option<String>> = queue
            .jobs
            .iter()
            .map(|job| {
                queue
                    .environment_of(job)
                    .and_then(|state| state.problem.clone())
            })
            .collect();
        for (job, problem) in queue.jobs.iter_mut().zip(problems) {
            if job.status != DeliveryJobStatus::Queued
                || !job.needs_environment
                || job.waits_environment == problem
            {
                continue;
            }
            if job.waits_environment.is_none() {
                store.record_event(
                    now,
                    "job_waits_environment",
                    Some(&job.task_id),
                    Some(&job.id),
                    &job.role,
                )?;
            }
            job.waits_environment = problem;
            store.put_job(job)?;
        }
        let (plan, leases) = queue.plan(now);
        for lease in leases.iter().filter(|lease| !queue.leases.contains(lease)) {
            store.put_lease(lease)?;
        }
        let mut started = Vec::new();
        for (job_id, hold) in plan {
            if hold.is_some() {
                continue;
            }
            let Some(job) = queue.jobs.iter_mut().find(|job| job.id == job_id) else {
                continue;
            };
            job.status = DeliveryJobStatus::Pending;
            store.put_job(job)?;
            if let Ok(mut cancels) = self.inner.cancels.lock() {
                cancels.insert(job.id.clone(), Arc::new(AtomicBool::new(false)));
            }
            started.push(job_id);
        }
        Ok(started)
    }

    /// Each repo's last "Entorno" check.
    pub(super) fn environments(&self) -> HashMap<PathBuf, RepoEnvironment> {
        self.inner
            .environment
            .lock()
            .map(|repos| repos.clone())
            .unwrap_or_default()
    }

    // ---- running a job ----

    fn run_job(&self, job_id: &str) {
        self.end_job(job_id, self.run_job_inner(job_id));
    }

    /// What follows a job's run, however it went: the next jobs start.
    fn end_job(&self, job_id: &str, outcome: Result<(), DeliveryError>) {
        if self.inner.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = outcome {
            let _ = self.finish_failed(job_id, &error.message);
        }
        if let Ok(mut cancels) = self.inner.cancels.lock() {
            cancels.remove(job_id);
        }
        // The branch as it is now: a push that drifted may match again.
        if let Ok(task) = self.job(job_id).and_then(|job| self.task(&job.task_id)) {
            if let Err(error) = self.check_push(&task) {
                eprintln!(
                    "tinto: delivery could not check {}'s push: {error}",
                    task.key
                );
            }
        }
        self.tick();
    }

    fn run_job_inner(&self, job_id: &str) -> Result<(), DeliveryError> {
        let mut job = self.job(job_id)?;
        let task = self.task(&job.task_id)?;
        let task = if job.agent == DeliveryAgent::Shell {
            task
        } else {
            self.catch_up_package(task)?
        };
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
            job.task_checks = self.checks_note(&task, &job)?;
            self.store()?.put_job(&job)?;
        }
        let paths = (self.inner.paths_for)(&job.id)?;
        paths.prepare()?;
        // Submodules a sync, a restack or a checkout brought in and the
        // worktree never checked out: every check would fail without them.
        let submodules =
            tasks::init_submodules(Place::of(task.distro.as_deref()), &task.worktree, false)?;
        if let Some(note) = &submodules {
            job.prepare = Some(DeliveryQaPrepare {
                note: note.clone(),
                ..Default::default()
            });
        }
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
        // Browser QA on the repo's stack starts once Tinto has it serving
        // this worktree; its time limit counts from then.
        if let Some(stack) = job.qa_stack.clone() {
            match self.prepare_qa_stack(&job, &task, &stack, &paths, start.as_ref()) {
                Ok(mut prepare) => {
                    if let Some(note) = &submodules {
                        prepare.note = format!("{note}\n{}", prepare.note);
                    }
                    job.prepare = Some(prepare)
                }
                Err(PrepareStop::Cancelled) => return self.cancel_preparing(&job.id),
                Err(PrepareStop::Failed(message)) => {
                    return Err(DeliveryError::new(
                        "qa_prepare_failed",
                        format!(
                            "No se pudo preparar el entorno de QA «{}»: {message}. Detalle en {}.",
                            stack.name,
                            plain_path(&paths.dir.join(super::qa_prepare::PREPARE_LOG))
                        ),
                    ))
                }
            }
        }
        let launch = (self.inner.launcher)(&job, &task, &paths)?;
        // The process writes its output straight to the job's files and
        // reads its input from one, so it outlives Tinto: a restart adopts
        // it (see `reconcile`).
        let stdin_path = paths.dir.join(STDIN_FILE);
        std::fs::write(&stdin_path, &launch.stdin).map_err(DeliveryError::io)?;
        for (path, text) in &launch.files {
            std::fs::write(path, text).map_err(DeliveryError::io)?;
        }
        let mut command = Command::new(&launch.program);
        // The repo's variables for checks reach every command the agent
        // runs; a WSL job exports them in its script instead.
        command
            .args(&launch.args)
            .envs(&job.check_env)
            .envs(launch.env.iter().cloned())
            .stdin(std::fs::File::open(&stdin_path).map_err(DeliveryError::io)?)
            .stdout(std::fs::File::create(&paths.events).map_err(DeliveryError::io)?)
            .stderr(std::fs::File::create(&paths.stderr).map_err(DeliveryError::io)?);
        if let Some(cwd) = &launch.cwd {
            command.current_dir(cwd);
        }
        // In a group of its own, so stopping the job reaches what it starts:
        // `codex` installed with npm is a Node script that runs the binary.
        let process = JobProcess::spawn(&mut command, &job.id).map_err(|error| {
            DeliveryError::new(
                "spawn_failed",
                format!("no se pudo iniciar {}: {error}", launch.program.display()),
            )
        })?;
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
            current.pid = Some(process.pid());
            current.process_started = process.started();
            current.provider_session_id = launch.provider_session_id.clone();
            current.ran_on = Some(ran_on(&launch));
            current.frozen_scripts = job.frozen_scripts.clone();
            current.prepare = job.prepare.clone();
            current.current_command = None;
            current.current_command_at_ms = None;
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
            job = current;
        }
        self.notify();
        self.follow(job, task, paths, start, process, launch.wsl_job, false)
    }

    /// Follows a job that a restart found still running, or that ended
    /// while Tinto was closed, until it ends as any other.
    fn follow_adopted(&self, job_id: &str) -> Result<(), DeliveryError> {
        let job = self.job(job_id)?;
        let task = self.task(&job.task_id)?;
        let paths = (self.inner.paths_for)(&job.id)?;
        let start = self.store()?.start_snapshot(&job.id)?;
        let pid = job.pid.unwrap_or_default();
        let process = job
            .process_started
            .as_deref()
            .and_then(|started| JobProcess::adopt(pid, started, &job.id))
            .unwrap_or_else(|| JobProcess::ended(pid, &job.id));
        let wsl_job = std::fs::read(paths.dir.join(WSL_JOB_FILE))
            .ok()
            .and_then(|data| serde_json::from_slice::<adapters::WslJob>(&data).ok());
        self.follow(job, task, paths, start, process, wsl_job, true)
    }

    /// Reads a running job's output as it grows, stops it when it is
    /// cancelled or its time runs out, and evaluates it once it ends.
    #[allow(clippy::too_many_arguments)]
    fn follow(
        &self,
        job: DeliveryJob,
        task: DeliveryTask,
        paths: JobPaths,
        start: Option<WorktreeSnapshot>,
        mut process: JobProcess,
        wsl_job: Option<adapters::WslJob>,
        adopted: bool,
    ) -> Result<(), DeliveryError> {
        let agent = job.agent;
        let mut events = EventsTail::default();
        let mut clock = adapters::JobClock::default();
        let mut session_recorded = job.provider_session_id.is_some();
        // A Claude session says in its first event which model it runs,
        // so the view names it even when the job asked for the default.
        let mut model_recorded = agent != DeliveryAgent::Claude || job.session_model.is_some();
        // After a restart the first read takes in what the job wrote while
        // Tinto was closed; the command it runs keeps its start.
        let mut kept_command = adopted
            .then(|| job.current_command.clone().zip(job.current_command_at_ms))
            .flatten();
        let mut read_events = |lines: Vec<String>| {
            let mut seen = adapters::Observation::default();
            for line in lines {
                if !session_recorded {
                    if let Some(id) = adapters::session_id_from_line(agent, &line) {
                        session_recorded = true;
                        self.record_session_id(&job.id, id);
                    }
                }
                if let Some(quota) = adapters::rate_limit(agent, &line) {
                    self.record_quota(quota);
                }
                if !model_recorded {
                    if let Some(model) = adapters::claude_init_model(&line) {
                        model_recorded = true;
                        self.record_model(&job.id, model);
                    }
                }
                // Agents write one line per message or tool call: the view
                // always knows when the agent last did something, and
                // which command it waits on.
                let one = adapters::observe(agent, &line);
                seen.activity = one.activity.or(seen.activity.take());
                seen.plan = one.plan.or(seen.plan.take());
                seen.usage = match (agent, one.usage, seen.usage.take()) {
                    // OpenCode reports each step's tokens, not the total.
                    (DeliveryAgent::Opencode, Some(step), Some(before)) => Some(before.plus(&step)),
                    (_, one, before) => one.or(before),
                };
                if let Some(command) = clock.line(agent, &line, now_ms()) {
                    seen.command = Some(command);
                }
            }
            if let (Some(kept), Some(Some((command, at)))) =
                (kept_command.take(), &mut seen.command)
            {
                if kept.0 == *command {
                    *at = kept.1;
                }
            }
            if seen != adapters::Observation::default() {
                self.record_observation(&job.id, seen);
            }
        };

        let cancel = self
            .inner
            .cancels
            .lock()
            .ok()
            .and_then(|cancels| cancels.get(&job.id).cloned())
            .unwrap_or_default();
        let deadline_ms =
            job.started_at_ms.unwrap_or_else(now_ms) + u64::from(job.timeout_minutes) * 60_000;
        let mut cancelled = false;
        let mut timed_out = false;
        // When the job is being stopped: kill it outright once this passes.
        let mut kill_at: Option<Instant> = None;
        let mut give_up_at: Option<Instant> = None;
        let exited = loop {
            if self.inner.closed.load(Ordering::SeqCst) {
                return Ok(());
            }
            read_events(events.read(&paths.events));
            match process.try_wait() {
                Ok(Some(exited)) => break Some(exited),
                Ok(None) => {}
                Err(_) => break None,
            }
            if !cancelled && !timed_out {
                if cancel.load(Ordering::SeqCst) {
                    cancelled = true;
                } else if now_ms() >= deadline_ms {
                    timed_out = true;
                }
                if cancelled || timed_out {
                    // Closing `wsl.exe` would leave the Linux processes
                    // running, so a WSL job is asked to stop first.
                    kill_at = Some(match &wsl_job {
                        Some(wsl_job) => {
                            adapters::stop_wsl_job(wsl_job, "TERM");
                            Instant::now() + STOP_GRACE
                        }
                        None => Instant::now(),
                    });
                }
            }
            if kill_at.is_some_and(|at| Instant::now() >= at) {
                kill_at = Some(Instant::now() + KILL_AGAIN);
                give_up_at.get_or_insert(Instant::now() + KILL_GIVE_UP);
                if let Some(wsl_job) = &wsl_job {
                    adapters::stop_wsl_job(wsl_job, "KILL");
                }
                process.kill_tree();
            }
            // Killed and still there: it ends as stopped all the same.
            if give_up_at.is_some_and(|at| Instant::now() >= at) {
                break None;
            }
            std::thread::sleep(MONITOR_TICK);
        };
        // Whatever ended it, nothing it started keeps running: a server it
        // left in the background would hold the environment and its data.
        let mut process = process;
        let leftovers = process.sweep();
        drop(process);
        if self.inner.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        // A command adopted after a restart left its exit code in its
        // folder. One that left none (killed while Tinto was closed) did not
        // fail: it runs again instead of failing the task's checks.
        let exited = match exited {
            Some(job_process::Exited { success: None, .. })
                if agent == DeliveryAgent::Shell && !cancelled && !timed_out =>
            {
                match adapters::exit_code_left(&paths.dir) {
                    Some(code) => Some(job_process::Exited {
                        code: Some(code),
                        success: Some(code == 0),
                    }),
                    None => return self.interrupt_unknown_end(&job.id, &leftovers),
                }
            }
            exited => exited,
        };
        let mut last = events.read(&paths.events);
        last.extend(events.rest());
        read_events(last);
        let timing = (agent != DeliveryAgent::Shell).then(|| clock.timing(now_ms()));

        let events = std::fs::read_to_string(&paths.events).unwrap_or_default();
        let result_file = std::fs::read_to_string(&paths.result).ok();
        let outcome = adapters::read_outcome(agent, &events, result_file.as_deref());
        let mut end = match &start {
            Some(start) => {
                Some(self.snapshot(&task, &format!("job-{}-end", job.id), Some(&start.tree))?)
            }
            None => None,
        };
        // QA only reads the worktree: what it changed there (a lockfile a
        // build rewrote) goes back as it was, so it never reaches a commit.
        let mut reverted = Vec::new();
        if let (Some(start), Some(found)) = (&start, &end) {
            if job.role == QA_ROLE && !found.changes.is_empty() {
                for change in to_changes(found) {
                    match self.revert_file(&task, start, Path::new(&change.path)) {
                        Ok(()) => reverted.push(change.path),
                        Err(error) => eprintln!(
                            "tinto: delivery could not put back {} after QA: {error}",
                            change.path
                        ),
                    }
                }
                end = Some(self.snapshot(
                    &task,
                    &format!("job-{}-end-reverted", job.id),
                    Some(&start.tree),
                )?);
            }
        }
        // A command's exit code is its result; one Tinto could not learn
        // after a restart counts as failed. An agent's result says itself.
        let exit_ok = match exited {
            Some(exited) => exited.success.unwrap_or(agent != DeliveryAgent::Shell),
            None => false,
        };
        let ending = Ending {
            exit_ok,
            exit_code: exited.and_then(|exited| exited.code),
            cancelled,
            timed_out,
            // A job the user cancelled before it did more than read left
            // its resource as it was.
            touched: !cancelled || adapters::acted(&job, &events),
            leftovers,
        };
        // What the output says failed, should the job fail: a command's
        // stdout is its own output, an agent's is its events.
        let failure = super::failure_summary::summary(
            if agent == DeliveryAgent::Shell {
                &events
            } else {
                ""
            },
            &std::fs::read_to_string(&paths.stderr).unwrap_or_default(),
        );
        // QA's visual changes record becomes one page the user opens. Saved
        // before the job ends, so whoever learns it ended (the view, the
        // coordinator, a test) finds the page and its problems with it. A
        // job the user cancelled without writing one is not told it lacks it.
        // A QA that ended without its record, its time out or its process
        // killed, still leaves its evidence: Tinto writes a stub from its
        // screenshots and task list. It has no problems to read.
        let mut stub = false;
        if job.role == QA_ROLE
            && !cancelled
            && (timed_out || outcome.result.is_none())
            && super::visual_record::find(&paths.qa_output, &task.key).is_none()
        {
            let why = match timed_out {
                true => format!("its {} minutes ran out", job.timeout_minutes),
                false => "its process ended without a result".to_string(),
            };
            let written = super::visual_record::write_stub(
                &paths.qa_output,
                &task.key,
                &task.branch,
                &super::visual_record::date_of(now_ms()),
                &why,
                &self.stub_items(&job.id)?,
            );
            match written {
                Ok(_) => stub = true,
                Err(error) => {
                    eprintln!("tinto: delivery could not write a stub QA record: {error}")
                }
            }
        }
        if job.role == QA_ROLE
            && (!cancelled || super::visual_record::find(&paths.qa_output, &task.key).is_some())
        {
            // Its problems wait for the user's call in the task. The job
            // ends even when they cannot be saved.
            if let Err(error) = self.save_visual_record(&task, &job.id, &paths.qa_output, !stub) {
                eprintln!("tinto: delivery could not save a QA record: {error}");
            }
        }
        self.finish(&job.id, ending, outcome, timing, end, reverted, &failure)?;
        // A task Tinto cannot queue checks on now (blocked, paused) goes on
        // without them.
        let _ = self.queue_checks(&job.id);
        if let Err(error) = self.settle_doubtful_check(&job.id) {
            eprintln!(
                "tinto: delivery could not look again at check {}: {error}",
                job.id
            );
        }
        if let Err(error) = self.settle_account_failure(&job.id) {
            eprintln!(
                "tinto: delivery could not settle the account failure of {}: {error}",
                job.id
            );
        }
        if let Err(error) = self.conclude_merge(&job.task_id) {
            eprintln!(
                "tinto: delivery could not settle the merge of {}: {error}",
                job.task_id
            );
        }
        Ok(())
    }

    /// Brings the repo's QA stack to serve the task's worktree as it is now:
    /// rebuilds the services its changes touch (and those another task's
    /// worktree was served by), unless the stack already serves it; waits
    /// for it to answer and requests the pages QA visits. Its log goes to
    /// the job's folder; what it did, to the job and to the stack's resource.
    fn prepare_qa_stack(
        &self,
        job: &DeliveryJob,
        task: &DeliveryTask,
        stack: &DeliveryQaStack,
        paths: &JobPaths,
        start: Option<&WorktreeSnapshot>,
    ) -> Result<DeliveryQaPrepare, PrepareStop> {
        use super::qa_prepare as prepare;
        let began = Instant::now();
        let until = began + prepare::PREPARE_LIMIT;
        let log = paths.dir.join(prepare::PREPARE_LOG);
        let failed = |message: String| PrepareStop::Failed(message);
        let cancel = self
            .inner
            .cancels
            .lock()
            .ok()
            .and_then(|cancels| cancels.get(&job.id).cloned())
            .unwrap_or_default();
        self.record_observation(
            &job.id,
            adapters::Observation {
                command: Some(Some((
                    "Tinto prepara el entorno de QA".to_string(),
                    now_ms(),
                ))),
                ..Default::default()
            },
        );
        // What the task changed since it left its base, committed or not.
        let place = Place::of(task.distro.as_deref());
        let base = tasks::git(
            place,
            &task.worktree,
            &["merge-base", "HEAD", &task.base_ref],
        )
        .unwrap_or_else(|_| task.base_commit.clone());
        let mut changed: Vec<String> = Vec::new();
        for args in [
            vec!["diff", "--name-only", base.as_str()],
            vec!["ls-files", "--others", "--exclude-standard"],
        ] {
            let listed = tasks::git(place, &task.worktree, &args).map_err(|e| failed(e.message))?;
            changed.extend(listed.lines().map(str::to_string));
        }
        let candidate = start
            .map(WorktreeSnapshot::candidate_id)
            .unwrap_or_default();
        let lease_name = job.lease.clone().unwrap_or_default();
        let prepared = self
            .store()
            .and_then(|store| store.lease(&lease_name))
            .map_err(|e| failed(e.message))?
            .and_then(|lease| lease.prepared);
        let (services, reused) = prepare::rebuild(
            &task.id,
            &candidate,
            prepare::services_for(&changed, &stack.services),
            prepared.as_ref(),
        );
        prepare::log(
            &log,
            &format!(
                "{} archivos cambiados desde {base}; servicios a reconstruir: {}",
                changed.len(),
                match (reused, services.is_empty()) {
                    (true, _) => "ninguno (el entorno ya sirve este worktree)".to_string(),
                    (false, true) => "ninguno".to_string(),
                    (false, false) => services.join(" "),
                }
            ),
        );
        let run = |command: &str| -> Result<(), PrepareStop> {
            let mut shell = job.clone();
            shell.agent = DeliveryAgent::Shell;
            shell.prompt = command.to_string();
            let launch =
                adapters::launch(&shell, task, paths, None).map_err(|e| failed(e.message))?;
            let mut process = Command::new(&launch.program);
            process.args(&launch.args).envs(&job.check_env);
            if let Some(cwd) = &launch.cwd {
                process.current_dir(cwd);
            }
            prepare::run(process, command, &log, &cancel, until, &job.id).map_err(|message| {
                if let Some(wsl_job) = &launch.wsl_job {
                    adapters::stop_wsl_job(wsl_job, "KILL");
                }
                match cancel.load(Ordering::SeqCst) {
                    true => PrepareStop::Cancelled,
                    false => failed(message),
                }
            })
        };
        let mut done = Vec::new();
        if !services.is_empty() {
            run(&prepare::up_command(stack, &services))?;
            done.push(format!("reconstruyó {}", services.join(", ")));
        }
        if !reused && !stack.preview.is_empty() {
            run(&stack.preview)?;
            done.push("sirve el frontend compilado para producción".to_string());
        }
        prepare::log(&log, &format!("esperando {}", stack.ready_url));
        prepare::wait_ready(&stack.ready_url, &cancel, until).map_err(|message| {
            match cancel.load(Ordering::SeqCst) {
                true => PrepareStop::Cancelled,
                false => failed(message),
            }
        })?;
        if !reused {
            for command in &stack.seed {
                run(command)?;
            }
        }
        prepare::log(&log, "calentando las páginas de QA");
        let warmed = prepare::warm(&stack.ready_url, &stack.warm_paths, &log).map_err(failed)?;
        done.push(format!(
            "respondió y compiló {warmed} módulos de sus páginas"
        ));
        let note = match reused {
            true => format!(
                "ya servía este worktree en el mismo commit, así que no reconstruyó nada; {}.",
                done.join("; ")
            ),
            false if services.is_empty() => format!(
                "la tarea no cambió ningún servicio del entorno, así que no reconstruyó nada; {}.",
                done.join("; ")
            ),
            false => format!("{}.", done.join("; ")),
        };
        // The stack serves this worktree now, with what this task built on
        // it before.
        let mut built = services.clone();
        if let Some(earlier) = prepared.filter(|earlier| earlier.task_id == task.id) {
            built.extend(earlier.services);
        }
        built.sort();
        built.dedup();
        if let Ok(store) = self.store() {
            if let Ok(Some(mut lease)) = store.lease(&lease_name) {
                lease.prepared = Some(DeliveryPreparedStack {
                    task_id: task.id.clone(),
                    candidate,
                    services: built,
                });
                let _ = store.put_lease(&lease);
            }
        }
        prepare::log(&log, &note);
        Ok(DeliveryQaPrepare {
            ms: began.elapsed().as_millis() as u64,
            services,
            reused,
            note,
        })
    }

    /// A QA job the user cancelled while Tinto prepared its stack.
    fn cancel_preparing(&self, job_id: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let Some(mut job) = store.job(job_id)? else {
            return Ok(());
        };
        job.status = DeliveryJobStatus::Cancelled;
        job.error =
            Some("Cancelado por el usuario mientras Tinto preparaba el entorno de QA.".to_string());
        job.ended_at_ms = Some(now_ms());
        job.current_command = None;
        job.current_command_at_ms = None;
        store.put_job(&job)?;
        settle_lease(&store, &job, false, &[])?;
        store.record_event(
            now_ms(),
            "job_cancelled",
            Some(&job.task_id),
            Some(&job.id),
            "",
        )
    }

    /// What a stub QA record lists: the agent's task list as it last wrote
    /// it, else the earlier problems it was to check again.
    fn stub_items(&self, job_id: &str) -> Result<Vec<(String, bool)>, DeliveryError> {
        let job = self.job(job_id)?;
        if !job.plan.is_empty() {
            return Ok(job
                .plan
                .into_iter()
                .map(|item| (item.text, item.status == "completed"))
                .collect());
        }
        Ok(job
            .qa_followup
            .iter()
            .filter_map(|note| {
                // "Check #9 again: «<problem>». The fix the user chose: …"
                let (head, rest) = note.strip_prefix("Check ")?.split_once(": «")?;
                let problem: String = rest.split('»').next()?.chars().take(160).collect();
                Some((
                    format!("{} {problem}", head.trim_end_matches(" again")),
                    false,
                ))
            })
            .collect())
    }

    /// A task based on another's branch (a package built on another, or a
    /// task created from another task's branch) starts from that task's
    /// latest work: until it has anything of its own, its branch moves up
    /// to that one's.
    fn catch_up_package(&self, mut task: DeliveryTask) -> Result<DeliveryTask, DeliveryError> {
        let Some(other) = self.base_task(&task)? else {
            return Ok(task);
        };
        if other.removed_at_ms.is_some() {
            return Ok(task);
        }
        let place = Place::of(task.distro.as_deref());
        let Some(head) =
            tasks::fast_forward_untouched(place, &task.worktree, &task.base_commit, &other.branch)
        else {
            return Ok(task);
        };
        task.base_commit = head;
        task.updated_at_ms = now_ms();
        let store = self.store()?;
        store.put_task(&task)?;
        store.record_event(
            task.updated_at_ms,
            "package_caught_up",
            Some(&task.id),
            None,
            &format!("al día con {}", other.label()),
        )?;
        Ok(task)
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
        let mut decisions = self.store()?.task_decisions(&task.id)?;
        // Past the brainstorm permissions are allowed by `auto`, so they are
        // allowed again for the code QA is about to run.
        if let Some(start) = start.filter(|_| self.unattended(task).unwrap_or(false)) {
            for decision in decisions.iter_mut().filter(|decision| {
                allowed_permission(decision)
                    && decision.runs_candidate
                    && decision.candidate.as_deref() != Some(start.tree.as_str())
            }) {
                let mut renewed = decision.clone();
                renewed.candidate = Some(start.tree.clone());
                *decision = self.record_answer(renewed, decisions::AUTO, "permission_renewed")?;
            }
        }
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

    fn record_observation(&self, job_id: &str, seen: adapters::Observation) {
        if let Ok(store) = self.store() {
            if let Ok(Some(mut job)) = store.job(job_id) {
                if let Some(activity) = seen.activity {
                    job.activity = Some(activity);
                    job.activity_at_ms = Some(now_ms());
                }
                if let Some(plan) = seen.plan {
                    job.plan = plan;
                }
                if let Some(usage) = seen.usage {
                    job.usage = Some(match (job.agent, &job.usage) {
                        (DeliveryAgent::Opencode, Some(before)) => before.plus(&usage),
                        _ => usage,
                    });
                }
                if let Some(command) = seen.command {
                    // A command that ends is the agent's turn again.
                    job.activity_at_ms = Some(now_ms());
                    job.current_command_at_ms = command.as_ref().map(|(_, at)| *at);
                    job.current_command = command.map(|(command, _)| command);
                }
                let _ = store.put_job(&job);
            }
        }
        self.notify();
    }

    fn record_model(&self, job_id: &str, model: String) {
        if let Ok(store) = self.store() {
            if let Ok(Some(mut job)) = store.job(job_id) {
                job.session_model = Some(model);
                let _ = store.put_job(&job);
            }
        }
        self.notify();
    }

    /// After a stage that changed the task's code and passed, Tinto runs the
    /// task's checks its changes reach, one shell job each, ahead of anything
    /// dispatched later. Not after the tests stage: its new tests are meant
    /// to fail until the implementation.
    fn queue_checks(&self, job_id: &str) -> Result<(), DeliveryError> {
        let job = self.job(job_id)?;
        let passed = job.status == DeliveryJobStatus::Finished
            && job.result_state == Some(DeliveryResultState::Accepted)
            && job
                .result
                .as_ref()
                .is_some_and(|result| result.status == "pass");
        if job.agent == DeliveryAgent::Shell
            || job.role == "tests"
            || job.changes.is_empty()
            || !passed
        {
            return Ok(());
        }
        self.queue_task_checks(&job.task_id, Some(&job))
    }

    /// Queues the task's checks as one shell job each, ahead of anything
    /// dispatched later: after a `stage`, those its changes reach (see
    /// `phase::touches`) and any that never ran on the task; otherwise all.
    pub(super) fn queue_task_checks(
        &self,
        task_id: &str,
        stage: Option<&DeliveryJob>,
    ) -> Result<(), DeliveryError> {
        let task = self.task(task_id)?;
        let jobs = self.store()?.task_jobs(task_id)?;
        let scopes: Vec<String> = task
            .checks
            .iter()
            .filter_map(|check| super::phase::check_scope(check))
            .collect();
        for check in task.checks.clone() {
            let ran = jobs.iter().any(|job| {
                job.agent == DeliveryAgent::Shell && job.role == CHECKS_ROLE && job.prompt == check
            });
            if ran && stage.is_some_and(|stage| !super::phase::touches(&check, &scopes, stage)) {
                continue;
            }
            self.dispatch(NewJob {
                task_id: task_id.to_string(),
                role: CHECKS_ROLE.to_string(),
                agent: DeliveryAgent::Shell,
                model: None,
                access: DeliveryAccess::Workspace,
                prompt: check,
                writes: None,
                lease: None,
                timeout_minutes: None,
                reasoning: None,
                platform: None,
                read_dirs: Vec::new(),
                stage_default: false,
                needs_environment: false,
            })?;
        }
        Ok(())
    }

    /// The coordinator asks for the task's checks on the code as it is now,
    /// instead of running them in its own shell.
    pub fn run_checks(&self, task_id: &str) -> Result<Vec<DeliveryJob>, DeliveryError> {
        let task = self.task(task_id)?;
        if task.checks.is_empty() {
            return Err(DeliveryError::new(
                "no_checks",
                "la tarea no tiene comprobaciones: fíjalas con set_task_state `checks`",
            ));
        }
        let open = |jobs: &[DeliveryJob]| -> Vec<DeliveryJob> {
            jobs.iter()
                .filter(|job| job.role == CHECKS_ROLE && job.status.is_open())
                .cloned()
                .collect()
        };
        if !open(&self.store()?.task_jobs(task_id)?).is_empty() {
            return Err(DeliveryError::new(
                "checks_running",
                "las comprobaciones de la tarea ya están en cola o corriendo: espera su resultado con wait_run",
            ));
        }
        self.queue_task_checks(task_id, None)?;
        Ok(open(&self.store()?.task_jobs(task_id)?))
    }

    /// What an agent job is told about the task's checks: that Tinto runs
    /// them, and how they did when they ran on the code it starts from.
    fn checks_note(
        &self,
        task: &DeliveryTask,
        job: &DeliveryJob,
    ) -> Result<Vec<String>, DeliveryError> {
        if task.checks.is_empty() || job.agent == DeliveryAgent::Shell {
            return Ok(Vec::new());
        }
        let jobs = self.store()?.task_jobs(&task.id)?;
        let jobs: Vec<&DeliveryJob> = jobs.iter().filter(|other| other.id != job.id).collect();
        let listed: Vec<String> = task
            .checks
            .iter()
            .map(|check| format!("`{check}`"))
            .collect();
        let mut lines = vec![format!(
            "Tinto runs this task's checks itself on what each stage changes: {}.",
            listed.join("; ")
        )];
        if job.role == "tests" {
            lines.push("Run only the tests you write or change: Tinto runs the full checks once the implementation makes them pass.".to_string());
        } else if job.writes {
            lines.push("While you work, run only the tests of what you change, not these checks in full: Tinto runs them on your result when you finish.".to_string());
            // Formatting and lint are quick, and each one Tinto finds after
            // you costs a round.
            let style: Vec<String> = task
                .checks
                .iter()
                .filter(|check| style_check(check))
                .map(|check| format!("`{check}`"))
                .collect();
            if !style.is_empty() {
                lines.push(format!(
                    "Before you finish, run the format and lint checks and fix what they report: {}.",
                    style.join("; ")
                ));
            }
        } else if job.role == QA_ROLE {
            lines.push("Do not run these checks, builds, linters, type checkers or test suites: Tinto runs them. Test the behaviour the task asks for.".to_string());
        } else {
            lines.push("Do not run these checks, builds, linters, type checkers or test suites to review: Tinto runs them on every stage's result and sends failures back. Review by reading the change; run a test only to confirm a specific doubt.".to_string());
        }
        let same_code = last_change(&jobs).is_some_and(|change| {
            change.end_candidate.is_some() && change.end_candidate == job.start_candidate
        });
        if !same_code {
            lines.extend(latest_check_runs(&task.checks, &jobs));
        }
        let checks = current_checks(&task.checks, &jobs);
        if !same_code || checks.is_empty() {
            return Ok(lines);
        }
        let failed: Vec<&&DeliveryJob> = checks.iter().filter(|check| check_fails(check)).collect();
        if failed.is_empty() {
            if checks
                .iter()
                .all(|check| check.status == DeliveryJobStatus::Finished)
            {
                lines.push(
                    "They all passed on the code you start from: do not run them again."
                        .to_string(),
                );
            }
            return Ok(lines);
        }
        lines.push("These failed on the code you start from:".to_string());
        for check in failed {
            lines.push(format!(
                "- `{}`: {}\n  Its output ends with:\n{}",
                check.prompt,
                check.error.as_deref().unwrap_or("falló"),
                self.check_output_tail(&check.id)
            ));
        }
        Ok(lines)
    }

    /// The end of a check's output, stdout then stderr.
    fn check_output_tail(&self, job_id: &str) -> String {
        let Ok(paths) = (self.inner.paths_for)(job_id) else {
            return String::new();
        };
        let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
        let output = format!(
            "{}\n{}",
            read(&paths.events).trim_end(),
            read(&paths.stderr).trim_end()
        );
        let output = output.trim();
        let start = output
            .char_indices()
            .rev()
            .nth(CHECK_TAIL_CHARS - 1)
            .map_or(0, |(at, _)| at);
        output[start..].to_string()
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &self,
        job_id: &str,
        ending: Ending,
        outcome: JobOutcome,
        timing: Option<DeliveryTiming>,
        end: Option<WorktreeSnapshot>,
        reverted: Vec<String>,
        failure: &str,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let mut job = store
            .job(job_id)?
            .ok_or_else(|| DeliveryError::not_found("el trabajo", job_id))?;
        let task = store.task(&job.task_id)?;
        job.ended_at_ms = Some(now_ms());
        job.exit_code = ending.exit_code;
        job.current_command = None;
        job.current_command_at_ms = None;
        job.timing = timing.map(|timing| DeliveryTiming {
            prepare_ms: job.prepare.as_ref().map(|prepare| prepare.ms),
            ..timing
        });
        job.reverted = reverted;
        if outcome.usage.is_some() {
            job.usage = outcome.usage.clone();
        }
        if outcome.provider_session_id.is_some() {
            job.provider_session_id = outcome.provider_session_id.clone();
        }
        if let Some(end) = &end {
            job.end_candidate = Some(end.candidate_id());
            job.changes = to_changes(end);
        }
        let paused = self
            .inner
            .pausing
            .lock()
            .is_ok_and(|mut pausing| pausing.remove(job_id));
        if ending.cancelled && paused {
            job.status = DeliveryJobStatus::Paused;
        } else if ending.cancelled {
            job.status = DeliveryJobStatus::Cancelled;
            job.error = Some("Cancelado por el usuario.".to_string());
        } else if ending.timed_out {
            job.status = DeliveryJobStatus::Failed;
            job.error = Some(format!("{TIMED_OUT} de {} minutos.", job.timeout_minutes));
        } else if !ending.exit_ok {
            job.status = DeliveryJobStatus::Failed;
            job.error = Some(
                outcome
                    .error
                    .clone()
                    .unwrap_or_else(|| match ending.exit_code {
                        Some(code) if failure.is_empty() => {
                            format!("El proceso terminó con código {code}.")
                        }
                        Some(code) => format!("El proceso terminó con código {code}: {failure}"),
                        None => "El proceso terminó de forma anormal.".to_string(),
                    }),
            );
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
        if !job.reverted.is_empty() {
            let note = format!(
                "Tinto deshizo lo que el trabajo cambió en el worktree: {}.",
                job.reverted.join(", ")
            );
            job.result_note = Some(match job.result_note.take() {
                Some(earlier) => format!("{earlier} {note}"),
                None => note,
            });
            store.record_event(
                now_ms(),
                "qa_changes_reverted",
                Some(&job.task_id),
                Some(&job.id),
                &job.reverted.join(", "),
            )?;
        }
        store.put_job(&job)?;
        settle_lease(&store, &job, ending.touched, &ending.leftovers)?;
        if !ending.leftovers.is_empty() {
            store.record_event(
                now_ms(),
                "job_leftovers",
                Some(&job.task_id),
                Some(&job.id),
                &ending.leftovers.join(", "),
            )?;
        }
        let detail = format!("{:?}", job.status).to_ascii_lowercase();
        // The coordinator hears of the task's checks, and of its setup, once,
        // when all ended.
        let tinto_run = match (job.agent, job.role.as_str()) {
            (DeliveryAgent::Shell, CHECKS_ROLE) => Some(("check_ended", "checks_ended")),
            (DeliveryAgent::Shell, SETUP_ROLE) => Some(("setup_command_ended", "setup_ended")),
            _ => None,
        };
        store.record_event(
            now_ms(),
            tinto_run.map_or("job_ended", |(one, _)| one),
            Some(&job.task_id),
            Some(&job.id),
            &detail,
        )?;
        if let (Some(task), Some((_, all))) = (task.as_ref(), tinto_run) {
            let jobs = store.task_jobs(&task.id)?;
            let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
            let (wanted, current) = if job.role == CHECKS_ROLE {
                (&task.checks, current_checks(&task.checks, &jobs))
            } else {
                (&task.setup, current_setup(&task.setup, &jobs))
            };
            if let Some(outcome) = checks_outcome(wanted, &current) {
                store.record_event(now_ms(), all, Some(&job.task_id), None, &outcome)?;
            }
        }
        // Findings or failing checks send the work back: the round is there
        // as soon as the job is seen ended. A check whose failure may not be
        // the code's is looked at first (see `settle_doubtful_check`).
        let doubtful = task.as_ref().is_some_and(|task| {
            doubtful_check(
                &job,
                task,
                &store.task_jobs(&task.id).unwrap_or_default(),
                &store,
            )
        });
        if !doubtful {
            if let Err(error) = super::rounds::after_job(&store, &job) {
                eprintln!("tinto: delivery could not record the round of {job_id}: {error}");
            }
        }
        Ok(())
    }

    /// A job the agent's account stopped, not its work: a model the account
    /// refused is not dispatched again until the catalog is seen anew, and
    /// a usage limit makes the job run again once the limit may have reset.
    fn settle_account_failure(&self, job_id: &str) -> Result<(), DeliveryError> {
        let job = self.job(job_id)?;
        if job.status != DeliveryJobStatus::Failed {
            return Ok(());
        }
        let error = job.error.as_deref().unwrap_or_default();
        if error.starts_with(adapters::MODEL_REFUSED) {
            if let Some(model) = &job.model {
                let store = self.store()?;
                let mut refused: Vec<String> =
                    store.setting(REFUSED_MODELS_SETTING)?.unwrap_or_default();
                let key = refused_model(job.agent, model);
                if !refused.contains(&key) {
                    refused.push(key);
                    store.set_setting(REFUSED_MODELS_SETTING, &refused)?;
                }
            }
            return Ok(());
        }
        if !usage_limit(error) {
            return Ok(());
        }
        let store = self.store()?;
        let mut stopped = job.clone();
        stopped.error = Some(format!(
            "{error} — Tinto lo reintenta en {} minutos, cuando el límite puede haberse renovado.",
            USAGE_LIMIT_WAIT.as_secs() / 60
        ));
        store.put_job(&stopped)?;
        drop(store);
        self.notify();
        let service = self.clone();
        let job_id = job.id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(USAGE_LIMIT_WAIT);
            if service.inner.closed.load(Ordering::SeqCst) {
                return;
            }
            if let Err(error) = service.retry_after_restart(&job_id, "tras el límite de uso") {
                eprintln!(
                    "tinto: delivery could not retry {job_id} after its usage limit: {error}"
                );
            }
        });
        Ok(())
    }

    /// A check that failed on the first run on this code because it was
    /// killed, or in a repo whose environment may be down, before it sends
    /// the work back: killed, it runs again; with the environment down, it
    /// waits for it; otherwise its round opens as any other.
    fn settle_doubtful_check(&self, job_id: &str) -> Result<(), DeliveryError> {
        let job = self.job(job_id)?;
        let task = self.task(&job.task_id)?;
        let jobs = self.store()?.task_jobs(&task.id)?;
        if !doubtful_check(&job, &task, &jobs, &*self.store()?) {
            return Ok(());
        }
        let again = |needs_environment: bool| {
            self.dispatch(NewJob {
                task_id: task.id.clone(),
                role: CHECKS_ROLE.to_string(),
                agent: DeliveryAgent::Shell,
                model: None,
                access: DeliveryAccess::Workspace,
                prompt: job.prompt.clone(),
                writes: None,
                lease: None,
                timeout_minutes: Some(job.timeout_minutes),
                reasoning: None,
                platform: None,
                read_dirs: Vec::new(),
                stage_default: false,
                needs_environment,
            })
            .map(|_| ())
        };
        if killed(&job) {
            return again(false);
        }
        let down = self
            .check_environment(&task.repo, task.distro.as_deref())?
            .iter()
            .any(|check| !check.ok);
        if down {
            return again(true);
        }
        let store = self.store()?;
        super::rounds::after_job(&store, &job)
    }

    /// A command Tinto adopted after a restart whose end it cannot learn:
    /// interrupted, as if the restart had cut it off, and queued again.
    fn interrupt_unknown_end(
        &self,
        job_id: &str,
        leftovers: &[String],
    ) -> Result<(), DeliveryError> {
        {
            let store = self.store()?;
            let mut job = store
                .job(job_id)?
                .ok_or_else(|| DeliveryError::not_found("el trabajo", job_id))?;
            job.status = DeliveryJobStatus::Interrupted;
            job.ended_at_ms = Some(now_ms());
            job.current_command = None;
            job.current_command_at_ms = None;
            job.error =
                Some("Tinto se reinició mientras corría y no pudo saber cómo terminó.".to_string());
            store.put_job(&job)?;
            settle_lease(&store, &job, true, leftovers)?;
            store.record_event(
                now_ms(),
                "job_interrupted",
                Some(&job.task_id),
                Some(&job.id),
                "",
            )?;
        }
        self.retry_after_restart(job_id, "tras reiniciar Tinto")
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
        job.current_command = None;
        job.current_command_at_ms = None;
        job.error = Some(message.to_string());
        store.put_job(&job)?;
        settle_lease(&store, &job, touched, &[])?;
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

/// The check's process was stopped from outside (the system or a closed
/// console on Windows, a signal elsewhere) or could not start, not failed
/// by its tests.
fn killed(job: &DeliveryJob) -> bool {
    // 0x40010004 (DBG_TERMINATE_PROCESS), 0xC000013A (STATUS_CONTROL_C_EXIT)
    // and 0xC0000142 (STATUS_DLL_INIT_FAILED: Windows could not start it,
    // as when many processes start at once).
    const WINDOWS_KILLED: [i32; 3] = [0x4001_0004, 0xC000_013A_u32 as i32, 0xC000_0142_u32 as i32];
    let timed_out = job
        .error
        .as_deref()
        .is_some_and(|error| error.starts_with(TIMED_OUT));
    match job.exit_code {
        Some(code) => WINDOWS_KILLED.contains(&code),
        None => !timed_out && job.ended_at_ms.is_some(),
    }
}

/// A failed check of Tinto's whose failure may not be the code's: its first
/// failure on this code, killed, or in a repo whose jobs need an
/// environment (Docker, probes) that may be down.
fn doubtful_check(
    job: &DeliveryJob,
    task: &DeliveryTask,
    jobs: &[DeliveryJob],
    store: &DeliveryStore,
) -> bool {
    if job.agent != DeliveryAgent::Shell
        || job.role != CHECKS_ROLE
        || job.status != DeliveryJobStatus::Failed
    {
        return false;
    }
    let refs: Vec<&DeliveryJob> = jobs.iter().collect();
    let since = last_change(&refs).map_or(0, |change| change.created_at_ms);
    let failed_before = jobs.iter().any(|other| {
        other.id != job.id
            && other.agent == DeliveryAgent::Shell
            && other.role == CHECKS_ROLE
            && other.prompt == job.prompt
            && other.created_at_ms > since
            && other.created_at_ms < job.created_at_ms
            && other.status == DeliveryJobStatus::Failed
    });
    if failed_before {
        return false;
    }
    killed(job)
        || store.repo_settings(&task.repo).is_ok_and(|settings| {
            !settings.environment_probes.is_empty()
                || super::environment::uses_docker(&task.repo, task.distro.as_deref(), &settings)
        })
}

/// Releases the job's exclusive resource, or quarantines it when the job
/// ran and did not finish cleanly. Also drops the job from any queue.
fn settle_lease(
    store: &DeliveryStore,
    job: &DeliveryJob,
    touched: bool,
    leftovers: &[String],
) -> Result<(), DeliveryError> {
    let Some(name) = job.lease.as_deref() else {
        return Ok(());
    };
    let Some(mut lease) = store.lease(name)? else {
        return Ok(());
    };
    lease.queue.retain(|waiter| waiter.job_id != job.id);
    // A paused job keeps its resource for the job that resumes it.
    if lease.holder_job_id.as_deref() == Some(job.id.as_str())
        && job.status != DeliveryJobStatus::Paused
    {
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
            let found = match leftovers {
                [] => String::new(),
                left => format!(
                    " Al terminar seguían en marcha, y Tinto los paró: {}.",
                    left.join(", ")
                ),
            };
            lease.note = Some(format!(
                "El trabajo {} de {key} {how}, así que el entorno puede haber quedado a medio usar.{found} Nadie más puede usarlo hasta que lo liberes: comprueba que no queden procesos ni datos de prueba suyos y pulsa Liberar.",
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

/// The repo's environment variables for checks as saved: names a shell can
/// export (`TEST_DB_MONGO_URL`), values trimmed.
pub(super) fn clean_check_env(
    env: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, DeliveryError> {
    env.into_iter()
        .map(|(name, value)| {
            let name = name.trim().to_string();
            if !valid_env_name(&name) {
                return Err(DeliveryError::new(
                    "invalid_check_env",
                    format!("variable de entorno no válida: «{name}» (letras, dígitos y _, sin empezar por dígito)"),
                ));
            }
            Ok((name, value.trim().to_string()))
        })
        .collect()
}

/// A name a shell can export: letters, digits and `_`, not starting with a digit.
pub(super) fn valid_env_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| !c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Folders a coordinator lets a task's jobs read, each checked, once each.
fn readable_dirs(
    dirs: &[String],
    repo: &Path,
    worktree: &Path,
    distro: Option<&str>,
) -> Result<Vec<String>, DeliveryError> {
    let mut readable: Vec<String> = Vec::new();
    for dir in dirs {
        let dir = readable_dir(dir, repo, worktree, distro)?;
        if !readable.contains(&dir) {
            readable.push(dir);
        }
    }
    Ok(readable)
}

fn validate_package(package: &NewPackage) -> Result<(), DeliveryError> {
    let invalid = |message: String| DeliveryError::new("invalid_package", message);
    if !(2..=20).contains(&package.of) {
        return Err(invalid("un issue se reparte en 2 a 20 paquetes".into()));
    }
    if !(1..=package.of).contains(&package.index) {
        return Err(invalid(format!("los paquetes van del 1 al {}", package.of)));
    }
    if package
        .builds_on
        .is_some_and(|other| other < 1 || other >= package.index)
    {
        return Err(invalid(
            "un paquete solo construye sobre uno anterior".into(),
        ));
    }
    if package.title.trim().chars().count() > 60 {
        return Err(invalid(
            "el título del paquete admite hasta 60 caracteres".into(),
        ));
    }
    Ok(())
}

/// A folder a coordinator lets a job read: an existing folder inside the
/// repo's main checkout or the task's worktree, by absolute path.
fn readable_dir(
    dir: &str,
    repo: &Path,
    worktree: &Path,
    distro: Option<&str>,
) -> Result<String, DeliveryError> {
    let dir = dir.trim();
    let normalized = decisions::norm(dir);
    let inside = [repo, worktree]
        .iter()
        .any(|root| decisions::within(&normalized, &decisions::norm(&plain_path(root))));
    let refused =
        |why: &str| DeliveryError::new("invalid_read_dir", format!("read_dirs: {dir} {why}"));
    if !decisions::is_absolute(&normalized) || normalized.split('/').any(|part| part == "..") {
        return Err(refused("debe ser una ruta absoluta, sin `..`"));
    }
    if !inside {
        return Err(refused(
            "debe estar dentro del checkout principal del repositorio o del worktree de la tarea",
        ));
    }
    // A WSL repo's folders are reached through the distro's network share.
    let local = match distro {
        Some(distro) if cfg!(windows) && dir.starts_with('/') => PathBuf::from(format!(
            r"\\wsl.localhost\{distro}{}",
            dir.replace('/', "\\")
        )),
        _ => PathBuf::from(dir),
    };
    if !local.is_dir() {
        return Err(refused("no existe o no es una carpeta"));
    }
    Ok(dir.to_string())
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

/// Why a job a restart interrupted must not run again on its own, if so:
/// what it did may have half-happened where a new attempt cannot see it.
/// The repo's commands a coordinator chose, trimmed and without repeats;
/// `Err` with the first one that is not among `allowed` as written.
fn pick_commands(chosen: &[String], allowed: &[String]) -> Result<Vec<String>, String> {
    let mut picked: Vec<String> = Vec::new();
    for command in chosen.iter().map(|command| command.trim()) {
        if !allowed.iter().any(|allowed| allowed.trim() == command) {
            return Err(command.to_string());
        }
        if !picked.iter().any(|picked| picked == command) {
            picked.push(command.to_string());
        }
    }
    Ok(picked)
}

/// How the task's latest checks (or setup commands) went, once each of
/// `wanted` ran to its end.
fn checks_outcome(wanted: &[String], checks: &[&DeliveryJob]) -> Option<String> {
    let ended = |command: &String| {
        checks
            .iter()
            .any(|check| &check.prompt == command && !check.status.is_open())
    };
    if !wanted.iter().all(ended) || checks.iter().any(|check| check.status.is_open()) {
        return None;
    }
    let named = |failed: bool| -> Vec<String> {
        checks
            .iter()
            .filter(|check| check.status != DeliveryJobStatus::Finished)
            .filter(|check| (check.status == DeliveryJobStatus::Failed) == failed)
            .map(|check| format!("`{}`", check.prompt))
            .collect()
    };
    let mut parts = Vec::new();
    if !named(true).is_empty() {
        parts.push(format!("fallaron {}", named(true).join(", ")));
    }
    if !named(false).is_empty() {
        parts.push(format!("no terminaron {}", named(false).join(", ")));
    }
    Some(if parts.is_empty() {
        "pasaron".to_string()
    } else {
        parts.join("; ")
    })
}

/// An agent with workspace access only changes its worktree (no network
/// outside QA, no push, no merge), and a new attempt starts from what the
/// first one left there.
fn not_safe_to_repeat(job: &DeliveryJob) -> Option<String> {
    if job.retry_of.is_some() {
        return Some("ya era el reintento automático de otro trabajo interrumpido".to_string());
    }
    // A check only reads the code, and setup commands are made to run again.
    if job.agent == DeliveryAgent::Shell && !matches!(job.role.as_str(), CHECKS_ROLE | SETUP_ROLE) {
        return Some(
            "es un comando de shell, que pudo quedar a medias haciendo cualquier cosa".to_string(),
        );
    }
    if job.access == DeliveryAccess::Full {
        return Some(
            "corría con acceso completo y pudo dejar a medias algo fuera del worktree (un push, un merge, datos de prueba)"
                .to_string(),
        );
    }
    if let Some(lease) = &job.lease {
        return Some(format!(
            "tenía el recurso «{lease}», que quedó en cuarentena: revisa el entorno y libéralo antes de repetirla"
        ));
    }
    None
}

/// The start of the error of a job that ran out of time.
const TIMED_OUT: &str = "Se agotó el tiempo máximo";

/// A stopped job's assignment again, to go on in its agent session.
fn in_same_session(job: &DeliveryJob) -> NewJob {
    NewJob {
        task_id: job.task_id.clone(),
        role: job.role.clone(),
        agent: job.agent,
        model: job.model.clone(),
        access: job.access,
        prompt: job.prompt.clone(),
        writes: Some(job.writes),
        lease: job.lease.clone(),
        timeout_minutes: Some(job.timeout_minutes),
        reasoning: job.reasoning.clone(),
        platform: job.platform,
        read_dirs: job.read_dirs.clone(),
        stage_default: job.stage_default,
        needs_environment: job.needs_environment,
    }
}

/// What an agent that goes on with `stopped` in its session is told, when
/// the job stopped on its own (time limit, restart) rather than paused.
fn continue_note(stopped: &DeliveryJob) -> Option<String> {
    let why = match stopped.status {
        _ if stopped.agent == DeliveryAgent::Shell => return None,
        DeliveryJobStatus::Interrupted => "Tinto se cerró mientras corría".to_string(),
        DeliveryJobStatus::Failed
            if stopped
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with(TIMED_OUT)) =>
        {
            format!(
                "se agotó su tiempo máximo de {} minutos",
                stopped.timeout_minutes
            )
        }
        _ => return None,
    };
    Some(format!(
        "La ejecución anterior de esta etapa se detuvo antes de terminar: {why}. El worktree conserva los cambios que hiciste. Sigue desde donde lo dejaste, con las mismas instrucciones, sin rehacer lo que ya está hecho, y termina entregando el resultado con el formato pedido."
    ))
}

/// A task's key as `{task}` gives it in the variables for checks: lowercase,
/// with `_` for anything not a letter or digit (`AGOS-474` → `agos_474`).
fn task_slug(key: &str) -> String {
    key.chars()
        .map(|c| match c.is_ascii_alphanumeric() {
            true => c.to_ascii_lowercase(),
            false => '_',
        })
        .collect()
}

pub(crate) fn agent_name(agent: DeliveryAgent) -> &'static str {
    match agent {
        DeliveryAgent::Codex => "Codex",
        DeliveryAgent::Claude => "Claude Code",
        DeliveryAgent::Muse => "Muse Code",
        DeliveryAgent::Opencode => "OpenCode",
        DeliveryAgent::Shell => "Shell",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::adapters::tests::sample_job;

    #[test]
    fn a_stage_on_other_code_hears_how_long_each_check_took() {
        let check = |at: u64, command: &str, status, seconds: u64| {
            let mut job = sample_job(DeliveryAgent::Shell);
            job.id = format!("c{at}");
            job.created_at_ms = at;
            job.role = CHECKS_ROLE.into();
            job.prompt = command.into();
            job.status = status;
            job.started_at_ms = Some(1_000);
            job.ended_at_ms = Some(1_000 + seconds * 1000);
            job
        };
        let jobs = [
            check(1, "npm run unit-tests", DeliveryJobStatus::Failed, 30),
            check(2, "npm run unit-tests", DeliveryJobStatus::Finished, 660),
            check(3, "npm run lint", DeliveryJobStatus::Failed, 4),
            check(4, "npm run removed", DeliveryJobStatus::Finished, 9),
        ];
        let jobs: Vec<&DeliveryJob> = jobs.iter().collect();
        let checks = ["npm run unit-tests".to_string(), "npm run lint".to_string()];
        assert_eq!(
            latest_check_runs(&checks, &jobs).as_deref(),
            Some("Their latest runs, on earlier code: `npm run unit-tests` passed in 11 min; `npm run lint` failed in 4 s.")
        );
        assert_eq!(latest_check_runs(&checks, &[]), None);
    }
}
