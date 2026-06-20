pub mod agent_console;
pub mod bus;
pub mod git;
pub mod paths;
pub mod ui_state;
pub mod watcher;
pub mod workbench;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PingResponse {
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TickPayload {
    pub timestamp_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
fn ping() -> PingResponse {
    PingResponse {
        message: "pong desde el backend de Tinto".into(),
        timestamp_ms: now_ms(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Config corrupta o ilegible: la app arranca con config vacía en memoria
    // sin tocar el archivo (solo se sobreescribe si el usuario muta).
    let store = workbench::WorkbenchStore::open_default().unwrap_or_else(|e| {
        eprintln!("tinto: no se pudo cargar la config de workbenches: {e}");
        let dir = dirs::config_dir()
            .map(|d| d.join("tinto"))
            .unwrap_or_else(std::env::temp_dir);
        workbench::WorkbenchStore::with_default_config(dir)
    });

    // Repos del workbench activo persistido: insumo de montaje inicial del bus.
    let initial_repos = store
        .active_workbench()
        .map(|w| w.repos.clone())
        .unwrap_or_default();

    // Canal de comandos del bus creado SÍNCRONAMENTE: los `invoke` tempranos
    // encolan sin carrera con el init async de la task.
    let (bus_handle, bus_rx) = bus::BusHandle::new_pair();
    let mut bus_rx = Some(bus_rx);
    let mut initial_repos = Some(initial_repos);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(std::sync::Mutex::new(store))
        .manage(std::sync::Mutex::new(
            agent_console::AgentSessionRegistry::new(),
        ))
        .manage(bus_handle)
        .invoke_handler(tauri::generate_handler![
            ping,
            workbench::commands::list_workbenches,
            workbench::commands::create_workbench,
            workbench::commands::rename_workbench,
            workbench::commands::delete_workbench,
            workbench::commands::add_repo,
            workbench::commands::remove_repo,
            workbench::commands::update_repo,
            workbench::commands::set_active_workbench,
            workbench::commands::autodetect_repos_under,
            bus::commands::get_workbench_snapshot,
            bus::commands::get_worktree_diff,
            bus::commands::get_commit_diff,
            bus::commands::get_commit_log,
            bus::commands::get_blob,
            bus::commands::get_file_content,
            bus::commands::get_media_content,
            bus::commands::list_repo_tree,
            bus::commands::set_subscriptions,
            bus::commands::retry_repo,
            agent_console::commands::start_agent_session,
            agent_console::commands::stop_agent_session,
            agent_console::commands::list_agent_sessions,
            agent_console::commands::write_agent_session_input,
            agent_console::commands::resize_agent_session,
            ui_state::get_ui_state,
            ui_state::set_ui_state
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                loop {
                    interval.tick().await;
                    let _ = handle.emit(
                        "tick",
                        TickPayload {
                            timestamp_ms: now_ms(),
                        },
                    );
                }
            });

            // Task del bus: emite vía AppHandle (DeltaSink inyectada).
            let sink_handle = app.handle().clone();
            let sink: bus::DeltaSink =
                std::sync::Arc::new(move |event: &str, payload: serde_json::Value| {
                    let _ = sink_handle.emit(event, payload);
                });
            let rx = bus_rx.take().expect("setup corre una sola vez");
            let initial = initial_repos.take().unwrap_or_default();
            tauri::async_runtime::spawn(bus::run_bus(rx, sink, initial));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Salida limpia: el bus apaga el watcher (flush de lotes
            // pendientes) antes de morir el proceso.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(bus) = app_handle.try_state::<bus::BusHandle>() {
                    // Apagado acotado: si el bus/watcher quedó atascado, no
                    // colgar la salida del proceso indefinidamente.
                    tauri::async_runtime::block_on(async {
                        let _ =
                            tokio::time::timeout(std::time::Duration::from_secs(3), bus.shutdown())
                                .await;
                    });
                }
                if let Some(registry) =
                    app_handle.try_state::<std::sync::Mutex<agent_console::AgentSessionRegistry>>()
                {
                    if let Ok(mut registry) = registry.lock() {
                        registry.cleanup_all();
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_devuelve_payload_estructurado() {
        let res = ping();
        assert_eq!(res.message, "pong desde el backend de Tinto");
        assert!(res.timestamp_ms > 0);
    }

    #[test]
    fn ping_serializa_con_campos_esperados() {
        let json = serde_json::to_value(ping()).expect("serializa");
        assert!(json.get("message").is_some());
        assert!(json.get("timestamp_ms").is_some());
    }
}
