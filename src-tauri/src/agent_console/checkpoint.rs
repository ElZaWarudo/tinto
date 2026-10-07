use std::{
    collections::HashSet,
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use git2::{Repository, StatusOptions};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::bus::contract::{
    AgentSessionChange, AgentSessionChangeKind, AgentSessionCheckpoint, AgentSessionCheckpointType,
};

use super::AgentConsoleError;
#[cfg(target_os = "windows")]
use crate::windows_process::hide_console;

const DEFAULT_RETENTION_PER_REPO: usize = 50;
const DEFAULT_MAX_REPO_BYTES: u64 = 500 * 1024 * 1024;
const EPHEMERAL_GIT_INDEX_BACKUP: &str = "git-index";
const EPHEMERAL_GIT_INDEX_ABSENT: &str = "git-index.absent";
const EPHEMERAL_NON_GIT: &str = "non-git";
static EPHEMERAL_INDEX_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    pub retention_per_repo: usize,
    pub max_repo_bytes: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            retention_per_repo: DEFAULT_RETENTION_PER_REPO,
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
    #[serde(default)]
    pub ephemeral: bool,
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
    /// Content-addressed snapshot of the whole working tree, stored as a git
    /// tree in the shadow store. Absent on legacy file-copy checkpoints.
    #[serde(default)]
    shadow_tree: Option<String>,
}

struct GitCheckpointState {
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
    create_checkpoint_inner(repo, session_id, created_at_ms, config, true, false)
}

pub fn create_ephemeral_checkpoint(
    repo: &Path,
    session_id: &str,
    created_at_ms: u64,
    config: &CheckpointConfig,
) -> Result<CheckpointRecord, AgentConsoleError> {
    create_checkpoint_inner(repo, session_id, created_at_ms, config, false, false)
}

/// A job-boundary snapshot for Delivery. It is always a shadow tree, even on
/// a clean repo, so the tree id doubles as the candidate identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeSnapshot {
    pub checkpoint: CheckpointRecord,
    pub head: Option<String>,
    pub tree: String,
    /// Paths that differ from the `compare_to` tree, when one was given.
    pub changes: Vec<AgentSessionChange>,
}

impl WorktreeSnapshot {
    /// `<HEAD>:<tree>`, the format `run_state.py candidate` uses.
    pub fn candidate_id(&self) -> String {
        format!("{}:{}", self.head.as_deref().unwrap_or("none"), self.tree)
    }
}

pub fn snapshot_worktree(
    repo: &Path,
    name: &str,
    created_at_ms: u64,
    config: &CheckpointConfig,
    compare_to: Option<&str>,
) -> Result<WorktreeSnapshot, AgentConsoleError> {
    let checkpoint = create_checkpoint_inner(repo, name, created_at_ms, config, true, true)?;
    let tree = read_metadata(&checkpoint.checkpoint_dir)?
        .shadow_tree
        .ok_or_else(|| {
            AgentConsoleError::new("checkpoint_invalid", "the snapshot has no shadow tree")
        })?;
    let changes = match compare_to {
        Some(from) => diff_snapshot_trees(&checkpoint, from, &tree, created_at_ms)?,
        None => Vec::new(),
    };
    Ok(WorktreeSnapshot {
        head: checkpoint.contract.git_hash.clone(),
        checkpoint,
        tree,
        changes,
    })
}

fn diff_snapshot_trees(
    record: &CheckpointRecord,
    from: &str,
    to: &str,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let repo_dir = record.checkpoint_dir.parent().ok_or_else(|| {
        AgentConsoleError::new(
            "checkpoint_invalid",
            "checkpoint has no repository directory",
        )
    })?;
    let args = [
        "diff-tree",
        "-r",
        "--name-status",
        "-z",
        "--no-renames",
        from,
        to,
    ];
    let mut command = git_command(&repo_dir.join(SHADOW_STORE_DIR));
    command.args(args);
    let output = run_git_command(command, &args, None)?;
    let mut fields = output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    let mut changes = Vec::new();
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let kind = match status.first() {
            Some(b'A') => AgentSessionChangeKind::Created,
            Some(b'D') => AgentSessionChangeKind::Removed,
            _ => AgentSessionChangeKind::Modified,
        };
        changes.push(change(
            Path::new(&*String::from_utf8_lossy(path)),
            kind,
            timestamp_ms,
        ));
    }
    Ok(changes)
}

fn create_checkpoint_inner(
    repo: &Path,
    session_id: &str,
    created_at_ms: u64,
    config: &CheckpointConfig,
    enforce_retention: bool,
    force_shadow: bool,
) -> Result<CheckpointRecord, AgentConsoleError> {
    let repo = canonical_repo(repo)?;
    if !enforce_retention {
        ensure_ephemeral_checkpoint_supported(&repo)?;
    }
    let repo_dir = checkpoints_repo_dir(&repo)?;
    let checkpoint_dir = repo_dir.join(session_id);
    if checkpoint_dir.exists() {
        fs::remove_dir_all(&checkpoint_dir).map_err(io_error)?;
    }
    fs::create_dir_all(&checkpoint_dir).map_err(io_error)?;

    let repository = Repository::open(&repo).ok();
    let clean_head = match &repository {
        Some(repository) if !force_shadow => clean_git_head(repository)?,
        _ => None,
    };
    let contract = match clean_head {
        Some(head_hash) => {
            let contract = AgentSessionCheckpoint {
                checkpoint_type: AgentSessionCheckpointType::GitRef,
                git_hash: Some(head_hash),
                snapshot_files: Vec::new(),
            };
            write_metadata(
                &checkpoint_dir,
                &repo,
                session_id,
                created_at_ms,
                &contract,
                None,
            )?;
            contract
        }
        None => {
            let snapshot = shadow_snapshot(
                &repo,
                repository.as_ref(),
                &repo_dir,
                &checkpoint_dir,
                session_id,
            );
            let (contract, tree) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    let _ = fs::remove_dir_all(&checkpoint_dir);
                    return Err(error);
                }
            };
            write_metadata(
                &checkpoint_dir,
                &repo,
                session_id,
                created_at_ms,
                &contract,
                Some(tree),
            )?;
            contract
        }
    };

    if enforce_retention {
        prune_checkpoints(&repo_dir, config.retention_per_repo)?;
        enforce_repo_budget(&repo_dir, config.max_repo_bytes)?;
    } else if let Err(error) = snapshot_ephemeral_git_index(&repo, &checkpoint_dir) {
        let _ = fs::remove_dir_all(&checkpoint_dir);
        return Err(error);
    }

    Ok(CheckpointRecord {
        contract,
        repo,
        session_id: session_id.to_string(),
        checkpoint_dir,
        created_at_ms,
        ephemeral: !enforce_retention,
    })
}

/// The start checkpoint a session left on disk, for undoing a session that
/// is no longer live (e.g. reopened from history after a restart).
pub fn load_session_checkpoint(
    repo: &Path,
    session_id: &str,
) -> Result<CheckpointRecord, AgentConsoleError> {
    validate_session_id(session_id)?;
    let repo = canonical_repo(repo)?;
    let checkpoint_dir = checkpoints_repo_dir(&repo)?.join(session_id);
    let metadata = read_metadata(&checkpoint_dir).map_err(|_| {
        AgentConsoleError::new(
            "checkpoint_not_found",
            "el punto de control de esta sesión ya no existe",
        )
    })?;
    Ok(CheckpointRecord {
        contract: AgentSessionCheckpoint {
            checkpoint_type: metadata.checkpoint_type,
            git_hash: metadata.git_hash,
            snapshot_files: metadata.snapshot_files,
        },
        repo,
        session_id: session_id.to_string(),
        checkpoint_dir,
        created_at_ms: metadata.created_at_ms,
        ephemeral: false,
    })
}

fn validate_session_id(session_id: &str) -> Result<(), AgentConsoleError> {
    let valid = !session_id.is_empty()
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "identificador de sesión no válido",
        ))
    }
}

pub fn remove_ephemeral_checkpoint(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    if !record.ephemeral {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "checkpoint cleanup requires an ephemeral checkpoint",
        ));
    }
    if !record.checkpoint_dir.exists() {
        return Ok(());
    }
    let repo = canonical_repo(&record.repo)?;
    let repo_dir = checkpoints_repo_dir(&repo)?;
    let checkpoint_dir = record.checkpoint_dir.canonicalize().map_err(io_error)?;
    let expected_parent = repo_dir.canonicalize().map_err(io_error)?;
    if checkpoint_dir.parent() != Some(expected_parent.as_path()) {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "checkpoint cleanup target is outside the managed repository directory",
        ));
    }
    remove_checkpoint_dir(&expected_parent, &checkpoint_dir)
}

pub fn revert_checkpoint(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => revert_git(record),
        AgentSessionCheckpointType::FsSnapshot => match shadow_of(record)? {
            Some(shadow) => revert_shadow(record, &shadow),
            None => revert_fs(record),
        },
    }?;
    if record.ephemeral {
        restore_ephemeral_git_index(record)?;
    }
    Ok(())
}

pub fn revert_checkpoint_file(
    record: &CheckpointRecord,
    path: &Path,
) -> Result<(), AgentConsoleError> {
    validate_checkpoint_relative_path(path)?;
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => revert_git_file(record, path),
        AgentSessionCheckpointType::FsSnapshot => match shadow_of(record)? {
            Some(shadow) => revert_shadow_file(record, &shadow, path),
            None => revert_fs_file(record, path),
        },
    }
}

pub fn scan_change_log(
    record: &CheckpointRecord,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    match record.contract.checkpoint_type {
        AgentSessionCheckpointType::GitRef => scan_git_changes(&record.repo, timestamp_ms),
        AgentSessionCheckpointType::FsSnapshot => match shadow_of(record)? {
            Some(shadow) => scan_shadow_changes(&shadow, timestamp_ms),
            None => scan_fs_changes(record, timestamp_ms),
        },
    }
}

fn ensure_ephemeral_checkpoint_supported(repo: &Path) -> Result<(), AgentConsoleError> {
    let Ok(repository) = Repository::open(repo) else {
        let walker = WalkBuilder::new(repo)
            .follow_links(false)
            .hidden(false)
            .build();
        for entry in walker {
            let entry = entry.map_err(|error| AgentConsoleError::new("io", error.to_string()))?;
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(repo) else {
                continue;
            };
            if relative.as_os_str().is_empty() || has_git_component(relative) {
                continue;
            }
            if entry
                .file_type()
                .is_some_and(|file_type| file_type.is_symlink())
            {
                return Err(ephemeral_symlink_error(relative));
            }
        }
        return Ok(());
    };

    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repository
        .statuses(Some(&mut options))
        .map_err(|error| AgentConsoleError::new("checkpoint_git_failed", error.to_string()))?;
    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().map_err(|_| {
            AgentConsoleError::new(
                "checkpoint_unsupported",
                "the safety checkpoint cannot preserve a non-UTF-8 Git path",
            )
        })?;
        let relative = Path::new(path);
        if status.is_index_typechange() || status.is_wt_typechange() {
            return Err(ephemeral_symlink_error(relative));
        }
        match fs::symlink_metadata(repo.join(relative)) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ephemeral_symlink_error(relative));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

fn ephemeral_symlink_error(path: &Path) -> AgentConsoleError {
    AgentConsoleError::new(
        "checkpoint_unsupported",
        format!(
            "the safety checkpoint cannot preserve symlink state for {}",
            path.display()
        ),
    )
}

fn snapshot_ephemeral_git_index(
    repo: &Path,
    checkpoint_dir: &Path,
) -> Result<(), AgentConsoleError> {
    let Ok(repository) = Repository::open(repo) else {
        fs::write(checkpoint_dir.join(EPHEMERAL_NON_GIT), []).map_err(io_error)?;
        return Ok(());
    };
    let index = repository.path().join("index");
    match fs::symlink_metadata(&index) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::copy(&index, checkpoint_dir.join(EPHEMERAL_GIT_INDEX_BACKUP)).map_err(io_error)?;
        }
        Ok(_) => {
            return Err(AgentConsoleError::new(
                "checkpoint_unsupported",
                "the safety checkpoint cannot preserve a non-file Git index",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(checkpoint_dir.join(EPHEMERAL_GIT_INDEX_ABSENT), []).map_err(io_error)?;
        }
        Err(error) => return Err(io_error(error)),
    }
    Ok(())
}

fn restore_ephemeral_git_index(record: &CheckpointRecord) -> Result<(), AgentConsoleError> {
    let backup = record.checkpoint_dir.join(EPHEMERAL_GIT_INDEX_BACKUP);
    let absent = record.checkpoint_dir.join(EPHEMERAL_GIT_INDEX_ABSENT);
    let non_git = record.checkpoint_dir.join(EPHEMERAL_NON_GIT);
    let state_count = usize::from(backup.is_file())
        + usize::from(absent.is_file())
        + usize::from(non_git.is_file());
    if state_count != 1 {
        return Err(AgentConsoleError::new(
            "checkpoint_invalid",
            "ephemeral checkpoint has no unambiguous Git index state",
        ));
    }
    if non_git.is_file() {
        return Ok(());
    }

    let repository = Repository::open(&record.repo)
        .map_err(|error| AgentConsoleError::new("checkpoint_git_failed", error.to_string()))?;
    let index = repository.path().join("index");
    if backup.is_file() {
        replace_file_transactionally(&backup, &index).map_err(io_error)?;
        return Ok(());
    }

    match fs::symlink_metadata(&index) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(index).map_err(io_error)?;
        }
        Ok(_) => {
            return Err(AgentConsoleError::new(
                "checkpoint_unsupported",
                "the safety checkpoint refuses to remove a non-file Git index",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    Ok(())
}

fn replace_file_transactionally(source: &Path, destination: &Path) -> io::Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git index destination has no parent",
        )
    })?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("index");
    let suffix = EPHEMERAL_INDEX_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stage = parent.join(format!(
        ".{file_name}.tinto-restore-{}-{suffix}",
        std::process::id()
    ));
    let previous = parent.join(format!(
        ".{file_name}.tinto-previous-{}-{suffix}",
        std::process::id()
    ));

    if let Err(error) = fs::copy(source, &stage) {
        let _ = fs::remove_file(&stage);
        return Err(error);
    }
    let had_destination = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if let Err(error) = fs::rename(destination, &previous) {
                let _ = fs::remove_file(&stage);
                return Err(error);
            }
            true
        }
        Ok(_) => {
            let _ = fs::remove_file(&stage);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Git index destination is not a regular file",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = fs::remove_file(&stage);
            return Err(error);
        }
    };

    if let Err(error) = fs::rename(&stage, destination) {
        if had_destination {
            let _ = fs::rename(&previous, destination);
        }
        let _ = fs::remove_file(&stage);
        return Err(error);
    }
    if had_destination {
        fs::remove_file(previous)?;
    }
    Ok(())
}

fn git_checkpoint_state(repo: &Path) -> Result<Option<GitCheckpointState>, AgentConsoleError> {
    let Ok(repository) = Repository::open(repo) else {
        return Ok(None);
    };
    let head = match repository.head() {
        Ok(head) => head,
        Err(_) => return Ok(None),
    };
    if head.target().is_none() {
        return Ok(None);
    }

    let mut opts = StatusOptions::new();
    // A submodule is its own repository: its dirty working tree shows up here
    // as one directory entry, which forced a full-tree copy (node_modules
    // included) that always exceeded the size limit.
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    let statuses = repository
        .statuses(Some(&mut opts))
        .map_err(|e| AgentConsoleError::new("checkpoint_git_failed", e.to_string()))?;
    if statuses.is_empty() {
        return Ok(Some(GitCheckpointState {
            snapshot_files: Vec::new(),
            created_files: Vec::new(),
            deleted_files: Vec::new(),
        }));
    }

    let mut snapshot_files = HashSet::new();
    let mut created_files = HashSet::new();
    let mut deleted_files = HashSet::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
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
        snapshot_files,
        created_files,
        deleted_files,
    }))
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
        let Ok(path) = entry.path() else { continue };
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

// ---- Content-addressed checkpoints ----------------------------------------
//
// A dirty git working tree is snapshotted as a git tree in a per-repo shadow
// store (a bare repository under ~/.tinto/checkpoints/<repo>/store.git):
// - content is compressed and deduplicated, so a turn that changed 3 files
//   adds 3 blobs, and files identical to commits cost nothing (the repo's own
//   object database is a read-only alternate);
// - each snapshot starts from the previous one's index, whose stat cache
//   means only files changed since then are read;
// - the user's repository (index, refs, objects, hooks) is never written.

const SHADOW_STORE_DIR: &str = "store.git";
const SHADOW_INDEX: &str = "index";
const SHADOW_LATEST_INDEX: &str = "latest.index";
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const INHERITED_GIT_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
];

/// HEAD when the working tree matches it exactly (submodules aside); such a
/// checkpoint needs no snapshot at all.
fn clean_git_head(repository: &Repository) -> Result<Option<String>, AgentConsoleError> {
    let Some(head) = repository.head().ok().and_then(|head| head.target()) else {
        return Ok(None);
    };
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .exclude_submodules(true);
    let statuses = repository
        .statuses(Some(&mut opts))
        .map_err(|e| AgentConsoleError::new("checkpoint_git_failed", e.to_string()))?;
    Ok(statuses.is_empty().then(|| head.to_string()))
}

struct ShadowGit {
    store: PathBuf,
    worktree: PathBuf,
    index: PathBuf,
}

struct ShadowCheckpoint {
    git: ShadowGit,
}

impl ShadowGit {
    fn run(&self, args: &[&str]) -> Result<Vec<u8>, AgentConsoleError> {
        self.run_with_input(args, None)
    }

    fn run_with_input(
        &self,
        args: &[&str],
        input: Option<&[u8]>,
    ) -> Result<Vec<u8>, AgentConsoleError> {
        let mut command = git_command(&self.store);
        command
            .env("GIT_WORK_TREE", git_path(&self.worktree))
            .env("GIT_INDEX_FILE", git_path(&self.index))
            .current_dir(&self.worktree)
            .args(args);
        run_git_command(command, args, input)
    }
}

/// `git` bound to the shadow store, isolated from inherited repository
/// variables. Snapshots are byte-exact (see the store's info/attributes) and
/// never trigger background maintenance.
fn git_command(store: &Path) -> Command {
    let mut command = Command::new(crate::git::git_program());
    for name in INHERITED_GIT_ENV {
        command.env_remove(name);
    }
    command.env("GIT_DIR", git_path(store)).args([
        "-c",
        "core.autocrlf=false",
        "-c",
        "core.safecrlf=false",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.quotepath=false",
        "-c",
        "gc.auto=0",
        "-c",
        "advice.addEmbeddedRepo=false",
        "--literal-pathspecs",
    ]);
    #[cfg(target_os = "windows")]
    hide_console(&mut command);
    command
}

fn run_git_command(
    mut command: Command,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Vec<u8>, AgentConsoleError> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        AgentConsoleError::new(
            "checkpoint_git_failed",
            format!("no se pudo ejecutar git: {error}"),
        )
    })?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        use std::io::Write;
        stdin.write_all(input).map_err(io_error)?;
    }
    let output = child.wait_with_output().map_err(io_error)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(AgentConsoleError::new(
            "checkpoint_git_failed",
            format!(
                "git {}: {}",
                args.first().copied().unwrap_or_default(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ))
    }
}

/// Path text git accepts on every platform (no `\\?\` verbatim prefix).
fn git_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text.into_owned()
    };
    if cfg!(target_os = "windows") {
        text.replace('\\', "/")
    } else {
        text
    }
}

fn shadow_ref(checkpoint_name: &str) -> String {
    let name: String = checkpoint_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("refs/tinto/{name}")
}

/// Ignored in folders without git, which have no .gitignore to keep
/// dependency and build output out of the snapshot.
const PLAIN_FOLDER_EXCLUDES: &str =
    "node_modules/\ntarget/\n.venv/\nvenv/\n__pycache__/\ndist/\nbuild/\n.next/\n.cache/\n";

fn ensure_shadow_store(
    repo_dir: &Path,
    repository: Option<&Repository>,
) -> Result<PathBuf, AgentConsoleError> {
    let store = repo_dir.join(SHADOW_STORE_DIR);
    if !store.join("HEAD").is_file() {
        fs::create_dir_all(&store).map_err(io_error)?;
        let mut command = Command::new(crate::git::git_program());
        for name in INHERITED_GIT_ENV {
            command.env_remove(name);
        }
        command
            .args(["init", "--bare", "--quiet", "--template="])
            .arg(git_path(&store));
        #[cfg(target_os = "windows")]
        hide_console(&mut command);
        run_git_command(command, &["init"], None)?;
    }
    let info = store.join("info");
    fs::create_dir_all(&info).map_err(io_error)?;
    // Byte-exact snapshots: no line-ending conversion, no LFS or clean filters.
    fs::write(
        info.join("attributes"),
        "* -text -filter -ident -working-tree-encoding\n",
    )
    .map_err(io_error)?;
    let alternates = store.join("objects").join("info");
    fs::create_dir_all(&alternates).map_err(io_error)?;
    let Some(repository) = repository else {
        fs::write(info.join("exclude"), PLAIN_FOLDER_EXCLUDES).map_err(io_error)?;
        let _ = fs::remove_file(alternates.join("alternates"));
        return Ok(store);
    };
    // Honour the repo's private ignore rules as well as its .gitignore files.
    let common = repository.commondir();
    match fs::read(common.join("info").join("exclude")) {
        Ok(rules) => fs::write(info.join("exclude"), rules).map_err(io_error)?,
        Err(_) => {
            let _ = fs::remove_file(info.join("exclude"));
        }
    }
    // Committed content is read from the repo itself, never copied.
    fs::write(
        alternates.join("alternates"),
        format!("{}\n", git_path(&common.join("objects"))),
    )
    .map_err(io_error)?;
    Ok(store)
}

fn shadow_snapshot(
    repo: &Path,
    repository: Option<&Repository>,
    repo_dir: &Path,
    checkpoint_dir: &Path,
    checkpoint_name: &str,
) -> Result<(AgentSessionCheckpoint, String), AgentConsoleError> {
    let git = ShadowGit {
        store: ensure_shadow_store(repo_dir, repository)?,
        worktree: repo.to_path_buf(),
        index: checkpoint_dir.join(SHADOW_INDEX),
    };
    // Seed from the newest snapshot (or the repo's own index): their stat
    // cache means only files changed since then are read and hashed.
    let seeded = std::iter::once(repo_dir.join(SHADOW_LATEST_INDEX))
        .chain(repository.map(|repository| repository.path().join("index")))
        .any(|seed| seed.is_file() && fs::copy(&seed, &git.index).is_ok());
    let snapshot = |git: &ShadowGit| -> Result<String, AgentConsoleError> {
        // --ignore-errors skips unreadable files (exit status 1) instead of
        // aborting; write-tree is what proves the index is complete.
        let _ = git.run(&["add", "--all", "--ignore-errors", "--", "."]);
        let tree = git.run(&["write-tree"])?;
        Ok(String::from_utf8_lossy(&tree).trim().to_string())
    };
    let tree = match snapshot(&git) {
        Ok(tree) => tree,
        Err(_) if seeded => {
            // A seed git cannot use here (split index, pruned objects): start clean.
            let _ = fs::remove_file(&git.index);
            snapshot(&git)?
        }
        Err(error) => return Err(error),
    };
    git.run(&["update-ref", &shadow_ref(checkpoint_name), &tree])?;

    let head = repository
        .and_then(|repository| repository.head().ok())
        .and_then(|head| head.target())
        .map(|oid| oid.to_string());
    let base = head.clone().unwrap_or_else(|| EMPTY_TREE.to_string());
    let differing = git.run(&[
        "diff-index",
        "--cached",
        "--name-only",
        "-z",
        "--no-renames",
        "--ignore-submodules=all",
        &base,
    ])?;

    let latest = repo_dir.join(SHADOW_LATEST_INDEX);
    let staging = repo_dir.join(format!("{SHADOW_LATEST_INDEX}.{}", std::process::id()));
    if fs::copy(&git.index, &staging).is_ok() && fs::rename(&staging, &latest).is_err() {
        let _ = fs::remove_file(&staging);
    }

    Ok((
        AgentSessionCheckpoint {
            checkpoint_type: AgentSessionCheckpointType::FsSnapshot,
            git_hash: head,
            snapshot_files: nul_separated_paths(&differing),
        },
        tree,
    ))
}

fn shadow_of(record: &CheckpointRecord) -> Result<Option<ShadowCheckpoint>, AgentConsoleError> {
    let Ok(metadata) = read_metadata(&record.checkpoint_dir) else {
        return Ok(None);
    };
    let Some(tree) = metadata.shadow_tree else {
        return Ok(None);
    };
    let repo_dir = record.checkpoint_dir.parent().ok_or_else(|| {
        AgentConsoleError::new(
            "checkpoint_invalid",
            "checkpoint has no repository directory",
        )
    })?;
    let git = ShadowGit {
        store: repo_dir.join(SHADOW_STORE_DIR),
        worktree: record.repo.clone(),
        index: record.checkpoint_dir.join(SHADOW_INDEX),
    };
    if !git.index.is_file() {
        git.run(&["read-tree", &tree])?;
    }
    Ok(Some(ShadowCheckpoint { git }))
}

fn scan_shadow_changes(
    shadow: &ShadowCheckpoint,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let git = &shadow.git;
    // Refreshing only updates this checkpoint's stat cache (its content stays
    // the snapshot), so later scans skip files that did not change.
    let _ = git.run(&["update-index", "-q", "--refresh"]);
    let mut changes = Vec::new();
    let modified = git.run(&[
        "diff-files",
        "--name-status",
        "-z",
        "--no-renames",
        "--ignore-submodules=all",
    ])?;
    let mut fields = modified
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty());
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let kind = if status.first() == Some(&b'D') {
            AgentSessionChangeKind::Removed
        } else {
            AgentSessionChangeKind::Modified
        };
        changes.push(change(
            Path::new(&*String::from_utf8_lossy(path)),
            kind,
            timestamp_ms,
        ));
    }
    let created = git.run(&["ls-files", "--others", "--exclude-standard", "-z"])?;
    for path in nul_separated_paths(&created) {
        // A nested repository shows up as "dir/"; it is not ours to track.
        if !path.to_string_lossy().ends_with('/') {
            changes.push(change(&path, AgentSessionChangeKind::Created, timestamp_ms));
        }
    }
    changes.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| kind_name(a.kind).cmp(kind_name(b.kind)))
    });
    Ok(changes)
}

fn revert_shadow(
    record: &CheckpointRecord,
    shadow: &ShadowCheckpoint,
) -> Result<(), AgentConsoleError> {
    let mut restore = Vec::new();
    for change in scan_shadow_changes(shadow, 0)? {
        match change.kind {
            AgentSessionChangeKind::Created => remove_created_file(&record.repo, &change.path)?,
            _ => {
                prepare_restore_target(&record.repo, &change.path)?;
                restore.push(change.path);
            }
        }
    }
    restore_from_shadow(&shadow.git, &restore)
}

fn revert_shadow_file(
    record: &CheckpointRecord,
    shadow: &ShadowCheckpoint,
    rel: &Path,
) -> Result<(), AgentConsoleError> {
    let rel_text = git_relative(rel);
    let in_snapshot = !shadow
        .git
        .run(&["ls-files", "-z", "--", &rel_text])?
        .is_empty();
    if in_snapshot {
        prepare_restore_target(&record.repo, rel)?;
        restore_from_shadow(&shadow.git, &[rel.to_path_buf()])
    } else {
        remove_created_file(&record.repo, rel)
    }
}

fn restore_from_shadow(git: &ShadowGit, paths: &[PathBuf]) -> Result<(), AgentConsoleError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(git_relative(path).as_bytes());
        input.push(0);
    }
    git.run_with_input(
        &["checkout-index", "--force", "-z", "--stdin"],
        Some(&input),
    )
    .map(|_| ())
    .map_err(|error| AgentConsoleError::new("revert_failed", error.message))
}

fn remove_created_file(repo: &Path, rel: &Path) -> Result<(), AgentConsoleError> {
    validate_current_path_ancestors(repo, rel)?;
    let target = repo.join(rel);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if !metadata.is_dir() => fs::remove_file(target).map_err(io_error),
        _ => Ok(()),
    }
}

fn git_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn nul_separated_paths(output: &[u8]) -> Vec<PathBuf> {
    output
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| PathBuf::from(String::from_utf8_lossy(field).into_owned()))
        .collect()
}

/// Removes a checkpoint and releases its snapshot in the shadow store.
fn remove_checkpoint_dir(repo_dir: &Path, checkpoint_dir: &Path) -> Result<(), AgentConsoleError> {
    let store = repo_dir.join(SHADOW_STORE_DIR);
    if let Some(name) = checkpoint_dir.file_name().and_then(|name| name.to_str()) {
        if store.join("HEAD").is_file() {
            let reference = shadow_ref(name);
            let args = ["update-ref", "-d", reference.as_str()];
            let mut command = git_command(&store);
            command.args(args);
            let _ = run_git_command(command, &args, None);
        }
    }
    fs::remove_dir_all(checkpoint_dir).map_err(io_error)
}

/// Deletes snapshot content no remaining checkpoint references.
fn prune_shadow_store(repo_dir: &Path) {
    let store = repo_dir.join(SHADOW_STORE_DIR);
    if store.join("HEAD").is_file() {
        let args = ["prune", "--expire=now"];
        let mut command = git_command(&store);
        command.args(args);
        let _ = run_git_command(command, &args, None);
    }
}

fn change(path: &Path, kind: AgentSessionChangeKind, timestamp_ms: u64) -> AgentSessionChange {
    AgentSessionChange {
        path: path.to_path_buf(),
        kind,
        timestamp_ms,
    }
}

fn run_git(repo: &Path, args: &[&str]) -> Result<(), AgentConsoleError> {
    let mut command = Command::new(crate::git::git_program());
    command.arg("-C").arg(repo).args(args);
    #[cfg(target_os = "windows")]
    hide_console(&mut command);
    let output = command
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
    let mut command = Command::new(crate::git::git_program());
    command.arg("-C").arg(repo).args(["cat-file", "-e", &spec]);
    #[cfg(target_os = "windows")]
    hide_console(&mut command);
    let output = command
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
    shadow_tree: Option<String>,
) -> Result<(), AgentConsoleError> {
    let metadata = CheckpointMetadata {
        repo: repo.to_path_buf(),
        session_id: session_id.to_string(),
        created_at_ms,
        checkpoint_type: contract.checkpoint_type,
        git_hash: contract.git_hash.clone(),
        snapshot_files: contract.snapshot_files.clone(),
        dirty_created_files: Vec::new(),
        dirty_deleted_files: Vec::new(),
        shadow_tree,
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
            remove_checkpoint_dir(repo_dir, path)?;
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
        remove_checkpoint_dir(repo_dir, &path)?;
        // Shared objects are reclaimed once no remaining checkpoint uses them.
        prune_shadow_store(repo_dir);
        total = dir_size(repo_dir)?;
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
        if entry.file_name() == SHADOW_STORE_DIR {
            continue;
        }
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

    fn git_output(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf-8 git output")
    }

    fn git_status_porcelain(repo: &Path) -> String {
        git_output(repo, &["status", "--porcelain=v1"])
    }

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
        // Content lives in the shared shadow store, not as per-checkpoint copies.
        assert!(!record.checkpoint_dir.join("files").exists());
        assert!(record.checkpoint_dir.join(SHADOW_INDEX).is_file());
        let repo_dir = record.checkpoint_dir.parent().unwrap();
        assert!(repo_dir.join(SHADOW_STORE_DIR).join("HEAD").is_file());
    }

    #[test]
    fn dirty_submodule_does_not_force_a_full_tree_snapshot() {
        let repo = TempRepo::with_initial_commit();
        // A nested repository registered as a gitlink, like a git submodule.
        let sub_path = repo.path().join("sub");
        let sub = Repository::init(&sub_path).unwrap();
        fs::write(sub_path.join("lib.txt"), "v1\n").unwrap();
        let mut sub_index = sub.index().unwrap();
        sub_index.add_path(Path::new("lib.txt")).unwrap();
        sub_index.write().unwrap();
        let sub_tree = sub.find_tree(sub_index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("t", "t@example.com").unwrap();
        let sub_head = sub
            .commit(Some("HEAD"), &signature, &signature, "sub", &sub_tree, &[])
            .unwrap();
        let outer = Repository::open(repo.path()).unwrap();
        let mut index = outer.index().unwrap();
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o160000,
                uid: 0,
                gid: 0,
                file_size: 0,
                id: sub_head,
                flags: 0,
                flags_extended: 0,
                path: b"sub".to_vec(),
            })
            .unwrap();
        index.write().unwrap();
        let tree = outer.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = outer.head().unwrap().peel_to_commit().unwrap();
        outer
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "add sub",
                &tree,
                &[&parent],
            )
            .unwrap();
        // The submodule's own working tree gets large and dirty.
        fs::write(sub_path.join("lib.txt"), "v2\n").unwrap();
        fs::create_dir_all(sub_path.join("node_modules")).unwrap();
        fs::write(sub_path.join("node_modules/big.bin"), vec![1u8; 4096]).unwrap();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig::default();

        let record = create_checkpoint(repo.path(), "sess-submodule", 1, &config).unwrap();

        assert_eq!(
            record.contract.snapshot_files,
            vec![PathBuf::from("base.txt")]
        );
    }

    /// Incompressible bytes, so store sizes reflect real storage.
    fn noise(len: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 33) as u8
            })
            .collect()
    }

    fn loose_object_count(git_dir: &Path) -> usize {
        WalkDir::new(git_dir.join("objects"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .count()
    }

    #[test]
    fn large_dirty_content_is_stored_once_and_restored_exactly() {
        let repo = TempRepo::with_initial_commit();
        let big = noise(3 * 1024 * 1024, 7);
        fs::create_dir_all(repo.path().join("docs/runs")).unwrap();
        fs::write(repo.path().join("docs/runs/a.log"), &big).unwrap();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig::default();

        let first = create_checkpoint(repo.path(), "sess-large-1", 1, &config).unwrap();
        repo.write("base.txt", "dirty again\n");
        let _second = create_checkpoint(repo.path(), "sess-large-2", 2, &config).unwrap();

        let store = first
            .checkpoint_dir
            .parent()
            .unwrap()
            .join(SHADOW_STORE_DIR);
        let stored = dir_size(&store).unwrap();
        assert!(stored < 4 * 1024 * 1024, "store holds {stored} bytes");

        fs::remove_file(repo.path().join("docs/runs/a.log")).unwrap();
        repo.write("base.txt", "agent edit\n");
        revert_checkpoint(&first).unwrap();
        assert_eq!(fs::read(repo.path().join("docs/runs/a.log")).unwrap(), big);
        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "dirty\n"
        );
    }

    #[test]
    fn snapshots_never_write_to_the_user_repository() {
        let repo = TempRepo::with_initial_commit();
        repo.write_and_stage("staged.txt", "staged\n");
        repo.write("base.txt", "dirty\n");
        repo.write("untracked.txt", "new\n");
        let git_dir = repo.path().join(".git");
        let status_before = git_status_porcelain(repo.path());
        let objects_before = loose_object_count(&git_dir);
        let index_before = fs::read(git_dir.join("index")).unwrap();

        let record = create_checkpoint(
            repo.path(),
            "sess-readonly",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        repo.write("base.txt", "agent edit\n");
        scan_change_log(&record, 2).unwrap();
        revert_checkpoint(&record).unwrap();

        assert_eq!(git_status_porcelain(repo.path()), status_before);
        assert_eq!(loose_object_count(&git_dir), objects_before);
        assert_eq!(fs::read(git_dir.join("index")).unwrap(), index_before);
    }

    #[test]
    fn renamed_files_are_snapshotted_without_a_full_tree_copy() {
        let repo = TempRepo::with_initial_commit();
        run_git(repo.path(), &["mv", "base.txt", "renamed.txt"]).unwrap();
        let record =
            create_checkpoint(repo.path(), "sess-rename", 1, &CheckpointConfig::default()).unwrap();

        assert!(record.checkpoint_dir.join(SHADOW_INDEX).is_file());
        repo.write("renamed.txt", "agent edit\n");
        repo.write("created.txt", "new\n");
        revert_checkpoint(&record).unwrap();

        assert!(!repo.path().join("base.txt").exists());
        assert!(!repo.path().join("created.txt").exists());
        assert_eq!(
            fs::read(repo.path().join("renamed.txt")).unwrap(),
            git_output(repo.path(), &["show", "HEAD:base.txt"]).into_bytes()
        );
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
        let config = CheckpointConfig::default();

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
    fn ephemeral_checkpoint_does_not_prune_the_target_and_can_be_removed() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "target\n");
        let config = CheckpointConfig {
            retention_per_repo: 1,
            ..CheckpointConfig::default()
        };
        let target = create_checkpoint(repo.path(), "target", 1, &config).unwrap();

        repo.write("base.txt", "current\n");
        let safety = create_ephemeral_checkpoint(repo.path(), "safety", 2, &config).unwrap();

        assert!(target.checkpoint_dir.exists());
        assert!(safety.checkpoint_dir.exists());
        remove_ephemeral_checkpoint(&safety).unwrap();
        assert!(target.checkpoint_dir.exists());
        assert!(!safety.checkpoint_dir.exists());
    }

    #[test]
    fn ephemeral_checkpoint_restores_staged_index_state() {
        let repo = TempRepo::with_initial_commit();
        repo.write_and_stage("base.txt", "staged content\n");
        let status_before = git_status_porcelain(repo.path());
        let safety = create_ephemeral_checkpoint(
            repo.path(),
            "safety-staged-index",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();

        run_git(repo.path(), &["reset", "--hard", "HEAD"]).unwrap();
        revert_checkpoint(&safety).unwrap();

        assert_eq!(git_status_porcelain(repo.path()), status_before);
        assert_eq!(
            git_output(repo.path(), &["show", ":base.txt"]),
            "staged content\n"
        );
        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "staged content\n"
        );
        remove_ephemeral_checkpoint(&safety).unwrap();
    }

    #[test]
    fn ephemeral_checkpoint_restores_staged_and_unstaged_index_state() {
        let repo = TempRepo::with_initial_commit();
        repo.write_and_stage("base.txt", "staged content\n");
        repo.write("base.txt", "unstaged content\n");
        let status_before = git_status_porcelain(repo.path());
        let safety = create_ephemeral_checkpoint(
            repo.path(),
            "safety-mixed-index",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();

        run_git(repo.path(), &["reset", "--hard", "HEAD"]).unwrap();
        revert_checkpoint(&safety).unwrap();

        assert_eq!(git_status_porcelain(repo.path()), status_before);
        assert_eq!(
            git_output(repo.path(), &["show", ":base.txt"]),
            "staged content\n"
        );
        assert_eq!(
            fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "unstaged content\n"
        );
        remove_ephemeral_checkpoint(&safety).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ephemeral_checkpoint_rejects_a_dirty_symlink_before_repo_mutation() {
        use std::os::unix::fs::symlink;

        let repo = TempRepo::with_initial_commit();
        let link = repo.path().join("base-link.txt");
        symlink("base.txt", &link).unwrap();

        let error = create_ephemeral_checkpoint(
            repo.path(),
            "safety-symlink",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap_err();

        assert_eq!(error.category, "checkpoint_unsupported");
        assert!(error.message.contains("base-link.txt"));
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    }

    #[test]
    fn ephemeral_checkpoint_cleanup_rejects_an_external_directory() {
        let repo = TempRepo::with_initial_commit();
        let _anchor = create_checkpoint(
            repo.path(),
            "cleanup-validation-anchor",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        let external = tempfile::tempdir().unwrap();
        let record = CheckpointRecord {
            contract: AgentSessionCheckpoint {
                checkpoint_type: AgentSessionCheckpointType::FsSnapshot,
                git_hash: None,
                snapshot_files: Vec::new(),
            },
            repo: repo.path().to_path_buf(),
            session_id: "external".into(),
            checkpoint_dir: external.path().to_path_buf(),
            created_at_ms: 1,
            ephemeral: true,
        };

        let error = remove_ephemeral_checkpoint(&record).unwrap_err();

        assert_eq!(error.category, "checkpoint_invalid");
        assert!(external.path().exists());
    }

    #[test]
    fn repo_budget_prunes_old_checkpoints_before_failing_start() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "dirty\n");
        let config = CheckpointConfig {
            retention_per_repo: 10,
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
    fn plain_folder_uses_the_shadow_store_without_a_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Bigger than the old 100 MB file-copy cap would allow at scale,
        // and a dependency folder that must stay out of the snapshot.
        fs::write(root.join("data.bin"), noise(2 * 1024 * 1024, 3)).unwrap();
        fs::write(root.join("notes.txt"), "before\n").unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "dep\n").unwrap();

        let record =
            create_checkpoint(root, "sess-plain", 1, &CheckpointConfig::default()).unwrap();
        assert_eq!(
            record.contract.checkpoint_type,
            AgentSessionCheckpointType::FsSnapshot
        );
        assert_eq!(record.contract.git_hash, None);
        assert!(!record.checkpoint_dir.join("files").exists());
        assert!(record
            .contract
            .snapshot_files
            .iter()
            .all(|path| !path.starts_with("node_modules")));

        fs::write(root.join("notes.txt"), "after\n").unwrap();
        fs::remove_file(root.join("data.bin")).unwrap();
        fs::write(root.join("created.txt"), "new\n").unwrap();
        let changes = scan_change_log(&record, 5).unwrap();
        assert_eq!(changes.len(), 3, "{changes:?}");

        revert_checkpoint(&record).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("notes.txt")).unwrap(),
            "before\n"
        );
        assert_eq!(
            fs::read(root.join("data.bin")).unwrap().len(),
            2 * 1024 * 1024
        );
        assert!(!root.join("created.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("node_modules/pkg/index.js")).unwrap(),
            "dep\n"
        );
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
