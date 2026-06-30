use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::{
    pty::TINTO_TURN_DONE_MARKER,
    validation::{resolve_agent_binary, validate_agent_type},
    AgentConsoleError, AgentSessionRegistry,
};
use crate::bus::{
    contract::{
        AgentSession, AgentSessionChangeLog, AgentSessionOutput, EVENT_AGENT_SESSIONS_CHANGED,
        EVENT_AGENT_SESSION_CHANGE_LOG, EVENT_AGENT_SESSION_OUTPUT,
    },
    BusHandle, RepoResolveError,
};
use crate::workbench::RepoSource;
use crate::wsl_agent::{
    launcher::request_wsl_agent,
    protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION},
};

const SESSION_OUTPUT_QUIET_REFRESH_MS: u64 = 2_500;
const SESSION_OUTPUT_MONITOR_TICK_MS: u64 = 500;

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
    let resolved = ensure_known_agent_repo(&bus, &repo).await?;
    let started = {
        let mut registry = lock_registry(&registry)?;
        match resolved.source {
            RepoSource::Local => registry.start_session_with_output(resolved.path, agent_type)?,
            RepoSource::Wsl => registry.start_wsl_session_with_output(
                resolved.path,
                resolved
                    .distro
                    .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?,
                agent_type,
            )?,
        }
    };
    if let Some(output_reader) = started.output_reader {
        spawn_output_reader(app.clone(), started.id.clone(), output_reader);
    }
    refresh_and_emit_sessions(&app);
    Ok(started.id)
}

#[tauri::command]
pub fn stop_agent_session(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
) -> Result<(), CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry
        .stop_session(&session_id)
        .map_err(CommandError::from)?;
    emit_sessions_snapshot(&app, &registry.list_sessions());
    Ok(())
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
    emit_sessions_snapshot(&app, &sessions);
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
pub async fn agent_binary_available_for_repo(
    bus: State<'_, BusHandle>,
    repo: PathBuf,
    agent_type: String,
) -> Result<bool, CommandError> {
    let resolved = ensure_known_agent_repo(&bus, &repo).await?;
    match resolved.source {
        RepoSource::Local => agent_binary_available(agent_type),
        RepoSource::Wsl => wsl_agent_binary_available(
            resolved
                .distro
                .as_deref()
                .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?,
            agent_type,
        ),
    }
}

#[tauri::command]
pub fn write_agent_session_input(
    app: AppHandle,
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
        .map_err(CommandError::from)?;
    emit_sessions_snapshot(&app, &registry.list_sessions());
    Ok(())
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
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(&app, &sessions);
    emit_change_logs(&app, std::slice::from_ref(&session));
    Ok(session)
}

#[tauri::command]
pub fn revert_session_turn_file(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    turn_checkpoint_id: String,
    path: PathBuf,
    user_consent: bool,
) -> Result<AgentSession, CommandError> {
    let mut registry = lock_registry(&registry)?;
    let session = registry
        .revert_turn_file(&session_id, &turn_checkpoint_id, &path, user_consent)
        .map_err(CommandError::from)?;
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(&app, &sessions);
    emit_change_logs(&app, std::slice::from_ref(&session));
    Ok(session)
}

async fn ensure_known_agent_repo(
    bus: &BusHandle,
    repo: &Path,
) -> Result<crate::bus::ResolvedRepo, CommandError> {
    bus.resolve_repo_identity(repo.to_path_buf())
        .await
        .map_err(map_repo_resolve_error)
}

fn map_repo_resolve_error(error: RepoResolveError) -> CommandError {
    match error {
        RepoResolveError::UnsupportedRepoSource { .. } => CommandError::new(
            "unsupported_repo_source",
            "la fuente del repo no está disponible en este entorno",
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

fn wsl_agent_binary_available(distro: &str, agent_type: String) -> Result<bool, CommandError> {
    validate_agent_type(&agent_type).map_err(CommandError::from)?;
    match request_wsl_agent(
        distro,
        &AgentRequest::AgentBinaryAvailable {
            protocol_version: PROTOCOL_VERSION,
            agent_type,
        },
    ) {
        Ok(AgentResponse::AgentBinaryAvailable { available }) => Ok(available),
        Ok(AgentResponse::Error { category, message }) => Err(CommandError::new(category, message)),
        Ok(_) => Err(CommandError::new(
            "malformed_response",
            "respuesta inesperada del agente WSL",
        )),
        Err(error) => Err(CommandError::new(error.safe_category(), error.message)),
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
    let last_output_at = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    spawn_output_quiet_monitor(app.clone(), last_output_at.clone(), done.clone());
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match output_reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let timestamp_ms = now_ms();
                    last_output_at.store(timestamp_ms, Ordering::Relaxed);
                    if let Some(registry) = app.try_state::<Mutex<AgentSessionRegistry>>() {
                        if let Ok(mut registry) = registry.lock() {
                            let _ = registry.record_session_output(&session_id, timestamp_ms);
                        }
                    }
                    let explicit_turn_done = emit_output_without_turn_done_marker(
                        &app,
                        &session_id,
                        &buffer[..read],
                        timestamp_ms,
                    );
                    if explicit_turn_done {
                        if let Some(registry) = app.try_state::<Mutex<AgentSessionRegistry>>() {
                            if let Ok(mut registry) = registry.lock() {
                                let _ =
                                    registry.record_session_turn_done(&session_id, timestamp_ms);
                            }
                        }
                        refresh_and_emit_sessions(&app);
                    }
                }
                Err(_) => break,
            }
        }
        done.store(true, Ordering::Relaxed);
        refresh_and_emit_sessions(&app);
    });
}

fn emit_output_without_turn_done_marker(
    app: &AppHandle,
    session_id: &str,
    output: &[u8],
    timestamp_ms: u64,
) -> bool {
    let marker = TINTO_TURN_DONE_MARKER.as_bytes();
    let mut cursor = 0;
    let mut explicit_turn_done = false;
    while let Some(offset) = find_bytes(&output[cursor..], marker) {
        let marker_start = cursor + offset;
        emit_output_chunk(app, session_id, &output[cursor..marker_start], timestamp_ms);
        cursor = marker_start + marker.len();
        explicit_turn_done = true;
    }
    emit_output_chunk(app, session_id, &output[cursor..], timestamp_ms);
    explicit_turn_done
}

fn emit_output_chunk(app: &AppHandle, session_id: &str, chunk: &[u8], timestamp_ms: u64) {
    if chunk.is_empty() {
        return;
    }
    let payload = AgentSessionOutput {
        session_id: session_id.to_string(),
        chunk_base64: STANDARD.encode(chunk),
        timestamp_ms,
    };
    let _ = app.emit(EVENT_AGENT_SESSION_OUTPUT, payload);
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn spawn_output_quiet_monitor(
    app: AppHandle,
    last_output_at: Arc<AtomicU64>,
    done: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut last_emitted_output_at = 0;
        loop {
            thread::sleep(std::time::Duration::from_millis(
                SESSION_OUTPUT_MONITOR_TICK_MS,
            ));
            let observed = last_output_at.load(Ordering::Relaxed);
            let is_done = done.load(Ordering::Relaxed);
            if observed != 0
                && observed != last_emitted_output_at
                && now_ms().saturating_sub(observed) >= SESSION_OUTPUT_QUIET_REFRESH_MS
            {
                refresh_and_emit_sessions(&app);
                last_emitted_output_at = observed;
            }
            if is_done && (observed == 0 || observed == last_emitted_output_at) {
                return;
            }
        }
    });
}

fn refresh_and_emit_sessions(app: &AppHandle) {
    let Some(registry) = app.try_state::<Mutex<AgentSessionRegistry>>() else {
        return;
    };
    let Ok(mut registry) = registry.lock() else {
        return;
    };
    if registry.refresh_session_statuses().is_err() {
        return;
    }
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(app, &sessions);
    emit_change_logs(app, &sessions);
}

fn emit_sessions_snapshot(app: &AppHandle, sessions: &[AgentSession]) {
    let _ = app.emit(EVENT_AGENT_SESSIONS_CHANGED, sessions);
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
        assert_eq!(
            error.message,
            "la fuente del repo no está disponible en este entorno"
        );
        assert!(!error.message.contains("backend local"));
        assert!(!error.message.contains("/home/me/proyecto"));
    }

    #[test]
    fn agent_binary_available_rejects_unsupported_agent() {
        let error = agent_binary_available("powershell".into()).unwrap_err();

        assert_eq!(error.category, "unsupported_agent");
    }

    #[test]
    fn wsl_agent_binary_available_rejects_unsupported_agent() {
        let error = wsl_agent_binary_available("Ubuntu", "powershell".into()).unwrap_err();

        assert_eq!(error.category, "unsupported_agent");
    }
}
