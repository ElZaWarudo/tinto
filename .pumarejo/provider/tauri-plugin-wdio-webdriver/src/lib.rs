use tauri::{
    command,
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State, WebviewWindow,
};

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod dialog;
mod error;
mod platform;
mod server;
mod webdriver;

pub use dialog::{
    DialogAction, DialogBroker, DialogBrokerError, DialogMetadata, DialogOutcome, DialogRequest,
};
pub use error::{Error, Result};

/// Default port for the `WebDriver` HTTP server
pub const DEFAULT_PORT: u16 = 4445;

/// Environment variable name for configuring the port
pub const PORT_ENV_VAR: &str = "TAURI_WEBDRIVER_PORT";

/// Request an application-owned, test-only dialog through the bounded
/// provider broker. No timeout or implicit choice is applied.
#[command]
async fn request_dialog<R: Runtime + 'static>(
    window: WebviewWindow<R>,
    state: State<'_, DialogBroker>,
    request: DialogRequest,
) -> std::result::Result<DialogOutcome, String> {
    state
        .request(window.label().to_owned(), request)
        .await
        .map_err(|error| error.message().to_owned())
}

/// Initializes the plugin with default settings.
///
/// The port is determined in the following order:
/// 1. `TAURI_WEBDRIVER_PORT` environment variable (if set and valid)
/// 2. Default port (4445)
#[must_use]
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    let port = std::env::var(PORT_ENV_VAR)
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    init_with_port(port)
}

/// Initializes the plugin with a custom port.
///
/// This ignores the `TAURI_WEBDRIVER_PORT` environment variable.
#[must_use]
pub fn init_with_port<R: Runtime>(port: u16) -> TauriPlugin<R> {
    let dialog_broker = DialogBroker::default();
    let command_broker = dialog_broker.clone();
    let setup_broker = dialog_broker.clone();
    Builder::new("wdio-webdriver")
        .invoke_handler(tauri::generate_handler![request_dialog])
        .setup(move |app, api| {
            #[cfg(mobile)]
            let webdriver = mobile::init(app, api)?;
            #[cfg(desktop)]
            let webdriver = desktop::init(app, api);
            app.manage(webdriver);
            app.manage(command_broker.clone());

            // Manage async script state for native message handlers (Windows only)
            #[cfg(target_os = "windows")]
            app.manage(platform::AsyncScriptState::default());
            // Serialize concurrent ExecuteScript calls per webview (Windows only)
            #[cfg(target_os = "windows")]
            app.manage(platform::ScriptExecutionLocks::default());

            // Manage per-window alert state
            app.manage(platform::AlertStateManager::default());

            // Start the WebDriver HTTP server
            let app_handle = app.app_handle().clone();
            server::start(app_handle, port, setup_broker.clone());
            tracing::info!("WDIO WebDriver plugin initialized on port {port}");

            Ok(())
        })
        .on_webview_ready(|webview| {
            platform::register_webview_handlers(&webview);
        })
        .on_event(move |_app, event| {
            if let tauri::RunEvent::WindowEvent { label, event, .. } = event {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    dialog_broker.clear_window(label);
                }
            }
        })
        .build()
}
