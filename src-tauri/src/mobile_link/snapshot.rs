//! What the phone sees (docs/contracts/mobile-link.md §5): the active
//! batches, their tasks and phases, and what is pending, cut from the
//! overview so that it carries no code, paths, logs or settings.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::delivery::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryDecision, DeliveryDecisionKind,
    DeliveryDecisionOption, DeliveryDecisionStatus, DeliveryJobStatus, DeliveryOverview,
    DeliveryQaProblemStatus, DeliveryRecommendation, DeliveryRung, DeliverySettingsProposalStatus,
};
use crate::delivery::phase::{DeliveryTaskPhase, Phase, PrWait};

/// The longest approval text the phone shows whole.
pub(super) const APPROVAL_TEXT: usize = 16_000;
/// The longest text of anything else.
const TEXT: usize = 4_000;
const FAILURES: usize = 5;
const FAILURE_TEXT: usize = 500;
/// Room for the envelope within a frame.
pub const MAX_SNAPSHOT: usize = super::crypto::MAX_PLAINTEXT - 1024;

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub at_ms: u64,
    pub runs: Vec<Run>,
    pub tasks: Vec<Task>,
    pub decisions: Vec<Decision>,
    pub approvals: Vec<Approval>,
    pub qa_problems: Vec<QaProblem>,
    pub settings_proposals: Vec<SettingsProposal>,
    pub failures: Vec<Failure>,
    /// It did not fit in a frame: the pending lists were left out.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

#[derive(Debug, Serialize)]
pub struct Run {
    pub id: String,
    pub title: String,
    pub paused: bool,
}

#[derive(Debug, Serialize)]
pub struct Task {
    pub id: String,
    pub run_id: Option<String>,
    pub key: String,
    pub title: String,
    pub phase: Option<TaskPhase>,
    pub pr: Option<Pr>,
}

#[derive(Debug, Serialize)]
pub struct TaskPhase {
    pub kind: String,
    pub label: String,
    pub waits_on: Value,
}

#[derive(Debug, Serialize)]
pub struct Pr {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub draft: bool,
    pub conflicts: bool,
    pub ci: Value,
    pub review_decision: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Decision {
    pub id: String,
    pub task_id: String,
    pub kind: Value,
    pub question: String,
    pub detail: String,
    pub text: String,
    pub command: Option<String>,
    pub options: Vec<DeliveryDecisionOption>,
    pub recommendation: Option<DeliveryRecommendation>,
    pub blocking: bool,
    /// The phone cannot show all it would allow or approve (hidden grants,
    /// a cut text): only Tinto does (a permission can still be denied
    /// from the phone).
    pub in_tinto: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub id: String,
    pub task_id: String,
    pub rung: Value,
    pub stage: Option<String>,
    pub title: String,
    pub body: String,
    pub commits: Vec<String>,
    pub merge_method: Option<String>,
    pub jira_statuses: Vec<String>,
    pub reviewers: Vec<String>,
    pub requested_at_ms: u64,
    pub digest: String,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct QaProblem {
    pub id: String,
    pub task_id: String,
    pub number: u32,
    pub label: String,
    pub place: String,
    pub text: String,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingsProposal {
    pub id: String,
    pub task_id: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub task_id: String,
    pub role: String,
    pub error: String,
    pub ended_at_ms: Option<u64>,
}

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

fn value<T: Serialize>(of: &T) -> Value {
    serde_json::to_value(of).unwrap_or(Value::Null)
}

/// An approval as the phone shows it, whole, with the digest of what it
/// shows: approving from the phone needs the same digest (§4).
pub fn approval(approval: &DeliveryApproval) -> Approval {
    let mut view = Approval {
        id: approval.id.clone(),
        task_id: approval.task_id.clone(),
        rung: value(&approval.rung),
        stage: approval.stage.clone(),
        title: approval.title.clone(),
        body: approval.body.clone(),
        commits: approval.commits.iter().map(|c| c.message.clone()).collect(),
        merge_method: approval.merge_method.clone(),
        jira_statuses: approval.jira_statuses.clone(),
        reviewers: approval.reviewers.clone(),
        requested_at_ms: approval.requested_at_ms,
        digest: String::new(),
        truncated: false,
    };
    let hash = Sha256::digest(serde_json::to_vec(&view).unwrap_or_default());
    view.digest = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    view
}

/// Its texts cut to `max`; `truncated` once any was.
fn cut_approval(mut view: Approval, max: usize) -> Approval {
    let long = |text: &String| text.chars().count() > max;
    view.truncated |= long(&view.title) || long(&view.body) || view.commits.iter().any(long);
    view.title = cut(&view.title, max);
    view.body = cut(&view.body, max);
    view.commits = view.commits.iter().map(|c| cut(c, max)).collect();
    view
}

fn decision(decision: &DeliveryDecision) -> Decision {
    Decision {
        id: decision.id.clone(),
        task_id: decision.task_id.clone(),
        kind: value(&decision.kind),
        question: cut(&decision.question, TEXT),
        detail: cut(&decision.detail, TEXT),
        text: cut(&decision.text, TEXT),
        command: decision
            .command
            .as_deref()
            .map(|command| cut(command, TEXT)),
        options: decision.options.clone(),
        recommendation: decision.recommendation.clone(),
        blocking: decision.blocking,
        in_tinto: in_tinto(decision),
    }
}

/// Whether allowing a permission or approving a text needs what the phone
/// does not show: the grants beyond its command, or a text it cuts.
pub(super) fn in_tinto(decision: &DeliveryDecision) -> bool {
    let long = |text: &str| text.chars().count() > TEXT;
    match decision.kind {
        DeliveryDecisionKind::Choice => false,
        DeliveryDecisionKind::Text => long(&decision.text),
        DeliveryDecisionKind::Permission => {
            decision.full_access
                || !decision.write_dirs.is_empty()
                || !decision.read_dirs.is_empty()
                || !decision.scripts.is_empty()
                || !decision.warnings.is_empty()
                || !decision.unfrozen.is_empty()
                || decision.command.as_deref().is_some_and(long)
        }
    }
}

pub fn build(overview: &DeliveryOverview, now: u64) -> Snapshot {
    let runs: Vec<Run> = overview
        .runs
        .iter()
        .filter(|run| run.status == "active")
        .map(|run| Run {
            id: run.id.clone(),
            title: run.title.clone(),
            paused: run.paused,
        })
        .collect();
    let active: HashSet<&str> = runs.iter().map(|run| run.id.as_str()).collect();
    let tasks: Vec<Task> = overview
        .tasks
        .iter()
        .filter(|task| task.removed_at_ms.is_none() && task.archived_at_ms.is_none())
        .filter(|task| {
            task.run_id
                .as_deref()
                .is_none_or(|run| active.contains(run))
        })
        .map(|task| Task {
            id: task.id.clone(),
            run_id: task.run_id.clone(),
            key: task.key.clone(),
            title: cut(&task.title, TEXT),
            phase: overview.phases.get(&task.id).map(task_phase),
            pr: overview
                .prs
                .iter()
                .find(|pr| pr.task_id == task.id)
                .map(|pr| Pr {
                    number: pr.number,
                    url: pr.url.clone(),
                    state: pr.state.as_str().to_string(),
                    draft: pr.draft,
                    conflicts: pr.conflicts,
                    ci: value(&pr.ci),
                    review_decision: pr.review_decision.clone(),
                }),
        })
        .collect();
    let shown: HashSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    let mine = |task_id: &str| shown.contains(task_id);

    let mut failed: Vec<_> = overview
        .jobs
        .iter()
        .filter(|job| job.status == DeliveryJobStatus::Failed && mine(&job.task_id))
        .collect();
    failed.sort_by_key(|job| std::cmp::Reverse(job.ended_at_ms));
    Snapshot {
        at_ms: now,
        decisions: overview
            .decisions
            .iter()
            .filter(|d| d.status == DeliveryDecisionStatus::Pending && mine(&d.task_id))
            .map(decision)
            .collect(),
        approvals: overview
            .approvals
            .iter()
            .filter(|a| a.status == DeliveryApprovalStatus::Pending && mine(&a.task_id))
            .map(|a| cut_approval(approval(a), APPROVAL_TEXT))
            .collect(),
        qa_problems: overview
            .qa_problems
            .iter()
            .filter(|p| p.status == DeliveryQaProblemStatus::Open && mine(&p.task_id))
            .map(|p| QaProblem {
                id: p.id.clone(),
                task_id: p.task_id.clone(),
                number: p.number,
                label: p.label.clone(),
                place: cut(&p.place, TEXT),
                text: cut(&p.text, TEXT),
                suggestions: p.suggestions.iter().map(|s| cut(s, TEXT)).collect(),
            })
            .collect(),
        settings_proposals: overview
            .settings_proposals
            .iter()
            .filter(|p| p.status == DeliverySettingsProposalStatus::Pending && mine(&p.task_id))
            .map(|p| SettingsProposal {
                id: p.id.clone(),
                task_id: p.task_id.clone(),
                reason: cut(&p.reason, TEXT),
            })
            .collect(),
        failures: failed
            .into_iter()
            .take(FAILURES)
            .map(|job| Failure {
                task_id: job.task_id.clone(),
                role: job.role.clone(),
                error: cut(
                    job.error
                        .as_deref()
                        .or(job.result_note.as_deref())
                        .unwrap_or(""),
                    FAILURE_TEXT,
                ),
                ended_at_ms: job.ended_at_ms,
            })
            .collect(),
        runs,
        tasks,
        partial: false,
    }
}

/// The snapshot as a message body that fits in a frame: what does not fit
/// is dropped, the least needed first.
pub fn body(mut snapshot: Snapshot) -> Value {
    let fits = |snapshot: &Snapshot| {
        serde_json::to_vec(snapshot).map_or(0, |bytes| bytes.len()) <= MAX_SNAPSHOT
    };
    if !fits(&snapshot) {
        snapshot.failures.clear();
        snapshot.decisions.iter_mut().for_each(|d| d.detail.clear());
    }
    if !fits(&snapshot) {
        snapshot.approvals = snapshot
            .approvals
            .into_iter()
            .map(|view| cut_approval(view, 2_000))
            .collect();
    }
    if !fits(&snapshot) {
        snapshot.decisions.clear();
        snapshot.approvals.clear();
        snapshot.qa_problems.clear();
        snapshot.settings_proposals.clear();
        snapshot.partial = true;
    }
    value(&snapshot)
}

fn stage(role: &str) -> String {
    match role {
        "explore" => "Alcance",
        "tests" => "Tests",
        "implementation" => "Implementación",
        "review" => "Revisión",
        "qa" => "QA",
        "bootstrap" | "setup" => "Preparación",
        "checks" => "Comprobaciones",
        other => other,
    }
    .to_string()
}

fn step(rung: DeliveryRung, jira_stage: Option<&str>) -> &'static str {
    match (rung, jira_stage) {
        (DeliveryRung::Commit, _) => "commit",
        (DeliveryRung::Sync, _) => "sync con la base",
        (DeliveryRung::Push, _) => "push",
        (DeliveryRung::Pr, _) => "PR",
        (DeliveryRung::Merge, _) => "merge",
        (DeliveryRung::Jira, Some("after_merge")) => "Jira (merge)",
        (DeliveryRung::Jira, _) => "Jira (PR)",
        (DeliveryRung::PrReply, _) => "respuesta al revisor",
    }
}

fn task_phase(phase: &DeliveryTaskPhase) -> TaskPhase {
    let label = match &phase.phase {
        Phase::Decisions { count: 1 } => "Una decisión pendiente".to_string(),
        Phase::Decisions { count } => format!("{count} decisiones pendientes"),
        Phase::QaProblems { count: 1 } => "Un problema de QA por decidir".to_string(),
        Phase::QaProblems { count } => format!("{count} problemas de QA por decidir"),
        Phase::Approval { rung, stage } => format!("Aprobar {}", step(*rung, stage.as_deref())),
        Phase::StepRunning { rung, .. } => format!("En curso: {}", step(*rung, None)),
        Phase::Record { rung, stage } => {
            format!("El coordinador informa: {}", step(*rung, stage.as_deref()))
        }
        Phase::Drifted { rung, stage } => {
            format!(
                "{} ya no coincide con GitHub",
                step(*rung, stage.as_deref())
            )
        }
        Phase::Running { role, .. } => format!("En marcha: {}", stage(role)),
        Phase::Paused { role, .. } => format!("En pausa: {}", stage(role)),
        Phase::BatchPaused { .. } => "Lote en pausa".to_string(),
        Phase::WaitsQuarantine { .. } | Phase::Quarantined { .. } => {
            "Recurso en cuarentena".to_string()
        }
        Phase::SetupFailed { .. } => "Falló la preparación".to_string(),
        Phase::WaitsEnvironment { .. } => "Esperando al entorno".to_string(),
        Phase::Queued { role, .. } => format!("En cola: {}", stage(role)),
        Phase::SettingsProposal => "Propuesta de ajustes".to_string(),
        Phase::Blocked { .. } => "Bloqueada".to_string(),
        Phase::Unblocked { .. } => "Desbloqueada".to_string(),
        Phase::Broken {
            role,
            blocked: true,
            ..
        } => format!("{} bloqueada", stage(role)),
        Phase::Broken { role, .. } => format!("Falló {}", stage(role)),
        Phase::Delivered => "Entregada".to_string(),
        Phase::Pr { number, wait, .. } => format!(
            "PR #{number}: {}",
            match wait {
                PrWait::Reply { .. } => "responder a revisores",
                PrWait::Trouble => "tiene problemas",
                PrWait::Answer { .. } => "el coordinador responde",
                PrWait::Review { .. } => "en revisión",
                PrWait::Checks => "CI en curso",
                PrWait::Draft => "borrador",
                PrWait::Ready => "listo",
                PrWait::Nobody => "sin revisores",
            }
        ),
        Phase::WaitsMergeCommit { .. } => "Falta el commit del merge".to_string(),
        Phase::MergeMixed { .. } => "Decidir el commit del merge".to_string(),
        Phase::PrWaitsPredecessor { .. } => "Espera al paquete anterior".to_string(),
        Phase::MergedFailingCi { number, .. } => format!("PR #{number} fusionado con CI en rojo"),
        Phase::NextStage { role } => format!("Siguiente: {}", stage(role)),
        Phase::NextStep { rung, stage } => {
            format!("Siguiente: {}", step(*rung, stage.as_deref()))
        }
    };
    let kind = value(&phase.phase)
        .get("kind")
        .cloned()
        .unwrap_or(Value::Null);
    TaskPhase {
        kind: kind.as_str().unwrap_or("").to_string(),
        label,
        waits_on: value(&phase.waits_on),
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::delivery::adapters::tests::{sample_job, sample_task};
    use crate::delivery::model::{DeliveryAgent, DeliveryRun, DeliverySettings};
    use serde_json::json;

    pub(in crate::mobile_link) fn sample_approval(id: &str, body: &str) -> DeliveryApproval {
        serde_json::from_value(json!({
            "id": id, "task_id": "t1", "rung": "pr", "title": "feat: algo",
            "body": body, "status": "pending", "requested_by": "user",
            "requested_at_ms": 1, "decided_at_ms": null, "executed_at_ms": null,
            "outcome": null
        }))
        .unwrap()
    }

    fn overview() -> DeliveryOverview {
        let run = |id: &str, status: &str| -> DeliveryRun {
            serde_json::from_value(json!({
                "id": id, "repo": "/repo", "title": id, "status": status, "generation": 1,
                "owner": null, "created_at_ms": 1, "updated_at_ms": 1
            }))
            .unwrap()
        };
        let task = |id: &str, run: &str| {
            let mut task = sample_task();
            task.id = id.into();
            task.run_id = Some(run.into());
            task
        };
        let mut failed = sample_job(DeliveryAgent::Codex);
        failed.status = DeliveryJobStatus::Failed;
        failed.error = Some("x".repeat(2_000));
        let mut answered = sample_approval("a2", "hecho");
        answered.status = DeliveryApprovalStatus::Executed;
        let mut closed_batch = sample_approval("a3", "de otro lote");
        closed_batch.task_id = "t2".into();
        DeliveryOverview {
            phases: Default::default(),
            runs: vec![run("r1", "active"), run("r2", "closed")],
            tasks: vec![task("t1", "r1"), task("t2", "r2")],
            jobs: vec![failed],
            leases: Vec::new(),
            approvals: vec![
                sample_approval("a1", &"b".repeat(APPROVAL_TEXT + 1)),
                answered,
                closed_batch,
            ],
            decisions: Vec::new(),
            qa_problems: Vec::new(),
            settings_proposals: Vec::new(),
            rounds: Vec::new(),
            prs: Vec::new(),
            settings: DeliverySettings::default(),
            capacity: 1,
            coordinator: None,
            coordinators: Vec::new(),
            issues: Vec::new(),
            queued: Default::default(),
            quotas: Vec::new(),
        }
    }

    #[test]
    fn the_snapshot_keeps_active_batches_and_what_is_pending_and_cuts_long_texts() {
        let overview = overview();
        let snapshot = build(&overview, 7);
        let ids = |items: Vec<&str>| items.join(",");
        assert_eq!(
            ids(snapshot.runs.iter().map(|r| r.id.as_str()).collect()),
            "r1"
        );
        assert_eq!(
            ids(snapshot.tasks.iter().map(|t| t.id.as_str()).collect()),
            "t1"
        );
        let approvals: Vec<_> = snapshot.approvals.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids(approvals), "a1");
        // Cut, and marked so that only Tinto can approve it; the digest is
        // of the whole text.
        let cut = &snapshot.approvals[0];
        assert!(cut.truncated);
        assert!(cut.body.chars().count() <= APPROVAL_TEXT + 1);
        assert_eq!(cut.digest, approval(&overview.approvals[0]).digest);
        assert!(snapshot.failures[0].error.chars().count() <= FAILURE_TEXT + 1);
        // Nothing of the overview's code, prompts or paths travels.
        let text = serde_json::to_string(&body(snapshot)).unwrap();
        assert!(!text.contains("Write the failing test") && !text.contains("/repo"));
    }

    #[test]
    fn a_snapshot_too_large_for_a_frame_drops_the_pending_lists() {
        let mut overview = overview();
        overview.approvals = (0..200)
            .map(|i| sample_approval(&format!("a{i}"), &"b".repeat(15_000)))
            .collect();
        let body = body(build(&overview, 7));
        assert!(serde_json::to_vec(&body).unwrap().len() <= MAX_SNAPSHOT);
        assert_eq!(body["partial"], true);
        assert_eq!(body["tasks"].as_array().unwrap().len(), 1);
    }
}
