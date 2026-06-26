use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

use ignore::WalkBuilder;

use crate::agent_console::checkpoint::{
    create_checkpoint, revert_checkpoint, scan_change_log, CheckpointConfig,
};
use crate::agent_console::validation::validate_agent_type;
use crate::bus::commands::{
    file_content_from_bytes, gitleaks_setup_status, list_repo_tree_capped,
    read_file_content_bounded, read_media_content_bounded, validate_media_path,
    write_repo_gitleaks_config,
};
use crate::bus::contract::{
    GitleaksInstallResult, RepoDelta, RepoErrorClass, RepoErrorState, RepoMetrics,
    SubscriptionTarget, REPO_TREE_MAX_ENTRIES,
};
use crate::bus::secret_scan;
use crate::bus::{git_error_state, recalc_blocking, RecalcScope};
use crate::file_ops::commands::{
    copy_recursive, move_path, read_delete_manifest, run_copy_blocking, run_move_blocking,
    undo_backup_root, write_delete_manifest, CopyResult, DeleteResult, DeletedEntry,
};
use crate::file_ops::{safe_join, FileConflict, FileConflictKind};
use crate::git::{Git2Engine, GitEngine};
use crate::paths::{Classification, PathClassifier};
use uuid::Uuid;

use super::protocol::{
    encode_agent_response, parse_agent_request_line, AgentError, AgentErrorCategory, AgentRequest,
    AgentResponse, FileFingerprint, RepoFileFingerprintSnapshot, RepoFsWatchConfig,
    WslDirectoryEntry, WslDirectoryListing,
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
            ..
        } => AgentResponse::RepoSnapshot {
            repos: repos
                .iter()
                .map(|repo| repo_delta(repo, &subscriptions))
                .collect(),
        },
        AgentRequest::RepoSnapshotWithFsEvents {
            repos,
            subscriptions,
            fs_watch,
            ..
        } => {
            let deltas = repos
                .iter()
                .map(|repo| repo_delta(repo, &subscriptions))
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
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            let checkpoint = create_checkpoint(
                &repo,
                &session_id,
                created_at_ms,
                &CheckpointConfig::default(),
            )?;
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
            export_from_repo_linux(&repo, &sources, &dest_dir)?;
            Ok(AgentResponse::Unit)
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
            restore_deleted_from_repo_linux(&repo, &token)?;
            Ok(AgentResponse::Unit)
        }),
        AgentRequest::RedoDeletedFromRepo {
            repo,
            allowed_repos,
            token,
            ..
        } => with_allowed_repo(&repo, &allowed_repos, || {
            redo_deleted_from_repo_linux(&repo, &token)?;
            Ok(AgentResponse::Unit)
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

fn repo_delta(repo: &Path, subscriptions: &[SubscriptionTarget]) -> RepoDelta {
    let subs: Vec<SubscriptionTarget> = subscriptions
        .iter()
        .filter(|target| same_linux_path(&target.repo, repo))
        .cloned()
        .collect();

    match recalc_blocking(repo, RecalcScope::Everything, &subs) {
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
            signals: outcome.signals,
            secret_findings: outcome.secret_findings,
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
            signals: Vec::new(),
            secret_findings: Vec::new(),
            subscribed_diffs: None,
        },
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
    let output = Command::new("sh")
        .arg("-lc")
        .arg("command -v -- \"$1\"")
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
        });
    }
    let copied = run_copy_blocking(repo.to_path_buf(), to_copy)?;
    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
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
        });
    }
    let copied = if move_sources {
        run_move_blocking(repo.to_path_buf(), pairs)?
    } else {
        run_copy_blocking(repo.to_path_buf(), pairs)?
    };
    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
    })
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
) -> Result<(), AgentRuntimeError> {
    if !dest_dir.is_dir() {
        return Err(AgentRuntimeError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio en el filesystem",
        ));
    }
    let mut srcs = Vec::with_capacity(sources.len());
    for src_rel in sources {
        let src_abs = safe_join(repo, src_rel)?;
        if !src_abs.exists() {
            return Err(AgentRuntimeError::new(
                "source-missing",
                format!("no existe {} en el repo", src_rel.display()),
            ));
        }
        srcs.push(src_abs);
    }
    for src_abs in &srcs {
        let name = src_abs.file_name().unwrap_or_default();
        let dest = dest_dir.join(name);
        if dest.exists() {
            if dest.is_dir() {
                fs::remove_dir_all(&dest)?;
            } else {
                fs::remove_file(&dest)?;
            }
        }
        copy_recursive(src_abs, &dest)?;
    }
    Ok(())
}

fn delete_from_repo_linux(
    repo: &Path,
    sources: &[PathBuf],
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
        &DeleteResult {
            token: token.clone(),
            entries: entries.clone(),
        },
    )?;
    fs::create_dir_all(&objects_root)?;
    for (index, (_, target, _)) in filtered.into_iter().enumerate() {
        move_path(&target, &objects_root.join(index.to_string()))?;
    }
    Ok(DeleteResult { token, entries })
}

fn restore_deleted_from_repo_linux(repo: &Path, token: &str) -> Result<(), AgentRuntimeError> {
    let backup_root = undo_backup_root(token)?;
    let manifest = read_delete_manifest(&backup_root)?;
    let objects_root = backup_root.join("objects");
    for entry in manifest.entries {
        let dest = safe_join(repo, &entry.path)?;
        if dest == repo {
            return Err(AgentRuntimeError::new(
                "restore-root-forbidden",
                "no se puede restaurar sobre el root del repo",
            ));
        }
        if dest.exists() {
            return Err(AgentRuntimeError::new(
                "restore-failed",
                format!("{} ya existe", entry.path.display()),
            ));
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        move_path(&objects_root.join(entry.backup_name), &dest)?;
    }
    Ok(())
}

fn redo_deleted_from_repo_linux(repo: &Path, token: &str) -> Result<(), AgentRuntimeError> {
    let backup_root = undo_backup_root(token)?;
    let manifest = read_delete_manifest(&backup_root)?;
    let objects_root = backup_root.join("objects");
    fs::create_dir_all(&objects_root)?;
    for entry in manifest.entries {
        let src = safe_join(repo, &entry.path)?;
        if src == repo {
            return Err(AgentRuntimeError::new(
                "redo-root-forbidden",
                "no se puede rehacer el borrado del root del repo",
            ));
        }
        if !src.exists() {
            return Err(AgentRuntimeError::new(
                "redo-delete-failed",
                format!("{} no existe", entry.path.display()),
            ));
        }
        let backup = objects_root.join(entry.backup_name);
        if backup.exists() {
            return Err(AgentRuntimeError::new(
                "redo-delete-failed",
                format!("backup {} ya existe", backup.display()),
            ));
        }
        move_path(&src, &backup)?;
    }
    Ok(())
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
            AgentResponse::AgentBinaryAvailable { .. }
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
        assert_eq!(response, AgentResponse::Unit);
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
        assert_eq!(response, AgentResponse::Unit);
        assert!(!repo.path().join("gone.txt").exists());
    }
}
