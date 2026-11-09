//! The review threads of a task's PR and whether each has a reply. Every
//! thread a reviewer opens must have one from the PR's author before the PR
//! goes back to review: a short interim reply is enough while its fix takes
//! more rounds. Tinto only follows and blocks; replies still go through
//! pr_reply.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::prs::DeliveryPr;
use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

/// How long a thread may go without a reply before wait_run says so.
pub const THREAD_ALERT_MS: u64 = 4 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Unanswered,
    /// The PR's author replied (through pr_reply or on GitHub).
    Answered,
    /// Resolved on GitHub, by anyone.
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewThread {
    /// Its first comment's id on GitHub.
    pub id: u64,
    pub url: String,
    pub author: String,
    pub path: String,
    pub line: Option<u32>,
    /// When the reviewer opened it (GitHub timestamp).
    pub created_at: String,
    pub state: ThreadState,
    /// Minutes without a reply, in `unanswered_threads`.
    #[serde(default)]
    pub waiting_minutes: u64,
    /// wait_run was told it passed THREAD_ALERT_MS.
    #[serde(default)]
    pub alerted: bool,
}

impl ReviewThread {
    /// "https://…#discussion_r1 (ana en src/a.rs:12)".
    fn label(&self) -> String {
        let at = match self.line {
            Some(line) => format!("{}:{line}", self.path),
            None => self.path.clone(),
        };
        format!("{} ({} en {at})", self.url, self.author)
    }
}

/// Milliseconds since the epoch of a GitHub timestamp
/// (`2026-11-05T11:50:00Z`, optionally with fractions).
pub fn github_ms(time: &str) -> Option<u64> {
    let (date, clock) = time.trim().trim_end_matches('Z').split_once('T')?;
    let mut date = date.split('-').map(|part| part.parse::<i64>());
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let clock = clock.split('.').next()?;
    let mut clock = clock.split(':').map(|part| part.parse::<i64>());
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    // Days from the civil date (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds * 1000).ok()
}

/// Brings `pr.review_threads` in line with a snapshot: its review comments
/// (`reviewComments`) and the first comment of each resolved thread
/// (`resolvedThreads`, which `GhCli::pr` adds). A snapshot without comments
/// leaves the threads as they were; one without `resolvedThreads` keeps
/// what was resolved.
pub fn follow_threads(pr: &mut DeliveryPr, snapshot: &Value, me: &str) {
    let Some(comments) = snapshot["reviewComments"].as_array() else {
        return;
    };
    let owner = snapshot["author"]["login"].as_str().unwrap_or_default();
    let ours = |comment: &Value| {
        let login = comment["user"]["login"].as_str().unwrap_or_default();
        !login.is_empty() && (login.eq_ignore_ascii_case(me) || login.eq_ignore_ascii_case(owner))
    };
    let resolved: Option<Vec<u64>> = snapshot["resolvedThreads"]
        .as_array()
        .map(|ids| ids.iter().filter_map(Value::as_u64).collect());
    let before = std::mem::take(&mut pr.review_threads);
    pr.review_threads = comments
        .iter()
        .filter(|comment| comment["in_reply_to_id"].as_u64().is_none() && !ours(comment))
        .filter_map(|root| {
            let id = root["id"].as_u64()?;
            let previous = before.iter().find(|thread| thread.id == id);
            let is_resolved = match &resolved {
                Some(ids) => ids.contains(&id),
                None => previous.is_some_and(|thread| thread.state == ThreadState::Resolved),
            };
            let replied = comments
                .iter()
                .any(|reply| reply["in_reply_to_id"].as_u64() == Some(id) && ours(reply));
            let text = |key: &str| root[key].as_str().unwrap_or_default().to_string();
            Some(ReviewThread {
                id,
                url: text("html_url"),
                author: root["user"]["login"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                path: text("path"),
                line: root["line"]
                    .as_u64()
                    .or_else(|| root["original_line"].as_u64())
                    .map(|line| line as u32),
                created_at: text("created_at"),
                state: if is_resolved {
                    ThreadState::Resolved
                } else if replied {
                    ThreadState::Answered
                } else {
                    ThreadState::Unanswered
                },
                waiting_minutes: 0,
                alerted: previous.is_some_and(|thread| thread.alerted),
            })
        })
        .collect();
}

impl DeliveryPr {
    /// Its threads still without a reply, while it is open.
    pub fn unanswered(&self) -> impl Iterator<Item = &ReviewThread> {
        self.review_threads
            .iter()
            .filter(|thread| self.is_open() && thread.state == ThreadState::Unanswered)
    }

    /// Fills `unanswered_threads` with how long each has waited at `now`.
    pub fn count_waits(&mut self, now: u64) {
        self.unanswered_threads = self
            .unanswered()
            .cloned()
            .map(|mut thread| {
                thread.waiting_minutes = github_ms(&thread.created_at)
                    .map_or(0, |since| now.saturating_sub(since) / 60_000);
                thread
            })
            .collect();
    }

    /// Marks the threads that passed THREAD_ALERT_MS without a reply as
    /// told, and returns one line for each.
    pub fn due_alerts(&mut self, now: u64) -> Vec<String> {
        let open = self.is_open();
        self.review_threads
            .iter_mut()
            .filter(|thread| open && thread.state == ThreadState::Unanswered && !thread.alerted)
            .filter_map(|thread| {
                let since = github_ms(&thread.created_at)?;
                (now.saturating_sub(since) >= THREAD_ALERT_MS).then(|| {
                    thread.alerted = true;
                    format!(
                        "{} lleva {} h sin respuesta",
                        thread.label(),
                        now.saturating_sub(since) / 3_600_000
                    )
                })
            })
            .collect()
    }

    /// Refusal of a step that sends the PR back to review while threads
    /// still have no reply, naming each.
    pub fn threads_refusal(&self, what: &str) -> Option<DeliveryError> {
        let open: Vec<String> = self.unanswered().map(ReviewThread::label).collect();
        if open.is_empty() {
            return None;
        }
        Some(DeliveryError::new(
            "threads_unanswered",
            format!(
                "{what}, y {} hilo(s) de revisión siguen sin respuesta: {}. Responde cada uno con pr_reply antes (sin resolve_thread mientras su arreglo no esté publicado)",
                open.len(),
                open.join("; ")
            ),
        ))
    }
}

impl DeliveryService {
    /// Refuses a step that sends the task's PR back to its reviewers while a
    /// review thread has no reply. A refusal reads GitHub again first: a
    /// reply posted there since the last read counts.
    pub(super) fn check_threads_answered(
        &self,
        task_id: &str,
        what: &str,
    ) -> Result<(), DeliveryError> {
        let refused = |pr: Option<DeliveryPr>| pr.and_then(|pr| pr.threads_refusal(what));
        if refused(self.store()?.pr(task_id)?).is_none() {
            return Ok(());
        }
        match refused(self.refresh_pr(task_id, std::time::Duration::ZERO)?) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// A reply Tinto posted in thread `first` answers it (and resolves it,
    /// when asked), before GitHub is read again.
    pub(super) fn mark_thread_answered(
        &self,
        task_id: &str,
        first: u64,
        resolved: bool,
    ) -> Result<(), DeliveryError> {
        let store = self.store()?;
        let Some(mut pr) = store.pr(task_id)? else {
            return Ok(());
        };
        let Some(thread) = pr
            .review_threads
            .iter_mut()
            .find(|thread| thread.id == first)
        else {
            return Ok(());
        };
        thread.state = if resolved {
            ThreadState::Resolved
        } else if thread.state == ThreadState::Unanswered {
            ThreadState::Answered
        } else {
            thread.state
        };
        pr.count_waits(now_ms());
        store.put_pr(&pr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn github_timestamps_become_epoch_milliseconds() {
        assert_eq!(github_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(github_ms("2026-11-05T11:50:00Z"), Some(1_793_879_400_000));
        assert_eq!(github_ms("2024-02-29T00:00:01.5Z"), Some(1_709_164_801_000));
        assert_eq!(github_ms("ayer"), None);
    }

    fn comment(id: u64, reply_to: Option<u64>, user: &str) -> Value {
        json!({
            "id": id, "in_reply_to_id": reply_to, "user": {"login": user},
            "path": "src/a.rs", "line": id, "created_at": "2026-11-05T11:50:00Z",
            "html_url": format!("https://github.com/o/r/pull/5#discussion_r{id}"), "body": "x"
        })
    }

    fn pr() -> DeliveryPr {
        let mut pr = DeliveryPr::linked(
            "t1",
            &super::super::prs::parse_pr_url("https://github.com/o/r/pull/5").unwrap(),
        );
        pr.state = "open".into();
        pr
    }

    #[test]
    fn a_thread_counts_as_answered_by_the_authors_reply_or_once_resolved() {
        let mut pr = pr();
        let snapshot = json!({
            "author": {"login": "owner"},
            "reviewComments": [
                comment(1, None, "ana"), comment(2, None, "ana"), comment(3, None, "ana"),
                comment(4, None, "me"),
                // Replies: by the user, by the PR's author, by the reviewer.
                comment(10, Some(1), "me"), comment(11, Some(2), "owner"), comment(12, Some(3), "ana"),
                comment(5, None, "luis"),
            ],
            "resolvedThreads": [5],
        });
        follow_threads(&mut pr, &snapshot, "me");
        let states: Vec<(u64, ThreadState)> =
            pr.review_threads.iter().map(|t| (t.id, t.state)).collect();
        assert_eq!(
            states,
            [
                (1, ThreadState::Answered),
                (2, ThreadState::Answered),
                (3, ThreadState::Unanswered),
                (5, ThreadState::Resolved),
            ]
        );
        // A read without the comments keeps what Tinto knew.
        follow_threads(&mut pr, &json!({}), "me");
        assert_eq!(pr.review_threads.len(), 4);
    }

    #[test]
    fn a_thread_alerts_once_after_four_hours_without_a_reply() {
        let mut pr = pr();
        follow_threads(
            &mut pr,
            &json!({"reviewComments": [comment(1, None, "ana"), comment(2, None, "ana")]}),
            "me",
        );
        let opened = github_ms("2026-11-05T11:50:00Z").unwrap();
        assert!(pr.due_alerts(opened + THREAD_ALERT_MS - 1).is_empty());
        let alerts = pr.due_alerts(opened + THREAD_ALERT_MS);
        assert_eq!(alerts.len(), 2);
        assert!(alerts[0].contains("discussion_r1 (ana en src/a.rs:1) lleva 4 h"));
        assert!(pr.due_alerts(opened + 2 * THREAD_ALERT_MS).is_empty());
        // The alert stays told across reads.
        follow_threads(
            &mut pr,
            &json!({"reviewComments": [comment(1, None, "ana"), comment(2, None, "ana")]}),
            "me",
        );
        assert!(pr.due_alerts(opened + 3 * THREAD_ALERT_MS).is_empty());
        pr.count_waits(opened + 90 * 60_000);
        assert_eq!(pr.unanswered_threads[0].waiting_minutes, 90);
    }
}
