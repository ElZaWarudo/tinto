//! Workbenches: conjuntos nombrados de repos a monitorear (diseño §2) con
//! persistencia TOML en el config dir del SO (diseño §8).
//!
//! El store mantiene la config en memoria y persiste cada mutación antes de
//! devolver Ok (escritura atómica: tmp + rename, que en Windows reemplaza el
//! destino vía MoveFileExW). La config corrupta se reporta sin sobrescribir
//! el archivo.

mod autodetect;
pub mod commands;

pub use autodetect::autodetect_repos;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::git::{Git2Engine, GitError};

pub const CONFIG_FILE: &str = "workbenches.toml";

/// Error del workbench manager, serializable hacia el frontend.
#[derive(Debug, Error)]
pub enum WorkbenchError {
    #[error("la configuración está corrupta y no se sobrescribió: {0}")]
    CorruptConfig(String),
    #[error("error de disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("ya existe un workbench llamado `{0}`")]
    DuplicateWorkbench(String),
    #[error("no existe el workbench `{0}`")]
    UnknownWorkbench(String),
    #[error("el repo ya está en el workbench: {0}")]
    DuplicateRepo(PathBuf),
    #[error("el repo no está en el workbench: {0}")]
    UnknownRepo(PathBuf),
    #[error("no es un repositorio git válido: {0}")]
    InvalidRepo(String),
    #[error("no se pudo resolver el directorio de configuración del SO")]
    NoConfigDir,
    #[error("el store de workbenches quedó inutilizable tras un error interno; reinicia la app")]
    StoreLocked,
}

impl From<GitError> for WorkbenchError {
    fn from(e: GitError) -> Self {
        WorkbenchError::InvalidRepo(e.to_string())
    }
}

impl Serialize for WorkbenchError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let kind = match self {
            WorkbenchError::CorruptConfig(_) => "corrupt_config",
            WorkbenchError::Io(_) => "io",
            WorkbenchError::DuplicateWorkbench(_) => "duplicate_workbench",
            WorkbenchError::UnknownWorkbench(_) => "unknown_workbench",
            WorkbenchError::DuplicateRepo(_) => "duplicate_repo",
            WorkbenchError::UnknownRepo(_) => "unknown_repo",
            WorkbenchError::InvalidRepo(_) => "invalid_repo",
            WorkbenchError::NoConfigDir => "no_config_dir",
            WorkbenchError::StoreLocked => "store_locked",
        };
        let mut s = serializer.serialize_struct("WorkbenchError", 2)?;
        s.serialize_field("kind", kind)?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

/// Un repo dentro de un workbench.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoEntry {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Patrones opt-in del Plano 2 (gitignoreados a vigilar igual).
    #[serde(default)]
    pub fs_watch: Vec<String>,
}

/// Conjunto nombrado de repos (diseño §2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workbench {
    pub name: String,
    #[serde(default, rename = "repos")]
    pub repos: Vec<RepoEntry>,
}

/// Config completa persistida.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkbenchConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default, rename = "workbench")]
    pub workbenches: Vec<Workbench>,
}

impl Default for WorkbenchConfig {
    fn default() -> Self {
        Self {
            version: 1,
            active: None,
            workbenches: Vec::new(),
        }
    }
}

/// Store de workbenches: config en memoria + persistencia explícita.
#[derive(Debug)]
pub struct WorkbenchStore {
    config_dir: PathBuf,
    config: WorkbenchConfig,
    /// Modo degradado (config corrupta descartada en memoria): el primer
    /// persist respalda el archivo original a `.corrupt` antes de escribir.
    degraded: bool,
}

impl WorkbenchStore {
    /// Store de producción: config dir del SO (`<config>/tinto`).
    pub fn open_default() -> Result<Self, WorkbenchError> {
        let base = dirs::config_dir().ok_or(WorkbenchError::NoConfigDir)?;
        Self::open(base.join("tinto"))
    }

    /// Abre (o inicializa vacío) el store en un config dir concreto.
    /// Config corrupta → error tipado, el archivo queda intacto (R3).
    pub fn open(config_dir: impl Into<PathBuf>) -> Result<Self, WorkbenchError> {
        let config_dir = config_dir.into();
        let file = config_dir.join(CONFIG_FILE);
        let mut config = if file.is_file() {
            let raw = fs::read_to_string(&file)?;
            toml::from_str::<WorkbenchConfig>(&raw)
                .map_err(|e| WorkbenchError::CorruptConfig(e.to_string()))?
        } else {
            WorkbenchConfig::default()
        };
        // Activo colgante → degradar sin error (R4).
        if let Some(active) = &config.active {
            if !config.workbenches.iter().any(|w| &w.name == active) {
                config.active = None;
            }
        }
        Ok(Self {
            config_dir,
            config,
            degraded: false,
        })
    }

    /// Store con config vacía en memoria sobre un dir concreto, SIN leer ni
    /// tocar el archivo existente (modo degradado ante config corrupta). El
    /// archivo original se respalda a `workbenches.toml.corrupt` en el primer
    /// persist provocado por una mutación explícita del usuario.
    pub fn with_default_config(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            config: WorkbenchConfig::default(),
            degraded: true,
        }
    }

    pub fn config(&self) -> &WorkbenchConfig {
        &self.config
    }

    pub fn active_workbench(&self) -> Option<&Workbench> {
        let name = self.config.active.as_deref()?;
        self.config.workbenches.iter().find(|w| w.name == name)
    }

    /// Persistencia atómica: tmp único por proceso + rename (reemplaza
    /// destino) — R5. En modo degradado, respalda primero el archivo
    /// corrupto original a `.corrupt` (recuperable a mano).
    fn persist(&mut self) -> Result<(), WorkbenchError> {
        fs::create_dir_all(&self.config_dir)?;
        let target = self.config_dir.join(CONFIG_FILE);
        if self.degraded && target.is_file() {
            let backup = self.config_dir.join(format!("{CONFIG_FILE}.corrupt"));
            fs::copy(&target, backup)?;
            self.degraded = false;
        }
        let body = toml::to_string_pretty(&self.config)
            .map_err(|e| WorkbenchError::CorruptConfig(e.to_string()))?;
        // Sufijo por proceso: dos instancias de Tinto no comparten tmp.
        let tmp = self
            .config_dir
            .join(format!("{CONFIG_FILE}.{}.tmp", std::process::id()));
        fs::write(&tmp, body)?;
        fs::rename(&tmp, target)?;
        Ok(())
    }

    fn find_mut(&mut self, name: &str) -> Result<&mut Workbench, WorkbenchError> {
        self.config
            .workbenches
            .iter_mut()
            .find(|w| w.name == name)
            .ok_or_else(|| WorkbenchError::UnknownWorkbench(name.to_string()))
    }

    pub fn create_workbench(&mut self, name: &str) -> Result<(), WorkbenchError> {
        if self.config.workbenches.iter().any(|w| w.name == name) {
            return Err(WorkbenchError::DuplicateWorkbench(name.to_string()));
        }
        self.config.workbenches.push(Workbench {
            name: name.to_string(),
            repos: Vec::new(),
        });
        self.persist()
    }

    pub fn rename_workbench(&mut self, from: &str, to: &str) -> Result<(), WorkbenchError> {
        if self.config.workbenches.iter().any(|w| w.name == to) {
            return Err(WorkbenchError::DuplicateWorkbench(to.to_string()));
        }
        self.find_mut(from)?.name = to.to_string();
        if self.config.active.as_deref() == Some(from) {
            self.config.active = Some(to.to_string());
        }
        self.persist()
    }

    pub fn delete_workbench(&mut self, name: &str) -> Result<(), WorkbenchError> {
        let before = self.config.workbenches.len();
        self.config.workbenches.retain(|w| w.name != name);
        if self.config.workbenches.len() == before {
            return Err(WorkbenchError::UnknownWorkbench(name.to_string()));
        }
        if self.config.active.as_deref() == Some(name) {
            self.config.active = None;
        }
        self.persist()
    }

    /// Agrega un repo validando que abra como repo git (R9) salvo
    /// `validate=false` (tests de CRUD puro).
    pub fn add_repo(
        &mut self,
        workbench: &str,
        path: PathBuf,
        alias: Option<String>,
        validate: bool,
    ) -> Result<PathBuf, WorkbenchError> {
        // Store the canonical path so it matches the canonical paths the bus
        // reports in snapshot/deltas (the frontend joins aliases by that key).
        // Falls back to the raw path when it cannot be canonicalized (e.g. it
        // does not exist), mirroring the bus's `canonicalize().unwrap_or(path)`.
        // The canonical path is returned so the caller can open its tab — it is
        // exactly the key the bus will report the repo under.
        let path = path.canonicalize().unwrap_or(path);
        if validate {
            Git2Engine::open(&path)?;
        }
        let wb = self.find_mut(workbench)?;
        if wb.repos.iter().any(|r| r.path == path) {
            return Err(WorkbenchError::DuplicateRepo(path));
        }
        wb.repos.push(RepoEntry {
            path: path.clone(),
            alias,
            fs_watch: Vec::new(),
        });
        self.persist()?;
        Ok(path)
    }

    pub fn remove_repo(&mut self, workbench: &str, path: &Path) -> Result<(), WorkbenchError> {
        // Stored paths are canonical (see `add_repo`). Match BOTH the canonical
        // form (for a live repo) AND the raw query (so a repo whose directory
        // was deleted — canonicalize now fails — can still be removed by its
        // stored canonical path).
        let canon = path.canonicalize().ok();
        let wb = self.find_mut(workbench)?;
        let before = wb.repos.len();
        wb.repos
            .retain(|r| Some(r.path.as_path()) != canon.as_deref() && r.path != path);
        if wb.repos.len() == before {
            return Err(WorkbenchError::UnknownRepo(path.to_path_buf()));
        }
        self.persist()
    }

    /// Edita alias y/o watchlist de un repo (R6).
    pub fn update_repo(
        &mut self,
        workbench: &str,
        path: &Path,
        alias: Option<Option<String>>,
        fs_watch: Option<Vec<String>>,
    ) -> Result<(), WorkbenchError> {
        let wb = self.find_mut(workbench)?;
        let repo = wb
            .repos
            .iter_mut()
            .find(|r| r.path == path)
            .ok_or_else(|| WorkbenchError::UnknownRepo(path.to_path_buf()))?;
        if let Some(alias) = alias {
            repo.alias = alias;
        }
        if let Some(fs_watch) = fs_watch {
            repo.fs_watch = fs_watch;
        }
        self.persist()
    }

    /// Reordena los repos de un workbench según la lista de paths dada (R6).
    pub fn reorder_repos(
        &mut self,
        workbench: &str,
        order: &[PathBuf],
    ) -> Result<(), WorkbenchError> {
        let wb = self.find_mut(workbench)?;
        wb.repos.sort_by_key(|r| {
            order
                .iter()
                .position(|p| p == &r.path)
                .unwrap_or(usize::MAX)
        });
        self.persist()
    }

    /// Conmuta el workbench activo y lo devuelve completo (R7).
    pub fn set_active(&mut self, name: &str) -> Result<Workbench, WorkbenchError> {
        let wb = self
            .config
            .workbenches
            .iter()
            .find(|w| w.name == name)
            .cloned()
            .ok_or_else(|| WorkbenchError::UnknownWorkbench(name.to_string()))?;
        self.config.active = Some(name.to_string());
        self.persist()?;
        Ok(wb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> WorkbenchStore {
        WorkbenchStore::open(dir.path()).expect("store")
    }

    // Covers AE1 (round-trip con doble escritura: rename reemplaza destino)
    #[test]
    fn config_persiste_y_se_recarga_identica() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("Trabajo").unwrap();
        store
            .add_repo(
                "Trabajo",
                dir.path().join("proyecto-x"),
                Some("Proyecto X".into()),
                false,
            )
            .unwrap();
        store
            .update_repo(
                "Trabajo",
                &dir.path().join("proyecto-x"),
                None,
                Some(vec![".env".into(), "dist/**".into()]),
            )
            .unwrap();
        store.set_active("Trabajo").unwrap();

        let reloaded = store_in(&dir);
        assert_eq!(reloaded.config(), store.config());
        assert_eq!(
            reloaded.active_workbench().unwrap().repos[0].fs_watch,
            vec![".env", "dist/**"]
        );
    }

    // Covers AE2
    #[test]
    fn config_corrupta_reporta_error_sin_sobrescribir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(CONFIG_FILE);
        std::fs::write(&file, "esto no es { toml valido").unwrap();
        let bytes_antes = std::fs::read(&file).unwrap();

        let err = WorkbenchStore::open(dir.path()).unwrap_err();
        assert!(matches!(err, WorkbenchError::CorruptConfig(_)));
        assert_eq!(std::fs::read(&file).unwrap(), bytes_antes);
    }

    #[test]
    fn archivo_ausente_arranca_vacio() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store_in(&dir);
        assert!(store.config().workbenches.is_empty());
        assert!(store.config().active.is_none());
    }

    // Covers R4
    #[test]
    fn activo_colgante_degrada_a_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join(CONFIG_FILE),
            "version = 1\nactive = \"fantasma\"\n",
        )
        .unwrap();
        let store = store_in(&dir);
        assert!(store.config().active.is_none());
    }

    #[test]
    fn nombres_duplicados_y_desconocidos_fallan_tipado() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        assert!(matches!(
            store.create_workbench("A").unwrap_err(),
            WorkbenchError::DuplicateWorkbench(_)
        ));
        assert!(matches!(
            store.rename_workbench("Z", "B").unwrap_err(),
            WorkbenchError::UnknownWorkbench(_)
        ));
        store.create_workbench("B").unwrap();
        assert!(matches!(
            store.rename_workbench("A", "B").unwrap_err(),
            WorkbenchError::DuplicateWorkbench(_)
        ));
    }

    #[test]
    fn eliminar_workbench_activo_limpia_active() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        store.set_active("A").unwrap();
        store.delete_workbench("A").unwrap();
        assert!(store.config().active.is_none());
    }

    // Covers AE4 (mitad duplicado; la validación git real vive en tests de
    // integración con TempRepo del módulo git)
    #[test]
    fn repo_duplicado_se_rechaza() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let repo = dir.path().join("repo");
        store.add_repo("A", repo.clone(), None, false).unwrap();
        assert!(matches!(
            store.add_repo("A", repo, None, false).unwrap_err(),
            WorkbenchError::DuplicateRepo(_)
        ));
    }

    // Covers AE4 (mitad validación git)
    #[test]
    fn agregar_path_no_git_falla_con_error_de_la_capa_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let no_repo = dir.path().join("no-repo");
        std::fs::create_dir_all(&no_repo).unwrap();
        let err = store.add_repo("A", no_repo, None, true).unwrap_err();
        assert!(matches!(err, WorkbenchError::InvalidRepo(_)));
    }

    #[test]
    fn add_repo_almacena_path_canonico_y_remove_lo_acepta() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        // Real subdir referenced through a non-canonical path (extra `/.`).
        let real = dir.path().join("repo");
        std::fs::create_dir_all(&real).unwrap();
        let noncanon = real.join(".");
        store.add_repo("A", noncanon.clone(), None, false).unwrap();

        let canonical = real.canonicalize().unwrap();
        assert_eq!(
            store.config().workbenches[0].repos[0].path,
            canonical,
            "stored path must be canonical so it matches bus/snapshot paths"
        );
        // Removing by the non-canonical form still resolves to the stored repo.
        store.remove_repo("A", &noncanon).unwrap();
        assert!(store.config().workbenches[0].repos.is_empty());
    }

    #[test]
    fn add_repo_canonicaliza_dot_dot_y_remove_lo_acepta() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let real = dir.path().join("a").join("b");
        std::fs::create_dir_all(&real).unwrap();
        // A `..` round-trip path that resolves to `real`.
        let via_dotdot = real.join("..").join("b");
        store
            .add_repo("A", via_dotdot.clone(), None, false)
            .unwrap();
        assert_eq!(
            store.config().workbenches[0].repos[0].path,
            real.canonicalize().unwrap(),
        );
        store.remove_repo("A", &via_dotdot).unwrap();
        assert!(store.config().workbenches[0].repos.is_empty());
    }

    #[test]
    fn remove_repo_acepta_entrada_cuyo_directorio_fue_borrado() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let real = dir.path().join("dead");
        std::fs::create_dir_all(&real).unwrap();
        store.add_repo("A", real.clone(), None, false).unwrap();
        let stored = store.config().workbenches[0].repos[0].path.clone(); // canonical
        std::fs::remove_dir_all(&real).unwrap(); // dir gone → canonicalize fails
                                                 // Removing by the stored canonical path still works (raw-path match).
        store.remove_repo("A", &stored).unwrap();
        assert!(store.config().workbenches[0].repos.is_empty());
    }

    #[test]
    fn reorden_persiste() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let (p1, p2) = (dir.path().join("uno"), dir.path().join("dos"));
        store.add_repo("A", p1.clone(), None, false).unwrap();
        store.add_repo("A", p2.clone(), None, false).unwrap();
        store.reorder_repos("A", &[p2.clone(), p1.clone()]).unwrap();

        let reloaded = store_in(&dir);
        let repos: Vec<_> = reloaded.config().workbenches[0]
            .repos
            .iter()
            .map(|r| r.path.clone())
            .collect();
        assert_eq!(repos, vec![p2, p1]);
    }

    #[test]
    fn primer_persist_degradado_respalda_el_archivo_corrupto() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join(CONFIG_FILE);
        std::fs::write(&file, "toml corrupto {{{").unwrap();
        let bytes_corruptos = std::fs::read(&file).unwrap();

        let mut store = WorkbenchStore::with_default_config(dir.path());
        store.create_workbench("Nuevo").unwrap();

        let backup = dir.path().join(format!("{CONFIG_FILE}.corrupt"));
        assert_eq!(std::fs::read(&backup).unwrap(), bytes_corruptos);
        // Y el archivo vivo ahora es la config nueva válida.
        assert!(WorkbenchStore::open(dir.path()).is_ok());
    }

    #[test]
    fn reorden_parcial_deja_el_resto_al_final_en_orden_estable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        let (p1, p2, p3) = (
            dir.path().join("uno"),
            dir.path().join("dos"),
            dir.path().join("tres"),
        );
        for p in [&p1, &p2, &p3] {
            store.add_repo("A", p.clone(), None, false).unwrap();
        }
        store.reorder_repos("A", std::slice::from_ref(&p3)).unwrap();
        let repos: Vec<_> = store.config().workbenches[0]
            .repos
            .iter()
            .map(|r| r.path.clone())
            .collect();
        assert_eq!(repos, vec![p3, p1, p2]);
    }

    #[test]
    fn set_active_devuelve_workbench_completo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = store_in(&dir);
        store.create_workbench("A").unwrap();
        store
            .add_repo("A", dir.path().join("r"), None, false)
            .unwrap();
        let wb = store.set_active("A").unwrap();
        assert_eq!(wb.name, "A");
        assert_eq!(wb.repos.len(), 1);
    }
}
