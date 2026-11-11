//! The scheduler's rules for queued jobs, in one place: which start now and,
//! for the rest, what holds each one. `select_jobs` starts the first; the
//! overview and read_job say the second, so a job never sits `queued`
//! without a reason.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use super::job_process;
use super::model::{
    DeliveryAccess, DeliveryAgent, DeliveryApprovalStatus, DeliveryJob, DeliveryJobStatus,
    DeliveryLease, DeliveryLeaseState, DeliveryLeaseWaiter, DeliveryQueueWait, DeliveryQuota,
    DeliveryRung, DeliveryStageAgent, DeliveryStageFallback, DeliveryTask,
};
use super::phase::current_setup;
use super::service::agent_name;
use super::service::{
    DeliveryService, RepoEnvironment, CHECKS_ROLE, ENVIRONMENT_FRESH_MS, SETUP_ROLE,
};
use super::store::DeliveryStore;
use super::DeliveryError;

/// How many of Tinto's checks run at once: they compete for the same cores,
/// so few, but not one, or every task's checks queue behind the others'.
const CHECKS_AT_ONCE: usize = 2;

/// A job queued longer than this says in its `job_started` event how long
/// it waited and what held it last: `queued_reason` is only ever computed
/// when read, so without it a long wait leaves no trace.
const LONG_WAIT_MS: u64 = 5 * 60_000;

/// One of Tinto's own checks: a verification command run on a stage's
/// result.
fn is_check(job: &DeliveryJob) -> bool {
    job.agent == DeliveryAgent::Shell && job.role == CHECKS_ROLE
}

/// One of the task's setup commands.
fn is_setup(job: &DeliveryJob) -> bool {
    job.agent == DeliveryAgent::Shell && job.role == SETUP_ROLE
}

/// Where the agents' quotas are kept, so a restart still knows them.
const QUOTAS_SETTING: &str = "agent_quotas";

/// The latest quota each agent's jobs reported.
pub(super) fn quotas(store: &DeliveryStore) -> Result<Vec<DeliveryQuota>, DeliveryError> {
    Ok(store.setting(QUOTAS_SETTING)?.unwrap_or_default())
}

/// What holds a queued job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Hold {
    /// Another job of its task runs: one at a time per task.
    TaskBusy(String),
    /// The user paused its batch.
    BatchPaused,
    /// Its task's setup has not passed.
    Setup,
    /// It needs the environment: what fails, or `None` while no check
    /// is fresh.
    Environment(Option<String>),
    /// As many jobs run as the capacity allows.
    Capacity,
    /// As many of Tinto's checks run as `CHECKS_AT_ONCE` allows: the first
    /// of them.
    CheckRunning(String),
    /// Its exclusive resource is held, or others asked for it first.
    Lease(String),
    /// Its agent's account is near its usage limit, and why its stage's
    /// fallback cannot run it instead, if the stage has one.
    Quota(DeliveryQuota, Option<String>),
}

/// Everything the scheduler reads.
pub(super) struct Queue {
    pub tasks: Vec<DeliveryTask>,
    /// Every job, oldest first.
    pub jobs: Vec<DeliveryJob>,
    pub leases: Vec<DeliveryLease>,
    pub capacity: usize,
    /// Tasks whose setup has not passed.
    unprepared: HashSet<String>,
    /// Tasks of a paused batch.
    held: HashSet<String>,
    /// Tasks about to merge: their merge asked, or their PR approved.
    merging: HashSet<String>,
    environments: HashMap<PathBuf, RepoEnvironment>,
    quotas: Vec<DeliveryQuota>,
    /// The settings' `quota_stop_percent`.
    quota_stop: u32,
    /// The stages of each task's repo, for their fallbacks.
    stages: HashMap<PathBuf, BTreeMap<String, DeliveryStageAgent>>,
}

impl Queue {
    pub(super) fn read(
        store: &DeliveryStore,
        environments: HashMap<PathBuf, RepoEnvironment>,
    ) -> Result<Self, DeliveryError> {
        let tasks = store.tasks()?;
        let jobs = store.jobs()?;
        let settings = store.settings()?;
        let capacity = settings.effective_capacity(tasks.len()) as usize;
        let mut stages = HashMap::new();
        for task in &tasks {
            if !stages.contains_key(&task.repo) {
                stages.insert(task.repo.clone(), store.repo_settings(&task.repo)?.stages);
            }
        }
        // A task's stages and checks wait for its setup to pass.
        let unprepared = tasks
            .iter()
            .filter(|task| {
                let task_jobs: Vec<&DeliveryJob> =
                    jobs.iter().filter(|job| job.task_id == task.id).collect();
                current_setup(&task.setup, &task_jobs)
                    .iter()
                    .any(|job| job.status != DeliveryJobStatus::Finished)
            })
            .map(|task| task.id.clone())
            .collect();
        // Tasks of a paused batch start nothing new.
        let paused_runs: HashSet<String> = store
            .runs()?
            .into_iter()
            .filter(|run| run.paused)
            .map(|run| run.id)
            .collect();
        let held = tasks
            .iter()
            .filter(|task| {
                task.run_id
                    .as_ref()
                    .is_some_and(|run| paused_runs.contains(run))
            })
            .map(|task| task.id.clone())
            .collect();
        // What is ready to merge goes before keeping other branches checked.
        let mut merging: HashSet<String> = store
            .approvals()?
            .into_iter()
            .filter(|approval| {
                approval.rung == DeliveryRung::Merge
                    && matches!(
                        approval.status,
                        DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                    )
            })
            .map(|approval| approval.task_id)
            .collect();
        merging.extend(
            store
                .prs()?
                .into_iter()
                .filter(|pr| pr.is_open() && pr.is_approved())
                .map(|pr| pr.task_id),
        );
        Ok(Self {
            quotas: quotas(store)?,
            quota_stop: settings.quota_stop_percent,
            stages,
            leases: store.leases()?,
            tasks,
            jobs,
            capacity,
            unprepared,
            held,
            merging,
            environments,
        })
    }

    /// The last "Entorno" check of the job's repo.
    pub(super) fn environment_of(&self, job: &DeliveryJob) -> Option<&RepoEnvironment> {
        self.tasks
            .iter()
            .find(|task| task.id == job.task_id)
            .and_then(|task| self.environments.get(&task.repo))
    }

    /// Each queued job, those of tasks about to merge first, then oldest
    /// first (a task's checks quickest first), with what holds it (`None`: it starts now): one job per task at
    /// a time, setup first, the environment for
    /// those that need it, up to the capacity, and exclusive resources in
    /// the order they were asked for. The resources it takes or queues for
    /// are updated in `leases`.
    pub(super) fn plan(&self, now: u64) -> (Vec<(String, Option<Hold>)>, Vec<DeliveryLease>) {
        let mut leases = self.leases.clone();
        let mut running = self
            .jobs
            .iter()
            .filter(|job| job.status.is_active())
            .count();
        let mut busy: HashMap<String, String> = self
            .jobs
            .iter()
            .filter(|job| job.status.is_active())
            .map(|job| (job.task_id.clone(), job.id.clone()))
            .collect();
        let mut checking: Vec<String> = self
            .jobs
            .iter()
            .filter(|job| job.status.is_active() && is_check(job))
            .map(|job| job.id.clone())
            .collect();
        let mut queued: Vec<&DeliveryJob> = self
            .jobs
            .iter()
            .filter(|job| job.status == DeliveryJobStatus::Queued)
            .collect();
        queued.sort_by_key(|job| !self.merging.contains(&job.task_id));
        // Within each task's batch of checks the quickest goes first, so a
        // build or lint that fails reports before a long suite ends. Only
        // there: tasks keep their turn, so no task's long check waits behind
        // newer tasks' short ones, and a batch is finite. A command with no
        // finished run yet goes first: it is measured once, and a new check
        // is never left last for good.
        let mut checks: HashMap<&str, Vec<usize>> = HashMap::new();
        for (at, job) in queued.iter().enumerate() {
            if is_check(job) {
                checks.entry(job.task_id.as_str()).or_default().push(at);
            }
        }
        for places in checks.values() {
            let mut batch: Vec<&DeliveryJob> = places.iter().map(|&at| queued[at]).collect();
            batch.sort_by_key(|job| self.last_run_ms(job));
            for (&at, job) in places.iter().zip(batch) {
                queued[at] = job;
            }
        }
        let mut plan = Vec::new();
        for job in queued {
            let hold = self.hold(job, &busy, running, &checking, &mut leases, now);
            if hold.is_none() {
                busy.insert(job.task_id.clone(), job.id.clone());
                running += 1;
                if is_check(job) {
                    checking.push(job.id.clone());
                }
            }
            plan.push((job.id.clone(), hold));
        }
        (plan, leases)
    }

    /// Keeps `notes` from one scheduling tick to the next: for each job
    /// `plan` holds, why; for each it starts after a long wait, what its
    /// `job_started` event says, from the reason the tick before left.
    /// Notes of jobs no longer queued nor about to start are dropped.
    pub(super) fn note_waits(
        &self,
        plan: &[(String, Option<Hold>)],
        now: u64,
        notes: &mut HashMap<String, String>,
    ) {
        notes.retain(|id, _| {
            self.job(id).is_some_and(|job| {
                matches!(
                    job.status,
                    DeliveryJobStatus::Queued | DeliveryJobStatus::Pending
                )
            })
        });
        for (id, hold) in plan {
            let Some(job) = self.job(id) else { continue };
            match hold {
                Some(hold) => {
                    notes.insert(id.clone(), self.explain(job, hold, now).0);
                }
                None => {
                    let last = notes.remove(id);
                    let waited = now.saturating_sub(job.created_at_ms);
                    if waited > LONG_WAIT_MS {
                        let why = match last {
                            Some(reason) => format!("lo último que lo retenía: {reason}"),
                            None => "sin motivo anotado desde que Tinto arrancó".to_string(),
                        };
                        notes.insert(
                            id.clone(),
                            format!("esperó {} min en la cola; {why}", waited / 60_000),
                        );
                    }
                }
            }
        }
    }

    fn hold(
        &self,
        job: &DeliveryJob,
        busy: &HashMap<String, String>,
        running: usize,
        checking: &[String],
        leases: &mut Vec<DeliveryLease>,
        now: u64,
    ) -> Option<Hold> {
        let setup = is_setup(job);
        // A task's setup commands prepare separate parts of its worktree (one
        // package's install each), so they run beside each other.
        if let Some(other) = busy.get(&job.task_id) {
            if !(setup && self.job(other).is_some_and(is_setup)) {
                return Some(Hold::TaskBusy(other.clone()));
            }
        }
        if self.held.contains(&job.task_id) {
            return Some(Hold::BatchPaused);
        }
        if self.unprepared.contains(&job.task_id) && !setup {
            return Some(Hold::Setup);
        }
        // Jobs already running go on; no new one starts that could fail
        // mid-stage when the account runs out, unless on its stage's fallback.
        if let Some(Err(hold)) = self.over_quota(job, now) {
            return Some(hold);
        }
        if job.needs_environment {
            let state = self.environment_of(job);
            let fresh = state.is_some_and(|state| {
                state
                    .checked_at_ms
                    .is_some_and(|at| now.saturating_sub(at) < ENVIRONMENT_FRESH_MS)
            });
            match state.and_then(|state| state.problem.clone()) {
                Some(problem) => return Some(Hold::Environment(Some(problem))),
                None if !fresh => return Some(Hold::Environment(None)),
                None => {}
            }
        }
        if is_check(job) && checking.len() >= CHECKS_AT_ONCE {
            return Some(Hold::CheckRunning(checking[0].clone()));
        }
        if running >= self.capacity {
            return Some(Hold::Capacity);
        }
        let name = job.lease.clone()?;
        let at = match leases.iter().position(|lease| lease.name == name) {
            Some(at) => at,
            None => {
                leases.push(DeliveryLease {
                    name: name.clone(),
                    state: DeliveryLeaseState::Free,
                    generation: 0,
                    holder_job_id: None,
                    holder_task_id: None,
                    acquired_at_ms: None,
                    note: None,
                    queue: Vec::new(),
                    last_task_id: None,
                    prepared: None,
                });
                leases.len() - 1
            }
        };
        let lease = &mut leases[at];
        let first_in_line = lease
            .queue
            .first()
            .is_none_or(|waiter| waiter.job_id == job.id);
        // The job resuming a paused one takes over what it held.
        let inherited = lease.state == DeliveryLeaseState::Active
            && job.resumes.is_some()
            && lease.holder_job_id == job.resumes;
        // A later attempt of the stage that left it in quarantine (continued
        // or retried) goes on in the environment it left: the same task and
        // stage, so nobody else's work is at stake. Others still wait.
        let quarantine_holder = lease
            .holder_job_id
            .as_deref()
            .filter(|_| lease.state == DeliveryLeaseState::Quarantined)
            .and_then(|holder| self.job(holder));
        if let Some(holder) = quarantine_holder.filter(|holder| {
            holder.task_id == job.task_id
                && holder.role == job.role
                && holder.created_at_ms < job.created_at_ms
        }) {
            let left = lease.note.take().unwrap_or_default();
            lease.queue.retain(|waiter| waiter.job_id != job.id);
            lease.state = DeliveryLeaseState::Active;
            lease.note = Some(
                format!(
                    "{} lo retomó en cuarentena, tras el intento {} de la misma etapa. {left}",
                    job.id, holder.attempt
                )
                .trim()
                .to_string(),
            );
            lease.holder_job_id = Some(job.id.clone());
            lease.holder_task_id = Some(job.task_id.clone());
            lease.last_task_id = Some(job.task_id.clone());
            lease.acquired_at_ms = Some(now);
            lease.generation += 1;
            return None;
        }
        // The environment may still be built for the task that held it last
        // (a QA stack on its branch): while that task has a job ready for
        // it, the job goes first and nothing is rebuilt. Others take their
        // turn as usual; the resource never waits empty for it.
        let stack_task = lease.last_task_id.clone();
        let ours = stack_task.as_deref() == Some(job.task_id.as_str());
        let yields = !ours
            && stack_task.as_deref().is_some_and(|task| {
                !busy.contains_key(task)
                    && !self.held.contains(task)
                    && !self.unprepared.contains(task)
                    && self.jobs.iter().any(|other| {
                        other.status == DeliveryJobStatus::Queued
                            && other.task_id == task
                            && other.lease.as_deref() == Some(name.as_str())
                    })
            });
        let free = lease.state == DeliveryLeaseState::Free && (first_in_line || ours) && !yields;
        if free || inherited {
            lease.queue.retain(|waiter| waiter.job_id != job.id);
            lease.state = DeliveryLeaseState::Active;
            lease.holder_job_id = Some(job.id.clone());
            lease.holder_task_id = Some(job.task_id.clone());
            lease.last_task_id = Some(job.task_id.clone());
            lease.acquired_at_ms = Some(now);
            lease.generation += 1;
            lease.note = None;
            return None;
        }
        if !lease.queue.iter().any(|waiter| waiter.job_id == job.id) {
            lease.queue.push(DeliveryLeaseWaiter {
                job_id: job.id.clone(),
                task_id: job.task_id.clone(),
                requested_at_ms: now,
            });
        }
        Some(Hold::Lease(name))
    }

    pub(super) fn job(&self, job_id: &str) -> Option<&DeliveryJob> {
        self.jobs.iter().find(|job| job.id == job_id)
    }

    /// The quota that stops new jobs of the agent now.
    fn stopping(&self, agent: DeliveryAgent, now: u64) -> Option<&DeliveryQuota> {
        self.quotas
            .iter()
            .find(|quota| quota.agent == agent && quota.stops(self.quota_stop, now))
    }

    /// Whether its agent's quota stops the job now and, if so, the stage's
    /// fallback it starts on instead (`Ok`) or the hold it waits on.
    pub(super) fn over_quota(
        &self,
        job: &DeliveryJob,
        now: u64,
    ) -> Option<Result<(&DeliveryQuota, &DeliveryStageFallback), Hold>> {
        let quota = self.stopping(job.agent, now)?;
        let Some(fallback) = self
            .tasks
            .iter()
            .find(|task| task.id == job.task_id)
            .and_then(|task| {
                self.stages
                    .get(&task.repo)?
                    .get(&job.role)?
                    .fallback
                    .as_ref()
            })
        else {
            return Some(Err(Hold::Quota(quota.clone(), None)));
        };
        let skipped = if fallback.agent == job.agent {
            Some("es el mismo agente".to_string())
        } else if job.resumes.is_some() {
            Some(format!("continúa una sesión de {}", agent_name(job.agent)))
        } else if fallback.agent == DeliveryAgent::Codex
            && job.needs_environment
            && job.access == DeliveryAccess::Workspace
        {
            Some("el trabajo necesita el entorno y el sandbox de Codex no llega a /var/run/docker.sock".to_string())
        } else {
            self.stopping(fallback.agent, now).map(|other| {
                format!(
                    "su cuenta lleva el {}% de su límite ({})",
                    other.utilization, other.window
                )
            })
        };
        Some(match skipped {
            Some(why) => Err(Hold::Quota(
                quota.clone(),
                Some(format!(
                    "su alternativo, {}, no sirve: {why}",
                    agent_name(fallback.agent)
                )),
            )),
            None => Ok((quota, fallback)),
        })
    }

    /// The hold in words, and whether what it waits for looks stuck.
    fn explain(&self, job: &DeliveryJob, hold: &Hold, now: u64) -> (String, Option<String>) {
        match hold {
            Hold::TaskBusy(other_id) => {
                let other = self.job(other_id);
                let what = match other.map(|other| other.role.as_str()) {
                    Some(CHECKS_ROLE) => "terminen las comprobaciones".to_string(),
                    Some(SETUP_ROLE) => "termine la preparación".to_string(),
                    Some(role) => format!("termine la etapa {role}"),
                    None => "termine otro trabajo".to_string(),
                };
                (
                    format!("espera a que {what} (trabajo {other_id}) de esta tarea"),
                    other.and_then(|other| self.stuck(other, now)),
                )
            }
            Hold::BatchPaused => ("el lote está en pausa".into(), None),
            Hold::Quota(quota, fallback) => (
                format!(
                    "la cuenta de {} lleva el {}% de su límite ({}): no empieza trabajos nuevos hasta que se renueve, en {} min{}",
                    agent_name(quota.agent),
                    quota.utilization,
                    quota.window,
                    quota.resets_at_ms.saturating_sub(now).div_ceil(60_000),
                    fallback.as_ref().map(|why| format!("; {why}")).unwrap_or_default()
                ),
                None,
            ),
            Hold::Setup => {
                let task_jobs: Vec<&DeliveryJob> = self
                    .jobs
                    .iter()
                    .filter(|other| other.task_id == job.task_id)
                    .collect();
                let setup = self
                    .tasks
                    .iter()
                    .find(|task| task.id == job.task_id)
                    .map(|task| current_setup(&task.setup, &task_jobs))
                    .unwrap_or_default();
                match setup
                    .iter()
                    .find(|other| other.status != DeliveryJobStatus::Finished)
                {
                    Some(other) if other.status.is_open() => (
                        format!("espera a la preparación (trabajo {}) de esta tarea", other.id),
                        self.stuck(other, now),
                    ),
                    Some(other) => (
                        format!(
                            "espera a la preparación de esta tarea, que falló (trabajo {}): reintenta con retry_job",
                            other.id
                        ),
                        None,
                    ),
                    None => ("espera a la preparación de esta tarea".into(), None),
                }
            }
            Hold::Environment(Some(problem)) => (format!("espera al entorno: {problem}"), None),
            Hold::Environment(None) => ("espera a la comprobación del entorno".into(), None),
            Hold::CheckRunning(other_id) => (
                format!(
                    "espera a que termine otra comprobación (trabajo {other_id}): corren {CHECKS_AT_ONCE} a la vez para no repartirse los núcleos"
                ),
                self.job(other_id).and_then(|other| self.stuck(other, now)),
            ),
            Hold::Capacity => {
                let running = self
                    .jobs
                    .iter()
                    .filter(|other| other.status.is_active())
                    .count();
                (
                    format!(
                        "capacidad llena: corren {running} de {} trabajos a la vez",
                        self.capacity
                    ),
                    None,
                )
            }
            Hold::Lease(name) => {
                let lease = self.leases.iter().find(|lease| lease.name == *name);
                let holder = lease
                    .filter(|lease| lease.state == DeliveryLeaseState::Active)
                    .and_then(|lease| lease.holder_job_id.as_deref());
                match (lease.map(|lease| lease.state), holder) {
                    (Some(DeliveryLeaseState::Quarantined), _) => {
                        (format!("el recurso {name} está en cuarentena"), None)
                    }
                    (_, Some(holder)) => {
                        let warning = match self.job(holder) {
                            Some(other) if !other.status.is_open() => Some(format!(
                                "el trabajo {holder} ya terminó y el recurso {name} sigue tomado"
                            )),
                            Some(other) => self.stuck(other, now),
                            None => Some(format!(
                                "el trabajo {holder}, que tiene el recurso {name}, ya no existe"
                            )),
                        };
                        (
                            format!("espera el recurso {name}, que tiene el trabajo {holder}"),
                            warning,
                        )
                    }
                    _ => (format!("espera su turno para el recurso {name}"), None),
                }
            }
        }
    }

    /// Why a running job looks stuck: its process is gone, or it runs far
    /// beyond its usual time (or past its own limit).
    fn stuck(&self, job: &DeliveryJob, now: u64) -> Option<String> {
        if job.status != DeliveryJobStatus::Started {
            return None;
        }
        if let (Some(pid), Some(started)) = (job.pid, job.process_started.as_deref()) {
            if !job_process::alive(pid, started) {
                return Some(format!("el proceso del trabajo {} ya no existe", job.id));
            }
        }
        let elapsed = now.saturating_sub(job.started_at_ms?);
        let minutes = |ms: u64| ms / 60_000;
        let limit = u64::from(job.timeout_minutes) * 60_000;
        if limit > 0 && elapsed > limit {
            return Some(format!(
                "el trabajo {} lleva {} min, más que su límite de {} min",
                job.id,
                minutes(elapsed),
                job.timeout_minutes
            ));
        }
        let usual = self.usual_ms(job)?;
        (elapsed >= usual * 3 && elapsed >= usual + 10 * 60_000).then(|| {
            format!(
                "el trabajo {} lleva {} min; suele tardar {} min",
                job.id,
                minutes(elapsed),
                minutes(usual).max(1)
            )
        })
    }

    /// How long the last finished run of the check's command took in its
    /// repo; `None` if it never finished there.
    fn last_run_ms(&self, check: &DeliveryJob) -> Option<u64> {
        let repo = |task_id: &str| {
            self.tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| &task.repo)
        };
        let ours = repo(&check.task_id);
        self.jobs
            .iter()
            .rev()
            .filter(|other| {
                other.status == DeliveryJobStatus::Finished
                    && is_check(other)
                    && other.prompt == check.prompt
                    && repo(&other.task_id) == ours
            })
            .find_map(|other| Some(other.ended_at_ms?.saturating_sub(other.started_at_ms?)))
    }

    /// How long jobs like this one (the same stage, or the same command
    /// for Tinto's own) took the last times they ended well: the median of
    /// the last five.
    fn usual_ms(&self, job: &DeliveryJob) -> Option<u64> {
        let mut times: Vec<u64> = self
            .jobs
            .iter()
            .rev()
            .filter(|other| {
                other.status == DeliveryJobStatus::Finished
                    && other.role == job.role
                    && (job.agent != DeliveryAgent::Shell || other.prompt == job.prompt)
            })
            .filter_map(|other| Some(other.ended_at_ms?.saturating_sub(other.started_at_ms?)))
            .take(5)
            .collect();
        times.sort_unstable();
        times.get(times.len() / 2).copied()
    }
}

impl DeliveryService {
    /// The latest quota each agent reported.
    pub fn quotas(&self) -> Result<Vec<DeliveryQuota>, DeliveryError> {
        quotas(&*self.store()?)
    }

    /// Keeps the latest quota an agent's job or console reported.
    pub(crate) fn record_quota(&self, mut quota: DeliveryQuota) {
        let Ok(store) = self.store() else { return };
        let Ok(mut all) = quotas(&store) else { return };
        if let Some(at) = all.iter().position(|known| known.agent == quota.agent) {
            let known = &all[at];
            if known.utilization == quota.utilization && known.resets_at_ms == quota.resets_at_ms {
                return;
            }
            all.remove(at);
        }
        quota.seen_at_ms = super::now_ms();
        all.push(quota);
        if let Err(error) = store.set_setting(QUOTAS_SETTING, &all) {
            eprintln!("tinto: delivery could not record an agent's quota: {error}");
        }
        drop(store);
        self.notify();
    }

    /// Forgets the quotas that held jobs and have renewed since, and starts
    /// what they held: nothing else would, with every agent job waiting.
    pub(super) fn release_renewed_quotas(&self) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let now = super::now_ms();
        let percent = store.settings()?.quota_stop_percent;
        let (renewed, kept): (Vec<DeliveryQuota>, Vec<DeliveryQuota>) = quotas(&store)?
            .into_iter()
            .partition(|quota| quota.utilization >= percent && now >= quota.resets_at_ms);
        if renewed.is_empty() {
            return Ok(());
        }
        store.set_setting(QUOTAS_SETTING, &kept)?;
        drop(store);
        self.tick();
        Ok(())
    }

    /// Why each queued job has not started, by the scheduler's own rules.
    pub fn queue_waits(&self) -> Result<BTreeMap<String, DeliveryQueueWait>, DeliveryError> {
        let queue = Queue::read(&*self.store()?, self.environments())?;
        let now = super::now_ms();
        let (plan, _) = queue.plan(now);
        let mut waits = BTreeMap::new();
        for (job_id, hold) in plan {
            let Some(job) = queue.job(&job_id) else {
                continue;
            };
            let (reason, warning) = match &hold {
                Some(hold) => queue.explain(job, hold, now),
                None => ("empieza ahora".to_string(), None),
            };
            waits.insert(
                job_id,
                DeliveryQueueWait {
                    reason,
                    since_ms: job.created_at_ms,
                    warning,
                },
            );
        }
        Ok(waits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::adapters::tests::{sample_job, sample_task};
    use crate::delivery::model::QUOTA_STOP_PERCENT;

    fn job(id: &str, task: &str, agent: DeliveryAgent) -> DeliveryJob {
        let mut job = sample_job(agent);
        job.id = id.into();
        job.task_id = task.into();
        job
    }

    fn quota(agent: DeliveryAgent, utilization: u32) -> DeliveryQuota {
        DeliveryQuota {
            agent,
            window: "five_hour".into(),
            utilization,
            resets_at_ms: 10 * 60_000,
            seen_at_ms: 0,
        }
    }

    fn queue(jobs: Vec<DeliveryJob>, quotas: Vec<DeliveryQuota>, quota_stop: u32) -> Queue {
        let tasks = jobs
            .iter()
            .map(|job| {
                let mut task = sample_task();
                task.id = job.task_id.clone();
                task
            })
            .collect();
        Queue {
            tasks,
            jobs,
            leases: Vec::new(),
            capacity: 8,
            unprepared: HashSet::new(),
            held: HashSet::new(),
            merging: HashSet::new(),
            environments: HashMap::new(),
            quotas,
            quota_stop,
            stages: HashMap::new(),
        }
    }

    fn held(queue: &Queue, now: u64) -> Vec<String> {
        queue
            .plan(now)
            .0
            .into_iter()
            .filter(|(_, hold)| hold.is_some())
            .map(|(id, _)| id)
            .collect()
    }

    #[test]
    fn an_agent_at_the_settings_share_of_its_limit_starts_no_new_job_until_it_renews() {
        let jobs = || {
            vec![
                job("claude", "t1", DeliveryAgent::Claude),
                job("codex", "t2", DeliveryAgent::Codex),
                job("check", "t3", DeliveryAgent::Shell),
            ]
        };
        // Only Claude's job waits, from the share the settings give; 100
        // never holds one.
        for (quota_stop, utilization, waits) in [
            (95, 94, false),
            (95, 96, true),
            (99, 98, false),
            (99, 99, true),
            (100, 100, false),
        ] {
            let near = queue(
                jobs(),
                vec![quota(DeliveryAgent::Claude, utilization)],
                quota_stop,
            );
            let expected: &[&str] = if waits { &["claude"] } else { &[] };
            assert_eq!(held(&near, 0), expected, "{quota_stop}% at {utilization}");
            // Once the window renewed, it starts.
            assert!(held(&near, 10 * 60_000).is_empty());
        }
        let near = queue(jobs(), vec![quota(DeliveryAgent::Claude, 99)], 99);
        let hold = Hold::Quota(quota(DeliveryAgent::Claude, 99), None);
        assert_eq!(
            near.explain(&near.jobs[0], &hold, 0).0,
            "la cuenta de Claude Code lleva el 99% de su límite (five_hour): no empieza trabajos nuevos hasta que se renueve, en 10 min"
        );
    }

    #[test]
    fn a_job_over_its_quota_starts_on_its_stages_fallback_when_it_can_run_it() {
        let fallback = DeliveryStageFallback {
            agent: DeliveryAgent::Codex,
            model: Some("gpt-6-sol".into()),
            reasoning: None,
        };
        let skipped = |why: &str| Some(format!("su alternativo, Codex, no sirve: {why}"));
        // What the job is, how much Codex's account used, why it is skipped.
        type Case = (&'static str, fn(&mut DeliveryJob), u32, Option<String>);
        let cases: [Case; 4] = [
            ("starts on it", |_| {}, 0, None),
            (
                "needs the environment",
                |job| job.needs_environment = true,
                0,
                skipped("el trabajo necesita el entorno y el sandbox de Codex no llega a /var/run/docker.sock"),
            ),
            (
                "continues a session",
                |job| job.resumes = Some("j0".into()),
                0,
                skipped("continúa una sesión de Claude Code"),
            ),
            (
                "is over its own quota",
                |_| {},
                99,
                skipped("su cuenta lleva el 99% de su límite (five_hour)"),
            ),
        ];
        for (case, change, codex_used, why) in cases {
            let mut claude = job("claude", "t1", DeliveryAgent::Claude);
            claude.role = "tests".into();
            change(&mut claude);
            let mut near = queue(
                vec![claude],
                vec![
                    quota(DeliveryAgent::Claude, 99),
                    quota(DeliveryAgent::Codex, codex_used),
                ],
                99,
            );
            let stage = DeliveryStageAgent {
                agent: DeliveryAgent::Claude,
                model: None,
                reasoning: None,
                timeout_minutes: None,
                fallback: Some(fallback.clone()),
            };
            near.stages.insert(
                near.tasks[0].repo.clone(),
                BTreeMap::from([("tests".to_string(), stage)]),
            );
            let job = &near.jobs[0];
            match why {
                None => {
                    assert!(held(&near, 0).is_empty(), "{case}");
                    let started = near.over_quota(job, 0).unwrap().unwrap();
                    assert_eq!(started.1, &fallback, "{case}");
                }
                Some(why) => {
                    // It stays queued, saying why its fallback was skipped.
                    let hold = near.over_quota(job, 0).unwrap().unwrap_err();
                    assert_eq!(held(&near, 0), ["claude"], "{case}");
                    let (reason, _) = near.explain(job, &hold, 0);
                    assert!(reason.ends_with(&format!("; {why}")), "{case}: {reason}");
                }
            }
        }
    }

    #[test]
    fn checks_of_a_task_about_to_merge_go_before_older_ones() {
        let check = |task: &str| {
            let mut job = sample_job(DeliveryAgent::Shell);
            job.id = task.into();
            job.task_id = task.into();
            job.role = CHECKS_ROLE.into();
            job
        };
        let queue = Queue {
            tasks: Vec::new(),
            jobs: vec![check("older"), check("old"), check("merging")],
            leases: Vec::new(),
            capacity: 8,
            unprepared: HashSet::new(),
            held: HashSet::new(),
            merging: HashSet::from(["merging".to_string()]),
            environments: HashMap::new(),
            quotas: Vec::new(),
            quota_stop: QUOTA_STOP_PERCENT,
            stages: HashMap::new(),
        };
        let started: Vec<String> = queue
            .plan(0)
            .0
            .into_iter()
            .filter(|(_, hold)| hold.is_none())
            .map(|(id, _)| id)
            .collect();
        assert_eq!(started, ["merging", "older"]);
    }

    #[test]
    fn a_tasks_checks_start_quickest_first_and_new_commands_before_any() {
        let check = |id: &str, task: &str, command: &str, took: Option<u64>| {
            let mut job = job(id, task, DeliveryAgent::Shell);
            job.role = CHECKS_ROLE.into();
            job.prompt = command.into();
            if let Some(took) = took {
                job.status = DeliveryJobStatus::Finished;
                job.started_at_ms = Some(0);
                job.ended_at_ms = Some(took);
            }
            job
        };
        let mut queue = queue(
            vec![
                check("old-suite", "done", "npm test", Some(5 * 60_000)),
                check("old-build", "done", "cargo build", Some(6_000)),
                // Another repo's run of the suite is not this one's.
                check("elsewhere", "other", "npm test", Some(1_000)),
                check("suite", "t1", "npm test", None),
                check("build", "t1", "cargo build", None),
                check("new", "t1", "npm run lint", None),
            ],
            Vec::new(),
            QUOTA_STOP_PERCENT,
        );
        queue.tasks[2].repo = PathBuf::from("/other");
        // One job of the task at a time: each ends before the next starts.
        let mut order = Vec::new();
        while let Some((id, _)) = queue.plan(0).0.into_iter().find(|(_, h)| h.is_none()) {
            let job = queue.jobs.iter_mut().find(|job| job.id == id).unwrap();
            job.status = DeliveryJobStatus::Finished;
            order.push(id);
        }
        assert_eq!(order, ["new", "build", "suite"]);
    }

    #[test]
    fn a_job_started_after_a_long_wait_keeps_what_held_it_last() {
        let minute = 60_000;
        let mut long = job("long", "t1", DeliveryAgent::Claude);
        long.created_at_ms = 0;
        let mut short = job("short", "t2", DeliveryAgent::Claude);
        short.created_at_ms = 9 * minute;
        let mut unseen = job("unseen", "t3", DeliveryAgent::Claude);
        unseen.created_at_ms = 0;
        let mut gone = job("gone", "t4", DeliveryAgent::Claude);
        gone.status = DeliveryJobStatus::Cancelled;
        let mut queue = queue(vec![long, short, unseen, gone], Vec::new(), 100);
        let mut notes = HashMap::from([("gone".to_string(), "old".to_string())]);
        // A tick that holds the first two, before "unseen" was known.
        queue.capacity = 0;
        let (plan, _) = queue.plan(minute);
        queue.note_waits(&plan[..2], minute, &mut notes);
        assert_eq!(notes.len(), 2, "{notes:?}");
        // The tick that starts them all.
        queue.capacity = 8;
        let (plan, _) = queue.plan(10 * minute);
        queue.note_waits(&plan, 10 * minute, &mut notes);
        assert_eq!(
            notes,
            HashMap::from([
                (
                    "long".to_string(),
                    "esperó 10 min en la cola; lo último que lo retenía: capacidad llena: corren 0 de 0 trabajos a la vez".to_string()
                ),
                (
                    "unseen".to_string(),
                    "esperó 10 min en la cola; sin motivo anotado desde que Tinto arrancó".to_string()
                ),
            ])
        );
    }
}
