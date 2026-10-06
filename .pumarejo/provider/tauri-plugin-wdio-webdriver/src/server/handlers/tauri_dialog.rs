use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tauri::Runtime;

use crate::dialog::{DialogAction, DialogBrokerError};
use crate::server::response::{WebDriverErrorResponse, WebDriverResponse, WebDriverResult};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogDecisionRequest {
    pub action: DialogAction,
    #[serde(rename = "instanceId")]
    pub instance_id: String,
}
fn denied(error: DialogBrokerError) -> WebDriverErrorResponse {
    WebDriverErrorResponse::new(
        StatusCode::FORBIDDEN,
        "dialog denied",
        error.message(),
        None,
    )
}

/// GET `/session/{session_id}/pumarejo/tauri-dialog`
///
/// This route reports only the explicitly requested application broker. It
/// does not inspect or intercept browser, Tauri-plugin-dialog, rfd, or OS
/// dialogs.
pub async fn detect<R: Runtime + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(session_id): Path<String>,
) -> WebDriverResult {
    let window = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id)?.current_window.clone()
    };
    let dialog = state.dialog_broker.pending(&window).map_err(denied)?;
    let Some(dialog) = dialog else {
        return Err(WebDriverErrorResponse::no_such_alert());
    };

    Ok(WebDriverResponse::success(json!({
        "supported": true,
        "code": "provider_dialog_supported",
        "dialog": dialog,
    })))
}

/// POST `/session/{session_id}/pumarejo/tauri-dialog/decision`
///
/// The instance UUID and action are both exact. The broker removes the
/// pending entry before delivering the oneshot decision, making replay a
/// deterministic denial.
pub async fn decide<R: Runtime + 'static>(
    State(state): State<Arc<AppState<R>>>,
    Path(session_id): Path<String>,
    Json(request): Json<DialogDecisionRequest>,
) -> WebDriverResult {
    let window = {
        let sessions = state.sessions.read().await;
        sessions.get(&session_id)?.current_window.clone()
    };
    state
        .dialog_broker
        .decide(&window, &request.instance_id, request.action)
        .map_err(denied)?;
    Ok(WebDriverResponse::success(json!({
        "resolved": true,
        "action": request.action,
    })))
}
