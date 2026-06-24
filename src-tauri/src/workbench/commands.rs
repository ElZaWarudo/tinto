//! Comandos Tauri del workbench manager: wrappers delgados sobre el store.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use super::{
    autodetect_repos, RepoEntry, Workbench, WorkbenchConfig, WorkbenchError, WorkbenchStore,
};

#[cfg(target_os = "windows")]
use crate::windows_process::hide_console;

#[cfg(target_os = "windows")]
use std::process::Command;

type Store<'a> = State<'a, Mutex<WorkbenchStore>>;

fn locked<T>(
    store: &Store<'_>,
    f: impl FnOnce(&mut WorkbenchStore) -> Result<T, WorkbenchError>,
) -> Result<T, WorkbenchError> {
    // Mutex envenenado (panic previo con el lock tomado): error tipado al
    // frontend en vez de derribar el event loop con otro panic.
    let mut guard = store.lock().map_err(|_| WorkbenchError::StoreLocked)?;
    f(&mut guard)
}

type Bus<'a> = State<'a, crate::bus::BusHandle>;

/// El snapshot en vivo (`get_workbench_snapshot`) lo sirve el bus desde su
/// estado interno, que solo se actualiza vía `bus.set_workbench`. Tras mutar
/// los repos de un workbench, si ese workbench es el activo, re-sembramos el
/// bus para que el repo añadido/quitado/editado aparezca SIN reiniciar la app.
fn reseed_if_active(store: &Store<'_>, bus: &Bus<'_>, workbench: &str) {
    let repos = locked(store, |s| Ok(active_runtime_repos_for(s, workbench)));
    if let Ok(Some(repos)) = repos {
        bus.set_workbench(repos);
    }
}

pub(crate) fn active_runtime_repos_for(
    store: &WorkbenchStore,
    workbench: &str,
) -> Option<Vec<RepoEntry>> {
    store
        .active_workbench_runtime()
        .filter(|w| w.name == workbench)
        .map(|w| w.repos)
}

pub(crate) fn list_workbenches_from_store(store: &WorkbenchStore) -> WorkbenchConfig {
    store.runtime_config()
}

#[tauri::command]
pub fn list_workbenches(store: Store<'_>) -> Result<WorkbenchConfig, WorkbenchError> {
    locked(&store, |s| Ok(list_workbenches_from_store(s)))
}

#[tauri::command]
pub fn create_workbench(store: Store<'_>, name: String) -> Result<(), WorkbenchError> {
    locked(&store, |s| s.create_workbench(&name))
}

#[tauri::command]
pub fn rename_workbench(store: Store<'_>, from: String, to: String) -> Result<(), WorkbenchError> {
    locked(&store, |s| s.rename_workbench(&from, &to))
}

#[tauri::command]
pub fn delete_workbench(store: Store<'_>, name: String) -> Result<(), WorkbenchError> {
    locked(&store, |s| s.delete_workbench(&name))
}

#[tauri::command]
pub fn add_repo(
    store: Store<'_>,
    bus: Bus<'_>,
    workbench: String,
    path: PathBuf,
    alias: Option<String>,
) -> Result<String, WorkbenchError> {
    // Returns the stored canonical path so the frontend can open the new repo's
    // project tab bound to the exact key the bus reports it under.
    let canonical = locked(&store, |s| s.add_repo(&workbench, path, alias, true))?;
    reseed_if_active(&store, &bus, &workbench);
    Ok(canonical.to_string_lossy().into_owned())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn add_wsl_repo(
    store: Store<'_>,
    bus: Bus<'_>,
    workbench: String,
    distro: String,
    path: String,
    alias: Option<String>,
) -> Result<String, WorkbenchError> {
    let stored = locked(&store, |s| s.add_wsl_repo(&workbench, distro, path, alias))?;
    reseed_if_active(&store, &bus, &workbench);
    Ok(stored.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn remove_repo(
    store: Store<'_>,
    bus: Bus<'_>,
    workbench: String,
    path: PathBuf,
) -> Result<(), WorkbenchError> {
    locked(&store, |s| s.remove_repo(&workbench, &path))?;
    reseed_if_active(&store, &bus, &workbench);
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn remove_wsl_repo(
    store: Store<'_>,
    bus: Bus<'_>,
    workbench: String,
    distro: String,
    path: String,
) -> Result<(), WorkbenchError> {
    locked(&store, |s| s.remove_wsl_repo(&workbench, &distro, &path))?;
    reseed_if_active(&store, &bus, &workbench);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct WslDirectoryEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WslDirectoryListing {
    pub path: String,
    pub is_git_repo: bool,
    pub entries: Vec<WslDirectoryEntry>,
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn list_wsl_distros() -> Result<Vec<String>, WorkbenchError> {
    let mut command = Command::new("wsl.exe");
    let output = hide_console(command.args(["--list", "--quiet"]))
        .output()
        .map_err(|error| WorkbenchError::WslCommandFailed(error.to_string()))?;
    if !output.status.success() {
        return Err(WorkbenchError::WslCommandFailed(
            decode_wsl_output(&output.stderr).trim().to_string(),
        ));
    }
    let mut distros = decode_wsl_output(&output.stdout)
        .lines()
        .map(|line| line.trim().trim_end_matches('\0').to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    distros.sort();
    distros.dedup();
    Ok(distros)
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn list_wsl_directory(
    distro: String,
    path: Option<String>,
) -> Result<WslDirectoryListing, WorkbenchError> {
    let distro = super::normalize_wsl_distro_for_commands(&distro)?;
    let path = match path {
        Some(path) if !path.trim().is_empty() => {
            Some(super::normalize_wsl_linux_path_for_commands(&path)?)
        }
        _ => None,
    };
    let path_arg = path
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let script = r#"set -eu
path_arg="${1:-}"
if [ -z "$path_arg" ]; then
  root="$HOME"
else
  root="$path_arg"
fi
if [ ! -d "$root" ]; then
  printf 'not a directory: %s\n' "$root" >&2
  exit 2
fi
root="$(cd "$root" && pwd -P)"
repo=0
if [ -d "$root/.git" ]; then
  repo=1
fi
printf 'ROOT\t%s\t%s\n' "$root" "$repo"
find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\t%p\n' | sort -f
"#;
    let mut command = Command::new("wsl.exe");
    let output = hide_console(command.args([
        "-d",
        distro.as_str(),
        "--",
        "sh",
        "-lc",
        script,
        "tinto-wsl-browse",
        path_arg.as_str(),
    ]))
    .output()
    .map_err(|error| WorkbenchError::WslCommandFailed(error.to_string()))?;
    if !output.status.success() {
        return Err(WorkbenchError::WslCommandFailed(
            decode_wsl_output(&output.stderr).trim().to_string(),
        ));
    }
    parse_wsl_directory_listing(&decode_wsl_output(&output.stdout))
}

/// `alias: Some(x)` lo cambia; `clear_alias: true` lo borra (JSON no puede
/// expresar un doble-Option: `null` deserializa a ausencia, no a Some(None)).
#[tauri::command]
pub fn update_repo(
    store: Store<'_>,
    bus: Bus<'_>,
    workbench: String,
    path: PathBuf,
    alias: Option<String>,
    clear_alias: Option<bool>,
    fs_watch: Option<Vec<String>>,
) -> Result<(), WorkbenchError> {
    let alias_update = if clear_alias.unwrap_or(false) {
        Some(None)
    } else {
        alias.map(Some)
    };
    locked(&store, |s| {
        s.update_repo(&workbench, &path, alias_update, fs_watch)
    })?;
    reseed_if_active(&store, &bus, &workbench);
    Ok(())
}

#[tauri::command]
pub fn set_active_workbench(
    store: Store<'_>,
    bus: State<'_, crate::bus::BusHandle>,
    name: String,
) -> Result<Workbench, WorkbenchError> {
    let workbench = locked(&store, |s| s.set_active(&name))?;
    // Notifica al bus (envío no-bloqueante por canal); el remount y el
    // snapshot del nuevo workbench corren en la task del bus, no inline.
    bus.set_workbench(workbench.repos.clone());
    Ok(workbench)
}

#[tauri::command]
pub fn autodetect_repos_under(root: PathBuf) -> Vec<PathBuf> {
    autodetect_repos(root)
}

#[cfg(target_os = "windows")]
fn decode_wsl_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[1] == 0 {
        let words = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&words).replace('\0', "")
    } else {
        String::from_utf8_lossy(bytes).replace('\0', "")
    }
}

#[cfg(target_os = "windows")]
fn parse_wsl_directory_listing(output: &str) -> Result<WslDirectoryListing, WorkbenchError> {
    let mut lines = output.lines();
    let root = lines
        .next()
        .ok_or_else(|| WorkbenchError::WslCommandFailed("respuesta WSL vacia".into()))?;
    let mut root_parts = root.splitn(3, '\t');
    if root_parts.next() != Some("ROOT") {
        return Err(WorkbenchError::WslCommandFailed(
            "respuesta WSL sin raiz".into(),
        ));
    }
    let path = root_parts.next().unwrap_or_default().to_string();
    let is_git_repo = root_parts.next() == Some("1");
    let entries = lines
        .filter_map(|line| {
            let (name, path) = line.split_once('\t')?;
            Some(WslDirectoryEntry {
                name: name.to_string(),
                path: path.to_string(),
            })
        })
        .collect();
    Ok(WslDirectoryListing {
        path,
        is_git_repo,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::workbench::{RepoSource, CONFIG_FILE};

    fn store_with_config(raw: &str) -> (tempfile::TempDir, WorkbenchStore) {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(CONFIG_FILE), raw).unwrap();
        let store = WorkbenchStore::open(dir.path()).expect("store");
        (dir, store)
    }

    #[test]
    fn list_workbenches_from_store_proyecta_runtime_sin_borrar_persistido() {
        let (_dir, store) = store_with_config(
            r#"
version = 1
active = "A"

[[workbench]]
name = "A"

  [[workbench.repos]]
  path = "/tmp/local"

  [[workbench.repos]]
  source = "wsl"
  path = "/home/me/proyecto"
  distro = "Ubuntu"
"#,
        );

        let visible = list_workbenches_from_store(&store);

        let expected_visible = if cfg!(target_os = "windows") { 2 } else { 1 };
        assert_eq!(visible.workbenches[0].repos.len(), expected_visible);
        assert_eq!(visible.workbenches[0].repos[0].source, RepoSource::Local);
        assert_eq!(store.config().workbenches[0].repos.len(), 2);
    }

    #[test]
    fn active_runtime_repos_for_filtra_wsl_en_reseed_activo() {
        let (_dir, store) = store_with_config(
            r#"
version = 1
active = "A"

[[workbench]]
name = "A"

  [[workbench.repos]]
  path = "/tmp/local"
  fs_watch = [".env"]

  [[workbench.repos]]
  source = "wsl"
  path = "/home/me/proyecto"
  distro = "Ubuntu"

[[workbench]]
name = "B"

  [[workbench.repos]]
  path = "/tmp/other"
"#,
        );

        let repos = active_runtime_repos_for(&store, "A").expect("active repos");

        let expected_visible = if cfg!(target_os = "windows") { 2 } else { 1 };
        assert_eq!(repos.len(), expected_visible);
        assert_eq!(repos[0].path, PathBuf::from("/tmp/local"));
        assert_eq!(repos[0].fs_watch, vec![".env"]);
        if cfg!(target_os = "windows") {
            assert_eq!(repos[1].source, RepoSource::Wsl);
            assert_eq!(repos[1].distro.as_deref(), Some("Ubuntu"));
        }
        assert_eq!(active_runtime_repos_for(&store, "B"), None);
    }
}
