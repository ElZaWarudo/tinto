//! What changed for a batch's coordinator, read from Delivery's audit trail
//! with a cursor (the events' `seq`).
//!
//! The coordinator waits for those changes with `wait_run`, which wakes on
//! the next one; one conversation may coordinate several batches and wait
//! on all of them at once. When it is not waiting, Tinto wakes its Agents
//! conversation with one message per burst of changes (see
//! `agent_console::commands::start_delivery_waker`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::model::{
    DeliveryCoordinatorState, DeliveryEvent, DeliveryJobStatus, DeliveryLeaseState, DeliveryRun,
    DeliveryUnblock,
};
use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

/// Events about something the coordinator has to act on or know. What the
/// coordinator itself asks (dispatches, requests) does not wake it.
const COORDINATOR_EVENTS: &[&str] = &[
    "job_ended",
    "checks_ended",
    "setup_ended",
    "job_cancelled",
    "job_interrupted",
    "job_retried",
    "job_not_retried",
    "job_undone",
    "job_resolved_by_user",
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
    "package_retargeted",
    "task_removed",
    "task_archived",
    "task_unarchived",
    "task_updated",
    "run_policy",
    "run_takeover",
    "run_closed",
    "qa_problems_sent",
    "round_opened",
    "round_changed",
    "blocker_cleared",
    "blockers_changed",
    "guides_requested",
];

/// Changes acquire_run and takeover_run sum up for the coordinator: what
/// the user decided that it has not acknowledged.
const PENDING_EVENTS: &[&str] = &[
    "decision_answered",
    "decision_changed",
    "approval_approved",
    "approval_rejected",
    "qa_problems_sent",
];

/// At most this many unacknowledged changes come again in a wait_run
/// without cursor: the newest.
const MAX_REDELIVERED: usize = 50;

/// Changes are sent to the coordinator's conversation once they have been
/// quiet this long, so a burst (several answers, a PR and its checks) goes
/// in one message.
pub const WAKE_QUIET_MS: u64 = 5_000;

/// A message for a coordinator's conversation, about the batches it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinatorWakeup {
    pub session_id: String,
    /// Each batch the message covers, with the last event it covers.
    pub runs: Vec<(String, u64)>,
    pub text: String,
}

/// Counts a coordinator inside wait_run, on each of its batches, for as
/// long as it waits.
pub(super) struct Waiting<'a> {
    service: &'a DeliveryService,
    run_ids: Vec<String>,
}

impl<'a> Waiting<'a> {
    pub(super) fn start(service: &'a DeliveryService, run_ids: &[String]) -> Self {
        if let Ok(mut waiting) = service.inner.waiting.lock() {
            for run_id in run_ids {
                *waiting.entry(run_id.clone()).or_default() += 1;
            }
        }
        Self {
            service,
            run_ids: run_ids.to_vec(),
        }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if let Ok(mut waiting) = self.service.inner.waiting.lock() {
            for run_id in &self.run_ids {
                if let Some(count) = waiting.get_mut(run_id) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        waiting.remove(run_id);
                    }
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
            .map(|task| (task.id.clone(), task.label()))
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
                "run_id": run_id,
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

    /// Waits until something changes for the coordinator of any of
    /// `run_ids` (a job ends, the user answers or decides a step, the PR
    /// moves…) and returns those changes, each naming its batch, with the
    /// cursor to wait from next. Passing `cursor` acknowledges every change
    /// up to it. Without `cursor` it returns at once with the current one
    /// and the changes not acknowledged yet: those handed over before (the
    /// coordinator was interrupted after reading them) come `redelivered`.
    pub fn wait_run(
        &self,
        run_ids: &[String],
        cursor: Option<u64>,
        timeout: Duration,
        idle_ok: bool,
    ) -> Result<Value, DeliveryError> {
        for run_id in run_ids {
            self.run(run_id)?;
        }
        if let Some(cursor) = cursor {
            for run_id in run_ids {
                self.acknowledge_cursor(run_id, cursor)?;
            }
        }
        let deadline = Instant::now() + timeout;
        let _waiting = Waiting::start(self, run_ids);
        let mut first_look = true;
        loop {
            let seen = self.inner.changes.0.lock().map(|count| *count).unwrap_or(0);
            let mut next = match cursor {
                Some(cursor) => cursor,
                None => self.store()?.last_event_seq()?,
            };
            let mut events = Vec::new();
            let mut idle = Vec::new();
            for run_id in run_ids {
                match cursor {
                    Some(cursor) => {
                        let (run_events, run_next) = self.coordinator_events(run_id, cursor)?;
                        events.extend(run_events);
                        next = next.max(run_next);
                    }
                    None => events.extend(self.unacknowledged(run_id)?),
                }
                idle.extend(self.idle_tasks(run_id)?);
            }
            events.sort_by_key(|event| event["seq"].as_u64());
            let nudge = first_look && !idle_ok && !idle.is_empty();
            let expired = Instant::now() >= deadline;
            if cursor.is_none() || !events.is_empty() || nudge || expired {
                let mut open_jobs = Vec::new();
                for run_id in run_ids {
                    self.record_returned_cursor(run_id, next)?;
                    open_jobs.extend(self.open_jobs(run_id)?);
                }
                let mut reply = json!({
                    "cursor": next,
                    "events": events,
                    "waited": !first_look,
                    "idle": idle,
                    "open_jobs": open_jobs,
                    "spare_capacity": self.spare_capacity()?,
                    "quotas": super::queue::quotas(&*self.store()?)?,
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

    /// The run's changes its coordinator has not acknowledged, the newest
    /// [`MAX_REDELIVERED`]; those it was handed before come `redelivered`.
    /// A takeover is news for the coordinator it replaced, not for this one.
    fn unacknowledged(&self, run_id: &str) -> Result<Vec<Value>, DeliveryError> {
        let run = self.run(run_id)?;
        let handed = handed_cursor(&run);
        let (mut events, _) = self.coordinator_events(run_id, run.coordinator_cursor)?;
        events.retain(|event| event["kind"] != "run_takeover");
        for event in &mut events {
            if event["seq"].as_u64().is_some_and(|seq| seq <= handed) {
                event["redelivered"] = json!(true);
            }
        }
        let skip = events.len().saturating_sub(MAX_REDELIVERED);
        Ok(events.split_off(skip))
    }

    /// What waits on the run's coordinator since the cursor it last
    /// acknowledged, one line each: tasks whose blockers cleared, answered
    /// decisions, decided steps and QA problems sent. acquire_run and
    /// takeover_run hand it over, so an interrupted coordinator, or the
    /// next one, starts from it.
    pub fn pending_summary(&self, run_id: &str) -> Result<Vec<Value>, DeliveryError> {
        let run = self.run(run_id)?;
        let mut pending = Vec::new();
        for task in self.store()?.tasks()? {
            if task.run_id.as_deref() != Some(run_id) {
                continue;
            }
            if let Some(unblock) = &task.unblocked {
                pending.push(json!({
                    "kind": "unblocked",
                    "task_id": task.id,
                    "key": task.label(),
                    "text": format!("{}: {}", task.label(), unblock_text(unblock)),
                }));
            }
        }
        let (events, _) = self.coordinator_events(run_id, run.coordinator_cursor)?;
        for event in events {
            if !PENDING_EVENTS.contains(&event["kind"].as_str().unwrap_or_default()) {
                continue;
            }
            pending.push(json!({
                "kind": event["kind"],
                "seq": event["seq"],
                "task_id": event["task_id"],
                "key": event["key"],
                "text": event["text"],
            }));
        }
        Ok(pending)
    }

    fn open_jobs(&self, run_id: &str) -> Result<Vec<Value>, DeliveryError> {
        let store = self.store()?;
        let tasks: HashMap<String, String> = store
            .tasks()?
            .into_iter()
            .filter(|task| task.run_id.as_deref() == Some(run_id))
            .map(|task| (task.id.clone(), task.label()))
            .collect();
        let jobs = store.jobs()?;
        let leases = store.leases()?;
        drop(store);
        // Who releases a quarantined resource the batch waits on.
        let releases = match self.brainstorm_waits(&self.run(run_id)?)? {
            None => "coordinator",
            Some(_) => "user",
        };
        Ok(jobs
            .iter()
            .filter(|job| job.status.is_open())
            .filter_map(|job| {
                let key = tasks.get(&job.task_id)?;
                let mut open = json!({
                    "run_id": run_id,
                    "job_id": job.id,
                    "key": key,
                    "role": job.role,
                    "status": job.status,
                });
                // Not just queued: it waits until someone releases it.
                let quarantined = leases.iter().find(|lease| {
                    job.status == DeliveryJobStatus::Queued
                        && Some(lease.name.as_str()) == job.lease.as_deref()
                        && lease.state == DeliveryLeaseState::Quarantined
                        && !lease
                            .holder_job_id
                            .as_deref()
                            .and_then(|holder| jobs.iter().find(|other| other.id == holder))
                            .is_some_and(|holder| {
                                holder.task_id == job.task_id && holder.role == job.role
                            })
                });
                if let Some(lease) = quarantined {
                    open["waits_quarantine"] = json!(lease.name);
                    open["waits_on"] = json!(releases);
                    open["quarantine_note"] = json!(lease.note);
                }
                Some(open)
            })
            .collect())
    }

    /// The coordinator called wait_run with `cursor`: it handled every
    /// change it was handed up to there.
    fn acknowledge_cursor(&self, run_id: &str, cursor: u64) -> Result<(), DeliveryError> {
        let store = self.store()?;
        if let Some(mut run) = store.run(run_id)? {
            let acknowledged = cursor.min(handed_cursor(&run));
            if run.coordinator_cursor < acknowledged {
                run.coordinator_cursor = acknowledged;
                store.put_run(&run)?;
            }
        }
        Ok(())
    }

    fn record_returned_cursor(&self, run_id: &str, cursor: u64) -> Result<(), DeliveryError> {
        let store = self.store()?;
        if let Some(mut run) = store.run(run_id)? {
            if run.returned_cursor < cursor {
                run.returned_cursor = cursor;
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

    /// A coordinator's conversation resumed under another id (Tinto
    /// restarted): its batches follow it, and it hears what it missed. Says
    /// whether it coordinates any.
    pub fn rebind_coordinator(&self, before: &str, after: &str) -> Result<bool, DeliveryError> {
        let store = self.store()?;
        let mut coordinates = false;
        for mut run in store.runs()? {
            if run.coordinator_session.as_deref() == Some(before) {
                run.coordinator_session = Some(after.to_string());
                store.put_run(&run)?;
                coordinates = true;
            }
        }
        Ok(coordinates)
    }

    /// For each conversation that coordinates active batches: the changes
    /// it has not seen in those it is not waiting on inside wait_run, once
    /// they have been quiet for [`WAKE_QUIET_MS`]; one message for all its
    /// batches.
    pub fn pending_wakeups(&self, now: u64) -> Result<Vec<CoordinatorWakeup>, DeliveryError> {
        let mut sessions: Vec<(String, Vec<DeliveryRun>)> = Vec::new();
        for run in self.store()?.runs()? {
            let Some(session) = run.coordinator_session.clone() else {
                continue;
            };
            if run.status != "active" {
                continue;
            }
            match sessions.iter_mut().find(|(id, _)| *id == session) {
                Some((_, runs)) => runs.push(run),
                None => sessions.push((session, vec![run])),
            }
        }
        let waiting = self
            .inner
            .waiting
            .lock()
            .map(|waiting| waiting.clone())
            .unwrap_or_default();
        let mut wakeups = Vec::new();
        for (session_id, held) in sessions {
            let mut changed = Vec::new();
            for run in &held {
                if waiting.get(&run.id).is_some_and(|count| *count > 0) {
                    continue;
                }
                let after = handed_cursor(run);
                let (events, _) = self.coordinator_events(&run.id, after)?;
                let Some(newest) = events.last() else {
                    continue;
                };
                let newest_at = newest["at_ms"].as_u64().unwrap_or(now);
                if now.saturating_sub(newest_at) < WAKE_QUIET_MS {
                    continue;
                }
                let cursor = newest["seq"].as_u64().unwrap_or(after);
                changed.push((run, events, cursor));
            }
            if changed.is_empty() {
                continue;
            }
            wakeups.push(CoordinatorWakeup {
                text: wakeup_text(&held, &changed),
                runs: changed
                    .iter()
                    .map(|(run, _, cursor)| (run.id.clone(), *cursor))
                    .collect(),
                session_id,
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
            let after = handed_cursor(&run);
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

/// The newest change the run's coordinator was handed, by wait_run or a
/// wake-up.
fn handed_cursor(run: &DeliveryRun) -> u64 {
    run.coordinator_cursor
        .max(run.returned_cursor)
        .max(run.notified_cursor)
}

/// How a task's blockers cleared, for its coordinator.
fn unblock_text(unblock: &DeliveryUnblock) -> String {
    let how = match unblock.by.as_str() {
        "user" => "el usuario lo quitó en Tinto",
        _ => "se resolvió solo",
    };
    format!(
        "se despejó el bloqueo {} ({how}); la tarea espera que la retomes",
        unblock.cleared
    )
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
        "checks_ended" if detail == "pasaron" => {
            "las comprobaciones de Tinto pasaron en el código de la última etapa".to_string()
        }
        "checks_ended" => format!(
            "las comprobaciones de Tinto no pasaron (si alguna falló, lanza implementation: sus instrucciones ya llevan la salida de cada fallo): {detail}"
        ),
        "setup_ended" if detail == "pasaron" => {
            "la preparación de Tinto terminó: ya puedes lanzar la siguiente etapa".to_string()
        }
        "setup_ended" => format!(
            "la preparación de Tinto no pasó; ninguna etapa empieza hasta que la reintentes con retry_job: {detail}"
        ),
        "job_cancelled" => format!("la etapa {stage} se canceló"),
        "job_interrupted" if detail.is_empty() => format!("la etapa {stage} se interrumpió"),
        "job_interrupted" => format!("la etapa {stage} se interrumpió al cerrarse Tinto; {detail}"),
        "job_retried" => format!(
            "Tinto reintentó la etapa {stage} al reiniciar (se había interrumpido al cerrarse); el trabajo nuevo está en cola"
        ),
        "job_not_retried" => {
            format!("la etapa {stage} interrumpida no se pudo reintentar al reiniciar: {detail}")
        }
        "job_undone" => format!("el usuario deshizo los cambios de la etapa {stage}"),
        "job_resolved_by_user" => format!(
            "el usuario dio por resuelta la etapa {stage} sin repetirla ({detail}); sigue con la siguiente"
        ),
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
        "package_retargeted" => format!("Tinto cambió la base de su PR: {detail}"),
        "task_removed" => "el usuario eliminó la tarea".to_string(),
        "task_archived" => "el usuario archivó la tarea".to_string(),
        "task_unarchived" => "el usuario recuperó la tarea".to_string(),
        "task_updated" if detail.is_empty() => "el usuario editó la tarea".to_string(),
        "task_updated" => format!("el usuario editó la tarea: {detail}"),
        "blocker_cleared" => format!("se despejó el bloqueo {detail}"),
        "blockers_changed" => format!("el usuario cambió los bloqueos: {detail}"),
        "run_policy" if detail.ends_with("paused") => "el usuario pausó el lote".to_string(),
        "run_policy" if detail.ends_with("resumed") => "el usuario reanudó el lote".to_string(),
        "run_policy" if detail.ends_with("no_new_tasks") => {
            "el usuario pidió no crear más tareas en el lote: termina las que tiene (create_task las rechaza)".to_string()
        }
        "run_policy" if detail.ends_with("new_tasks_allowed") => {
            "el usuario volvió a permitir tareas nuevas en el lote".to_string()
        }
        "run_takeover" => "otro coordinador tomó el lote".to_string(),
        "run_closed" => "el lote se cerró".to_string(),
        "guides_requested" => format!(
            "el usuario pide destilar las guías del repositorio: {} comentarios nuevos de sus revisores guía. Léelos con read_guide_comments y propón las reglas con propose_guide",
            detail.split(' ').nth(1).unwrap_or("0")
        ),
        "round_opened" => format!(
            "se abrió una ronda: {detail}. El trabajo vuelve a implementation; las etapas que lances la llevan en sus instrucciones"
        ),
        "round_changed" => format!("la ronda abierta cambió: {detail}"),
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

/// The message Tinto sends to a coordinator's conversation: the changes of
/// each batch in `changed`, and how to read them all from the batches it
/// holds (`held`).
fn wakeup_text(held: &[DeliveryRun], changed: &[(&DeliveryRun, Vec<Value>, u64)]) -> String {
    const MAX_LINES: usize = 12;
    let mut sections = Vec::new();
    for (run, events, _) in changed {
        let mut lines: Vec<String> = events
            .iter()
            .take(MAX_LINES)
            .filter_map(|event| event["text"].as_str().map(|text| format!("- {text}")))
            .collect();
        if events.len() > MAX_LINES {
            lines.push(format!("- y {} cambios más", events.len() - MAX_LINES));
        }
        sections.push(format!(
            "[Tinto] Cambios en el lote «{}» que esperan al coordinador:\n{}",
            run.title,
            lines.join("\n")
        ));
    }
    // From the oldest change any of its batches has not had read.
    let after = held.iter().map(handed_cursor).min().unwrap_or(0);
    let read = match held {
        [run] => format!(
            "Llama a wait_run con run_id {} y cursor {after} para leerlos completos y sigue con el lote.",
            run.id
        ),
        _ => format!(
            "Llama a wait_run con `runs` (cada lote que coordinas: {}, con su owner y generation) y cursor {after} para leerlos completos y sigue con los lotes.",
            run_list(held)
        ),
    };
    format!("{}\n\n{read}", sections.join("\n\n"))
}

/// The message that asks an Agents conversation to coordinate `runs`.
pub fn handover_text(runs: &[DeliveryRun]) -> String {
    match runs {
        [run] => format!(
            "[Tinto] El usuario te pide coordinar el lote «{}» (run_id {}). Tómalo con takeover_run (run_id {}, tu owner, reason «pedido por el usuario en Tinto»); no hace falta comprobar al coordinador anterior. Después llama a wait_run sin cursor, lee delivery_overview y sigue con el lote.",
            run.title, run.id, run.id
        ),
        _ => format!(
            "[Tinto] El usuario te pide coordinar {} lotes en esta conversación: {}. Toma cada uno con takeover_run (su run_id, tu owner, reason «pedido por el usuario en Tinto»); no hace falta comprobar a los coordinadores anteriores. Después llama a wait_run sin cursor con `runs` (cada lote con su run_id, owner y generation): una sola espera los cubre a todos. Lee delivery_overview y sigue con las tareas de todos los lotes a la vez.",
            runs.len(),
            run_list(runs)
        ),
    }
}

/// «Title» (run_id …) of each batch.
fn run_list(runs: &[DeliveryRun]) -> String {
    runs.iter()
        .map(|run| format!("«{}» (run_id {})", run.title, run.id))
        .collect::<Vec<_>>()
        .join(", ")
}
