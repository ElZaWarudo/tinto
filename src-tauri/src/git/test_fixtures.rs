//! Helpers de test: repos git temporales reales creados con git2.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use git2::{Repository, Signature, Time};
use tempfile::TempDir;

/// Repo git temporal; se borra al drop.
pub(crate) struct TempRepo {
    dir: TempDir,
    /// Timestamp creciente por commit: commits en el mismo segundo de reloj
    /// real romperían el orden TIME del revwalk (flake en CI).
    next_ts: Cell<i64>,
}

impl TempRepo {
    /// Repo recién `git init`, sin commits (HEAD unborn).
    pub(crate) fn empty() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        Repository::init(dir.path()).expect("git init");
        Self {
            dir,
            next_ts: Cell::new(1_750_000_000),
        }
    }

    /// Repo con un commit inicial que contiene `base.txt` (3 líneas).
    pub(crate) fn with_initial_commit() -> Self {
        let fixture = Self::empty();
        fixture.write("base.txt", "linea 1\nlinea 2\nlinea 3\n");
        fixture.stage_all_and_commit("commit inicial");
        fixture
    }

    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }

    fn repo(&self) -> Repository {
        Repository::open(self.path()).expect("open repo")
    }

    fn signature(&self) -> Signature<'static> {
        let ts = self.next_ts.get();
        self.next_ts.set(ts + 60);
        Signature::new("Test", "test@tinto.dev", &Time::new(ts, 0)).expect("signature")
    }

    /// Escribe (o sobrescribe) un archivo sin stagear.
    pub(crate) fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    pub(crate) fn write_bytes(&self, rel: &str, content: &[u8]) {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, content).expect("write");
    }

    /// Escribe un archivo y lo stagea (queda en el índice, sin commit).
    pub(crate) fn write_and_stage(&self, rel: &str, content: &str) {
        self.write(rel, content);
        let repo = self.repo();
        let mut index = repo.index().expect("index");
        index.add_path(&PathBuf::from(rel)).expect("add");
        index.write().expect("index write");
    }

    fn stage_all_and_commit(&self, message: &str) -> String {
        let repo = self.repo();
        let mut index = repo.index().expect("index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("add all");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let sig = self.signature();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .expect("commit");
        oid.to_string()
    }

    /// Escribe + stagea + commitea un archivo de texto; devuelve el oid.
    pub(crate) fn commit_file(&self, rel: &str, content: &str, message: &str) -> String {
        self.write(rel, content);
        self.stage_and_commit_path(rel, message)
    }

    /// Igual que `commit_file` pero con bytes arbitrarios (binarios).
    pub(crate) fn commit_bytes(&self, rel: &str, content: &[u8], message: &str) -> String {
        self.write_bytes(rel, content);
        self.stage_and_commit_path(rel, message)
    }

    fn stage_and_commit_path(&self, rel: &str, message: &str) -> String {
        let repo = self.repo();
        let mut index = repo.index().expect("index");
        index.add_path(&PathBuf::from(rel)).expect("add");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let sig = self.signature();
        let parent = repo.head().expect("head").peel_to_commit().expect("commit");
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .expect("commit");
        oid.to_string()
    }

    /// Oid hex del commit de HEAD.
    pub(crate) fn head_id(&self) -> String {
        self.repo()
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("commit")
            .id()
            .to_string()
    }

    /// Crea una branch desde HEAD con `n` commits extra (sin mover HEAD).
    pub(crate) fn branch_with_extra_commits(&self, name: &str, n: usize) {
        let repo = self.repo();
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch(name, &head, false).expect("branch");

        let sig = self.signature();
        let mut parent_oid = head.id();
        for i in 0..n {
            let parent = repo.find_commit(parent_oid).expect("parent");
            let tree = parent.tree().expect("tree");
            parent_oid = repo
                .commit(
                    Some(&format!("refs/heads/{name}")),
                    &sig,
                    &sig,
                    &format!("commit upstream {i}"),
                    &tree,
                    &[&parent],
                )
                .expect("commit en branch");
        }
    }

    /// Borra un archivo del working tree (sin stagear el borrado).
    pub(crate) fn delete_file(&self, rel: &str) {
        fs::remove_file(self.path().join(rel)).expect("remove");
    }

    /// Crea una branch con un commit que añade `rel`, y la mergea a HEAD con
    /// un merge commit real de dos padres. Devuelve el oid del merge.
    pub(crate) fn merge_branch_adding(&self, branch: &str, rel: &str, content: &str) -> String {
        let repo = self.repo();
        let head = repo.head().expect("head").peel_to_commit().expect("commit");
        repo.branch(branch, &head, false).expect("branch");

        // Commit en la branch: tree de HEAD + archivo nuevo.
        let sig = self.signature();
        let mut index = repo.index().expect("index");
        self.write(rel, content);
        index.add_path(&PathBuf::from(rel)).expect("add");
        index.write().expect("index write");
        let tree_id = index.write_tree().expect("tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let branch_oid = repo
            .commit(
                Some(&format!("refs/heads/{branch}")),
                &sig,
                &sig,
                &format!("agrega {rel} en {branch}"),
                &tree,
                &[&head],
            )
            .expect("commit branch");
        let branch_commit = repo.find_commit(branch_oid).expect("branch commit");

        // Merge commit con ambos padres sobre HEAD.
        let mut merged_index = repo
            .merge_commits(&head, &branch_commit, None)
            .expect("merge commits");
        assert!(!merged_index.has_conflicts(), "fixture sin conflictos");
        let merged_tree_id = merged_index.write_tree_to(&repo).expect("merged tree");
        let merged_tree = repo.find_tree(merged_tree_id).expect("tree");
        let sig = self.signature();
        let merge_oid = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("merge {branch}"),
                &merged_tree,
                &[&head, &branch_commit],
            )
            .expect("merge commit");
        // Sincroniza el working tree con el merge para dejar el repo limpio.
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .expect("checkout");
        merge_oid.to_string()
    }

    /// Configura `branch.<local>.remote/merge` para usar otra branch local
    /// como upstream (sin remote real).
    pub(crate) fn set_upstream(&self, local: &str, upstream: &str) {
        let repo = self.repo();
        let mut config = repo.config().expect("config");
        config
            .set_str(&format!("branch.{local}.remote"), ".")
            .expect("remote .");
        config
            .set_str(
                &format!("branch.{local}.merge"),
                &format!("refs/heads/{upstream}"),
            )
            .expect("merge ref");
    }
}
