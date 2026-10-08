//! Delivery mode: several tasks, each in its own worktree and branch,
//! advanced by agents that run as one-shot background jobs. Tinto keeps the
//! state, the locks and the approvals; the model only does the work.
//!
//! Delivery reuses Tinto's stable layers (git, agent CLI resolution, the
//! checkpoint store, the WSL helper) and none of the interactive Agents
//! machinery: a job ends when its process exits, never on a turn marker.

pub mod adapters;
pub mod commands;
pub mod coordination;
pub mod mcp;
pub mod model;
pub mod service;
pub mod store;
pub mod tasks;
#[cfg(test)]
mod tests;
pub mod wiring;

use crate::agent_console::commands::CommandError;
use crate::agent_console::AgentConsoleError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryError {
    pub category: String,
    pub message: String,
}

impl DeliveryError {
    pub fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }

    pub fn io(error: std::io::Error) -> Self {
        Self::new("io", error.to_string())
    }

    pub fn not_found(what: &str, id: &str) -> Self {
        Self::new("not_found", format!("{what} {id} no existe"))
    }
}

impl std::fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.category, self.message)
    }
}

impl From<rusqlite::Error> for DeliveryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::new("delivery_store_failed", error.to_string())
    }
}

impl From<AgentConsoleError> for DeliveryError {
    fn from(error: AgentConsoleError) -> Self {
        Self::new(error.category, error.message)
    }
}

impl From<crate::wsl_agent::protocol::AgentError> for DeliveryError {
    fn from(error: crate::wsl_agent::protocol::AgentError) -> Self {
        Self::new(error.safe_category(), error.message)
    }
}

impl From<DeliveryError> for CommandError {
    fn from(error: DeliveryError) -> Self {
        CommandError::new(error.category, error.message)
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
