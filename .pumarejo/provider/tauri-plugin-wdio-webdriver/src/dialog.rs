//! A deliberately small, opt-in dialog broker for test fixtures.
//!
//! This is not an interception layer for `tauri-plugin-dialog`, `rfd`, or
//! operating-system dialogs. An application must explicitly invoke the
//! `request_dialog` command. The provider can then observe and resolve that
//! one application-owned request through the authenticated WebDriver channel.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_DIALOG_TEXT_LENGTH: usize = 4_096;
pub const MAX_DIALOG_BUTTONS: usize = 8;
pub const MAX_DIALOG_BUTTON_LENGTH: usize = 128;

/// The only decisions the provider may send to the application broker.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DialogAction {
    Accept,
    Cancel,
}

impl DialogAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Cancel => "cancel",
        }
    }
}

/// Application input for the opt-in broker.
///
/// Unknown fields are rejected by serde so this command cannot become a
/// generic command/script/selector/path transport by accident.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DialogRequest {
    pub title: String,
    pub message: String,
    pub buttons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DialogMetadata {
    /// Private provider identity. It is never returned by the application
    /// command and is only used on the authenticated provider route.
    pub instance_id: String,
    pub title: String,
    pub message: String,
    pub buttons: Vec<String>,
    pub surface_ref: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DialogOutcome {
    pub action: DialogAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogBrokerError {
    InvalidWindow,
    InvalidRequest,
    Pending,
    NotFound,
    InstanceMismatch,
    DecisionUnavailable,
    StateUnavailable,
}

impl DialogBrokerError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidWindow => "dialog window is invalid",
            Self::InvalidRequest => "dialog request is invalid or exceeds bounds",
            Self::Pending => "a dialog is already pending for this window",
            Self::NotFound => "no dialog is pending for this window",
            Self::InstanceMismatch => "dialog instance does not match the pending request",
            Self::DecisionUnavailable => "dialog decision could not be delivered",
            Self::StateUnavailable => "dialog broker state is unavailable",
        }
    }
}

struct PendingDialog {
    metadata: DialogMetadata,
    sender: tokio::sync::oneshot::Sender<DialogAction>,
}

/// Shared state between the Tauri command and the authenticated provider.
///
/// The map is intentionally keyed by the current window label. There can be
/// at most one pending request per window, and a request is removed before a
/// decision is delivered so a replay cannot observe or resolve it again.
#[derive(Clone, Default)]
pub struct DialogBroker {
    pending: Arc<Mutex<HashMap<String, PendingDialog>>>,
}

impl DialogBroker {
    fn valid_window(window: &str) -> bool {
        !window.is_empty() && window.len() <= MAX_DIALOG_BUTTON_LENGTH
    }

    fn valid_text(value: &str, max: usize) -> bool {
        value.chars().count() <= max
            && !value
                .chars()
                .any(|character| matches!(character, '\u{0000}'..='\u{001f}' | '\u{007f}'))
    }

    /// Validate and normalize only the bounded fields owned by this broker.
    pub fn validate_request(request: &DialogRequest) -> Result<(), DialogBrokerError> {
        if !Self::valid_text(&request.title, MAX_DIALOG_TEXT_LENGTH)
            || !Self::valid_text(&request.message, MAX_DIALOG_TEXT_LENGTH)
            || request.buttons.is_empty()
            || request.buttons.len() > MAX_DIALOG_BUTTONS
            || request.buttons.iter().any(|button| {
                button.is_empty() || !Self::valid_text(button, MAX_DIALOG_BUTTON_LENGTH)
            })
        {
            return Err(DialogBrokerError::InvalidRequest);
        }
        Ok(())
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, HashMap<String, PendingDialog>>, DialogBrokerError> {
        self.pending
            .lock()
            .map_err(|_| DialogBrokerError::StateUnavailable)
    }

    /// Register one request and wait for the exact provider decision.
    ///
    /// There is intentionally no timeout and no default action. Dropping the
    /// application future leaves the broker without an implicit choice.
    pub async fn request(
        &self,
        window: impl Into<String>,
        request: DialogRequest,
    ) -> Result<DialogOutcome, DialogBrokerError> {
        let window = window.into();
        if !Self::valid_window(&window) {
            return Err(DialogBrokerError::InvalidWindow);
        }
        Self::validate_request(&request)?;
        let instance_id = Uuid::new_v4().to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.lock()?;
            if pending.contains_key(&window) {
                return Err(DialogBrokerError::Pending);
            }
            pending.insert(
                window.clone(),
                PendingDialog {
                    metadata: DialogMetadata {
                        instance_id,
                        title: request.title,
                        message: request.message,
                        buttons: request.buttons,
                        surface_ref: window.clone(),
                    },
                    sender,
                },
            );
        }

        let action = receiver
            .await
            .map_err(|_| DialogBrokerError::DecisionUnavailable)?;
        // The provider removes the request before sending. This cleanup also
        // handles an application-side receiver that was dropped unexpectedly.
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&window);
        }
        Ok(DialogOutcome { action })
    }

    pub fn pending(&self, window: &str) -> Result<Option<DialogMetadata>, DialogBrokerError> {
        if !Self::valid_window(window) {
            return Err(DialogBrokerError::InvalidWindow);
        }
        Ok(self.lock()?.get(window).map(|entry| entry.metadata.clone()))
    }

    /// Deliver exactly one decision to the request identified by the private
    /// provider instance UUID.
    pub fn decide(
        &self,
        window: &str,
        instance_id: &str,
        action: DialogAction,
    ) -> Result<(), DialogBrokerError> {
        if !Self::valid_window(window) || instance_id.is_empty() || instance_id.len() > 64 {
            return Err(DialogBrokerError::InstanceMismatch);
        }
        let pending = self
            .lock()?
            .remove(window)
            .ok_or(DialogBrokerError::NotFound)?;
        if pending.metadata.instance_id != instance_id {
            // Put the request back so a caller with the correct instance can
            // still resolve it; a wrong instance has no side effect.
            let mut state = self.lock()?;
            state.insert(window.to_owned(), pending);
            return Err(DialogBrokerError::InstanceMismatch);
        }
        pending
            .sender
            .send(action)
            .map_err(|_| DialogBrokerError::DecisionUnavailable)
    }

    /// Remove an unresolved request when a window/session closes. This never
    /// chooses accept or cancel; the waiting application receives closure.
    pub fn clear_window(&self, window: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(window);
        }
    }

    pub fn clear_all(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn request() -> DialogRequest {
        DialogRequest {
            title: "Confirm".into(),
            message: "Continue?".into(),
            buttons: vec!["accept".into(), "cancel".into()],
        }
    }

    fn wait_for_metadata(broker: &DialogBroker) -> DialogMetadata {
        for _ in 0..100 {
            if let Some(metadata) = broker.pending("main").unwrap() {
                return metadata;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("dialog request was not registered");
    }

    #[test]
    fn validates_finite_bounded_dialog_fields() {
        assert!(DialogBroker::validate_request(&request()).is_ok());
        assert_eq!(
            DialogBroker::validate_request(&DialogRequest {
                title: "x".repeat(MAX_DIALOG_TEXT_LENGTH + 1),
                ..request()
            }),
            Err(DialogBrokerError::InvalidRequest)
        );
        assert_eq!(
            DialogBroker::validate_request(&DialogRequest {
                buttons: vec!["x".into(); MAX_DIALOG_BUTTONS + 1],
                ..request()
            }),
            Err(DialogBrokerError::InvalidRequest)
        );
        assert_eq!(
            DialogBroker::validate_request(&DialogRequest {
                buttons: vec!["x".repeat(MAX_DIALOG_BUTTON_LENGTH + 1)],
                ..request()
            }),
            Err(DialogBrokerError::InvalidRequest)
        );
        assert!(serde_json::from_value::<DialogRequest>(serde_json::json!({
            "title": "Confirm",
            "message": "Continue?",
            "buttons": ["accept", "cancel"],
            "selector": "#arbitrary"
        }))
        .is_err());
        assert!(serde_json::from_str::<DialogAction>("\"approve\"").is_err());
    }

    #[test]
    fn wrong_instance_does_not_consume_pending_request() {
        let broker = DialogBroker::default();
        let owner = broker.clone();
        let waiter = std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(owner.request("main", request()))
        });
        let metadata = wait_for_metadata(&broker);
        assert_eq!(
            broker.decide("main", "wrong-instance", DialogAction::Accept),
            Err(DialogBrokerError::InstanceMismatch)
        );
        assert!(broker.pending("main").unwrap().is_some());
        broker
            .decide("main", &metadata.instance_id, DialogAction::Cancel)
            .unwrap();
        assert_eq!(waiter.join().unwrap().unwrap().action, DialogAction::Cancel);
        assert!(broker.pending("main").unwrap().is_none());
    }

    #[test]
    fn decision_is_one_shot_and_close_never_auto_chooses() {
        let broker = DialogBroker::default();
        let owner = broker.clone();
        let waiter = std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(owner.request("main", request()))
        });
        let metadata = wait_for_metadata(&broker);
        broker
            .decide("main", &metadata.instance_id, DialogAction::Accept)
            .unwrap();
        assert_eq!(
            broker.decide("main", &metadata.instance_id, DialogAction::Cancel),
            Err(DialogBrokerError::NotFound)
        );
        assert_eq!(waiter.join().unwrap().unwrap().action, DialogAction::Accept);

        let owner = broker.clone();
        let waiter = std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(owner.request("main", request()))
        });
        let _ = wait_for_metadata(&broker);
        broker.clear_window("main");
        assert!(waiter.join().unwrap().is_err());
    }
}
