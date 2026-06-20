use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use tauri::State;

use crate::bus::{contract::AgentSession, BusHandle};

use super::{AgentConsoleError, AgentSessionRegistry};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub category: String,
    pub message: String,
}

impl CommandError {
    fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }
}

impl From<AgentConsoleError> for CommandError {
    fn from(error: AgentConsoleError) -> Self {
        Self::new(error.category, error.message)
    }
}

#[tauri::command]
pub async fn start_agent_session(
    bus: State<'_, BusHandle>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    repo: PathBuf,
    agent_type: String,
) -> Result<String, CommandError> {
    let repo = ensure_known_agent_repo(&bus, &repo).await?;
    let mut registry = lock_registry(&registry)?;
    registry.start_session(repo, agent_type).map_err(Into::into)
}

#[tauri::command]
pub fn stop_agent_session(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
) -> Result<(), CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry.stop_session(&session_id).map_err(Into::into)
}

#[tauri::command]
pub fn list_agent_sessions(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
) -> Result<Vec<AgentSession>, CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry
        .refresh_session_statuses()
        .map_err(CommandError::from)?;
    Ok(registry.list_sessions())
}

async fn ensure_known_agent_repo(bus: &BusHandle, repo: &Path) -> Result<PathBuf, CommandError> {
    let canon = repo
        .canonicalize()
        .map_err(|_| CommandError::new("repository_not_found", "el repo no existe"))?;
    if bus.is_known(canon.clone()).await {
        Ok(canon)
    } else {
        Err(CommandError::new(
            "repo_not_allowed",
            "el repo no pertenece al workbench activo",
        ))
    }
}

fn lock_registry(
    registry: &Mutex<AgentSessionRegistry>,
) -> Result<std::sync::MutexGuard<'_, AgentSessionRegistry>, CommandError> {
    registry
        .lock()
        .map_err(|_| CommandError::new("lock_poisoned", "el registro de agentes fallo"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_agent_console_error_without_losing_category() {
        let error = CommandError::from(AgentConsoleError::new("unsupported_agent", "nope"));

        assert_eq!(error.category, "unsupported_agent");
        assert_eq!(error.message, "nope");
    }

    #[test]
    fn lock_registry_reports_poisoning() {
        let registry = Mutex::new(AgentSessionRegistry::new());
        let _ = std::panic::catch_unwind(|| {
            let _guard = registry.lock().unwrap();
            panic!("poison");
        });

        let error = match lock_registry(&registry) {
            Ok(_) => panic!("registry lock should be poisoned"),
            Err(error) => error,
        };

        assert_eq!(error.category, "lock_poisoned");
    }
}
