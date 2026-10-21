//! Blocked tasks: what a task waits for before it can go on (another task,
//! a PR, a decision of another task, a note). Tinto follows each blocker
//! and clears it when it resolves, telling the batch's coordinator; once
//! none is open, the task is back where it was.

use std::time::Duration;

use super::model::{
    DeliveryApproval, DeliveryBlocker, DeliveryBlockerKind, DeliveryDecision,
    DeliveryDecisionStatus, DeliveryPrMilestone, DeliveryRung, DeliveryTask, DeliveryTaskMilestone,
    NewBlocker,
};
use super::phase::{is_delivered, step_done};
use super::prs::{parse_pr_url, DeliveryPr};
use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

fn milestone_text(milestone: DeliveryTaskMilestone) -> &'static str {
    match milestone {
        DeliveryTaskMilestone::PrOpened => "PR abierto",
        DeliveryTaskMilestone::Merged => "fusionado",
        DeliveryTaskMilestone::Delivered => "entregada",
    }
}

/// A blocker in a few words: "AGOS-580 (fusionado)", "PR owner/repo#305".
pub fn blocker_label(blocker: &DeliveryBlocker) -> String {
    match &blocker.kind {
        DeliveryBlockerKind::Task { key, until, .. } => {
            format!("{key} ({})", milestone_text(*until))
        }
        DeliveryBlockerKind::Pr { url, .. } => parse_pr_url(url)
            .map(|pr| format!("PR {}#{}", pr.repo, pr.number))
            .unwrap_or_else(|| url.clone()),
        DeliveryBlockerKind::Decision { key, .. } => format!("una decisión de {key}"),
        DeliveryBlockerKind::Note { text } => text.clone(),
    }
}

/// What Tinto knows that a blocker can depend on.
struct World<'a> {
    tasks: &'a [DeliveryTask],
    approvals: &'a [DeliveryApproval],
    decisions: &'a [DeliveryDecision],
    prs: &'a [DeliveryPr],
}

fn same_pr(pr: &DeliveryPr, url: &str) -> bool {
    parse_pr_url(url).is_some_and(|wanted| {
        wanted.number == pr.number && wanted.repo.eq_ignore_ascii_case(&pr.repo)
    })
}

/// Where a blocker stands now, and whether that clears it.
fn blocker_state(blocker: &DeliveryBlocker, world: &World<'_>) -> (String, bool) {
    match &blocker.kind {
        DeliveryBlockerKind::Task { task_id, until, .. } => {
            let Some(other) = world.tasks.iter().find(|task| task.id == *task_id) else {
                return ("tarea no encontrada".into(), false);
            };
            let approvals: Vec<&DeliveryApproval> = world
                .approvals
                .iter()
                .filter(|approval| approval.task_id == *task_id)
                .collect();
            let pr = world.prs.iter().find(|pr| pr.task_id == *task_id);
            let reached = if is_delivered(other, &approvals) {
                Some(DeliveryTaskMilestone::Delivered)
            } else if step_done(&approvals, DeliveryRung::Merge)
                || pr.is_some_and(|pr| pr.state == "merged")
            {
                Some(DeliveryTaskMilestone::Merged)
            } else if step_done(&approvals, DeliveryRung::Pr) || pr.is_some() {
                Some(DeliveryTaskMilestone::PrOpened)
            } else {
                None
            };
            let status = reached.map_or("sin PR", milestone_text).to_string();
            (status, reached.is_some_and(|reached| reached >= *until))
        }
        DeliveryBlockerKind::Pr { url, until, state } => {
            // A PR Tinto follows for a task has fresher news than the blocker.
            let state = world
                .prs
                .iter()
                .find(|pr| same_pr(pr, url))
                .map(|pr| pr.state.clone())
                .or_else(|| state.clone());
            let status = match state.as_deref() {
                Some("open") => "abierto",
                Some("merged") => "fusionado",
                Some("closed") => "cerrado sin fusionar",
                _ => "sin leer todavía",
            };
            let done = match until {
                DeliveryPrMilestone::Merged => state.as_deref() == Some("merged"),
                DeliveryPrMilestone::Closed => {
                    matches!(state.as_deref(), Some("merged") | Some("closed"))
                }
            };
            (status.into(), done)
        }
        DeliveryBlockerKind::Decision { decision_id, .. } => {
            match world
                .decisions
                .iter()
                .find(|decision| decision.id == *decision_id)
            {
                Some(decision) if decision.status == DeliveryDecisionStatus::Pending => {
                    ("pendiente".into(), false)
                }
                Some(_) => ("contestada".into(), true),
                None => ("ya no existe".into(), true),
            }
        }
        DeliveryBlockerKind::Note { .. } => (String::new(), false),
    }
}

impl DeliveryService {
    fn new_blocker(
        &self,
        task: &DeliveryTask,
        request: NewBlocker,
    ) -> Result<DeliveryBlocker, DeliveryError> {
        let invalid = |message: String| DeliveryError::new("invalid_blocker", message);
        let kind = match request {
            NewBlocker::Task {
                task: wanted,
                until,
            } => {
                let wanted = wanted.trim();
                let store = self.store()?;
                let mut candidates: Vec<DeliveryTask> = store
                    .tasks()?
                    .into_iter()
                    .filter(|other| other.removed_at_ms.is_none())
                    // By key, another package of the issue, never this one.
                    .filter(|other| {
                        other.id == wanted
                            || (other.id != task.id && other.key.eq_ignore_ascii_case(wanted))
                    })
                    .collect();
                // The live task before an archived one of the same key.
                candidates.sort_by_key(|other| {
                    (
                        other.archived_at_ms.is_some(),
                        u64::MAX - other.created_at_ms,
                    )
                });
                let other = candidates
                    .into_iter()
                    .next()
                    .ok_or_else(|| invalid(format!("no hay ninguna tarea {wanted} en Tinto")))?;
                if other.id == task.id {
                    return Err(invalid("una tarea no puede bloquearse a sí misma".into()));
                }
                DeliveryBlockerKind::Task {
                    key: other.label(),
                    task_id: other.id,
                    until: until.unwrap_or(DeliveryTaskMilestone::Merged),
                }
            }
            NewBlocker::Pr { url, until } => {
                let pr = parse_pr_url(&url)
                    .ok_or_else(|| invalid(format!("{url} no es la URL de un PR de GitHub")))?;
                DeliveryBlockerKind::Pr {
                    url: pr.url,
                    until: until.unwrap_or(DeliveryPrMilestone::Merged),
                    state: None,
                }
            }
            NewBlocker::Decision { decision_id } => {
                let store = self.store()?;
                let decision = store
                    .decisions()?
                    .into_iter()
                    .find(|decision| decision.id == decision_id)
                    .ok_or_else(|| invalid(format!("no hay ninguna decisión {decision_id}")))?;
                let key = store
                    .task(&decision.task_id)?
                    .map(|owner| owner.key)
                    .unwrap_or_default();
                DeliveryBlockerKind::Decision { decision_id, key }
            }
            NewBlocker::Note { text } => {
                let text = text.trim().to_string();
                if text.is_empty() || text.chars().count() > 200 {
                    return Err(invalid(
                        "la nota debe tener entre 1 y 200 caracteres".into(),
                    ));
                }
                DeliveryBlockerKind::Note { text }
            }
        };
        Ok(DeliveryBlocker {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            status: String::new(),
            cleared_at_ms: None,
        })
    }

    /// Records the task's blockers and settles them at once (one may have
    /// cleared already). `by_user` changes wake the coordinator.
    fn save_blockers(
        &self,
        mut task: DeliveryTask,
        event: &str,
        detail: &str,
    ) -> Result<DeliveryTask, DeliveryError> {
        task.updated_at_ms = now_ms();
        {
            let store = self.store()?;
            store.put_task(&task)?;
            store.record_event(task.updated_at_ms, event, Some(&task.id), None, detail)?;
        }
        self.settle_blockers()?;
        self.notify();
        self.task(&task.id)
    }

    /// Blocks the task until each blocker clears, in place of any it had.
    pub fn set_blockers(
        &self,
        task_id: &str,
        blockers: Vec<NewBlocker>,
        reason: &str,
    ) -> Result<DeliveryTask, DeliveryError> {
        if blockers.is_empty() {
            return Err(DeliveryError::new(
                "invalid_blocker",
                "indica al menos un bloqueo; para quitarlos usa clear_task_blocked",
            ));
        }
        let mut task = self.task(task_id)?;
        let blockers = blockers
            .into_iter()
            .map(|blocker| self.new_blocker(&task, blocker))
            .collect::<Result<Vec<_>, _>>()?;
        let detail = blockers
            .iter()
            .map(blocker_label)
            .collect::<Vec<_>>()
            .join(", ");
        task.blockers = blockers;
        task.blocked_reason = reason.trim().to_string();
        self.save_blockers(task, "task_blocked", &detail)
    }

    /// Adds a blocker, as the user does from the task.
    pub fn add_blocker(
        &self,
        task_id: &str,
        blocker: NewBlocker,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let blocker = self.new_blocker(&task, blocker)?;
        let detail = format!("añadió {}", blocker_label(&blocker));
        task.blockers.push(blocker);
        self.save_blockers(task, "blockers_changed", &detail)
    }

    /// Clears the given blockers, or every open one. The user's clearing
    /// overrides the block and wakes the coordinator.
    pub fn clear_blockers(
        &self,
        task_id: &str,
        ids: Option<&[String]>,
        by_user: bool,
    ) -> Result<DeliveryTask, DeliveryError> {
        let mut task = self.task(task_id)?;
        let now = now_ms();
        let mut cleared = Vec::new();
        for blocker in task.blockers.iter_mut().filter(|blocker| blocker.is_open()) {
            if ids.is_none_or(|ids| ids.contains(&blocker.id)) {
                blocker.cleared_at_ms = Some(now);
                cleared.push(blocker_label(blocker));
            }
        }
        if cleared.is_empty() {
            return Ok(task);
        }
        let event = if by_user {
            "blockers_changed"
        } else {
            "task_unblocked"
        };
        self.save_blockers(task, event, &format!("quitó {}", cleared.join(", ")))
    }

    /// Updates where each open blocker stands and clears the ones that
    /// resolved, with a `blocker_cleared` event for the coordinator.
    pub fn settle_blockers(&self) -> Result<usize, DeliveryError> {
        let store = self.store()?;
        let tasks = store.tasks()?;
        if !tasks
            .iter()
            .any(|task| task.blockers.iter().any(DeliveryBlocker::is_open))
        {
            return Ok(0);
        }
        let approvals = store.approvals()?;
        let decisions = store.decisions()?;
        let prs = store.prs()?;
        let world = World {
            tasks: &tasks,
            approvals: &approvals,
            decisions: &decisions,
            prs: &prs,
        };
        let now = now_ms();
        let mut cleared = 0;
        let mut changed_any = false;
        for task in tasks.iter().filter(|task| task.removed_at_ms.is_none()) {
            let mut next = task.clone();
            let mut changed = false;
            for blocker in next.blockers.iter_mut().filter(|blocker| blocker.is_open()) {
                let (status, done) = blocker_state(blocker, &world);
                if status != blocker.status {
                    blocker.status = status;
                    changed = true;
                }
                if done {
                    blocker.cleared_at_ms = Some(now);
                    store.record_event(
                        now,
                        "blocker_cleared",
                        Some(&task.id),
                        None,
                        &blocker_label(blocker),
                    )?;
                    cleared += 1;
                    changed = true;
                }
            }
            if changed {
                store.put_task(&next)?;
                changed_any = true;
            }
        }
        drop(store);
        if changed_any {
            self.notify();
        }
        Ok(cleared)
    }

    /// Reads, from GitHub, the PRs that block a task and that Tinto does not
    /// follow for one of its own; settling then clears the merged ones.
    pub fn refresh_blocking_prs(&self) -> Result<(), DeliveryError> {
        let (tasks, followed) = {
            let store = self.store()?;
            (store.tasks()?, store.prs()?)
        };
        let mut urls: Vec<String> = Vec::new();
        for task in &tasks {
            for blocker in task.blockers.iter().filter(|blocker| blocker.is_open()) {
                if let DeliveryBlockerKind::Pr { url, .. } = &blocker.kind {
                    if !followed.iter().any(|pr| same_pr(pr, url)) && !urls.contains(url) {
                        urls.push(url.clone());
                    }
                }
            }
        }
        if urls.is_empty() {
            return Ok(());
        }
        let github = self.github();
        let states: Vec<(String, String)> = urls
            .into_iter()
            .filter_map(|url| {
                let snapshot = github.pr(&url).ok()?;
                let state = snapshot["state"].as_str()?.to_ascii_lowercase();
                Some((url, state))
            })
            .collect();
        let store = self.store()?;
        let mut changed_any = false;
        for mut task in store.tasks()? {
            let mut changed = false;
            for blocker in task.blockers.iter_mut().filter(|blocker| blocker.is_open()) {
                if let DeliveryBlockerKind::Pr { url, state, .. } = &mut blocker.kind {
                    if let Some((_, read)) = states.iter().find(|(wanted, _)| wanted == url) {
                        if state.as_deref() != Some(read.as_str()) {
                            *state = Some(read.clone());
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                store.put_task(&task)?;
                changed_any = true;
            }
        }
        drop(store);
        if changed_any {
            self.settle_blockers()?;
            self.notify();
        }
        Ok(())
    }

    /// Settles the blockers after every change in Delivery (an approval, a
    /// decision, a PR), so a cleared one reaches the coordinator at once.
    pub fn start_blocker_watch(&self) {
        let service = self.clone();
        std::thread::spawn(move || {
            let mut seen = service
                .inner
                .changes
                .0
                .lock()
                .map(|count| *count)
                .unwrap_or(0);
            loop {
                if let Err(error) = service.settle_blockers() {
                    eprintln!("tinto: delivery could not settle the blocked tasks: {error}");
                }
                // A merged package lets the one built on it merge where it did.
                if let Err(error) = service.retarget_packages() {
                    eprintln!("tinto: delivery could not retarget packages: {error}");
                }
                // Jobs waiting on the environment start once it answers.
                service.check_due_environments();
                let (count, changed) = &service.inner.changes;
                let Ok(guard) = count.lock() else { return };
                let Ok((guard, _)) =
                    changed
                        .wait_timeout_while(guard, Duration::from_secs(60), |count| *count == seen)
                else {
                    return;
                };
                seen = *guard;
            }
        });
    }
}
