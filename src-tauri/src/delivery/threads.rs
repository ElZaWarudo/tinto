//! The code a decision is about, as Tinto reads it: a PR review comment
//! with the diff it is anchored to, or lines of the task's worktree. The
//! coordinator only points at them; what the user sees comes from GitHub
//! and git, not from the coordinator's summary.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::model::{DeliveryCodeLine, DeliveryCodeThread, DeliveryTask, DeliveryThreadComment};
use super::service::DeliveryService;
use super::tasks::{self, Place};

/// What a coordinator points a decision at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewReference {
    /// A review comment on GitHub (`…/pull/7#discussion_r123`).
    pub comment_url: Option<String>,
    /// Lines of the task's worktree, relative to it.
    pub path: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    /// Why it matters, in the coordinator's words.
    pub note: String,
}

/// Lines of a review comment's diff shown above it, as GitHub does.
const COMMENT_LINES: usize = 6;
/// Most lines one code reference shows.
const MAX_CODE_LINES: u32 = 40;
pub const MAX_REFERENCES: usize = 5;

/// `owner/name`, the PR number and the comment id of a review comment URL.
pub fn parse_comment_url(url: &str) -> Option<(String, u64, u64)> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let (path, fragment) = rest.split_once('#')?;
    let parts: Vec<&str> = path.split('/').collect();
    let [owner, name, "pull", number, ..] = parts.as_slice() else {
        return None;
    };
    let id = fragment
        .strip_prefix("discussion_r")
        .or_else(|| fragment.strip_prefix('r'))?
        .parse()
        .ok()?;
    Some((format!("{owner}/{name}"), number.parse().ok()?, id))
}

/// The last lines of a review comment's `diff_hunk`, the commented one
/// (its last line) marked.
pub fn hunk_lines(diff_hunk: &str, keep: usize) -> Vec<DeliveryCodeLine> {
    let mut lines = diff_hunk.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    // `@@ -old,count +new,count @@`
    let start = |sign: char| -> u32 {
        header
            .split_whitespace()
            .find_map(|part| part.strip_prefix(sign))
            .and_then(|range| range.split(',').next()?.parse().ok())
            .unwrap_or(1)
    };
    let (mut old, mut new) = (start('-'), start('+'));
    let mut parsed = Vec::new();
    for line in lines {
        let (kind, text) = match line.chars().next() {
            Some('+') => ("add", &line[1..]),
            Some('-') => ("del", &line[1..]),
            Some(' ') => ("ctx", &line[1..]),
            Some('\\') => continue,
            _ => ("ctx", line),
        };
        parsed.push(DeliveryCodeLine {
            kind: kind.to_string(),
            old: (kind != "add").then_some(old),
            new: (kind != "del").then_some(new),
            text: text.to_string(),
            target: false,
        });
        if kind != "add" {
            old += 1;
        }
        if kind != "del" {
            new += 1;
        }
    }
    if let Some(last) = parsed.last_mut() {
        last.target = true;
    }
    let skip = parsed.len().saturating_sub(keep);
    parsed.split_off(skip)
}

/// The lines `@@ +start,count @@` of `git diff -U0` adds, in the new file.
pub fn added_lines(diff: &str) -> Vec<u32> {
    let mut added = Vec::new();
    for header in diff.lines().filter(|line| line.starts_with("@@")) {
        let Some(range) = header
            .split_whitespace()
            .find_map(|part| part.strip_prefix('+'))
        else {
            continue;
        };
        let mut parts = range.split(',');
        let start: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let count: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
        added.extend(start..start + count);
    }
    added
}

fn failed(path: &str, note: &str, kind: &str, why: String) -> DeliveryCodeThread {
    DeliveryCodeThread {
        kind: kind.to_string(),
        path: path.to_string(),
        line: None,
        url: None,
        outdated: false,
        lines: Vec::new(),
        comment: None,
        note: note.to_string(),
        error: Some(why),
    }
}

impl DeliveryService {
    /// Reads every reference of a decision. One that cannot be read is kept
    /// with why, never filled in.
    pub(super) fn resolve_references(
        &self,
        task: &DeliveryTask,
        references: &[NewReference],
    ) -> Vec<DeliveryCodeThread> {
        // One read of a PR's comments serves all its references.
        let mut comments: HashMap<(String, u64), Result<Vec<Value>, String>> = HashMap::new();
        references
            .iter()
            .map(|reference| {
                let note = reference.note.trim();
                match reference.comment_url.as_deref() {
                    Some(url) => self.comment_thread(url, note, &mut comments),
                    None => self.code_thread(task, reference, note),
                }
            })
            .collect()
    }

    fn comment_thread(
        &self,
        url: &str,
        note: &str,
        cache: &mut HashMap<(String, u64), Result<Vec<Value>, String>>,
    ) -> DeliveryCodeThread {
        let Some((repo, number, id)) = parse_comment_url(url) else {
            return failed(
                "",
                note,
                "pr_comment",
                format!("no es la URL de un comentario de revisión de GitHub: {url}"),
            );
        };
        let listed = cache
            .entry((repo.clone(), number))
            .or_insert_with(|| {
                self.github_client()
                    .pr_review_comments(&repo, number)
                    .map(|value| value.as_array().cloned().unwrap_or_default())
                    .map_err(|error| error.message)
            })
            .clone();
        let comments = match listed {
            Ok(comments) => comments,
            Err(why) => {
                return failed(
                    "",
                    note,
                    "pr_comment",
                    format!("GitHub no respondió: {why}"),
                )
            }
        };
        let Some(comment) = comments
            .iter()
            .find(|comment| comment["id"].as_u64() == Some(id))
        else {
            return failed(
                "",
                note,
                "pr_comment",
                "el comentario ya no existe en GitHub".to_string(),
            );
        };
        let replies = comments
            .iter()
            .filter(|reply| reply["in_reply_to_id"].as_u64() == Some(id))
            .count() as u32;
        let text = |key: &str| comment[key].as_str().unwrap_or_default().to_string();
        DeliveryCodeThread {
            kind: "pr_comment".to_string(),
            path: text("path"),
            line: comment["line"]
                .as_u64()
                .or_else(|| comment["original_line"].as_u64())
                .map(|line| line as u32),
            url: Some(text("html_url")).filter(|url| !url.is_empty()),
            // GitHub drops the position once the commented line changed.
            outdated: comment["position"].is_null(),
            lines: hunk_lines(
                comment["diff_hunk"].as_str().unwrap_or_default(),
                COMMENT_LINES,
            ),
            comment: Some(DeliveryThreadComment {
                author: comment["user"]["login"]
                    .as_str()
                    .unwrap_or("alguien")
                    .to_string(),
                body: text("body"),
                created_at: text("created_at"),
                replies,
            }),
            note: note.to_string(),
            error: None,
        }
    }

    fn code_thread(
        &self,
        task: &DeliveryTask,
        reference: &NewReference,
        note: &str,
    ) -> DeliveryCodeThread {
        let path = reference
            .path
            .as_deref()
            .unwrap_or_default()
            .trim()
            .replace('\\', "/");
        let fail = |why: &str| failed(&path, note, "code", why.to_string());
        let relative = Path::new(&path);
        if path.is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return fail("la ruta tiene que ser relativa al worktree de la tarea");
        }
        let start = reference.start_line.unwrap_or(1).max(1);
        let end = reference
            .end_line
            .unwrap_or(start)
            .max(start)
            .min(start + MAX_CODE_LINES - 1);
        let content = match std::fs::read_to_string(worktree_file(task, &path)) {
            Ok(content) => content,
            Err(_) => return fail("el archivo no existe en el worktree de la tarea"),
        };
        let place = Place::of(task.distro.as_deref());
        // What the task changed shows as added, against the task's base.
        let added = tasks::git(
            place,
            &task.worktree,
            &["diff", "-U0", &task.base_commit, "--", &path],
        )
        .map(|diff| added_lines(&diff))
        .unwrap_or_default();
        let lines: Vec<DeliveryCodeLine> = content
            .lines()
            .enumerate()
            .map(|(index, text)| (index as u32 + 1, text))
            .filter(|(number, _)| (start..=end).contains(number))
            .map(|(number, text)| {
                let kind = if added.contains(&number) {
                    "add"
                } else {
                    "ctx"
                };
                DeliveryCodeLine {
                    kind: kind.to_string(),
                    old: None,
                    new: Some(number),
                    text: text.to_string(),
                    target: false,
                }
            })
            .collect();
        if lines.is_empty() {
            return fail("esas líneas no existen en el archivo");
        }
        DeliveryCodeThread {
            kind: "code".to_string(),
            line: Some(start),
            path,
            url: None,
            outdated: false,
            lines,
            comment: None,
            note: note.to_string(),
            error: None,
        }
    }
}

/// A file of the task's worktree, as Windows reaches it.
fn worktree_file(task: &DeliveryTask, path: &str) -> PathBuf {
    match task.distro.as_deref() {
        Some(distro) => PathBuf::from(format!(
            r"\\wsl.localhost\{distro}{}/{path}",
            task.worktree.to_string_lossy().trim_end_matches('/')
        )),
        None => task.worktree.join(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_comment_urls_name_the_repo_pr_and_comment() {
        assert_eq!(
            parse_comment_url("https://github.com/acme/agentos/pull/299#discussion_r2105"),
            Some(("acme/agentos".to_string(), 299, 2105))
        );
        assert_eq!(
            parse_comment_url("https://github.com/acme/agentos/pull/299/files#r77"),
            Some(("acme/agentos".to_string(), 299, 77))
        );
        assert_eq!(
            parse_comment_url("https://github.com/acme/agentos/pull/299"),
            None
        );
        assert_eq!(parse_comment_url("https://example.com/a/b/pull/1#r2"), None);
    }

    #[test]
    fn a_hunk_keeps_its_last_lines_with_their_numbers_and_marks_the_commented_one() {
        let hunk = "@@ -978,3 +1090,3 @@ export async function install(\n   const target = x;\n-  // Replace only a directory\n-  await installIntoDir(a);\n+  // Replace a directory\n+  await installIntoDir(b);";
        let lines = hunk_lines(hunk, 4);
        let shown: Vec<(&str, Option<u32>, Option<u32>, bool)> = lines
            .iter()
            .map(|line| (line.kind.as_str(), line.old, line.new, line.target))
            .collect();
        assert_eq!(
            shown,
            [
                ("del", Some(979), None, false),
                ("del", Some(980), None, false),
                ("add", None, Some(1091), false),
                ("add", None, Some(1092), true),
            ]
        );
        assert_eq!(lines[3].text, "  await installIntoDir(b);");
    }

    #[test]
    fn added_lines_come_from_the_new_side_of_each_hunk() {
        let diff = "diff --git a/x b/x\n@@ -3 +3,2 @@\n-a\n+b\n+c\n@@ -10,0 +11 @@\n+d\n";
        assert_eq!(added_lines(diff), [3, 4, 11]);
    }
}
