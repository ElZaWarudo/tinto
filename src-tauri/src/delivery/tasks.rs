//! Task worktrees: a named branch in its own checkout, created from a
//! verified base. The main checkout's branch is never touched.
//!
//! Native repos run `git` directly; WSL repos run it inside their distro.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::DeliveryError;

/// Where a task's git commands run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    Native,
    Wsl(&'a str),
}

impl<'a> Place<'a> {
    pub fn of(distro: Option<&'a str>) -> Self {
        distro.map_or(Place::Native, Place::Wsl)
    }
}

/// The git commands that reach the remote: a remote that never answers
/// must not hold the task's git lock for good.
const NETWORK_COMMANDS: [&str; 3] = ["push", "fetch", "ls-remote"];
const NETWORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Runs `git -C <dir> <args>` and returns trimmed stdout.
pub fn git(place: Place<'_>, dir: &Path, args: &[&str]) -> Result<String, DeliveryError> {
    let failed = |error: std::io::Error| {
        DeliveryError::new("git_failed", format!("no se pudo ejecutar git: {error}"))
    };
    let mut command = git_command(place, dir, args);
    let output = match args.first() {
        Some(name) if NETWORK_COMMANDS.contains(name) => {
            super::prs::output_within(&mut command, NETWORK_TIMEOUT)
                .map_err(failed)?
                .ok_or_else(|| {
                    DeliveryError::new(
                        "git_failed",
                        format!(
                            "git {name}: el remoto no respondió en {} minutos",
                            NETWORK_TIMEOUT.as_secs() / 60
                        ),
                    )
                })?
        }
        _ => command.output().map_err(failed)?,
    };
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(DeliveryError::new(
            "git_failed",
            format!(
                "git {}: {}",
                args.first().copied().unwrap_or_default(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ))
    }
}

/// Like [`git`] but answers whether it succeeded, for probes.
fn git_ok(place: Place<'_>, dir: &Path, args: &[&str]) -> bool {
    git_command(place, dir, args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_command(place: Place<'_>, dir: &Path, args: &[&str]) -> Command {
    let mut command = match place {
        Place::Native => {
            let mut command = Command::new(crate::git::git_program());
            command.arg("-C").arg(plain_path(dir));
            command
        }
        Place::Wsl(distro) => {
            let mut command = Command::new("wsl.exe");
            command
                .args(["-d", distro, "--exec", "git", "-C"])
                .arg(plain_path(dir));
            command
        }
    };
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    crate::windows_process::hide_console(&mut command);
    command
}

/// Path text without the `\\?\` verbatim prefix, which git does not accept.
pub fn plain_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_string(),
        _ => text.into_owned(),
    }
}

/// Issue keys and labels become folder and branch names.
pub fn validate_key(key: &str) -> Result<String, DeliveryError> {
    let key = key.trim();
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !key.starts_with('.');
    if valid {
        Ok(key.to_string())
    } else {
        Err(DeliveryError::new(
            "invalid_key",
            "la clave solo admite letras, números, '-', '_' y '.' (máximo 64)",
        ))
    }
}

pub fn default_branch(key: &str) -> String {
    format!("delivery/{}", key.to_ascii_lowercase())
}

/// `<parent>/<repo name>-wt`, next to the repository.
pub fn default_worktree_root(repo: &Path) -> PathBuf {
    let repo = PathBuf::from(plain_path(repo));
    let name = repo
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    repo.parent()
        .map(|parent| parent.join(format!("{name}-wt")))
        .unwrap_or_else(|| PathBuf::from(format!("{name}-wt")))
}

/// For WSL repos the worktree path is a Linux path built with `/`.
pub fn join_place(place: Place<'_>, root: &Path, leaf: &str) -> PathBuf {
    match place {
        Place::Native => root.join(leaf),
        Place::Wsl(_) => PathBuf::from(format!(
            "{}/{leaf}",
            root.to_string_lossy().trim_end_matches('/')
        )),
    }
}

/// Resolves the base ref to a commit: the requested ref, the repo setting,
/// then `develop`, `main`, `master` and finally `HEAD`.
pub fn resolve_base(
    place: Place<'_>,
    repo: &Path,
    requested: Option<&str>,
    configured: Option<&str>,
) -> Result<(String, String), DeliveryError> {
    if let Some(requested) = requested.filter(|value| !value.trim().is_empty()) {
        return verify_commit(place, repo, requested.trim())
            .map(|commit| (requested.trim().to_string(), commit))
            .ok_or_else(|| {
                DeliveryError::new(
                    "base_not_found",
                    format!("la base {requested} no existe en el repositorio"),
                )
            });
    }
    let candidates = configured
        .into_iter()
        .chain(["develop", "main", "master", "HEAD"]);
    for candidate in candidates {
        if let Some(commit) = verify_commit(place, repo, candidate) {
            return Ok((candidate.to_string(), commit));
        }
    }
    Err(DeliveryError::new(
        "base_not_found",
        "el repositorio no tiene commits desde los que crear la tarea",
    ))
}

/// A local branch as a base is often behind its remote (AGOS: develop
/// 1,075 commits behind): after fetching it, the remote-tracking branch is
/// the base when the local one has nothing of its own. A remote ref, HEAD,
/// a branch without remote or one with its own commits stays as it is.
pub fn prefer_remote(place: Place<'_>, repo: &Path, base: (String, String)) -> (String, String) {
    let (name, commit) = &base;
    if name.starts_with("origin/")
        || name == "HEAD"
        || name.starts_with('-')
        || !git_ok(place, repo, &["remote", "get-url", "origin"])
    {
        return base;
    }
    let tracking = format!("refs/remotes/origin/{name}");
    // A fetch that does not answer in time leaves the ref as it was.
    let refspec = format!("+refs/heads/{name}:{tracking}");
    let mut fetch = git_command(place, repo, &["fetch", "--quiet", "origin", &refspec]);
    fetch.stdin(std::process::Stdio::null());
    let _ = super::prs::output_within(&mut fetch, std::time::Duration::from_secs(30));
    match verify_commit(place, repo, &tracking) {
        Some(remote)
            if git_ok(
                place,
                repo,
                &["merge-base", "--is-ancestor", commit, &remote],
            ) =>
        {
            (format!("origin/{name}"), remote)
        }
        _ => base,
    }
}

fn verify_commit(place: Place<'_>, repo: &Path, reference: &str) -> Option<String> {
    if reference.starts_with('-') {
        return None;
    }
    let spec = format!("{reference}^{{commit}}");
    git(place, repo, &["rev-parse", "--verify", "--quiet", &spec])
        .ok()
        .filter(|commit| !commit.is_empty())
}

/// Moves a worktree that has nothing of its own yet (no changes and no
/// commit past `base_commit`) up to `reference`, when that only adds
/// commits. Returns the new HEAD when it moved.
pub fn fast_forward_untouched(
    place: Place<'_>,
    worktree: &Path,
    base_commit: &str,
    reference: &str,
) -> Option<String> {
    let head = git(place, worktree, &["rev-parse", "HEAD"]).ok()?;
    let clean = git(place, worktree, &["status", "--porcelain"]).ok()?;
    let target = verify_commit(place, worktree, reference)?;
    if head != base_commit || !clean.trim().is_empty() || target == head {
        return None;
    }
    git_ok(place, worktree, &["merge", "--ff-only", "--quiet", &target]).then_some(target)
}

/// Creates the worktree on a new branch, or on the existing branch when one
/// with that name already exists (resuming a task). Returns the commit the
/// worktree starts from.
pub fn create_worktree(
    place: Place<'_>,
    repo: &Path,
    worktree: &Path,
    branch: &str,
    base_commit: &str,
) -> Result<String, DeliveryError> {
    if branch.starts_with('-') || !git_ok(place, repo, &["check-ref-format", "--branch", branch]) {
        return Err(DeliveryError::new(
            "invalid_branch",
            format!("{branch} no es un nombre de rama válido"),
        ));
    }
    if place == Place::Native && worktree.exists() {
        return Err(DeliveryError::new(
            "worktree_exists",
            format!("ya existe {}", worktree.display()),
        ));
    }
    let target = plain_path(worktree);
    let reference = format!("refs/heads/{branch}");
    if git_ok(
        place,
        repo,
        &["rev-parse", "--verify", "--quiet", &reference],
    ) {
        git(place, repo, &["worktree", "add", &target, branch])?;
    } else {
        git(
            place,
            repo,
            &["worktree", "add", "-b", branch, &target, base_commit],
        )?;
    }
    if place == Place::Native {
        link_worktree_relatively(worktree);
    }
    git(place, worktree, &["rev-parse", "HEAD"])
}

/// Rewrites the worktree's `.git` link as a relative path. Git writes it as
/// `C:/…`, which git inside WSL (where Claude Code runs for Windows repos)
/// cannot follow. Left as is when the two are on different drives.
fn link_worktree_relatively(worktree: &Path) {
    let link = Path::new(&plain_path(worktree)).join(".git");
    let Ok(text) = std::fs::read_to_string(&link) else {
        return;
    };
    let Some(gitdir) = text.trim().strip_prefix("gitdir:") else {
        return;
    };
    if let Some(relative) =
        relative_path(Path::new(&plain_path(worktree)), Path::new(gitdir.trim()))
    {
        let _ = std::fs::write(&link, format!("gitdir: {relative}\n"));
    }
}

/// `to` relative to the directory `from`, with `/` separators; `None` when
/// they share no root (another drive).
fn relative_path(from: &Path, to: &Path) -> Option<String> {
    let parts = |path: &Path| -> Vec<String> {
        path.components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .filter(|part| part != "\\" && part != "/")
            .collect()
    };
    let same = |a: &String, b: &String| {
        if cfg!(windows) {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    let (from, to) = (parts(from), parts(to));
    let common = from.iter().zip(&to).take_while(|(a, b)| same(a, b)).count();
    if common == 0 {
        return None;
    }
    let mut relative = vec!["..".to_string(); from.len() - common];
    relative.extend(to[common..].iter().cloned());
    Some(relative.join("/"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeState {
    pub dirty: bool,
    /// Commits on the branch that are not on its upstream (or, without one,
    /// not on the base commit).
    pub unpublished: u32,
    pub has_upstream: bool,
}

/// Whether `commit` is an ancestor of `of`.
pub fn is_ancestor(place: Place<'_>, dir: &Path, commit: &str, of: &str) -> bool {
    git_ok(place, dir, &["merge-base", "--is-ancestor", commit, of])
}

/// Lockfiles an install rewrites as a side effect: the only files Tinto
/// puts back after a setup or check command.
pub const LOCKFILES: [&str; 4] = [
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
];

/// Lockfiles the worktree has modified against HEAD, staged or not, as git
/// names them (unquoted).
pub fn changed_lockfiles(place: Place<'_>, worktree: &Path) -> Result<Vec<String>, DeliveryError> {
    Ok(git(
        place,
        worktree,
        &["diff", "--name-only", "-z", "--diff-filter=M", "HEAD"],
    )?
    .split('\0')
    .filter(|path| {
        let name = path.rsplit('/').next().unwrap_or(path);
        LOCKFILES.contains(&name)
    })
    .map(str::to_string)
    .collect())
}

pub fn worktree_state(
    place: Place<'_>,
    worktree: &Path,
    base_commit: &str,
) -> Result<WorktreeState, DeliveryError> {
    let dirty = !git(place, worktree, &["status", "--porcelain"])?.is_empty();
    let has_upstream = git_ok(
        place,
        worktree,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    );
    let range = if has_upstream {
        "@{u}..HEAD".to_string()
    } else {
        format!("{base_commit}..HEAD")
    };
    let unpublished = git(place, worktree, &["rev-list", "--count", &range])?
        .parse()
        .unwrap_or(0);
    Ok(WorktreeState {
        dirty,
        unpublished,
        has_upstream,
    })
}

/// Whether `worktree` is still a checkout git can use.
pub fn is_worktree(place: Place<'_>, worktree: &Path) -> bool {
    git_ok(place, worktree, &["rev-parse", "--is-inside-work-tree"])
}

/// Makes git forget worktrees whose folder is gone.
pub fn prune_worktrees(place: Place<'_>, repo: &Path) {
    let _ = git(place, repo, &["worktree", "prune"]);
}

/// Removes the checkout; the branch is kept.
pub fn remove_worktree(
    place: Place<'_>,
    repo: &Path,
    worktree: &Path,
    force: bool,
) -> Result<(), DeliveryError> {
    let target = plain_path(worktree);
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&target);
    git(place, repo, &args).map(|_| ())
}

/// Every file with uncommitted changes (modified, deleted or new), relative
/// to the worktree with `/`. A rename counts as a deletion and a new file,
/// so each side can go to the commit it belongs to.
/// Lines added plus removed in the worktree's changes to `paths`, new
/// files counted whole. Binary files count no lines.
pub fn changed_lines(place: Place<'_>, worktree: &Path, paths: &[String]) -> u64 {
    let sum = |numstat: &[u8]| -> u64 {
        String::from_utf8_lossy(numstat)
            .lines()
            .map(|line| {
                line.split('\t')
                    .take(2)
                    .filter_map(|count| count.parse::<u64>().ok())
                    .sum::<u64>()
            })
            .sum()
    };
    let mut args = vec!["diff", "--numstat", "HEAD", "--"];
    args.extend(paths.iter().map(String::as_str));
    let mut total = git_command(place, worktree, &args)
        .output()
        .map(|output| sum(&output.stdout))
        .unwrap_or(0);
    let mut untracked = vec!["ls-files", "--others", "--exclude-standard", "--"];
    untracked.extend(paths.iter().map(String::as_str));
    for path in git(place, worktree, &untracked).unwrap_or_default().lines() {
        // Exits 1 when the files differ, which they always do.
        let args = ["diff", "--no-index", "--numstat", "--", "/dev/null", path];
        total += git_command(place, worktree, &args)
            .output()
            .map(|output| sum(&output.stdout))
            .unwrap_or(0);
    }
    total
}

pub fn changed_files(place: Place<'_>, worktree: &Path) -> Result<Vec<String>, DeliveryError> {
    let args = [
        "-c",
        "status.renames=false",
        "status",
        "--porcelain",
        "-z",
        "--untracked-files=all",
    ];
    let output = git_command(place, worktree, &args)
        .output()
        .map_err(|error| {
            DeliveryError::new("git_failed", format!("no se pudo ejecutar git: {error}"))
        })?;
    if !output.status.success() {
        return Err(DeliveryError::new(
            "git_failed",
            format!(
                "git status: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    // Each entry is `XY <path>` and ends with NUL.
    Ok(String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter_map(|entry| entry.get(3..))
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn head(place: Place<'_>, worktree: &Path) -> Result<String, DeliveryError> {
    git(place, worktree, &["rev-parse", "HEAD"])
}

/// Leaves nothing staged; the files keep their changes.
pub fn unstage_all(place: Place<'_>, worktree: &Path) -> Result<(), DeliveryError> {
    git(place, worktree, &["reset", "--quiet"]).map(|_| ())
}

/// Commits exactly `paths`, as they are now, with exactly `message`.
/// Nothing else may be staged. Returns the new commit.
pub fn commit_paths(
    place: Place<'_>,
    worktree: &Path,
    message: &str,
    paths: &[String],
) -> Result<String, DeliveryError> {
    let mut add = vec!["add", "--all", "--"];
    add.extend(paths.iter().map(String::as_str));
    git(place, worktree, &add)?;
    git(place, worktree, &["commit", "--quiet", "-m", message])?;
    head(place, worktree)
}

/// The commit being merged while a merge is in progress (`MERGE_HEAD`).
pub fn merge_head(place: Place<'_>, worktree: &Path) -> Option<String> {
    git(
        place,
        worktree,
        &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"],
    )
    .ok()
    .filter(|commit| !commit.is_empty())
}

/// Files of the merge in progress that git left conflicted and that still
/// have conflict markers.
pub fn unresolved_conflicts(place: Place<'_>, worktree: &Path) -> Vec<String> {
    let conflicted =
        git(place, worktree, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    if conflicted.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["grep", "-l", "-E", "^(<<<<<<<|>>>>>>>)( |$)", "--"];
    args.extend(conflicted.lines());
    // Exits 1 when no file has markers.
    git(place, worktree, &args)
        .map(|files| files.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Concludes the merge in progress as it is in the worktree: every change,
/// with exactly `message`. The commit keeps both parents.
pub fn commit_merge(
    place: Place<'_>,
    worktree: &Path,
    message: &str,
) -> Result<String, DeliveryError> {
    git(place, worktree, &["add", "--all"])?;
    git(place, worktree, &["commit", "--quiet", "-m", message])?;
    head(place, worktree)
}

/// Where the task's base is: `origin/<base>` when the repo has an origin,
/// else the local branch. `fetch` brings the remote one up to date first.
pub fn base_tip(
    place: Place<'_>,
    worktree: &Path,
    base: &str,
    fetch: bool,
) -> Result<(String, String), DeliveryError> {
    let base = base.trim_start_matches("origin/");
    let has_origin = origin_url(place, worktree).is_some();
    if fetch && has_origin {
        let refspec = format!("+refs/heads/{base}:refs/remotes/origin/{base}");
        git(place, worktree, &["fetch", "--quiet", "origin", &refspec])?;
    }
    let name = if has_origin {
        format!("origin/{base}")
    } else {
        base.to_string()
    };
    let commit = verify_commit(place, worktree, &name).ok_or_else(|| {
        DeliveryError::new("base_not_found", format!("no se encontró la base {name}"))
    })?;
    Ok((name, commit))
}

/// The result of merging the base into a task's branch.
pub enum Merged {
    /// The merge commit, with the branch and the base as parents.
    Commit(String),
    /// The files git left in conflict; the merge stays in progress.
    Conflicts(Vec<String>),
}

/// Merges `commit` into the branch checked out in `worktree` with exactly
/// `message`, always as a merge commit.
pub fn merge_base(
    place: Place<'_>,
    worktree: &Path,
    commit: &str,
    message: &str,
) -> Result<Merged, DeliveryError> {
    let merged = git(
        place,
        worktree,
        &["merge", "--no-ff", "--quiet", "-m", message, commit],
    );
    match (merged, merge_head(place, worktree)) {
        (Ok(_), _) => head(place, worktree).map(Merged::Commit),
        (Err(_), Some(_)) => Ok(Merged::Conflicts(
            git(place, worktree, &["diff", "--name-only", "--diff-filter=U"])?
                .lines()
                .map(str::to_string)
                .collect(),
        )),
        (Err(error), None) => Err(error),
    }
}

/// Whether `commit` is still part of the branch checked out in `worktree`.
pub fn on_branch(place: Place<'_>, worktree: &Path, commit: &str) -> bool {
    git_ok(
        place,
        worktree,
        &["merge-base", "--is-ancestor", commit, "HEAD"],
    )
}

/// Whether some remote branch already has `commit`.
pub fn published(place: Place<'_>, worktree: &Path, commit: &str) -> bool {
    git(
        place,
        worktree,
        &["branch", "--remotes", "--contains", commit],
    )
    .is_ok_and(|branches| !branches.is_empty())
}

/// Moves the branch back to `commit`; what the later commits changed stays
/// in the worktree, uncommitted.
pub fn uncommit_to(place: Place<'_>, worktree: &Path, commit: &str) -> Result<(), DeliveryError> {
    git(place, worktree, &["reset", "--quiet", "--mixed", commit]).map(|_| ())
}

/// Subjects of the latest commits, newest first, to show the repo's
/// convention.
pub fn recent_subjects(place: Place<'_>, worktree: &Path, count: usize) -> Vec<String> {
    let limit = format!("-{count}");
    git(
        place,
        worktree,
        &["log", &limit, "--no-merges", "--format=%s"],
    )
    .map(|log| log.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

/// How PRs reach `base` lately, to propose the same: `merge` when most of
/// its latest PR merges are GitHub merge commits ("Merge pull request
/// #…"), else `squash` (the commits GitHub squashes end in "(#123)").
/// Reads `origin/<base>` when there is one, as the local branch may lag.
pub fn base_merge_method(place: Place<'_>, repo: &Path, base: &str) -> &'static str {
    let remote = format!("origin/{base}");
    let log = |reference: &str| {
        git(
            place,
            repo,
            &[
                "log",
                "--first-parent",
                "-50",
                "--format=%s",
                reference,
                "--",
            ],
        )
    };
    let subjects = log(&remote).or_else(|_| log(base)).unwrap_or_default();
    let squashed = |subject: &str| {
        subject
            .strip_suffix(')')
            .and_then(|rest| rest.rsplit_once("(#"))
            .is_some_and(|(_, number)| {
                !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
            })
    };
    let latest: Vec<bool> = subjects
        .lines()
        .filter_map(|subject| {
            if subject.starts_with("Merge pull request #") {
                Some(true)
            } else {
                squashed(subject).then_some(false)
            }
        })
        .take(5)
        .collect();
    let merges = latest.iter().filter(|merge| **merge).count();
    if merges * 2 > latest.len() {
        "merge"
    } else {
        "squash"
    }
}

/// `origin`'s URL as configured, before any `insteadOf` rewriting.
pub fn origin_url(place: Place<'_>, repo: &Path) -> Option<String> {
    git(place, repo, &["config", "--get", "remote.origin.url"])
        .ok()
        .filter(|url| !url.is_empty())
}

/// Where `origin`'s `branch` ends now, or `None` when it has no such
/// branch. Fails when the remote cannot be read.
pub fn remote_tip(
    place: Place<'_>,
    repo: &Path,
    branch: &str,
) -> Result<Option<String>, DeliveryError> {
    let head = format!("refs/heads/{branch}");
    let listed = git(place, repo, &["ls-remote", "origin", &head])?;
    Ok(listed.split_whitespace().next().map(str::to_string))
}

/// Whether the remote branch, now at `tip`, still has `commit`: it moved
/// ahead from it. The branch is fetched when `tip` is not known here.
pub fn remote_has(place: Place<'_>, repo: &Path, branch: &str, commit: &str, tip: &str) -> bool {
    let known = git_ok(
        place,
        repo,
        &["cat-file", "-e", &format!("{tip}^{{commit}}")],
    );
    if !known && git(place, repo, &["fetch", "-q", "origin", branch]).is_err() {
        return false;
    }
    git_ok(place, repo, &["merge-base", "--is-ancestor", commit, tip])
}

/// Publishes the branch. With `lease`, a branch whose history was rewritten
/// (a restack) replaces the remote one only while that still ends at
/// `lease`, what Tinto last published there.
pub fn push_branch(
    place: Place<'_>,
    worktree: &Path,
    branch: &str,
    lease: Option<&str>,
) -> Result<String, DeliveryError> {
    match lease {
        Some(expected) => {
            let lease = format!("--force-with-lease=refs/heads/{branch}:{expected}");
            git(place, worktree, &["push", "-u", &lease, "origin", branch])?
        }
        None => git(place, worktree, &["push", "-u", "origin", branch])?,
    };
    Ok(format!("origin/{branch}"))
}

/// The tree (content) of `commit`, to tell a history rewrite from a change.
pub fn tree(place: Place<'_>, dir: &Path, commit: &str) -> Option<String> {
    git(place, dir, &["rev-parse", &format!("{commit}^{{tree}}")])
        .ok()
        .filter(|tree| !tree.is_empty())
}

/// Moves the branch's own commits, those not in `from`, onto `onto`
/// (`rebase --onto`). On conflicts the rebase stays paused and the files
/// are returned; see [`rebase_step`].
pub fn restack(
    place: Place<'_>,
    worktree: &Path,
    onto: &str,
    from: &str,
) -> Result<Result<(), Vec<String>>, DeliveryError> {
    let mut args = REBASE_CONFIG.to_vec();
    // A commit whose changes the base already has is dropped, not kept empty.
    args.extend(["rebase", "--quiet", "--empty=drop", "--onto", onto, from]);
    rebase_step(place, worktree, &args, true)
}

/// Where the branch last met any of `heads` (commits of the package it was
/// built on): the newest of its merge bases with them. Heads this git does
/// not have are skipped.
pub fn last_met(place: Place<'_>, worktree: &Path, heads: &[String]) -> Option<String> {
    let mut newest: Option<String> = None;
    for head in heads {
        let Some(met) = git(place, worktree, &["merge-base", "HEAD", head])
            .ok()
            .filter(|met| !met.is_empty())
        else {
            continue;
        };
        if newest
            .as_deref()
            .is_none_or(|known| known != met && is_ancestor(place, worktree, known, &met))
        {
            newest = Some(met);
        }
    }
    newest
}

/// How many commits, merges left out, HEAD has that none of `not` has.
/// Commits in `not` this git does not have are skipped.
fn count_own(place: Place<'_>, worktree: &Path, not: &[&str]) -> Result<usize, DeliveryError> {
    let excluded: Vec<String> = not
        .iter()
        .filter(|commit| {
            git_ok(
                place,
                worktree,
                &["cat-file", "-e", &format!("{commit}^{{commit}}")],
            )
        })
        .map(|commit| format!("^{commit}"))
        .collect();
    let mut args = vec!["rev-list", "--count", "--no-merges", "HEAD"];
    args.extend(excluded.iter().map(String::as_str));
    Ok(git(place, worktree, &args)?.parse().unwrap_or(0))
}

/// The commits a restack from `from` would replay (`from..HEAD`, merges
/// left out) that `onto` or `heads` already have: the base's own, or the
/// merged package's.
pub fn replayed_twice(
    place: Place<'_>,
    worktree: &Path,
    from: &str,
    onto: &str,
    heads: &[String],
) -> Result<usize, DeliveryError> {
    let replayed = count_own(place, worktree, &[from])?;
    let mut not = vec![from, onto];
    not.extend(heads.iter().map(String::as_str));
    Ok(replayed.saturating_sub(count_own(place, worktree, &not)?))
}

/// Whether the branch still has commits of `heads` that `onto` lacks.
pub fn carries(
    place: Place<'_>,
    worktree: &Path,
    onto: &str,
    heads: &[String],
) -> Result<bool, DeliveryError> {
    let mut not = vec![onto];
    not.extend(heads.iter().map(String::as_str));
    Ok(count_own(place, worktree, &[onto])? > count_own(place, worktree, &not)?)
}

/// Git settings every restack runs with, whatever the user's config says:
/// no autosquash or ref updates rewriting more than the branch, no rerere
/// reusing a recorded resolution unseen, and no editor waiting for input.
const REBASE_CONFIG: [&str; 10] = [
    "-c",
    "rebase.autoSquash=false",
    "-c",
    "rebase.updateRefs=false",
    "-c",
    "rerere.enabled=false",
    "-c",
    "core.editor=true",
    "-c",
    "sequence.editor=true",
];

/// Runs a rebase command. It finishes (`Ok(Ok)`), stops on conflicts and
/// stays paused for them to be resolved (`Ok(Err(files))`), or fails for
/// another reason (a hook, a lock). A failed start (`abort`) is abandoned
/// so the branch is as it was; a failed continue stays paused, keeping the
/// conflicts resolved so far. The error says why.
fn rebase_step(
    place: Place<'_>,
    worktree: &Path,
    args: &[&str],
    abort: bool,
) -> Result<Result<(), Vec<String>>, DeliveryError> {
    let Err(error) = git(place, worktree, args) else {
        return Ok(Ok(()));
    };
    let conflicts: Vec<String> = git(place, worktree, &["diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    if !conflicts.is_empty() && rebase_head(place, worktree).is_some() {
        return Ok(Err(conflicts));
    }
    if !abort {
        return Err(DeliveryError::new(
            "restack_failed",
            format!("el rebase no pudo seguir: {}", error.message),
        ));
    }
    if rebase_head(place, worktree).is_some() {
        git(place, worktree, &["rebase", "--abort"])?;
    }
    Err(DeliveryError::new(
        "restack_failed",
        format!(
            "el rebase no pudo seguir y se abandonó, la rama quedó como estaba: {}",
            error.message
        ),
    ))
}

/// The commit a paused rebase stopped on (`REBASE_HEAD`). Git leaves that
/// ref behind once a rebase ends, so a rebase counts as paused only while
/// HEAD is detached too, as it is for the whole rebase.
pub fn rebase_head(place: Place<'_>, worktree: &Path) -> Option<String> {
    if git_ok(place, worktree, &["symbolic-ref", "--quiet", "HEAD"]) {
        return None;
    }
    git(
        place,
        worktree,
        &["rev-parse", "--verify", "--quiet", "REBASE_HEAD"],
    )
    .ok()
    .filter(|commit| !commit.is_empty())
}

/// Whether a sync left git mid-operation in the worktree: a merge or a
/// restack paused on its conflicts.
pub fn sync_in_progress(place: Place<'_>, worktree: &Path) -> bool {
    merge_head(place, worktree).is_some() || rebase_head(place, worktree).is_some()
}

/// Goes on with a paused restack once its conflicts are resolved: stages
/// exactly the files that were conflicted (deleted ones too) and continues.
/// The rebase finishes, stops on the next conflicts, or fails and stays
/// paused (see [`rebase_step`]).
pub fn continue_restack(
    place: Place<'_>,
    worktree: &Path,
    files: &[String],
) -> Result<Result<(), Vec<String>>, DeliveryError> {
    let literal: Vec<String> = files
        .iter()
        .map(|file| format!(":(literal){file}"))
        .collect();
    let mut add = vec!["add", "-A", "--"];
    add.extend(literal.iter().map(String::as_str));
    git(place, worktree, &add)?;
    let mut args = REBASE_CONFIG.to_vec();
    args.extend(["rebase", "--continue"]);
    rebase_step(place, worktree, &args, false)
}

/// Abandons a paused restack: the branch goes back to where it was. What
/// the worktree held (the conflicts resolved so far, new files) is kept
/// first under `refs/tinto/recovery/<name>` (tracked files: untracked ones
/// are not touched), whose commit is returned when
/// there was anything to keep.
pub fn abort_restack(
    place: Place<'_>,
    worktree: &Path,
    name: &str,
) -> Result<Option<String>, DeliveryError> {
    git(place, worktree, &["add", "-u"])?;
    let kept = git(
        place,
        worktree,
        &["stash", "create", "resolución del restack abandonado"],
    )?;
    if !kept.is_empty() {
        git(
            place,
            worktree,
            &["update-ref", &format!("refs/tinto/recovery/{name}"), &kept],
        )?;
    }
    git(place, worktree, &["rebase", "--abort"])?;
    Ok((!kept.is_empty()).then_some(kept))
}

/// Checks out the worktree's submodules when it has a `.gitmodules`: all
/// of them at the commits the branch records (`all`, after Tinto moved the
/// branch), or only those never checked out. Says what it did, `None` when
/// there was nothing to do.
pub fn init_submodules(
    place: Place<'_>,
    worktree: &Path,
    all: bool,
) -> Result<Option<String>, DeliveryError> {
    if !git_ok(place, worktree, &["cat-file", "-e", "HEAD:.gitmodules"]) {
        return Ok(None);
    }
    let status = git(place, worktree, &["submodule", "status", "--recursive"])?;
    let pending: Vec<&str> = status
        .lines()
        .filter(|line| line.starts_with('-') || (all && line.starts_with('+')))
        .filter_map(|line| line.get(1..)?.split_whitespace().nth(1))
        .collect();
    if pending.is_empty() {
        return Ok(None);
    }
    let mut args = vec!["submodule", "update", "--init", "--recursive", "--"];
    args.extend(pending.iter().copied());
    git(place, worktree, &args).map_err(|error| {
        DeliveryError::new(
            "submodules_failed",
            format!(
                "no se pudieron preparar los submódulos ({}): {}",
                pending.join(", "),
                error.message
            ),
        )
    })?;
    Ok(Some(format!(
        "submódulos preparados: {}",
        pending.join(", ")
    )))
}

/// The author git records on a commit made in `worktree` now, as `Name
/// <email>`, from its config; Tinto never sets one of its own.
pub fn author(place: Place<'_>, worktree: &Path) -> Result<String, DeliveryError> {
    let ident = git(place, worktree, &["var", "GIT_AUTHOR_IDENT"]).map_err(|_| {
        DeliveryError::new(
            "no_identity",
            "git no tiene nombre ni email de autor en este repositorio: configura user.name y user.email (git config) antes del commit",
        )
    })?;
    // "Name <email> 1700000000 +0100": without the date.
    Ok(match ident.rfind('>') {
        Some(end) => ident[..=end].to_string(),
        None => ident,
    })
}

/// The emails of the latest authors of `reference`'s history.
pub fn recent_authors(place: Place<'_>, worktree: &Path, reference: &str) -> Vec<String> {
    let log = git(
        place,
        worktree,
        &["log", "-50", "--no-merges", "--format=%ae", reference, "--"],
    )
    .unwrap_or_default();
    let mut authors: Vec<String> = Vec::new();
    for email in log.lines() {
        if !authors.iter().any(|known| known == email) {
            authors.push(email.to_string());
        }
    }
    authors
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;
    use std::process::Command;

    pub fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// A repository with one commit on `main`.
    pub fn repo_with_commit() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["config", "user.email", "t@t"]);
        git(dir.path(), &["config", "user.name", "t"]);
        std::fs::write(dir.path().join("README.md"), "base\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-q", "-m", "init"]);
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{git as run_git, repo_with_commit};
    use super::*;

    #[test]
    fn relative_paths_climb_to_the_common_root() {
        assert_eq!(
            relative_path(
                Path::new("/work/repo-wt/K-1"),
                Path::new("/work/repo/.git/worktrees/K-1")
            )
            .as_deref(),
            Some("../../repo/.git/worktrees/K-1")
        );
        #[cfg(windows)]
        {
            assert_eq!(
                relative_path(
                    Path::new(r"C:\Work\repo-wt\K-1"),
                    Path::new("c:/work/repo/.git/worktrees/K-1")
                )
                .as_deref(),
                Some("../../repo/.git/worktrees/K-1")
            );
            assert_eq!(
                relative_path(
                    Path::new(r"D:\wt\K-1"),
                    Path::new("C:/repo/.git/worktrees/K-1")
                ),
                None
            );
        }
    }

    #[test]
    fn keys_become_safe_folder_and_branch_names() {
        assert_eq!(validate_key(" AGOS-501 ").unwrap(), "AGOS-501");
        assert!(validate_key("../x").is_err());
        assert!(validate_key("a b").is_err());
        assert!(validate_key(".hidden").is_err());
        assert_eq!(default_branch("AGOS-501"), "delivery/agos-501");
    }

    #[test]
    fn worktree_root_sits_next_to_the_repo() {
        let repo = if cfg!(target_os = "windows") {
            r"C:\work\agentos"
        } else {
            "/work/agentos"
        };
        let root = default_worktree_root(Path::new(repo));
        assert!(root.ends_with("agentos-wt"));
        assert_eq!(
            join_place(Place::Wsl("Ubuntu"), Path::new("/home/me/app-wt/"), "K-1"),
            PathBuf::from("/home/me/app-wt/K-1")
        );
    }

    #[test]
    fn base_falls_back_to_existing_branches() {
        let repo = repo_with_commit();
        let head = git(Place::Native, repo.path(), &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(
            resolve_base(Place::Native, repo.path(), None, Some("develop")).unwrap(),
            ("main".to_string(), head.clone())
        );
        assert!(resolve_base(Place::Native, repo.path(), Some("nope"), None).is_err());
        assert!(resolve_base(Place::Native, repo.path(), Some("--all"), None).is_err());
    }

    #[test]
    fn worktree_gets_a_named_branch_and_leaves_the_main_checkout_alone() {
        let repo = repo_with_commit();
        let (_, base) = resolve_base(Place::Native, repo.path(), None, None).unwrap();
        let worktree = repo.path().parent().unwrap().join(format!(
            "{}-wt-K1",
            repo.path().file_name().unwrap().to_string_lossy()
        ));
        let start =
            create_worktree(Place::Native, repo.path(), &worktree, "delivery/k-1", &base).unwrap();
        assert_eq!(start, base);
        assert_eq!(
            git(Place::Native, &worktree, &["branch", "--show-current"]).unwrap(),
            "delivery/k-1"
        );
        assert_eq!(
            git(Place::Native, repo.path(), &["branch", "--show-current"]).unwrap(),
            "main"
        );
        // A second worktree at the same path is refused without side effects.
        assert_eq!(
            create_worktree(Place::Native, repo.path(), &worktree, "other", &base)
                .unwrap_err()
                .category,
            "worktree_exists"
        );

        std::fs::write(worktree.join("new.txt"), "x").unwrap();
        let state = worktree_state(Place::Native, &worktree, &base).unwrap();
        assert!(state.dirty);
        assert_eq!(state.unpublished, 0);
        assert_eq!(
            changed_files(Place::Native, &worktree).unwrap(),
            ["new.txt"]
        );
        let commit = commit_paths(
            Place::Native,
            &worktree,
            "Add new",
            &["new.txt".to_string()],
        )
        .unwrap();
        assert!(on_branch(Place::Native, &worktree, &commit));
        assert!(!published(Place::Native, &worktree, &commit));
        assert_eq!(recent_subjects(Place::Native, &worktree, 1), ["Add new"]);
        let state = worktree_state(Place::Native, &worktree, &base).unwrap();
        assert!(!state.dirty);
        assert_eq!(state.unpublished, 1);
        assert!(!state.has_upstream);

        remove_worktree(Place::Native, repo.path(), &worktree, false).unwrap();
        assert!(!worktree.exists());
        // The branch survives the checkout.
        assert!(git_ok(
            Place::Native,
            repo.path(),
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/delivery/k-1"
            ]
        ));
    }

    #[test]
    fn an_existing_branch_is_resumed_instead_of_recreated() {
        let repo = repo_with_commit();
        run_git(repo.path(), &["branch", "delivery/k-2"]);
        let (_, base) = resolve_base(Place::Native, repo.path(), None, None).unwrap();
        let worktree = repo.path().parent().unwrap().join(format!(
            "{}-wt-K2",
            repo.path().file_name().unwrap().to_string_lossy()
        ));
        create_worktree(Place::Native, repo.path(), &worktree, "delivery/k-2", &base).unwrap();
        assert_eq!(
            git(Place::Native, &worktree, &["branch", "--show-current"]).unwrap(),
            "delivery/k-2"
        );
        remove_worktree(Place::Native, repo.path(), &worktree, true).unwrap();
    }

    #[test]
    fn a_continue_that_git_refuses_leaves_the_restack_paused_with_its_resolution() {
        let repo = repo_with_commit();
        let dir = repo.path();
        std::fs::write(dir.join("other.txt"), "other\n").unwrap();
        run_git(dir, &["add", "."]);
        run_git(dir, &["commit", "-q", "-m", "other"]);
        run_git(dir, &["checkout", "-q", "-b", "task"]);
        std::fs::write(dir.join("README.md"), "task\n").unwrap();
        run_git(dir, &["commit", "-q", "-am", "task"]);
        run_git(dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("README.md"), "main\n").unwrap();
        run_git(dir, &["commit", "-q", "-am", "main"]);
        run_git(dir, &["checkout", "-q", "task"]);

        let stopped = restack(Place::Native, dir, "main", "main~1").unwrap();
        assert_eq!(stopped, Err(vec!["README.md".to_string()]));
        // The resolution, plus an edit to a file that was not conflicted:
        // git refuses to continue on that unstaged change.
        std::fs::write(dir.join("README.md"), "resolved\n").unwrap();
        std::fs::write(dir.join("other.txt"), "edited\n").unwrap();
        let error = continue_restack(Place::Native, dir, &["README.md".to_string()]).unwrap_err();
        assert_eq!(error.category, "restack_failed");
        assert!(rebase_head(Place::Native, dir).is_some());
        assert_eq!(
            std::fs::read_to_string(dir.join("README.md")).unwrap(),
            "resolved\n"
        );
    }

    #[test]
    fn invalid_branch_names_are_refused() {
        let repo = repo_with_commit();
        let worktree = repo.path().join("never");
        assert_eq!(
            create_worktree(Place::Native, repo.path(), &worktree, "bad..name", "HEAD")
                .unwrap_err()
                .category,
            "invalid_branch"
        );
    }

    #[test]
    fn a_worktree_gets_the_submodules_its_branch_records() {
        let sub = repo_with_commit();
        let repo = repo_with_commit();
        run_git(
            repo.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                &plain_path(sub.path()),
                "vendor/sub",
            ],
        );
        run_git(repo.path(), &["commit", "-q", "-m", "add submodule"]);
        let head = git(Place::Native, repo.path(), &["rev-parse", "HEAD"]).unwrap();
        let worktree = repo.path().join("wt");
        create_worktree(Place::Native, repo.path(), &worktree, "delivery/s-1", &head).unwrap();
        assert!(!worktree.join("vendor/sub/README.md").exists());
        // git refuses to clone a local submodule outside the user's own
        // command (protocol.file.allow): the worktree gets the clone the
        // main checkout already has, as a remote one would be fetched.
        copy_dir(
            &repo.path().join(".git/modules"),
            &repo.path().join(".git/worktrees/wt/modules"),
        );

        let note = init_submodules(Place::Native, &worktree, false).unwrap();
        assert_eq!(note.as_deref(), Some("submódulos preparados: vendor/sub"));
        assert!(worktree.join("vendor/sub/README.md").exists());
        // Nothing left to do; a repo without submodules has nothing either.
        assert_eq!(
            init_submodules(Place::Native, &worktree, true).unwrap(),
            None
        );
        assert_eq!(
            init_submodules(Place::Native, sub.path(), true).unwrap(),
            None
        );
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
