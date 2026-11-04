//! Alcance, the scoping stage: the task's plan in fixed sections, kept in
//! versions with the task's contract, and skipping the stage with a reason.

use serde::Deserialize;

use super::model::{
    DeliveryDecision, DeliveryDecisionStatus, DeliveryJob, DeliveryJobStatus, DeliveryResultState,
    DeliveryScopeMode, DeliveryScopePlace, DeliveryScopePlan, DeliveryScopeSkip, DeliveryScopeTest,
    DeliveryTask, SCOPE_ROLE,
};
use super::phase::scope_settled;
use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

/// The stages Alcance comes before.
const AFTER_SCOPE: [&str; 4] = ["tests", "implementation", "review", "qa"];

/// A plan as a coordinator writes it (set_task_state `plan`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PlanInput {
    pub what: String,
    pub places: Vec<PlaceInput>,
    pub risk_level: String,
    pub risks: Vec<String>,
    pub tests: Vec<DeliveryScopeTest>,
    pub out_of_scope: Vec<String>,
    /// What changed from the previous version, in a line.
    pub changed: String,
}

/// A place is a path, or a path with a note.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PlaceInput {
    Path(String),
    Place(DeliveryScopePlace),
}

fn clean(list: Vec<String>) -> Vec<String> {
    list.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

impl DeliveryService {
    /// Records a version of the task's plan. The first is written for the
    /// task's contract version; each later one needs a reason and raises
    /// the contract version (unless the same call already raised it), so a
    /// plan is never rewritten silently and earlier versions stay readable.
    pub fn set_task_plan(
        &self,
        task_id: &str,
        input: PlanInput,
        reason: Option<String>,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let what = input.what.trim().to_string();
        if what.is_empty() {
            return Err(DeliveryError::new(
                "invalid_plan",
                "el plan necesita «what»: qué cambia, en dos o tres líneas",
            ));
        }
        let reason = reason.map(|r| r.trim().to_string()).unwrap_or_default();
        let version = match task.plans.last() {
            None => task.contract_version,
            Some(_) if reason.is_empty() => return Err(DeliveryError::new(
                "plan_reason_required",
                "la tarea ya tiene plan: una versión nueva necesita plan_reason (por qué cambia)",
            )),
            Some(last) if task.contract_version > last.version => task.contract_version,
            Some(_) => task.contract_version + 1,
        };
        let store = self.store()?;
        let jobs = store.task_jobs(&task.id)?;
        // Written after an Alcance job explored, or by the coordinator alone.
        let explored = jobs.iter().rev().find(|job| {
            job.role == SCOPE_ROLE
                && job.status == DeliveryJobStatus::Finished
                && job.undone_at_ms.is_none()
                && job.result_state != Some(DeliveryResultState::Invalid)
        });
        let places = input
            .places
            .into_iter()
            .map(|place| match place {
                PlaceInput::Path(path) => DeliveryScopePlace {
                    path,
                    note: String::new(),
                },
                PlaceInput::Place(place) => place,
            })
            .map(|place| DeliveryScopePlace {
                path: place.path.trim().replace('\\', "/"),
                note: place.note.trim().to_string(),
            })
            .filter(|place| !place.path.is_empty())
            .collect();
        let now = now_ms();
        task.plans.push(DeliveryScopePlan {
            version,
            at_ms: now,
            author: if explored.is_some() {
                "agent"
            } else {
                "coordinator"
            }
            .to_string(),
            job_id: explored.map(|job| job.id.clone()),
            reason,
            changed: input.changed.trim().to_string(),
            what,
            places,
            risk_level: input.risk_level.trim().to_lowercase(),
            risks: clean(input.risks),
            tests: input
                .tests
                .into_iter()
                .filter(|test| !test.requirement.trim().is_empty())
                .collect(),
            out_of_scope: clean(input.out_of_scope),
        });
        task.contract_version = version;
        task.updated_at_ms = now;
        store.put_task(&task)?;
        store.record_event(
            now,
            "plan_set",
            Some(&task.id),
            None,
            &format!("plan v{version}"),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }

    /// Skips Alcance on the task. A reason is required; a coordinator
    /// cannot skip it in a repo that always runs it, the user can.
    pub fn skip_scope(
        &self,
        task_id: &str,
        reason: &str,
        by_user: bool,
    ) -> Result<DeliveryTask, DeliveryError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(DeliveryError::new(
                "scope_reason_required",
                "omitir el alcance necesita un motivo",
            ));
        }
        let mut task = self.task(task_id)?;
        let store = self.store()?;
        if !by_user && store.repo_settings(&task.repo)?.scope == DeliveryScopeMode::Always {
            return Err(DeliveryError::new(
                "scope_required",
                "este repositorio lanza el alcance en todas las tareas: solo el usuario puede omitirlo",
            ));
        }
        if store
            .task_jobs(&task.id)?
            .iter()
            .any(|job| job.role == SCOPE_ROLE && job.status.is_open())
        {
            return Err(DeliveryError::new(
                "scope_running",
                "el alcance está en marcha: cancélalo antes de omitirlo",
            ));
        }
        let now = now_ms();
        task.scope_skip = Some(DeliveryScopeSkip {
            reason: reason.to_string(),
            by: if by_user { "user" } else { "coordinator" }.to_string(),
            at_ms: now,
        });
        task.updated_at_ms = now;
        store.put_task(&task)?;
        store.record_event(
            now,
            if by_user {
                "task_updated"
            } else {
                "task_state_set"
            },
            Some(&task.id),
            None,
            &format!("alcance omitido: {reason}"),
        )?;
        drop(store);
        self.notify();
        Ok(task)
    }
}

/// Whether the repo's Alcance setting lets a stage start: a repo that never
/// runs it refuses Alcance; one that always does refuses the other stages
/// until it ran or was skipped.
pub fn check_dispatch(
    mode: DeliveryScopeMode,
    role: &str,
    task: &DeliveryTask,
    jobs: &[DeliveryJob],
) -> Result<(), DeliveryError> {
    match mode {
        DeliveryScopeMode::Never if role == SCOPE_ROLE => Err(DeliveryError::new(
            "scope_disabled",
            "este repositorio no lanza el alcance (Ajustes): escribe el plan con set_task_state plan",
        )),
        DeliveryScopeMode::Always if AFTER_SCOPE.contains(&role) => {
            let refs: Vec<&DeliveryJob> = jobs.iter().collect();
            if scope_settled(task, &refs) {
                Ok(())
            } else {
                Err(DeliveryError::new(
                    "scope_required",
                    "este repositorio lanza el alcance antes de las demás etapas: lanza explore, o el usuario lo omite en Tinto",
                ))
            }
        }
        _ => Ok(()),
    }
}

/// The task's plan as later stages read it in their instructions: always
/// the same seven sections, saying so where nothing was recorded.
pub fn plan_notes(task: &DeliveryTask, decisions: &[DeliveryDecision]) -> Vec<String> {
    let missing = "not recorded in Tinto".to_string();
    let joined = |items: Vec<String>| {
        if items.is_empty() {
            missing.clone()
        } else {
            items.join("; ")
        }
    };
    let plan = task.plans.last();
    let mut lines = vec![match plan {
        Some(plan) => format!(
            "The task's plan (contract v{}, written by {}): work within it.",
            plan.version,
            if plan.author == "agent" {
                "the coordinator after the scoping stage explored the code"
            } else {
                "the coordinator without a scoping stage"
            }
        ),
        None => "The task's plan is not recorded in Tinto: work from the assignment.".to_string(),
    }];
    let what = plan.map(|plan| plan.what.clone()).unwrap_or_default();
    lines.push(format!(
        "- What changes: {}",
        if what.is_empty() {
            missing.clone()
        } else {
            what
        }
    ));
    lines.push(format!(
        "- Where: {}",
        joined(plan.map_or(Vec::new(), |plan| {
            plan.places
                .iter()
                .map(|place| match place.note.as_str() {
                    "" => place.path.clone(),
                    note => format!("{} ({note})", place.path),
                })
                .collect()
        }))
    ));
    lines.push(format!(
        "- Risks{}: {}",
        plan.filter(|plan| !plan.risk_level.is_empty())
            .map(|plan| format!(" ({})", plan.risk_level))
            .unwrap_or_default(),
        joined(plan.map_or(Vec::new(), |plan| plan.risks.clone()))
    ));
    lines.push(format!(
        "- Tests (requirement → test): {}",
        joined(plan.map_or(Vec::new(), |plan| {
            plan.tests
                .iter()
                .map(|test| {
                    let at: Vec<&str> = [test.layer.as_str(), test.file.as_str()]
                        .into_iter()
                        .filter(|part| !part.trim().is_empty())
                        .collect();
                    match at.is_empty() {
                        true => test.requirement.clone(),
                        false => format!("{} → {}", test.requirement, at.join(", ")),
                    }
                })
                .collect()
        }))
    ));
    lines.push(format!(
        "- Checks: {}",
        if task.checks.is_empty() {
            "none set on the task".to_string()
        } else {
            task.checks.join("; ")
        }
    ));
    let answered = decisions
        .iter()
        .filter(|decision| decision.status == DeliveryDecisionStatus::Answered)
        .count();
    lines.push(format!(
        "- Questions: {}",
        match answered {
            0 => "none answered".to_string(),
            count => format!("{count} answered, listed below with the decisions"),
        }
    ));
    lines.push(format!(
        "- Out of scope: {}",
        joined(plan.map_or(Vec::new(), |plan| plan.out_of_scope.clone()))
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::adapters::tests::{sample_job, sample_task};
    use crate::delivery::model::DeliveryAgent;

    #[test]
    fn the_plan_reaches_later_stages_in_seven_sections_even_unrecorded() {
        let mut task = sample_task();
        let unrecorded = plan_notes(&task, &[]);
        assert_eq!(unrecorded.len(), 8);
        assert!(unrecorded[1..].iter().all(|line| line.starts_with("- ")));
        task.checks = vec!["npm test".into()];
        task.plans.push(DeliveryScopePlan {
            version: 1,
            author: "agent".into(),
            what: "Valida el nombre de carpeta".into(),
            tests: vec![DeliveryScopeTest {
                requirement: "rechaza nombres con /".into(),
                layer: "unit".into(),
                file: "src/name.test.ts".into(),
            }],
            ..DeliveryScopePlan::default()
        });
        let notes = plan_notes(&task, &[]);
        assert!(notes[0].contains("v1") && notes[0].contains("explored"));
        assert!(notes.contains(
            &"- Tests (requirement → test): rechaza nombres con / → unit, src/name.test.ts"
                .to_string()
        ));
        assert!(notes.contains(&"- Checks: npm test".to_string()));
    }

    #[test]
    fn the_repo_setting_gates_alcance_and_the_stages_after_it() {
        let mut task = sample_task();
        let mut scope = sample_job(DeliveryAgent::Claude);
        scope.role = SCOPE_ROLE.into();
        let tests = |mode, jobs: &[DeliveryJob], task: &DeliveryTask| {
            check_dispatch(mode, "tests", task, jobs).is_ok()
        };
        assert!(check_dispatch(DeliveryScopeMode::Never, SCOPE_ROLE, &task, &[]).is_err());
        assert!(tests(DeliveryScopeMode::Coordinator, &[], &task));
        assert!(!tests(DeliveryScopeMode::Always, &[], &task));
        scope.status = DeliveryJobStatus::Finished;
        assert!(tests(DeliveryScopeMode::Always, &[scope], &task));
        task.scope_skip = Some(DeliveryScopeSkip {
            reason: "una línea".into(),
            by: "user".into(),
            at_ms: 1,
        });
        assert!(tests(DeliveryScopeMode::Always, &[], &task));
    }
}
