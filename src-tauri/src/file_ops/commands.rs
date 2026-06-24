//! Tauri commands for filesystem mutations scoped to known repos. Each
//! command validates paths stay inside the repo and outside `.git/`. Conflicts
//! (destination already exists) are reported; the caller sets the overwrite
//! policy. Never auto-overwrites.

use serde::{Deserialize, Serialize};
use std::fs;
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
        });
    }

    let repo_root = repo_abs.clone();
    let copied = run_copy_blocking(repo_root, to_copy)?;

    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
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
        });
    }

    let copied = run_copy_blocking(repo_abs, to_copy)?;

    Ok(CopyResult {
        copied,
        conflicts: Vec::new(),
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
        });
    }

    let moved = run_move_blocking(repo_abs, to_move)?;

    Ok(CopyResult {
        copied: moved,
        conflicts: Vec::new(),
    })
}

/// Copia archivos del repo hacia un directorio del OS (absoluto).
#[tauri::command]
pub async fn export_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
    dest_dir: PathBuf,     // absoluto del OS
) -> Result<(), CommandError> {
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

    let dest_root = dest_dir.clone();
    std::thread::spawn(move || -> std::io::Result<()> {
        for src_abs in &srcs {
            let name = src_abs.file_name().unwrap_or_default();
            let dest = dest_root.join(name);
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
    })
    .join()
    .map_err(|_| CommandError::new("export-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("export-failed", format!("no se pudo exportar: {e}")))?;

    Ok(())
}

/// Elimina archivos o directorios dentro del repo. Rechaza el root del repo y
/// cualquier path fuera del repo o dentro de `.git/` vía `safe_join`.
#[tauri::command]
pub async fn delete_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    sources: Vec<PathBuf>, // relativas al repo
) -> Result<DeleteResult, CommandError> {
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
        &DeleteResult {
            token: token.clone(),
            entries: entries.clone(),
        },
    )?;

    std::thread::spawn(move || -> std::io::Result<()> {
        fs::create_dir_all(&objects_root)?;
        for (index, (_, target, _)) in filtered.into_iter().enumerate() {
            move_path(&target, &objects_root.join(index.to_string()))?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| CommandError::new("delete-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("delete-failed", format!("no se pudo eliminar: {e}")))?;

    Ok(DeleteResult { token, entries })
}

#[tauri::command]
pub async fn restore_deleted_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    token: String,
) -> Result<(), CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_restore_deleted_from_repo(resolved, token);
    }
    let repo_abs = resolved.path;
    let backup_root = undo_backup_root(&token)?;
    let manifest = read_delete_manifest(&backup_root)?;
    let objects_root = backup_root.join("objects");
    let mut targets = Vec::with_capacity(manifest.entries.len());
    for entry in manifest.entries {
        let dest = safe_join(&repo_abs, &entry.path)?;
        if dest == repo_abs {
            return Err(CommandError::new(
                "restore-root-forbidden",
                "no se puede restaurar sobre el root del repo",
            ));
        }
        targets.push((dest, entry.path, entry.backup_name));
    }

    std::thread::spawn(move || -> std::io::Result<()> {
        for (dest, rel, backup_name) in targets {
            if dest.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} ya existe", rel.display()),
                ));
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            move_path(&objects_root.join(backup_name), &dest)?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| CommandError::new("restore-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("restore-failed", format!("no se pudo restaurar: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn redo_deleted_from_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    token: String,
) -> Result<(), CommandError> {
    let resolved = resolve_file_op_repo(&bus, &repo).await?;
    if resolved.source == RepoSource::Wsl {
        return wsl_redo_deleted_from_repo(resolved, token);
    }
    let repo_abs = resolved.path;
    let backup_root = undo_backup_root(&token)?;
    let manifest = read_delete_manifest(&backup_root)?;
    let objects_root = backup_root.join("objects");
    let mut targets = Vec::with_capacity(manifest.entries.len());
    for entry in manifest.entries {
        let src = safe_join(&repo_abs, &entry.path)?;
        if src == repo_abs {
            return Err(CommandError::new(
                "redo-root-forbidden",
                "no se puede rehacer el borrado del root del repo",
            ));
        }
        targets.push((src, entry.path, entry.backup_name));
    }

    std::thread::spawn(move || -> std::io::Result<()> {
        fs::create_dir_all(&objects_root)?;
        for (src, rel, backup_name) in targets {
            if !src.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("{} no existe", rel.display()),
                ));
            }
            let backup = objects_root.join(backup_name);
            if backup.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("backup {} ya existe", backup.display()),
                ));
            }
            move_path(&src, &backup)?;
        }
        Ok(())
    })
    .join()
    .map_err(|_| CommandError::new("redo-delete-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("redo-delete-failed", format!("no se pudo rehacer: {e}")))?;

    Ok(())
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
) -> Result<(), CommandError> {
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
        AgentResponse::Unit => Ok(()),
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
) -> Result<(), CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::RestoreDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            token,
        },
    )? {
        AgentResponse::Unit => Ok(()),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn wsl_redo_deleted_from_repo(resolved: ResolvedRepo, token: String) -> Result<(), CommandError> {
    match wsl_request(
        resolved.distro,
        AgentRequest::RedoDeletedFromRepo {
            protocol_version: PROTOCOL_VERSION,
            repo: resolved.path,
            allowed_repos: resolved.wsl_repos,
            token,
        },
    )? {
        AgentResponse::Unit => Ok(()),
        response => Err(unexpected_file_ops_wsl_response(response)),
    }
}

fn unexpected_file_ops_wsl_response(_response: AgentResponse) -> CommandError {
    CommandError::new("malformed_response", "respuesta inesperada del agente WSL")
}

pub(crate) fn run_copy_blocking(
    repo_abs: PathBuf,
    to_copy: Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<String>, CommandError> {
    std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut out = Vec::with_capacity(to_copy.len());
        for (src, dest) in to_copy {
            if dest.exists() {
                if dest.is_dir() {
                    fs::remove_dir_all(&dest)?;
                } else {
                    fs::remove_file(&dest)?;
                }
            }
            copy_recursive(&src, &dest)?;
            out.push(
                dest.strip_prefix(&repo_abs)
                    .unwrap_or(&dest)
                    .display()
                    .to_string(),
            );
        }
        Ok(out)
    })
    .join()
    .map_err(|_| CommandError::new("copy-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("copy-failed", format!("no se pudo copiar: {e}")))
}

pub(crate) fn run_move_blocking(
    repo_abs: PathBuf,
    to_move: Vec<(PathBuf, PathBuf)>,
) -> Result<Vec<String>, CommandError> {
    std::thread::spawn(move || -> std::io::Result<Vec<String>> {
        let mut out = Vec::with_capacity(to_move.len());
        for (src, dest) in to_move {
            if dest.exists() {
                if dest.is_dir() {
                    fs::remove_dir_all(&dest)?;
                } else {
                    fs::remove_file(&dest)?;
                }
            }
            if fs::rename(&src, &dest).is_err() {
                copy_recursive(&src, &dest)?;
                if src.is_dir() {
                    fs::remove_dir_all(&src)?;
                } else {
                    fs::remove_file(&src)?;
                }
            }
            out.push(
                dest.strip_prefix(&repo_abs)
                    .unwrap_or(&dest)
                    .display()
                    .to_string(),
            );
        }
        Ok(out)
    })
    .join()
    .map_err(|_| CommandError::new("move-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("move-failed", format!("no se pudo mover: {e}")))
}

pub(crate) fn move_path(src: &Path, dest: &Path) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(src, dest).is_err() {
        copy_recursive(src, dest)?;
        if src.is_dir() {
            fs::remove_dir_all(src)?;
        } else {
            fs::remove_file(src)?;
        }
    }
    Ok(())
}

pub(crate) fn undo_backup_root(token: &str) -> Result<PathBuf, CommandError> {
    Uuid::parse_str(token)
        .map_err(|_| CommandError::new("invalid-undo-token", "token inválido"))?;
    Ok(std::env::temp_dir().join("tinto-delete-undo").join(token))
}

pub(crate) fn write_delete_manifest(
    root: &Path,
    manifest: &DeleteResult,
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

pub(crate) fn read_delete_manifest(root: &Path) -> Result<DeleteResult, CommandError> {
    let bytes = fs::read(root.join("manifest.json")).map_err(|e| {
        CommandError::new(
            "undo-manifest-missing",
            format!("no se pudo leer manifest: {e}"),
        )
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| CommandError::new("undo-manifest-invalid", format!("manifest inválido: {e}")))
}
