//! Tauri commands for filesystem mutations scoped to known repos. Each
//! command validates paths stay inside the repo and outside `.git/`. Conflicts
//! (destination already exists) are reported; the caller sets the overwrite
//! policy. Never auto-overwrites.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use tauri::State;

use crate::bus::BusHandle;
use crate::bus::commands::{CommandError, ensure_known};

use super::{FileConflict, FileConflictKind, safe_join};

/// Recursively copia un archivo o directorio a `dest`.
fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CopyResult {
    /// Paths relativos al repo de los archivos recién creados/actualizados.
    pub copied: Vec<String>,
    /// Conflictos detectados (vacío si todo OK).
    pub conflicts: Vec<FileConflict>,
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
    dest_dir: PathBuf, // relativo al repo; "" = raíz
    sources: Vec<PathBuf>, // absolutos del OS
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let repo_abs = ensure_known(&bus, &repo).await?;
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
        let dest_rel = dest
            .strip_prefix(&repo_abs)
            .unwrap_or(&dest)
            .to_path_buf();

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
    dest_dir: PathBuf, // relativa al repo
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let repo_abs = ensure_known(&bus, &repo).await?;
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
        let dest_rel = dest
            .strip_prefix(&repo_abs)
            .unwrap_or(&dest)
            .to_path_buf();
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
    dest_dir: PathBuf, // relativa al repo
    overwrite: bool,
) -> Result<CopyResult, CommandError> {
    let repo_abs = ensure_known(&bus, &repo).await?;
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
        let dest_rel = dest
            .strip_prefix(&repo_abs)
            .unwrap_or(&dest)
            .to_path_buf();
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
    dest_dir: PathBuf, // absoluto del OS
) -> Result<(), CommandError> {
    let repo_abs = ensure_known(&bus, &repo).await?;
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

fn run_copy_blocking(
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
            out.push(dest
                .strip_prefix(&repo_abs)
                .unwrap_or(&dest)
                .display()
                .to_string());
        }
        Ok(out)
    })
    .join()
    .map_err(|_| CommandError::new("copy-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("copy-failed", format!("no se pudo copiar: {e}")))
}

fn run_move_blocking(
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
            out.push(dest
                .strip_prefix(&repo_abs)
                .unwrap_or(&dest)
                .display()
                .to_string());
        }
        Ok(out)
    })
    .join()
    .map_err(|_| CommandError::new("move-panic", "thread panicked"))?
    .map_err(|e| CommandError::new("move-failed", format!("no se pudo mover: {e}")))
}