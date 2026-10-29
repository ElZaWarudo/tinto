//! Guides: reviewers the user marks as the repo's visual or technical
//! guide. Tinto collects their review comments from the repo's PRs, a
//! coordinator distills them into short rules, and each version of the
//! rules waits for the user's approval before it replaces the last.
//!
//! - The comments are evidence, kept whole; the rules are what agents get.
//! - A coordinator reads the comments not distilled yet, oldest first, and
//!   proposes the whole new set of rules with the newest comment it read.
//!   Approving it replaces the rules and marks those comments distilled;
//!   rejecting it leaves them to distill again.
//! - A rule keeps its id across versions; Tinto gives one (`V-3`, `T-12`)
//!   to a rule proposed without one.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::decisions::norm;
use super::model::{DeliveryGuideKind, DeliveryGuidePerson};
use super::prs::github_repo;
use super::service::DeliveryService;
use super::tasks::{self, plain_path, Place};
use super::{now_ms, DeliveryError};

/// Pages of the repo's latest review comments a collection reads, a
/// hundred each, newest first.
const PAGES: u32 = 5;
const PAGE_SIZE: usize = 100;
/// The comments a coordinator gets at once, oldest first.
const READ_LIMIT: usize = 150;
/// A comment's text as a coordinator reads it, at most.
const BODY_CHARS: usize = 1500;
/// The end of a comment's diff a coordinator reads: the lines it is on.
const HUNK_LINES: usize = 12;

/// A guide's review comment, as GitHub gave it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuideComment {
    pub id: u64,
    pub login: String,
    pub body: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub diff_hunk: String,
    pub url: String,
    pub pr_url: String,
    pub created_at: String,
}

/// One thing a guide keeps asking for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuideRule {
    /// `V-3`, `T-12`: kept across versions.
    #[serde(default)]
    pub id: String,
    pub kind: DeliveryGuideKind,
    /// The rule, in one line.
    pub rule: String,
    /// Folders or file patterns it applies to; everywhere when empty.
    #[serde(default)]
    pub applies_to: Vec<String>,
    /// Stages it applies to (`implementation`, `review`, `qa`); all when
    /// empty.
    #[serde(default)]
    pub stages: Vec<String>,
    #[serde(default)]
    pub why: String,
    /// A short don't and do, from a real comment.
    #[serde(default)]
    pub example: String,
    /// The comments it comes from, by URL.
    #[serde(default)]
    pub sources: Vec<String>,
    /// How many comments asked for it.
    #[serde(default)]
    pub seen: u32,
}

/// A new version of the rules, waiting for the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuideProposal {
    pub rules: Vec<DeliveryGuideRule>,
    /// The newest comment it took in.
    pub through: u64,
    pub note: String,
    pub requested_by: String,
    pub requested_at_ms: u64,
}

/// A repo's guide: its rules, the comments they come from and the next
/// version, if one waits.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuide {
    pub rules: Vec<DeliveryGuideRule>,
    pub comments: Vec<DeliveryGuideComment>,
    /// The newest comment the approved rules took in.
    pub distilled_through: u64,
    pub proposal: Option<DeliveryGuideProposal>,
    pub collected_at_ms: Option<u64>,
}

/// The guide as Tinto shows it: its comments counted, not listed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryGuideView {
    pub people: Vec<DeliveryGuidePerson>,
    pub rules: Vec<DeliveryGuideRule>,
    pub proposal: Option<DeliveryGuideProposal>,
    pub comments: usize,
    /// Comments not distilled yet.
    pub pending: usize,
    pub collected_at_ms: Option<u64>,
}

fn invalid(message: impl Into<String>) -> DeliveryError {
    DeliveryError::new("invalid_guide", message)
}

fn comment_from(comment: &Value) -> Option<DeliveryGuideComment> {
    let text = |key: &str| comment[key].as_str().unwrap_or_default().to_string();
    Some(DeliveryGuideComment {
        id: comment["id"].as_u64()?,
        login: comment["user"]["login"].as_str()?.to_string(),
        body: text("body"),
        path: comment["path"].as_str().map(str::to_string),
        line: comment["line"]
            .as_u64()
            .or_else(|| comment["original_line"].as_u64()),
        diff_hunk: text("diff_hunk"),
        url: text("html_url"),
        pr_url: comment["_links"]["pull_request"]["href"]
            .as_str()
            .map(|api| {
                api.replace("api.github.com/repos/", "github.com/")
                    .replace("/pulls/", "/pull/")
            })
            .unwrap_or_default(),
        created_at: text("created_at"),
    })
}

/// The first `limit` characters of `text`, with an ellipsis when cut.
fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

/// The rules a job of `role` follows, one line each, as its instructions
/// state them: a rule without stages holds for every stage.
pub fn rule_notes(rules: &[DeliveryGuideRule], role: &str) -> Vec<String> {
    rules
        .iter()
        .filter(|rule| rule.stages.is_empty() || rule.stages.iter().any(|stage| stage == role))
        .map(|rule| {
            let mut note = format!("{}: {}", rule.id, rule.rule.trim());
            if !rule.applies_to.is_empty() {
                note.push_str(&format!(" (in {})", rule.applies_to.join(", ")));
            }
            for (label, text) in [("Why", &rule.why), ("Example", &rule.example)] {
                if !text.trim().is_empty() {
                    note.push_str(&format!(" {label}: {}", text.trim()));
                }
            }
            note
        })
        .collect()
}

fn prefix(kind: DeliveryGuideKind) -> &'static str {
    match kind {
        DeliveryGuideKind::Visual => "V",
        DeliveryGuideKind::Technical => "T",
    }
}

/// Gives each rule without a known id the next one of its kind.
fn number_rules(rules: &mut [DeliveryGuideRule], current: &[DeliveryGuideRule]) {
    let number = |id: &str| id.split_once('-').and_then(|(_, n)| n.parse::<u32>().ok());
    for kind in [DeliveryGuideKind::Visual, DeliveryGuideKind::Technical] {
        let tag = prefix(kind);
        let valid = |id: &str| id.starts_with(&format!("{tag}-")) && number(id).is_some();
        let mut next = current
            .iter()
            .chain(rules.iter())
            .filter(|rule| rule.kind == kind && valid(&rule.id))
            .filter_map(|rule| number(&rule.id))
            .max()
            .unwrap_or(0);
        let mut seen = Vec::new();
        for rule in rules.iter_mut().filter(|rule| rule.kind == kind) {
            if !valid(&rule.id) || seen.contains(&rule.id) {
                next += 1;
                rule.id = format!("{tag}-{next}");
            }
            seen.push(rule.id.clone());
        }
    }
}

impl DeliveryService {
    pub fn guide_view(&self, repo: &Path) -> Result<DeliveryGuideView, DeliveryError> {
        let store = self.store()?;
        let people = store.repo_settings(repo)?.guides;
        let guide = store.guide(repo)?;
        Ok(DeliveryGuideView {
            people,
            pending: guide
                .comments
                .iter()
                .filter(|comment| comment.id > guide.distilled_through)
                .count(),
            comments: guide.comments.len(),
            rules: guide.rules,
            proposal: guide.proposal,
            collected_at_ms: guide.collected_at_ms,
        })
    }

    /// The people who can review the repo's PRs on GitHub, to choose its
    /// guides from.
    pub fn guide_candidates(
        &self,
        repo: &Path,
        distro: Option<&str>,
    ) -> Result<Vec<String>, DeliveryError> {
        let mut logins = self.github().assignees(&repo_on_github(repo, distro)?)?;
        logins.sort_by_key(|login| login.to_ascii_lowercase());
        Ok(logins)
    }

    /// Reads the latest review comments of the repo's PRs and keeps the
    /// guides' new ones.
    pub fn collect_guide_comments(
        &self,
        repo: &Path,
        distro: Option<&str>,
    ) -> Result<DeliveryGuideView, DeliveryError> {
        let people = self.store()?.repo_settings(repo)?.guides;
        if people.is_empty() {
            return Err(invalid("elige antes quién es guía de este repositorio"));
        }
        let github_repo = repo_on_github(repo, distro)?;
        let github = self.github();
        let mut found = Vec::new();
        for page in 1..=PAGES {
            let listed = github.repo_review_comments(&github_repo, page)?;
            let items = listed.as_array().map(Vec::as_slice).unwrap_or_default();
            found.extend(items.iter().filter_map(comment_from).filter(|comment| {
                people
                    .iter()
                    .any(|person| person.login.eq_ignore_ascii_case(&comment.login))
            }));
            if items.len() < PAGE_SIZE {
                break;
            }
        }
        {
            let store = self.store()?;
            let mut guide = store.guide(repo)?;
            for comment in found {
                if !guide.comments.iter().any(|known| known.id == comment.id) {
                    guide.comments.push(comment);
                }
            }
            guide.comments.sort_by_key(|comment| comment.id);
            guide.collected_at_ms = Some(now_ms());
            store.put_guide(repo, &guide)?;
        }
        self.guide_view(repo)
    }

    /// Asks the coordinators of the repo's batches to distill the comments
    /// not distilled yet; says how many batches were told.
    pub fn request_guide(&self, repo: &Path) -> Result<usize, DeliveryError> {
        let store = self.store()?;
        let guide = store.guide(repo)?;
        let pending = guide
            .comments
            .iter()
            .filter(|comment| comment.id > guide.distilled_through)
            .count();
        if pending == 0 {
            return Err(invalid(
                "no hay comentarios nuevos de las guías: léelos antes de GitHub",
            ));
        }
        let key = norm(&plain_path(repo));
        let runs: Vec<_> = store
            .runs()?
            .into_iter()
            .filter(|run| {
                run.status == "active"
                    && norm(&plain_path(&run.repo)) == key
                    && run.owner.as_deref().is_some_and(|owner| owner != "user")
            })
            .collect();
        if runs.is_empty() {
            return Err(invalid(
                "ningún lote de este repositorio tiene coordinador: inicia uno para destilar las guías",
            ));
        }
        for run in &runs {
            store.record_event(
                now_ms(),
                "guides_requested",
                None,
                None,
                &format!("{} {pending}", run.id),
            )?;
        }
        Ok(runs.len())
    }

    /// What a coordinator distills: the guides, the current rules and the
    /// comments not distilled yet, oldest first, with the newest one in
    /// `through` to pass back.
    pub fn read_guide_comments(&self, run_id: &str) -> Result<Value, DeliveryError> {
        let run = self.run(run_id)?;
        let store = self.store()?;
        let people = store.repo_settings(&run.repo)?.guides;
        let guide = store.guide(&run.repo)?;
        let pending: Vec<&DeliveryGuideComment> = guide
            .comments
            .iter()
            .filter(|comment| comment.id > guide.distilled_through)
            .collect();
        let shown = &pending[..pending.len().min(READ_LIMIT)];
        let kinds = |login: &str| {
            people
                .iter()
                .find(|person| person.login.eq_ignore_ascii_case(login))
                .map(|person| person.kinds.clone())
                .unwrap_or_default()
        };
        Ok(json!({
            "guides": people,
            "rules": guide.rules,
            "comments": shown.iter().map(|comment| json!({
                "id": comment.id,
                "by": comment.login,
                "kinds": kinds(&comment.login),
                "path": comment.path,
                "line": comment.line,
                "text": cut(&comment.body, BODY_CHARS),
                "diff": last_lines(&comment.diff_hunk, HUNK_LINES),
                "url": comment.url,
            })).collect::<Vec<_>>(),
            "through": shown.last().map_or(guide.distilled_through, |comment| comment.id),
            "more": pending.len() - shown.len(),
        }))
    }

    /// A coordinator's new version of the rules, whole, for the user to
    /// approve; it replaces one still waiting.
    pub fn propose_guide(
        &self,
        run_id: &str,
        owner: &str,
        mut rules: Vec<DeliveryGuideRule>,
        through: u64,
        note: &str,
    ) -> Result<DeliveryGuideProposal, DeliveryError> {
        let run = self.run(run_id)?;
        if rules.iter().any(|rule| rule.rule.trim().is_empty()) {
            return Err(invalid("cada regla necesita su texto en `rule`"));
        }
        let store = self.store()?;
        let mut guide = store.guide(&run.repo)?;
        let newest = guide.comments.last().map_or(0, |comment| comment.id);
        if through < guide.distilled_through || through > newest {
            return Err(invalid(format!(
                "`through` es el último comentario que leíste con read_guide_comments (entre {} y {newest})",
                guide.distilled_through
            )));
        }
        number_rules(&mut rules, &guide.rules);
        let proposal = DeliveryGuideProposal {
            rules,
            through,
            note: note.trim().to_string(),
            requested_by: owner.to_string(),
            requested_at_ms: now_ms(),
        };
        guide.proposal = Some(proposal.clone());
        store.put_guide(&run.repo, &guide)?;
        store.record_event(
            now_ms(),
            "guide_proposed",
            None,
            None,
            &plain_path(&run.repo),
        )?;
        Ok(proposal)
    }

    /// The user approves the proposed rules, which replace the current
    /// ones, or rejects them.
    pub fn decide_guide(
        &self,
        repo: &Path,
        approve: bool,
    ) -> Result<DeliveryGuideView, DeliveryError> {
        {
            let store = self.store()?;
            let mut guide = store.guide(repo)?;
            let proposal = guide
                .proposal
                .take()
                .ok_or_else(|| invalid("no hay reglas nuevas que decidir"))?;
            if approve {
                guide.rules = proposal.rules;
                guide.distilled_through = proposal.through;
            }
            store.put_guide(repo, &guide)?;
            store.record_event(
                now_ms(),
                if approve {
                    "guide_approved"
                } else {
                    "guide_rejected"
                },
                None,
                None,
                &plain_path(repo),
            )?;
        }
        self.guide_view(repo)
    }
}

/// The repo's `owner/name` on GitHub, from its `origin`.
fn repo_on_github(repo: &Path, distro: Option<&str>) -> Result<String, DeliveryError> {
    tasks::origin_url(Place::of(distro), repo)
        .as_deref()
        .and_then(github_repo)
        .ok_or_else(|| {
            DeliveryError::new("github_unavailable", "el remoto origin no está en GitHub")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, kind: DeliveryGuideKind) -> DeliveryGuideRule {
        DeliveryGuideRule {
            id: id.into(),
            kind,
            rule: "x".into(),
            applies_to: Vec::new(),
            stages: Vec::new(),
            why: String::new(),
            example: String::new(),
            sources: Vec::new(),
            seen: 1,
        }
    }

    #[test]
    fn a_rule_keeps_its_id_and_a_new_one_gets_the_next_of_its_kind() {
        use DeliveryGuideKind::{Technical, Visual};
        let current = [rule("V-2", Visual), rule("T-1", Technical)];
        let mut proposed = [
            rule("V-2", Visual),
            rule("", Visual),
            rule("T-9x", Technical),
            rule("V-2", Visual),
        ];
        number_rules(&mut proposed, &current);
        let ids: Vec<&str> = proposed.iter().map(|rule| rule.id.as_str()).collect();
        assert_eq!(ids, ["V-2", "V-3", "T-2", "V-4"]);
    }
}
