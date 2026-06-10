//! Clasificador de paths del watcher: decide a qué plano pertenece cada
//! evento de FS (árbol de clasificación del diseño, §3).
//!
//! El clasificador es puro: se construye una vez por repo leyendo los
//! `.gitignore` del árbol y compilando la watchlist; `classify` no hace
//! ningún I/O (el flag `is_dir` lo aporta el caller, que lo conoce del
//! evento). Ante cambios en los `.gitignore`, el consumidor reconstruye el
//! clasificador.
//!
//! Limitación v1: archivos tracked-pero-gitignoreados (añadidos con
//! `git add -f`) se clasifican por las reglas de ignore, no por el índice.
//! El recálculo de status (git engine) sigue siendo la fuente de verdad del
//! Plano 1.

use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use thiserror::Error;

/// Bucket de clasificación de un path (diseño §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    /// Interno de `.git/`: descartar.
    GitInternal,
    /// `.git/HEAD` o `.git/index`: señal de commit/cambio de branch.
    GitMeta,
    /// Trackeable por git (no ignorado): Plano 1, recalcular status/diff.
    Plane1,
    /// Gitignoreado pero presente en la watchlist `fs_watch`: Plano 2.
    Plane2,
    /// Gitignoreado y no vigilado: descartar.
    Ignored,
    /// Fuera del root del repo: descartar (evento mal enrutado).
    OutsideRepo,
}

/// Error de construcción del clasificador.
#[derive(Debug, Error)]
pub enum ClassifierError {
    #[error("el root del repo no existe: {0}")]
    RootNotFound(PathBuf),
    #[error("patrón de watchlist inválido `{pattern}`: {source}")]
    InvalidWatchPattern {
        pattern: String,
        source: globset::Error,
    },
}

/// Clasificador de paths de un repo: gitignore compilado + watchlist.
///
/// Como en git, cada `.gitignore` escopa a su directorio: se compila un
/// matcher por directorio con `.gitignore` y, al clasificar, decide el
/// match del matcher más profundo que aplique al path.
#[derive(Debug)]
pub struct PathClassifier {
    root: PathBuf,
    /// (dir relativo al root, matcher) ordenados por profundidad creciente.
    gitignores: Vec<(PathBuf, Gitignore)>,
    watchlist: GlobSet,
}

impl PathClassifier {
    /// Construye el clasificador leyendo los `.gitignore` del árbol (una
    /// vez) y compilando los patrones de watchlist. Los `.gitignore`
    /// ilegibles se omiten con tolerancia, igual que git; un patrón de
    /// watchlist inválido sí es error (nombra el patrón culpable).
    pub fn new(repo_root: impl AsRef<Path>, watchlist: &[String]) -> Result<Self, ClassifierError> {
        let root = repo_root.as_ref();
        if !root.is_dir() {
            return Err(ClassifierError::RootNotFound(root.to_path_buf()));
        }

        // Un matcher por directorio con .gitignore (GitignoreBuilder escopa
        // sus globs al root del builder, no al archivo añadido): semántica
        // git de anidamiento real. Errores de parseo se toleran como git.
        // El walk poda directorios ya ignorados por los matchers acumulados:
        // igual que git, un .gitignore dentro de un dir ignorado no se lee
        // (sus negaciones no pueden "des-ignorar" contenido), y de paso no
        // se recorre node_modules/target al construir.
        let gitignores = collect_gitignore_matchers(root);

        let mut glob_builder = GlobSetBuilder::new();
        for pattern in watchlist {
            let glob =
                Glob::new(pattern).map_err(|source| ClassifierError::InvalidWatchPattern {
                    pattern: pattern.clone(),
                    source,
                })?;
            glob_builder.add(glob);
        }
        let watchlist =
            glob_builder
                .build()
                .map_err(|source| ClassifierError::InvalidWatchPattern {
                    pattern: "<watchlist>".into(),
                    source,
                })?;

        Ok(Self {
            root: root.to_path_buf(),
            gitignores,
            watchlist,
        })
    }

    /// Clasifica un path (absoluto dentro del repo, o relativo a su raíz).
    /// `is_dir` lo aporta el caller; usar `false` para paths borrados.
    /// No hace I/O.
    pub fn classify(&self, path: &Path, is_dir: bool) -> Classification {
        let rel = match self.relativize(path) {
            Some(rel) => rel,
            None => return Classification::OutsideRepo,
        };
        // Normalización única de separadores (notify entrega backslashes en
        // Windows); matchers y watchlist ven la misma forma con `/`.
        let rel = PathBuf::from(rel.to_string_lossy().replace('\\', "/"));

        // ¿Dentro de .git/?
        let mut components = rel.components();
        if components.next() == Some(Component::Normal(".git".as_ref())) {
            let rest: PathBuf = components.collect();
            return if rest == Path::new("HEAD") || rest == Path::new("index") {
                Classification::GitMeta
            } else {
                Classification::GitInternal
            };
        }

        // ¿Gitignoreado? El match más profundo gana (semántica git).
        if !is_ignored_by(&self.gitignores, &rel, is_dir) {
            return Classification::Plane1;
        }

        // Gitignoreado: ¿vigilado por la watchlist? (rel ya normalizado a /)
        if self.watchlist.is_match(&rel) {
            Classification::Plane2
        } else {
            Classification::Ignored
        }
    }

    fn relativize(&self, path: &Path) -> Option<PathBuf> {
        if path.is_absolute() {
            path.strip_prefix(&self.root).ok().map(Path::to_path_buf)
        } else {
            // Relativo: se asume relativo al root; rechazamos escapes con ..
            if path.components().any(|c| c == Component::ParentDir) {
                None
            } else {
                Some(path.to_path_buf())
            }
        }
    }
}

/// Recorre el árbol en BFS construyendo un matcher por directorio con
/// `.gitignore`, podando los directorios que ya quedan ignorados por los
/// matchers acumulados (no se desciende a `node_modules`/`target` ni se
/// leen sus `.gitignore`, igual que git).
fn collect_gitignore_matchers(root: &Path) -> Vec<(PathBuf, Gitignore)> {
    let mut matchers: Vec<(PathBuf, Gitignore)> = Vec::new();
    // BFS por niveles: garantiza que los matchers de ancestros existen antes
    // de evaluar/descender a los hijos.
    let mut frontier = vec![root.to_path_buf()];
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for dir in frontier {
            // Matcher del propio dir, si tiene .gitignore.
            let gitignore_file = dir.join(".gitignore");
            if gitignore_file.is_file() {
                if let Ok(rel_dir) = dir.strip_prefix(root) {
                    let mut builder = GitignoreBuilder::new(&dir);
                    let _ = builder.add(&gitignore_file);
                    if let Ok(matcher) = builder.build() {
                        matchers.push((rel_dir.to_path_buf(), matcher));
                    }
                }
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                // Poda: si los matchers acumulados ignoran este dir, ni se
                // desciende ni se leerán sus .gitignore.
                let Ok(rel) = path.strip_prefix(root) else {
                    continue;
                };
                if is_ignored_by(&matchers, rel, true) {
                    continue;
                }
                next.push(path);
            }
        }
        frontier = next;
    }
    matchers
}

/// Evalúa los matchers (ordenados raíz→profundo) sobre un path relativo;
/// el match más profundo gana, incluidas negaciones `!`.
fn is_ignored_by(matchers: &[(PathBuf, Gitignore)], rel: &Path, is_dir: bool) -> bool {
    let mut ignored = false;
    for (dir, matcher) in matchers {
        let scoped = if dir.as_os_str().is_empty() {
            rel
        } else {
            match rel.strip_prefix(dir) {
                Ok(scoped) => scoped,
                Err(_) => continue,
            }
        };
        let m = matcher.matched_path_or_any_parents(scoped, is_dir);
        if m.is_ignore() {
            ignored = true;
        } else if m.is_whitelist() {
            ignored = false;
        }
    }
    ignored
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn repo_with(gitignores: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git")).expect("fake .git");
        for (rel, content) in gitignores {
            let path = dir.path().join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("mkdir");
            }
            fs::write(path, content).expect("write");
        }
        dir
    }

    fn classifier(dir: &tempfile::TempDir, watchlist: &[&str]) -> PathClassifier {
        let watchlist: Vec<String> = watchlist.iter().map(|s| s.to_string()).collect();
        PathClassifier::new(dir.path(), &watchlist).expect("classifier")
    }

    // Covers AE1
    #[test]
    fn paths_de_git_interno_y_metadata() {
        let dir = repo_with(&[(".gitignore", "target/\n")]);
        let c = classifier(&dir, &[]);

        assert_eq!(
            c.classify(Path::new(".git/objects/ab/cdef"), false),
            Classification::GitInternal
        );
        assert_eq!(
            c.classify(Path::new(".git/HEAD"), false),
            Classification::GitMeta
        );
        assert_eq!(
            c.classify(Path::new(".git/index"), false),
            Classification::GitMeta
        );
        assert_eq!(
            c.classify(Path::new(".git"), true),
            Classification::GitInternal
        );
    }

    // Covers AE2
    #[test]
    fn path_no_ignorado_es_plano1() {
        let dir = repo_with(&[(".gitignore", "target/\n")]);
        let c = classifier(&dir, &[]);
        assert_eq!(
            c.classify(Path::new("src/main.rs"), false),
            Classification::Plane1
        );
    }

    // Covers AE3
    #[test]
    fn ignorado_con_watchlist_es_plano2_y_sin_watchlist_descarta() {
        let dir = repo_with(&[(".gitignore", "*.log\n")]);

        let con = classifier(&dir, &["*.log"]);
        assert_eq!(c0(&con, "app.log"), Classification::Plane2);

        let sin = classifier(&dir, &[]);
        assert_eq!(c0(&sin, "app.log"), Classification::Ignored);
    }

    fn c0(c: &PathClassifier, p: &str) -> Classification {
        c.classify(Path::new(p), false)
    }

    // Covers AE4
    #[test]
    fn gitignores_anidados_y_patrones_de_directorio() {
        let dir = repo_with(&[
            (".gitignore", "target/\n"),
            ("sub/.gitignore", "local.txt\n"),
        ]);
        let c = classifier(&dir, &[]);

        assert_eq!(c0(&c, "target/debug/x"), Classification::Ignored);
        assert_eq!(c0(&c, "sub/local.txt"), Classification::Ignored);
        assert_eq!(c0(&c, "sub/otro.txt"), Classification::Plane1);
        // El scope del .gitignore anidado no se filtra a la raíz:
        assert_eq!(c0(&c, "local.txt"), Classification::Plane1);
    }

    #[test]
    fn watchlist_con_glob_de_directorio_cubre_borrados() {
        let dir = repo_with(&[(".gitignore", "dist/\n")]);
        let c = classifier(&dir, &["dist/**"]);
        // El archivo no existe (borrado): is_dir=false del caller.
        assert_eq!(c0(&c, "dist/bundle.js"), Classification::Plane2);
    }

    #[test]
    fn path_absoluto_y_relativo_clasifican_igual() {
        let dir = repo_with(&[(".gitignore", "*.log\n")]);
        let c = classifier(&dir, &["*.log"]);
        let abs = dir.path().join("app.log");
        assert_eq!(c.classify(&abs, false), c0(&c, "app.log"));
    }

    #[test]
    fn path_fuera_del_repo_es_outside_repo() {
        let dir = repo_with(&[]);
        let c = classifier(&dir, &[]);
        let outside = tempfile::tempdir().expect("otro tempdir");
        assert_eq!(
            c.classify(&outside.path().join("x.txt"), false),
            Classification::OutsideRepo
        );
        assert_eq!(
            c.classify(Path::new("../escape.txt"), false),
            Classification::OutsideRepo
        );
    }

    #[test]
    fn patron_de_watchlist_invalido_nombra_el_culpable() {
        let dir = repo_with(&[]);
        let err = PathClassifier::new(dir.path(), &["[invalido".to_string()]).unwrap_err();
        match err {
            ClassifierError::InvalidWatchPattern { pattern, .. } => {
                assert_eq!(pattern, "[invalido")
            }
            other => panic!("esperaba InvalidWatchPattern, fue {other:?}"),
        }
    }

    #[test]
    fn root_inexistente_falla_tipado() {
        let err = PathClassifier::new("Z:/no/existe", &[]).unwrap_err();
        assert!(matches!(err, ClassifierError::RootNotFound(_)));
    }

    #[test]
    fn negacion_dentro_de_dir_ignorado_no_des_ignora() {
        // git no lee .gitignore dentro de directorios ignorados: la negación
        // de build/.gitignore no puede rescatar a build/keep.txt.
        let dir = repo_with(&[
            (".gitignore", "build/\n"),
            ("build/.gitignore", "!keep.txt\n"),
        ]);
        let c = classifier(&dir, &[]);
        assert_eq!(c0(&c, "build/keep.txt"), Classification::Ignored);
        assert_eq!(c0(&c, "build/otro.bin"), Classification::Ignored);
    }

    #[test]
    fn negacion_en_dir_no_ignorado_si_aplica() {
        // Caso válido de negación: el dir NO está ignorado, el patrón del
        // root ignora *.log y el .gitignore anidado rescata keep.log.
        let dir = repo_with(&[(".gitignore", "*.log\n"), ("sub/.gitignore", "!keep.log\n")]);
        let c = classifier(&dir, &[]);
        assert_eq!(c0(&c, "sub/keep.log"), Classification::Plane1);
        assert_eq!(c0(&c, "sub/otro.log"), Classification::Ignored);
        assert_eq!(c0(&c, "raiz.log"), Classification::Ignored);
    }

    #[test]
    fn borrado_bajo_patron_de_directorio_sigue_ignorado() {
        // is_dir=false del leaf borrado: los ancestros se evalúan como dirs
        // (el crate lo hace internamente), así target/ matchea igual.
        let dir = repo_with(&[(".gitignore", "target/\n")]);
        let c = classifier(&dir, &[]);
        assert_eq!(c0(&c, "target/debug/lib.rs"), Classification::Ignored);
    }
}
