use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

use ignore::WalkBuilder;

use crate::agent_console::checkpoint::{
    create_checkpoint, create_ephemeral_checkpoint, remove_ephemeral_checkpoint, revert_checkpoint,
    revert_checkpoint_file, scan_change_log, CheckpointConfig,
};
use crate::agent_console::validation::validate_agent_type;
use crate::bus::commands::{
    fetch_repo_local, file_content_from_bytes, gitleaks_setup_status, list_repo_tree_capped,
    read_file_content_bounded, read_media_content_bounded, repo_fetch_preview, validate_media_path,
    write_repo_agents_md_config, write_repo_gitleaks_config,
};
use crate::bus::contract::{
    GitleaksInstallResult, RepoDelta, RepoErrorClass, RepoErrorState, RepoMetrics,
    SubscriptionTarget, REPO_TREE_MAX_ENTRIES,
};
use crate::bus::secret_scan;
use crate::bus::{git_error_state, recalc_blocking, RecalcScope};
#[cfg(test)]
use crate::file_ops::commands::transactional_copy_with_stage_copy;
use crate::file_ops::commands::{
    plan_delete_replay, read_bound_delete_manifest, run_copy_batch_with_hook,
    run_delete_batch_with_hook, run_move_batch_with_hook, run_replay_batch_with_hook,
    undo_backup_root, write_delete_manifest, CopyResult, DeleteManifest, DeleteResult,
    DeletedEntry, FileOpOutcome, ReplayDirection,
};
use crate::file_ops::{safe_join, FileConflict, FileConflictKind};
use crate::git::{Git2Engine, GitEngine};
use crate::paths::{Classification, PathClassifier};
use uuid::Uuid;

use super::protocol::{
    encode_agent_response, parse_agent_request_line, AgentError, AgentErrorCategory, AgentRequest,
    AgentResponse, FileFingerprint, GitReviewFinding, GitReviewSummary,
    RepoFileFingerprintSnapshot, RepoFsWatchConfig, RepoSnapshotScope, WslDirectoryEntry,
    WslDirectoryListing,
};

pub fn respond_to_request_line(line: &str) -> Result<String, AgentError> {
    let request = parse_agent_request_line(line)?;
    let response = handle_request(request);
    encode_agent_response(&response)
}

pub fn serve_request_lines<R: io::BufRead, W: io::Write>(
    mut reader: R,
    mut writer: W,
) -> Result<(), AgentError> {
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdin cerrado"))?;
        if read == 0 {
            return Ok(());
        }
        let response = match respond_to_request_line(&line) {
            Ok(response) => response,
            Err(error) => encode_agent_response(&AgentResponse::Error {
                category: error.safe_category().to_string(),
                message: error.message,
            })?,
        };
        writer
            .write_all(response.as_bytes())
            .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdout cerrado"))?;
        writer
            .flush()
            .map_err(|_| AgentError::new(AgentErrorCategory::ChildExit, "stdout cerrado"))?;
    }
}

fn handle_request(request: AgentRequest) -> AgentResponse {
    match request {
        AgentRequest::Handshake { .. } => AgentResponse::current_handshake(),
        AgentRequest::RepoSnapshot {
            repos,
            subscriptions,
            scope,
            ..
        } => AgentResponse::RepoSnapshot {
            repos: repos
                .iter()
                .map(|repo| repo_delta(repo, &subscriptions, scope))
                .collect(),
        },
        AgentRequest::RepoSnapshotWithFsEvents {
            repos,
            subscriptions,
            fs_watch,
            scope,
            ..
        } => {
            let deltas = repos
                .iter()
                .map(|repo| repo_delta(repo, &subscriptions, scope))
                .collect();
            let fingerprints = repos
                .iter()
                .map(|repo| {
                    with_allowed_repo(repo, &repos, || {
                        Ok(AgentResponse::RepoSnapshotWithFsEvents {
                            repos: Vec::new(),
                            fingerprints: vec![RepoFileFingerprintSnapshot {
                                repo: repo.clone(),
                                files: file_fingerprints(repo, fs_watch_patterns(repo, &fs_watch))?,
                            }],
                        })
                    })
                })
                .filter_map(|response| match response {
                    AgentResponse::RepoSnapshotWithFsEvents { fingerprints, .. } => {
                        fingerprints.into_iter().next()
                    }
                    _ => None,
                })
                .collect();
            AgentResponse::RepoSnapshotWithFsEvents {
                repos: deltas,
                fingerprints,
            }
        }
        AgentRequest::ListDirectory { path, .. } => match list_directory(path.as_deref()) {
            Ok(listing) => AgentResponse::DirectoryListing { listing },
            Err(error) => AgentResponse::error(error.category, error.message),
        },
        AgentRequest::WorktreeDiff {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let diffs = Git2Engine::open(&repo)?.worktree_diff()?;
            Ok(AgentResponse::WorktreeDiff { diffs })
        }),
        AgentRequest::CommitDiff {
            repo,
            allowed_repos,
            commit_id,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let diffs = Git2Engine::open(&repo)?.commit_diff(&commit_id)?;
            Ok(AgentResponse::CommitDiff { diffs })
        }),
        AgentRequest::CommitLog {
            repo,
            allowed_repos,
            offset,
            limit,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let commits = Git2Engine::open(&repo)?.log(offset, limit)?;
            Ok(AgentResponse::CommitLog { commits })
        }),
        AgentRequest::Blob {
            repo,
            allowed_repos,
            commit_id,
            path,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let bytes = Git2Engine::open(&repo)?.blob_at(&commit_id, &path)?;
            Ok(AgentResponse::Blob {
                content: file_content_from_bytes(bytes),
            })
        }),
        AgentRequest::FileContent {
            repo,
            allowed_repos,
            path,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let abs = resolve_within_linux(&repo, &path)?;
            Ok(AgentResponse::FileContent {
                content: read_file_content_bounded(&abs)?,
            })
        }),
        AgentRequest::MediaContent {
            repo,
            allowed_repos,
            path,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let abs = resolve_within_linux(&repo, &path)?;
            validate_media_path(&path)?;
            Ok(AgentResponse::MediaContent {
                content: read_media_content_bounded(&abs)?,
            })
        }),
        AgentRequest::RepoTree {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let tree = list_repo_tree_capped(&repo, REPO_TREE_MAX_ENTRIES)?;
            Ok(AgentResponse::RepoTree { tree })
        }),
        AgentRequest::GitReviewSummary {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::GitReviewSummary {
                summary: git_review_summary_linux(&repo)?,
            })
        }),
        AgentRequest::RepoFetchPreview {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::RepoFetchPreview {
                preview: repo_fetch_preview(&repo, None)?,
            })
        }),
        AgentRequest::FetchRepo {
            repo,
            allowed_repos,
            remote,
            confirmed_host,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::RepoFetchResult {
                result: fetch_repo_local(&repo, &remote, &confirmed_host)?,
            })
        }),
        AgentRequest::CreateGitWorktree {
            repo,
            allowed_repos,
            session_id,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::GitWorktreeCreated {
                path: create_git_worktree_linux(&repo, &session_id)?,
            })
        }),
        AgentRequest::RemoveGitWorktree {
            repo,
            allowed_repos,
            target,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            remove_git_worktree_linux(&repo, &target)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::GitleaksSetupStatus {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::GitleaksSetupStatus {
                status: gitleaks_setup_status(),
            })
        }),
        AgentRequest::InstallGitleaks {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let outcome = secret_scan::install_gitleaks();
            let status = gitleaks_setup_status();
            Ok(AgentResponse::GitleaksInstallResult {
                result: GitleaksInstallResult {
                    installed: status.installed,
                    version: status.version,
                    binary_path: status.binary_path,
                    method: outcome.method.map(str::to_string),
                    message: outcome.message,
                },
            })
        }),
        AgentRequest::CreateGitleaksConfig {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            write_repo_gitleaks_config(&repo)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::CreateAgentsMdConfig {
            repo,
            allowed_repos,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            write_repo_agents_md_config(&repo)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::AgentBinaryAvailable { agent_type, .. } => {
            match agent_binary_available(&agent_type) {
                Ok(available) => AgentResponse::AgentBinaryAvailable { available },
                Err(error) => AgentResponse::error(error.category, error.message),
            }
        }
        AgentRequest::AgentCheckpointCreate {
            repo,
            allowed_repos,
            session_id,
            created_at_ms,
            ephemeral,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let checkpoint = if ephemeral {
                create_ephemeral_checkpoint(
                    &repo,
                    &session_id,
                    created_at_ms,
                    &CheckpointConfig::default(),
                )?
            } else {
                create_checkpoint(
                    &repo,
                    &session_id,
                    created_at_ms,
                    &CheckpointConfig::default(),
                )?
            };
            Ok(AgentResponse::AgentCheckpoint { checkpoint })
        }),
        AgentRequest::AgentCheckpointScan {
            allowed_repos,
            checkpoint,
            timestamp_ms,
            ..
        } => with_allowed_repo(&checkpoint.repo.clone(), &allowed_repos, || {
            Ok(AgentResponse::AgentChangeLog {
                changes: scan_change_log(&checkpoint, timestamp_ms)
                    .map_err(AgentRuntimeError::from_agent_console)?,
            })
        }),
        AgentRequest::AgentCheckpointRevert {
            allowed_repos,
            checkpoint,
            ..
        } => with_allowed_repo(&checkpoint.repo.clone(), &allowed_repos, || {
            revert_checkpoint(&checkpoint)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::AgentCheckpointRevertFile {
            allowed_repos,
            checkpoint,
            path,
            ..
        } => with_allowed_repo(&checkpoint.repo.clone(), &allowed_repos, || {
            revert_checkpoint_file(&checkpoint, &path)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::AgentCheckpointRemove {
            allowed_repos,
            checkpoint,
            ..
        } => with_allowed_repo(&checkpoint.repo.clone(), &allowed_repos, || {
            remove_ephemeral_checkpoint(&checkpoint)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::CopyToRepo {
            repo,
            allowed_repos,
            dest_dir,
            sources,
            overwrite,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::CopyResult {
                result: copy_to_repo_linux(&repo, &dest_dir, &sources, overwrite)?,
            })
        }),
        AgentRequest::CopyWithinRepo {
            repo,
            allowed_repos,
            sources,
            dest_dir,
            overwrite,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::CopyResult {
                result: copy_within_repo_linux(&repo, &sources, &dest_dir, overwrite)?,
            })
        }),
        AgentRequest::MoveWithinRepo {
            repo,
            allowed_repos,
            sources,
            dest_dir,
            overwrite,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::CopyResult {
                result: move_within_repo_linux(&repo, &sources, &dest_dir, overwrite)?,
            })
        }),
        AgentRequest::ExportFromRepo {
            repo,
            allowed_repos,
            sources,
            dest_dir,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::FileOpOutcome {
                result: export_from_repo_linux(&repo, &sources, &dest_dir)?,
            })
        }),
        AgentRequest::DeleteFromRepo {
            repo,
            allowed_repos,
            sources,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::DeleteResult {
                result: delete_from_repo_linux(&repo, &sources)?,
            })
        }),
        AgentRequest::RestoreDeletedFromRepo {
            repo,
            allowed_repos,
            token,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::FileOpOutcome {
                result: restore_deleted_from_repo_linux(&repo, &token)?,
            })
        }),
        AgentRequest::RedoDeletedFromRepo {
            repo,
            allowed_repos,
            token,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            Ok(AgentResponse::FileOpOutcome {
                result: redo_deleted_from_repo_linux(&repo, &token)?,
            })
        }),
    }
}

fn with_allowed_repo<F>(repo: &Path, allowed_repos: &[PathBuf], f: F) -> AgentResponse
where
    F: FnOnce() -> Result<AgentResponse, AgentRuntimeError>,
{
    if !allowed_repos
        .iter()
        .any(|allowed| same_linux_path(allowed, repo))
    {
        return AgentResponse::error(
            "repo-not-allowed",
            "el repo no pertenece al workbench activo",
        );
    }
    match f() {
        Ok(response) => response,
        Err(error) => AgentResponse::error(error.category, error.message),
    }
}

fn git_review_summary_linux(repo: &Path) -> Result<GitReviewSummary, AgentRuntimeError> {
    let branch = git_stdout_linux(repo, &["branch", "--show-current"])?;
    let branch = if branch.trim().is_empty() {
        "detached HEAD".to_string()
    } else {
        branch.trim().to_string()
    };
    let status = git_stdout_linux(repo, &["status", "--short"])?;
    let status_lines = status
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let files = status_lines.iter().take(12).cloned().collect::<Vec<_>>();
    let changed_files = status_lines.len();
    let truncated_count = changed_files.saturating_sub(files.len());
    let working_shortstat = empty_to_none(git_stdout_linux(repo, &["diff", "--shortstat"])?);
    let staged_shortstat = empty_to_none(git_stdout_linux(
        repo,
        &["diff", "--cached", "--shortstat"],
    )?);
    let findings = git_review_findings_linux(repo, &status_lines);
    Ok(GitReviewSummary {
        branch,
        changed_files,
        working_shortstat,
        staged_shortstat,
        files,
        truncated_count,
        findings,
    })
}

fn git_review_findings_linux(repo: &Path, status_lines: &[String]) -> Vec<GitReviewFinding> {
    let mut findings = Vec::new();
    let changed_paths = changed_paths_from_status(repo, status_lines);
    let has_package_json = changed_paths
        .iter()
        .any(|path| path == Path::new("package.json"));
    if changed_paths
        .iter()
        .any(|path| path == Path::new("package-lock.json"))
        && !has_package_json
    {
        findings.push(GitReviewFinding {
            severity: "medium".to_string(),
            title: "Lockfile changed without package manifest".to_string(),
            detail: "package-lock.json changed but package.json did not; verify the lockfile was intentionally regenerated without dependency metadata changes.".to_string(),
            path: Some(PathBuf::from("package-lock.json")),
            line: None,
        });
    }
    for path in changed_paths {
        if sensitive_review_path(&path) {
            findings.push(GitReviewFinding {
                severity: "high".to_string(),
                title: "Sensitive path changed".to_string(),
                detail: format!(
                    "{} looks like an environment, credential, or secret-bearing path; verify no secrets are committed.",
                    path.display()
                ),
                path: Some(path.clone()),
                line: None,
            });
        }
        if let Some(line) = conflict_marker_line(repo, &path) {
            findings.push(GitReviewFinding {
                severity: "high".to_string(),
                title: "Conflict marker present".to_string(),
                detail: format!(
                    "{} still contains a merge conflict marker; resolve it before review.",
                    path.display()
                ),
                path: Some(path),
                line: Some(line),
            });
        }
    }
    findings
}

fn git_stdout_linux(repo: &Path, args: &[&str]) -> Result<String, AgentRuntimeError> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        Err(AgentRuntimeError::new("git_review_failed", message))
    }
}

fn changed_paths_from_status(repo: &Path, status_lines: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in status_lines {
        let Some(path) = line.get(2..).map(str::trim) else {
            continue;
        };
        let path = path.rsplit_once(" -> ").map(|(_, new)| new).unwrap_or(path);
        if path.is_empty() {
            continue;
        }
        let path_buf = PathBuf::from(path);
        paths.push(path_buf.clone());
        if path.ends_with('/') {
            append_changed_directory_paths(repo, &path_buf, &mut paths, 64);
        }
    }
    paths
}

fn append_changed_directory_paths(
    repo: &Path,
    relative_dir: &Path,
    paths: &mut Vec<PathBuf>,
    limit: usize,
) {
    let mut added = 0;
    let mut stack = vec![relative_dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if added >= limit {
            return;
        }
        let Ok(abs) = safe_join(repo, &current) else {
            continue;
        };
        let Ok(entries) = fs::read_dir(abs) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if added >= limit {
                return;
            }
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            let child = current.join(name);
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(child);
            } else if file_type.is_file() {
                paths.push(child);
                added += 1;
            }
        }
    }
}

fn sensitive_review_path(path: &Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name == "id_rsa"
        || file_name == "id_ed25519"
        || lower.contains("/secrets/")
        || lower.contains("/secret/")
}

fn conflict_marker_line(repo: &Path, path: &Path) -> Option<usize> {
    let abs = safe_join(repo, path).ok()?;
    if !abs.is_file() {
        return None;
    }
    let metadata = fs::metadata(&abs).ok()?;
    if metadata.len() > 512 * 1024 {
        return None;
    }
    let bytes = fs::read(&abs).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    text.lines().enumerate().find_map(|(index, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("<<<<<<< ")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>> ")
        {
            Some(index + 1)
        } else {
            None
        }
    })
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn create_git_worktree_linux(repo: &Path, session_id: &str) -> Result<PathBuf, AgentRuntimeError> {
    if !git_has_head_linux(repo)? {
        return Err(AgentRuntimeError::new(
            "worktree_no_head",
            "this repo has no HEAD commit yet",
        ));
    }
    let target = linux_fork_worktree_path(repo, session_id)?;
    let parent = target
        .parent()
        .ok_or_else(|| AgentRuntimeError::new("worktree_path_invalid", "invalid worktree path"))?;
    fs::create_dir_all(parent)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "add", "--detach"])
        .arg(&target)
        .arg("HEAD")
        .output()?;
    if output.status.success() {
        Ok(target)
    } else {
        Err(AgentRuntimeError::new(
            "worktree_create_failed",
            command_output_message(&output),
        ))
    }
}

fn remove_git_worktree_linux(repo: &Path, target: &Path) -> Result<(), AgentRuntimeError> {
    ensure_tinto_worktree_target(target)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "remove", "--force"])
        .arg(target)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AgentRuntimeError::new(
            "worktree_remove_failed",
            command_output_message(&output),
        ))
    }
}

fn git_has_head_linux(repo: &Path) -> Result<bool, AgentRuntimeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?;
    Ok(output.status.success())
}

fn linux_fork_worktree_path(repo: &Path, session_id: &str) -> Result<PathBuf, AgentRuntimeError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| AgentRuntimeError::new("worktree_home_unavailable", "HOME unavailable"))?;
    Ok(home
        .join(".tinto")
        .join("worktrees")
        .join(linux_path_hash(repo))
        .join(format!(
            "fork-{}-{}",
            short_session_id(session_id),
            Uuid::new_v4()
        )))
}

fn ensure_tinto_worktree_target(target: &Path) -> Result<(), AgentRuntimeError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| AgentRuntimeError::new("worktree_home_unavailable", "HOME unavailable"))?;
    let root = home.join(".tinto").join("worktrees");
    if target.is_absolute() && target.starts_with(root) {
        Ok(())
    } else {
        Err(AgentRuntimeError::new(
            "worktree_target_not_allowed",
            "worktree target is outside the Tinto worktree root",
        ))
    }
}

fn linux_path_hash(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn short_session_id(session_id: &str) -> String {
    let short = session_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>();
    if short.is_empty() {
        "session".to_string()
    } else {
        short
    }
}

fn command_output_message(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        stdout
    } else {
        format!("command exited with status {}", output.status)
    }
}

fn repo_delta(
    repo: &Path,
    subscriptions: &[SubscriptionTarget],
    scope: RepoSnapshotScope,
) -> RepoDelta {
    let subs: Vec<SubscriptionTarget> = subscriptions
        .iter()
        .filter(|target| same_linux_path(&target.repo, repo))
        .cloned()
        .collect();

    match recalc_blocking(repo, recalc_scope_for_snapshot(scope), &subs) {
        Ok(outcome) => RepoDelta {
            repo: repo.to_path_buf(),
            revision: 0,
            status: outcome.status,
            branch: outcome.branch,
            head: outcome.head.flatten(),
            last_activity_ms: now_ms(),
            error: None,
            metrics: outcome.metrics,
            gitleaks_configured: outcome.gitleaks_configured,
            agents_md_configured: outcome.agents_md_configured,
            signals: outcome.signals,
            secret_findings: outcome.secret_findings.unwrap_or_default(),
            secret_scan_status: outcome.secret_scan_status.unwrap_or_default(),
            subscribed_diffs: outcome.subscribed_diffs,
        },
        Err(error) => RepoDelta {
            repo: repo.to_path_buf(),
            revision: 0,
            status: Default::default(),
            branch: None,
            head: None,
            last_activity_ms: now_ms(),
            error: Some(agent_repo_error(git_error_state(&error))),
            metrics: RepoMetrics::default(),
            gitleaks_configured: false,
            agents_md_configured: false,
            signals: Vec::new(),
            secret_findings: Vec::new(),
            secret_scan_status: Default::default(),
            subscribed_diffs: None,
        },
    }
}

fn recalc_scope_for_snapshot(scope: RepoSnapshotScope) -> RecalcScope {
    match scope {
        RepoSnapshotScope::StatusOnly => RecalcScope::StatusOnly,
        RepoSnapshotScope::Metadata => RecalcScope::Metadata,
        RepoSnapshotScope::Everything => RecalcScope::Everything,
    }
}

fn fs_watch_patterns(repo: &Path, configs: &[RepoFsWatchConfig]) -> Vec<String> {
    configs
        .iter()
        .find(|config| same_linux_path(&config.repo, repo))
        .map(|config| config.patterns.clone())
        .unwrap_or_default()
}

fn list_directory(path: Option<&Path>) -> Result<WslDirectoryListing, AgentRuntimeError> {
    let root = match path {
        Some(path) => path.to_path_buf(),
        None => std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| AgentRuntimeError::new("missing-home", "HOME no esta definido"))?,
    };
    if !root.is_dir() {
        return Err(AgentRuntimeError::new(
            "not-a-directory",
            format!("not a directory: {}", root.display()),
        ));
    }
    let root = root
        .canonicalize()
        .map_err(|error| AgentRuntimeError::new("io", error.to_string()))?;
    let mut entries = Vec::new();
    for entry in
        fs::read_dir(&root).map_err(|error| AgentRuntimeError::new("io", error.to_string()))?
    {
        let entry = entry.map_err(|error| AgentRuntimeError::new("io", error.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| AgentRuntimeError::new("io", error.to_string()))?;
        if !file_type.is_dir() {
            continue;
        }
        entries.push(WslDirectoryEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: entry.path().to_string_lossy().into_owned(),
        });
    }
    entries.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(WslDirectoryListing {
        is_git_repo: root.join(".git").is_dir(),
        path: root.to_string_lossy().into_owned(),
        entries,
    })
}

fn agent_binary_available(agent_type: &str) -> Result<bool, AgentRuntimeError> {
    let binary = validate_agent_type(agent_type)
        .map_err(|error| AgentRuntimeError::new(error.category, error.message))?;
    let output = Command::new("bash")
        .arg("-lc")
        .arg(super::shell_env::agent_binary_check_script())
        .arg("tinto-agent-binary-check")
        .arg(binary)
        .output()?;
    Ok(output.status.success() && !output.stdout.is_empty())
}

fn file_fingerprints(
    repo: &Path,
    fs_watch: Vec<String>,
) -> Result<Vec<FileFingerprint>, AgentRuntimeError> {
    let repo = repo
        .canonicalize()
        .map_err(|_| AgentRuntimeError::new("repository-not-found", "el repo no existe"))?;
    let classifier = PathClassifier::new(&repo, &fs_watch)
        .map_err(|error| AgentRuntimeError::new("watchlist", error.to_string()))?;
    let mut files = Vec::new();
    let walker = WalkBuilder::new(&repo)
        .follow_links(false)
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build();

    for entry in walker {
        let entry = entry.map_err(|error| AgentRuntimeError::new("io", error.to_string()))?;
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(&repo) else {
            continue;
        };
        if rel.as_os_str().is_empty() || has_git_component(rel) {
            continue;
        }
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        if classifier.classify(rel, false) != Classification::Plane2 {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| AgentRuntimeError::new("io", error.to_string()))?;
        files.push(FileFingerprint {
            path: rel.to_path_buf(),
            size: metadata.len(),
            modified_ms: metadata_mtime_ms(&metadata),
        });
        if files.len() >= REPO_TREE_MAX_ENTRIES {
            break;
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn metadata_mtime_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn agent_repo_error(mut error: RepoErrorState) -> RepoErrorState {
    error.class = RepoErrorClass::Transient;
    error
}

fn resolve_within_linux(repo: &Path, rel: &Path) -> Result<PathBuf, AgentRuntimeError> {
    if rel.is_absolute() || has_navigation_component(rel) {
        return Err(AgentRuntimeError::new(
            "path-traversal",
            "el path se sale del repositorio",
        ));
    }
    if rel
        .components()
        .any(|component| component.as_os_str() == ".git")
    {
        return Err(AgentRuntimeError::new(
            "path-forbidden",
            "el directorio .git no se expone",
        ));
    }
    Ok(repo.join(rel))
}

fn same_linux_path(a: &Path, b: &Path) -> bool {
    normalize_lexically(a) == normalize_lexically(b)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn has_navigation_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) || path
        .to_string_lossy()
        .split('/')
        .any(|segment| segment == "." || segment == "..")
}

fn has_git_component(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == ".git")
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn copy_to_repo_linux(
    repo: &Path,
    dest_dir: &Path,
    sources: &[PathBuf],
    overwrite: bool,
) -> Result<CopyResult, AgentRuntimeError> {
    let dest_abs = safe_join(repo, dest_dir)?;
    if !dest_abs.is_dir() {
        return Err(AgentRuntimeError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio dentro del repo",
        ));
    }
    let mut to_copy = Vec::with_capacity(sources.len());
    let mut conflicts = Vec::new();
    for src in sources {
        let name = src
            .file_name()
            .ok_or_else(|| AgentRuntimeError::new("invalid-source", "source sin file_name"))?;
        let dest = dest_abs.join(name);
        let dest_rel = dest.strip_prefix(repo).unwrap_or(&dest).to_path_buf();
        classify_for_copy(
            src,
            &dest,
            overwrite,
            &mut to_copy,
            &mut conflicts,
            dest_rel,
        );
    }
    if !conflicts.is_empty() {
        return Ok(CopyResult {
            copied: Vec::new(),
            conflicts,
            warnings: Vec::new(),
        });
    }
    let (copied, warnings) = run_linux_copy_or_move_batch(repo, to_copy, false, |_| Ok(()))?;
    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
        warnings,
    })
}

fn copy_within_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
    overwrite: bool,
) -> Result<CopyResult, AgentRuntimeError> {
    copy_or_move_within_repo_linux(repo, sources, dest_dir, overwrite, false)
}

fn move_within_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
    overwrite: bool,
) -> Result<CopyResult, AgentRuntimeError> {
    copy_or_move_within_repo_linux(repo, sources, dest_dir, overwrite, true)
}

fn copy_or_move_within_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
    overwrite: bool,
    move_sources: bool,
) -> Result<CopyResult, AgentRuntimeError> {
    let dest_abs = safe_join(repo, dest_dir)?;
    if !dest_abs.is_dir() {
        return Err(AgentRuntimeError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio dentro del repo",
        ));
    }
    let mut pairs = Vec::with_capacity(sources.len());
    let mut conflicts = Vec::new();
    for src_rel in sources {
        let src_abs = safe_join(repo, src_rel)?;
        let name = src_abs
            .file_name()
            .ok_or_else(|| AgentRuntimeError::new("invalid-source", "source sin file_name"))?;
        let dest = dest_abs.join(name);
        if src_abs == dest {
            return Err(AgentRuntimeError::new(
                "same-src-dest",
                "source y destino son el mismo path",
            ));
        }
        let dest_rel = dest.strip_prefix(repo).unwrap_or(&dest).to_path_buf();
        classify_for_copy(
            &src_abs,
            &dest,
            overwrite,
            &mut pairs,
            &mut conflicts,
            dest_rel,
        );
    }
    if !conflicts.is_empty() {
        return Ok(CopyResult {
            copied: Vec::new(),
            conflicts,
            warnings: Vec::new(),
        });
    }
    let (copied, warnings) = run_linux_copy_or_move_batch(repo, pairs, move_sources, |_| Ok(()))?;
    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
        warnings,
    })
}

fn run_linux_copy_or_move_batch(
    repo: &Path,
    pairs: Vec<(PathBuf, PathBuf)>,
    move_sources: bool,
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> Result<(Vec<String>, Vec<String>), AgentRuntimeError> {
    let batch = if move_sources {
        run_move_batch_with_hook(pairs, after_item)?
    } else {
        run_copy_batch_with_hook(pairs, after_item)?
    };
    let copied = batch
        .destinations
        .into_iter()
        .map(|destination| {
            destination
                .strip_prefix(repo)
                .unwrap_or(&destination)
                .display()
                .to_string()
        })
        .collect();
    Ok((copied, batch.warnings))
}

fn classify_for_copy(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    out: &mut Vec<(PathBuf, PathBuf)>,
    conflicts: &mut Vec<FileConflict>,
    dest_rel: PathBuf,
) {
    if !src.exists() {
        conflicts.push(FileConflict::new(dest_rel, FileConflictKind::SourceMissing));
    } else if dest.exists() && !overwrite {
        let kind = if dest.is_dir() {
            FileConflictKind::DirExists
        } else {
            FileConflictKind::FileExists
        };
        conflicts.push(FileConflict::new(dest_rel, kind));
    } else {
        out.push((src.to_path_buf(), dest.to_path_buf()));
    }
}

fn export_from_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
) -> Result<FileOpOutcome, AgentRuntimeError> {
    let pairs = export_pairs_linux(repo, sources, dest_dir)?;
    let batch = run_copy_batch_with_hook(pairs, |_| Ok(()))?;
    Ok(FileOpOutcome {
        warnings: batch.warnings,
    })
}

#[cfg(test)]
fn export_from_repo_linux_with_copy(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
    mut copy: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), AgentRuntimeError> {
    let pairs = export_pairs_linux(repo, sources, dest_dir)?;
    for (src, dest) in pairs {
        copy(&src, &dest)?;
    }
    Ok(())
}

fn export_pairs_linux(
    repo: &Path,
    sources: &[PathBuf],
    dest_dir: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, AgentRuntimeError> {
    if !dest_dir.is_dir() {
        return Err(AgentRuntimeError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio en el filesystem",
        ));
    }
    let mut pairs = Vec::with_capacity(sources.len());
    for src_rel in sources {
        let src_abs = safe_join(repo, src_rel)?;
        if !src_abs.exists() {
            return Err(AgentRuntimeError::new(
                "source-missing",
                format!("no existe {} en el repo", src_rel.display()),
            ));
        }
        let name = src_abs.file_name().unwrap_or_default();
        let dest = dest_dir.join(name);
        pairs.push((src_abs, dest));
    }
    Ok(pairs)
}

fn delete_from_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
) -> Result<DeleteResult, AgentRuntimeError> {
    delete_from_repo_linux_with_hook(repo, sources, |_| Ok(()))
}

fn delete_from_repo_linux_with_hook(
    repo: &Path,
    sources: &[PathBuf],
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> Result<DeleteResult, AgentRuntimeError> {
    let repo_abs = repo
        .canonicalize()
        .map_err(|_| AgentRuntimeError::new("repository-not-found", "el repo no existe"))?;
    let mut targets = Vec::with_capacity(sources.len());
    for src_rel in sources {
        let src_abs = safe_join(&repo_abs, src_rel)?;
        if src_abs == repo_abs {
            return Err(AgentRuntimeError::new(
                "delete-root-forbidden",
                "no se puede eliminar el root del repo",
            ));
        }
        if !src_abs.exists() {
            return Err(AgentRuntimeError::new(
                "source-missing",
                format!("no existe {} en el repo", src_rel.display()),
            ));
        }
        let rel = src_abs
            .strip_prefix(&repo_abs)
            .unwrap_or(&src_abs)
            .to_path_buf();
        let is_dir = src_abs.is_dir();
        targets.push((rel, src_abs, is_dir));
    }
    targets.sort_by(|a, b| a.1.cmp(&b.1));
    targets.dedup_by(|a, b| a.1 == b.1);
    let mut filtered: Vec<(PathBuf, PathBuf, bool)> = Vec::with_capacity(targets.len());
    'targets: for target in targets {
        for kept in &filtered {
            if target.1.starts_with(&kept.1) {
                continue 'targets;
            }
        }
        filtered.push(target);
    }
    let token = Uuid::new_v4().to_string();
    let backup_root = undo_backup_root(&token)?;
    let objects_root = backup_root.join("objects");
    let entries: Vec<DeletedEntry> = filtered
        .iter()
        .enumerate()
        .map(|(index, (path, _, is_dir))| DeletedEntry {
            path: path.clone(),
            is_dir: *is_dir,
            backup_name: index.to_string(),
        })
        .collect();
    write_delete_manifest(
        &backup_root,
        &DeleteManifest {
            token: token.clone(),
            repo: repo_abs.clone(),
            entries: entries.clone(),
        },
    )?;
    let moves = filtered
        .into_iter()
        .enumerate()
        .map(|(index, (_, target, _))| (target, objects_root.join(index.to_string())))
        .collect();
    match run_delete_batch_with_hook(&backup_root, moves, after_item) {
        Ok(warnings) => Ok(DeleteResult {
            token,
            entries,
            completed: true,
            recovery_required: false,
            warnings,
        }),
        Err(failure) if failure.recovery_required => Ok(DeleteResult {
            token,
            entries,
            completed: false,
            recovery_required: true,
            warnings: vec![format!(
                "El borrado no terminó y se conservó una copia recuperable. Usa Deshacer para restaurarla: {}",
                failure.error
            )],
        }),
        Err(failure) => Err(failure.error.into()),
    }
}

fn restore_deleted_from_repo_linux(
    repo: &Path,
    token: &str,
) -> Result<FileOpOutcome, AgentRuntimeError> {
    restore_deleted_from_repo_linux_with_hook(repo, token, |_| Ok(()))
}

fn restore_deleted_from_repo_linux_with_hook(
    repo: &Path,
    token: &str,
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> Result<FileOpOutcome, AgentRuntimeError> {
    let manifest = read_bound_delete_manifest(repo, token)?;
    let moves = plan_delete_replay(repo, &manifest, ReplayDirection::Restore)?;
    let warnings = run_replay_batch_with_hook(token, moves, true, after_item)?;
    Ok(FileOpOutcome { warnings })
}

fn redo_deleted_from_repo_linux(
    repo: &Path,
    token: &str,
) -> Result<FileOpOutcome, AgentRuntimeError> {
    redo_deleted_from_repo_linux_with_hook(repo, token, |_| Ok(()))
}

fn redo_deleted_from_repo_linux_with_hook(
    repo: &Path,
    token: &str,
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> Result<FileOpOutcome, AgentRuntimeError> {
    let manifest = read_bound_delete_manifest(repo, token)?;
    let moves = plan_delete_replay(repo, &manifest, ReplayDirection::Redo)?;
    let warnings = run_replay_batch_with_hook(token, moves, false, after_item)?;
    Ok(FileOpOutcome { warnings })
}

struct AgentRuntimeError {
    category: String,
    message: String,
}

impl AgentRuntimeError {
    fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }

    fn from_agent_console(error: crate::agent_console::AgentConsoleError) -> Self {
        Self::new(error.category, error.message)
    }
}

impl From<crate::agent_console::AgentConsoleError> for AgentRuntimeError {
    fn from(error: crate::agent_console::AgentConsoleError) -> Self {
        Self::from_agent_console(error)
    }
}

impl From<crate::git::GitError> for AgentRuntimeError {
    fn from(error: crate::git::GitError) -> Self {
        Self::new(error.category(), error.to_string())
    }
}

impl From<crate::bus::commands::CommandError> for AgentRuntimeError {
    fn from(error: crate::bus::commands::CommandError) -> Self {
        Self::new(error.category, error.message)
    }
}

impl From<io::Error> for AgentRuntimeError {
    fn from(error: io::Error) -> Self {
        Self::new("io", error.to_string())
    }
}

impl From<AgentRuntimeError> for AgentError {
    fn from(error: AgentRuntimeError) -> Self {
        AgentError::new(AgentErrorCategory::MalformedResponse, error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::contract::ContentEncoding;
    use crate::git::test_fixtures::TempRepo;
    use crate::wsl_agent::protocol::{
        encode_agent_request, parse_agent_response_line, AgentRequest, RepoFsWatchConfig,
        PROTOCOL_VERSION,
    };

    fn injected_batch_failure() -> io::Error {
        io::Error::other("fallo de lote Linux inyectado")
    }

    fn assert_no_transaction_artifacts(parent: &Path) {
        let artifacts: Vec<PathBuf> = fs::read_dir(parent)
            .expect("read transaction parent")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().contains(".tinto-"))
            })
            .collect();
        assert!(
            artifacts.is_empty(),
            "transaction artifacts remained: {artifacts:?}"
        );
    }

    #[test]
    fn rejects_repo_outside_allowlist() {
        let request = AgentRequest::WorktreeDiff {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/other".into(),
            allowed_repos: vec!["/home/me/repo".into()],
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        assert_eq!(
            response,
            AgentResponse::Error {
                category: "repo-not-allowed".into(),
                message: "el repo no pertenece al workbench activo".into()
            }
        );
    }

    #[test]
    fn serve_request_lines_handles_multiple_requests_in_one_agent_process() {
        let request = encode_agent_request(&AgentRequest::handshake("test")).expect("encode");
        let input = format!("{request}{request}");
        let mut output = Vec::new();

        serve_request_lines(std::io::Cursor::new(input), &mut output).expect("serve");

        let text = String::from_utf8(output).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        for line in lines {
            assert!(matches!(
                parse_agent_response_line(line).expect("parse"),
                AgentResponse::Handshake { .. }
            ));
        }
    }

    #[test]
    fn serve_request_lines_returns_error_response_and_keeps_running() {
        let good = encode_agent_request(&AgentRequest::handshake("test")).expect("encode");
        let input = format!("{{not json}}\n{good}");
        let mut output = Vec::new();

        serve_request_lines(std::io::Cursor::new(input), &mut output).expect("serve");

        let text = String::from_utf8(output).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(matches!(
            parse_agent_response_line(lines[0]).expect("parse"),
            AgentResponse::Error { .. }
        ));
        assert!(matches!(
            parse_agent_response_line(lines[1]).expect("parse"),
            AgentResponse::Handshake { .. }
        ));
    }

    #[test]
    fn agent_binary_available_is_checked_inside_agent_runtime() {
        let request = AgentRequest::AgentBinaryAvailable {
            protocol_version: PROTOCOL_VERSION,
            agent_type: "codex".into(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");

        assert!(matches!(
            response,
            AgentResponse::AgentBinaryAvailable { .. } | AgentResponse::Error { .. }
        ));
    }

    #[test]
    fn agent_binary_available_rejects_unsupported_agent_inside_agent_runtime() {
        let request = AgentRequest::AgentBinaryAvailable {
            protocol_version: PROTOCOL_VERSION,
            agent_type: "powershell".into(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");

        assert_eq!(
            response,
            AgentResponse::Error {
                category: "unsupported_agent".into(),
                message: "agente no soportado: 'powershell'".into(),
            }
        );
    }

    #[test]
    fn snapshot_returns_repo_delta_for_allowed_repo() {
        let repo = TempRepo::with_initial_commit();
        repo.write("changed.txt", "hello\n");
        let request = AgentRequest::RepoSnapshot {
            protocol_version: PROTOCOL_VERSION,
            repos: vec![repo.path().to_path_buf()],
            subscriptions: Vec::new(),
            scope: RepoSnapshotScope::Everything,
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        let AgentResponse::RepoSnapshot { repos } = response else {
            panic!("expected snapshot");
        };
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo, repo.path());
        assert!(repos[0]
            .status
            .untracked
            .contains(&PathBuf::from("changed.txt")));
    }

    #[test]
    fn git_review_summary_returns_bounded_changed_files_for_allowed_repo() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "linea 1\nlinea 2\nlinea 3\nlinea 4\n");
        repo.write("new.txt", "new\n");
        fs::create_dir_all(repo.path().join("src")).expect("create src");
        repo.write("src/App.tsx", "<<<<<<< HEAD\nconflict\n");
        repo.write(".env", "TOKEN=value\n");
        repo.write("package-lock.json", "{}\n");
        let request = AgentRequest::GitReviewSummary {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        let AgentResponse::GitReviewSummary { summary } = response else {
            panic!("expected git review summary");
        };
        assert_eq!(summary.changed_files, 5);
        assert!(summary.files.iter().any(|file| file.contains("M base.txt")));
        assert!(summary.files.iter().any(|file| file.contains("?? new.txt")));
        assert_eq!(summary.truncated_count, 0);
        assert!(summary
            .findings
            .iter()
            .any(|finding| finding.title == "Conflict marker present"
                && finding.path == Some(PathBuf::from("src/App.tsx"))
                && finding.line == Some(1)));
        assert!(summary
            .findings
            .iter()
            .any(|finding| finding.title == "Sensitive path changed"
                && finding.path == Some(PathBuf::from(".env"))));
        assert!(summary.findings.iter().any(|finding| finding.title
            == "Lockfile changed without package manifest"
            && finding.path == Some(PathBuf::from("package-lock.json"))));
    }

    #[test]
    fn create_git_worktree_request_creates_detached_worktree_under_tinto_home() {
        let repo = TempRepo::with_initial_commit();
        let home = tempfile::tempdir().expect("home");
        let old_home = std::env::var_os("HOME");
        std::env::set_var("HOME", home.path());

        let request = AgentRequest::CreateGitWorktree {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            session_id: "sess-worktree".into(),
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        let AgentResponse::GitWorktreeCreated { path } = response else {
            panic!("expected worktree response");
        };
        assert!(path.starts_with(home.path().join(".tinto").join("worktrees")));
        assert!(path.join("base.txt").is_file());

        let remove = AgentRequest::RemoveGitWorktree {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            target: path.clone(),
        };
        let line = encode_agent_request(&remove).expect("encode remove");
        let response_line = respond_to_request_line(&line).expect("respond remove");
        let response = parse_agent_response_line(&response_line).expect("parse remove");
        assert_eq!(response, AgentResponse::Unit);
        assert!(!path.exists());

        if let Some(old_home) = old_home {
            std::env::set_var("HOME", old_home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn list_directory_returns_dirs_sorted_and_git_repo_marker() {
        let repo = TempRepo::with_initial_commit();
        std::fs::create_dir_all(repo.path().join("zeta")).unwrap();
        std::fs::create_dir_all(repo.path().join("Alpha")).unwrap();
        std::fs::write(repo.path().join("file.txt"), "not a dir").unwrap();

        let request = AgentRequest::ListDirectory {
            protocol_version: PROTOCOL_VERSION,
            path: Some(repo.path().to_path_buf()),
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        let AgentResponse::DirectoryListing { listing } = response else {
            panic!("expected directory listing");
        };
        assert_eq!(
            listing.path,
            repo.path().canonicalize().unwrap().display().to_string()
        );
        assert!(listing.is_git_repo);
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec![".git", "Alpha", "zeta"]
        );
    }

    #[test]
    fn snapshot_with_fs_events_returns_watchlisted_plane2_fingerprints_without_dot_git() {
        let repo = TempRepo::with_initial_commit();
        repo.write(".gitignore", ".env\n*.log\n");
        repo.write(".env", "SECRET=1\n");
        repo.write("debug.log", "ignored but not watched\n");
        repo.write("changed.txt", "tracked plane1\n");
        std::fs::write(repo.path().join(".git").join("hidden.txt"), "hidden").unwrap();

        let request = AgentRequest::RepoSnapshotWithFsEvents {
            protocol_version: PROTOCOL_VERSION,
            repos: vec![repo.path().to_path_buf()],
            subscriptions: Vec::new(),
            fs_watch: vec![RepoFsWatchConfig {
                repo: repo.path().to_path_buf(),
                patterns: vec![".env".into()],
            }],
            scope: RepoSnapshotScope::Everything,
        };
        let line = encode_agent_request(&request).expect("encode");
        let response_line = respond_to_request_line(&line).expect("respond");
        let response = parse_agent_response_line(&response_line).expect("parse");

        let AgentResponse::RepoSnapshotWithFsEvents {
            repos,
            fingerprints,
        } = response
        else {
            panic!("expected snapshot with fs events");
        };
        assert_eq!(repos.len(), 1);
        assert_eq!(fingerprints.len(), 1);
        assert!(fingerprints[0]
            .files
            .iter()
            .any(|file| file.path == Path::new(".env")));
        assert!(!fingerprints[0]
            .files
            .iter()
            .any(|file| file.path == Path::new("debug.log")));
        assert!(!fingerprints[0]
            .files
            .iter()
            .any(|file| file.path == Path::new("changed.txt")));
        assert!(!fingerprints[0]
            .files
            .iter()
            .any(|file| file.path.starts_with(".git")));
    }

    #[test]
    fn file_content_rejects_dot_git_and_navigation() {
        let repo = TempRepo::with_initial_commit();
        for path in [PathBuf::from("../x"), PathBuf::from(".git/config")] {
            let request = AgentRequest::FileContent {
                protocol_version: PROTOCOL_VERSION,
                repo: repo.path().to_path_buf(),
                allowed_repos: vec![repo.path().to_path_buf()],
                path,
            };
            let line = encode_agent_request(&request).expect("encode");
            let response_line = respond_to_request_line(&line).expect("respond");
            let response = parse_agent_response_line(&response_line).expect("parse");
            assert!(matches!(response, AgentResponse::Error { .. }));
        }
    }

    #[test]
    fn media_content_returns_base64_and_rejects_unsupported_extensions() {
        let repo = TempRepo::with_initial_commit();
        repo.write("image.png", "fake-png");
        repo.write("note.txt", "not media");

        let request = AgentRequest::MediaContent {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            path: "image.png".into(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::MediaContent { content } = response else {
            panic!("expected media content");
        };
        assert!(matches!(content.encoding, ContentEncoding::Base64));
        assert!(!content.truncated);

        let request = AgentRequest::MediaContent {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            path: "note.txt".into(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert!(matches!(response, AgentResponse::Error { .. }));
    }

    #[test]
    fn gitleaks_status_and_config_creation_are_agent_side() {
        let repo = TempRepo::with_initial_commit();

        let request = AgentRequest::GitleaksSetupStatus {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert!(matches!(
            response,
            AgentResponse::GitleaksSetupStatus { .. }
        ));

        let request = AgentRequest::CreateGitleaksConfig {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert_eq!(response, AgentResponse::Unit);
        assert!(repo.path().join(".gitleaks.toml").is_file());
    }

    #[test]
    fn agent_checkpoint_scan_and_revert_are_agent_side() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "before\n");

        let create = AgentRequest::AgentCheckpointCreate {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            session_id: "sess-wsl".into(),
            created_at_ms: 1,
            ephemeral: false,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&create).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::AgentCheckpoint { checkpoint } = response else {
            panic!("expected checkpoint");
        };

        repo.write("base.txt", "after\n");
        repo.write("created.txt", "new\n");

        let scan = AgentRequest::AgentCheckpointScan {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
            timestamp_ms: 10,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&scan).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::AgentChangeLog { changes } = response else {
            panic!("expected change log");
        };
        assert!(changes
            .iter()
            .any(|change| change.path == Path::new("base.txt")));
        assert!(changes
            .iter()
            .any(|change| change.path == Path::new("created.txt")));

        let revert_file = AgentRequest::AgentCheckpointRevertFile {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
            path: "base.txt".into(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&revert_file).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert_eq!(response, AgentResponse::Unit);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "before\n"
        );
        assert!(repo.path().join("created.txt").exists());

        repo.write("base.txt", "after again\n");
        let revert = AgentRequest::AgentCheckpointRevert {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&revert).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert_eq!(response, AgentResponse::Unit);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "before\n"
        );
        assert!(!repo.path().join("created.txt").exists());
    }

    #[test]
    fn agent_ephemeral_checkpoint_can_be_created_and_removed_agent_side() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "current\n");
        let create = AgentRequest::AgentCheckpointCreate {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            session_id: "sess-wsl-safety".into(),
            created_at_ms: 1,
            ephemeral: true,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&create).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::AgentCheckpoint { checkpoint } = response else {
            panic!("expected checkpoint");
        };
        assert!(checkpoint.checkpoint_dir.exists());

        let remove = AgentRequest::AgentCheckpointRemove {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&remove).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");

        assert_eq!(response, AgentResponse::Unit);
        assert!(!checkpoint.checkpoint_dir.exists());
    }

    #[test]
    fn copy_within_repo_reports_conflict_then_overwrites() {
        let repo = TempRepo::with_initial_commit();
        repo.write("src.txt", "one\n");
        std::fs::create_dir_all(repo.path().join("dest")).expect("dest");
        repo.write("dest/src.txt", "old\n");

        let request = AgentRequest::CopyWithinRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            sources: vec!["src.txt".into()],
            dest_dir: "dest".into(),
            overwrite: false,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::CopyResult { result } = response else {
            panic!("expected copy result");
        };
        assert_eq!(result.conflicts.len(), 1);

        let request = AgentRequest::CopyWithinRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            sources: vec!["src.txt".into()],
            dest_dir: "dest".into(),
            overwrite: true,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::CopyResult { result } = response else {
            panic!("expected copy result");
        };
        assert!(result.conflicts.is_empty());
        assert_eq!(
            std::fs::read_to_string(repo.path().join("dest/src.txt")).unwrap(),
            "one\n"
        );
    }

    #[test]
    fn copy_to_repo_places_an_external_file_in_a_nested_directory() {
        let repo = TempRepo::with_initial_commit();
        std::fs::create_dir_all(repo.path().join("src/assets")).expect("nested destination");
        let repo_path = repo.path().canonicalize().expect("canonical repo");
        let external = tempfile::tempdir().expect("external temp dir");
        let source = external.path().join("dropped.txt");
        std::fs::write(&source, "from host\n").expect("external source");
        let request = AgentRequest::CopyToRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo_path.clone(),
            allowed_repos: vec![repo_path],
            dest_dir: "src/assets".into(),
            sources: vec![source],
            overwrite: false,
        };

        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::CopyResult { result } = response else {
            panic!("expected copy result");
        };

        assert!(result.conflicts.is_empty());
        assert_eq!(result.copied, vec![PathBuf::from("src/assets/dropped.txt")]);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("src/assets/dropped.txt")).unwrap(),
            "from host\n"
        );
    }

    #[test]
    fn export_from_repo_safely_overwrites_an_existing_destination() {
        let repo = TempRepo::with_initial_commit();
        repo.write("report.txt", "new report\n");
        let repo_path = repo.path().canonicalize().expect("canonical repo");
        let destination = tempfile::tempdir().expect("export destination");
        std::fs::write(destination.path().join("report.txt"), "old report\n")
            .expect("existing destination");
        let request = AgentRequest::ExportFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo_path.clone(),
            allowed_repos: vec![repo_path],
            sources: vec!["report.txt".into()],
            dest_dir: destination.path().to_path_buf(),
        };

        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");

        assert_eq!(
            response,
            AgentResponse::FileOpOutcome {
                result: FileOpOutcome::default(),
            }
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("report.txt")).unwrap(),
            "new report\n"
        );
        assert_eq!(
            std::fs::read_to_string(destination.path().join("report.txt")).unwrap(),
            "new report\n"
        );
        assert!(std::fs::read_dir(destination.path())
            .expect("read destination")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".tinto-")));
    }

    #[test]
    fn export_from_repo_rolls_back_a_partial_staging_copy_in_the_linux_runtime() {
        let fixture =
            tempfile::tempdir_in(std::env::current_dir().expect("current test directory"))
                .expect("runtime export fixture");
        let repo_path = fixture.path().join("repo");
        let destination = fixture.path().join("destination");
        std::fs::create_dir_all(&repo_path).expect("repo directory");
        std::fs::create_dir_all(&destination).expect("export destination");
        let source_before: Vec<u8> = (0..=255).cycle().take(8 * 1024).collect();
        let source_file = repo_path.join("report.bin");
        std::fs::write(&source_file, &source_before).expect("source file");
        let repo_path = repo_path.canonicalize().expect("canonical repo");
        let destination_file = destination.join("report.bin");
        let destination_before: Vec<u8> = (0..=127).rev().cycle().take(4 * 1024).collect();
        std::fs::write(&destination_file, &destination_before).expect("existing destination");

        let result = export_from_repo_linux_with_copy(
            &repo_path,
            &[PathBuf::from("report.bin")],
            &destination,
            |src, dest| {
                transactional_copy_with_stage_copy(src, dest, |from, stage| {
                    let bytes = std::fs::read(from)?;
                    std::fs::write(stage, &bytes[..bytes.len() / 2])?;
                    Err(io::Error::other("fallo WSL inyectado"))
                })
            },
        );

        assert!(result.is_err());
        assert_eq!(std::fs::read(&source_file).unwrap(), source_before);
        assert_eq!(
            std::fs::read(&destination_file).unwrap(),
            destination_before
        );
        assert!(std::fs::read_dir(&destination)
            .expect("read destination")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".tinto-")));
    }

    #[test]
    fn move_within_repo_moves_a_file_into_a_nested_directory() {
        let repo = TempRepo::with_initial_commit();
        repo.write("README.md", "move me\n");
        std::fs::create_dir_all(repo.path().join("docs/reference")).expect("nested destination");
        let repo_path = repo.path().canonicalize().expect("canonical repo");
        let request = AgentRequest::MoveWithinRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo_path.clone(),
            allowed_repos: vec![repo_path],
            sources: vec!["README.md".into()],
            dest_dir: "docs/reference".into(),
            overwrite: false,
        };

        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&request).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::CopyResult { result } = response else {
            panic!("expected move result");
        };

        assert!(result.conflicts.is_empty());
        assert_eq!(
            result.copied,
            vec![PathBuf::from("docs/reference/README.md")]
        );
        assert!(!repo.path().join("README.md").exists());
        assert_eq!(
            std::fs::read_to_string(repo.path().join("docs/reference/README.md")).unwrap(),
            "move me\n"
        );
    }

    #[test]
    fn delete_restore_redo_roundtrip() {
        let repo = TempRepo::with_initial_commit();
        repo.write("gone.txt", "bye\n");
        let delete = AgentRequest::DeleteFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            sources: vec!["gone.txt".into()],
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&delete).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        let AgentResponse::DeleteResult { result } = response else {
            panic!("expected delete result");
        };
        assert!(!repo.path().join("gone.txt").exists());

        let restore = AgentRequest::RestoreDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            token: result.token.clone(),
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&restore).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert_eq!(
            response,
            AgentResponse::FileOpOutcome {
                result: FileOpOutcome::default(),
            }
        );
        assert!(repo.path().join("gone.txt").exists());

        let redo = AgentRequest::RedoDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.path().to_path_buf(),
            allowed_repos: vec![repo.path().to_path_buf()],
            token: result.token,
        };
        let response = parse_agent_response_line(
            &respond_to_request_line(&encode_agent_request(&redo).expect("encode"))
                .expect("respond"),
        )
        .expect("parse");
        assert_eq!(
            response,
            AgentResponse::FileOpOutcome {
                result: FileOpOutcome::default(),
            }
        );
        assert!(!repo.path().join("gone.txt").exists());
    }

    #[test]
    fn linux_copy_batch_rolls_back_after_the_first_object() {
        let fixture = tempfile::tempdir().expect("linux copy fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir_all(&source).expect("source dir");
        fs::create_dir_all(&destination).expect("destination dir");
        let source_one = vec![0x71; 4096];
        let source_two = vec![0x72; 8192];
        let destination_one = vec![0xC1; 1024];
        let destination_two = vec![0xC2; 2048];
        fs::write(source.join("one.bin"), &source_one).expect("source one");
        fs::write(source.join("two.bin"), &source_two).expect("source two");
        fs::write(destination.join("one.bin"), &destination_one).expect("destination one");
        fs::write(destination.join("two.bin"), &destination_two).expect("destination two");

        let result = run_linux_copy_or_move_batch(
            fixture.path(),
            vec![
                (source.join("one.bin"), destination.join("one.bin")),
                (source.join("two.bin"), destination.join("two.bin")),
            ],
            false,
            |completed| {
                if completed == 1 {
                    Err(injected_batch_failure())
                } else {
                    Ok(())
                }
            },
        );

        assert!(result.is_err());
        assert_eq!(fs::read(source.join("one.bin")).unwrap(), source_one);
        assert_eq!(fs::read(source.join("two.bin")).unwrap(), source_two);
        assert_eq!(
            fs::read(destination.join("one.bin")).unwrap(),
            destination_one
        );
        assert_eq!(
            fs::read(destination.join("two.bin")).unwrap(),
            destination_two
        );
        assert_no_transaction_artifacts(&source);
        assert_no_transaction_artifacts(&destination);
    }

    #[test]
    fn linux_move_batch_rolls_back_after_the_first_object() {
        let fixture = tempfile::tempdir().expect("linux move fixture");
        let source = fixture.path().join("source");
        let destination = fixture.path().join("destination");
        fs::create_dir_all(&source).expect("source dir");
        fs::create_dir_all(&destination).expect("destination dir");
        let source_one = vec![0x81; 4096];
        let source_two = vec![0x82; 8192];
        let destination_one = vec![0xD1; 1024];
        let destination_two = vec![0xD2; 2048];
        fs::write(source.join("one.bin"), &source_one).expect("source one");
        fs::write(source.join("two.bin"), &source_two).expect("source two");
        fs::write(destination.join("one.bin"), &destination_one).expect("destination one");
        fs::write(destination.join("two.bin"), &destination_two).expect("destination two");

        let result = run_linux_copy_or_move_batch(
            fixture.path(),
            vec![
                (source.join("one.bin"), destination.join("one.bin")),
                (source.join("two.bin"), destination.join("two.bin")),
            ],
            true,
            |completed| {
                if completed == 1 {
                    Err(injected_batch_failure())
                } else {
                    Ok(())
                }
            },
        );

        assert!(result.is_err());
        assert_eq!(fs::read(source.join("one.bin")).unwrap(), source_one);
        assert_eq!(fs::read(source.join("two.bin")).unwrap(), source_two);
        assert_eq!(
            fs::read(destination.join("one.bin")).unwrap(),
            destination_one
        );
        assert_eq!(
            fs::read(destination.join("two.bin")).unwrap(),
            destination_two
        );
        assert_no_transaction_artifacts(&source);
        assert_no_transaction_artifacts(&destination);
    }

    #[test]
    fn linux_delete_batch_rolls_back_after_the_first_object() {
        let fixture = tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
            .expect("linux delete fixture");
        let repo = fixture.path().join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        let one = vec![0x91; 4096];
        let two = vec![0x92; 8192];
        fs::write(repo.join("one.bin"), &one).expect("source one");
        fs::write(repo.join("two.bin"), &two).expect("source two");
        let repo_path = repo.canonicalize().expect("canonical repo");

        let result = delete_from_repo_linux_with_hook(
            &repo_path,
            &[PathBuf::from("one.bin"), PathBuf::from("two.bin")],
            |completed| {
                if completed == 1 {
                    Err(injected_batch_failure())
                } else {
                    Ok(())
                }
            },
        );

        assert!(result.is_err());
        assert_eq!(fs::read(repo.join("one.bin")).unwrap(), one);
        assert_eq!(fs::read(repo.join("two.bin")).unwrap(), two);
        assert_no_transaction_artifacts(&repo);
    }

    #[test]
    fn linux_restore_and_redo_failures_leave_the_token_retriable() {
        let fixture = tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
            .expect("linux replay fixture");
        let repo = fixture.path().join("repo");
        fs::create_dir_all(&repo).expect("repo dir");
        let one = vec![0xA1; 4096];
        let two = vec![0xA2; 8192];
        fs::write(repo.join("one.bin"), &one).expect("source one");
        fs::write(repo.join("two.bin"), &two).expect("source two");
        let repo_path = repo.canonicalize().expect("canonical repo");
        let deleted = delete_from_repo_linux(
            &repo_path,
            &[PathBuf::from("one.bin"), PathBuf::from("two.bin")],
        )
        .unwrap_or_else(|error| panic!("initial delete: {}", error.message));
        let backup_root = undo_backup_root(&deleted.token).expect("backup root");
        let objects = backup_root.join("objects");

        let restore =
            restore_deleted_from_repo_linux_with_hook(&repo_path, &deleted.token, |completed| {
                if completed == 1 {
                    Err(injected_batch_failure())
                } else {
                    Ok(())
                }
            });
        assert!(restore.is_err());
        assert!(!repo.join("one.bin").exists());
        assert!(!repo.join("two.bin").exists());
        assert_eq!(fs::read(objects.join("0")).unwrap(), one);
        assert_eq!(fs::read(objects.join("1")).unwrap(), two);
        assert_no_transaction_artifacts(&repo);
        assert_no_transaction_artifacts(&objects);

        restore_deleted_from_repo_linux(&repo_path, &deleted.token)
            .unwrap_or_else(|error| panic!("restore retry: {}", error.message));
        let redo =
            redo_deleted_from_repo_linux_with_hook(&repo_path, &deleted.token, |completed| {
                if completed == 1 {
                    Err(injected_batch_failure())
                } else {
                    Ok(())
                }
            });
        assert!(redo.is_err());
        assert_eq!(fs::read(repo.join("one.bin")).unwrap(), one);
        assert_eq!(fs::read(repo.join("two.bin")).unwrap(), two);
        assert!(!objects.join("0").exists());
        assert!(!objects.join("1").exists());
        assert_no_transaction_artifacts(&repo);
        assert_no_transaction_artifacts(&objects);

        redo_deleted_from_repo_linux(&repo_path, &deleted.token)
            .unwrap_or_else(|error| panic!("redo retry: {}", error.message));
        fs::remove_dir_all(backup_root).expect("cleanup undo fixture");
    }
}
