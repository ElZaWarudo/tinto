use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Manager, Runtime};
use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::RwLock;

pub mod handlers;
pub mod response;
pub mod router;

use crate::dialog::DialogBroker;
use crate::platform::{create_executor, FrameId, PlatformExecutor};
use crate::server::response::WebDriverErrorResponse;
use crate::webdriver::{SessionManager, Timeouts};

const PROVIDER_NONCE_ENV: &str = "TAURI_WEBDRIVER_NONCE";
const PROVIDER_NONCE_HEADER: &str = "x-pumarejo-provider-nonce";

/// Shared state for the `WebDriver` server
pub struct AppState<R: Runtime> {
    pub app: AppHandle<R>,
    pub sessions: RwLock<SessionManager>,
    pub dialog_broker: DialogBroker,
}

impl<R: Runtime + 'static> AppState<R> {
    pub fn new(app: AppHandle<R>, dialog_broker: DialogBroker) -> Self {
        Self {
            app,
            sessions: RwLock::new(SessionManager::new()),
            dialog_broker,
        }
    }

    /// Get a platform executor for a specific window by label
    pub fn get_executor_for_window(
        &self,
        window_label: &str,
        timeouts: Timeouts,
        frame_context: Vec<FrameId>,
    ) -> Result<Arc<dyn PlatformExecutor<R>>, WebDriverErrorResponse> {
        self.app
            .webview_windows()
            .get(window_label)
            .cloned()
            .map(|window| create_executor(window, timeouts, frame_context))
            .ok_or_else(WebDriverErrorResponse::no_such_window)
    }

    /// Get all window labels
    pub fn get_window_labels(&self) -> Vec<String> {
        self.app.webview_windows().keys().cloned().collect()
    }
}

/// Start the `WebDriver` HTTP server on the specified port
pub fn start<R: Runtime + 'static>(app: AppHandle<R>, port: u16, dialog_broker: DialogBroker) {
    std::thread::spawn(move || {
        let rt = match TokioRuntime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create Tokio runtime for WebDriver server: {}", e);
                return;
            }
        };

        rt.block_on(async {
            let state = Arc::new(AppState::new(app, dialog_broker));
            let provider_nonce = match std::env::var(PROVIDER_NONCE_ENV) {
                Ok(value) if value.len() >= 32 => Arc::<str>::from(value),
                _ => {
                    tracing::error!(
                        "{PROVIDER_NONCE_ENV} must contain at least 32 characters"
                    );
                    return;
                }
            };
            let auth = middleware::from_fn(move |request: Request, next: Next| {
                let provider_nonce = provider_nonce.clone();
                async move {
                    require_provider_nonce(request, next, provider_nonce.as_bytes()).await
                }
            });
            let router = router::create_router(state).layer(auth);

            // On Android, bind to all interfaces for WiFi accessibility
            // On other platforms, bind to localhost only for security
            #[cfg(target_os = "android")]
            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            #[cfg(not(target_os = "android"))]
            let addr = SocketAddr::from(([127, 0, 0, 1], port));

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(
                        "Failed to bind WebDriver server to {} — port may already be in use: {}",
                        addr, e
                    );
                    return;
                }
            };

            tracing::info!("WebDriver server listening on http://{}", addr);

            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!("WebDriver server error: {}", e);
            }
        });
    });
}

async fn require_provider_nonce(request: Request, next: Next, expected: &[u8]) -> Response {
    let accepted = request
        .headers()
        .get(PROVIDER_NONCE_HEADER)
        .map(|actual| bool::from(actual.as_bytes().ct_eq(expected)))
        .unwrap_or(false);
    if accepted {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            r#"{"value":{"error":"unauthorized"}}"#,
        )
            .into_response()
    }
}
