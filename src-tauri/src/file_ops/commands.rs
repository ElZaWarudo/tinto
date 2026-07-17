//! Tauri commands for filesystem mutations scoped to known repos. Each
//! command validates paths stay inside the repo and outside `.git/`. Conflicts
//! (destination already exists) are reported; the caller sets the overwrite
//! policy. Never auto-overwrites.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tauri::State;
use uuid::Uuid;

use crate::bus::commands::{map_repo_resolve_error, wsl_request, CommandError};
use crate::bus::{BusHandle, ResolvedRepo};
use crate::workbench::RepoSource;
use crate::wsl_agent::launcher::windows_path_to_wsl_mount;
use crate::wsl_agent::protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION};

use super::{safe_join, FileConflict, FileConflictKind};

/// Recursively copia un archivo o directorio a `dest`.
pub(crate) fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let from = entry.path();
            let to = dest.join(entry.file_name());
            copy_recursive(&from, &to)?;
        }
    } else {
        fs::copy(src, dest)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CopyResult {
    /// Paths relativos al repo de los archivos recién creados/actualizados.
    pub copied: Vec<String>,
    /// Conflictos detectados (vacío si todo OK).
    pub conflicts: Vec<FileConflict>,
    /// Limpiezas auxiliares que no invalidan la mutación ya completada.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileOpOutcome {
    /// Limpiezas auxiliares que no invalidan la mutación ya completada.
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletedEntry {
    /// Path relativo al repo.
    pub path: PathBuf,
    pub is_dir: bool,
    /// Nombre interno del backup dentro del staging temporal.
    pub(crate) backup_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteResult {
    pub token: String,
    pub entries: Vec<DeletedEntry>,
    /// `false` indica que el borrado se interrumpió y requiere restauración.
    #[serde(default = "default_true")]
    pub completed: bool,
    #[serde(default)]
    pub recovery_required: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeleteManifest {
    pub token: String,
    pub repo: PathBuf,
    pub entries: Vec<DeletedEntry>,
}

enum ResolveOutcome {
    Proceed,
    Conflict(FileConflictKind),
    SourceMissing,
}

/// Decide qué hacer con un par (src, dest). Nunca toca FS.
fn classify(src: &Path, dest: &Path, overwrite: bool) -> ResolveOutcome {
    if !src.exists() {
        return ResolveOutcome::SourceMissing;
    }
    if dest.exists() {
        if overwrite {
            ResolveOutcome::Proceed
        } else if dest.is_dir() {
            ResolveOutcome::Conflict(FileConflictKind::DirExists)
        } else {
            ResolveOutcome::Conflict(FileConflictKind::FileExists)
        }
    } else {
        ResolveOutcome::Proceed
    }
}

/// Copia archivos desde el OS (paths absolutos) a una carpeta del repo. Si
/// hay conflictos, retorna `conflicts` para que el frontend confirme y
/// reintente con `overwrite=true`.
#[tauri::command]
pub async fn copy_to_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    dest_dir: PathBuf,     // relativo al repo; "" = raíz
    sources: Vec<PathBuf>, // absolutos del OS
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_copy_to_repo(resolved, dest_dir, sources, overwrite);
    }
    let repo_abs = resolved.path;
    let dest_abs = safe_join(&repo_abs, &dest_dir)?;
    if !dest_abs.is_dir() {
        return Err(CommandError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio dentro del repo",
        ));
    }

    let mut to_copy = Vec::with_capacity(sources.len());
    let mut conflicts = Vec::new();

    for src in &sources {
        let name = src
            .file_name()
            .ok_or_else(|| CommandError::new("invalid-source", "source sin file_name"))?;
        let dest = dest_abs.join(name);
        let dest_rel = dest.strip_prefix(&repo_abs).unwrap_or(&dest).to_path_buf();

        match classify(src, &dest, overwrite) {
            ResolveOutcome::Proceed => to_copy.push((src.clone(), dest)),
            ResolveOutcome::Conflict(kind) => conflicts.push(FileConflict::new(dest_rel, kind)),
            ResolveOutcome::SourceMissing => {
                conflicts.push(FileConflict::new(dest_rel, FileConflictKind::SourceMissing))
            }
        }
    }

    if !conflicts.is_empty() {
        return Ok(CopyResult {
            copied: Vec::new(),
            conflicts,
            warnings: Vec::new(),
        });
    }

    let repo_root = repo_abs.clone();
    let (copied, warnings) = run_copy_blocking(repo_root, to_copy)?;

    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
        warnings,
    })
}

/// Copia archivos dentro del mismo repo (de cualquier carpeta a otra).
#[tauri::command]
pub async fn copy_within_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
    dest_dir: PathBuf,     // relativa al repo
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_copy_within_repo(resolved, sources, dest_dir, overwrite);
    }
    let repo_abs = resolved.path;
    let dest_abs = safe_join(&repo_abs, &dest_dir)?;
    if !dest_abs.is_dir() {
        return Err(CommandError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio dentro del repo",
        ));
    }

    let mut to_copy = Vec::with_capacity(sources.len());
    let mut conflicts = Vec::new();

    for src_rel in &sources {
        let src_abs = safe_join(&repo_abs, src_rel)?;
        let name = src_abs
            .file_name()
            .ok_or_else(|| CommandError::new("invalid-source", "source sin file_name"))?;
        let dest = dest_abs.join(name);
        let dest_rel = dest.strip_prefix(&repo_abs).unwrap_or(&dest).to_path_buf();
        if src_abs == dest {
            return Err(CommandError::new(
                "same-src-dest",
                "source y destino son el mismo path",
            ));
        }

        match classify(&src_abs, &dest, overwrite) {
            ResolveOutcome::Proceed => to_copy.push((src_abs, dest)),
            ResolveOutcome::Conflict(kind) => conflicts.push(FileConflict::new(dest_rel, kind)),
            ResolveOutcome::SourceMissing => {
                conflicts.push(FileConflict::new(dest_rel, FileConflictKind::SourceMissing))
            }
        }
    }

    if !conflicts.is_empty() {
        return Ok(CopyResult {
            copied: Vec::new(),
            conflicts,
            warnings: Vec::new(),
        });
    }

    let (copied, warnings) = run_copy_blocking(repo_abs, to_copy)?;

    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
        warnings,
    })
}

/// Mueve archivos dentro del repo (rename). Igual semantics que copy_within
/// pero remueve el source después de copiar.
#[tauri::command]
pub async fn move_within_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
    dest_dir: PathBuf,     // relativa al repo
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_move_within_repo(resolved, sources, dest_dir, overwrite);
    }
    let repo_abs = resolved.path;
    let dest_abs = safe_join(&repo_abs, &dest_dir)?;
    if !dest_abs.is_dir() {
        return Err(CommandError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio dentro del repo",
        ));
    }

    let mut to_move = Vec::with_capacity(sources.len());
    let mut conflicts = Vec::new();

    for src_rel in &sources {
        let src_abs = safe_join(&repo_abs, src_rel)?;
        let name = src_abs
            .file_name()
            .ok_or_else(|| CommandError::new("invalid-source", "source sin file_name"))?;
        let dest = dest_abs.join(name);
        let dest_rel = dest.strip_prefix(&repo_abs).unwrap_or(&dest).to_path_buf();
        if src_abs == dest {
            return Err(CommandError::new(
                "same-src-dest",
                "source y destino son el mismo path",
            ));
        }

        match classify(&src_abs, &dest, overwrite) {
            ResolveOutcome::Proceed => to_move.push((src_abs, dest)),
            ResolveOutcome::Conflict(kind) => conflicts.push(FileConflict::new(dest_rel, kind)),
            ResolveOutcome::SourceMissing => {
                conflicts.push(FileConflict::new(dest_rel, FileConflictKind::SourceMissing))
            }
        }
    }

    if !conflicts.is_empty() {
        return Ok(CopyResult {
            copied: Vec::new(),
            conflicts,
            warnings: Vec::new(),
        });
    }

    let (moved, warnings) = run_move_blocking(repo_abs, to_move)?;

    Ok(CopyResult {
        copied: moved,
        conflicts: Vec::new(),
        warnings,
    })
}

/// Copia archivos del repo hacia un directorio del OS (absoluto).
#[tauri::command]
pub async fn export_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
    dest_dir: PathBuf,     // absoluto del OS
) -> Result<FileOpOutcome, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_export_from_repo(resolved, sources, dest_dir);
    }
    let repo_abs = resolved.path;
    if !dest_dir.is_dir() {
        return Err(CommandError::new(
            "dest-not-a-dir",
            "dest_dir no es un directorio en el filesystem",
        ));
    }

    let mut srcs = Vec::with_capacity(sources.len());
    for src_rel in &sources {
        let src_abs = safe_join(&repo_abs, src_rel)?;
        if !src_abs.exists() {
            return Err(CommandError::new(
                "source-missing",
                format!("no existe {} en el repo", src_rel.display()),
            ));
        }
        srcs.push(src_abs);
    }

    let to_copy = srcs
        .into_iter()
        .map(|src| {
            let name = src.file_name().unwrap_or_default();
            let dest = dest_dir.join(name);
            (src, dest)
        })
        .collect();
    let batch = std::thread::spawn(move || run_copy_batch_with_hook(to_copy, |_| Ok(())))
        .join()
        .map_err(|_| CommandError::new("export-panic", "thread panicked"))?
        .map_err(|e| CommandError::new("export-failed", format!("no se pudo exportar: {e}")))?;

    Ok(FileOpOutcome {
        warnings: batch.warnings,
    })
}

/// Elimina archivos o directorios dentro del repo. Rechaza el root del repo y
/// cualquier path fuera del repo o dentro de `.git/` vía `safe_join`.
#[tauri::command]
pub async fn delete_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
    user_consent: bool,
) -> Result<DeleteResult, CommandError> {
    require_delete_user_consent(user_consent)?;
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_delete_from_repo(resolved, sources);
    }
    let repo_abs = resolved.path;
    let mut targets = Vec::with_capacity(sources.len());

    for src_rel in &sources {
        let src_abs = safe_join(&repo_abs, src_rel)?;
        if src_abs == repo_abs {
            return Err(CommandError::new(
                "delete-root-forbidden",
                "no se puede eliminar el root del repo",
            ));
        }
        if !src_abs.exists() {
            return Err(CommandError::new(
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
    let runner_root = backup_root.clone();
    let batch =
        std::thread::spawn(move || run_delete_batch_with_hook(&runner_root, moves, |_| Ok(())))
            .join()
            .map_err(|_| CommandError::new("delete-panic", "thread panicked"))?;

    match batch {
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
        Err(failure) => Err(CommandError::new(
            "delete-failed",
            format!("no se pudo eliminar: {}", failure.error),
        )),
    }
}

fn require_delete_user_consent(user_consent: bool) -> Result<(), CommandError> {
    if user_consent {
        return Ok(());
    }
    Err(CommandError::new(
        "user-consent-required",
        "eliminar archivos requiere confirmación explícita del usuario",
    ))
}

#[tauri::command]
pub async fn restore_deleted_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    token: String,
) -> Result<FileOpOutcome, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_restore_deleted_from_repo(resolved, token);
    }
    let repo_abs = resolved.path;
    let manifest = read_bound_delete_manifest(&repo_abs, &token)?;
    let targets = plan_delete_replay(&repo_abs, &manifest, ReplayDirection::Restore)?;

    let warnings =
        std::thread::spawn(move || run_replay_batch_with_hook(&token, targets, true, |_| Ok(())))
            .join()
            .map_err(|_| CommandError::new("restore-panic", "thread panicked"))?
            .map_err(|e| {
                CommandError::new("restore-failed", format!("no se pudo restaurar: {e}"))
            })?;

    Ok(FileOpOutcome { warnings })
}

#[tauri::command]
pub async fn redo_deleted_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    token: String,
) -> Result<FileOpOutcome, CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_redo_deleted_from_repo(resolved, token);
    }
    let repo_abs = resolved.path;
    let manifest = read_bound_delete_manifest(&repo_abs, &token)?;
    let targets = plan_delete_replay(&repo_abs, &manifest, ReplayDirection::Redo)?;

    let warnings =
        std::thread::spawn(move || run_replay_batch_with_hook(&token, targets, false, |_| Ok(())))
            .join()
            .map_err(|_| CommandError::new("redo-delete-panic", "thread panicked"))?
            .map_err(|e| {
                CommandError::new("redo-delete-failed", format!("no se pudo rehacer: {e}"))
            })?;

    Ok(FileOpOutcome { warnings })
}

async fn resolve_file_op_repo(bus: &BusHandle, repo: &Path) -> Result<ResolvedRepo, CommandError> {
    bus.resolve_repo_identity(repo.to_path_buf())
        .await
        .map_err(map_repo_resolve_error)
}

fn translate_host_path_for_wsl(path: PathBuf) -> Result<PathBuf, CommandError> {
    if path.is_absolute() && path.to_string_lossy().starts_with('/') {
        return Ok(path);
    }
    windows_path_to_wsl_mount(&path)
        .map(PathBuf::from)
        .map_err(|error| CommandError::new(error.safe_category(), error.message))
}

fn wsl_copy_to_repo(
    resolved: ResolvedRepo,
    dest_dir: PathBuf,
    sources: Vec<PathBuf>,
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let sources: Vec<PathBuf> = sources
        .into_iter()
        .map(translate_host_path_for_wsl)
        .collect::<Result<_, _>>()?;
    match wsl_request(
        resolved.distro,
        AgentRequest::CopyToRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            dest_dir,
            sources,
            overwrite,
        },
    )? {
        AgentResponse::CopyResult { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_copy_within_repo(
    resolved: ResolvedRepo,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::CopyWithinRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            sources,
            dest_dir,
            overwrite,
        },
    )? {
        AgentResponse::CopyResult { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_move_within_repo(
    resolved: ResolvedRepo,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::MoveWithinRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            sources,
            dest_dir,
            overwrite,
        },
    )? {
        AgentResponse::CopyResult { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_export_from_repo(
    resolved: ResolvedRepo,
    sources: Vec<PathBuf>,
    dest_dir: PathBuf,
) -> Result<FileOpOutcome, CommandError> {
    let dest_dir = translate_host_path_for_wsl(dest_dir)?;
    match wsl_request(
        resolved.distro,
        AgentRequest::ExportFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            sources,
            dest_dir,
        },
    )? {
        AgentResponse::FileOpOutcome { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_delete_from_repo(
    resolved: ResolvedRepo,
    sources: Vec<PathBuf>,
) -> Result<DeleteResult, CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::DeleteFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            sources,
        },
    )? {
        AgentResponse::DeleteResult { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_restore_deleted_from_repo(
    resolved: ResolvedRepo,
    token: String,
) -> Result<FileOpOutcome, CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::RestoreDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            token,
        },
    )? {
        AgentResponse::FileOpOutcome { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_redo_deleted_from_repo(
    resolved: ResolvedRepo,
    token: String,
) -> Result<FileOpOutcome, CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::RedoDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            token,
        },
    )? {
        AgentResponse::FileOpOutcome { result } => Ok(result),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn unexpected_file_ops_wsl_response(_response: AgentResponse) -> CommandError {
    CommandError::new("malformed_response", "respuesta inesperada del agente WSL")
}

// Staging and backups are siblings of the destination so every install/rollback
// rename stays on one filesystem on both Windows and Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplacementPhase {
    Staged,
    DestinationBackedUp,
    ReplacementInstalled,
    SourceBackedUp,
}

struct InstalledReplacement {
    dest: PathBuf,
    stage: PathBuf,
    backup: Option<PathBuf>,
}

struct InstalledMove {
    source: PathBuf,
    source_backup: PathBuf,
    replacement: InstalledReplacement,
}

impl InstalledMove {
    fn commit(self) -> Vec<String> {
        let mut warnings = self.replacement.commit();
        if let Some(warning) = cleanup_after_commit(&self.source_backup) {
            warnings.push(warning);
        }
        warnings
    }

    fn rollback(self) -> io::Result<()> {
        self.rollback_with_source_restore(|from, to| fs::rename(from, to))
    }

    fn rollback_with_source_restore(
        self,
        restore_source: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        match restore_source(&self.source_backup, &self.source) {
            Ok(()) => self.replacement.rollback(),
            // Conserva el reemplazo instalado: en un borrado es el objeto al que
            // apunta el manifest y permite recuperar el origen ausente.
            Err(error) => Err(io::Error::new(
                error.kind(),
                format!(
                    "no se pudo restaurar el origen {}; se conservó una copia en {}: {error}",
                    self.source.display(),
                    self.replacement.dest.display()
                ),
            )),
        }
    }
}

impl InstalledReplacement {
    fn commit(self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(backup) = self.backup {
            if let Some(warning) = cleanup_after_commit(&backup) {
                warnings.push(warning);
            }
        }
        warnings
    }

    fn rollback(self) -> io::Result<()> {
        fs::rename(&self.dest, &self.stage)?;

        if let Some(backup) = &self.backup {
            if let Err(restore_error) = fs::rename(backup, &self.dest) {
                let replacement_restore = fs::rename(&self.stage, &self.dest);
                return Err(combine_io_errors(
                    io::Error::new(
                        restore_error.kind(),
                        format!(
                            "no se pudo restaurar el destino original desde {}: {restore_error}",
                            backup.display()
                        ),
                    ),
                    replacement_restore,
                    "tampoco se pudo devolver el reemplazo al destino",
                ));
            }
        }

        remove_path(&self.stage)
    }
}

#[cfg(test)]
pub(crate) fn transactional_copy(src: &Path, dest: &Path) -> io::Result<()> {
    transactional_copy_with_hook(src, dest, |_| Ok(()))
}

#[cfg(test)]
fn transactional_copy_with_hook(
    src: &Path,
    dest: &Path,
    mut hook: impl FnMut(ReplacementPhase) -> io::Result<()>,
) -> io::Result<()> {
    let mut stage_copy = copy_recursive;
    transactional_copy_with_hook_and_stage_copy(src, dest, &mut hook, &mut stage_copy)
}

#[cfg(test)]
fn transactional_copy_with_hook_and_stage_copy(
    src: &Path,
    dest: &Path,
    hook: &mut impl FnMut(ReplacementPhase) -> io::Result<()>,
    stage_copy: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let installed = install_copy(src, dest, hook, stage_copy)?;
    let _warnings = installed.commit();
    Ok(())
}

fn install_copy(
    src: &Path,
    dest: &Path,
    hook: &mut impl FnMut(ReplacementPhase) -> io::Result<()>,
    stage_copy: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<InstalledReplacement> {
    stage_and_install(src, dest, hook, stage_copy)
}

#[cfg(test)]
pub(crate) fn transactional_copy_with_stage_copy(
    src: &Path,
    dest: &Path,
    mut stage_copy: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut hook = |_| Ok(());
    transactional_copy_with_hook_and_stage_copy(src, dest, &mut hook, &mut stage_copy)
}

#[cfg(test)]
fn transactional_move(src: &Path, dest: &Path) -> io::Result<()> {
    transactional_move_with_hook(src, dest, |_| Ok(()))
}

#[cfg(test)]
fn transactional_move_with_hook(
    src: &Path,
    dest: &Path,
    mut hook: impl FnMut(ReplacementPhase) -> io::Result<()>,
) -> io::Result<()> {
    let mut stage_copy = copy_recursive;
    transactional_move_with_hook_and_stage_copy(src, dest, &mut hook, &mut stage_copy)
}

#[cfg(test)]
fn transactional_move_with_hook_and_stage_copy(
    src: &Path,
    dest: &Path,
    hook: &mut impl FnMut(ReplacementPhase) -> io::Result<()>,
    stage_copy: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let installed = install_move(src, dest, hook, stage_copy)?;
    let _warnings = installed.commit();
    Ok(())
}

fn install_move(
    src: &Path,
    dest: &Path,
    hook: &mut impl FnMut(ReplacementPhase) -> io::Result<()>,
    stage_copy: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<InstalledMove> {
    let source_backup = neighbor_path(src, "source")?;
    let installed = stage_and_install(src, dest, hook, stage_copy)?;

    if let Err(source_error) = fs::rename(src, &source_backup) {
        return Err(combine_io_errors(
            source_error,
            installed.rollback(),
            "falló el rollback del destino",
        ));
    }

    if let Err(injected_error) = hook(ReplacementPhase::SourceBackedUp) {
        let source_restore = fs::rename(&source_backup, src);
        let error = combine_io_errors(
            injected_error,
            source_restore,
            "falló la restauración del origen",
        );
        return Err(combine_io_errors(
            error,
            installed.rollback(),
            "falló el rollback del destino",
        ));
    }

    Ok(InstalledMove {
        source: src.to_path_buf(),
        source_backup,
        replacement: installed,
    })
}

#[cfg(test)]
fn transactional_move_with_stage_copy(
    src: &Path,
    dest: &Path,
    mut stage_copy: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut hook = |_| Ok(());
    transactional_move_with_hook_and_stage_copy(src, dest, &mut hook, &mut stage_copy)
}

fn stage_and_install(
    src: &Path,
    dest: &Path,
    hook: &mut impl FnMut(ReplacementPhase) -> io::Result<()>,
    stage_copy: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<InstalledReplacement> {
    if src == dest || (src.is_dir() && dest.starts_with(src)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "el destino no puede ser el origen ni estar dentro de él",
        ));
    }

    let stage = neighbor_path(dest, "stage")?;
    if let Err(copy_error) = stage_copy(src, &stage) {
        return Err(combine_io_errors(
            copy_error,
            remove_path(&stage),
            "no se pudo limpiar el staging incompleto",
        ));
    }

    if let Err(injected_error) = hook(ReplacementPhase::Staged) {
        return Err(combine_io_errors(
            injected_error,
            remove_path(&stage),
            "no se pudo limpiar el staging",
        ));
    }

    let backup = if dest.exists() {
        let backup = neighbor_path(dest, "backup")?;
        if let Err(backup_error) = fs::rename(dest, &backup) {
            return Err(combine_io_errors(
                backup_error,
                remove_path(&stage),
                "no se pudo limpiar el staging",
            ));
        }
        Some(backup)
    } else {
        None
    };

    if let Err(injected_error) = hook(ReplacementPhase::DestinationBackedUp) {
        return Err(rollback_before_install(
            injected_error,
            dest,
            &stage,
            backup.as_deref(),
        ));
    }

    if let Err(install_error) = fs::rename(&stage, dest) {
        return Err(rollback_before_install(
            install_error,
            dest,
            &stage,
            backup.as_deref(),
        ));
    }

    let installed = InstalledReplacement {
        dest: dest.to_path_buf(),
        stage,
        backup,
    };

    if let Err(injected_error) = hook(ReplacementPhase::ReplacementInstalled) {
        return Err(combine_io_errors(
            injected_error,
            installed.rollback(),
            "falló el rollback del reemplazo",
        ));
    }

    Ok(installed)
}

fn rollback_before_install(
    primary: io::Error,
    dest: &Path,
    stage: &Path,
    backup: Option<&Path>,
) -> io::Error {
    let error = if let Some(backup) = backup {
        combine_io_errors(
            primary,
            fs::rename(backup, dest),
            "no se pudo restaurar el destino original",
        )
    } else {
        primary
    };
    combine_io_errors(error, remove_path(stage), "no se pudo limpiar el staging")
}

fn neighbor_path(path: &Path, role: &str) -> io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} no tiene directorio padre", path.display()),
        )
    })?;
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} no tiene nombre de archivo", path.display()),
        )
    })?;
    let mut candidate = OsString::from(".");
    candidate.push(file_name);
    candidate.push(format!(".tinto-{role}-{}", Uuid::new_v4()));
    Ok(parent.join(candidate))
}

fn remove_path(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn cleanup_after_commit(path: &Path) -> Option<String> {
    cleanup_after_commit_with(path, remove_path)
}

fn cleanup_after_commit_with(
    path: &Path,
    cleanup: impl FnOnce(&Path) -> io::Result<()>,
) -> Option<String> {
    cleanup(path).err().map(|error| {
        format!(
            "La operación terminó, pero no se pudo limpiar el respaldo temporal {}: {error}",
            path.display()
        )
    })
}

fn combine_io_errors(
    primary: io::Error,
    follow_up: io::Result<()>,
    follow_up_context: &str,
) -> io::Error {
    match follow_up {
        Ok(()) => primary,
        Err(follow_up_error) => io::Error::new(
            primary.kind(),
            format!("{primary}; {follow_up_context}: {follow_up_error}"),
        ),
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod transactional_replacement_tests;

pub(crate) struct BatchMutationResult {
    pub destinations: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

pub(crate) fn run_copy_batch_with_hook(
    to_copy: Vec<(PathBuf, PathBuf)>,
    mut after_item: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<BatchMutationResult> {
    let mut installed = Vec::with_capacity(to_copy.len());
    let mut destinations = Vec::with_capacity(to_copy.len());

    for (index, (src, dest)) in to_copy.into_iter().enumerate() {
        let mut replacement_hook = |_| Ok(());
        let mut stage_copy = copy_recursive;
        let replacement = match install_copy(&src, &dest, &mut replacement_hook, &mut stage_copy) {
            Ok(replacement) => replacement,
            Err(error) => return Err(rollback_copy_batch(error, installed)),
        };
        destinations.push(dest);
        installed.push(replacement);

        if let Err(error) = after_item(index + 1) {
            return Err(rollback_copy_batch(error, installed));
        }
    }

    let mut warnings = Vec::new();
    for replacement in installed {
        warnings.extend(replacement.commit());
    }
    Ok(BatchMutationResult {
        destinations,
        warnings,
    })
}

pub(crate) fn run_move_batch_with_hook(
    to_move: Vec<(PathBuf, PathBuf)>,
    mut after_item: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<BatchMutationResult> {
    let mut installed = Vec::with_capacity(to_move.len());
    let mut destinations = Vec::with_capacity(to_move.len());

    for (index, (src, dest)) in to_move.into_iter().enumerate() {
        let mut replacement_hook = |_| Ok(());
        let mut stage_copy = copy_recursive;
        let moved = match install_move(&src, &dest, &mut replacement_hook, &mut stage_copy) {
            Ok(moved) => moved,
            Err(error) => return Err(rollback_move_batch(error, installed)),
        };
        destinations.push(dest);
        installed.push(moved);

        if let Err(error) = after_item(index + 1) {
            return Err(rollback_move_batch(error, installed));
        }
    }

    let mut warnings = Vec::new();
    for moved in installed {
        warnings.extend(moved.commit());
    }
    Ok(BatchMutationResult {
        destinations,
        warnings,
    })
}

fn rollback_copy_batch(primary: io::Error, installed: Vec<InstalledReplacement>) -> io::Error {
    installed.into_iter().rev().fold(primary, |error, item| {
        combine_io_errors(error, item.rollback(), "fallo el rollback del lote")
    })
}

fn rollback_move_batch(primary: io::Error, installed: Vec<InstalledMove>) -> io::Error {
    installed.into_iter().rev().fold(primary, |error, item| {
        combine_io_errors(error, item.rollback(), "fallo el rollback del lote")
    })
}

pub(crate) fn run_delete_batch_with_hook(
    backup_root: &Path,
    moves: Vec<(PathBuf, PathBuf)>,
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> Result<Vec<String>, DeleteBatchFailure> {
    let objects_root = backup_root.join("objects");
    if let Err(error) = fs::create_dir_all(&objects_root) {
        return Err(DeleteBatchFailure::rolled_back(combine_io_errors(
            error,
            remove_path(backup_root),
            "no se pudo limpiar el token de borrado",
        )));
    }

    let rollback_check = moves.clone();
    match run_move_batch_with_hook(moves, after_item) {
        Ok(batch) => Ok(batch.warnings),
        Err(error) if batch_is_at_source(&rollback_check) => {
            Err(DeleteBatchFailure::rolled_back(combine_io_errors(
                error,
                remove_path(backup_root),
                "no se pudo limpiar el token de borrado revertido",
            )))
        }
        Err(error) if recovery_manifest_is_usable(&rollback_check) => {
            Err(DeleteBatchFailure::recovery_required(io::Error::new(
                error.kind(),
                format!("{error}; el borrado quedó incompleto y requiere restauración"),
            )))
        }
        Err(error) => Err(DeleteBatchFailure::rolled_back(io::Error::new(
            error.kind(),
            format!("{error}; rollback incompleto sin una copia recuperable para cada elemento"),
        ))),
    }
}

#[derive(Debug)]
pub(crate) struct DeleteBatchFailure {
    pub error: io::Error,
    pub recovery_required: bool,
}

impl DeleteBatchFailure {
    fn rolled_back(error: io::Error) -> Self {
        Self {
            error,
            recovery_required: false,
        }
    }

    fn recovery_required(error: io::Error) -> Self {
        Self {
            error,
            recovery_required: true,
        }
    }
}

fn recovery_manifest_is_usable(moves: &[(PathBuf, PathBuf)]) -> bool {
    let mut recovery_needed = false;
    moves.iter().all(|(source, backup)| {
        if source.exists() {
            true
        } else {
            recovery_needed = true;
            backup.exists()
        }
    }) && recovery_needed
}

pub(crate) fn run_replay_batch_with_hook(
    token: &str,
    moves: Vec<(PathBuf, PathBuf)>,
    create_destination_parents: bool,
    after_item: impl FnMut(usize) -> io::Result<()>,
) -> io::Result<Vec<String>> {
    let created_dirs = if create_destination_parents {
        create_missing_parent_dirs(&moves)?
    } else {
        Vec::new()
    };
    let rollback_check = moves.clone();

    match run_move_batch_with_hook(moves, after_item) {
        Ok(batch) => Ok(batch.warnings),
        Err(error) if batch_is_at_source(&rollback_check) => Err(combine_io_errors(
            error,
            remove_created_dirs(&created_dirs),
            "no se pudieron limpiar los directorios creados",
        )),
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!("{error}; rollback incompleto: conserva el token {token} para recuperacion",),
        )),
    }
}

fn batch_is_at_source(moves: &[(PathBuf, PathBuf)]) -> bool {
    moves.iter().all(|(source, destination)| {
        source.exists()
            && !destination.exists()
            && !has_transaction_artifact(source)
            && !has_transaction_artifact(destination)
    })
}

fn has_transaction_artifact(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(file_name) = path.file_name() else {
        return false;
    };
    let prefix = format!(".{}.tinto-", file_name.to_string_lossy());
    fs::read_dir(parent)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        })
        .unwrap_or(true)
}

fn create_missing_parent_dirs(moves: &[(PathBuf, PathBuf)]) -> io::Result<Vec<PathBuf>> {
    let mut missing = Vec::new();
    for (_, destination) in moves {
        let mut cursor = destination.parent();
        while let Some(directory) = cursor {
            if directory.exists() {
                break;
            }
            missing.push(directory.to_path_buf());
            cursor = directory.parent();
        }
    }
    missing.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    missing.dedup();

    let mut created = Vec::with_capacity(missing.len());
    for directory in missing {
        if let Err(error) = fs::create_dir(&directory) {
            return Err(combine_io_errors(
                error,
                remove_created_dirs(&created),
                "no se pudieron limpiar los directorios creados",
            ));
        }
        created.push(directory);
    }
    Ok(created)
}

fn remove_created_dirs(created: &[PathBuf]) -> io::Result<()> {
    let mut first_error = None;
    for directory in created.iter().rev() {
        if let Err(error) = fs::remove_dir(directory) {
            if error.kind() != io::ErrorKind::NotFound && first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

pub(crate) fn run_copy_blocking(
    repo_abs: PathBuf,
    to_copy: Vec<(PathBuf, PathBuf)>,
) -> Result<(Vec<String>, Vec<String>), CommandError> {
    std::thread::spawn(move || -> std::io::Result<(Vec<String>, Vec<String>)> {
        let batch = run_copy_batch_with_hook(to_copy, |_| Ok(()))?;
        let copied = batch
            .destinations
            .into_iter()
            .map(|dest| {
                dest.strip_prefix(&repo_abs)
                    .unwrap_or(&dest)
                    .display()
                    .to_string()
            })
            .collect();
        Ok((copied, batch.warnings))
    })
    .join()
    .map_err(|_| CommandError::new("copy-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("copy-failed", format!("no se pudo copiar: {e}")))
}

pub(crate) fn run_move_blocking(
    repo_abs: PathBuf,
    to_move: Vec<(PathBuf, PathBuf)>,
) -> Result<(Vec<String>, Vec<String>), CommandError> {
    std::thread::spawn(move || -> std::io::Result<(Vec<String>, Vec<String>)> {
        let batch = run_move_batch_with_hook(to_move, |_| Ok(()))?;
        let moved = batch
            .destinations
            .into_iter()
            .map(|dest| {
                dest.strip_prefix(&repo_abs)
                    .unwrap_or(&dest)
                    .display()
                    .to_string()
            })
            .collect();
        Ok((moved, batch.warnings))
    })
    .join()
    .map_err(|_| CommandError::new("move-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("move-failed", format!("no se pudo mover: {e}")))
}

pub(crate) fn undo_backup_root(token: &str) -> Result<PathBuf, CommandError> {
    Uuid::parse_str(token)
        .map_err(|_| CommandError::new("invalid-undo-token", "token inválido"))?;
    Ok(std::env::temp_dir().join("tinto-delete-undo").join(token))
}

pub(crate) fn write_delete_manifest(
    root: &Path,
    manifest: &DeleteManifest,
) -> Result<(), CommandError> {
    fs::create_dir_all(root).map_err(|e| {
        CommandError::new(
            "undo-backup-failed",
            format!("no se pudo preparar undo: {e}"),
        )
    })?;
    let bytes = serde_json::to_vec(manifest).map_err(|e| {
        CommandError::new("undo-manifest-failed", format!("manifest inválido: {e}"))
    })?;
    fs::write(root.join("manifest.json"), bytes).map_err(|e| {
        CommandError::new(
            "undo-manifest-failed",
            format!("no se pudo escribir manifest: {e}"),
        )
    })
}

pub(crate) fn read_delete_manifest(root: &Path) -> Result<DeleteManifest, CommandError> {
    let bytes = fs::read(root.join("manifest.json")).map_err(|e| {
        CommandError::new(
            "undo-manifest-missing",
            format!("no se pudo leer manifest: {e}"),
        )
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| CommandError::new("undo-manifest-invalid", format!("manifest inválido: {e}")))
}

pub(crate) fn read_bound_delete_manifest(
    repo: &Path,
    token: &str,
) -> Result<DeleteManifest, CommandError> {
    let backup_root = undo_backup_root(token)?;
    let manifest = read_delete_manifest(&backup_root)?;
    if manifest.token != token {
        return Err(CommandError::new(
            "undo-manifest-invalid",
            "el token no coincide con su manifest de recuperación",
        ));
    }
    let requested_repo = repo.canonicalize().map_err(|error| {
        CommandError::new(
            "repository-not-found",
            format!("no se pudo resolver el repositorio: {error}"),
        )
    })?;
    let manifest_repo = manifest.repo.canonicalize().map_err(|error| {
        CommandError::new(
            "undo-repo-mismatch",
            format!("el repositorio original del token ya no está disponible: {error}"),
        )
    })?;
    if requested_repo != manifest_repo {
        return Err(CommandError::new(
            "undo-repo-mismatch",
            "el token de recuperación pertenece a otro repositorio",
        ));
    }
    Ok(manifest)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ReplayDirection {
    Restore,
    Redo,
}

pub(crate) fn plan_delete_replay(
    repo: &Path,
    manifest: &DeleteManifest,
    direction: ReplayDirection,
) -> Result<Vec<(PathBuf, PathBuf)>, CommandError> {
    let objects_root = undo_backup_root(&manifest.token)?.join("objects");
    let mut moves = Vec::with_capacity(manifest.entries.len());
    for entry in &manifest.entries {
        let repo_path = safe_join(repo, &entry.path)?;
        if repo_path == repo {
            return Err(CommandError::new(
                "delete-replay-root-forbidden",
                "no se puede restaurar ni eliminar el root del repo",
            ));
        }
        let backup = objects_root.join(&entry.backup_name);
        let repo_exists = repo_path.exists();
        let backup_exists = backup.exists();
        match (direction, repo_exists, backup_exists) {
            (ReplayDirection::Restore, false, true) => moves.push((backup, repo_path)),
            (ReplayDirection::Redo, true, false) => moves.push((repo_path, backup)),
            // La entrada ya alcanzó el estado solicitado. Esto vuelve a hacer
            // reintentable un lote cuyo rollback fue parcial.
            (ReplayDirection::Restore, true, false) | (ReplayDirection::Redo, false, true) => {}
            (_, true, true) => {
                return Err(CommandError::new(
                    "delete-replay-conflict",
                    format!(
                        "{} existe tanto en el repositorio como en el respaldo",
                        entry.path.display()
                    ),
                ));
            }
            (_, false, false) => {
                return Err(CommandError::new(
                    "delete-replay-missing",
                    format!(
                        "{} no existe ni en el repositorio ni en el respaldo",
                        entry.path.display()
                    ),
                ));
            }
        }
    }
    Ok(moves)
}
