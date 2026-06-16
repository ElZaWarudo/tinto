//! Comandos Tauri del workbench manager: wrappers delgados sobre el store.

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;

use super::{autodetect_repos, Workbench, WorkbenchConfig, WorkbenchError, WorkbenchStore};

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
    let repos = locked(store, |s| {
        Ok(s.active_workbench()
            .filter(|w| w.name == workbench)
            .map(|w| w.repos.clone()))
    });
    if let Ok(Some(repos)) = repos {
        bus.set_workbench(repos);
    }
}

#[tauri::command]
pub fn list_workbenches(store: Store<'_>) -> Result<WorkbenchConfig, WorkbenchError> {
    locked(&store, |s| Ok(s.config().clone()))
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
