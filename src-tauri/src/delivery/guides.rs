//! Guides: reviewers the user marks as the repo's visual or technical
//! guide. Tinto collects what they write on the repo's PRs (comments on
//! lines, review summaries and the PR conversation) on its own while it
//! follows PRs, asks a coordinator to distill the new ones into short rules,
//! and each version of the rules waits for the user's approval before it
//! replaces the last.
//!
//! - The comments are evidence, kept whole; the rules are what agents get.
//! - Each comment gets Tinto's own number in the order it was collected:
//!   GitHub numbers each kind of comment apart, so its ids say nothing of
//!   which came first.
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
/// How often Tinto collects a repo's guide comments on its own.
const COLLECT_EVERY: std::time::Duration = std::time::Duration::from_secs(30 * 60);
/// The comments a coordinator gets at once, oldest first.
const READ_LIMIT: usize = 150;
/// A comment's text as a coordinator reads it, at most.
const BODY_CHARS: usize = 1500;
/// The end of a comment's diff a coordinator reads: the lines it is on.
const HUNK_LINES: usize = 12;

/// A guide's comment, as GitHub gave it: on a line, a review's summary or
/// in the PR's conversation (these two without `path`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryGuideComment {
    /// Tinto's number for it, in the order collected: what `through` and
    /// `distilled_through` count.
    #[serde(default)]
    pub seq: u64,
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
    /// The newest comment it took in, by `seq`.
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
    /// The newest comment the approved rules took in, by `seq`.
    pub distilled_through: u64,
    pub proposal: Option<DeliveryGuideProposal>,
    pub collected_at_ms: Option<u64>,
}

impl DeliveryGuide {
    /// A guide saved before comments had `seq`: numbered in the order of
    /// their ids, which then were all of one kind, and its marks moved to
    /// those numbers.
    pub(super) fn upgraded(mut self) -> Self {
        if self.comments.iter().all(|comment| comment.seq > 0) {
            return self;
        }
        self.comments.sort_by_key(|comment| comment.id);
        for (index, comment) in self.comments.iter_mut().enumerate() {
            comment.seq = index as u64 + 1;
        }
        let renumber = |id: u64, comments: &[DeliveryGuideComment]| {
            comments
                .iter()
                .filter(|comment| comment.id <= id)
                .map(|comment| comment.seq)
                .max()
                .unwrap_or(0)
        };
        self.distilled_through = renumber(self.distilled_through, &self.comments);
        if let Some(proposal) = &mut self.proposal {
            proposal.through = renumber(proposal.through, &self.comments);
        }
        self
    }
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
    // A review approved without words says nothing to distill.
    if text("body").trim().is_empty() {
        return None;
    }
    let url = text("html_url");
    Some(DeliveryGuideComment {
        seq: 0,
        id: comment["id"].as_u64()?,
        login: comment["user"]["login"].as_str()?.to_string(),
        body: text("body"),
        path: comment["path"].as_str().map(str::to_string),
        line: comment["line"]
            .as_u64()
            .or_else(|| comment["original_line"].as_u64()),
        diff_hunk: text("diff_hunk"),
        pr_url: comment["_links"]["pull_request"]["href"]
            .as_str()
            .map(|api| {
                api.replace("api.github.com/repos/", "github.com/")
                    .replace("/pulls/", "/pull/")
            })
            // A conversation comment or a review: its PR is in its URL.
            .unwrap_or_else(|| url.split('#').next().unwrap_or_default().to_string()),
        url,
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
                .filter(|comment| comment.seq > guide.distilled_through)
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

    /// Reads what the guides wrote lately on the repo's PRs and keeps the
    /// new comments.
    pub fn collect_guide_comments(
        &self,
        repo: &Path,
        distro: Option<&str>,
    ) -> Result<DeliveryGuideView, DeliveryError> {
        if self.store()?.repo_settings(repo)?.guides.is_empty() {
            return Err(invalid("elige antes quién es guía de este repositorio"));
        }
        self.collect_new_guide_comments(repo, distro)?;
        self.guide_view(repo)
    }

    /// Collects the guides' comments on lines, their review summaries and
    /// what they wrote in the PRs' conversation; returns how many are new.
    fn collect_new_guide_comments(
        &self,
        repo: &Path,
        distro: Option<&str>,
    ) -> Result<usize, DeliveryError> {
        let people = self.store()?.repo_settings(repo)?.guides;
        let github_repo = repo_on_github(repo, distro)?;
        let github = self.github();
        let mut listed = Vec::new();
        for conversation in [false, true] {
            for page in 1..=PAGES {
                let items = match conversation {
                    false => github.repo_review_comments(&github_repo, page)?,
                    true => github.repo_conversation_comments(&github_repo, page)?,
                };
                let items = items.as_array().cloned().unwrap_or_default();
                let last = items.len() < PAGE_SIZE;
                listed.extend(items);
                if last {
                    break;
                }
            }
        }
        listed.extend(
            github
                .repo_review_bodies(&github_repo)?
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        let mut found: Vec<DeliveryGuideComment> = listed
            .iter()
            .filter_map(comment_from)
            // Conversation comments of issues are not about a PR's code.
            .filter(|comment| comment.pr_url.contains("/pull/"))
            .filter(|comment| {
                people
                    .iter()
                    .any(|person| person.login.eq_ignore_ascii_case(&comment.login))
            })
            .collect();
        found.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let store = self.store()?;
        let mut guide = store.guide(repo)?;
        let mut next = guide
            .comments
            .iter()
            .map(|comment| comment.seq)
            .max()
            .unwrap_or(0);
        let mut new = 0;
        for mut comment in found {
            if guide.comments.iter().any(|known| known.url == comment.url) {
                continue;
            }
            next += 1;
            comment.seq = next;
            guide.comments.push(comment);
            new += 1;
        }
        guide.collected_at_ms = Some(now_ms());
        store.put_guide(repo, &guide)?;
        Ok(new)
    }

    /// While Tinto follows PRs: in each repo of the tasks with guides,
    /// collects their comments at most every [`COLLECT_EVERY`], and asks the
    /// repo's coordinators to distill when new ones came and no proposal
    /// waits for the user.
    pub(super) fn watch_guides(&self) -> Result<(), DeliveryError> {
        let mut repos: Vec<(std::path::PathBuf, Option<String>)> = Vec::new();
        for task in self.store()?.tasks()? {
            if !repos.iter().any(|(repo, _)| *repo == task.repo) {
                repos.push((task.repo.clone(), task.distro.clone()));
            }
        }
        for (repo, distro) in repos {
            let (people, guide) = {
                let store = self.store()?;
                (store.repo_settings(&repo)?.guides, store.guide(&repo)?)
            };
            let due = guide
                .collected_at_ms
                .is_none_or(|at| now_ms().saturating_sub(at) >= COLLECT_EVERY.as_millis() as u64);
            if people.is_empty() || !due {
                continue;
            }
            let new = self.collect_new_guide_comments(&repo, distro.as_deref())?;
            if new > 0 && guide.proposal.is_none() {
                // Without a coordinator the comments wait, counted on the
                // Guías page, for the next batch.
                let _ = self.request_guide(&repo);
            }
            self.notify();
        }
        Ok(())
    }

    /// Asks the coordinators of the repo's batches to distill the comments
    /// not distilled yet; says how many batches were told.
    pub fn request_guide(&self, repo: &Path) -> Result<usize, DeliveryError> {
        let store = self.store()?;
        let guide = store.guide(repo)?;
        let pending = guide
            .comments
            .iter()
            .filter(|comment| comment.seq > guide.distilled_through)
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
            .filter(|comment| comment.seq > guide.distilled_through)
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
                "id": comment.seq,
                "by": comment.login,
                "kinds": kinds(&comment.login),
                "path": comment.path,
                "line": comment.line,
                "text": cut(&comment.body, BODY_CHARS),
                "diff": last_lines(&comment.diff_hunk, HUNK_LINES),
                "url": comment.url,
            })).collect::<Vec<_>>(),
            "through": shown.last().map_or(guide.distilled_through, |comment| comment.seq),
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
        // A stage named otherwise would match no job: the rule reaches none.
        let stages = super::phase::STAGES;
        if let Some(stage) = rules
            .iter()
            .flat_map(|rule| &rule.stages)
            .find(|stage| !stages.contains(&stage.as_str()))
        {
            return Err(invalid(format!(
                "la etapa «{stage}» no existe: `stages` toma {}",
                stages.join(", ")
            )));
        }
        let store = self.store()?;
        let mut guide = store.guide(&run.repo)?;
        let newest = guide
            .comments
            .iter()
            .map(|comment| comment.seq)
            .max()
            .unwrap_or(0);
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
        drop(store);
        // The Guías page shows it as soon as it is proposed.
        self.notify();
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

    #[test]
    fn a_guide_saved_by_github_ids_is_renumbered_with_its_marks() {
        let comment = |id: u64| DeliveryGuideComment {
            seq: 0,
            id,
            login: "ana".into(),
            body: "x".into(),
            path: None,
            line: None,
            diff_hunk: String::new(),
            url: format!("https://github.com/o/r/pull/1#discussion_r{id}"),
            pr_url: String::new(),
            created_at: String::new(),
        };
        let guide = DeliveryGuide {
            comments: vec![comment(300), comment(100), comment(200)],
            distilled_through: 200,
            proposal: Some(DeliveryGuideProposal {
                rules: Vec::new(),
                through: 300,
                note: String::new(),
                requested_by: "coord".into(),
                requested_at_ms: 1,
            }),
            ..Default::default()
        }
        .upgraded();
        let numbered: Vec<(u64, u64)> = guide.comments.iter().map(|c| (c.id, c.seq)).collect();
        assert_eq!(numbered, [(100, 1), (200, 2), (300, 3)]);
        assert_eq!(guide.distilled_through, 2);
        assert_eq!(guide.proposal.unwrap().through, 3);
    }

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
