//! What changed for a batch's coordinator, read from Delivery's audit trail
//! with a cursor (the events' `seq`).
//!
//! The coordinator waits for those changes with `wait_run`, which wakes on
//! the next one. When it is not waiting, Tinto wakes its Agents
//! conversation with one message per batch of changes (see
//! `agent_console::commands::start_delivery_waker`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::model::{DeliveryCoordinatorState, DeliveryEvent, DeliveryRun};
use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

/// Events about something the coordinator has to act on or know. What the
/// coordinator itself asks (dispatches, requests) does not wake it.
const COORDINATOR_EVENTS: &[&str] = &[
    "job_ended",
    "job_cancelled",
    "job_interrupted",
    "job_undone",
    "approval_approved",
    "approval_rejected",
    "approval_completed",
    "approval_corrected",
    "pr_found",
    "merge_found",
    "step_drifted",
    "step_matches",
    "commit_missing",
    "commit_undone",
    "decision_answered",
    "decision_changed",
    "permission_renewed",
    "settings_approved",
    "settings_rejected",
    "pr_changed",
    "task_removed",
    "task_archived",
    "task_unarchived",
    "task_updated",
    "run_policy",
    "run_takeover",
    "run_closed",
    "qa_problems_sent",
];

/// Changes are sent to the coordinator's conversation once they have been
/// quiet this long, so a burst (several answers, a PR and its checks) goes
/// in one message.
pub const WAKE_QUIET_MS: u64 = 5_000;

/// A message for a coordinator's conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorWakeup {
    pub run_id: String,
    pub session_id: String,
    /// The last event the message covers.
    pub cursor: u64,
    pub text: String,
}

/// Counts a coordinator inside wait_run for as long as it waits.
struct Waiting<'a> {
    service: &'a DeliveryService,
    run_id: String,
}

impl<'a> Waiting<'a> {
    fn start(service: &'a DeliveryService, run_id: &str) -> Self {
        if let Ok(mut waiting) = service.inner.waiting.lock() {
            *waiting.entry(run_id.to_string()).or_default() += 1;
        }
        Self {
            service,
            run_id: run_id.to_string(),
        }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if let Ok(mut waiting) = self.service.inner.waiting.lock() {
            if let Some(count) = waiting.get_mut(&self.run_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    waiting.remove(&self.run_id);
                }
            }
        }
    }
}

impl DeliveryService {
    /// The coordinator's changes in `run_id` after `after`, and the cursor
    /// to read from next (the newest event of all, relevant or not).
    pub fn coordinator_events(
        &self,
        run_id: &str,
        after: u64,
    ) -> Result<(Vec<Value>, u64), DeliveryError> {
        let store = self.store()?;
        let keys: HashMap<String, String> = store
            .tasks()?
            .into_iter()
            .filter(|task| task.run_id.as_deref() == Some(run_id))
            .map(|task| (task.id, task.key))
            .collect();
        let events = store.events_after(after)?;
        let cursor = events.last().map(|event| event.seq).unwrap_or(after);
        let mut changes = Vec::new();
        for event in events {
            if !COORDINATOR_EVENTS.contains(&event.kind.as_str()) {
                continue;
            }
            let key = match event.task_id.as_deref() {
                Some(task_id) => match keys.get(task_id) {
                    Some(key) => Some(key.clone()),
                    None => continue,
                },
                // Batch-wide events name their run first.
                None if event.detail.split(' ').next() == Some(run_id) => None,
                None => continue,
            };
            let role = event
                .job_id
                .as_deref()
                .and_then(|job_id| store.job(job_id).ok().flatten())
                .map(|job| job.role);
            changes.push(json!({
                "seq": event.seq,
                "at_ms": event.at_ms,
                "kind": event.kind,
                "task_id": event.task_id,
                "key": key,
                "job_id": event.job_id,
                "detail": event.detail,
                "text": change_text(&event, key.as_deref(), role.as_deref()),
            }));
        }
        Ok((changes, cursor))
    }

    /// Waits until something changes for the run's coordinator (a job
    /// ends, the user answers or decides a step, the PR moves…) and returns
    /// those changes with the cursor to wait from next. Without `cursor` it
    /// returns at once with the current one.
    pub fn wait_run(
        &self,
        run_id: &str,
        cursor: Option<u64>,
        timeout: Duration,
        idle_ok: bool,
    ) -> Result<Value, DeliveryError> {
        self.run(run_id)?;
        let deadline = Instant::now() + timeout;
        let _waiting = Waiting::start(self, run_id);
        let mut first_look = true;
        loop {
            let seen = self.inner.changes.0.lock().map(|count| *count).unwrap_or(0);
            let after = match cursor {
                Some(cursor) => cursor,
                None => self.store()?.last_event_seq()?,
            };
            let (events, next) = self.coordinator_events(run_id, after)?;
            let idle = self.idle_tasks(run_id)?;
            let nudge = first_look && !idle_ok && !idle.is_empty();
            let expired = Instant::now() >= deadline;
            if cursor.is_none() || !events.is_empty() || nudge || expired {
                self.record_coordinator_cursor(run_id, next)?;
                let mut reply = json!({
                    "cursor": next,
                    "events": events,
                    "waited": !first_look,
                    "idle": idle,
                    "open_jobs": self.open_jobs(run_id)?,
                    "spare_capacity": self.spare_capacity()?,
                });
                if nudge && events_is_empty(&reply) {
                    reply["note"] = json!("Not waiting: these tasks of the batch wait on you. Handle them (dispatch their next stage, ask for or record their step), then wait again; pass idle_ok only for tasks that wait on something outside Tinto.");
                }
                return Ok(reply);
            }
            first_look = false;
            // Sleep until the next change anywhere in Delivery, at most until
            // the deadline; the loop then checks whether it concerns the run.
            let (count, changed) = &self.inner.changes;
            let guard = count.lock().map_err(|_| lock_error())?;
            let left = deadline.saturating_duration_since(Instant::now());
            let _ = changed
                .wait_timeout_while(guard, left, |count| *count == seen)
                .map_err(|_| lock_error())?;
        }
    }

    fn open_jobs(&self, run_id: &str) -> Result<Vec<Value>, DeliveryError> {
        let store = self.store()?;
        let tasks: HashMap<String, String> = store
            .tasks()?
            .into_iter()
            .filter(|task| task.run_id.as_deref() == Some(run_id))
            .map(|task| (task.id, task.key))
            .collect();
        Ok(store
            .jobs()?
            .into_iter()
            .filter(|job| job.status.is_open())
            .filter_map(|job| {
                let key = tasks.get(&job.task_id)?;
                Some(json!({
                    "job_id": job.id,
                    "key": key,
                    "role": job.role,
                    "status": job.status,
                }))
            })
            .collect())
    }

    fn record_coordinator_cursor(&self, run_id: &str, cursor: u64) -> Result<(), DeliveryError> {
        let store = self.store()?;
        if let Some(mut run) = store.run(run_id)? {
            if run.coordinator_cursor < cursor {
                run.coordinator_cursor = cursor;
                store.put_run(&run)?;
            }
        }
        Ok(())
    }

    /// The Agents conversation a Delivery tool call came from becomes the
    /// run's coordinator: the one Tinto wakes.
    pub fn set_coordinator_session(
        &self,
        run_id: &str,
        session: &str,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        if let Some(mut run) = store.run(run_id)? {
            if run.coordinator_session.as_deref() != Some(session) {
                // A conversation that starts coordinating is woken only by
                // what happens from now on.
                run.coordinator_session = Some(session.to_string());
                run.notified_cursor = run.notified_cursor.max(store.last_event_seq()?);
                store.put_run(&run)?;
            }
        }
        Ok(())
    }

    /// For each active batch whose coordinator's conversation is known and
    /// not inside wait_run: the changes it has not seen, once quiet for
    /// [`WAKE_QUIET_MS`].
    pub fn pending_wakeups(&self, now: u64) -> Result<Vec<CoordinatorWakeup>, DeliveryError> {
        let runs: Vec<DeliveryRun> = self
            .store()?
            .runs()?
            .into_iter()
            .filter(|run| run.status == "active" && run.coordinator_session.is_some())
            .collect();
        let waiting = self
            .inner
            .waiting
            .lock()
            .map(|waiting| waiting.clone())
            .unwrap_or_default();
        let mut wakeups = Vec::new();
        for run in runs {
            if waiting.get(&run.id).is_some_and(|count| *count > 0) {
                continue;
            }
            let after = run.coordinator_cursor.max(run.notified_cursor);
            let (events, _) = self.coordinator_events(&run.id, after)?;
            let Some(newest) = events.last() else {
                continue;
            };
            let newest_at = newest["at_ms"].as_u64().unwrap_or(now);
            if now.saturating_sub(newest_at) < WAKE_QUIET_MS {
                continue;
            }
            let cursor = newest["seq"].as_u64().unwrap_or(after);
            wakeups.push(CoordinatorWakeup {
                text: wakeup_text(&run, &events, after),
                run_id: run.id,
                session_id: run.coordinator_session.unwrap_or_default(),
                cursor,
            });
        }
        Ok(wakeups)
    }

    /// For each active batch: the changes its coordinator has neither read
    /// with wait_run nor been woken for, and whether it waits now. A
    /// coordinator Tinto cannot wake (one outside Agents) shows here when
    /// it stops waiting with work pending.
    pub fn coordinator_states(&self) -> Result<Vec<DeliveryCoordinatorState>, DeliveryError> {
        let runs = self.store()?.runs()?;
        let waiting = self
            .inner
            .waiting
            .lock()
            .map(|waiting| waiting.clone())
            .unwrap_or_default();
        let mut states = Vec::new();
        for run in runs.into_iter().filter(|run| run.status == "active") {
            let after = run.coordinator_cursor.max(run.notified_cursor);
            let (events, _) = self.coordinator_events(&run.id, after)?;
            states.push(DeliveryCoordinatorState {
                waiting: waiting.get(&run.id).is_some_and(|count| *count > 0),
                unread: events.len() as u32,
                unread_since_ms: events.first().and_then(|event| event["at_ms"].as_u64()),
                run_id: run.id,
            });
        }
        Ok(states)
    }

    /// Tinto woke the run's coordinator up to `cursor`.
    pub fn mark_coordinator_notified(
        &self,
        run_id: &str,
        cursor: u64,
    ) -> Result<(), DeliveryError> {
        {
            let store = self.store()?;
            let Some(mut run) = store.run(run_id)? else {
                return Ok(());
            };
            run.notified_cursor = run.notified_cursor.max(cursor);
            run.notified_at_ms = Some(now_ms());
            store.put_run(&run)?;
        }
        self.notify();
        Ok(())
    }
}

fn events_is_empty(reply: &Value) -> bool {
    reply["events"].as_array().is_none_or(Vec::is_empty)
}

fn lock_error() -> DeliveryError {
    DeliveryError::new("delivery_lock_poisoned", "Delivery state is unavailable")
}

/// One line for the coordinator about an event.
fn change_text(event: &DeliveryEvent, key: Option<&str>, role: Option<&str>) -> String {
    let detail = event.detail.trim();
    let stage = role.unwrap_or("la etapa");
    let what = match event.kind.as_str() {
        "job_ended" => format!("la etapa {stage} terminó ({detail})"),
        "job_cancelled" => format!("la etapa {stage} se canceló"),
        "job_interrupted" => format!("la etapa {stage} se interrumpió"),
        "job_undone" => format!("el usuario deshizo los cambios de la etapa {stage}"),
        "approval_approved" => format!("el usuario aprobó el paso {detail}"),
        "approval_rejected" => format!("el usuario rechazó el paso {detail}"),
        "approval_completed" => format!("se registró el paso {detail}"),
        "approval_corrected" => format!("el usuario corrigió el resultado del paso {detail}"),
        "pr_found" => format!("Tinto enlazó el PR de la rama ({detail})"),
        "merge_found" => format!("el PR se fusionó fuera de Tinto ({detail})"),
        "step_drifted" => format!("un paso ya no coincide con git o GitHub ({detail})"),
        "step_matches" => format!("un paso vuelve a coincidir ({detail})"),
        "commit_missing" => "los commits que hizo Tinto ya no están en la rama".to_string(),
        "commit_undone" => "el usuario deshizo el commit".to_string(),
        "decision_answered" => "el usuario contestó una decisión".to_string(),
        "decision_changed" => "el usuario cambió una decisión".to_string(),
        "permission_renewed" => "el usuario renovó un permiso".to_string(),
        "settings_approved" => "el usuario aprobó un cambio de ajustes".to_string(),
        "settings_rejected" => "el usuario rechazó un cambio de ajustes".to_string(),
        "pr_changed" => format!("el PR cambió: {detail}"),
        "task_removed" => "el usuario eliminó la tarea".to_string(),
        "task_archived" => "el usuario archivó la tarea".to_string(),
        "task_unarchived" => "el usuario recuperó la tarea".to_string(),
        "task_updated" => "el usuario editó la tarea".to_string(),
        "run_policy" if detail.ends_with("paused") => "el usuario pausó el lote".to_string(),
        "run_policy" if detail.ends_with("resumed") => "el usuario reanudó el lote".to_string(),
        "run_takeover" => "otro coordinador tomó el lote".to_string(),
        "run_closed" => "el lote se cerró".to_string(),
        "qa_problems_sent" => format!(
            "el usuario envió problemas de la QA para arreglar ({detail}); léelos con read_qa_problems"
        ),
        other => format!("{other}: {detail}"),
    };
    let line: String = what.chars().take(240).collect();
    match key {
        Some(key) => format!("{key}: {line}"),
        None => line,
    }
}

/// The message Tinto sends to the coordinator's conversation.
fn wakeup_text(run: &DeliveryRun, events: &[Value], after: u64) -> String {
    const MAX_LINES: usize = 12;
    let mut lines: Vec<String> = events
        .iter()
        .take(MAX_LINES)
        .filter_map(|event| event["text"].as_str().map(|text| format!("- {text}")))
        .collect();
    if events.len() > MAX_LINES {
        lines.push(format!("- y {} cambios más", events.len() - MAX_LINES));
    }
    format!(
        "[Tinto] Cambios en el lote «{}» que esperan al coordinador:\n{}\n\nLlama a wait_run con run_id {} y cursor {after} para leerlos completos y sigue con el lote.",
        run.title,
        lines.join("\n"),
        run.id
    )
}
