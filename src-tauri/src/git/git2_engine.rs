use std::path::{Path, PathBuf};

use git2::{BranchType, DiffOptions, ErrorCode, Repository, StatusOptions};

use super::{
    BranchInfo, CommitInfo, DiffHunk, DiffLine, DiffLineKind, FileDiff, GitEngine, GitError,
    RepoStatus,
};

/// Implementación de [`GitEngine`] con git2-rs (libgit2 vendored).
#[derive(Debug)]
pub struct Git2Engine {
    repo_path: PathBuf,
}

impl Git2Engine {
    /// Abre un repo existente. Distingue path inexistente de path no-git.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(GitError::RepositoryNotFound(path.to_path_buf()));
        }
        match Repository::open(path) {
            Ok(_) => Ok(Self {
                repo_path: path.to_path_buf(),
            }),
            Err(e) if e.code() == ErrorCode::NotFound => {
                Err(GitError::NotARepository(path.to_path_buf()))
            }
            Err(e) => Err(GitError::Internal(e)),
        }
    }

    /// git2::Repository no es Sync; se reabre por operación. Abrir un repo es
    /// barato en libgit2 y mantiene el engine `Send + Sync` sin locks.
    fn repo(&self) -> Result<Repository, GitError> {
        Repository::open(&self.repo_path).map_err(GitError::Internal)
    }

    fn commit_info(commit: &git2::Commit<'_>) -> CommitInfo {
        CommitInfo {
            id: commit.id().to_string(),
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author: commit.author().name().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
        }
    }

    fn collect_diff(diff: &git2::Diff<'_>) -> Result<Vec<FileDiff>, GitError> {
        use std::cell::RefCell;
        // Tres callbacks de git2 mutan la misma colección; RefCell evita el
        // conflicto de préstamos (la iteración de libgit2 es secuencial).
        let files: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());

        diff.foreach(
            &mut |delta, _progress| {
                let path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .map(Path::to_path_buf)
                    .unwrap_or_default();
                let old_path = delta.old_file().path().map(Path::to_path_buf);
                let old_path = match (&old_path, &path) {
                    (Some(old), new) if old != new => Some(old.clone()),
                    _ => None,
                };
                files.borrow_mut().push(FileDiff {
                    path,
                    old_path,
                    is_binary: delta.new_file().is_binary() || delta.old_file().is_binary(),
                    hunks: Vec::new(),
                });
                true
            },
            None,
            Some(&mut |_delta, hunk| {
                if let Some(file) = files.borrow_mut().last_mut() {
                    file.hunks.push(DiffHunk {
                        old_start: hunk.old_start(),
                        new_start: hunk.new_start(),
                        lines: Vec::new(),
                    });
                }
                true
            }),
            Some(&mut |_delta, _hunk, line| {
                let kind = match line.origin() {
                    '+' => Some(DiffLineKind::Added),
                    '-' => Some(DiffLineKind::Removed),
                    ' ' => Some(DiffLineKind::Context),
                    // encabezados y metadata de hunk se ignoran: la estructura ya
                    // vive en DiffHunk
                    _ => None,
                };
                if let Some(kind) = kind {
                    if let Some(file) = files.borrow_mut().last_mut() {
                        if let Some(hunk) = file.hunks.last_mut() {
                            hunk.lines.push(DiffLine {
                                kind,
                                content: String::from_utf8_lossy(line.content())
                                    .trim_end_matches(['\n', '\r'])
                                    .to_string(),
                                old_lineno: line.old_lineno(),
                                new_lineno: line.new_lineno(),
                            });
                        }
                    }
                }
                true
            }),
        )?;

        let mut files = files.into_inner();
        // git2 marca binarios a nivel delta solo con content checks; un binario
        // modificado produce delta sin hunks de texto. Normalizamos: si quedó
        // marcado binario, no exponemos hunks.
        for file in &mut files {
            if file.is_binary {
                file.hunks.clear();
            }
        }
        Ok(files)
    }

    fn find_commit<'r>(
        repo: &'r Repository,
        commit_id: &str,
    ) -> Result<git2::Commit<'r>, GitError> {
        let oid = git2::Oid::from_str(commit_id)
            .map_err(|_| GitError::NotFound(format!("commit id inválido: {commit_id}")))?;
        repo.find_commit(oid)
            .map_err(|_| GitError::NotFound(format!("commit no encontrado: {commit_id}")))
    }
}

impl GitEngine for Git2Engine {
    fn status(&self) -> Result<RepoStatus, GitError> {
        let repo = self.repo()?;
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = repo.statuses(Some(&mut opts))?;
        let mut result = RepoStatus::default();
        for entry in statuses.iter() {
            let Ok(path) = entry.path() else { continue };
            let path = PathBuf::from(path);
            let s = entry.status();
            if s.is_wt_new() {
                result.untracked.push(path);
                continue;
            }
            if s.is_index_new()
                || s.is_index_modified()
                || s.is_index_deleted()
                || s.is_index_renamed()
                || s.is_index_typechange()
            {
                result.staged.push(path.clone());
            }
            if s.is_wt_modified() || s.is_wt_deleted() || s.is_wt_renamed() || s.is_wt_typechange()
            {
                result.modified.push(path);
            }
        }
        Ok(result)
    }

    fn branch_info(&self) -> Result<BranchInfo, GitError> {
        let repo = self.repo()?;

        let head = match repo.head() {
            Ok(head) => head,
            Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => {
                // Repo sin commits: reportamos unborn con el nombre objetivo si existe.
                let name = repo
                    .find_reference("HEAD")
                    .ok()
                    .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_string))
                    .and_then(|t| t.strip_prefix("refs/heads/").map(str::to_string));
                return Ok(BranchInfo {
                    name,
                    detached: false,
                    unborn: true,
                    ahead: None,
                    behind: None,
                });
            }
            Err(e) => return Err(GitError::Internal(e)),
        };

        let detached = repo.head_detached().unwrap_or(false);
        let name = if detached {
            None
        } else {
            head.shorthand().ok().map(str::to_string)
        };

        let (ahead, behind) = match (&name, head.target()) {
            (Some(branch_name), Some(local_oid)) => {
                let upstream = repo
                    .find_branch(branch_name, BranchType::Local)
                    .ok()
                    .and_then(|b| b.upstream().ok());
                match upstream.and_then(|u| u.get().target()) {
                    Some(upstream_oid) => {
                        let (a, b) = repo.graph_ahead_behind(local_oid, upstream_oid)?;
                        (Some(a), Some(b))
                    }
                    None => (None, None),
                }
            }
            _ => (None, None),
        };

        Ok(BranchInfo {
            name,
            detached,
            unborn: false,
            ahead,
            behind,
        })
    }

    fn head_commit(&self) -> Result<CommitInfo, GitError> {
        let repo = self.repo()?;
        let head = match repo.head() {
            Ok(h) => h,
            Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => {
                return Err(GitError::UnbornHead);
            }
            Err(e) => return Err(GitError::Internal(e)),
        };
        let commit = head.peel_to_commit()?;
        Ok(Self::commit_info(&commit))
    }

    fn log(&self, offset: usize, limit: usize) -> Result<Vec<CommitInfo>, GitError> {
        let repo = self.repo()?;
        // push_head sobre HEAD unborn reporta GenericError/Reference, no
        // UnbornBranch; detectamos unborn vía head() antes del revwalk.
        if let Err(e) = repo.head() {
            if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound {
                return Err(GitError::UnbornHead);
            }
            return Err(GitError::Internal(e));
        }
        let mut revwalk = match repo.revwalk() {
            Ok(r) => r,
            Err(e) => return Err(GitError::Internal(e)),
        };
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

        let mut commits = Vec::with_capacity(limit);
        for oid in revwalk.skip(offset).take(limit) {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;
            commits.push(Self::commit_info(&commit));
        }
        Ok(commits)
    }

    fn blob_at(&self, commit_id: &str, path: &Path) -> Result<Vec<u8>, GitError> {
        let repo = self.repo()?;
        let commit = Self::find_commit(&repo, commit_id)?;
        let tree = commit.tree()?;
        let entry = tree
            .get_path(path)
            .map_err(|_| GitError::NotFound(format!("path no encontrado en commit: {path:?}")))?;
        let blob = repo
            .find_blob(entry.id())
            .map_err(|_| GitError::NotFound(format!("blob no encontrado para: {path:?}")))?;
        Ok(blob.content().to_vec())
    }

    fn worktree_diff(&self) -> Result<Vec<FileDiff>, GitError> {
        let repo = self.repo()?;
        let head_tree = match repo.head() {
            Ok(head) => Some(head.peel_to_tree()?),
            Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => {
                None
            }
            Err(e) => return Err(GitError::Internal(e)),
        };

        let mut opts = DiffOptions::new();
        // Solo archivos trackeados: los untracked viven en status() (contrato del trait).
        opts.include_untracked(false);
        let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))?;
        Self::collect_diff(&diff)
    }

    fn commit_diff(&self, commit_id: &str) -> Result<Vec<FileDiff>, GitError> {
        let repo = self.repo()?;
        let commit = Self::find_commit(&repo, commit_id)?;
        let tree = commit.tree()?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree()?),
            Err(_) => None, // commit raíz: diff contra árbol vacío
        };
        let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;
        Self::collect_diff(&diff)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::TempRepo;
    use super::*;

    #[test]
    fn open_sobre_dir_no_git_devuelve_not_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = Git2Engine::open(dir.path()).unwrap_err();
        assert!(matches!(err, GitError::NotARepository(_)));
    }

    #[test]
    fn open_sobre_path_inexistente_devuelve_repository_not_found() {
        let err = Git2Engine::open("Z:/definitivamente/no/existe").unwrap_err();
        assert!(matches!(err, GitError::RepositoryNotFound(_)));
    }

    // Covers AE1
    #[test]
    fn status_distingue_modified_staged_y_untracked() {
        let fixture = TempRepo::with_initial_commit();
        fixture.write("base.txt", "contenido v2\n");
        fixture.write_and_stage("staged.txt", "nuevo staged\n");
        fixture.write("nuevo.txt", "sin trackear\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let status = engine.status().unwrap();

        assert_eq!(status.modified, vec![PathBuf::from("base.txt")]);
        assert_eq!(status.staged, vec![PathBuf::from("staged.txt")]);
        assert_eq!(status.untracked, vec![PathBuf::from("nuevo.txt")]);
    }

    // Covers AE2
    #[test]
    fn branch_info_reporta_behind_contra_upstream_local() {
        let fixture = TempRepo::with_initial_commit();
        fixture.branch_with_extra_commits("upstream-sim", 2);
        fixture.set_upstream("master", "upstream-sim");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let info = engine.branch_info().unwrap();

        assert_eq!(info.name.as_deref(), Some("master"));
        assert!(!info.unborn);
        assert_eq!(info.ahead, Some(0));
        assert_eq!(info.behind, Some(2));
    }

    #[test]
    fn branch_info_en_repo_unborn_no_paniquea() {
        let fixture = TempRepo::empty();
        let engine = Git2Engine::open(fixture.path()).unwrap();
        let info = engine.branch_info().unwrap();
        assert!(info.unborn);
        assert!(info.ahead.is_none());
    }

    // Covers AE4 (variante de head_commit)
    #[test]
    fn head_commit_en_repo_unborn_devuelve_error_tipado() {
        let fixture = TempRepo::empty();
        let engine = Git2Engine::open(fixture.path()).unwrap();
        assert!(matches!(
            engine.head_commit().unwrap_err(),
            GitError::UnbornHead
        ));
    }

    #[test]
    fn log_pagina_con_offset_y_limit() {
        let fixture = TempRepo::with_initial_commit();
        fixture.commit_file("a.txt", "a\n", "segundo commit");
        fixture.commit_file("b.txt", "b\n", "tercer commit");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let page1 = engine.log(0, 2).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].summary, "tercer commit");
        assert_eq!(page1[1].summary, "segundo commit");

        let page2 = engine.log(2, 2).unwrap();
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].summary, "commit inicial");
    }

    #[test]
    fn head_commit_devuelve_metadata() {
        let fixture = TempRepo::with_initial_commit();
        let engine = Git2Engine::open(fixture.path()).unwrap();
        let head = engine.head_commit().unwrap();
        assert_eq!(head.summary, "commit inicial");
        assert_eq!(head.author, "Test");
        assert!(head.timestamp > 0);
        assert_eq!(head.id.len(), 40);
    }

    // Covers AE3
    #[test]
    fn worktree_diff_estructura_hunk_de_archivo_modificado() {
        let fixture = TempRepo::with_initial_commit();
        // base.txt del commit inicial tiene "linea 1\nlinea 2\nlinea 3\n"
        fixture.write("base.txt", "linea 1\nlinea DOS\nlinea 3\nlinea 4\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.worktree_diff().unwrap();
        assert_eq!(diffs.len(), 1);

        let file = &diffs[0];
        assert_eq!(file.path, PathBuf::from("base.txt"));
        assert!(!file.is_binary);
        assert_eq!(file.hunks.len(), 1);

        let lines = &file.hunks[0].lines;
        let removed: Vec<_> = lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Removed)
            .collect();
        let added: Vec<_> = lines
            .iter()
            .filter(|l| l.kind == DiffLineKind::Added)
            .collect();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].content, "linea 2");
        assert_eq!(removed[0].old_lineno, Some(2));
        assert_eq!(added.len(), 2);
        assert_eq!(added[0].content, "linea DOS");
        assert_eq!(added[0].new_lineno, Some(2));
    }

    #[test]
    fn worktree_diff_excluye_untracked() {
        let fixture = TempRepo::with_initial_commit();
        fixture.write("sin-trackear.txt", "nuevo\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.worktree_diff().unwrap();
        assert!(diffs.is_empty());
    }

    #[test]
    fn worktree_diff_marca_binarios_sin_hunks() {
        let fixture = TempRepo::with_initial_commit();
        fixture.commit_bytes("blob.bin", &[0u8, 159, 146, 150, 0, 255], "binario inicial");
        fixture.write_bytes("blob.bin", &[0u8, 1, 2, 3, 0, 254]);

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.worktree_diff().unwrap();
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].is_binary);
        assert!(diffs[0].hunks.is_empty());
    }

    #[test]
    fn commit_diff_de_commit_que_agrega_archivo() {
        let fixture = TempRepo::with_initial_commit();
        let id = fixture.commit_file("nuevo.txt", "uno\ndos\n", "agrega nuevo");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.commit_diff(&id).unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, PathBuf::from("nuevo.txt"));
        assert!(diffs[0]
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .all(|l| l.kind == DiffLineKind::Added));
    }

    #[test]
    fn blob_at_devuelve_contenido_historico() {
        let fixture = TempRepo::with_initial_commit();
        let first = fixture.head_id();
        fixture.commit_file("base.txt", "contenido nuevo\n", "cambia base");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let old = engine.blob_at(&first, Path::new("base.txt")).unwrap();
        assert_eq!(
            String::from_utf8(old).unwrap(),
            "linea 1\nlinea 2\nlinea 3\n"
        );
    }

    #[test]
    fn blob_at_con_commit_inexistente_devuelve_error_tipado() {
        let fixture = TempRepo::with_initial_commit();
        let engine = Git2Engine::open(fixture.path()).unwrap();
        let err = engine
            .blob_at(
                "0123456789012345678901234567890123456789",
                Path::new("base.txt"),
            )
            .unwrap_err();
        assert!(matches!(err, GitError::NotFound(_)));
    }

    #[test]
    fn status_archivo_staged_y_remodificado_aparece_en_ambas_listas() {
        let fixture = TempRepo::with_initial_commit();
        fixture.write_and_stage("doble.txt", "version staged\n");
        fixture.write("doble.txt", "version re-modificada\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let status = engine.status().unwrap();

        let doble = PathBuf::from("doble.txt");
        assert!(status.staged.contains(&doble), "debe estar en staged");
        assert!(status.modified.contains(&doble), "debe estar en modified");
    }

    #[test]
    fn status_archivo_borrado_aparece_en_modified() {
        let fixture = TempRepo::with_initial_commit();
        fixture.delete_file("base.txt");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let status = engine.status().unwrap();
        assert_eq!(status.modified, vec![PathBuf::from("base.txt")]);
        assert!(status.staged.is_empty());
    }

    #[test]
    fn status_recursa_directorios_untracked_listando_archivos() {
        let fixture = TempRepo::with_initial_commit();
        fixture.write("nuevo-dir/interno.txt", "contenido\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let status = engine.status().unwrap();
        assert_eq!(
            status.untracked,
            vec![PathBuf::from("nuevo-dir/interno.txt")],
            "lista archivos, no el directorio"
        );
    }

    #[test]
    fn todos_los_metodos_en_repo_unborn_devuelven_tipado_o_vacio() {
        let fixture = TempRepo::empty();
        let engine = Git2Engine::open(fixture.path()).unwrap();

        assert!(engine.status().unwrap().modified.is_empty());
        assert!(engine.branch_info().unwrap().unborn);
        assert!(matches!(
            engine.head_commit().unwrap_err(),
            GitError::UnbornHead
        ));
        match engine.log(0, 10) {
            Err(GitError::UnbornHead) => {}
            other => panic!("esperaba UnbornHead, fue: {other:?}"),
        }
        assert!(engine.worktree_diff().unwrap().is_empty());
    }

    #[test]
    fn log_offset_mas_alla_del_historial_devuelve_vacio() {
        let fixture = TempRepo::with_initial_commit();
        let engine = Git2Engine::open(fixture.path()).unwrap();
        assert!(engine.log(100, 10).unwrap().is_empty());
    }

    #[test]
    fn blob_at_resuelve_paths_anidados() {
        let fixture = TempRepo::with_initial_commit();
        let id = fixture.commit_file("src/profundo/archivo.rs", "fn main() {}\n", "anidado");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let content = engine
            .blob_at(&id, Path::new("src/profundo/archivo.rs"))
            .unwrap();
        assert_eq!(String::from_utf8(content).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn worktree_diff_asocia_hunks_al_archivo_correcto_con_varios_archivos() {
        let fixture = TempRepo::with_initial_commit();
        fixture.commit_file("otro.txt", "alfa\nbeta\n", "agrega otro");
        fixture.write("base.txt", "linea 1\nCAMBIO BASE\nlinea 3\n");
        fixture.write("otro.txt", "alfa\nCAMBIO OTRO\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.worktree_diff().unwrap();
        assert_eq!(diffs.len(), 2);

        for file in &diffs {
            let added: Vec<_> = file
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == DiffLineKind::Added)
                .map(|l| l.content.as_str())
                .collect();
            match file.path.to_str().unwrap() {
                "base.txt" => assert_eq!(added, vec!["CAMBIO BASE"]),
                "otro.txt" => assert_eq!(added, vec!["CAMBIO OTRO"]),
                other => panic!("archivo inesperado en diff: {other}"),
            }
        }
    }

    #[test]
    fn commit_diff_de_merge_compara_contra_primer_padre() {
        let fixture = TempRepo::with_initial_commit();
        let merge_id = fixture.merge_branch_adding("feature", "de-feature.txt", "nuevo\n");

        let engine = Git2Engine::open(fixture.path()).unwrap();
        let diffs = engine.commit_diff(&merge_id).unwrap();

        // Contra el primer padre (la línea principal), el merge introduce
        // exactamente el archivo que vino de la branch.
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, PathBuf::from("de-feature.txt"));
    }
}
