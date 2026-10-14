//! Following a task's pull request on GitHub after its PR step.
//!
//! - Marking the PR step done with a GitHub URL links the PR to the task.
//! - GitHub cannot push to a desktop app without a webhook (which needs
//!   admin rights on the repo and a public relay), so Tinto reads the user's
//!   notifications feed: one conditional request per interval GitHub sets,
//!   free when nothing changed (304), and only while some PR is open. A PR is
//!   read in full only when the feed mentions it, while its checks are still
//!   running, or when someone opens it in Tinto.
//! - Activity newer than what the user last saw counts as new; reading it
//!   (or starting a stage on the task) moves that mark.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::service::DeliveryService;
use super::{now_ms, DeliveryError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub url: String,
}

/// The first `https://github.com/<owner>/<repo>/pull/<n>` in a text.
pub fn parse_pr_url(text: &str) -> Option<PrRef> {
    let start = text.find("https://github.com/")?;
    let rest = &text[start + "https://github.com/".len()..];
    let path: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ')' && *c != '>' && *c != '"')
        .collect();
    let mut parts = path.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    // Only the digits: a link can end a sentence or carry `#…`/`?…`.
    let digits: String = number.chars().take_while(char::is_ascii_digit).collect();
    let number: u64 = digits.parse().ok()?;
    if owner.is_empty() || repo.is_empty() || kind != "pull" {
        return None;
    }
    Some(PrRef {
        repo: format!("{owner}/{repo}"),
        number,
        url: format!("https://github.com/{owner}/{repo}/pull/{number}"),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrChecks {
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryPr {
    pub task_id: String,
    pub url: String,
    pub repo: String,
    pub number: u64,
    /// `open`, `merged`, `closed`, or `unknown` until the first read.
    pub state: String,
    pub draft: bool,
    pub conflicts: bool,
    /// `approved`, `changes_requested`, `review_required` or none.
    pub review_decision: Option<String>,
    pub checks: PrChecks,
    /// Reviews and comments by others newer than `seen_until`.
    pub new_reviews: u32,
    pub new_comments: u32,
    pub new_authors: Vec<String>,
    /// GitHub timestamp of the newest activity Tinto has read.
    pub latest_activity: Option<String>,
    /// Activity up to this GitHub timestamp is not new. Set to the newest
    /// activity on the first read, so what came before linking is not news.
    pub seen_until: Option<String>,
    pub checked_at_ms: Option<u64>,
    pub error: Option<String>,
}

impl DeliveryPr {
    pub fn linked(task_id: &str, pr: &PrRef) -> Self {
        Self {
            task_id: task_id.to_string(),
            url: pr.url.clone(),
            repo: pr.repo.clone(),
            number: pr.number,
            state: "unknown".to_string(),
            draft: false,
            conflicts: false,
            review_decision: None,
            checks: PrChecks::default(),
            new_reviews: 0,
            new_comments: 0,
            new_authors: Vec::new(),
            latest_activity: None,
            seen_until: None,
            checked_at_ms: None,
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == "open" || self.state == "unknown"
    }
}

/// `gh pr view --json` fields Tinto reads.
pub const PR_FIELDS: &str =
    "state,isDraft,mergeable,reviewDecision,statusCheckRollup,reviews,comments";

fn check_state(check: &Value) -> &'static str {
    // Check runs carry status + conclusion; commit statuses carry state.
    let upper = |key: &str| check[key].as_str().unwrap_or_default().to_ascii_uppercase();
    if let Some(state) = check["state"].as_str() {
        return match state.to_ascii_uppercase().as_str() {
            "SUCCESS" => "passing",
            "FAILURE" | "ERROR" => "failing",
            _ => "pending",
        };
    }
    if upper("status") != "COMPLETED" {
        return "pending";
    }
    match upper("conclusion").as_str() {
        "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => "failing",
        _ => "passing",
    }
}

/// Updates `pr` from a `gh pr view --json` answer. `me` is the user's login:
/// their own reviews and comments are never news.
pub fn apply_snapshot(pr: &mut DeliveryPr, snapshot: &Value, me: &str) {
    pr.state = match snapshot["state"].as_str().unwrap_or_default() {
        "MERGED" => "merged",
        "CLOSED" => "closed",
        _ => "open",
    }
    .to_string();
    pr.draft = snapshot["isDraft"].as_bool().unwrap_or(false);
    pr.conflicts = snapshot["mergeable"].as_str() == Some("CONFLICTING");
    pr.review_decision = snapshot["reviewDecision"]
        .as_str()
        .filter(|decision| !decision.is_empty())
        .map(str::to_ascii_lowercase);
    let mut checks = PrChecks::default();
    for check in snapshot["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
    {
        match check_state(check) {
            "passing" => checks.passing += 1,
            "failing" => checks.failing += 1,
            _ => checks.pending += 1,
        }
    }
    pr.checks = checks;

    // (time, author, is_review) of everything others did on the PR.
    let mut activity: Vec<(String, String, bool)> = Vec::new();
    for (key, time_key, review) in [
        ("reviews", "submittedAt", true),
        ("comments", "createdAt", false),
    ] {
        for item in snapshot[key].as_array().into_iter().flatten() {
            let author = item["author"]["login"].as_str().unwrap_or_default();
            let Some(time) = item[time_key].as_str() else {
                continue;
            };
            if !author.eq_ignore_ascii_case(me) {
                activity.push((time.to_string(), author.to_string(), review));
            }
        }
    }
    pr.latest_activity = activity
        .iter()
        .map(|(time, _, _)| time.clone())
        .max()
        .or(pr.latest_activity.take());
    if pr.seen_until.is_none() {
        // First read: what was there before Tinto followed the PR is not news.
        pr.seen_until = Some(pr.latest_activity.clone().unwrap_or_default());
    }
    let seen = pr.seen_until.clone().unwrap_or_default();
    let fresh: Vec<&(String, String, bool)> = activity
        .iter()
        .filter(|(time, _, _)| *time > seen)
        .collect();
    pr.new_reviews = fresh.iter().filter(|(_, _, review)| *review).count() as u32;
    pr.new_comments = fresh.iter().filter(|(_, _, review)| !review).count() as u32;
    let mut authors: Vec<String> = fresh.iter().map(|(_, author, _)| author.clone()).collect();
    authors.sort();
    authors.dedup();
    pr.new_authors = authors;
    pr.error = None;
}

/// One page of the notifications feed, or "nothing changed".
pub enum Feed {
    NotModified {
        poll_seconds: u64,
    },
    Changed {
        etag: Option<String>,
        poll_seconds: u64,
        /// `owner/name#number` of the PRs the notifications are about.
        prs: Vec<String>,
    },
}

/// GitHub as Tinto uses it. The app uses `gh`; tests use fakes.
pub trait GitHub: Send + Sync {
    fn pr(&self, url: &str) -> Result<Value, DeliveryError>;
    fn me(&self) -> Result<String, DeliveryError>;
    fn feed(&self, etag: Option<&str>) -> Result<Feed, DeliveryError>;
    /// Merges the PR and deletes its branch on GitHub. `subject` and `body`
    /// are the merge commit's message (`rebase` has none).
    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), DeliveryError>;
}

/// Until the app sets `gh`, nothing reaches GitHub.
pub struct Offline;

impl GitHub for Offline {
    fn pr(&self, _url: &str) -> Result<Value, DeliveryError> {
        Err(DeliveryError::new(
            "github_unavailable",
            "GitHub no está conectado",
        ))
    }
    fn me(&self) -> Result<String, DeliveryError> {
        self.pr("").map(|_| String::new())
    }
    fn feed(&self, _etag: Option<&str>) -> Result<Feed, DeliveryError> {
        self.pr("").map(|_| Feed::NotModified { poll_seconds: 60 })
    }
    fn merge(
        &self,
        _url: &str,
        _method: &str,
        _subject: &str,
        _body: &str,
    ) -> Result<(), DeliveryError> {
        self.pr("").map(|_| ())
    }
}

/// GitHub through the user's `gh` login.
pub struct GhCli;

impl GhCli {
    fn run(args: &[&str]) -> Result<std::process::Output, DeliveryError> {
        let mut command = std::process::Command::new("gh");
        // Outside any checkout: gh acts on the PR URL, never on a local repo.
        command
            .args(args)
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::null());
        #[cfg(target_os = "windows")]
        crate::windows_process::hide_console(&mut command);
        command.output().map_err(|error| {
            DeliveryError::new(
                "github_unavailable",
                format!("no se pudo ejecutar gh (GitHub CLI): {error}"),
            )
        })
    }

    fn json(args: &[&str]) -> Result<Value, DeliveryError> {
        let output = Self::run(args)?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(DeliveryError::new(
                "github_failed",
                if message.is_empty() {
                    "gh falló".to_string()
                } else {
                    message
                },
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| DeliveryError::new("github_failed", error.to_string()))
    }
}

impl GitHub for GhCli {
    fn pr(&self, url: &str) -> Result<Value, DeliveryError> {
        Self::json(&["pr", "view", url, "--json", PR_FIELDS])
    }

    fn me(&self) -> Result<String, DeliveryError> {
        Ok(Self::json(&["api", "user"])?["login"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), DeliveryError> {
        let flag = format!("--{method}");
        let mut args = vec!["pr", "merge", url, flag.as_str(), "--delete-branch"];
        if method != "rebase" {
            if !subject.trim().is_empty() {
                args.extend(["--subject", subject]);
            }
            args.extend(["--body", body]);
        }
        let output = Self::run(&args)?;
        if output.status.success() {
            return Ok(());
        }
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DeliveryError::new(
            "merge_failed",
            if message.is_empty() {
                "gh no pudo fusionar el PR".to_string()
            } else {
                message
            },
        ))
    }

    fn feed(&self, etag: Option<&str>) -> Result<Feed, DeliveryError> {
        let header = etag.map(|etag| format!("If-None-Match: {etag}"));
        let mut args = vec!["api", "-i", "notifications"];
        if let Some(header) = header.as_deref() {
            args.extend(["-H", header]);
        }
        let output = Self::run(&args)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let (head, body) = text
            .split_once("\r\n\r\n")
            .or_else(|| text.split_once("\n\n"))
            .unwrap_or((&text, ""));
        let header_value = |name: &str| {
            head.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_string())
            })
        };
        let poll_seconds = header_value("X-Poll-Interval")
            .and_then(|value| value.parse().ok())
            .unwrap_or(60);
        if head
            .lines()
            .next()
            .is_some_and(|status| status.contains(" 304"))
        {
            return Ok(Feed::NotModified { poll_seconds });
        }
        if !output.status.success() {
            return Err(DeliveryError::new(
                "github_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        let items: Value = serde_json::from_str(body.trim())
            .map_err(|error| DeliveryError::new("github_failed", error.to_string()))?;
        let prs = items
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["subject"]["type"] == "PullRequest")
            .filter_map(|item| feed_pr(item["subject"]["url"].as_str()?))
            .collect();
        Ok(Feed::Changed {
            etag: header_value("ETag"),
            poll_seconds,
            prs,
        })
    }
}

/// `owner/name#number` from an API URL such as
/// `https://api.github.com/repos/owner/name/pulls/279`.
pub fn feed_pr(api_url: &str) -> Option<String> {
    let path = api_url.split("/repos/").nth(1)?;
    let mut parts = path.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    (kind == "pulls").then(|| format!("{owner}/{repo}#{number}"))
}

impl DeliveryService {
    pub fn set_github(&self, github: Arc<dyn GitHub>) {
        if let Ok(mut slot) = self.inner.github.lock() {
            *slot = github;
        }
    }

    pub(super) fn github_client(&self) -> Arc<dyn GitHub> {
        self.github()
    }

    fn github(&self) -> Arc<dyn GitHub> {
        self.inner
            .github
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_else(|_| Arc::new(Offline))
    }

    pub fn task_pr(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        self.store()?.pr(task_id)
    }

    /// Follows `pr` from the task. Reading it happens in the background.
    pub fn link_pr(&self, task_id: &str, pr: &PrRef) -> Result<DeliveryPr, DeliveryError> {
        let linked = DeliveryPr::linked(task_id, pr);
        let store = self.store()?;
        store.put_pr(&linked)?;
        store.record_event(now_ms(), "pr_linked", Some(task_id), None, &linked.url)?;
        drop(store);
        self.notify();
        let service = self.clone();
        let task_id = task_id.to_string();
        std::thread::spawn(move || {
            let _ = service.refresh_pr(&task_id, Duration::ZERO);
        });
        Ok(linked)
    }

    /// Reads the task's PR from GitHub unless it was read less than
    /// `max_age` ago.
    pub fn refresh_pr(
        &self,
        task_id: &str,
        max_age: Duration,
    ) -> Result<Option<DeliveryPr>, DeliveryError> {
        let Some(mut pr) = self.task_pr(task_id)? else {
            return Ok(None);
        };
        let fresh = pr
            .checked_at_ms
            .is_some_and(|at| now_ms().saturating_sub(at) < max_age.as_millis() as u64);
        if fresh {
            return Ok(Some(pr));
        }
        let github = self.github();
        let read = github
            .pr(&pr.url)
            .and_then(|snapshot| Ok((snapshot, self.github_login(&*github)?)));
        let before = pr.clone();
        match read {
            Ok((snapshot, me)) => apply_snapshot(&mut pr, &snapshot, &me),
            Err(error) => pr.error = Some(error.message),
        }
        pr.checked_at_ms = Some(now_ms());
        self.store()?.put_pr(&pr)?;
        if pr_changed(&before, &pr) {
            self.notify();
        }
        Ok(Some(pr))
    }

    fn github_login(&self, github: &dyn GitHub) -> Result<String, DeliveryError> {
        let store = self.store()?;
        if let Some(login) = store.setting::<String>("github_login")? {
            return Ok(login);
        }
        drop(store);
        let login = github.me()?;
        self.store()?.set_setting("github_login", &login)?;
        Ok(login)
    }

    /// What the user has seen of the PR: its activity so far stops being new.
    pub fn mark_pr_seen(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        let store = self.store()?;
        let Some(mut pr) = store.pr(task_id)? else {
            return Ok(None);
        };
        if pr.new_reviews + pr.new_comments == 0 {
            return Ok(Some(pr));
        }
        pr.seen_until = pr.latest_activity.clone();
        pr.new_reviews = 0;
        pr.new_comments = 0;
        pr.new_authors.clear();
        store.put_pr(&pr)?;
        store.record_event(now_ms(), "pr_seen", Some(task_id), None, &pr.url)?;
        drop(store);
        self.notify();
        Ok(Some(pr))
    }

    /// One pass of the watch. Returns how long to wait before the next one.
    pub fn watch_prs_once(&self, etag: &mut Option<String>) -> Duration {
        let open: Vec<DeliveryPr> = match self.store().and_then(|store| store.prs()) {
            Ok(prs) => prs.into_iter().filter(DeliveryPr::is_open).collect(),
            Err(_) => return Duration::from_secs(60),
        };
        if open.is_empty() {
            // Nothing to follow: no request at all.
            return Duration::from_secs(60);
        }
        let mut poll = 60;
        let mut mentioned: Vec<String> = Vec::new();
        match self.github().feed(etag.as_deref()) {
            Ok(Feed::NotModified { poll_seconds }) => poll = poll_seconds,
            Ok(Feed::Changed {
                etag: next,
                poll_seconds,
                prs,
            }) => {
                poll = poll_seconds;
                *etag = next;
                mentioned = prs;
            }
            Err(_) => {}
        }
        for pr in open {
            let key = format!("{}#{}", pr.repo, pr.number);
            let due = pr.state == "unknown" || pr.checks.pending > 0 || mentioned.contains(&key);
            if due {
                let _ = self.refresh_pr(&pr.task_id, Duration::ZERO);
            }
        }
        Duration::from_secs(poll.max(30))
    }

    /// Links the PRs of PR steps marked done before Tinto followed PRs.
    pub fn link_recorded_prs(&self) -> Result<usize, DeliveryError> {
        let (tasks, approvals, linked) = {
            let store = self.store()?;
            (store.tasks()?, store.approvals()?, store.prs()?)
        };
        let mut count = 0;
        for task in tasks {
            if linked.iter().any(|pr| pr.task_id == task.id) {
                continue;
            }
            let recorded = approvals
                .iter()
                .rev()
                .filter(|approval| approval.task_id == task.id)
                .filter(|approval| approval.rung == super::model::DeliveryRung::Pr)
                .filter(|approval| {
                    approval.status == super::model::DeliveryApprovalStatus::Executed
                })
                .find_map(|approval| parse_pr_url(approval.outcome.as_deref()?));
            if let Some(pr) = recorded {
                self.link_pr(&task.id, &pr)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// Follows the linked PRs for as long as the app runs.
    pub fn start_pr_watch(&self) {
        let service = self.clone();
        std::thread::spawn(move || {
            let _ = service.link_recorded_prs();
            let mut etag = None;
            loop {
                let wait = service.watch_prs_once(&mut etag);
                std::thread::sleep(wait);
            }
        });
    }
}

fn pr_changed(before: &DeliveryPr, after: &DeliveryPr) -> bool {
    let mut a = before.clone();
    let mut b = after.clone();
    a.checked_at_ms = None;
    b.checked_at_ms = None;
    a != b
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_the_pr_in_a_text() {
        let pr =
            parse_pr_url("PR abierto: https://github.com/AIRInstitute/agentos/pull/279.").unwrap();
        assert_eq!(pr.repo, "AIRInstitute/agentos");
        assert_eq!(pr.number, 279);
        assert_eq!(pr.url, "https://github.com/AIRInstitute/agentos/pull/279");
        assert_eq!(
            parse_pr_url("https://github.com/o/r/pull/7#discussion_r1").map(|pr| pr.number),
            Some(7)
        );
        assert!(parse_pr_url("https://github.com/o/r/issues/7").is_none());
        assert!(parse_pr_url("https://gitlab.com/o/r/-/merge_requests/7").is_none());
        assert_eq!(
            feed_pr("https://api.github.com/repos/o/r/pulls/12").as_deref(),
            Some("o/r#12")
        );
        assert!(feed_pr("https://api.github.com/repos/o/r/issues/12").is_none());
    }

    fn snapshot(reviews: Value, comments: Value) -> Value {
        json!({
            "state": "OPEN",
            "isDraft": false,
            "mergeable": "CONFLICTING",
            "reviewDecision": "CHANGES_REQUESTED",
            "statusCheckRollup": [
                {"name": "build", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"name": "lint", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"name": "e2e", "status": "IN_PROGRESS", "conclusion": ""},
                {"context": "deploy", "state": "PENDING"},
                {"name": "docs", "status": "COMPLETED", "conclusion": "SKIPPED"}
            ],
            "reviews": reviews,
            "comments": comments,
        })
    }

    #[test]
    fn a_snapshot_becomes_state_and_news() {
        let reference = parse_pr_url("https://github.com/o/r/pull/1").unwrap();
        let mut pr = DeliveryPr::linked("t1", &reference);
        let before = json!([{"author": {"login": "ana"}, "state": "COMMENTED", "submittedAt": "2026-10-14T08:00:00Z"}]);
        apply_snapshot(&mut pr, &snapshot(before.clone(), json!([])), "me");
        assert_eq!(pr.state, "open");
        assert!(pr.conflicts);
        assert_eq!(pr.review_decision.as_deref(), Some("changes_requested"));
        assert_eq!(
            pr.checks,
            PrChecks {
                passing: 2,
                failing: 1,
                pending: 2
            }
        );
        // What was there before Tinto followed the PR is not news.
        assert_eq!(pr.new_reviews, 0);
        assert_eq!(pr.seen_until.as_deref(), Some("2026-10-14T08:00:00Z"));

        let reviews = json!([
            {"author": {"login": "ana"}, "state": "COMMENTED", "submittedAt": "2026-10-14T08:00:00Z"},
            {"author": {"login": "luis"}, "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-14T09:00:00Z"},
            {"author": {"login": "ME"}, "state": "COMMENTED", "submittedAt": "2026-10-14T09:30:00Z"}
        ]);
        let comments = json!([{"author": {"login": "ana"}, "createdAt": "2026-10-14T09:10:00Z"}]);
        apply_snapshot(&mut pr, &snapshot(reviews.clone(), comments.clone()), "me");
        assert_eq!((pr.new_reviews, pr.new_comments), (1, 1));
        assert_eq!(pr.new_authors, vec!["ana".to_string(), "luis".to_string()]);

        pr.seen_until = pr.latest_activity.clone();
        apply_snapshot(&mut pr, &snapshot(reviews, comments), "me");
        assert_eq!((pr.new_reviews, pr.new_comments), (0, 0));

        apply_snapshot(&mut pr, &json!({"state": "MERGED"}), "me");
        assert_eq!(pr.state, "merged");
        assert!(!pr.is_open());
    }
}
