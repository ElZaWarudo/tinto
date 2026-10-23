//! The scheduler's rules for queued jobs, in one place: which start now and,
//! for the rest, what holds each one. `select_jobs` starts the first; the
//! overview and read_job say the second, so a job never sits `queued`
//! without a reason.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use super::job_process;
use super::model::{
    DeliveryAgent, DeliveryJob, DeliveryJobStatus, DeliveryLease, DeliveryLeaseState,
    DeliveryLeaseWaiter, DeliveryQueueWait, DeliveryTask,
};
use super::phase::current_setup;
use super::service::{
    DeliveryService, RepoEnvironment, CHECKS_ROLE, ENVIRONMENT_FRESH_MS, SETUP_ROLE,
};
use super::store::DeliveryStore;
use super::DeliveryError;

/// One of Tinto's own checks: a verification command run on a stage's
/// result.
fn is_check(job: &DeliveryJob) -> bool {
    job.agent == DeliveryAgent::Shell && job.role == CHECKS_ROLE
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
    /// Another of Tinto's checks runs: they compete for the same cores, so
    /// one at a time.
    CheckRunning(String),
    /// Its exclusive resource is held, or others asked for it first.
    Lease(String),
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
    environments: HashMap<PathBuf, RepoEnvironment>,
}

impl Queue {
    pub(super) fn read(
        store: &DeliveryStore,
        environments: HashMap<PathBuf, RepoEnvironment>,
    ) -> Result<Self, DeliveryError> {
        let tasks = store.tasks()?;
        let jobs = store.jobs()?;
        let capacity = store.settings()?.effective_capacity(tasks.len()) as usize;
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
        Ok(Self {
            leases: store.leases()?,
            tasks,
            jobs,
            capacity,
            unprepared,
            held,
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

    /// Each queued job, oldest first, with what holds it (`None`: it starts
    /// now): one job per task at a time, setup first, the environment for
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
        let mut checking = self
            .jobs
            .iter()
            .find(|job| job.status.is_active() && is_check(job))
            .map(|job| job.id.clone());
        let mut plan = Vec::new();
        for job in self
            .jobs
            .iter()
            .filter(|job| job.status == DeliveryJobStatus::Queued)
        {
            let hold = self.hold(job, &busy, running, checking.as_deref(), &mut leases, now);
            if hold.is_none() {
                busy.insert(job.task_id.clone(), job.id.clone());
                running += 1;
                if is_check(job) {
                    checking = Some(job.id.clone());
                }
            }
            plan.push((job.id.clone(), hold));
        }
        (plan, leases)
    }

    fn hold(
        &self,
        job: &DeliveryJob,
        busy: &HashMap<String, String>,
        running: usize,
        checking: Option<&str>,
        leases: &mut Vec<DeliveryLease>,
        now: u64,
    ) -> Option<Hold> {
        if let Some(other) = busy.get(&job.task_id) {
            return Some(Hold::TaskBusy(other.clone()));
        }
        if self.held.contains(&job.task_id) {
            return Some(Hold::BatchPaused);
        }
        let setup = job.agent == DeliveryAgent::Shell && job.role == SETUP_ROLE;
        if self.unprepared.contains(&job.task_id) && !setup {
            return Some(Hold::Setup);
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
        if let Some(other) = checking.filter(|_| is_check(job)) {
            return Some(Hold::CheckRunning(other.to_string()));
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
        if (lease.state == DeliveryLeaseState::Free && first_in_line) || inherited {
            lease.queue.retain(|waiter| waiter.job_id != job.id);
            lease.state = DeliveryLeaseState::Active;
            lease.holder_job_id = Some(job.id.clone());
            lease.holder_task_id = Some(job.task_id.clone());
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

    fn job(&self, job_id: &str) -> Option<&DeliveryJob> {
        self.jobs.iter().find(|job| job.id == job_id)
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
                    "espera a que termine otra comprobación (trabajo {other_id}): corren de una en una para no repartirse los núcleos"
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
