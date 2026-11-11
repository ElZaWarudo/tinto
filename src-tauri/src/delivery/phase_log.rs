//! A `phase_changed` event each time a task's phase changes, so the trail
//! says how long each task waited on whom. The phase is never stored: it is
//! compared, where the overview derives it, with the last one recorded.

use std::collections::{BTreeMap, HashMap};

use super::phase::{DeliveryTaskPhase, Phase, Waiter};
use super::service::DeliveryService;
use super::store::DeliveryStore;
use super::{now_ms, DeliveryError};

pub const PHASE_CHANGED: &str = "phase_changed";

/// `kind · waits_on`, with a PR's wait after its kind (`pr:review ·
/// reviewers`), in their serialized names; nothing that changes by itself
/// (positions, times).
fn label(phase: &Phase, waits_on: Waiter) -> String {
    let phase = serde_json::to_value(phase).unwrap_or_default();
    let mut kind = phase["kind"].as_str().unwrap_or_default().to_string();
    if let Some(wait) = phase["wait"]["kind"].as_str() {
        kind = format!("{kind}:{wait}");
    }
    let waits_on = serde_json::to_value(waits_on).unwrap_or_default();
    format!("{kind} · {}", waits_on.as_str().unwrap_or_default())
}

/// What to record, by task, and the labels to compare with next time: only
/// the tasks given, so removed and archived ones are forgotten.
fn changes<'a>(
    seen: &HashMap<String, String>,
    phases: impl IntoIterator<Item = (&'a str, &'a Phase, Waiter)>,
) -> (Vec<(String, String)>, HashMap<String, String>) {
    let mut record = Vec::new();
    let mut now = HashMap::new();
    for (task_id, phase, waits_on) in phases {
        let label = label(phase, waits_on);
        if seen.get(task_id) != Some(&label) {
            record.push((task_id.to_string(), label.clone()));
        }
        now.insert(task_id.to_string(), label);
    }
    (record, now)
}

impl DeliveryService {
    /// Records the phases that changed since the last overview. After a
    /// restart, or a failed write, they are compared with each task's last
    /// `phase_changed`. It notifies no one: nothing waits on it, and an
    /// overview it woke would only find the same phases.
    pub(super) fn record_phase_changes(
        &self,
        store: &DeliveryStore,
        phases: &BTreeMap<String, DeliveryTaskPhase>,
    ) -> Result<(), DeliveryError> {
        let mut seen = self
            .inner
            .phases_seen
            .lock()
            .map_err(|_| DeliveryError::new("delivery_lock_poisoned", "phases unavailable"))?;
        let last = match seen.take() {
            Some(last) => last,
            None => store.latest_task_details(PHASE_CHANGED)?,
        };
        let (record, now) = changes(
            &last,
            phases
                .iter()
                .map(|(task_id, phase)| (task_id.as_str(), &phase.phase, phase.waits_on)),
        );
        let at = now_ms();
        for (task_id, detail) in &record {
            store.record_event(at, PHASE_CHANGED, Some(task_id), None, detail)?;
        }
        *seen = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery::phase::PrWait;

    fn queued(position: usize) -> Phase {
        Phase::Queued {
            job_id: "j".into(),
            role: "tests".into(),
            lease: None,
            position,
            reason: None,
            since_ms: Some(1),
        }
    }

    fn pr(wait: PrWait) -> Phase {
        Phase::Pr {
            number: 7,
            ci: None,
            wait,
        }
    }

    #[test]
    fn records_only_what_changed_in_kind_wait_or_waiter() {
        let review = || PrWait::Review {
            reviewers: vec!["ana".into()],
        };
        let steps: Vec<(Vec<(&str, Phase, Waiter)>, Vec<(&str, &str)>)> = vec![
            // First sighting.
            (
                vec![
                    ("a", queued(2), Waiter::Queue),
                    ("b", pr(review()), Waiter::Reviewers),
                ],
                vec![("a", "queued · queue"), ("b", "pr:review · reviewers")],
            ),
            // No change, and a queue position does not count.
            (
                vec![
                    ("a", queued(1), Waiter::Queue),
                    ("b", pr(review()), Waiter::Reviewers),
                ],
                vec![],
            ),
            // Kind change; a PR's wait change.
            (
                vec![
                    (
                        "a",
                        Phase::Running {
                            job_id: "j".into(),
                            role: "tests".into(),
                        },
                        Waiter::Agent,
                    ),
                    ("b", pr(PrWait::Checks), Waiter::External),
                ],
                vec![("a", "running · agent"), ("b", "pr:checks · external")],
            ),
            // waits_on change alone; b is gone and forgotten.
            (
                vec![(
                    "a",
                    Phase::Running {
                        job_id: "j".into(),
                        role: "tests".into(),
                    },
                    Waiter::User,
                )],
                vec![("a", "running · user")],
            ),
        ];
        let mut seen = HashMap::new();
        for (phases, expected) in steps {
            let (record, now) = changes(
                &seen,
                phases
                    .iter()
                    .map(|(id, phase, waiter)| (*id, phase, *waiter)),
            );
            let expected: Vec<(String, String)> = expected
                .into_iter()
                .map(|(id, detail)| (id.to_string(), detail.to_string()))
                .collect();
            assert_eq!(record, expected);
            assert_eq!(now.len(), phases.len());
            seen = now;
        }
    }
}
