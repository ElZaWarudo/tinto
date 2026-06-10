//! Autodetección de repos git bajo una carpeta raíz (diseño §2).

use std::path::{Path, PathBuf};

/// Directorios que no vale la pena escanear.
const SKIP_DIRS: &[&str] = &["node_modules", "target", "dist", "build", "vendor"];

const MAX_DEPTH: usize = 4;

/// Busca repos git bajo `root`: BFS acotado a 4 niveles que no desciende
/// dentro de repos encontrados (un repo no contiene a otro en el resultado)
/// ni a directorios pesados conocidos. Un candidato es un dir con entrada
/// `.git` (dir o archivo — worktrees).
pub fn autodetect_repos(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let root = root.as_ref();
    let mut found = Vec::new();
    let mut frontier = vec![root.to_path_buf()];
    // 0..=MAX_DEPTH expande la frontera MAX_DEPTH+1 veces: la raíz es nivel 0
    // y se alcanzan candidatos hasta 4 niveles bajo ella.
    for _depth in 0..=MAX_DEPTH {
        let mut next = Vec::new();
        for dir in frontier.drain(..) {
            if dir.join(".git").exists() {
                found.push(dir);
                continue; // no descender dentro de un repo
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if name.starts_with('.') || SKIP_DIRS.contains(&name) {
                    continue;
                }
                next.push(path);
            }
        }
        frontier = next;
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mkrepo(base: &Path, rel: &str) {
        let dir = base.join(rel);
        fs::create_dir_all(dir.join(".git")).expect("mkrepo");
    }

    // Covers AE3
    #[test]
    fn encuentra_repos_sin_descender_dentro_de_ellos() {
        let dir = tempfile::tempdir().expect("tempdir");
        mkrepo(dir.path(), "a");
        mkrepo(dir.path(), "a/vendored/b"); // anidado: no debe aparecer
        fs::create_dir_all(dir.path().join("c")).unwrap(); // no repo

        let found = autodetect_repos(dir.path());
        assert_eq!(found, vec![dir.path().join("a")]);
    }

    #[test]
    fn detecta_worktrees_con_git_como_archivo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wt = dir.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: ../real/.git/worktrees/wt\n").unwrap();

        let found = autodetect_repos(dir.path());
        assert_eq!(found, vec![wt]);
    }

    #[test]
    fn salta_directorios_pesados_y_ocultos() {
        let dir = tempfile::tempdir().expect("tempdir");
        mkrepo(dir.path(), "node_modules/escondido");
        mkrepo(dir.path(), ".oculto/repo");
        mkrepo(dir.path(), "visible");

        let found = autodetect_repos(dir.path());
        assert_eq!(found, vec![dir.path().join("visible")]);
    }

    #[test]
    fn respeta_profundidad_maxima() {
        let dir = tempfile::tempdir().expect("tempdir");
        mkrepo(dir.path(), "n1/n2/n3/dentro"); // profundidad 4: entra
        mkrepo(dir.path(), "m1/m2/m3/m4/fuera"); // profundidad 5: no

        let found = autodetect_repos(dir.path());
        assert_eq!(found, vec![dir.path().join("n1/n2/n3/dentro")]);
    }

    #[test]
    fn raiz_que_es_repo_se_devuelve_a_si_misma() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        let found = autodetect_repos(dir.path());
        assert_eq!(found, vec![dir.path().to_path_buf()]);
    }
}
