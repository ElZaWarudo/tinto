use std::{
    collections::HashSet,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

use git2::{Repository, StatusOptions};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::bus::contract::{
    AgentSessionChange, AgentSessionChangeKind, AgentSessionCheckpoint, AgentSessionCheckpointType,
};

use super::AgentConsoleError;

const DEFAULT_RETENTION_PER_REPO: usize = 50;
const DEFAULT_MAX_CHECKPOINT_BYTES: u64 = 100 * 1024 * 1024;
const DEFAULT_MAX_REPO_BYTES: u64 = 500 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub retention_per_repo: usize,
    pub max_checkpoint_bytes: u64,
    pub max_repo_bytes: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            retention_per_repo: DEFAULT_RETENTION_PER_REPO,
            max_checkpoint_bytes: DEFAULT_MAX_CHECKPOINT_BYTES,
            max_repo_bytes: DEFAULT_MAX_REPO_BYTES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointRecord {
    pub contract: AgentSessionCheckpoint,
    pub repo: PathBuf,
    pub session_id: String,
    pub checkpoint_dir: PathBuf,
    pub created_at_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CheckpointMetadata {
    repo: PathBuf,
    session_id: String,
    created_at_ms: u64,
    checkpoint_type: AgentSessionCheckpointType,
    git_hash: Option<String>,
    snapshot_files: Vec<PathBuf>,
    #[serde(default)]
    dirty_created_files: Vec<PathBuf>,
    #[serde(default)]
    dirty_deleted_files: Vec<PathBuf>,
}

struct GitCheckpointState {
    head_hash: String,
    snapshot_files: Vec<PathBuf>,
    created_files: Vec<PathBuf>,
    deleted_files: Vec<PathBuf>,
}

pub fn create_checkpoint(
    repo: &Path,
    session_id: &str,
    created_at_ms: u64,
    config: &CheckpointConfig,
) -> Result<CheckpointRecord, AgentConsoleError> {
    let repo = canonical_repo(repo)?;
    let repo_dir = checkpoints_repo_dir(&repo)?;
    let checkpoint_dir = repo_dir.join(session_id);
    if checkpoint_dir.exists() {
        fs::remove_dir_all(&checkpoint_dir).map_err(io_error)?;
    }
    fs::create_dir_all(&checkpoint_dir).map_err(io_error)?;

    let contract = match git_checkpoint_state(&repo)? {
        Some(state) if state.snapshot_files.is_empty() && state.deleted_files.is_empty() => {
            let contract = AgentSessionCheckpoint {
                checkpoint_type: AgentSessionCheckpointType::GitRef,
                git_hash: Some(state.head_hash),
                snapshot_files: Vec::new(),
            };
            write_metadata(
                &checkpoint_dir,
                &repo,
                session_id,
                created_at_ms,
                &contract,
                &[],
                &[],
            )?;
            contract
        }
        Some(state) => {
            let (contract, created_files, deleted_files) =
                snapshot_dirty_git_filesystem(&repo, &checkpoint_dir, config, state)?;
            write_metadata(
                &checkpoint_dir,
                &repo,
                session_id,
                created_at_ms,
                &contract,
                &created_files,
                &deleted_files,
            )?;
            contract
        }
        None => {
            let contract = snapshot_filesystem(&repo, &checkpoint_dir, config)?;
            write_metadata(
                &checkpoint_dir,
                &repo,
                session_id,
                created_at_ms,
                &contract,
                &[],
                &[],
            )?;
            contract
        }
    };

    prune_checkpoints(&repo_dir, config.retention_per_repo)?;
    enforce_repo_budget(&repo_dir, config.max_repo_bytes)?;

    Ok(CheckpointRecord {
        contract,
        repo,
        session_id: session_id.to_string(),
        checkpoint_dir,
        created_at_ms,
    })
}

pub fn revert_checkpoint(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => revert_git(record),
        AgentSessionCheckpointType::FsSnapshot => revert_fs(record),
    }
}

pub fn revert_checkpoint_file(
    record: &CheckpointRecord,
    path: &Path,
) -> Result<(), AgentConsoleError> {
    validate_checkpoint_relative_path(path)?;
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => revert_git_file(record, path),
        AgentSessionCheckpointType::FsSnapshot => revert_fs_file(record, path),
    }
}

pub fn scan_change_log(
    record: &CheckpointRecord,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => scan_git_changes(&record.repo, timestamp_ms),
        AgentSessionCheckpointType::FsSnapshot => scan_fs_changes(record, timestamp_ms),
    }
}

fn git_checkpoint_state(repo: &Path) -> Result<Option<GitCheckpointState>, AgentConsoleError> {
    let Ok(repository) = Repository::open(repo) else {
        return Ok(None);
    };
    let head = match repository.head() {
        Ok(head) => head,
        Err(_) => return Ok(None),
    };
    let Some(oid) = head.target() else {
        return Ok(None);
    };

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repository
        .statuses(Some(&mut opts))
        .map_err(|e| AgentConsoleError::new("checkpoint_git_failed", e.to_string()))?;
    if statuses.is_empty() {
        return Ok(Some(GitCheckpointState {
            head_hash: oid.to_string(),
            snapshot_files: Vec::new(),
            created_files: Vec::new(),
            deleted_files: Vec::new(),
        }));
    }

    let mut snapshot_files = HashSet::new();
    let mut created_files = HashSet::new();
    let mut deleted_files = HashSet::new();
    for entry in statuses.iter() {
        let Some(path) = entry.path() else { continue };
        let rel = PathBuf::from(path);
        let status = entry.status();
        if status.is_index_renamed()
            || status.is_wt_renamed()
            || status.is_index_typechange()
            || status.is_wt_typechange()
        {
            return Ok(None);
        }
        if repo.join(&rel).is_file() {
            if status.is_index_new() || status.is_wt_new() {
                created_files.insert(rel.clone());
            }
            snapshot_files.insert(rel);
        } else if status.is_index_deleted() || status.is_wt_deleted() {
            deleted_files.insert(rel);
        } else {
            return Ok(None);
        }
    }
    let mut snapshot_files = snapshot_files.into_iter().collect::<Vec<_>>();
    let mut created_files = created_files.into_iter().collect::<Vec<_>>();
    let mut deleted_files = deleted_files.into_iter().collect::<Vec<_>>();
    snapshot_files.sort();
    created_files.sort();
    deleted_files.sort();
    Ok(Some(GitCheckpointState {
        head_hash: oid.to_string(),
        snapshot_files,
        created_files,
        deleted_files,
    }))
}

fn snapshot_filesystem(
    repo: &Path,
    checkpoint_dir: &Path,
    config: &CheckpointConfig,
) -> Result<AgentSessionCheckpoint, AgentConsoleError> {
    let snapshot_root = checkpoint_dir.join("files");
    fs::create_dir_all(&snapshot_root).map_err(io_error)?;

    let files = collect_repo_files(repo)?;
    let total_bytes = files.iter().try_fold(0u64, |acc, rel| {
        file_size(repo.join(rel)).map(|size| acc + size)
    })?;
    if total_bytes > config.max_checkpoint_bytes {
        let _ = fs::remove_dir_all(checkpoint_dir);
        return Err(AgentConsoleError::new(
            "checkpoint_too_large",
            format!(
                "checkpoint exceeds {} MB",
                config.max_checkpoint_bytes / 1024 / 1024
            ),
        ));
    }

    for rel in &files {
        let source = repo.join(rel);
        let target = snapshot_root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(&source, &target).map_err(io_error)?;
    }

    Ok(AgentSessionCheckpoint {
        checkpoint_type: AgentSessionCheckpointType::FsSnapshot,
        git_hash: None,
        snapshot_files: files,
    })
}

fn snapshot_dirty_git_filesystem(
    repo: &Path,
    checkpoint_dir: &Path,
    config: &CheckpointConfig,
    state: GitCheckpointState,
) -> Result<(AgentSessionCheckpoint, Vec<PathBuf>, Vec<PathBuf>), AgentConsoleError> {
    let snapshot_root = checkpoint_dir.join("files");
    fs::create_dir_all(&snapshot_root).map_err(io_error)?;

    let total_bytes = state.snapshot_files.iter().try_fold(0u64, |acc, rel| {
        file_size(repo.join(rel)).map(|size| acc + size)
    })?;
    if total_bytes > config.max_checkpoint_bytes {
        let _ = fs::remove_dir_all(checkpoint_dir);
        return Err(AgentConsoleError::new(
            "checkpoint_too_large",
            format!(
                "checkpoint exceeds {} MB",
                config.max_checkpoint_bytes / 1024 / 1024
            ),
        ));
    }

    for rel in &state.snapshot_files {
        let source = repo.join(rel);
        let target = snapshot_root.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(&source, &target).map_err(io_error)?;
    }

    Ok((
        AgentSessionCheckpoint {
            checkpoint_type: AgentSessionCheckpointType::FsSnapshot,
            git_hash: Some(state.head_hash),
            snapshot_files: state.snapshot_files,
        },
        state.created_files,
        state.deleted_files,
    ))
}

fn revert_git(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    let Some(hash) = &record.contract.git_hash else {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "git checkpoint has no hash",
        ));
    };
    run_git(&record.repo, &["checkout", hash, "--", "."])?;
    run_git(&record.repo, &["clean", "-fd"])?;
    Ok(())
}

fn revert_git_file(record: &CheckpointRecord, rel: &Path) -> Result<(), AgentConsoleError> {
    let Some(hash) = &record.contract.git_hash else {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "git checkpoint has no hash",
        ));
    };
    if git_file_exists_at(&record.repo, hash, rel)? {
        let rel_text = rel.to_string_lossy().into_owned();
        run_git(&record.repo, &["checkout", hash, "--", &rel_text])?;
    } else {
        validate_current_path_ancestors(&record.repo, rel)?;
        let path = record.repo.join(rel);
        if path.exists() {
            fs::remove_file(path).map_err(io_error)?;
        }
    }
    Ok(())
}

fn revert_fs(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    if record.contract.git_hash.is_some() {
        return revert_dirty_git_snapshot(record);
    }
    let snapshot_root = record.checkpoint_dir.join("files");
    let snapshot_files: HashSet<PathBuf> = record.contract.snapshot_files.iter().cloned().collect();
    let current_files: HashSet<PathBuf> = collect_repo_files(&record.repo)?.into_iter().collect();

    for rel in current_files.difference(&snapshot_files) {
        validate_current_path_ancestors(&record.repo, rel)?;
        let path = record.repo.join(rel);
        if path.exists() {
            fs::remove_file(path).map_err(io_error)?;
        }
    }

    for rel in &record.contract.snapshot_files {
        let source = snapshot_root.join(rel);
        let target = prepare_restore_target(&record.repo, rel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(&source, &target).map_err(io_error)?;
    }

    Ok(())
}

fn revert_dirty_git_snapshot(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    let Some(hash) = &record.contract.git_hash else {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "dirty git checkpoint has no hash",
        ));
    };
    let metadata = read_metadata(&record.checkpoint_dir)?;
    run_git(&record.repo, &["checkout", hash, "--", "."])?;
    run_git(&record.repo, &["clean", "-fd"])?;

    let snapshot_root = record.checkpoint_dir.join("files");
    for rel in &record.contract.snapshot_files {
        let source = snapshot_root.join(rel);
        let target = prepare_restore_target(&record.repo, rel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(source, target).map_err(io_error)?;
    }
    for rel in metadata.dirty_deleted_files {
        validate_current_path_ancestors(&record.repo, &rel)?;
        let target = record.repo.join(rel);
        if target.exists() {
            fs::remove_file(target).map_err(io_error)?;
        }
    }
    Ok(())
}

fn revert_fs_file(record: &CheckpointRecord, rel: &Path) -> Result<(), AgentConsoleError> {
    let snapshot_root = record.checkpoint_dir.join("files");
    if record
        .contract
        .snapshot_files
        .iter()
        .any(|snapshot_path| snapshot_path == rel)
    {
        let source = snapshot_root.join(rel);
        let target = prepare_restore_target(&record.repo, rel)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::copy(source, target).map_err(io_error)?;
    } else {
        validate_current_path_ancestors(&record.repo, rel)?;
        let target = record.repo.join(rel);
        if target.exists() {
            fs::remove_file(target).map_err(io_error)?;
        }
    }
    Ok(())
}

fn scan_git_changes(
    repo: &Path,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let repository = Repository::open(repo)
        .map_err(|e| AgentConsoleError::new("checkpoint_git_failed", e.to_string()))?;
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repository
        .statuses(Some(&mut opts))
        .map_err(|e| AgentConsoleError::new("checkpoint_git_failed", e.to_string()))?;

    let mut changes = Vec::new();
    for entry in statuses.iter() {
        let Some(path) = entry.path() else { continue };
        let status = entry.status();
        let kind = if status.is_wt_new() || status.is_index_new() {
            AgentSessionChangeKind::Created
        } else if status.is_wt_deleted() || status.is_index_deleted() {
            AgentSessionChangeKind::Removed
        } else {
            AgentSessionChangeKind::Modified
        };
        changes.push(AgentSessionChange {
            path: PathBuf::from(path),
            kind,
            timestamp_ms,
        });
    }
    changes.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| kind_name(a.kind).cmp(kind_name(b.kind)))
    });
    Ok(changes)
}

fn scan_fs_changes(
    record: &CheckpointRecord,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    if record.contract.git_hash.is_some() {
        return scan_dirty_git_snapshot_changes(record, timestamp_ms);
    }
    let snapshot_root = record.checkpoint_dir.join("files");
    let snapshot_files: HashSet<PathBuf> = record.contract.snapshot_files.iter().cloned().collect();
    let current_files: HashSet<PathBuf> = collect_repo_files(&record.repo)?.into_iter().collect();

    let mut changes = Vec::new();
    for rel in current_files.difference(&snapshot_files) {
        changes.push(change(rel, AgentSessionChangeKind::Created, timestamp_ms));
    }
    for rel in snapshot_files.difference(&current_files) {
        changes.push(change(rel, AgentSessionChangeKind::Removed, timestamp_ms));
    }
    for rel in snapshot_files.intersection(&current_files) {
        let before = fs::read(snapshot_root.join(rel)).map_err(io_error)?;
        let after = fs::read(record.repo.join(rel)).map_err(io_error)?;
        if before != after {
            changes.push(change(rel, AgentSessionChangeKind::Modified, timestamp_ms));
        }
    }
    changes.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| kind_name(a.kind).cmp(kind_name(b.kind)))
    });
    Ok(changes)
}

fn scan_dirty_git_snapshot_changes(
    record: &CheckpointRecord,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let metadata = read_metadata(&record.checkpoint_dir)?;
    let state = git_checkpoint_state(&record.repo)?.unwrap_or(GitCheckpointState {
        head_hash: record.contract.git_hash.clone().unwrap_or_default(),
        snapshot_files: Vec::new(),
        created_files: Vec::new(),
        deleted_files: Vec::new(),
    });
    let baseline_files: HashSet<PathBuf> = record.contract.snapshot_files.iter().cloned().collect();
    let baseline_created: HashSet<PathBuf> = metadata.dirty_created_files.into_iter().collect();
    let baseline_deleted: HashSet<PathBuf> = metadata.dirty_deleted_files.into_iter().collect();
    let current_files: HashSet<PathBuf> = state.snapshot_files.into_iter().collect();
    let current_created: HashSet<PathBuf> = state.created_files.into_iter().collect();
    let current_deleted: HashSet<PathBuf> = state.deleted_files.into_iter().collect();
    let snapshot_root = record.checkpoint_dir.join("files");
    let mut changes = Vec::new();
    let mut seen = HashSet::new();

    for rel in current_files.difference(&baseline_files) {
        if seen.insert(rel.clone()) {
            let kind = if current_created.contains(rel) {
                AgentSessionChangeKind::Created
            } else {
                AgentSessionChangeKind::Modified
            };
            changes.push(change(rel, kind, timestamp_ms));
        }
    }
    for rel in current_deleted.difference(&baseline_deleted) {
        if seen.insert(rel.clone()) {
            changes.push(change(rel, AgentSessionChangeKind::Removed, timestamp_ms));
        }
    }
    for rel in baseline_files.difference(&current_files) {
        if seen.insert(rel.clone()) {
            let kind = if baseline_created.contains(rel) {
                AgentSessionChangeKind::Removed
            } else {
                AgentSessionChangeKind::Modified
            };
            changes.push(change(rel, kind, timestamp_ms));
        }
    }
    for rel in baseline_deleted.difference(&current_deleted) {
        if seen.insert(rel.clone()) {
            changes.push(change(rel, AgentSessionChangeKind::Modified, timestamp_ms));
        }
    }
    for rel in baseline_files.intersection(&current_files) {
        let before = fs::read(snapshot_root.join(rel)).map_err(io_error)?;
        let after = fs::read(record.repo.join(rel)).map_err(io_error)?;
        if before != after && seen.insert(rel.clone()) {
            changes.push(change(rel, AgentSessionChangeKind::Modified, timestamp_ms));
        }
    }

    changes.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| kind_name(a.kind).cmp(kind_name(b.kind)))
    });
    Ok(changes)
}

fn change(path: &Path, kind: AgentSessionChangeKind, timestamp_ms: u64) -> AgentSessionChange {
    AgentSessionChange {
        path: path.to_path_buf(),
        kind,
        timestamp_ms,
    }
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), AgentConsoleError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| AgentConsoleError::new("revert_failed", e.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AgentConsoleError::new(
            "revert_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn git_file_exists_at(repo: &Path, hash: &str, rel: &Path) -> Result<bool, AgentConsoleError> {
    let rel_text = rel.to_string_lossy();
    let spec = format!("{hash}:{rel_text}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e", &spec])
        .output()
        .map_err(|e| AgentConsoleError::new("revert_failed", e.to_string()))?;
    Ok(output.status.success())
}

fn collect_repo_files(repo: &Path) -> Result<Vec<PathBuf>, AgentConsoleError> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(repo)
        .follow_links(false)
        .hidden(false)
        .build();
    for entry in walker {
        let entry = entry.map_err(|e| AgentConsoleError::new("io", e.to_string()))?;
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(repo) else {
            continue;
        };
        if rel.as_os_str().is_empty() || has_git_component(rel) {
            continue;
        }
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            files.push(rel.to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn has_git_component(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new(".git"))
}

fn validate_checkpoint_relative_path(path: &Path) -> Result<(), AgentConsoleError> {
    if path.is_absolute() || has_navigation_component(path) {
        return Err(AgentConsoleError::new(
            "path-traversal",
            "el path se sale del repositorio",
        ));
    }
    if path.as_os_str().is_empty() || has_git_component(path) {
        return Err(AgentConsoleError::new(
            "path-forbidden",
            "el directorio .git no se expone",
        ));
    }
    Ok(())
}

fn prepare_restore_target(repo: &Path, rel: &Path) -> Result<PathBuf, AgentConsoleError> {
    validate_current_path_ancestors(repo, rel)?;
    let target = repo.join(rel);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::remove_file(&target).map_err(io_error)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    Ok(target)
}

fn validate_current_path_ancestors(repo: &Path, rel: &Path) -> Result<(), AgentConsoleError> {
    let mut current = repo.to_path_buf();
    let mut components = rel.components().peekable();
    while let Some(component) = components.next() {
        if components.peek().is_none() {
            break;
        }
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AgentConsoleError::new(
                    "path-forbidden",
                    "checkpoint revert refuses symlink ancestors",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn has_navigation_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    })
}

fn write_metadata(
    checkpoint_dir: &Path,
    repo: &Path,
    session_id: &str,
    created_at_ms: u64,
    contract: &AgentSessionCheckpoint,
    dirty_created_files: &[PathBuf],
    dirty_deleted_files: &[PathBuf],
) -> Result<(), AgentConsoleError> {
    let metadata = CheckpointMetadata {
        repo: repo.to_path_buf(),
        session_id: session_id.to_string(),
        created_at_ms,
        checkpoint_type: contract.checkpoint_type,
        git_hash: contract.git_hash.clone(),
        snapshot_files: contract.snapshot_files.clone(),
        dirty_created_files: dirty_created_files.to_vec(),
        dirty_deleted_files: dirty_deleted_files.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&metadata)
        .map_err(|e| AgentConsoleError::new("checkpoint_metadata_failed", e.to_string()))?;
    fs::write(checkpoint_dir.join("metadata.json"), json).map_err(io_error)
}

fn read_metadata(checkpoint_dir: &Path) -> Result<CheckpointMetadata, AgentConsoleError> {
    let bytes = fs::read(checkpoint_dir.join("metadata.json")).map_err(io_error)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| AgentConsoleError::new("checkpoint_metadata_failed", e.to_string()))
}

fn prune_checkpoints(repo_dir: &Path, keep: usize) -> Result<(), AgentConsoleError> {
    if keep == 0 {
        return Ok(());
    }
    let mut dirs = checkpoint_dirs(repo_dir)?;
    dirs.sort_by_key(|(_, modified)| *modified);
    while dirs.len() > keep {
        if let Some((path, _)) = dirs.first() {
            fs::remove_dir_all(path).map_err(io_error)?;
        }
        dirs.remove(0);
    }
    Ok(())
}

fn enforce_repo_budget(repo_dir: &Path, max_bytes: u64) -> Result<(), AgentConsoleError> {
    let mut total = dir_size(repo_dir)?;
    if total <= max_bytes {
        return Ok(());
    }

    let mut dirs = checkpoint_dirs(repo_dir)?;
    dirs.sort_by_key(|(_, modified)| *modified);

    while total > max_bytes && dirs.len() > 1 {
        let (path, _) = dirs.remove(0);
        let reclaimed = dir_size(&path)?;
        fs::remove_dir_all(path).map_err(io_error)?;
        total = total.saturating_sub(reclaimed);
    }

    if total <= max_bytes {
        Ok(())
    } else {
        Err(AgentConsoleError::new(
            "checkpoint_repo_budget_exceeded",
            format!("repo checkpoints exceed {} MB", max_bytes / 1024 / 1024),
        ))
    }
}

fn checkpoint_dirs(
    repo_dir: &Path,
) -> Result<Vec<(PathBuf, std::time::SystemTime)>, AgentConsoleError> {
    let mut dirs = Vec::new();
    if !repo_dir.exists() {
        return Ok(dirs);
    }
    for entry in fs::read_dir(repo_dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_dir() {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            dirs.push((entry.path(), modified));
        }
    }
    Ok(dirs)
}

fn checkpoints_repo_dir(repo: &Path) -> Result<PathBuf, AgentConsoleError> {
    let home = crate::runtime_paths::user_home_dir().ok_or_else(|| {
        AgentConsoleError::new("checkpoint_home_unavailable", "home directory unavailable")
    })?;
    Ok(home
        .join(".tinto")
        .join("checkpoints")
        .join(repo_hash(repo)))
}

fn repo_hash(repo: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    repo.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn canonical_repo(repo: &Path) -> Result<PathBuf, AgentConsoleError> {
    let repo = repo
        .canonicalize()
        .map_err(|_| AgentConsoleError::repo_not_found())?;
    if repo.is_dir() {
        Ok(repo)
    } else {
        Err(AgentConsoleError::repo_not_found())
    }
}

fn dir_size(path: &Path) -> Result<u64, AgentConsoleError> {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .try_fold(0u64, |acc, entry| {
            let entry = entry.map_err(|e| AgentConsoleError::new("io", e.to_string()))?;
            if entry.file_type().is_file() {
                Ok(acc
                    + entry
                        .metadata()
                        .map_err(|e| AgentConsoleError::new("io", e.to_string()))?
                        .len())
            } else {
                Ok(acc)
            }
        })
}

fn file_size(path: PathBuf) -> Result<u64, AgentConsoleError> {
    fs::metadata(path).map(|m| m.len()).map_err(io_error)
}

fn io_error(error: std::io::Error) -> AgentConsoleError {
    let category = match error.kind() {
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        _ => "io",
    };
    AgentConsoleError::new(category, error.to_string())
}

fn kind_name(kind: AgentSessionChangeKind) -> &'static str {
    match kind {
        AgentSessionChangeKind::Created => "created",
        AgentSessionChangeKind::Modified => "modified",
        AgentSessionChangeKind::Removed => "removed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_fixtures::TempRepo;

    #[test]
    fn checkpoint_creation_records_git_head_when_repo_is_clean() {
        let repo = TempRepo::with_initial_commit();
        let record =
            create_checkpoint(repo.path(), "sess-clean", 1, &CheckpointConfig::default()).unwrap();

        assert_eq!(
            record.contract.checkpoint_type,
            AgentSessionCheckpointType::GitRef
        );
        assert_eq!(
            record.contract.git_hash.as_deref(),
            Some(repo.head_id().as_str())
        );
        assert!(record.contract.snapshot_files.is_empty());
    }

    #[test]
    fn dirty_git_repo_uses_filesystem_snapshot() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty\n");
        repo.write("untracked.txt", "new\n");

        let record =
            create_checkpoint(repo.path(), "sess-dirty", 1, &CheckpointConfig::default()).unwrap();

        assert_eq!(
            record.contract.checkpoint_type,
            AgentSessionCheckpointType::FsSnapshot
        );
        assert_eq!(
            record.contract.git_hash.as_deref(),
            Some(repo.head_id().as_str())
        );
        assert_eq!(
            record.contract.snapshot_files,
            vec![PathBuf::from("base.txt"), PathBuf::from("untracked.txt")]
        );
        assert!(record
            .contract
            .snapshot_files
            .contains(&PathBuf::from("base.txt")));
        assert!(record.checkpoint_dir.join("files/base.txt").is_file());
        assert!(record.checkpoint_dir.join("files/untracked.txt").is_file());
    }

    #[test]
    fn filesystem_snapshot_respects_gitignore_for_large_ignored_dirs() {
        let repo = TempRepo::with_initial_commit();
        repo.write(".gitignore", "node_modules/\n");
        fs::create_dir_all(repo.path().join("node_modules/pkg")).unwrap();
        fs::write(
            repo.path().join("node_modules/pkg/big.bin"),
            vec![1u8; 2048],
        )
        .unwrap();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig {
            max_checkpoint_bytes: 1024,
            ..CheckpointConfig::default()
        };

        let record = create_checkpoint(repo.path(), "sess-ignore", 1, &config).unwrap();

        assert_eq!(
            record.contract.checkpoint_type,
            AgentSessionCheckpointType::FsSnapshot
        );
        assert!(record
            .contract
            .snapshot_files
            .contains(&PathBuf::from("base.txt")));
        assert!(!record
            .contract
            .snapshot_files
            .iter()
            .any(|path| path.starts_with("node_modules")));
    }

    #[test]
    fn filesystem_revert_restores_modified_and_deletes_created_files() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty before\n");
        repo.write("baseline-new.txt", "baseline untracked\n");
        let record = create_checkpoint(
            repo.path(),
            "sess-fs-revert",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();

        repo.write("base.txt", "after\n");
        repo.write("created.txt", "new\n");
        fs::remove_file(repo.path().join("baseline-new.txt")).unwrap();
        revert_checkpoint(&record).unwrap();

        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "dirty before\n"
        );
        assert_eq!(
            fs::read_to_string(repo.path().join("baseline-new.txt")).unwrap(),
            "baseline untracked\n"
        );
        assert!(!repo.path().join("created.txt").exists());
        revert_checkpoint(&record).unwrap();
        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "dirty before\n"
        );
    }

    #[test]
    fn filesystem_file_revert_restores_only_selected_file() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty before\n");
        repo.write("other.txt", "other before\n");
        let record = create_checkpoint(
            repo.path(),
            "sess-file-revert",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();

        repo.write("base.txt", "after\n");
        repo.write("other.txt", "other after\n");
        repo.write("created.txt", "new\n");

        revert_checkpoint_file(&record, Path::new("base.txt")).unwrap();
        revert_checkpoint_file(&record, Path::new("created.txt")).unwrap();

        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "dirty before\n"
        );
        assert_eq!(
            fs::read_to_string(repo.path().join("other.txt")).unwrap(),
            "other after\n"
        );
        assert!(!repo.path().join("created.txt").exists());
    }

    #[test]
    fn file_revert_rejects_paths_outside_repo_or_dot_git() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty before\n");
        let record = create_checkpoint(
            repo.path(),
            "sess-file-reject",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();

        for path in [Path::new("../base.txt"), Path::new(".git/config")] {
            let error = revert_checkpoint_file(&record, path).unwrap_err();
            assert!(matches!(
                error.category.as_str(),
                "path-traversal" | "path-forbidden"
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn file_revert_rejects_symlink_ancestor_escape() {
        let repo = TempRepo::with_initial_commit();
        repo.write("dir/base.txt", "before\n");
        let outside = tempfile::tempdir().unwrap();
        let outside_target = outside.path().join("base.txt");
        fs::write(&outside_target, "outside\n").unwrap();
        let record = create_checkpoint(
            repo.path(),
            "sess-symlink-escape",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        fs::remove_dir_all(repo.path().join("dir")).unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("dir")).unwrap();

        let error = revert_checkpoint_file(&record, Path::new("dir/base.txt")).unwrap_err();

        assert_eq!(error.category, "path-forbidden");
        assert_eq!(fs::read_to_string(outside_target).unwrap(), "outside\n");
    }

    #[test]
    fn retention_deletes_old_checkpoints() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig {
            retention_per_repo: 5,
            ..CheckpointConfig::default()
        };
        let mut last_dir = None;
        for i in 0..6 {
            let record = create_checkpoint(repo.path(), &format!("sess-{i}"), i, &config).unwrap();
            last_dir = Some(record.checkpoint_dir);
        }

        let repo_dir = last_dir.unwrap().parent().unwrap().to_path_buf();
        let dirs = checkpoint_dirs(&repo_dir).unwrap();
        assert_eq!(dirs.len(), 5);
    }

    #[test]
    fn repo_budget_prunes_old_checkpoints_before_failing_start() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig {
            retention_per_repo: 10,
            max_checkpoint_bytes: 2048,
            max_repo_bytes: 1800,
        };
        let mut newest_dir = None;

        for i in 0..4 {
            repo.write("base.txt", &format!("dirty {i}\n{}", "x".repeat(512)));
            let record =
                create_checkpoint(repo.path(), &format!("budget-{i}"), i, &config).unwrap();
            newest_dir = Some(record.checkpoint_dir);
        }

        let newest_dir = newest_dir.unwrap();
        let repo_dir = newest_dir.parent().unwrap().to_path_buf();
        let dirs = checkpoint_dirs(&repo_dir).unwrap();
        let total = dir_size(&repo_dir).unwrap();

        assert!(newest_dir.exists());
        assert!(dirs.len() < 4);
        assert!(total <= config.max_repo_bytes);
    }

    #[test]
    fn change_log_detects_fs_created_modified_removed() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty before\n");
        repo.write("removed.txt", "gone soon\n");
        let record =
            create_checkpoint(repo.path(), "sess-log", 1, &CheckpointConfig::default()).unwrap();
        repo.write("base.txt", "dirty after\n");
        repo.write("created.txt", "new\n");
        fs::remove_file(repo.path().join("removed.txt")).unwrap();

        let changes = scan_change_log(&record, 10).unwrap();
        assert!(
            changes
                .iter()
                .any(|c| c.path == Path::new("base.txt")
                    && c.kind == AgentSessionChangeKind::Modified)
        );
        assert!(changes.iter().any(
            |c| c.path == Path::new("created.txt") && c.kind == AgentSessionChangeKind::Created
        ));
        assert!(changes.iter().any(
            |c| c.path == Path::new("removed.txt") && c.kind == AgentSessionChangeKind::Removed
        ));
    }
}
