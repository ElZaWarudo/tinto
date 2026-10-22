//! Following a task's pull request on GitHub after its PR step.
//!
//! - Marking the PR step done links the PR to the task: the one GitHub has
//!   for the task's branch, else the URL the step recorded. A task pushed
//!   without a linked PR looks for its branch's PR at startup and when it
//!   is opened.
//! - GitHub cannot push to a desktop app without a webhook (which needs
//!   admin rights on the repo and a public relay), so Tinto reads the user's
//!   notifications feed: one conditional request per interval GitHub sets,
//!   free when nothing changed (304), and only while some PR is open. A PR is
//!   read in full only when the feed mentions it, while its checks are still
//!   running, or when someone opens it in Tinto.
//! - Activity newer than what the user last saw counts as new; reading it
//!   (or starting a stage on the task) moves that mark.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::coordination::found_step;
use super::model::{DeliveryApproval, DeliveryApprovalStatus, DeliveryRung, DeliveryTask};
use super::service::DeliveryService;
use super::tasks::{self, Place};
use super::{now_ms, DeliveryError};

/// How often every done push is checked against its remote branch.
const PUSH_CHECK_EVERY: Duration = Duration::from_secs(5 * 60);

/// How a PR step's notice starts when its reviewers could not be asked.
const REVIEW_NOT_ASKED: &str = "No se pudo pedir la revisión a";

/// `owner/name` of the task's `origin`, when it is on GitHub.
fn task_github_repo(task: &DeliveryTask) -> Option<String> {
    let place = Place::of(task.distro.as_deref());
    tasks::origin_url(place, &task.repo)
        .as_deref()
        .and_then(github_repo)
}

/// The notice of a PR step whose PR says something else than was approved.
pub const PUBLISHED_MISMATCH: &str = "La PR publicada no coincide con el texto aprobado";

/// How a published title and body differ from the approved ones, ignoring
/// line endings and surrounding blank space; `None` when they match.
pub fn text_difference(
    published_title: &str,
    published_body: &str,
    title: &str,
    body: &str,
) -> Option<String> {
    let clean = |text: &str| text.replace("\r\n", "\n").trim().to_string();
    let mut differences = Vec::new();
    if clean(published_title) != clean(title) {
        differences.push(format!(
            "el título publicado es «{}» y el aprobado «{}»",
            clean(published_title),
            clean(title)
        ));
    }
    let (published, approved) = (clean(published_body), clean(body));
    if published != approved {
        let (published_lines, approved_lines): (Vec<_>, Vec<_>) =
            (published.lines().collect(), approved.lines().collect());
        let at = published_lines
            .iter()
            .zip(&approved_lines)
            .position(|(left, right)| left != right)
            .unwrap_or(published_lines.len().min(approved_lines.len()));
        let line = |lines: &[&str]| -> String {
            lines
                .get(at)
                .map(|line| line.chars().take(120).collect())
                .unwrap_or_else(|| "(nada)".to_string())
        };
        differences.push(format!(
            "el cuerpo cambia en la línea {}: publicado «{}», aprobado «{}»",
            at + 1,
            line(&published_lines),
            line(&approved_lines)
        ));
    }
    (!differences.is_empty()).then(|| differences.join("; "))
}

/// Marking the PR step done needs a PR Tinto can follow.
pub fn no_branch_pr(branch: &str) -> DeliveryError {
    DeliveryError::new(
        "pr_not_found",
        format!("No encuentro un PR de la rama {branch}. Pega su URL"),
    )
}

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

/// `owner/name` of a GitHub remote: `https://github.com/o/r(.git)`,
/// `git@github.com:o/r(.git)` or `ssh://git@github.com/o/r(.git)`.
pub fn github_repo(remote: &str) -> Option<String> {
    let (_, path) = remote.trim().split_once("github.com")?;
    let path = path.trim_start_matches([':', '/']).trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !name.is_empty() && parts.next().is_none())
        .then(|| format!("{owner}/{name}"))
}

/// The branch's PR in a `gh pr list --json number,url,state` answer: the
/// open one, else the most recent.
pub fn pick_branch_pr(list: &Value) -> Option<PrRef> {
    let prs = list.as_array()?;
    let newest = |state: Option<&str>| {
        prs.iter()
            .filter(|pr| state.is_none_or(|state| pr["state"].as_str() == Some(state)))
            .max_by_key(|pr| pr["number"].as_u64())
    };
    let chosen = newest(Some("OPEN")).or_else(|| newest(None))?;
    parse_pr_url(chosen["url"].as_str()?)
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
    /// Once merged: who merged it, when (GitHub timestamp) and the merge
    /// commit, when GitHub gives them.
    #[serde(default)]
    pub merged_by: Option<String>,
    #[serde(default)]
    pub merged_at: Option<String>,
    #[serde(default)]
    pub merge_commit: Option<String>,
    /// Who approved it: reviewers whose latest review is an approval.
    #[serde(default)]
    pub approved_by: Vec<String>,
    /// The branch it merges into, as GitHub last said.
    #[serde(default)]
    pub base: Option<String>,
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
            merged_by: None,
            merged_at: None,
            merge_commit: None,
            approved_by: Vec::new(),
            base: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == "open" || self.state == "unknown"
    }

    /// Reviewed and approved: GitHub's decision when the repo requires
    /// reviews, else at least one approval and no changes requested.
    pub fn is_approved(&self) -> bool {
        match self.review_decision.as_deref() {
            Some(decision) => decision == "approved",
            None => !self.approved_by.is_empty(),
        }
    }
}

/// `gh pr view --json` fields Tinto reads.
pub const PR_FIELDS: &str =
    "title,body,state,isDraft,mergeable,reviewDecision,statusCheckRollup,reviews,comments,mergedBy,mergedAt,mergeCommit,baseRefName";

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
    let text = |value: &Value| {
        value
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    pr.merged_by = text(&snapshot["mergedBy"]["login"]);
    pr.merged_at = text(&snapshot["mergedAt"]);
    pr.merge_commit = text(&snapshot["mergeCommit"]["oid"]);
    if let Some(base) = text(&snapshot["baseRefName"]) {
        pr.base = Some(base);
    }
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

    // Each reviewer's latest verdict; comments alone are not one.
    let mut verdicts: Vec<(&str, &str, &str)> = snapshot["reviews"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|review| {
            let state = review["state"].as_str()?;
            matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED").then_some((
                review["author"]["login"].as_str()?,
                review["submittedAt"].as_str().unwrap_or_default(),
                state,
            ))
        })
        .collect();
    verdicts.sort_by_key(|(author, time, _)| (*author, *time));
    let mut approved_by: Vec<String> = Vec::new();
    for (index, (author, _, state)) in verdicts.iter().enumerate() {
        let latest = verdicts
            .get(index + 1)
            .is_none_or(|(next, _, _)| next != author);
        if latest && *state == "APPROVED" {
            approved_by.push(author.to_string());
        }
    }
    pr.approved_by = approved_by;

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
    /// `gh pr list --json` of every PR, in any state, whose head is
    /// `branch` in `repo` (`owner/name`).
    fn branch_prs(&self, repo: &str, branch: &str) -> Result<Value, DeliveryError>;
    /// Logins that can be assigned in `repo` (`owner/name`): the people who
    /// can review its PRs.
    fn assignees(&self, repo: &str) -> Result<Vec<String>, DeliveryError>;
    /// Asks `logins` to review the PR.
    fn request_reviewers(&self, url: &str, logins: &[String]) -> Result<(), DeliveryError>;
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
    /// Opens a PR from `head` into `base` (`None`: the repo's default
    /// branch) in `repo` (`owner/name`) and returns its URL.
    fn create_pr(
        &self,
        _repo: &str,
        _base: Option<&str>,
        _head: &str,
        _title: &str,
        _body: &str,
    ) -> Result<String, DeliveryError> {
        Err(offline())
    }
    /// Sets the PR's title and body.
    fn edit_pr(&self, _url: &str, _title: &str, _body: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Points the PR at another base branch.
    fn set_pr_base(&self, _url: &str, _base: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Closes the PR without merging it.
    fn close_pr(&self, _url: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Turns the PR back into a draft.
    fn pr_to_draft(&self, _url: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Every review comment of PR `number` in `repo` (`owner/name`), as
    /// GitHub's REST API lists them.
    fn pr_review_comments(&self, _repo: &str, _number: u64) -> Result<Value, DeliveryError> {
        Err(offline())
    }
}

fn offline() -> DeliveryError {
    DeliveryError::new("github_unavailable", "GitHub no está conectado")
}

/// Until the app sets `gh`, nothing reaches GitHub.
pub struct Offline;

impl GitHub for Offline {
    fn pr(&self, _url: &str) -> Result<Value, DeliveryError> {
        Err(offline())
    }
    fn me(&self) -> Result<String, DeliveryError> {
        self.pr("").map(|_| String::new())
    }
    fn branch_prs(&self, _repo: &str, _branch: &str) -> Result<Value, DeliveryError> {
        self.pr("")
    }
    fn assignees(&self, _repo: &str) -> Result<Vec<String>, DeliveryError> {
        self.pr("").map(|_| Vec::new())
    }
    fn request_reviewers(&self, _url: &str, _logins: &[String]) -> Result<(), DeliveryError> {
        self.pr("").map(|_| ())
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

    /// Runs `gh` for its effect; its stdout on success.
    fn act(args: &[&str], failure: &str) -> Result<String, DeliveryError> {
        let output = Self::run(args)?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(DeliveryError::new(
            "github_failed",
            if message.is_empty() {
                failure.to_string()
            } else {
                message
            },
        ))
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

    fn branch_prs(&self, repo: &str, branch: &str) -> Result<Value, DeliveryError> {
        Self::json(&[
            "pr",
            "list",
            "--repo",
            repo,
            "--head",
            branch,
            "--state",
            "all",
            "--json",
            "number,url,state,headRefOid",
        ])
    }

    fn assignees(&self, repo: &str) -> Result<Vec<String>, DeliveryError> {
        let path = format!("repos/{repo}/assignees");
        let output = Self::run(&["api", "--paginate", &path, "--jq", ".[].login"])?;
        if !output.status.success() {
            return Err(DeliveryError::new(
                "github_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|login| !login.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn request_reviewers(&self, url: &str, logins: &[String]) -> Result<(), DeliveryError> {
        let logins = logins.join(",");
        let output = Self::run(&["pr", "edit", url, "--add-reviewer", &logins])?;
        if output.status.success() {
            return Ok(());
        }
        Err(DeliveryError::new(
            "github_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
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

    fn create_pr(
        &self,
        repo: &str,
        base: Option<&str>,
        head: &str,
        title: &str,
        body: &str,
    ) -> Result<String, DeliveryError> {
        let mut args = vec![
            "pr", "create", "--repo", repo, "--head", head, "--title", title, "--body", body,
        ];
        if let Some(base) = base {
            args.extend(["--base", base]);
        }
        // gh prints the new PR's URL last.
        let output = Self::act(&args, "gh no pudo abrir el PR")?;
        Ok(output.lines().last().unwrap_or_default().trim().to_string())
    }

    fn edit_pr(&self, url: &str, title: &str, body: &str) -> Result<(), DeliveryError> {
        Self::act(
            &["pr", "edit", url, "--title", title, "--body", body],
            "gh no pudo editar el PR",
        )
        .map(|_| ())
    }

    fn close_pr(&self, url: &str) -> Result<(), DeliveryError> {
        Self::act(&["pr", "close", url], "gh no pudo cerrar el PR").map(|_| ())
    }

    fn set_pr_base(&self, url: &str, base: &str) -> Result<(), DeliveryError> {
        Self::act(
            &["pr", "edit", url, "--base", base],
            "gh no pudo cambiar la base del PR",
        )
        .map(|_| ())
    }

    fn pr_review_comments(&self, repo: &str, number: u64) -> Result<Value, DeliveryError> {
        let path = format!("repos/{repo}/pulls/{number}/comments");
        // One comment per line, across every page.
        let output = Self::act(
            &["api", "--paginate", &path, "--jq", ".[]"],
            "gh no pudo leer los comentarios del PR",
        )?;
        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .map_err(|error| DeliveryError::new("github_failed", error.to_string()))
            })
            .collect::<Result<Vec<Value>, _>>()
            .map(Value::Array)
    }

    fn pr_to_draft(&self, url: &str) -> Result<(), DeliveryError> {
        Self::act(
            &["pr", "ready", url, "--undo"],
            "gh no pudo pasar el PR a borrador",
        )
        .map(|_| ())
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

    pub(super) fn github(&self) -> Arc<dyn GitHub> {
        self.inner
            .github
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_else(|_| Arc::new(Offline))
    }

    /// Once the package a package builds on has merged, the package's PR
    /// merges where that one did: Tinto points it there (GitHub may have
    /// done it already, deleting the merged branch). With merge commits the
    /// PR then shows only its own commits; after a squash or rebase merge it
    /// would need a rebase, which Tinto leaves to the user.
    pub fn retarget_packages(&self) -> Result<usize, DeliveryError> {
        let tasks = self.store()?.tasks()?;
        let mut moved = 0;
        for task in &tasks {
            let Some(other_id) = task.package.as_ref().and_then(|p| p.builds_on.as_deref()) else {
                continue;
            };
            let Some(other) = self.store()?.task(other_id)? else {
                continue;
            };
            if task.base_ref == other.base_ref || !self.task_merged(&other)? {
                continue;
            }
            let squashed = self
                .latest_approval(&other.id, DeliveryRung::Merge)?
                .and_then(|merge| merge.merge_method)
                .is_some_and(|method| method != "merge");
            if squashed {
                continue;
            }
            let target = other.base_ref.clone();
            let branch = target.trim_start_matches("origin/").to_string();
            let pr = self.task_pr(&task.id)?.filter(DeliveryPr::is_open);
            if let Some(pr) = &pr {
                if pr.base.as_deref() != Some(branch.as_str()) {
                    if let Err(error) = self.github().set_pr_base(&pr.url, &branch) {
                        eprintln!("tinto: delivery could not retarget {}: {error}", pr.url);
                        continue;
                    }
                }
            }
            let store = self.store()?;
            let mut moved_task = task.clone();
            moved_task.base_ref = target;
            moved_task.updated_at_ms = now_ms();
            store.put_task(&moved_task)?;
            if let Some(mut pr) = pr {
                pr.base = Some(branch.clone());
                store.put_pr(&pr)?;
            }
            store.record_event(
                now_ms(),
                "package_retargeted",
                Some(&task.id),
                None,
                &format!(
                    "{} se fusionó: este paquete va ahora a {branch}",
                    other.label()
                ),
            )?;
            moved += 1;
        }
        if moved > 0 {
            self.notify();
        }
        Ok(moved)
    }

    /// Why a package's PR cannot merge yet: it still goes into the branch of
    /// the package it builds on.
    pub(super) fn package_merge_refusal(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<String>, DeliveryError> {
        let Some(other_id) = task.package.as_ref().and_then(|p| p.builds_on.as_deref()) else {
            return Ok(None);
        };
        let Some(other) = self.store()?.task(other_id)? else {
            return Ok(None);
        };
        if task.base_ref != other.branch {
            return Ok(None);
        }
        let squashed = self
            .latest_approval(&other.id, DeliveryRung::Merge)?
            .and_then(|merge| merge.merge_method)
            .filter(|method| method != "merge");
        Ok(Some(match squashed {
            Some(method) => format!(
                "sigue sobre {}, que se fusionó con {method}: rebasa este paquete sobre {} antes de fusionarlo (Tinto no lo hace)",
                other.label(),
                other.base_ref
            ),
            None => format!(
                "va a la rama de {}: se fusiona después de ese paquete, cuando Tinto lo apunta a {}",
                other.label(),
                other.base_ref.trim_start_matches("origin/")
            ),
        }))
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
        // One read at a time: two at once (the background read a link starts
        // and the watch, say) would both record a merge found on GitHub.
        let _reading = self
            .inner
            .pr_reads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(pr) = self.task_pr(task_id)? else {
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
        // GitHub's answer goes onto the PR as it is now: while it was asked,
        // the user may have seen its news (a stage started on the task).
        let (before, pr) = {
            let store = self.store()?;
            let Some(mut pr) = store.pr(task_id)? else {
                return Ok(None);
            };
            let before = pr.clone();
            match read {
                Ok((snapshot, me)) => apply_snapshot(&mut pr, &snapshot, &me),
                Err(error) => pr.error = Some(error.message),
            }
            pr.checked_at_ms = Some(now_ms());
            store.put_pr(&pr)?;
            if let Some(news) = pr_news(&before, &pr) {
                store.record_event(now_ms(), "pr_changed", Some(task_id), None, &news)?;
            }
            (before, pr)
        };
        let changed = pr_changed(&before, &pr);
        if changed {
            self.notify();
        }
        self.check_pr_steps(&pr)?;
        // New commits on the PR, or its branch gone: the push may have
        // drifted or match again.
        if changed {
            self.check_push(&self.task(task_id)?)?;
        }
        Ok(Some(pr))
    }

    /// Checks the PR and merge steps against the PR as GitHub last showed
    /// it: a PR closed without merging (or gone) no longer bears out the PR
    /// step, and a PR merged outside Tinto makes the merge step done.
    pub(super) fn check_pr_steps(&self, pr: &DeliveryPr) -> Result<(), DeliveryError> {
        let step = self
            .latest_approval(&pr.task_id, DeliveryRung::Pr)?
            .filter(|step| step.status.was_done());
        if let Some(step) = step {
            let gone = pr
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Could not resolve"));
            let drift = if pr.state == "closed" {
                Some(format!("El PR #{} se cerró sin fusionar", pr.number))
            } else if gone {
                Some(format!("El PR #{} ya no existe en GitHub", pr.number))
            } else {
                None
            };
            self.set_drift(step, drift)?;
        }
        if pr.state != "merged" {
            return Ok(());
        }
        let merge = self.latest_approval(&pr.task_id, DeliveryRung::Merge)?;
        let step = match merge {
            Some(merge) if merge.status == DeliveryApprovalStatus::Executed => return Ok(()),
            // A merge the user was about to approve happened on GitHub.
            Some(mut merge)
                if matches!(
                    merge.status,
                    DeliveryApprovalStatus::Pending | DeliveryApprovalStatus::Approved
                ) =>
            {
                merge.status = DeliveryApprovalStatus::Executed;
                merge.executed_at_ms = Some(now_ms());
                merge.outcome = Some(merged_outside(pr));
                merge
            }
            _ => found_step(
                &pr.task_id,
                DeliveryRung::Merge,
                format!("PR #{}", pr.number),
                merged_outside(pr),
            ),
        };
        self.save_approval(&step, "merge_found")
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
        // PRs outside Tinto that block a task are read every pass.
        let _ = self.refresh_blocking_prs();
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

    /// Links a PR to every task that has none and can have one.
    pub fn link_recorded_prs(&self) -> Result<usize, DeliveryError> {
        let tasks = self.store()?.tasks()?;
        let mut count = 0;
        for task in tasks {
            if self.task_pr(&task.id)?.is_none() && self.ensure_pr_linked(&task)?.is_some() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// The task's PR, linking it first when none is: once the branch is
    /// pushed, the PR GitHub has for it; else the URL its PR step recorded.
    pub(super) fn ensure_pr_linked(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<DeliveryPr>, DeliveryError> {
        if let Some(pr) = self.task_pr(&task.id)? {
            return Ok(Some(pr));
        }
        // A PR step done also means the branch was pushed, even by hand.
        let pushed = [DeliveryRung::Push, DeliveryRung::Pr]
            .into_iter()
            .map(|rung| self.latest_approval(&task.id, rung))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .any(|step| step.status.was_done());
        // GitHub out of reach finds nothing; the recorded URL still counts.
        if let Some(found) = pushed
            .then(|| self.find_branch_pr(task).ok().flatten())
            .flatten()
        {
            self.adopt_pr(task, &found)?;
            return self.task_pr(&task.id);
        }
        let recorded = self
            .latest_approval(&task.id, DeliveryRung::Pr)?
            .filter(|step| step.status == DeliveryApprovalStatus::Executed)
            .and_then(|step| parse_pr_url(step.outcome.as_deref()?));
        match recorded {
            Some(pr) => self.link_pr(&task.id, &pr).map(Some),
            None => Ok(None),
        }
    }

    /// The PR GitHub has for the task's branch: the open one, else the most
    /// recent. `None` when there is none or `origin` is not on GitHub.
    pub(super) fn find_branch_pr(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<PrRef>, DeliveryError> {
        let Some(repo) = task_github_repo(task) else {
            return Ok(None);
        };
        Ok(pick_branch_pr(
            &self.github().branch_prs(&repo, &task.branch)?,
        ))
    }

    /// Opens the task's PR with the approved title and body, or, when the
    /// branch already has an open PR, gives it that text: what GitHub shows
    /// is what the user approved.
    pub(super) fn open_approved_pr(
        &self,
        approval: &DeliveryApproval,
    ) -> Result<PrRef, DeliveryError> {
        let task = self.task(&approval.task_id)?;
        let repo = task_github_repo(&task).ok_or_else(|| {
            DeliveryError::new("github_unavailable", "el remoto origin no está en GitHub")
        })?;
        let github = self.github_client();
        let open = pick_branch_pr(&github.branch_prs(&repo, &task.branch)?).filter(|found| {
            github
                .pr(&found.url)
                .is_ok_and(|pr| pr["state"].as_str() == Some("OPEN"))
        });
        if let Some(found) = open {
            github.edit_pr(&found.url, &approval.title, &approval.body)?;
            return Ok(found);
        }
        let base = task.base_ref.trim_start_matches("origin/");
        let base = (base != "HEAD" && !base.is_empty()).then_some(base);
        let url = github.create_pr(&repo, base, &task.branch, &approval.title, &approval.body)?;
        parse_pr_url(&url).ok_or_else(|| {
            DeliveryError::new(
                "github_failed",
                format!("gh no devolvió la URL del PR abierto: {url}"),
            )
        })
    }

    /// How the PR on GitHub differs from the approved title and body, or
    /// `None` when it says exactly what was approved.
    pub(super) fn published_difference(
        &self,
        url: &str,
        approval: &DeliveryApproval,
    ) -> Result<Option<String>, DeliveryError> {
        let pr = self.github_client().pr(url)?;
        Ok(text_difference(
            pr["title"].as_str().unwrap_or_default(),
            pr["body"].as_str().unwrap_or_default(),
            &approval.title,
            &approval.body,
        ))
    }

    /// Closes the task's PR on GitHub without merging it.
    pub fn close_task_pr(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        let pr = self.linked_pr(task_id)?;
        self.github_client().close_pr(&pr.url)?;
        self.refresh_pr(task_id, std::time::Duration::ZERO)
    }

    /// Turns the task's PR back into a draft on GitHub.
    pub fn task_pr_to_draft(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        let pr = self.linked_pr(task_id)?;
        self.github_client().pr_to_draft(&pr.url)?;
        self.refresh_pr(task_id, std::time::Duration::ZERO)
    }

    fn linked_pr(&self, task_id: &str) -> Result<DeliveryPr, DeliveryError> {
        self.task_pr(task_id)?
            .ok_or_else(|| DeliveryError::new("pr_missing", "la tarea no tiene un PR de GitHub"))
    }

    /// Who can review the task's PR: the repo's assignable people, without
    /// the user, who cannot review their own PR.
    pub fn pr_reviewers(&self, task_id: &str) -> Result<Vec<String>, DeliveryError> {
        let task = self.task(task_id)?;
        let repo = task_github_repo(&task).ok_or_else(|| {
            DeliveryError::new("github_unavailable", "el remoto origin no está en GitHub")
        })?;
        let github = self.github();
        let me = self.github_login(&*github).unwrap_or_default();
        let mut logins = github.assignees(&repo)?;
        logins.retain(|login| !login.eq_ignore_ascii_case(&me));
        logins.sort_by_key(|login| login.to_ascii_lowercase());
        Ok(logins)
    }

    /// The reviewers the user chose for a PR step before approving it.
    pub fn set_pr_reviewers(
        &self,
        approval_id: &str,
        reviewers: Vec<String>,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.rung != DeliveryRung::Pr || approval.status != DeliveryApprovalStatus::Pending {
            return Err(DeliveryError::new(
                "invalid_approval",
                "los revisores se eligen al aprobar el paso PR",
            ));
        }
        approval.reviewers = reviewers
            .iter()
            .map(|login| login.trim().trim_start_matches('@').to_string())
            .filter(|login| !login.is_empty())
            .collect();
        self.save_approval(&approval, "pr_reviewers")?;
        Ok(approval)
    }

    /// Asks the PR step's reviewers on GitHub once its PR is linked, once.
    /// A failure stays in the step's notice and is retried when the task is
    /// checked again.
    pub(super) fn request_pr_reviewers(&self, task_id: &str) -> Result<(), DeliveryError> {
        let Some(mut step) = self
            .latest_approval(task_id, DeliveryRung::Pr)?
            .filter(|step| step.status.was_done())
            .filter(|step| !step.reviewers.is_empty() && !step.reviewers_requested)
        else {
            return Ok(());
        };
        let Some(pr) = self.task_pr(task_id)? else {
            return Ok(());
        };
        match self.github().request_reviewers(&pr.url, &step.reviewers) {
            Ok(()) => {
                step.reviewers_requested = true;
                if step
                    .notice
                    .as_deref()
                    .is_some_and(|n| n.starts_with(REVIEW_NOT_ASKED))
                {
                    step.notice = None;
                }
            }
            Err(error) => {
                step.notice = Some(format!(
                    "{REVIEW_NOT_ASKED} {}: {}",
                    step.reviewers.join(", "),
                    error.message
                ));
            }
        }
        self.save_approval(&step, "pr_reviewers_requested")
    }

    /// Links `found` and brings the PR step in line: a step done with
    /// another URL keeps its text and gets a notice, an approved one is
    /// completed, and a PR opened outside Tinto becomes a step found on
    /// GitHub. A request still waiting for the user is left to them.
    pub(super) fn adopt_pr(&self, task: &DeliveryTask, found: &PrRef) -> Result<(), DeliveryError> {
        if self.task_pr(&task.id)?.map(|pr| pr.url).as_ref() != Some(&found.url) {
            self.link_pr(&task.id, found)?;
        }
        let latest = self.latest_approval(&task.id, DeliveryRung::Pr)?;
        let mut step = match latest.clone() {
            Some(step) if step.status == DeliveryApprovalStatus::Pending => return Ok(()),
            Some(mut step) if step.status.was_done() => {
                // Another PR of the branch puts right a drift on the old one.
                step.status = DeliveryApprovalStatus::Executed;
                step
            }
            Some(mut step) if step.status == DeliveryApprovalStatus::Approved => {
                step.status = DeliveryApprovalStatus::Executed;
                step.executed_at_ms = Some(now_ms());
                step.outcome = Some(found.url.clone());
                // Found on GitHub rather than opened by Tinto: say so when it
                // is not what the user approved.
                if let Ok(Some(difference)) = self.published_difference(&found.url, &step) {
                    let notice = format!("{PUBLISHED_MISMATCH}: {difference}");
                    step.notice = Some(notice);
                    self.save_approval(&step, "pr_found")?;
                    return self.request_pr_reviewers(&task.id);
                }
                step
            }
            _ => found_step(
                &task.id,
                DeliveryRung::Pr,
                format!("PR #{}", found.number),
                format!(
                    "PR #{} encontrado en GitHub por su rama: {}",
                    found.number, found.url
                ),
            ),
        };
        let recorded = step.outcome.as_deref().and_then(parse_pr_url);
        step.notice = recorded
            .filter(|recorded| recorded.url != found.url)
            .map(|_| "La URL anotada no es el PR de esta rama".to_string());
        if latest.as_ref() != Some(&step) {
            self.save_approval(&step, "pr_found")?;
        }
        self.request_pr_reviewers(&task.id)
    }

    /// "Buscar el PR de la rama": links the PR GitHub has for it.
    pub fn find_task_pr(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        let task = self.task(task_id)?;
        let found = self
            .find_branch_pr(&task)?
            .ok_or_else(|| no_branch_pr(&task.branch))?;
        self.adopt_pr(&task, &found)?;
        self.refresh_pr(task_id, Duration::ZERO)
    }

    /// What opening a task checks: its PR is linked and recently read, and
    /// its push, PR and merge steps still match git and GitHub.
    pub fn check_task(&self, task_id: &str) -> Result<Option<DeliveryPr>, DeliveryError> {
        let task = self.task(task_id)?;
        if let Err(error) = self.ensure_pr_linked(&task) {
            eprintln!(
                "tinto: delivery could not look for {}'s PR: {error}",
                task.key
            );
        }
        self.request_pr_reviewers(task_id)?;
        let pr = self.refresh_pr(task_id, Duration::from_secs(30))?;
        if let Some(pr) = &pr {
            // A PR read moments ago is not read again, but still checked.
            self.check_pr_steps(pr)?;
        }
        self.check_push(&task)?;
        Ok(pr)
    }

    /// Startup: every task gets its PR if it can, and its steps are checked
    /// against git and GitHub as last seen. Open PRs are read by the watch.
    pub(super) fn check_tasks(&self) -> Result<(), DeliveryError> {
        self.link_recorded_prs()?;
        // Bound first: a guard in the `for` expression would hold the store
        // for the whole loop, and the checks inside lock it again.
        let tasks = self.store()?.tasks()?;
        for task in tasks {
            if let Some(pr) = self.task_pr(&task.id)? {
                self.check_pr_steps(&pr)?;
            }
            self.check_push(&task)?;
        }
        Ok(())
    }

    /// Follows the linked PRs for as long as the app runs, and checks the
    /// tasks' pushes against their remote branches: a drifted one on every
    /// pass, so it clears once put right, and every one each few minutes.
    pub fn start_pr_watch(&self) {
        let service = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = service.check_tasks() {
                eprintln!("tinto: delivery could not check the tasks' steps: {error}");
            }
            let mut etag = None;
            let mut pushes_checked = Instant::now();
            loop {
                let wait = service.watch_prs_once(&mut etag);
                let all = pushes_checked.elapsed() >= PUSH_CHECK_EVERY;
                if all {
                    pushes_checked = Instant::now();
                }
                if let Err(error) = service.check_pushes(all) {
                    eprintln!("tinto: delivery could not check the tasks' pushes: {error}");
                }
                std::thread::sleep(wait);
            }
        });
    }

    /// Runs [`Self::check_push`] on the tasks whose push drifted, or on all
    /// with a push done.
    pub(super) fn check_pushes(&self, all: bool) -> Result<(), DeliveryError> {
        let tasks = self.store()?.tasks()?;
        for task in tasks {
            let Some(push) = self.latest_approval(&task.id, DeliveryRung::Push)? else {
                continue;
            };
            if push.status == DeliveryApprovalStatus::Drifted || (all && push.status.was_done()) {
                self.check_push(&task)?;
            }
        }
        Ok(())
    }
}

/// "Fusionado en GitHub por ana el 2026-10-14 (commit 1a2b3c4)", with what
/// GitHub gave.
fn merged_outside(pr: &DeliveryPr) -> String {
    let mut text = "Fusionado en GitHub".to_string();
    if let Some(by) = &pr.merged_by {
        text.push_str(&format!(" por {by}"));
    }
    if let Some(at) = &pr.merged_at {
        text.push_str(&format!(" el {}", at.get(..10).unwrap_or(at)));
    }
    if let Some(commit) = &pr.merge_commit {
        text.push_str(&format!(" (commit {})", commit.get(..7).unwrap_or(commit)));
    }
    text
}

/// What a coordinator acts on in a PR that changed: its state, the review
/// decision, conflicts, checks starting to fail, others' new reviews and
/// comments. Checks that merely progress are left out.
fn pr_news(before: &DeliveryPr, after: &DeliveryPr) -> Option<String> {
    let mut news = Vec::new();
    if after.state != before.state {
        news.push(format!("ahora está {}", after.state));
    }
    if after.review_decision != before.review_decision {
        if let Some(decision) = after.review_decision.as_deref() {
            news.push(format!("revisión: {decision}"));
        }
    }
    if after.conflicts && !before.conflicts {
        news.push("tiene conflictos".to_string());
    }
    if after.checks.failing > 0 && before.checks.failing == 0 {
        news.push(format!("{} checks fallan", after.checks.failing));
    }
    let reviews = after.new_reviews.saturating_sub(before.new_reviews);
    let comments = after.new_comments.saturating_sub(before.new_comments);
    if reviews > 0 {
        news.push(format!("{reviews} revisiones nuevas"));
    }
    if comments > 0 {
        news.push(format!("{comments} comentarios nuevos"));
    }
    (!news.is_empty()).then(|| format!("PR #{}: {}", after.number, news.join(", ")))
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
    fn finds_the_github_repo_and_the_branchs_pr() {
        for remote in [
            "https://github.com/o/r.git",
            "https://github.com/o/r",
            "git@github.com:o/r.git",
            "ssh://git@github.com/o/r.git",
        ] {
            assert_eq!(github_repo(remote).as_deref(), Some("o/r"), "{remote}");
        }
        assert!(github_repo("https://gitlab.com/o/r.git").is_none());
        let pr = |number: u64, state: &str| json!({"number": number, "url": format!("https://github.com/o/r/pull/{number}"), "state": state});
        let pick = |list: Value| pick_branch_pr(&list).map(|pr| pr.number);
        assert_eq!(pick(json!([pr(4, "MERGED")])), Some(4));
        assert_eq!(pick(json!([pr(4, "CLOSED"), pr(2, "OPEN")])), Some(2));
        assert_eq!(pick(json!([pr(4, "CLOSED"), pr(6, "MERGED")])), Some(6));
        assert_eq!(pick(json!([])), None);
    }

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
