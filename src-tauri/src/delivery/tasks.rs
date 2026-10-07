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

/// Runs `git -C <dir> <args>` and returns trimmed stdout.
pub fn git(place: Place<'_>, dir: &Path, args: &[&str]) -> Result<String, DeliveryError> {
    let output = git_command(place, dir, args).output().map_err(|error| {
        DeliveryError::new("git_failed", format!("no se pudo ejecutar git: {error}"))
    })?;
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

fn verify_commit(place: Place<'_>, repo: &Path, reference: &str) -> Option<String> {
    if reference.starts_with('-') {
        return None;
    }
    let spec = format!("{reference}^{{commit}}");
    git(place, repo, &["rev-parse", "--verify", "--quiet", &spec])
        .ok()
        .filter(|commit| !commit.is_empty())
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
    git(place, worktree, &["rev-parse", "HEAD"])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeState {
    pub dirty: bool,
    /// Commits on the branch that are not on its upstream (or, without one,
    /// not on the base commit).
    pub unpublished: u32,
    pub has_upstream: bool,
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

pub fn commit_all(
    place: Place<'_>,
    worktree: &Path,
    subject: &str,
    body: &str,
) -> Result<String, DeliveryError> {
    git(place, worktree, &["add", "--all"])?;
    let mut args = vec!["commit", "-m", subject];
    if !body.trim().is_empty() {
        args.extend(["-m", body]);
    }
    git(place, worktree, &args)?;
    git(place, worktree, &["rev-parse", "--short", "HEAD"])
}

pub fn push_branch(
    place: Place<'_>,
    worktree: &Path,
    branch: &str,
) -> Result<String, DeliveryError> {
    git(place, worktree, &["push", "-u", "origin", branch])?;
    Ok(format!("origin/{branch}"))
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
    fn keys_become_safe_folder_and_branch_names() {
        assert_eq!(validate_key(" AGOS-501 ").unwrap(), "AGOS-501");
        assert!(validate_key("../x").is_err());
        assert!(validate_key("a b").is_err());
        assert!(validate_key(".hidden").is_err());
        assert_eq!(default_branch("AGOS-501"), "delivery/agos-501");
    }

    #[test]
    fn worktree_root_sits_next_to_the_repo() {
        let root = default_worktree_root(Path::new(r"C:\work\agentos"));
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
        commit_all(Place::Native, &worktree, "Add new", "").unwrap();
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
}
