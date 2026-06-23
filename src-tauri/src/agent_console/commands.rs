use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::bus::{
    contract::{
        AgentSession, AgentSessionChangeLog, AgentSessionOutput, EVENT_AGENT_SESSION_CHANGE_LOG,
        EVENT_AGENT_SESSION_OUTPUT,
    },
    BusHandle, RepoResolveError,
};

use super::{validation::resolve_agent_binary, AgentConsoleError, AgentSessionRegistry};

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
    app: AppHandle,
    bus: State<'_, BusHandle>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    repo: PathBuf,
    agent_type: String,
) -> Result<String, CommandError> {
    let repo = ensure_known_agent_repo(&bus, &repo).await?;
    let started = {
        let mut registry = lock_registry(&registry)?;
        registry.start_session_with_output(repo, agent_type)?
    };
    if let Some(output_reader) = started.output_reader {
        spawn_output_reader(app, started.id.clone(), output_reader);
    }
    Ok(started.id)
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
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
) -> Result<Vec<AgentSession>, CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry
        .refresh_session_statuses()
        .map_err(CommandError::from)?;
    let sessions = registry.list_sessions();
    emit_change_logs(&app, &sessions);
    Ok(sessions)
}

#[tauri::command]
pub fn agent_binary_available(agent_type: String) -> Result<bool, CommandError> {
    match resolve_agent_binary(&agent_type) {
        Ok(_) => Ok(true),
        Err(error) if error.category == "binary_not_found" => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[tauri::command]
pub fn write_agent_session_input(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    input_base64: String,
) -> Result<(), CommandError> {
    let input = STANDARD
        .decode(input_base64)
        .map_err(|e| CommandError::new("invalid_input", e.to_string()))?;
    let mut registry = lock_registry(&registry)?;
    registry
        .write_session_input(&session_id, &input)
        .map_err(Into::into)
}

#[tauri::command]
pub fn resize_agent_session(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry
        .resize_session(&session_id, cols, rows)
        .map_err(Into::into)
}

#[tauri::command]
pub fn revert_session(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    user_consent: bool,
) -> Result<AgentSession, CommandError> {
    let mut registry = lock_registry(&registry)?;
    let session = registry
        .revert_session(&session_id, user_consent)
        .map_err(CommandError::from)?;
    emit_change_logs(&app, std::slice::from_ref(&session));
    Ok(session)
}

async fn ensure_known_agent_repo(bus: &BusHandle, repo: &Path) -> Result<PathBuf, CommandError> {
    bus.resolve_repo(repo.to_path_buf())
        .await
        .map_err(map_repo_resolve_error)
}

fn map_repo_resolve_error(error: RepoResolveError) -> CommandError {
    match error {
        RepoResolveError::UnsupportedRepoSource { .. } => CommandError::new(
            "unsupported_repo_source",
            "la fuente del repo no está soportada por este backend local",
        ),
        RepoResolveError::RepositoryNotFound => {
            CommandError::new("repository_not_found", "el repo no existe")
        }
        RepoResolveError::RepoNotAllowed => CommandError::new(
            "repo_not_allowed",
            "el repo no pertenece al workbench activo",
        ),
        RepoResolveError::BusUnavailable => {
            CommandError::new("bus_unavailable", "el bus no está disponible")
        }
    }
}

fn lock_registry(
    registry: &Mutex<AgentSessionRegistry>,
) -> Result<std::sync::MutexGuard<'_, AgentSessionRegistry>, CommandError> {
    registry
        .lock()
        .map_err(|_| CommandError::new("lock_poisoned", "el registro de agentes fallo"))
}

fn spawn_output_reader(
    app: AppHandle,
    session_id: String,
    mut output_reader: Box<dyn Read + Send>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match output_reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let payload = AgentSessionOutput {
                        session_id: session_id.clone(),
                        chunk_base64: STANDARD.encode(&buffer[..read]),
                        timestamp_ms: now_ms(),
                    };
                    let _ = app.emit(EVENT_AGENT_SESSION_OUTPUT, payload);
                }
                Err(_) => break,
            }
        }
    });
}

fn emit_change_logs(app: &AppHandle, sessions: &[AgentSession]) {
    for session in sessions {
        let payload = AgentSessionChangeLog {
            session_id: session.id.clone(),
            changes: session.change_log.clone(),
        };
        let _ = app.emit(EVENT_AGENT_SESSION_CHANGE_LOG, payload);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

    #[test]
    fn invalid_input_base64_maps_to_command_error() {
        let error = STANDARD
            .decode("not base64!")
            .map_err(|e| CommandError::new("invalid_input", e.to_string()))
            .unwrap_err();

        assert_eq!(error.category, "invalid_input");
    }

    #[test]
    fn unsupported_repo_resolve_error_maps_to_safe_category() {
        let error = map_repo_resolve_error(RepoResolveError::UnsupportedRepoSource {
            source: crate::workbench::RepoSource::Wsl,
        });

        assert_eq!(error.category, "unsupported_repo_source");
        assert!(!error.message.contains("/home/me/proyecto"));
    }

    #[test]
    fn agent_binary_available_rejects_unsupported_agent() {
        let error = agent_binary_available("powershell".into()).unwrap_err();

        assert_eq!(error.category, "unsupported_agent");
    }
}
