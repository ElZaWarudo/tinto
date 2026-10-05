//! Capa de lectura de git del Plano 1 (archivos trackeados).
//!
//! Todo acceso a git pasa por el trait [`GitEngine`]; los consumidores (bus de
//! eventos, timeline, diff viewer) nunca dependen de git2 directamente. La
//! implementación por defecto es [`Git2Engine`] (libgit2 vendored).
//!
//! # Escape hatch a CLI
//!
//! git2-rs puede ser más lento que el binario `git` nativo calculando status y
//! diffs en repos muy grandes (monorepos). Si un baseline futuro muestra que el
//! p95 de `status`/`worktree_diff` excede el presupuesto del live diff, la
//! salida es introducir un `GitCliEngine` detrás de este mismo trait. El
//! criterio numérico queda pendiente de medir; no se implementa ahora.
//!
//! # Paths
//!
//! Los paths devueltos son relativos a la raíz del repo y se serializan con el
//! separador del SO (backslash en Windows). La normalización a forward-slash
//! para el frontend, si hace falta, la decide el contrato de eventos.

mod git2_engine;
#[cfg(test)]
pub(crate) mod test_fixtures;

pub use git2_engine::Git2Engine;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Error tipado de la capa de git. Ningún path de error esperable hace panic.
#[derive(Debug, Error)]
pub enum GitError {
    /// El path no existe o no es accesible.
    #[error("el path no existe o no es accesible: {0}")]
    RepositoryNotFound(PathBuf),
    /// El path existe pero no es un repositorio git.
    #[error("el path no es un repositorio git: {0}")]
    NotARepository(PathBuf),
    /// El repo no tiene commits todavía (HEAD unborn).
    #[error("el repositorio no tiene commits todavía")]
    UnbornHead,
    /// No se encontró el objeto pedido (commit, blob, ref).
    #[error("objeto no encontrado: {0}")]
    NotFound(String),
    /// Cualquier otro error interno de git2.
    #[error("error interno de git: {0}")]
    Internal(#[from] git2::Error),
}

impl GitError {
    /// Categoría kebab-case estable para el contrato (frozen): única fuente
    /// de verdad compartida por el path de delta (`git_error_state`) y el de
    /// comandos (`CommandError`).
    pub fn category(&self) -> &'static str {
        match self {
            GitError::RepositoryNotFound(_) => "repository-not-found",
            GitError::NotARepository(_) => "not-a-repository",
            GitError::UnbornHead => "unborn-head",
            GitError::NotFound(_) => "not-found",
            GitError::Internal(_) => "internal",
        }
    }
}

/// Status del working tree de un repo (Plano 1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepoStatus {
    /// Archivos trackeados con cambios sin stagear (bits WT_*).
    pub modified: Vec<PathBuf>,
    /// Archivos con cambios staged en el índice (bits INDEX_*).
    /// Un archivo staged y luego re-modificado aparece en ambas listas.
    pub staged: Vec<PathBuf>,
    /// Archivos nuevos sin trackear (no ignorados).
    pub untracked: Vec<PathBuf>,
}

/// Status via the `git` CLI for repos on Windows drives seen from WSL
/// (`/mnt/<letter>/...`). libgit2 stats every untracked entry, which costs
/// seconds per poll over the drvfs mount; `git status` takes ~0.1 s there.
/// Returns `None` when the path is not on such a mount or `git` fails, so the
/// caller falls back to libgit2.
pub fn drvfs_cli_status(repo: &Path) -> Option<RepoStatus> {
    if !is_wsl_windows_drive_path(repo) {
        return None;
    }
    let output = std::process::Command::new("git")
        .args([
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ])
        .current_dir(repo)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_porcelain_v1_z(&output.stdout))
}

fn is_wsl_windows_drive_path(path: &Path) -> bool {
    cfg!(target_os = "linux")
        && path
            .to_str()
            .and_then(|path| path.strip_prefix("/mnt/"))
            .is_some_and(|rest| {
                let mut chars = rest.chars();
                chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                    && matches!(chars.next(), None | Some('/'))
            })
}

/// Maps `git status --porcelain=v1 -z --no-renames` to the same buckets the
/// libgit2 engine uses: `??` untracked, index column staged, worktree column
/// modified. Unmerged entries are skipped, as libgit2 reports them only as
/// conflicted.
fn parse_porcelain_v1_z(stdout: &[u8]) -> RepoStatus {
    let mut status = RepoStatus::default();
    for record in stdout.split(|byte| *byte == 0) {
        if record.len() < 4 {
            continue;
        }
        let (x, y) = (record[0], record[1]);
        let path = PathBuf::from(String::from_utf8_lossy(&record[3..]).into_owned());
        if x == b'?' && y == b'?' {
            status.untracked.push(path);
            continue;
        }
        if x == b'U' || y == b'U' || (x == b'A' && y == b'A') || (x == b'D' && y == b'D') {
            continue;
        }
        if matches!(x, b'A' | b'M' | b'D' | b'R' | b'T' | b'C') {
            status.staged.push(path.clone());
        }
        if matches!(y, b'M' | b'D' | b'T') {
            status.modified.push(path);
        }
    }
    status
}

/// Branch actual y divergencia con su upstream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchInfo {
    /// Nombre de la branch, o `None` en detached HEAD.
    pub name: Option<String>,
    /// HEAD apunta directo a un commit (no a una branch).
    pub detached: bool,
    /// Repo recién inicializado sin commits.
    pub unborn: bool,
    /// Commits locales por encima del upstream, si hay upstream.
    pub ahead: Option<usize>,
    /// Commits del upstream que faltan localmente, si hay upstream.
    pub behind: Option<usize>,
}

/// Metadata de un commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitInfo {
    /// Id (oid hex completo).
    pub id: String,
    /// Primera línea del mensaje.
    pub summary: String,
    /// Nombre del autor.
    pub author: String,
    /// Timestamp del commit en segundos unix.
    pub timestamp: i64,
}

/// Tipo de línea dentro de un hunk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiffLineKind {
    Added,
    Removed,
    Context,
}

/// Una línea de diff con sus números de línea en ambos lados.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    /// Número de línea en el lado viejo (`None` para Added).
    pub old_lineno: Option<u32>,
    /// Número de línea en el lado nuevo (`None` para Removed).
    pub new_lineno: Option<u32>,
}

/// Un hunk de diff contiguo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

/// Diff de un archivo: datos estructurados, sin render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiff {
    /// Path actual del archivo, relativo al repo.
    pub path: PathBuf,
    /// Path anterior cuando hubo rename, si difiere de `path`.
    pub old_path: Option<PathBuf>,
    /// Archivo binario: sin hunks de texto.
    pub is_binary: bool,
    pub hunks: Vec<DiffHunk>,
}

/// Acceso de solo lectura a un repositorio git.
///
/// El trait es síncrono (git2 lo es); los consumidores async deben envolver las
/// llamadas (p. ej. `spawn_blocking`). Las implementaciones deben ser
/// `Send + Sync` para compartirse entre tareas.
///
/// Tinto nunca escribe sobre los repos: este trait no tiene (ni tendrá)
/// operaciones de escritura.
pub trait GitEngine: Send + Sync {
    /// Status del working tree: modificados, staged y untracked.
    fn status(&self) -> Result<RepoStatus, GitError>;

    /// Branch actual y ahead/behind respecto a su upstream cuando existe.
    fn branch_info(&self) -> Result<BranchInfo, GitError>;

    /// Último commit de HEAD. `Err(UnbornHead)` en repos sin commits.
    fn head_commit(&self) -> Result<CommitInfo, GitError>;

    /// Log de commits desde HEAD, paginado por offset/limit.
    ///
    /// La paginación por offset asume historia estable durante la sesión; un
    /// cursor por oid puede añadirse sin romper el trait si hiciera falta.
    fn log(&self, offset: usize, limit: usize) -> Result<Vec<CommitInfo>, GitError>;

    /// Contenido de un archivo en un commit dado.
    fn blob_at(&self, commit_id: &str, path: &Path) -> Result<Vec<u8>, GitError>;

    /// Diff del working tree (incluye cambios staged) contra HEAD.
    /// Los archivos untracked no aparecen aquí; viven en [`GitEngine::status`].
    fn worktree_diff(&self) -> Result<Vec<FileDiff>, GitError>;

    /// Diff de un commit contra su primer padre (o contra el árbol vacío para
    /// el commit raíz).
    fn commit_diff(&self, commit_id: &str) -> Result<Vec<FileDiff>, GitError>;
}

#[cfg(test)]
mod porcelain_tests {
    use super::*;

    #[test]
    fn porcelain_maps_like_the_libgit2_engine() {
        let stdout = b"?? new.txt\0M  staged.rs\0MM both.rs\0 M changed.rs\0 D gone.rs\0D  removed.rs\0UU conflict.rs\0A  added dir/x y.md\0";
        let status = parse_porcelain_v1_z(stdout);
        assert_eq!(status.untracked, vec![PathBuf::from("new.txt")]);
        assert_eq!(
            status.staged,
            vec![
                PathBuf::from("staged.rs"),
                PathBuf::from("both.rs"),
                PathBuf::from("removed.rs"),
                PathBuf::from("added dir/x y.md"),
            ]
        );
        assert_eq!(
            status.modified,
            vec![
                PathBuf::from("both.rs"),
                PathBuf::from("changed.rs"),
                PathBuf::from("gone.rs"),
            ]
        );
    }

    #[test]
    fn only_windows_drive_mounts_use_the_cli() {
        let linux = cfg!(target_os = "linux");
        assert_eq!(
            is_wsl_windows_drive_path(Path::new("/mnt/c/Users/me/repo")),
            linux
        );
        assert!(!is_wsl_windows_drive_path(Path::new("/mnt/wsl/repo")));
        assert!(!is_wsl_windows_drive_path(Path::new("/home/me/repo")));
    }
}
