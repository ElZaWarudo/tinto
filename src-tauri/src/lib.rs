pub mod git;
pub mod paths;
pub mod watcher;
pub mod workbench;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

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

    tauri::Builder::default()
        .manage(std::sync::Mutex::new(store))
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
            workbench::commands::autodetect_repos_under
        ])
        .setup(|app| {
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
