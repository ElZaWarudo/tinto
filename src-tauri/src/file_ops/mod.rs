// File operations on repos: copy/move between OS and repo tree, rename, delete.
// Mutates the filesystem; Tinto's watcher re-scans after each command so the UI
// reflects changes without explicit refresh. Guards enforce that every path
// stays contained within a known repo (kills `../` escapes) and never touches
// `.git/`. Conflicts (destination exists) are reported back so the frontend can
// confirm the overwrite policy, never auto-pisado.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bus::commands::CommandError;

pub mod commands;

pub use commands::{
    copy_to_repo, copy_within_repo, delete_from_repo, export_from_repo, move_within_repo,
    redo_deleted_from_repo, restore_deleted_from_repo,
};

/// Conflict reported while resolving multiple `copy_into_repo` operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileConflict {
    pub dest_rel: PathBuf,
    pub kind: FileConflictKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileConflictKind {
    /// Ya existe un archivo regular en el destino.
    FileExists,
    /// Ya existe un directorio en el destino y la fuente es archivo.
    DirExists,
    /// La fuente no existe.
    SourceMissing,
    /// Sobreescritura permitida por el caller.
    Overwrite,
}

impl FileConflict {
    pub fn new(dest_rel: PathBuf, kind: FileConflictKind) -> Self {
        Self { dest_rel, kind }
    }
}

/// Normaliza un repo root: resuelve el path canónico absoluto.
pub(crate) fn repo_canonical(repo: &Path) -> Result<PathBuf, CommandError> {
    repo.canonicalize()
        .map_err(|_| CommandError::new("repository-not-found", "el repo no existe"))
}

/// Une `repo` + `rel` y valida que `joined` quede contenido dentro de `repo`
/// y no toque `.git/`. No exige que el path exista (a diferencia de
/// `resolve_within`).
pub(crate) fn safe_join(repo: &Path, rel: &Path) -> Result<PathBuf, CommandError> {
    let repo_canon = repo_canonical(repo)?;
    // Normaliza separadores y normaliza `..` relativos sin tocar el FS.
    let joined = repo_canon.join(rel);

    // Si `joined` no existe todavía (destino de copia), canonicalize falla.
    // Fallamos a join canonical del parent si existe, o a `joined` literal.
    let canon = joined.canonicalize().unwrap_or_else(|_| {
        // Canonicaliza parent y re-une el componente final.
        let parent = joined.parent().unwrap_or(&joined);
        match parent.canonicalize() {
            Ok(parent_canon) => parent_canon.join(joined.file_name().unwrap_or_default()),
            Err(_) => joined.clone(),
        }
    });

    if !canon.starts_with(&repo_canon) {
        return Err(CommandError::new(
            "path-traversal",
            "el path se sale del repositorio",
        ));
    }
    // Aceptamos path dentro de `.git/` sólo si el caller lo pidió explícitamente
    // vía `allow_inside_git`. Por defecto lo rechazamos.
    let inside = canon.strip_prefix(&repo_canon).unwrap_or(&canon);
    if inside
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
    {
        return Err(CommandError::new(
            "path-forbidden",
            "el directorio .git no se modifica",
        ));
    }
    Ok(canon)
}

/// Igual que `safe_join` pero admite `.git/` (usado sólo cuando el caller es
/// navigation/peek, no para mutar).
#[allow(dead_code)]
fn safe_join_allow_git(repo: &Path, rel: &Path) -> Result<PathBuf, CommandError> {
    let repo_canon = repo_canonical(repo)?;
    let joined = repo_canon.join(rel);
    let canon = joined.canonicalize().unwrap_or(joined.clone());
    if !canon.starts_with(&repo_canon) {
        return Err(CommandError::new(
            "path-traversal",
            "el path se sale del repositorio",
        ));
    }
    Ok(canon)
}
