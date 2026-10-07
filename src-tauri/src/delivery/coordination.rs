//! Runs and approvals: the coordination the backlog-delivery skill used to
//! keep by hand, made deterministic.
//!
//! - A run groups tasks under one coordinator. Its lock carries a generation
//!   that increases on every takeover; a coordinator acting with an older
//!   generation is fenced.
//! - Approvals climb a ladder (commit → push → PR → Jira), one rung at a
//!   time. Approving a rung never approves the next. Tinto runs the commit
//!   and the push itself; the coordinator runs the PR and Jira steps after
//!   they are approved and reports back.

use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryRun, DeliveryRung, DeliveryTask,
};
use super::service::DeliveryService;
use super::tasks::{self, Place};
use super::{now_ms, DeliveryError};

#[derive(Debug, Clone)]
pub struct NewApproval {
    pub task_id: String,
    pub rung: DeliveryRung,
    pub title: String,
    pub body: String,
    pub requested_by: String,
}

impl DeliveryService {
    // ---- runs ----

    pub fn create_run(
        &self,
        repo: std::path::PathBuf,
        title: &str,
        owner: Option<String>,
    ) -> Result<DeliveryRun, DeliveryError> {
        let now = now_ms();
        let run = DeliveryRun {
            id: format!("run-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]),
            repo,
            title: title.trim().to_string(),
            status: "active".to_string(),
            generation: 1,
            owner,
            created_at_ms: now,
            updated_at_ms: now,
        };
        let store = self.store()?;
        store.put_run(&run)?;
        store.record_event(now, "run_created", None, None, &run.id)?;
        drop(store);
        self.notify();
        Ok(run)
    }

    pub fn run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        self.store()?
            .run(run_id)?
            .ok_or_else(|| DeliveryError::not_found("la ejecución", run_id))
    }

    /// Takes a free run, or confirms the caller already holds it.
    pub fn acquire_run(&self, run_id: &str, owner: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        match run.owner.as_deref() {
            Some(current) if current != owner => Err(DeliveryError::new(
                "run_locked",
                format!(
                    "la ejecución la coordina {current} (generación {}); usa takeover solo si ese coordinador se detuvo",
                    run.generation
                ),
            )),
            Some(_) => Ok(run),
            None => {
                run.owner = Some(owner.to_string());
                run.updated_at_ms = now_ms();
                self.save_run(&run, "run_acquired", owner)?;
                Ok(run)
            }
        }
    }

    /// Replaces the coordinator. The generation increases, so anything the
    /// previous coordinator still sends is fenced.
    pub fn takeover_run(
        &self,
        run_id: &str,
        owner: &str,
        expect_generation: Option<u64>,
        reason: &str,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.active_run(run_id)?;
        if let Some(expected) = expect_generation {
            if expected != run.generation {
                return Err(DeliveryError::new(
                    "fenced",
                    format!("la ejecución ya está en la generación {}", run.generation),
                ));
            }
        }
        run.owner = Some(owner.to_string());
        run.generation += 1;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_takeover", &format!("{owner}: {}", reason.trim()))?;
        Ok(run)
    }

    pub fn release_run(
        &self,
        run_id: &str,
        owner: &str,
        generation: u64,
    ) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.verify_run(run_id, owner, generation)?;
        run.owner = None;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_released", owner)?;
        Ok(run)
    }

    pub fn close_run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let mut run = self.run(run_id)?;
        run.status = "closed".to_string();
        run.owner = None;
        run.generation += 1;
        run.updated_at_ms = now_ms();
        self.save_run(&run, "run_closed", "")?;
        Ok(run)
    }

    /// Fencing check for every coordinator write.
    pub fn verify_run(
        &self,
        run_id: &str,
        owner: &str,
        generation: u64,
    ) -> Result<DeliveryRun, DeliveryError> {
        let run = self.active_run(run_id)?;
        if run.owner.as_deref() != Some(owner) || run.generation != generation {
            return Err(DeliveryError::new(
                "fenced",
                format!(
                    "la ejecución está en la generación {} coordinada por {}; esta escritura se descarta",
                    run.generation,
                    run.owner.as_deref().unwrap_or("nadie")
                ),
            ));
        }
        Ok(run)
    }

    fn active_run(&self, run_id: &str) -> Result<DeliveryRun, DeliveryError> {
        let run = self.run(run_id)?;
        if run.status != "active" {
            return Err(DeliveryError::new(
                "run_closed",
                "la ejecución está cerrada",
            ));
        }
        Ok(run)
    }

    fn save_run(&self, run: &DeliveryRun, event: &str, detail: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        store.put_run(run)?;
        store.record_event(
            run.updated_at_ms,
            event,
            None,
            None,
            &format!("{} {detail}", run.id),
        )?;
        drop(store);
        self.notify();
        Ok(())
    }

    // ---- approvals ----

    pub fn request_approval(
        &self,
        request: NewApproval,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let task = self.task(&request.task_id)?;
        if request.title.trim().is_empty() {
            return Err(DeliveryError::new(
                "invalid_approval",
                "la aprobación necesita el texto exacto (título o mensaje)",
            ));
        }
        {
            let store = self.store()?;
            let open = store.approvals()?.into_iter().any(|approval| {
                approval.task_id == task.id
                    && approval.rung == request.rung
                    && matches!(
                        approval.status,
                        DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                    )
            });
            if open {
                return Err(DeliveryError::new(
                    "approval_exists",
                    "ya hay una aprobación abierta para este paso",
                ));
            }
        }
        rung_prerequisites(&task, request.rung)?;
        let approval = DeliveryApproval {
            id: uuid::Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            rung: request.rung,
            title: request.title.trim().to_string(),
            body: request.body,
            status: DeliveryApprovalStatus::Pending,
            requested_by: request.requested_by,
            requested_at_ms: now_ms(),
            decided_at_ms: None,
            executed_at_ms: None,
            outcome: None,
        };
        let store = self.store()?;
        store.put_approval(&approval)?;
        store.record_event(
            approval.requested_at_ms,
            "approval_requested",
            Some(&task.id),
            None,
            &format!("{:?}", approval.rung).to_ascii_lowercase(),
        )?;
        drop(store);
        self.notify();
        Ok(approval)
    }

    pub fn approval(&self, approval_id: &str) -> Result<DeliveryApproval, DeliveryError> {
        self.store()?
            .approval(approval_id)?
            .ok_or_else(|| DeliveryError::not_found("la aprobación", approval_id))
    }

    /// The user's decision. An approved commit or push runs right away.
    pub fn decide_approval(
        &self,
        approval_id: &str,
        approve: bool,
        note: Option<String>,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.status != DeliveryApprovalStatus::Pending {
            return Err(DeliveryError::new(
                "approval_decided",
                "esta aprobación ya tiene una decisión",
            ));
        }
        approval.decided_at_ms = Some(now_ms());
        if !approve {
            approval.status = DeliveryApprovalStatus::Rejected;
            approval.outcome = note.filter(|note| !note.trim().is_empty());
            self.save_approval(&approval, "approval_rejected")?;
            return Ok(approval);
        }
        approval.status = DeliveryApprovalStatus::Approved;
        if matches!(approval.rung, DeliveryRung::Commit | DeliveryRung::Push) {
            let task = self.task(&approval.task_id)?;
            let place = Place::of(task.distro.as_deref());
            let executed = match approval.rung {
                DeliveryRung::Commit => {
                    tasks::commit_all(place, &task.worktree, &approval.title, &approval.body)
                        .map(|commit| format!("Commit {commit}"))
                }
                _ => tasks::push_branch(place, &task.worktree, &task.branch)
                    .map(|upstream| format!("Publicado en {upstream}")),
            };
            approval.executed_at_ms = Some(now_ms());
            match executed {
                Ok(outcome) => {
                    approval.status = DeliveryApprovalStatus::Executed;
                    approval.outcome = Some(outcome);
                }
                Err(error) => {
                    approval.status = DeliveryApprovalStatus::Failed;
                    approval.outcome = Some(error.message);
                }
            }
        }
        self.save_approval(&approval, "approval_approved")?;
        Ok(approval)
    }

    /// The coordinator (or the user) reports how an approved PR or Jira step
    /// went.
    pub fn complete_approval(
        &self,
        approval_id: &str,
        success: bool,
        outcome: &str,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.status != DeliveryApprovalStatus::Approved {
            return Err(DeliveryError::new(
                "approval_not_approved",
                "solo se informa el resultado de un paso aprobado",
            ));
        }
        approval.status = if success {
            DeliveryApprovalStatus::Executed
        } else {
            DeliveryApprovalStatus::Failed
        };
        approval.executed_at_ms = Some(now_ms());
        approval.outcome = Some(outcome.trim().to_string()).filter(|text| !text.is_empty());
        self.save_approval(&approval, "approval_completed")?;
        Ok(approval)
    }

    fn save_approval(&self, approval: &DeliveryApproval, event: &str) -> Result<(), DeliveryError> {
        let store = self.store()?;
        store.put_approval(approval)?;
        store.record_event(
            now_ms(),
            event,
            Some(&approval.task_id),
            None,
            approval.outcome.as_deref().unwrap_or_default(),
        )?;
        drop(store);
        self.notify();
        Ok(())
    }
}

/// Each rung needs the git state the previous one leaves: something to
/// commit, commits to push, a published branch for the PR.
fn rung_prerequisites(task: &DeliveryTask, rung: DeliveryRung) -> Result<(), DeliveryError> {
    let place = Place::of(task.distro.as_deref());
    match rung {
        DeliveryRung::Commit => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if !state.dirty {
                return Err(DeliveryError::new(
                    "nothing_to_commit",
                    "no hay cambios que confirmar",
                ));
            }
        }
        DeliveryRung::Push => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if state.unpublished == 0 {
                return Err(DeliveryError::new(
                    "nothing_to_push",
                    "no hay commits por publicar",
                ));
            }
        }
        DeliveryRung::Pr => {
            let state = tasks::worktree_state(place, &task.worktree, &task.base_commit)?;
            if !state.has_upstream {
                return Err(DeliveryError::new(
                    "not_pushed",
                    "publica la rama antes de pedir el PR",
                ));
            }
        }
        DeliveryRung::Jira => {}
    }
    Ok(())
}
