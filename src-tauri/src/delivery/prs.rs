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

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::coordination::found_step;
use super::model::{
    DeliveryApproval, DeliveryApprovalStatus, DeliveryCodeThread, DeliveryFixCommit, DeliveryRung,
    DeliveryTask,
};
use super::service::DeliveryService;
use super::tasks::{self, Place};
use super::{now_ms, DeliveryError};

/// How often every done push is checked against its remote branch.
const PUSH_CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
/// Every open PR is read at least this often: GitHub's notifications miss
/// what the user already read there, and never tell of CI.
const PR_READ_EVERY: Duration = Duration::from_secs(2 * 60);

/// How a PR step's notice starts when its reviewers could not be asked.
const REVIEW_NOT_ASKED: &str = "No se pudo pedir la revisión a";

/// `owner/name` of the task's `origin`, when it is on GitHub.
pub(super) fn task_github_repo(task: &DeliveryTask) -> Option<String> {
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
    /// Checks that concluded skipped or neutral (a deploy that runs only on
    /// main, a workflow whose condition was false): they block nothing.
    #[serde(default)]
    pub skipped: u32,
    /// Checks that never started: required checks with no run on the head,
    /// statuses GitHub still expects.
    #[serde(default)]
    pub not_run: Vec<String>,
    /// The base's required checks, when it has any: only they count.
    #[serde(default)]
    pub required: Vec<String>,
}

/// The CI of a PR's head commit, as the merge reads it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiState {
    Passing,
    Failing,
    Pending,
    /// A check never started, or nothing ran on the head: no check, or
    /// (without required checks) every one skipped, as when CI waits for
    /// a label.
    NotRun,
}

impl PrChecks {
    /// What every reader of a PR's CI goes by: the merge, its phase and a
    /// merge found on GitHub.
    pub fn state(&self) -> CiState {
        if self.failing > 0 {
            CiState::Failing
        } else if self.pending > 0 {
            CiState::Pending
        } else if !self.not_run.is_empty() || (self.passing == 0 && self.required.is_empty()) {
            CiState::NotRun
        } else {
            CiState::Passing
        }
    }
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
    /// Those reviews and comments, one line each ("ana: …"), oldest first.
    #[serde(default)]
    pub news: Vec<String>,
    /// When Tinto saw the review decision turn to changes requested.
    #[serde(default)]
    pub changes_requested_at_ms: Option<u64>,
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
    /// Who GitHub is still waiting on for a review.
    #[serde(default)]
    pub requested_reviewers: Vec<String>,
    /// The branch it merges into, as GitHub last said.
    #[serde(default)]
    pub base: Option<String>,
    /// The head commit its checks ran on, as GitHub last said.
    #[serde(default)]
    pub head: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Merged on GitHub while a check on its head failed: what it was.
    #[serde(default)]
    pub merged_failing_ci: Option<String>,
    /// Its head's CI as the merge reads it; `None` in a repo without CI
    /// where no check ran.
    #[serde(default)]
    pub ci: Option<CiState>,
    /// Who must approve it before it merges: the reviewers chosen in Tinto
    /// or asked on GitHub, until they are removed.
    #[serde(default)]
    pub reviewers: Vec<String>,
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
            news: Vec::new(),
            changes_requested_at_ms: None,
            latest_activity: None,
            seen_until: None,
            checked_at_ms: None,
            error: None,
            merged_by: None,
            merged_at: None,
            merge_commit: None,
            approved_by: Vec::new(),
            requested_reviewers: Vec::new(),
            base: None,
            head: None,
            labels: Vec::new(),
            merged_failing_ci: None,
            ci: None,
            reviewers: Vec::new(),
        }
    }

    /// The reviewers who must approve it and have not yet.
    pub fn missing_approvals(&self) -> Vec<String> {
        self.reviewers
            .iter()
            .filter(|login| {
                !self
                    .approved_by
                    .iter()
                    .any(|approved| approved.eq_ignore_ascii_case(login))
            })
            .cloned()
            .collect()
    }

    /// Adds `logins` to who must approve it, once each.
    pub fn add_reviewers(&mut self, logins: &[String]) {
        for login in logins {
            if !self
                .reviewers
                .iter()
                .any(|known| known.eq_ignore_ascii_case(login))
            {
                self.reviewers.push(login.clone());
            }
        }
    }

    /// Why its head's CI does not let it merge, or `None` when it passed.
    /// `label` is the one the repo runs CI on, when it has one: named only
    /// when the PR lacks it.
    pub fn ci_refusal(&self, label: Option<&str>) -> Option<String> {
        let sha = self
            .head
            .as_deref()
            .map_or("su último commit".to_string(), |head| {
                head.chars().take(7).collect()
            });
        match self.checks.state() {
            CiState::Passing => None,
            CiState::Failing => Some(format!(
                "el PR #{} tiene checks fallando en {sha}",
                self.number
            )),
            CiState::Pending => Some(format!(
                "el PR #{} tiene checks en curso en {sha}",
                self.number
            )),
            CiState::NotRun => {
                let missing = label.filter(|label| {
                    !self
                        .labels
                        .iter()
                        .any(|known| known.eq_ignore_ascii_case(label))
                });
                Some(match missing {
                    Some(label) => {
                        format!("la CI no se ejecutó en {sha}: añade la etiqueta {label}")
                    }
                    None => format!("la CI no se ejecutó en {sha}"),
                })
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == "open" || self.state == "unknown"
    }

    /// Reviewed and approved: GitHub's decision when the repo requires
    /// reviews, else at least one approval and no changes requested; and,
    /// either way, by every reviewer who must approve it.
    pub fn is_approved(&self) -> bool {
        let decided = match self.review_decision.as_deref() {
            Some(decision) => decision == "approved",
            None => !self.approved_by.is_empty(),
        };
        decided && self.missing_approvals().is_empty()
    }

    /// Its reviews let it merge: approved or, in a repo that merges without
    /// a review ("PR sin revisor"), with no changes requested, no review
    /// GitHub requires and approved by every reviewer who was chosen anyway.
    pub fn review_allows_merge(&self, review_optional: bool) -> bool {
        if review_optional {
            !matches!(
                self.review_decision.as_deref(),
                Some("changes_requested" | "review_required")
            ) && self.missing_approvals().is_empty()
        } else {
            self.is_approved()
        }
    }
}

/// A reply the user approves with their text, in a batch.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovedReply {
    pub approval_id: String,
    pub body: String,
    #[serde(default)]
    pub images: Vec<super::model::DeliveryReplyImage>,
}

/// How an approval reads among a PR's news ("ana aprobó"): news that asks
/// for no work.
pub(super) const APPROVED_NEWS: &str = " aprobó";

/// `gh pr view --json` fields Tinto reads.
pub const PR_FIELDS: &str =
    "title,body,state,isDraft,mergeable,reviewDecision,reviewRequests,statusCheckRollup,reviews,comments,mergedBy,mergedAt,mergeCommit,baseRefName,headRefOid,labels";

/// Whether the repo has CI at all: workflow files in the worktree's commit.
pub fn has_ci(place: Place<'_>, worktree: &std::path::Path) -> bool {
    !workflow_files(place, worktree).is_empty()
}

fn workflow_files(place: Place<'_>, worktree: &std::path::Path) -> Vec<String> {
    tasks::git(
        place,
        worktree,
        &["ls-tree", "--name-only", "HEAD", ".github/workflows/"],
    )
    .unwrap_or_default()
    .lines()
    .filter(|path| path.ends_with(".yml") || path.ends_with(".yaml"))
    .map(str::to_string)
    .collect()
}

/// The label the repo's workflows run CI on for a PR
/// (`github.event.label.name == 'Ready'`), when they wait for one.
pub fn ci_label(place: Place<'_>, worktree: &std::path::Path) -> Option<String> {
    const KEY: &str = "github.event.label.name";
    workflow_files(place, worktree).iter().find_map(|path| {
        let text = tasks::git(place, worktree, &["show", &format!("HEAD:{path}")]).ok()?;
        let at = text.find(KEY)? + KEY.len();
        let rest = text[at..].trim_start().strip_prefix("==")?.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        let label: String = rest[1..].chars().take_while(|c| *c != quote).collect();
        (!label.is_empty()).then_some(label)
    })
}

/// Why CI is not required on the task's PR: every workflow of the repo
/// filters by paths (`on.pull_request.paths`, `paths-ignore`, and the
/// branches and paths of `push`) that the PR's changed files never reach,
/// so GitHub runs none and reports no check. `None` when one may run or
/// the files cannot be read.
pub fn ci_skipped_by_paths(
    place: Place<'_>,
    worktree: &std::path::Path,
    base: &str,
    branch: &str,
) -> Option<String> {
    let workflows: Vec<String> = workflow_files(place, worktree)
        .iter()
        .filter_map(|path| tasks::git(place, worktree, &["show", &format!("HEAD:{path}")]).ok())
        .collect();
    let base = format!("origin/{}", base.trim_start_matches("origin/"));
    let range = format!("{base}...HEAD");
    let files: Vec<String> = tasks::git(place, worktree, &["diff", "--name-only", &range])
        .ok()?
        .lines()
        .map(str::to_string)
        .collect();
    (!workflows.is_empty()
        && !files.is_empty()
        && workflows
            .iter()
            .all(|workflow| !workflow_runs(workflow, &files, branch)))
    .then(|| {
        "ningún workflow de la CI se ejecuta para los archivos que cambia el PR (todos filtran por paths o ramas): la CI no hace falta".to_string()
    })
}

/// A YAML line without its comment, with its indentation.
fn yaml_lines(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .filter_map(|line| {
            let content = match line.find(" #") {
                Some(at) => &line[..at],
                None => line,
            };
            let trimmed = content.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('#'))
                .then(|| (content.len() - content.trim_start().len(), trimmed))
        })
        .collect()
}

fn unquote(text: &str) -> String {
    text.trim()
        .trim_matches(|c| c == '\'' || c == '"')
        .to_string()
}

/// `key: value` of a mapping line, the key unquoted.
fn yaml_entry(line: &str) -> Option<(String, &str)> {
    let (key, value) = line.split_once(':')?;
    Some((unquote(key), value.trim()))
}

/// The lines under the one at `at`: those indented deeper than it.
fn yaml_block<'a>(lines: &[(usize, &'a str)], at: usize) -> Vec<(usize, &'a str)> {
    let indent = lines[at].0;
    lines[at + 1..]
        .iter()
        .take_while(|(deeper, _)| *deeper > indent)
        .copied()
        .collect()
}

/// A list given inline (`[a, b]`, `a`) or as `- a` lines under its key.
fn yaml_list(inline: &str, block: &[(usize, &str)]) -> Vec<String> {
    if !inline.is_empty() {
        return inline
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(unquote)
            .filter(|item| !item.is_empty())
            .collect();
    }
    block
        .iter()
        .filter_map(|(_, line)| line.strip_prefix("- ").or(line.strip_prefix('-')))
        .map(unquote)
        .collect()
}

/// A mapping key with its inline value and the lines under it.
type YamlKey<'a> = (String, &'a str, Vec<(usize, &'a str)>);

/// The mapping's own keys (shallowest lines) with their value and block.
fn yaml_keys<'a>(lines: &[(usize, &'a str)]) -> Vec<YamlKey<'a>> {
    let Some(top) = lines.iter().map(|(indent, _)| *indent).min() else {
        return Vec::new();
    };
    lines
        .iter()
        .enumerate()
        .filter(|(_, (indent, _))| *indent == top)
        .map(|(at, (_, line))| {
            let line = line.strip_prefix("- ").unwrap_or(line);
            let (key, value) = yaml_entry(line).unwrap_or((unquote(line), ""));
            (key, value, yaml_block(lines, at))
        })
        .collect()
}

/// Whether a GitHub filter list (globs, `!` excluding) takes `item`: the
/// last pattern that matches decides.
fn filter_takes(patterns: &[String], item: &str) -> bool {
    let mut taken = false;
    for pattern in patterns {
        let (negated, glob) = match pattern.strip_prefix('!') {
            Some(glob) => (true, glob),
            None => (false, pattern.as_str()),
        };
        let matches = globset::GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map(|glob| glob.compile_matcher().is_match(item))
            // A pattern Tinto cannot read may take anything.
            .unwrap_or(true);
        if matches {
            taken = !negated;
        }
    }
    taken
}

/// Whether a workflow may run for a PR of `branch` that changes `files`.
/// Anything Tinto does not understand counts as running.
pub fn workflow_runs(text: &str, files: &[String], branch: &str) -> bool {
    let lines = yaml_lines(text);
    let Some(on) = lines.iter().position(|(indent, line)| {
        *indent == 0 && yaml_entry(line).is_some_and(|(key, _)| key == "on" || key == "true")
    }) else {
        return true;
    };
    let inline = yaml_entry(lines[on].1).map_or("", |(_, value)| value);
    let events: Vec<YamlKey> = if inline.is_empty() {
        yaml_keys(&yaml_block(&lines, on))
    } else {
        yaml_list(inline, &[])
            .into_iter()
            .map(|event| (event, "", Vec::new()))
            .collect()
    };
    events.iter().any(|(event, _, block)| {
        if !matches!(
            event.as_str(),
            "pull_request" | "pull_request_target" | "push"
        ) {
            return false;
        }
        let filters = yaml_keys(block);
        let list = |name: &str| {
            filters
                .iter()
                .find(|(key, _, _)| key == name)
                .map(|(_, inline, block)| yaml_list(inline, block))
        };
        if event == "push" {
            if let Some(branches) = list("branches") {
                if !filter_takes(&branches, branch) {
                    return false;
                }
            }
            if list("branches-ignore").is_some_and(|ignored| filter_takes(&ignored, branch)) {
                return false;
            }
            // Only tags: no push of the branch runs it.
            if list("tags").is_some() && list("branches").is_none() {
                return false;
            }
        }
        match (list("paths"), list("paths-ignore")) {
            (Some(paths), _) => files.iter().any(|file| filter_takes(&paths, file)),
            (None, Some(ignored)) => files.iter().any(|file| !filter_takes(&ignored, file)),
            (None, None) => true,
        }
    })
}

fn check_state(check: &Value) -> &'static str {
    // Check runs carry status + conclusion; commit statuses carry state.
    let upper = |key: &str| check[key].as_str().unwrap_or_default().to_ascii_uppercase();
    if let Some(state) = check["state"].as_str() {
        return match state.to_ascii_uppercase().as_str() {
            "SUCCESS" => "passing",
            "FAILURE" | "ERROR" => "failing",
            "EXPECTED" => "not_run",
            _ => "pending",
        };
    }
    if upper("status") != "COMPLETED" {
        return "pending";
    }
    match upper("conclusion").as_str() {
        // A run cancelled (by Tinto, the coordinator or a newer push) did
        // not test the head: it never ran, it did not fail.
        "CANCELLED" => "not_run",
        "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => "failing",
        "SKIPPED" | "NEUTRAL" => "skipped",
        _ => "passing",
    }
}

/// The checks on the head of a `gh pr view --json` answer. With the base's
/// required checks (`requiredChecks`, which `GhCli::pr` adds), only they
/// count, and one with no run on the head never started.
fn head_checks(snapshot: &Value) -> PrChecks {
    let required: Vec<String> = snapshot["requiredChecks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_string))
        .collect();
    let mut checks = PrChecks::default();
    let mut seen: Vec<String> = Vec::new();
    for check in snapshot["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = ["name", "context"]
            .iter()
            .find_map(|key| check[*key].as_str().filter(|name| !name.is_empty()))
            .unwrap_or_default()
            .to_string();
        if !required.is_empty() && !required.contains(&name) {
            continue;
        }
        match check_state(check) {
            "passing" => checks.passing += 1,
            "failing" => checks.failing += 1,
            "skipped" => checks.skipped += 1,
            "not_run" => checks.not_run.push(name.clone()),
            _ => checks.pending += 1,
        }
        seen.push(name);
    }
    for name in &required {
        if !seen.contains(name) && !checks.not_run.contains(name) {
            checks.not_run.push(name.clone());
        }
    }
    checks.required = required;
    checks
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
    pr.head = text(&snapshot["headRefOid"]);
    pr.labels = snapshot["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|label| label["name"].as_str().map(str::to_string))
        .collect();
    pr.checks = head_checks(snapshot);

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
    // Teams have a name instead of a login; only people are asked from Tinto.
    let requested: Vec<String> = snapshot["reviewRequests"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|request| request["login"].as_str())
        .filter(|login| !login.is_empty())
        .map(str::to_string)
        .collect();
    // Who must approve it: one asked on GitHub joins; one whose request was
    // taken back there before they reviewed leaves.
    let reviewed = |login: &str| {
        snapshot["reviews"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|review| {
                review["author"]["login"]
                    .as_str()
                    .is_some_and(|author| author.eq_ignore_ascii_case(login))
            })
    };
    let removed: Vec<String> = pr
        .requested_reviewers
        .iter()
        .filter(|login| !requested.contains(login) && !reviewed(login))
        .cloned()
        .collect();
    pr.reviewers.retain(|login| !removed.contains(login));
    pr.add_reviewers(&requested);
    pr.requested_reviewers = requested;

    // (time, author, is_review, line) of everything others did on the PR.
    let mut activity: Vec<(String, String, bool, String)> = Vec::new();
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
                let body = item["body"].as_str().unwrap_or_default();
                let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
                let line = match (item["state"].as_str(), body.is_empty()) {
                    (Some("CHANGES_REQUESTED"), true) => format!("{author} pidió cambios"),
                    (Some("CHANGES_REQUESTED"), false) => {
                        format!("{author} pidió cambios: {body}")
                    }
                    (Some("APPROVED"), _) => format!("{author}{APPROVED_NEWS}"),
                    (_, true) => String::new(),
                    (_, false) => format!("{author}: {body}"),
                };
                activity.push((time.to_string(), author.to_string(), review, line));
            }
        }
    }
    // Comments on the code (`GhCli::pr` adds GitHub's REST answer): where
    // and what, which a review's verdict alone does not say.
    for item in snapshot["reviewComments"].as_array().into_iter().flatten() {
        let author = item["user"]["login"].as_str().unwrap_or_default();
        let (Some(time), Some(path)) = (item["created_at"].as_str(), item["path"].as_str()) else {
            continue;
        };
        let body = item["body"].as_str().unwrap_or_default();
        let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
        if author.eq_ignore_ascii_case(me) || body.is_empty() {
            continue;
        }
        let at = match item["line"].as_u64().or(item["original_line"].as_u64()) {
            Some(line) => format!("{path}:{line}"),
            None => path.to_string(),
        };
        let line = format!("{author} en {at}: {body}");
        activity.push((time.to_string(), author.to_string(), false, line));
    }
    activity.sort();
    pr.latest_activity = activity
        .iter()
        .map(|(time, _, _, _)| time.clone())
        .max()
        .or(pr.latest_activity.take());
    if pr.seen_until.is_none() {
        // First read: what was there before Tinto followed the PR is not news.
        pr.seen_until = Some(pr.latest_activity.clone().unwrap_or_default());
    }
    let seen = pr.seen_until.clone().unwrap_or_default();
    let fresh: Vec<&(String, String, bool, String)> = activity
        .iter()
        .filter(|(time, _, _, _)| *time > seen)
        .collect();
    pr.new_reviews = fresh.iter().filter(|(_, _, review, _)| *review).count() as u32;
    pr.new_comments = fresh.iter().filter(|(_, _, review, _)| !review).count() as u32;
    let mut authors: Vec<String> = fresh
        .iter()
        .map(|(_, author, _, _)| author.clone())
        .collect();
    authors.sort();
    authors.dedup();
    pr.new_authors = authors;
    pr.news = fresh
        .iter()
        .filter(|(_, _, _, line)| !line.is_empty())
        .map(|(_, _, _, line)| line.clone())
        .collect();
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
    /// Takes back the review requests of `logins` on the PR.
    fn remove_reviewers(&self, _url: &str, _logins: &[String]) -> Result<(), DeliveryError> {
        Err(offline())
    }
    fn feed(&self, etag: Option<&str>) -> Result<Feed, DeliveryError>;
    /// Adds `label` to the PR.
    fn add_label(&self, _url: &str, _label: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Takes `label` off the PR.
    fn remove_label(&self, _url: &str, _label: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Adds a file at `path` on `branch` of `repo` (`owner/name`), creating
    /// the branch with no history when it is missing; a file already there
    /// stays as it is. Returns the branch's commit that has it.
    fn upload_file(
        &self,
        _repo: &str,
        _branch: &str,
        _path: &str,
        _bytes: &[u8],
        _message: &str,
    ) -> Result<String, DeliveryError> {
        Err(offline())
    }
    /// Deletes `branch` of `repo`; one already gone is no error.
    fn delete_branch(&self, _repo: &str, _branch: &str) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// Merges the PR and, with `delete_branch`, deletes its branch on
    /// GitHub. `subject` and `body` are the merge commit's message
    /// (`rebase` has none).
    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        body: &str,
        delete_branch: bool,
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
    /// Replies in the thread of review comment `comment_id` (its first
    /// comment) of PR `number`; returns the reply's URL.
    fn reply_to_review_comment(
        &self,
        _repo: &str,
        _number: u64,
        _comment_id: u64,
        _body: &str,
    ) -> Result<String, DeliveryError> {
        Err(offline())
    }
    /// Comments on the PR; returns the comment's URL.
    fn comment_on_pr(&self, _url: &str, _body: &str) -> Result<String, DeliveryError> {
        Err(offline())
    }
    /// Resolves the review thread that holds comment `comment_id`.
    fn resolve_review_thread(
        &self,
        _repo: &str,
        _number: u64,
        _comment_id: u64,
    ) -> Result<(), DeliveryError> {
        Err(offline())
    }
    /// The latest review comments of every PR in `repo` (`owner/name`),
    /// newest first, as GitHub's REST API lists them: page `page` (from 1)
    /// of a hundred.
    fn repo_review_comments(&self, _repo: &str, _page: u32) -> Result<Value, DeliveryError> {
        Err(offline())
    }
    /// The latest comments in the conversation of `repo`'s issues and PRs,
    /// newest first, as GitHub's REST API lists them: page `page` of a
    /// hundred.
    fn repo_conversation_comments(&self, _repo: &str, _page: u32) -> Result<Value, DeliveryError> {
        Err(offline())
    }
    /// The reviews of `repo`'s latest PRs, each with its summary, shaped as
    /// a REST comment (`id`, `user.login`, `body`, `html_url`, `created_at`).
    fn repo_review_bodies(&self, _repo: &str) -> Result<Value, DeliveryError> {
        Err(offline())
    }
}

fn offline() -> DeliveryError {
    DeliveryError::new("github_unavailable", "GitHub no está conectado")
}

/// The status label a reply opens with ("Fixed:", "Done.", "Fixed in
/// abc1234"), if any: the user's replies say what the code does now instead.
pub fn status_label(body: &str) -> Option<String> {
    let text = body
        .trim_start()
        .trim_start_matches(['*', '_', '>', '#', ' ']);
    let word: String = text.chars().take_while(|c| c.is_alphabetic()).collect();
    if !["fixed", "done", "addressed", "resolved"].contains(&word.to_lowercase().as_str()) {
        return None;
    }
    let after = text[word.len()..].trim_start_matches(['*', '_']);
    let hash = |rest: &str| {
        rest.split_whitespace()
            .next()
            .map(|hash| hash.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
            .is_some_and(|hash| hash.len() >= 4 && hash.chars().all(|c| c.is_ascii_hexdigit()))
    };
    let labelled = match after.chars().next() {
        None | Some(':' | '.' | ',' | '!' | '\n' | '\r') => true,
        // "Fixed in abc1234"
        Some(' ') => after.trim_start().strip_prefix("in ").is_some_and(hash),
        _ => false,
    };
    labelled.then_some(word)
}

/// The questions of a review comment the reply leaves unanswered: those
/// none of whose longer words (five letters or more) the reply mentions.
pub fn unanswered_questions(comment: &str, reply: &str) -> Vec<String> {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.chars().count() >= 5)
            .map(str::to_lowercase)
            .collect()
    };
    let answered = words(reply);
    let mut questions = Vec::new();
    let mut start = 0;
    for (at, c) in comment.char_indices() {
        match c {
            '.' | '!' | '\n' => start = at + 1,
            '?' => {
                let question = comment[start..=at].trim();
                start = at + 1;
                let asked = words(question);
                if !asked.is_empty() && !asked.iter().any(|word| answered.contains(word)) {
                    questions.push(question.to_string());
                }
            }
            _ => {}
        }
    }
    questions
}

/// The warning a reply gets when it leaves the review comment's questions
/// unanswered.
pub fn unanswered_notice(thread: Option<&DeliveryCodeThread>, reply: &str) -> Option<String> {
    let comment = thread?.comment.as_ref()?;
    let unanswered = unanswered_questions(&comment.body, reply);
    (!unanswered.is_empty()).then(|| {
        format!(
            "La respuesta no parece contestar lo que pregunta el revisor: {}",
            unanswered
                .iter()
                .map(|question| format!("«{question}»"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

const COMMENT_GONE: &str = "el comentario ya no existe en GitHub: lo borraron";

/// Why GitHub did not take a reply, in plain words.
fn reply_failure(error: DeliveryError) -> DeliveryError {
    let lower = error.message.to_ascii_lowercase();
    let (category, message) = if lower.contains("404") || lower.contains("not found") {
        ("comment_gone", COMMENT_GONE.to_string())
    } else if lower.contains("locked") {
        (
            "thread_locked",
            "la conversación del PR está bloqueada: GitHub no deja responder".to_string(),
        )
    } else if lower.contains("auth login")
        || lower.contains("not logged")
        || lower.contains("401")
        || lower.contains("bad credentials")
    {
        (
            "github_signed_out",
            "gh no tiene sesión en GitHub: inicia sesión con gh auth login y pide la respuesta otra vez"
                .to_string(),
        )
    } else if error.category == "github_unavailable" {
        return error;
    } else {
        (
            "reply_failed",
            format!("GitHub no publicó la respuesta: {}", error.message),
        )
    };
    DeliveryError::new(category, message)
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
        _delete_branch: bool,
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

    /// `gh api` with a JSON body, sent through a file: a screenshot in
    /// base64 is longer than a command line may be.
    fn api(method: &str, endpoint: &str, body: &Value) -> Result<Value, DeliveryError> {
        let file = std::env::temp_dir().join(format!("tinto-gh-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&file, body.to_string()).map_err(DeliveryError::io)?;
        let input = file.to_string_lossy().to_string();
        let answer = Self::json(&["api", "-X", method, endpoint, "--input", &input]);
        let _ = std::fs::remove_file(&file);
        answer
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

    /// The status checks `branch` of `repo` requires, by branch protection
    /// or a ruleset; none when GitHub does not say.
    fn required_checks(repo: &str, branch: &str) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let mut add = |name: Option<&str>| {
            if let Some(name) = name.filter(|name| !name.is_empty()) {
                if !names.iter().any(|known| known == name) {
                    names.push(name.to_string());
                }
            }
        };
        if let Ok(found) = Self::json(&["api", &format!("repos/{repo}/branches/{branch}")]) {
            let protection = &found["protection"]["required_status_checks"];
            for context in protection["contexts"].as_array().into_iter().flatten() {
                add(context.as_str());
            }
        }
        if let Ok(rules) = Self::json(&["api", &format!("repos/{repo}/rules/branches/{branch}")]) {
            for rule in rules.as_array().into_iter().flatten() {
                if rule["type"] != "required_status_checks" {
                    continue;
                }
                let checks = &rule["parameters"]["required_status_checks"];
                for check in checks.as_array().into_iter().flatten() {
                    add(check["context"].as_str());
                }
            }
        }
        names
    }
}

impl GitHub for GhCli {
    fn pr(&self, url: &str) -> Result<Value, DeliveryError> {
        let mut snapshot = Self::json(&["pr", "view", url, "--json", PR_FIELDS])?;
        if let Some(pr) = parse_pr_url(url) {
            if let Some(base) = snapshot["baseRefName"].as_str() {
                snapshot["requiredChecks"] = Value::from(Self::required_checks(&pr.repo, base));
            }
            // What a review said on the code: news a round must carry.
            if let Ok(comments) = self.pr_review_comments(&pr.repo, pr.number) {
                snapshot["reviewComments"] = comments;
            }
        }
        Ok(snapshot)
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

    fn remove_reviewers(&self, url: &str, logins: &[String]) -> Result<(), DeliveryError> {
        let logins = logins.join(",");
        let output = Self::run(&["pr", "edit", url, "--remove-reviewer", &logins])?;
        if output.status.success() {
            return Ok(());
        }
        Err(DeliveryError::new(
            "github_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }

    fn upload_file(
        &self,
        repo: &str,
        branch: &str,
        path: &str,
        bytes: &[u8],
        message: &str,
    ) -> Result<String, DeliveryError> {
        let head = |repo: &str| {
            Self::json(&["api", &format!("repos/{repo}/git/ref/heads/{branch}")]).map(|found| {
                found["object"]["sha"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
        };
        if head(repo).is_err() {
            // A branch of its own, with no history: nothing to merge from it.
            let tree = Self::api(
                "POST",
                &format!("repos/{repo}/git/trees"),
                &serde_json::json!({"tree": [{
                    "path": "README.md", "mode": "100644", "type": "blob",
                    "content": "QA screenshots Tinto shows in replies to reviewers. Never merged.
                "
                }]}),
            )?;
            let commit = Self::api(
                "POST",
                &format!("repos/{repo}/git/commits"),
                &serde_json::json!({"message": message, "tree": tree["sha"], "parents": []}),
            )?;
            Self::api(
                "POST",
                &format!("repos/{repo}/git/refs"),
                &serde_json::json!({"ref": format!("refs/heads/{branch}"), "sha": commit["sha"]}),
            )?;
        }
        let content = base64::engine::general_purpose::STANDARD.encode(bytes);
        let put = Self::api(
            "PUT",
            &format!("repos/{repo}/contents/{path}"),
            &serde_json::json!({"message": message, "content": content, "branch": branch}),
        );
        match put {
            Ok(done) => Ok(done["commit"]["sha"]
                .as_str()
                .unwrap_or_default()
                .to_string()),
            // Already there (GitHub asks for its sha to replace it): kept.
            Err(error) if error.message.contains("sha") => head(repo),
            Err(error) => Err(error),
        }
    }

    fn delete_branch(&self, repo: &str, branch: &str) -> Result<(), DeliveryError> {
        let output = Self::run(&[
            "api",
            "-X",
            "DELETE",
            &format!("repos/{repo}/git/refs/heads/{branch}"),
        ])?;
        let message = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() || message.contains("Reference does not exist") {
            return Ok(());
        }
        Err(DeliveryError::new(
            "github_failed",
            message.trim().to_string(),
        ))
    }

    fn add_label(&self, url: &str, label: &str) -> Result<(), DeliveryError> {
        Self::act(
            &["pr", "edit", url, "--add-label", label],
            "gh no pudo poner la etiqueta",
        )
        .map(|_| ())
    }

    fn remove_label(&self, url: &str, label: &str) -> Result<(), DeliveryError> {
        Self::act(
            &["pr", "edit", url, "--remove-label", label],
            "gh no pudo quitar la etiqueta",
        )
        .map(|_| ())
    }

    fn merge(
        &self,
        url: &str,
        method: &str,
        subject: &str,
        body: &str,
        delete_branch: bool,
    ) -> Result<(), DeliveryError> {
        let flag = format!("--{method}");
        let mut args = vec!["pr", "merge", url, flag.as_str()];
        if delete_branch {
            args.push("--delete-branch");
        }
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

    fn reply_to_review_comment(
        &self,
        repo: &str,
        number: u64,
        comment_id: u64,
        body: &str,
    ) -> Result<String, DeliveryError> {
        let path = format!("repos/{repo}/pulls/{number}/comments/{comment_id}/replies");
        let field = format!("body={body}");
        Self::act(
            &[
                "api",
                "-X",
                "POST",
                &path,
                "-f",
                &field,
                "--jq",
                ".html_url",
            ],
            "gh no pudo responder al comentario",
        )
    }

    fn repo_review_comments(&self, repo: &str, page: u32) -> Result<Value, DeliveryError> {
        let path = format!(
            "repos/{repo}/pulls/comments?sort=created&direction=desc&per_page=100&page={page}"
        );
        let output = Self::act(
            &["api", &path],
            "gh no pudo leer los comentarios del repositorio",
        )?;
        serde_json::from_str(&output)
            .map_err(|error| DeliveryError::new("github_failed", error.to_string()))
    }

    fn repo_conversation_comments(&self, repo: &str, page: u32) -> Result<Value, DeliveryError> {
        let path = format!(
            "repos/{repo}/issues/comments?sort=created&direction=desc&per_page=100&page={page}"
        );
        let output = Self::act(
            &["api", &path],
            "gh no pudo leer las conversaciones del repositorio",
        )?;
        serde_json::from_str(&output)
            .map_err(|error| DeliveryError::new("github_failed", error.to_string()))
    }

    fn repo_review_bodies(&self, repo: &str) -> Result<Value, DeliveryError> {
        let (owner, name) = repo.split_once('/').ok_or_else(|| {
            DeliveryError::new("github_failed", format!("repositorio inválido: {repo}"))
        })?;
        // One query for the reviews of the 30 PRs updated last.
        let query = "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { pullRequests(first: 30, orderBy: {field: UPDATED_AT, direction: DESC}) { nodes { reviews(first: 50) { nodes { databaseId body url createdAt author { login } } } } } } }";
        let select = "[.data.repository.pullRequests.nodes[].reviews.nodes[] | {id: .databaseId, user: {login: (.author.login // \"\")}, body, html_url: .url, created_at: .createdAt}]";
        let output = Self::act(
            &[
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("owner={owner}"),
                "-f",
                &format!("name={name}"),
                "--jq",
                select,
            ],
            "gh no pudo leer las revisiones del repositorio",
        )?;
        serde_json::from_str(&output)
            .map_err(|error| DeliveryError::new("github_failed", error.to_string()))
    }

    fn comment_on_pr(&self, url: &str, body: &str) -> Result<String, DeliveryError> {
        // gh prints the new comment's URL last.
        let output = Self::act(
            &["pr", "comment", url, "--body", body],
            "gh no pudo comentar en el PR",
        )?;
        Ok(output.lines().last().unwrap_or_default().trim().to_string())
    }

    fn resolve_review_thread(
        &self,
        repo: &str,
        number: u64,
        comment_id: u64,
    ) -> Result<(), DeliveryError> {
        let (owner, name) = repo.split_once('/').unwrap_or((repo, ""));
        let query = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100) { nodes { id comments(first: 100) { nodes { databaseId } } } } } } }";
        let select = format!(
            ".data.repository.pullRequest.reviewThreads.nodes[] | select(any(.comments.nodes[]; .databaseId == {comment_id})) | .id"
        );
        let thread = Self::act(
            &[
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("owner={owner}"),
                "-f",
                &format!("name={name}"),
                "-F",
                &format!("number={number}"),
                "--jq",
                &select,
            ],
            "gh no pudo leer los hilos del PR",
        )?;
        let thread = thread.lines().next().unwrap_or_default().trim();
        if thread.is_empty() {
            return Err(DeliveryError::new(
                "github_failed",
                "GitHub no tiene el hilo de ese comentario",
            ));
        }
        let mutation = "mutation($id: ID!) { resolveReviewThread(input: {threadId: $id}) { thread { isResolved } } }";
        Self::act(
            &[
                "api",
                "graphql",
                "-f",
                &format!("query={mutation}"),
                "-f",
                &format!("id={thread}"),
            ],
            "gh no pudo resolver el hilo",
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

    /// The task whose branch this one is based on: the package it builds
    /// on, or any other active task of the repo whose branch is its base.
    pub(super) fn base_task(
        &self,
        task: &DeliveryTask,
    ) -> Result<Option<DeliveryTask>, DeliveryError> {
        let base = task.base_ref.trim_start_matches("origin/");
        let store = self.store()?;
        if let Some(other_id) = task.package.as_ref().and_then(|p| p.builds_on.as_deref()) {
            return Ok(store.task(other_id)?.filter(|other| other.branch == base));
        }
        Ok(store
            .tasks()?
            .into_iter()
            .find(|other| other.id != task.id && other.repo == task.repo && other.branch == base))
    }

    /// Once the task a task is based on (see `base_task`) has merged, the
    /// task's PR merges where that one did: Tinto points it there (GitHub
    /// may have done it already, deleting the merged branch). With merge
    /// commits the PR then shows only its own commits; after a squash or
    /// rebase merge the branch still carries the merged commits as they
    /// were, so it is marked for a restack (`restack_from`), which its next
    /// sync step does.
    pub fn retarget_packages(&self) -> Result<usize, DeliveryError> {
        let tasks = self.store()?.tasks()?;
        let mut moved = 0;
        for task in &tasks {
            let Some(other) = self.base_task(task)? else {
                continue;
            };
            if !self.task_merged(&other)? {
                continue;
            }
            let rewritten = self
                .latest_approval(&other.id, DeliveryRung::Merge)?
                .and_then(|merge| merge.merge_method)
                .is_some_and(|method| method != "merge");
            // What the branch has of the merged one: its local branch, else
            // what Tinto last published of it.
            let place = Place::of(other.distro.as_deref());
            let restack_from = match rewritten {
                false => None,
                true => tasks::git(
                    place,
                    &other.repo,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{}", other.branch),
                    ],
                )
                .ok()
                .filter(|commit| !commit.is_empty())
                .or(self
                    .latest_approval(&other.id, DeliveryRung::Push)?
                    .and_then(|push| push.tip)),
            };
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
            moved_task.restack_from = restack_from.clone();
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
                &match restack_from {
                    None => format!("{} se fusionó: esta tarea va ahora a {branch}", other.label()),
                    Some(_) => format!(
                        "{} se fusionó reescrita (squash o rebase): esta tarea va ahora a {branch} y necesita un restack; pide el paso sync, que la rebasa sobre {branch}, y después el push",
                        other.label()
                    ),
                },
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
        let task = self.task(task_id)?;
        // Whether CI that never ran is still awaited (see `ci_not_needed`).
        let ci_needed = self.ci_not_needed(&task, &pr).is_none();
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
            let state = pr.checks.state();
            pr.ci = (state != CiState::NotRun || ci_needed).then_some(state);
            let asked =
                |pr: &DeliveryPr| pr.review_decision.as_deref() == Some("changes_requested");
            if asked(&pr) && !asked(&before) {
                pr.changes_requested_at_ms = pr.checked_at_ms;
            }
            store.put_pr(&pr)?;
            if let Some(news) = pr_news(&before, &pr) {
                store.record_event(now_ms(), "pr_changed", Some(task_id), None, &news)?;
            }
            // Changes asked send the work back as they arrive, whatever the
            // push's state: the round is there before any stage answers it.
            if asked(&pr) && !asked(&before) {
                super::rounds::for_pr(&store, task_id)?;
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
        self.sync_conflicted_pr(&pr)?;
        // Merged since Tinto last read it, not found merged when linked.
        if pr.state == "merged" && before.state != "merged" && before.checked_at_ms.is_some() {
            self.sync_after_merge(&pr)?;
        }
        // Done with: the screenshots its replies showed are not kept.
        let done = |pr: &DeliveryPr| pr.state == "merged" || pr.state == "closed";
        if done(&pr) && !done(&before) && before.checked_at_ms.is_some() {
            self.drop_evidence(&self.task(task_id)?);
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
            Some(merge) if merge.status == DeliveryApprovalStatus::Executed => None,
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
                Some(merge)
            }
            _ => Some(found_step(
                &pr.task_id,
                DeliveryRung::Merge,
                format!("PR #{}", pr.number),
                merged_outside(pr),
            )),
        };
        if let Some(step) = step {
            self.save_approval(&step, "merge_found")?;
        }
        self.check_merged_ci(pr)
    }

    /// A PR merged on GitHub while a check on its head failed: the task says
    /// so, and a round fixes it on the base. Skipped, running or never run
    /// checks are no failure; one recorded as failing that GitHub no longer
    /// shows failing (read again, or read before skipped checks stopped
    /// counting) is taken back, its round closed.
    fn check_merged_ci(&self, pr: &DeliveryPr) -> Result<(), DeliveryError> {
        if pr.checks.state() != CiState::Failing {
            return self.clear_merged_ci(pr);
        }
        if pr.merged_failing_ci.is_some() {
            return Ok(());
        }
        let task = self.task(&pr.task_id)?;
        let Some(why) = self.ci_refusal(&task, pr) else {
            return Ok(());
        };
        let base = pr.base.clone().unwrap_or_else(|| task.base_ref.clone());
        let store = self.store()?;
        let mut merged = pr.clone();
        merged.merged_failing_ci = Some(why.clone());
        store.put_pr(&merged)?;
        store.record_event(
            now_ms(),
            "pr_merged_failing_ci",
            Some(&task.id),
            None,
            &format!("PR #{} fusionado con checks fallando: {why}", pr.number),
        )?;
        super::rounds::open_round(
            &store,
            &task.id,
            super::rounds::NewRound {
                kind: "ci",
                short: "CI en rojo".to_string(),
                cause: format!(
                    "El PR #{} se fusionó en GitHub con checks fallando: arréglalo en {base}",
                    pr.number
                ),
                items: vec![why],
                source: Some(format!("merged:{}", pr.url)),
            },
        )?;
        drop(store);
        self.notify();
        Ok(())
    }

    /// Takes back a merge recorded with failing CI that GitHub does not show
    /// failing: the merge stands, and the round to fix it closes.
    fn clear_merged_ci(&self, pr: &DeliveryPr) -> Result<(), DeliveryError> {
        if pr.merged_failing_ci.is_none() {
            return Ok(());
        }
        let store = self.store()?;
        let mut merged = pr.clone();
        merged.merged_failing_ci = None;
        store.put_pr(&merged)?;
        let source = Some(format!("merged:{}", pr.url));
        for mut round in store.task_rounds(&pr.task_id)? {
            if round.source != source || !round.is_open() {
                continue;
            }
            round.closed_at_ms = Some(now_ms());
            round.outcome = Some("CI passed; no fix needed".to_string());
            store.put_round(&round)?;
            store.record_event(
                now_ms(),
                "round_closed",
                Some(&pr.task_id),
                None,
                &super::rounds::round_text(&round),
            )?;
        }
        drop(store);
        self.notify();
        Ok(())
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
        pr.news.clear();
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
        let now = now_ms();
        for pr in open {
            let key = format!("{}#{}", pr.repo, pr.number);
            // What can change any moment is read on every pass: CI running,
            // red (it may be running again), not run yet, conflicts.
            let hot = pr.state == "unknown"
                || pr.checks.pending > 0
                || pr.checks.failing > 0
                || pr.ci == Some(CiState::NotRun)
                || pr.conflicts;
            let stale = pr
                .checked_at_ms
                .is_none_or(|at| now.saturating_sub(at) >= PR_READ_EVERY.as_millis() as u64);
            if hot || stale || mentioned.contains(&key) {
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
        approval: &mut DeliveryApproval,
    ) -> Result<PrRef, DeliveryError> {
        let task = self.task(&approval.task_id)?;
        // Its QA screenshots go up first: the description shows them, and
        // the step then holds the text GitHub has.
        let body = self.text_with_images(approval)?;
        approval.body = body;
        let repo = task_github_repo(&task).ok_or_else(|| {
            DeliveryError::new("github_unavailable", "el remoto origin no está en GitHub")
        })?;
        let github = self.github_client();
        let open = pick_branch_pr(&github.branch_prs(&repo, &task.branch)?).filter(|found| {
            github
                .pr(&found.url)
                .is_ok_and(|pr| pr["state"].as_str() == Some("OPEN"))
        });
        let found = match open {
            Some(found) => {
                github.edit_pr(&found.url, &approval.title, &approval.body)?;
                found
            }
            None => {
                let base = task.base_ref.trim_start_matches("origin/");
                let base = (base != "HEAD" && !base.is_empty()).then_some(base);
                let url =
                    github.create_pr(&repo, base, &task.branch, &approval.title, &approval.body)?;
                parse_pr_url(&url).ok_or_else(|| {
                    DeliveryError::new(
                        "github_failed",
                        format!("gh no devolvió la URL del PR abierto: {url}"),
                    )
                })?
            }
        };
        // Without it the merge would wait for a CI that never runs.
        if let Some(problem) = self.label_for_ci(&task, &found.url, false) {
            eprintln!("tinto: delivery PR #{}: {problem}", found.number);
        }
        Ok(found)
    }

    /// Just published: CI starts on the new head, so the PR shows it running
    /// until GitHub says otherwise, and is read again on the next pass.
    pub(super) fn expect_ci(&self, task: &DeliveryTask) -> Result<(), DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        let store = self.store()?;
        let Some(mut pr) = store.pr(&task.id)?.filter(DeliveryPr::is_open) else {
            return Ok(());
        };
        pr.head = tasks::head(place, &task.worktree).ok().or(pr.head);
        pr.checks = PrChecks {
            pending: 1,
            ..PrChecks::default()
        };
        pr.ci = Some(CiState::Pending);
        store.put_pr(&pr)?;
        drop(store);
        self.notify();
        Ok(())
    }

    /// Makes the repo's CI run on the PR's head when it runs only on a
    /// label: the label goes on, and after a push it goes off and on again
    /// (adding it is what starts CI, and one already there adds nothing).
    /// Returns what could not be done, for the step's notice. A task that
    /// needs no CI gets no label.
    pub(super) fn label_for_ci(
        &self,
        task: &DeliveryTask,
        url: &str,
        pushed: bool,
    ) -> Option<String> {
        if task.skip_ci {
            return None;
        }
        let place = Place::of(task.distro.as_deref());
        let label = ci_label(place, &task.worktree)?;
        let github = self.github_client();
        let labelled = self
            .task_pr(&task.id)
            .ok()
            .flatten()
            .is_some_and(|pr| pr.labels.contains(&label));
        if pushed && labelled {
            if let Err(error) = github.remove_label(url, &label) {
                return Some(format!(
                    "no se pudo quitar la etiqueta {label} para relanzar la CI: {error}"
                ));
            }
        }
        if let Err(error) = github.add_label(url, &label) {
            return Some(format!(
                "no se pudo poner la etiqueta {label}, sin la que la CI no corre: {error}"
            ));
        }
        None
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

    /// Changes who reviews the task's open PR: `request` are asked for a
    /// review (again, when they already gave one), `remove` lose their
    /// pending request. Returns the PR as GitHub has it afterwards.
    pub fn change_pr_reviewers(
        &self,
        task_id: &str,
        request: Vec<String>,
        remove: Vec<String>,
    ) -> Result<Option<DeliveryPr>, DeliveryError> {
        let pr = self.linked_pr(task_id)?;
        if pr.state != "open" {
            return Err(DeliveryError::new(
                "pr_not_open",
                "solo se cambian los revisores de un PR abierto",
            ));
        }
        let clean = |logins: Vec<String>| -> Vec<String> {
            logins
                .iter()
                .map(|login| login.trim().trim_start_matches('@').to_string())
                .filter(|login| !login.is_empty())
                .collect()
        };
        let (request, remove) = (clean(request), clean(remove));
        let github = self.github();
        if !remove.is_empty() {
            github.remove_reviewers(&pr.url, &remove)?;
        }
        if !request.is_empty() {
            github.request_reviewers(&pr.url, &request)?;
        }
        // Who must approve it before it merges changes with them.
        {
            let store = self.store()?;
            if let Some(mut stored) = store.pr(task_id)? {
                stored
                    .reviewers
                    .retain(|login| !remove.iter().any(|gone| gone.eq_ignore_ascii_case(login)));
                stored.add_reviewers(&request);
                store.put_pr(&stored)?;
            }
        }
        self.refresh_pr(task_id, Duration::ZERO)
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

    /// Approves the task's replies with the user's texts and returns once
    /// they are saved; a background thread then posts them one after
    /// another, as GitHub wants one user's writes, and asks `rerequest` to
    /// review again after the last. Each reply shows as posted or failed when
    /// its turn ends; one Tinto closes before posting stays pending.
    pub fn approve_replies(
        &self,
        replies: Vec<ApprovedReply>,
        rerequest: Vec<String>,
    ) -> Result<(), DeliveryError> {
        let mut task_id = None;
        for reply in &replies {
            let saved = self.set_reply_body(&reply.approval_id, &reply.body)?;
            self.set_reply_images(&reply.approval_id, reply.images.clone())?;
            task_id = Some(saved.task_id);
        }
        let service = self.clone();
        std::thread::spawn(move || {
            for reply in replies {
                if let Err(error) = service.decide_approval(&reply.approval_id, true, None) {
                    eprintln!(
                        "tinto: no se pudo responder {}: {}",
                        reply.approval_id, error.message
                    );
                }
            }
            if let Some(task_id) = task_id.filter(|_| !rerequest.is_empty()) {
                if let Err(error) = service.change_pr_reviewers(&task_id, rerequest, Vec::new()) {
                    eprintln!(
                        "tinto: no se pudo volver a pedir la revisión: {}",
                        error.message
                    );
                }
            }
        });
        Ok(())
    }

    /// The reply as the user edited it before approving: the text posted.
    pub fn set_reply_body(
        &self,
        approval_id: &str,
        body: &str,
    ) -> Result<DeliveryApproval, DeliveryError> {
        let mut approval = self.approval(approval_id)?;
        if approval.rung != DeliveryRung::PrReply
            || approval.status != DeliveryApprovalStatus::Pending
        {
            return Err(DeliveryError::new(
                "invalid_approval",
                "solo se edita una respuesta que aún no se aprobó",
            ));
        }
        if body.trim().is_empty() {
            return Err(DeliveryError::new(
                "invalid_approval",
                "la respuesta no puede quedar vacía",
            ));
        }
        approval.body = body.to_string();
        approval.notice = unanswered_notice(approval.thread.as_ref(), body);
        self.save_approval(&approval, "pr_reply_edited")?;
        Ok(approval)
    }

    /// Posts an approved reply as the user: in the review comment's thread,
    /// or as a comment on the PR. A reply of the user's with the same text
    /// already in the thread is taken as this one, so it is never posted
    /// twice. Returns its URL, and a notice when the thread could not be
    /// resolved after replying.
    pub(super) fn post_reply(
        &self,
        approval: &DeliveryApproval,
    ) -> Result<(String, Option<String>), DeliveryError> {
        let url = approval.comment_url.as_deref().unwrap_or_default();
        let text = self.text_with_images(approval)?;
        let body = text.as_str();
        let github = self.github();
        let Some((repo, number, id)) = super::threads::parse_comment_url(url) else {
            let pr = parse_pr_url(url).ok_or_else(|| {
                DeliveryError::new("invalid_approval", "la respuesta no tiene PR")
            })?;
            let posted = github.comment_on_pr(&pr.url, body).map_err(reply_failure)?;
            return Ok((posted, None));
        };
        let comments = github
            .pr_review_comments(&repo, number)
            .map_err(reply_failure)?;
        let comments = comments.as_array().cloned().unwrap_or_default();
        let Some(comment) = comments
            .iter()
            .find(|comment| comment["id"].as_u64() == Some(id))
        else {
            return Err(DeliveryError::new("comment_gone", COMMENT_GONE));
        };
        // GitHub answers in the thread of its first comment.
        let first = comment["in_reply_to_id"].as_u64().unwrap_or(id);
        let me = self.github_login(&*github).map_err(reply_failure)?;
        let existing = comments.iter().find(|reply| {
            reply["in_reply_to_id"].as_u64() == Some(first)
                && reply["user"]["login"].as_str() == Some(me.as_str())
                && reply["body"].as_str().map(str::trim) == Some(body)
        });
        let posted = match existing {
            Some(reply) => reply["html_url"].as_str().unwrap_or(url).to_string(),
            None => github
                .reply_to_review_comment(&repo, number, first, body)
                .map_err(reply_failure)?,
        };
        let notice = approval
            .resolve_thread
            .then(|| github.resolve_review_thread(&repo, number, first).err())
            .flatten()
            .map(|error| {
                format!(
                    "Respondido, pero no se pudo resolver el hilo: {}",
                    reply_failure(error).message
                )
            });
        Ok((posted, notice))
    }

    /// Up to five of the user's latest replies to review comments in the
    /// task's repository, newest first. Empty when GitHub cannot say.
    pub(super) fn style_examples(&self, task: &DeliveryTask) -> Vec<String> {
        let Some(repo) = task_github_repo(task) else {
            return Vec::new();
        };
        let github = self.github();
        let (Ok(me), Ok(comments)) = (
            self.github_login(&*github),
            // One page: enough to find the user's latest replies.
            github.repo_review_comments(&repo, 1),
        ) else {
            return Vec::new();
        };
        comments
            .as_array()
            .into_iter()
            .flatten()
            .filter(|comment| {
                comment["in_reply_to_id"].is_u64()
                    && comment["user"]["login"].as_str() == Some(me.as_str())
            })
            .filter_map(|comment| comment["body"].as_str())
            .map(|body| body.trim().to_string())
            .filter(|body| !body.is_empty())
            .take(5)
            .collect()
    }

    /// The task's own commits that changed the commented file since the
    /// review comment, newest first (at most five), each with its link in
    /// the PR. Empty for a general comment or when git cannot say.
    pub(super) fn fix_commits(
        &self,
        task: &DeliveryTask,
        comment_url: Option<&str>,
        thread: Option<&DeliveryCodeThread>,
    ) -> Vec<DeliveryFixCommit> {
        let (Some((repo, number, _)), Some(thread)) = (
            comment_url.and_then(super::threads::parse_comment_url),
            thread,
        ) else {
            return Vec::new();
        };
        let Some(since) = thread.comment.as_ref().map(|comment| &comment.created_at) else {
            return Vec::new();
        };
        if thread.path.is_empty() || since.is_empty() {
            return Vec::new();
        }
        let place = Place::of(task.distro.as_deref());
        let since = format!("--since={since}");
        // The task's own commits: not its base's, nor those a sync merged in.
        let range = format!("{}..HEAD", task.base_commit);
        let log = tasks::git(
            place,
            &task.worktree,
            &[
                "log",
                "-5",
                "--first-parent",
                "--no-merges",
                "--format=%H%x09%s",
                &since,
                &range,
                "--",
                &thread.path,
            ],
        )
        .unwrap_or_default();
        log.lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(sha, subject)| DeliveryFixCommit {
                url: format!("https://github.com/{repo}/pull/{number}/commits/{sha}"),
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
            .collect()
    }

    /// A review thread is resolved once its fix is on GitHub: no commit of
    /// the branch that changes the commented file is missing from
    /// `origin/<branch>`, and the file has nothing uncommitted. Without a
    /// path, the whole branch is checked.
    pub(super) fn check_fix_pushed(
        &self,
        task: &DeliveryTask,
        path: &str,
    ) -> Result<(), DeliveryError> {
        let place = Place::of(task.distro.as_deref());
        let remote = format!("origin/{}", task.branch);
        let unpushed = |why: String| {
            DeliveryError::new(
                "fix_unpushed",
                format!("{why}: resolve_thread espera a que el arreglo esté publicado; pide el push y vuelve a pedir la respuesta, o pídela sin resolve_thread"),
            )
        };
        let git = |args: &[&str]| tasks::git(place, &task.worktree, args);
        if git(&["rev-parse", "--verify", "--quiet", &remote]).is_err() {
            return Err(unpushed(format!(
                "la rama {} no está publicada",
                task.branch
            )));
        }
        let range = format!("{remote}..HEAD");
        let (only, what) = match path {
            "" => (Vec::new(), "la rama"),
            path => (vec!["--", path], path),
        };
        let commits = git(&[&["log", "--format=%h", &range][..], &only].concat())?;
        if let Some(commit) = commits.lines().next() {
            return Err(unpushed(format!(
                "el commit {commit}, que cambia {what}, no está en {remote}"
            )));
        }
        if !git(&[&["status", "--porcelain"][..], &only].concat())?.is_empty() {
            return Err(unpushed(format!("{what} tiene cambios sin commit")));
        }
        Ok(())
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
                // They must all approve it before it merges.
                let store = self.store()?;
                if let Some(mut stored) = store.pr(task_id)? {
                    stored.add_reviewers(&step.reviewers);
                    store.put_pr(&stored)?;
                }
                drop(store);
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
                if let Err(error) = service.read_parents() {
                    eprintln!("tinto: delivery could not read the issues' parents: {error}");
                }
                if let Err(error) = service.watch_guides() {
                    eprintln!("tinto: delivery could not collect the guides' comments: {error}");
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
    fn a_reply_github_refused_says_why_in_plain_words() {
        let why = |message: &str| reply_failure(DeliveryError::new("github_failed", message));
        assert_eq!(why("gh: Not Found (HTTP 404)").category, "comment_gone");
        assert_eq!(
            why("To get started with GitHub CLI, please run:  gh auth login").category,
            "github_signed_out"
        );
        assert_eq!(
            why("gh: Unable to create comment because issue is locked. (HTTP 403)").category,
            "thread_locked"
        );
        assert!(why("boom").message.contains("boom"));
    }

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
                passing: 1,
                failing: 1,
                pending: 2,
                skipped: 1,
                not_run: Vec::new(),
                required: Vec::new(),
            }
        );
        assert_eq!(pr.checks.state(), CiState::Failing);
        // What was there before Tinto followed the PR is not news.
        assert_eq!(pr.new_reviews, 0);
        assert_eq!(pr.seen_until.as_deref(), Some("2026-10-14T08:00:00Z"));

        let reviews = json!([
            {"author": {"login": "ana"}, "state": "COMMENTED", "submittedAt": "2026-10-14T08:00:00Z"},
            {"author": {"login": "luis"}, "state": "CHANGES_REQUESTED", "submittedAt": "2026-10-14T09:00:00Z"},
            {"author": {"login": "ME"}, "state": "COMMENTED", "submittedAt": "2026-10-14T09:30:00Z"}
        ]);
        let comments = json!([{"author": {"login": "ana"}, "createdAt": "2026-10-14T09:10:00Z"}]);
        // With the review's comments on the code; the user's own replies are
        // no news.
        let mut read = snapshot(reviews.clone(), comments.clone());
        read["reviewComments"] = json!([
            {"user": {"login": "luis"}, "path": "src/a.rs", "line": 12, "body": "Esto  falla con None", "created_at": "2026-10-14T09:00:00Z"},
            {"user": {"login": "me"}, "path": "src/a.rs", "line": 12, "body": "Hecho", "created_at": "2026-10-14T09:20:00Z"}
        ]);
        apply_snapshot(&mut pr, &read, "me");
        assert_eq!((pr.new_reviews, pr.new_comments), (1, 2));
        assert_eq!(pr.new_authors, vec!["ana".to_string(), "luis".to_string()]);
        assert!(
            pr.news
                .contains(&"luis en src/a.rs:12: Esto falla con None".to_string()),
            "{:?}",
            pr.news
        );

        pr.seen_until = pr.latest_activity.clone();
        apply_snapshot(&mut pr, &snapshot(reviews, comments), "me");
        assert_eq!((pr.new_reviews, pr.new_comments), (0, 0));

        apply_snapshot(&mut pr, &json!({"state": "MERGED"}), "me");
        assert_eq!(pr.state, "merged");
        assert!(!pr.is_open());
    }

    /// AIRInstitute/agentos#327 at 163a9b8 as GitHub gave it: builds passed,
    /// docker jobs skipped (they run on push to main) and the whole PR Agent
    /// workflow skipped by its own condition.
    fn agentos_327() -> Value {
        let run = |workflow: &str, name: &str, conclusion: &str| json!({"__typename": "CheckRun", "workflowName": workflow, "name": name, "status": "COMPLETED", "conclusion": conclusion});
        json!([
            run("PR Agent", "PR Agent review and describe", "SKIPPED"),
            run("API Gateway CI", "ci / build", "SUCCESS"),
            run("Frontend CI", "build", "SUCCESS"),
            run("Scheduler service CI", "ci / build", "SUCCESS"),
            run("Session Executor Service CI", "ci / build", "SUCCESS"),
            run("Webhooks service CI", "ci / build", "SUCCESS"),
            run("Workflows service CI", "ci / build", "SUCCESS"),
            run("API Gateway CI", "ci / docker_image", "SKIPPED"),
            run("Frontend CI", "docker_image", "SKIPPED"),
            run("Scheduler service CI", "ci / docker_image", "SKIPPED"),
            run(
                "Session Executor Service CI",
                "ci / docker_image",
                "SKIPPED"
            ),
            run("Webhooks service CI", "ci / docker_image", "SKIPPED"),
            run("Workflows service CI", "ci / docker_image", "SKIPPED")
        ])
    }

    #[test]
    fn the_head_ci_counts_skipped_and_neutral_checks_as_no_block() {
        let run = |name: &str, status: &str, conclusion: &str| json!({"name": name, "status": status, "conclusion": conclusion});
        let ok = || run("build", "COMPLETED", "SUCCESS");
        let cases = [
            (
                "agentos#327",
                agentos_327(),
                json!(null),
                CiState::Passing,
                vec![],
            ),
            (
                "neutral beside a pass",
                json!([ok(), run("lint", "COMPLETED", "NEUTRAL")]),
                json!(null),
                CiState::Passing,
                vec![],
            ),
            (
                "one failing",
                json!([ok(), run("lint", "COMPLETED", "TIMED_OUT")]),
                json!(null),
                CiState::Failing,
                vec![],
            ),
            (
                "one running",
                json!([ok(), run("e2e", "QUEUED", "")]),
                json!(null),
                CiState::Pending,
                vec![],
            ),
            (
                "nothing on the sha",
                json!([]),
                json!(null),
                CiState::NotRun,
                vec![],
            ),
            (
                "everything skipped, as without the CI label",
                json!([run("build", "COMPLETED", "SKIPPED")]),
                json!(null),
                CiState::NotRun,
                vec![],
            ),
            (
                "a status still expected",
                json!([ok(), {"context": "ci/jenkins", "state": "EXPECTED"}]),
                json!(null),
                CiState::NotRun,
                vec!["ci/jenkins"],
            ),
            // Required checks decide: others failing do not count, theirs
            // skipped pass, and one missing never started.
            (
                "required passed, another failing",
                json!([ok(), run("lint", "COMPLETED", "FAILURE")]),
                json!(["build"]),
                CiState::Passing,
                vec![],
            ),
            (
                "required skipped",
                json!([run("build", "COMPLETED", "SKIPPED")]),
                json!(["build"]),
                CiState::Passing,
                vec![],
            ),
            (
                "required missing",
                json!([ok()]),
                json!(["build", "e2e"]),
                CiState::NotRun,
                vec!["e2e"],
            ),
            // Cancelled tested nothing: not run, not failing.
            (
                "cancelled",
                json!([run("build", "COMPLETED", "CANCELLED")]),
                json!([]),
                CiState::NotRun,
                vec!["build"],
            ),
        ];
        for (case, rollup, required, state, not_run) in cases {
            let checks =
                head_checks(&json!({"statusCheckRollup": rollup, "requiredChecks": required}));
            assert_eq!(checks.state(), state, "{case}");
            assert_eq!(checks.not_run, not_run, "{case}");
        }
    }

    #[test]
    fn a_workflow_runs_only_for_the_paths_and_branches_it_filters_on() {
        let backend = "name: Backend\non:\n  push:\n    branches: [main]\n  pull_request:\n    paths:\n      - 'web-application/backend/**' # API\n      - \"!**/*.md\"\njobs: {}\n";
        let docs_ignored = "on:\n  pull_request:\n    paths-ignore: ['docs/**', '**.md']\n";
        let every = "on: [push, pull_request]\n";
        let manual = "on:\n  workflow_dispatch:\n  schedule:\n    - cron: '0 0 * * *'\n";
        let files =
            |names: &[&str]| -> Vec<String> { names.iter().map(|name| name.to_string()).collect() };
        let dpp = files(&["web-application/Dockerfile", "docs/adr/0007.md"]);
        let api = files(&["web-application/backend/src/app.ts"]);
        let branch = "delivery/dpp-199";
        for (case, workflow, changed, runs) in [
            ("paths none match", backend, &dpp, false),
            ("paths match", backend, &api, true),
            (
                "negated path",
                backend,
                &files(&["web-application/backend/README.md"]),
                false,
            ),
            (
                "all ignored",
                docs_ignored,
                &files(&["docs/a.md", "README.md"]),
                false,
            ),
            ("one not ignored", docs_ignored, &dpp, true),
            ("no filter", every, &dpp, true),
            ("no pr or push", manual, &dpp, false),
        ] {
            assert_eq!(workflow_runs(workflow, changed, branch), runs, "{case}");
        }
        // A push to any branch runs it whatever the paths.
        let push = "on:\n  push:\n  pull_request:\n    paths: ['src/**']\n";
        assert!(workflow_runs(push, &dpp, branch));
    }

    #[test]
    fn the_ci_label_is_asked_only_when_the_pr_lacks_it() {
        let reference = parse_pr_url("https://github.com/o/r/pull/7").unwrap();
        let mut pr = DeliveryPr::linked("t1", &reference);
        pr.head = Some("163a9b8f4c".into());
        assert_eq!(
            pr.ci_refusal(Some("Ready")).as_deref(),
            Some("la CI no se ejecutó en 163a9b8: añade la etiqueta Ready")
        );
        pr.labels = vec!["ready".into()];
        assert_eq!(
            pr.ci_refusal(Some("Ready")).as_deref(),
            Some("la CI no se ejecutó en 163a9b8")
        );
    }
}
