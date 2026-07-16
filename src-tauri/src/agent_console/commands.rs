use std::{
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::{
    journal::AgentJournal,
    pty::{AgentTurnAttachment, TINTO_TURN_DONE_MARKER},
    validation::{resolve_agent_binary, validate_agent_type},
    AgentConsoleError, AgentSessionRegistry,
};
use crate::bus::{
    commands::write_repo_agents_md_config,
    contract::{
        AgentHostCommandResult, AgentHostCommandStatus, AgentJournalSessionSummary,
        AgentReviewFinding, AgentReviewSummary, AgentRuntimeCatalog, AgentSession,
        AgentSessionAttachment, AgentSessionChangeLog, AgentSessionContextSummary,
        AgentSessionFeedback, AgentSessionGoalStatus, AgentSessionOutput, AgentSessionResumeMode,
        AgentSessionResumeResult, AgentSessionRuntimeOptions, AgentSessionStatus,
        AgentSessionTimelineItem, AgentSessionTimelineKind, EVENT_AGENT_SESSIONS_CHANGED,
        EVENT_AGENT_SESSION_CHANGE_LOG, EVENT_AGENT_SESSION_OUTPUT, EVENT_AGENT_SESSION_TIMELINE,
    },
    BusHandle, RepoResolveError,
};
#[cfg(target_os = "windows")]
use crate::windows_process::hide_console;
use crate::workbench::RepoSource;
use crate::workbench::{WorkbenchError, WorkbenchStore};
use crate::wsl_agent::{
    launcher::request_wsl_agent,
    protocol::{AgentRequest, AgentResponse, GitReviewSummary, PROTOCOL_VERSION},
};

const SESSION_OUTPUT_QUIET_REFRESH_MS: u64 = 2_500;
const SESSION_OUTPUT_MONITOR_TICK_MS: u64 = 500;
const MAX_SESSION_GOAL_CHARS: usize = 4_000;
pub(crate) const TIMELINE_FRAME_PREFIX: &[u8] = b"\x1dTINTO_TIMELINE ";
static TIMELINE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    emit_timeline_text(
        &app,
        &started.id,
        AgentSessionTimelineKind::Lifecycle,
        Some("Session started".to_string()),
        now_ms(),
    );
    refresh_and_emit_sessions(&app);
    Ok(started.id)
}

#[tauri::command]
pub async fn resume_agent_journal_session(
    app: AppHandle,
    bus: State<'_, BusHandle>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    journal: State<'_, Mutex<AgentJournal>>,
    session_id: String,
) -> Result<AgentSessionResumeResult, CommandError> {
    let archived = {
        let journal = lock_journal(&journal)?;
        journal
            .session_from_journal(&session_id)
            .map_err(|error| CommandError::new("agent_journal_failed", error.to_string()))?
            .ok_or_else(|| {
                CommandError::new("session_not_found", "la conversacion guardada ya no existe")
            })?
    };
    let resolved = ensure_known_agent_repo(&bus, &archived.repo).await?;
    let native_resume_id = archived.provider_session_id.clone();
    let (started, mode) = {
        let mut registry = lock_registry(&registry)?;
        let native = native_resume_id.as_deref().and_then(|provider_session_id| {
            let result = match resolved.source {
                RepoSource::Local => registry.resume_session_with_output(
                    resolved.path.clone(),
                    archived.agent_type.clone(),
                    provider_session_id.to_string(),
                ),
                RepoSource::Wsl => registry.resume_wsl_session_with_output(
                    resolved.path.clone(),
                    resolved.distro.clone()?,
                    archived.agent_type.clone(),
                    provider_session_id.to_string(),
                ),
            };
            result.ok()
        });
        match native {
            Some(started) => (started, AgentSessionResumeMode::Native),
            None => {
                let started = match resolved.source {
                    RepoSource::Local => registry.start_session_with_output(
                        resolved.path.clone(),
                        archived.agent_type.clone(),
                    )?,
                    RepoSource::Wsl => registry.start_wsl_session_with_output(
                        resolved.path.clone(),
                        resolved.distro.clone().ok_or_else(|| {
                            CommandError::new("missing_distro", "repo WSL sin distro")
                        })?,
                        archived.agent_type.clone(),
                    )?,
                };
                (started, AgentSessionResumeMode::ContextBridge)
            }
        }
    };

    if let Some(output_reader) = started.output_reader {
        spawn_output_reader(app.clone(), started.id.clone(), output_reader);
    }

    let resumed_timeline = remap_archived_timeline(&archived.timeline, &started.id);
    {
        let mut registry = lock_registry(&registry)?;
        registry
            .set_session_runtime_options(&started.id, archived.runtime_options.clone())
            .map_err(CommandError::from)?;
        if mode == AgentSessionResumeMode::ContextBridge && archived.agent_type == "codex" {
            if let Some(goal) = archived.goal.as_ref() {
                registry
                    .restore_session_goal(&started.id, goal.clone())
                    .map_err(CommandError::from)?;
            }
        }
        if let Some(personality) = archived.personality.as_ref() {
            registry
                .set_session_personality(
                    &started.id,
                    personality.name.clone(),
                    personality.updated_at_ms,
                )
                .map_err(CommandError::from)?;
        }
        if let Some(plan_mode) = archived.plan_mode.as_ref() {
            registry
                .set_session_plan_mode(&started.id, plan_mode.enabled, plan_mode.updated_at_ms)
                .map_err(CommandError::from)?;
        }
        for feedback in archived.feedback.iter().cloned() {
            registry
                .add_session_feedback(&started.id, feedback)
                .map_err(CommandError::from)?;
        }
        let summary = match mode {
            AgentSessionResumeMode::Native => archived.context_summary.clone(),
            AgentSessionResumeMode::ContextBridge => Some(resume_context_summary(&archived)),
        };
        if let Some(summary) = summary {
            registry
                .set_session_context_summary(&started.id, summary)
                .map_err(CommandError::from)?;
        }
    }
    for item in resumed_timeline {
        record_timeline_item(&app, item.clone());
        let _ = app.emit(EVENT_AGENT_SESSION_TIMELINE, item);
    }
    emit_timeline_text(
        &app,
        &started.id,
        AgentSessionTimelineKind::Lifecycle,
        Some(match mode {
            AgentSessionResumeMode::Native => "Conversacion de Codex retomada".to_string(),
            AgentSessionResumeMode::ContextBridge => {
                "Conversacion retomada con el contexto archivado".to_string()
            }
        }),
        now_ms(),
    );
    refresh_and_emit_sessions(&app);
    Ok(AgentSessionResumeResult {
        session_id: started.id,
        mode,
    })
}

fn remap_archived_timeline(
    timeline: &[AgentSessionTimelineItem],
    session_id: &str,
) -> Vec<AgentSessionTimelineItem> {
    timeline
        .iter()
        .enumerate()
        .map(|(index, item)| AgentSessionTimelineItem {
            session_id: session_id.to_string(),
            id: format!("{session_id}:resumed:{index}"),
            kind: item.kind,
            text: item.text.clone(),
            timestamp_ms: item.timestamp_ms,
            attachments: item.attachments.clone(),
        })
        .collect()
}

fn resume_context_summary(session: &AgentSession) -> AgentSessionContextSummary {
    if let Some(summary) = session.context_summary.clone() {
        return summary;
    }
    context_summary_from_timeline(&session.timeline)
}

fn context_summary_from_timeline(
    timeline: &[AgentSessionTimelineItem],
) -> AgentSessionContextSummary {
    let mut lines = timeline
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                AgentSessionTimelineKind::UserMessage
                    | AgentSessionTimelineKind::SteerMessage
                    | AgentSessionTimelineKind::AgentMessage
            )
        })
        .rev()
        .take(10)
        .map(|item| {
            let role = if matches!(
                item.kind,
                AgentSessionTimelineKind::UserMessage | AgentSessionTimelineKind::SteerMessage
            ) {
                "Usuario"
            } else {
                "Agente"
            };
            let compact = item.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let compact = compact.chars().take(220).collect::<String>();
            format!("{role}: {compact}")
        })
        .collect::<Vec<_>>();
    lines.reverse();
    AgentSessionContextSummary {
        text: if lines.is_empty() {
            "Continua la conversacion archivada en el mismo repositorio y conserva sus decisiones previas."
                .to_string()
        } else {
            lines.join(" | ")
        },
        created_at_ms: now_ms(),
        source_events: timeline.len(),
        source_turns: timeline
            .iter()
            .filter(|item| item.kind == AgentSessionTimelineKind::UserMessage)
            .count(),
    }
}

#[tauri::command]
pub async fn branch_agent_session_from_message(
    app: AppHandle,
    bus: State<'_, BusHandle>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    message_id: String,
) -> Result<AgentSessionResumeResult, CommandError> {
    let source = {
        let registry = lock_registry(&registry)?;
        registry
            .get_session(&session_id)
            .ok_or_else(|| CommandError::new("session_not_found", "la conversación ya no existe"))?
    };
    let previous_timeline = timeline_before_user_message(&source.timeline, &message_id)?;
    let resolved = ensure_known_agent_repo(&bus, &source.repo).await?;
    let started_result =
        {
            let mut registry = lock_registry(&registry)?;
            if matches!(
                source.status,
                AgentSessionStatus::Starting | AgentSessionStatus::Running
            ) {
                registry
                    .stop_session(&session_id)
                    .map_err(CommandError::from)?;
            }
            match resolved.source {
                RepoSource::Local => registry
                    .start_session_with_output(resolved.path.clone(), source.agent_type.clone()),
                RepoSource::Wsl => registry.start_wsl_session_with_output(
                    resolved.path.clone(),
                    resolved.distro.clone().ok_or_else(|| {
                        CommandError::new("missing_distro", "repo WSL sin distro")
                    })?,
                    source.agent_type.clone(),
                ),
            }
        };
    let started = match started_result {
        Ok(started) => started,
        Err(error) => {
            refresh_and_emit_sessions(&app);
            return Err(CommandError::from(error));
        }
    };
    if let Some(output_reader) = started.output_reader {
        spawn_output_reader(app.clone(), started.id.clone(), output_reader);
    }
    {
        let mut registry = lock_registry(&registry)?;
        registry
            .set_session_runtime_options(&started.id, source.runtime_options.clone())
            .map_err(CommandError::from)?;
        registry
            .set_session_context_summary(
                &started.id,
                context_summary_from_timeline(&previous_timeline),
            )
            .map_err(CommandError::from)?;
    }
    for item in remap_archived_timeline(&previous_timeline, &started.id) {
        record_timeline_item(&app, item.clone());
        let _ = app.emit(EVENT_AGENT_SESSION_TIMELINE, item);
    }
    emit_timeline_text(
        &app,
        &started.id,
        AgentSessionTimelineKind::Lifecycle,
        Some("Conversación continuada desde el último mensaje editado".to_string()),
        now_ms(),
    );
    refresh_and_emit_sessions(&app);
    Ok(AgentSessionResumeResult {
        session_id: started.id,
        mode: AgentSessionResumeMode::ContextBridge,
    })
}

fn timeline_before_user_message(
    timeline: &[AgentSessionTimelineItem],
    message_id: &str,
) -> Result<Vec<AgentSessionTimelineItem>, CommandError> {
    let target_index = timeline
        .iter()
        .position(|item| {
            item.id == message_id && item.kind == AgentSessionTimelineKind::UserMessage
        })
        .ok_or_else(|| CommandError::new("turn_not_found", "no se encontró el mensaje a editar"))?;
    Ok(timeline[..target_index].to_vec())
}

#[derive(Debug, Deserialize)]
struct TimelineFrame {
    kind: AgentSessionTimelineKind,
    text: String,
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
pub fn get_agent_runtime_catalog(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    refresh: Option<bool>,
) -> Result<Option<AgentRuntimeCatalog>, CommandError> {
    let mut registry = lock_registry(&registry)?;
    registry
        .session_runtime_catalog(&session_id, refresh.unwrap_or(false))
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn list_agent_journal_sessions(
    journal: State<'_, Mutex<AgentJournal>>,
    limit: Option<usize>,
) -> Result<Vec<AgentJournalSessionSummary>, CommandError> {
    let journal = lock_journal(&journal)?;
    journal
        .session_summaries(limit.unwrap_or(24))
        .map_err(|error| CommandError::new("agent_journal_failed", error.to_string()))
}

#[tauri::command]
pub fn get_agent_journal_session(
    journal: State<'_, Mutex<AgentJournal>>,
    session_id: String,
) -> Result<Option<AgentSession>, CommandError> {
    let journal = lock_journal(&journal)?;
    journal
        .session_from_journal(&session_id)
        .map_err(|error| CommandError::new("agent_journal_failed", error.to_string()))
}

#[tauri::command]
pub fn delete_agent_journal_session(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    journal: State<'_, Mutex<AgentJournal>>,
    session_id: String,
) -> Result<bool, CommandError> {
    if session_id.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_session_id",
            "la sesión guardada no tiene un identificador válido",
        ));
    }
    {
        let mut registry = lock_registry(&registry)?;
        registry
            .refresh_session_statuses()
            .map_err(CommandError::from)?;
        let active = registry.list_sessions().iter().any(|session| {
            session.id == session_id
                && matches!(
                    session.status,
                    AgentSessionStatus::Starting | AgentSessionStatus::Running
                )
        });
        if active {
            return Err(CommandError::new(
                "agent_session_active",
                "detén la sesión antes de eliminar su conversación guardada",
            ));
        }
    }
    let journal = lock_journal(&journal)?;
    journal
        .delete_session(&session_id)
        .map_err(|error| CommandError::new("agent_journal_failed", error.to_string()))
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
    options: Option<AgentSessionRuntimeOptions>,
) -> Result<(), CommandError> {
    let input = STANDARD
        .decode(input_base64)
        .map_err(|e| CommandError::new("invalid_input", e.to_string()))?;
    let timestamp_ms = now_ms();
    let mut registry = lock_registry(&registry)?;
    registry
        .write_session_input(&session_id, &input, options)
        .map_err(CommandError::from)?;
    drop(registry);
    emit_timeline_text(
        &app,
        &session_id,
        AgentSessionTimelineKind::UserMessage,
        timeline_text_from_input(&input),
        timestamp_ms,
    );
    refresh_and_emit_sessions(&app);
    Ok(())
}

#[tauri::command]
pub fn write_agent_session_turn(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    text: String,
    attachment_paths: Vec<String>,
    options: Option<AgentSessionRuntimeOptions>,
) -> Result<(), CommandError> {
    if text.trim().is_empty() && attachment_paths.is_empty() {
        return Err(CommandError::new(
            "invalid_input",
            "el mensaje o los archivos adjuntos no pueden estar vacios",
        ));
    }
    if attachment_paths.len() > 10 {
        return Err(CommandError::new(
            "too_many_attachments",
            "puedes adjuntar hasta 10 archivos por turno",
        ));
    }
    let attachments = attachment_paths
        .into_iter()
        .map(PathBuf::from)
        .map(validate_agent_attachment_path)
        .collect::<Result<Vec<_>, _>>()?;
    if attachments
        .iter()
        .filter(|attachment| attachment.is_image)
        .count()
        > 4
    {
        return Err(CommandError::new(
            "too_many_images",
            "puedes adjuntar hasta 4 imagenes por turno",
        ));
    }
    let text = if text.trim().is_empty() {
        "Revisa los archivos adjuntos.".to_string()
    } else {
        text
    };
    let timestamp_ms = now_ms();
    let mut registry = lock_registry(&registry)?;
    registry
        .write_session_turn(&session_id, &text, &attachments, options)
        .map_err(CommandError::from)?;
    drop(registry);
    let timeline_attachments = attachments
        .iter()
        .map(|attachment| AgentSessionAttachment {
            path: attachment.path.clone(),
            name: attachment
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "archivo".to_string()),
            is_image: attachment.is_image,
        })
        .collect::<Vec<_>>();
    emit_timeline_item(
        &app,
        &session_id,
        AgentSessionTimelineKind::UserMessage,
        Some(text),
        timestamp_ms,
        timeline_attachments,
    );
    refresh_and_emit_sessions(&app);
    Ok(())
}

#[tauri::command]
pub fn steer_agent_session_turn(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    text: String,
    attachment_paths: Vec<String>,
) -> Result<(), CommandError> {
    if text.trim().is_empty() && attachment_paths.is_empty() {
        return Err(CommandError::new(
            "invalid_input",
            "el mensaje o los archivos adjuntos no pueden estar vacios",
        ));
    }
    if attachment_paths.len() > 10 {
        return Err(CommandError::new(
            "too_many_attachments",
            "puedes adjuntar hasta 10 archivos por mensaje",
        ));
    }
    let attachments = attachment_paths
        .into_iter()
        .map(PathBuf::from)
        .map(validate_agent_attachment_path)
        .collect::<Result<Vec<_>, _>>()?;
    let text = if text.trim().is_empty() {
        "Revisa los archivos adjuntos.".to_string()
    } else {
        text
    };
    let timestamp_ms = now_ms();
    let mut registry = lock_registry(&registry)?;
    registry
        .steer_session_turn(&session_id, &text, &attachments)
        .map_err(CommandError::from)?;
    drop(registry);
    let timeline_attachments = attachments
        .iter()
        .map(|attachment| AgentSessionAttachment {
            path: attachment.path.clone(),
            name: attachment
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "archivo".to_string()),
            is_image: attachment.is_image,
        })
        .collect::<Vec<_>>();
    emit_timeline_item(
        &app,
        &session_id,
        AgentSessionTimelineKind::SteerMessage,
        Some(text),
        timestamp_ms,
        timeline_attachments,
    );
    refresh_and_emit_sessions(&app);
    Ok(())
}

#[tauri::command]
pub fn get_agent_image_preview(path: String) -> Result<Option<String>, CommandError> {
    let attachment = validate_agent_attachment_path(PathBuf::from(path))?;
    if !attachment.is_image {
        return Ok(None);
    }
    let path = attachment.path;
    let metadata = std::fs::metadata(&path)
        .map_err(|error| CommandError::new("image_not_found", error.to_string()))?;
    if metadata.len() > 4 * 1024 * 1024 {
        return Ok(None);
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return Ok(None),
    };
    let bytes = std::fs::read(path)
        .map_err(|error| CommandError::new("image_preview_failed", error.to_string()))?;
    Ok(Some(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    )))
}

fn validate_agent_attachment_path(path: PathBuf) -> Result<AgentTurnAttachment, CommandError> {
    if !path.is_absolute() {
        return Err(CommandError::new(
            "invalid_attachment_path",
            "la ruta del archivo adjunto debe ser absoluta",
        ));
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_image = matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif");
    let metadata = std::fs::metadata(&path)
        .map_err(|error| CommandError::new("attachment_not_found", error.to_string()))?;
    if !metadata.is_file() {
        return Err(CommandError::new(
            "invalid_attachment_path",
            "el adjunto no es un archivo",
        ));
    }
    if is_image && metadata.len() > 20 * 1024 * 1024 {
        return Err(CommandError::new(
            "image_too_large",
            "cada imagen debe pesar 20 MB o menos",
        ));
    }
    Ok(AgentTurnAttachment { path, is_image })
}

#[tauri::command]
pub async fn run_agent_host_command(
    app: AppHandle,
    bus: State<'_, BusHandle>,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    workbenches: State<'_, Mutex<WorkbenchStore>>,
    session_id: String,
    command: String,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let normalized = normalize_host_command(&command);
    let session = {
        let mut registry = lock_registry(&registry)?;
        registry
            .refresh_session_statuses()
            .map_err(CommandError::from)?;
        registry
            .get_session(&session_id)
            .ok_or_else(|| CommandError::new("session_not_found", "session not found"))?
    };

    let result = match normalized.as_str() {
        "status" => AgentHostCommandResult {
            command: normalized.clone(),
            status: AgentHostCommandStatus::Completed,
            message: host_status_message(&session),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        },
        "init" => {
            create_agents_md_for_session(&bus, &session).await?;
            refresh_and_emit_sessions(&app);
            AgentHostCommandResult {
                command: normalized.clone(),
                status: AgentHostCommandStatus::Completed,
                message: "AGENTS.md is configured for this repo.".to_string(),
                session_id: None,
                repo: None,
                agent_type: None,
                review_summary: None,
                review_findings: None,
            }
        }
        "goal" | "objective" => run_goal_host_command(&app, &registry, &session, argument)?,
        "plan" | "plan-mode" => {
            run_plan_mode_host_command(&app, &registry, &session, &normalized, argument)?
        }
        "personality" => run_personality_host_command(&app, &registry, &session, argument)?,
        "comments" | "feedback" => {
            run_feedback_host_command(&app, &registry, &session, &normalized, argument)?
        }
        "review" | "code-review" => run_review_host_command(&bus, &session, &normalized).await?,
        "compact" => run_compact_host_command(&app, &registry, &session, argument)?,
        "branch" | "fork" | "lateral" => {
            run_fork_host_command(
                &normalized,
                &app,
                &bus,
                &registry,
                &workbenches,
                &session,
                argument,
            )
            .await?
        }
        "mcp" => run_mcp_host_command()?,
        "details" => AgentHostCommandResult {
            command: normalized.clone(),
            status: AgentHostCommandStatus::Completed,
            message: "Session details opened in Tinto.".to_string(),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        },
        "model" | "reasoning" | "effort" | "fast" => AgentHostCommandResult {
            command: normalized.clone(),
            status: AgentHostCommandStatus::Completed,
            message: "Use the Codex runtime controls in the composer for this command.".to_string(),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        },
        known if is_known_pending_host_command(known) => AgentHostCommandResult {
            command: known.to_string(),
            status: AgentHostCommandStatus::Unavailable,
            message: format!(
                "/{} needs a Tinto host backend before it can run from this palette.",
                known
            ),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        },
        _ => AgentHostCommandResult {
            command: normalized.clone(),
            status: AgentHostCommandStatus::Unavailable,
            message: format!("/{command} is not a known Tinto host command."),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        },
    };
    Ok(result)
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

#[tauri::command]
pub fn restore_session_turn(
    app: AppHandle,
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
    turn_checkpoint_id: String,
    user_consent: bool,
) -> Result<AgentSession, CommandError> {
    let mut registry = lock_registry(&registry)?;
    let session = registry
        .restore_turn(&session_id, &turn_checkpoint_id, user_consent)
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

async fn create_agents_md_for_session(
    bus: &BusHandle,
    session: &AgentSession,
) -> Result<(), CommandError> {
    let resolved = ensure_known_agent_repo(bus, &session.repo).await?;
    match resolved.source {
        RepoSource::Local => {
            let repo = resolved.path;
            tokio::task::spawn_blocking(move || write_repo_agents_md_config(&repo))
                .await
                .map_err(|_| CommandError::new("internal", "AGENTS.md task failed"))?
                .map_err(|error| CommandError::new(error.category, error.message))
        }
        RepoSource::Wsl => {
            let distro = resolved
                .distro
                .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?;
            match request_wsl_agent(
                &distro,
                &AgentRequest::CreateAgentsMdConfig {
                    protocol_version: PROTOCOL_VERSION,
                    repo: resolved.path,
                    allowed_repos: resolved.wsl_repos,
                },
            ) {
                Ok(AgentResponse::Unit) => Ok(()),
                Ok(AgentResponse::Error { category, message }) => {
                    Err(CommandError::new(category, message))
                }
                Ok(_) => Err(CommandError::new(
                    "malformed_response",
                    "respuesta inesperada del agente WSL",
                )),
                Err(error) => Err(CommandError::new(error.safe_category(), error.message)),
            }
        }
    }
}

fn normalize_host_command(command: &str) -> String {
    command
        .trim()
        .trim_start_matches('/')
        .to_lowercase()
        .replace('_', "-")
}

fn is_known_pending_host_command(command: &str) -> bool {
    matches!(command, "mascot" | "memories" | "memory")
}

async fn run_fork_host_command(
    command_name: &str,
    app: &AppHandle,
    bus: &BusHandle,
    registry: &Mutex<AgentSessionRegistry>,
    workbenches: &Mutex<WorkbenchStore>,
    session: &AgentSession,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let requested_mode = argument.unwrap_or_default().trim().to_lowercase();
    let worktree_requested = matches!(
        requested_mode.as_str(),
        "worktree" | "new-worktree" | "isolated"
    );
    let session_active = matches!(
        session.status,
        AgentSessionStatus::Starting | AgentSessionStatus::Running
    );

    let resolved = ensure_known_agent_repo(bus, &session.repo).await?;
    let (launch_repo, fork_kind) = match resolved.source {
        RepoSource::Local if session_active || worktree_requested => {
            let Some(worktree_repo) =
                provision_local_fork_worktree(bus, workbenches, &resolved.path, session).await?
            else {
                return Ok(AgentHostCommandResult {
                    command: command_name.to_string(),
                    status: AgentHostCommandStatus::Unavailable,
                    message:
                        "This repo has no HEAD commit yet; worktree forks need at least one commit."
                            .to_string(),
                    session_id: None,
                    repo: None,
                    agent_type: None,
                    review_summary: None,
                    review_findings: None,
                });
            };
            (worktree_repo, "worktree")
        }
        RepoSource::Wsl if session_active || worktree_requested => {
            let distro = resolved
                .distro
                .clone()
                .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?;
            let Some(worktree_repo) = provision_wsl_fork_worktree(
                bus,
                workbenches,
                &distro,
                &resolved.path,
                &resolved.wsl_repos,
                session,
            )
            .await?
            else {
                return Ok(AgentHostCommandResult {
                    command: command_name.to_string(),
                    status: AgentHostCommandStatus::Unavailable,
                    message:
                        "This repo has no HEAD commit yet; worktree forks need at least one commit."
                            .to_string(),
                    session_id: None,
                    repo: None,
                    agent_type: None,
                    review_summary: None,
                    review_findings: None,
                });
            };
            (worktree_repo, "worktree")
        }
        _ => (resolved.path.clone(), "session"),
    };
    let started_result = {
        let mut registry = lock_registry(registry)?;
        match resolved.source {
            RepoSource::Local => registry
                .start_session_with_output(launch_repo.clone(), session.agent_type.clone())
                .map_err(CommandError::from),
            RepoSource::Wsl => {
                let distro = resolved
                    .distro
                    .clone()
                    .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?;
                registry
                    .start_wsl_session_with_output(
                        launch_repo.clone(),
                        distro,
                        session.agent_type.clone(),
                    )
                    .map_err(CommandError::from)
            }
        }
    };
    let started = match started_result {
        Ok(started) => started,
        Err(error) => {
            if fork_kind == "worktree" {
                match resolved.source {
                    RepoSource::Local => {
                        cleanup_local_fork_worktree(bus, workbenches, &resolved.path, &launch_repo)
                            .await;
                    }
                    RepoSource::Wsl => {
                        if let Some(distro) = resolved.distro.as_deref() {
                            cleanup_wsl_fork_worktree(
                                bus,
                                workbenches,
                                distro,
                                &resolved.path,
                                &launch_repo,
                                &resolved.wsl_repos,
                            )
                            .await;
                        }
                    }
                }
            }
            return Err(error);
        }
    };
    if let Some(output_reader) = started.output_reader {
        spawn_output_reader(app.clone(), started.id.clone(), output_reader);
    }
    emit_timeline_text(
        app,
        &started.id,
        AgentSessionTimelineKind::Lifecycle,
        Some(format!(
            "Forked from session {} into {fork_kind}",
            session.id
        )),
        now_ms(),
    );
    refresh_and_emit_sessions(app);
    Ok(AgentHostCommandResult {
        command: command_name.to_string(),
        status: AgentHostCommandStatus::Completed,
        message: format!(
            "Forked {fork_kind} session {} from {}.",
            started.id, session.id
        ),
        session_id: Some(started.id),
        repo: Some(launch_repo),
        agent_type: Some(session.agent_type.clone()),
        review_summary: None,
        review_findings: None,
    })
}

async fn provision_local_fork_worktree(
    bus: &BusHandle,
    workbenches: &Mutex<WorkbenchStore>,
    source_repo: &Path,
    session: &AgentSession,
) -> Result<Option<PathBuf>, CommandError> {
    if !git_has_head(source_repo)? {
        return Ok(None);
    }
    let target = local_fork_worktree_path(source_repo, &session.id)?;
    let target_parent = target
        .parent()
        .ok_or_else(|| CommandError::new("worktree_path_invalid", "invalid worktree path"))?;
    std::fs::create_dir_all(target_parent)
        .map_err(|error| CommandError::new("worktree_create_failed", error.to_string()))?;
    create_git_worktree(source_repo, &target)?;
    let canonical = target
        .canonicalize()
        .map_err(|error| CommandError::new("worktree_create_failed", error.to_string()))?;
    let repos = {
        let mut store = lock_workbenches(workbenches)?;
        let active = store
            .active_workbench_runtime()
            .ok_or_else(|| CommandError::new("workbench_not_active", "no active workbench"))?;
        let alias = Some(format!("fork {}", short_session_id(&session.id)));
        if let Err(error) = store.add_repo(&active.name, canonical.clone(), alias, true) {
            let _ = remove_git_worktree(source_repo, &canonical);
            return Err(map_workbench_error(error));
        }
        store
            .active_workbench_runtime()
            .filter(|workbench| workbench.name == active.name)
            .map(|workbench| workbench.repos)
            .unwrap_or_default()
    };
    bus.set_workbench(repos);
    Ok(Some(canonical))
}

async fn provision_wsl_fork_worktree(
    bus: &BusHandle,
    workbenches: &Mutex<WorkbenchStore>,
    distro: &str,
    source_repo: &Path,
    allowed_repos: &[PathBuf],
    session: &AgentSession,
) -> Result<Option<PathBuf>, CommandError> {
    let target = match request_wsl_agent(
        distro,
        &AgentRequest::CreateGitWorktree {
            protocol_version: PROTOCOL_VERSION,
            repo: source_repo.to_path_buf(),
            allowed_repos: allowed_repos.to_vec(),
            session_id: session.id.clone(),
        },
    ) {
        Ok(AgentResponse::GitWorktreeCreated { path }) => path,
        Ok(AgentResponse::Error { category, message }) if category == "worktree_no_head" => {
            let _ = message;
            return Ok(None);
        }
        Ok(AgentResponse::Error { category, message }) => {
            return Err(CommandError::new(category, message));
        }
        Ok(_) => {
            return Err(CommandError::new(
                "malformed_response",
                "respuesta inesperada del agente WSL",
            ));
        }
        Err(error) => return Err(CommandError::new(error.safe_category(), error.message)),
    };
    let repos = {
        let mut store = lock_workbenches(workbenches)?;
        let active = store
            .active_workbench_runtime()
            .ok_or_else(|| CommandError::new("workbench_not_active", "no active workbench"))?;
        let alias = Some(format!("fork {}", short_session_id(&session.id)));
        if let Err(error) = store.add_wsl_repo(
            &active.name,
            distro.to_string(),
            target.to_string_lossy().to_string(),
            alias,
        ) {
            cleanup_wsl_git_worktree(distro, source_repo, &target, allowed_repos);
            return Err(map_workbench_error(error));
        }
        store
            .active_workbench_runtime()
            .filter(|workbench| workbench.name == active.name)
            .map(|workbench| workbench.repos)
            .unwrap_or_default()
    };
    bus.set_workbench(repos);
    Ok(Some(target))
}

async fn cleanup_local_fork_worktree(
    bus: &BusHandle,
    workbenches: &Mutex<WorkbenchStore>,
    source_repo: &Path,
    worktree_repo: &Path,
) {
    let repos = {
        let Ok(mut store) = workbenches.lock() else {
            let _ = remove_git_worktree(source_repo, worktree_repo);
            return;
        };
        let active_name = store
            .active_workbench_runtime()
            .map(|workbench| workbench.name);
        if let Some(active_name) = active_name {
            let _ = store.remove_repo(&active_name, worktree_repo);
            store
                .active_workbench_runtime()
                .filter(|workbench| workbench.name == active_name)
                .map(|workbench| workbench.repos)
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    if !repos.is_empty() {
        bus.set_workbench(repos);
    }
    let _ = remove_git_worktree(source_repo, worktree_repo);
}

async fn cleanup_wsl_fork_worktree(
    bus: &BusHandle,
    workbenches: &Mutex<WorkbenchStore>,
    distro: &str,
    source_repo: &Path,
    worktree_repo: &Path,
    allowed_repos: &[PathBuf],
) {
    let repos = {
        let Ok(mut store) = workbenches.lock() else {
            cleanup_wsl_git_worktree(distro, source_repo, worktree_repo, allowed_repos);
            return;
        };
        let active_name = store
            .active_workbench_runtime()
            .map(|workbench| workbench.name);
        if let Some(active_name) = active_name {
            let _ = store.remove_wsl_repo(&active_name, distro, &worktree_repo.to_string_lossy());
            store
                .active_workbench_runtime()
                .filter(|workbench| workbench.name == active_name)
                .map(|workbench| workbench.repos)
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    };
    if !repos.is_empty() {
        bus.set_workbench(repos);
    }
    cleanup_wsl_git_worktree(distro, source_repo, worktree_repo, allowed_repos);
}

fn cleanup_wsl_git_worktree(
    distro: &str,
    source_repo: &Path,
    worktree_repo: &Path,
    allowed_repos: &[PathBuf],
) {
    let _ = request_wsl_agent(
        distro,
        &AgentRequest::RemoveGitWorktree {
            protocol_version: PROTOCOL_VERSION,
            repo: source_repo.to_path_buf(),
            allowed_repos: allowed_repos.to_vec(),
            target: worktree_repo.to_path_buf(),
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpServerSummary {
    name: String,
    command_found: Option<bool>,
}

fn run_mcp_host_command() -> Result<AgentHostCommandResult, CommandError> {
    Ok(AgentHostCommandResult {
        command: "mcp".to_string(),
        status: AgentHostCommandStatus::Completed,
        message: codex_mcp_status_message()?,
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

fn codex_mcp_status_message() -> Result<String, CommandError> {
    let Some(config_path) = codex_config_path() else {
        return Ok("No Codex config directory was found for MCP status.".to_string());
    };
    if !config_path.is_file() {
        return Ok("No Codex MCP config was found.".to_string());
    }
    let raw = std::fs::read_to_string(&config_path)
        .map_err(|error| CommandError::new("mcp_config_read_failed", error.to_string()))?;
    let servers = mcp_servers_from_codex_config(&raw)
        .map_err(|error| CommandError::new("mcp_config_invalid", error.to_string()))?;
    if servers.is_empty() {
        return Ok("No MCP servers are configured in Codex config.".to_string());
    }
    let available = servers
        .iter()
        .filter(|server| server.command_found == Some(true))
        .count();
    let missing = servers
        .iter()
        .filter(|server| server.command_found == Some(false))
        .count();
    let unknown = servers.len().saturating_sub(available + missing);
    let names = servers
        .iter()
        .take(8)
        .map(|server| match server.command_found {
            Some(true) => format!("{}: command found", server.name),
            Some(false) => format!("{}: command missing", server.name),
            None => format!("{}: command unchecked", server.name),
        })
        .collect::<Vec<_>>()
        .join("; ");
    let overflow = servers.len().saturating_sub(8);
    let overflow_text = if overflow > 0 {
        format!("; plus {overflow} more")
    } else {
        String::new()
    };
    Ok(format!(
        "MCP: {} configured server(s); {} command(s) found, {} missing, {} unchecked. {}{}.",
        servers.len(),
        available,
        missing,
        unknown,
        names,
        overflow_text
    ))
}

fn codex_config_path() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::runtime_paths::user_home_dir().map(|home| home.join(".codex")))?;
    Some(home.join("config.toml"))
}

fn mcp_servers_from_codex_config(raw: &str) -> Result<Vec<McpServerSummary>, toml::de::Error> {
    let value = toml::from_str::<toml::Value>(raw)?;
    let mut servers = std::collections::BTreeMap::new();
    for table_name in ["mcp_servers", "mcpServers"] {
        let Some(table) = value.get(table_name).and_then(toml::Value::as_table) else {
            continue;
        };
        for (name, server) in table {
            let command_found = server
                .get("command")
                .and_then(toml::Value::as_str)
                .and_then(command_availability);
            servers.entry(name.clone()).or_insert(McpServerSummary {
                name: name.to_string(),
                command_found,
            });
        }
    }
    Ok(servers.into_values().collect())
}

fn command_availability(command: &str) -> Option<bool> {
    let command = command.trim();
    if command.is_empty() {
        return Some(false);
    }
    let path = Path::new(command);
    if path.is_absolute() || command.contains('\\') || command.contains('/') {
        return Some(path.is_file());
    }
    None
}

fn run_compact_host_command(
    app: &AppHandle,
    registry: &Mutex<AgentSessionRegistry>,
    session: &AgentSession,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let raw = argument.unwrap_or_default();
    let argument = raw.trim();
    let mut registry = lock_registry(registry)?;
    let updated = if matches!(
        argument.to_lowercase().as_str(),
        "clear" | "reset" | "none" | "off"
    ) {
        registry
            .clear_session_context_summary(&session.id)
            .map_err(CommandError::from)?
    } else {
        let summary = build_context_summary(session, now_ms());
        registry
            .set_session_context_summary(&session.id, summary)
            .map_err(CommandError::from)?
    };
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(app, &sessions);
    Ok(AgentHostCommandResult {
        command: "compact".to_string(),
        status: AgentHostCommandStatus::Completed,
        message: updated
            .context_summary
            .as_ref()
            .map(|summary| {
                format!(
                    "Context summary saved from {} events across {} turns.",
                    summary.source_events, summary.source_turns
                )
            })
            .unwrap_or_else(|| "Context summary cleared for this session.".to_string()),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

fn build_context_summary(session: &AgentSession, created_at_ms: u64) -> AgentSessionContextSummary {
    let mut lines = Vec::new();
    lines.push(format!("Session: {} ({})", session.id, session.agent_type));
    lines.push(format!("Repo: {}", session.repo.to_string_lossy()));
    lines.push(format!("Status: {:?}", session.status));
    if let Some(goal) = session.goal.as_ref() {
        lines.push(format!("Goal: {}", goal.text));
    }
    if let Some(personality) = session.personality.as_ref() {
        lines.push(format!("Personality: {}", personality.name));
    }
    if let Some(plan_mode) = session.plan_mode.as_ref() {
        lines.push(format!(
            "Plan mode: {}",
            if plan_mode.enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));
    }
    if let Some(feedback) = session.feedback.last() {
        lines.push(format!("Latest {}: {}", feedback.kind, feedback.text));
    }
    if !session.change_log.is_empty() {
        let files = session
            .change_log
            .iter()
            .take(8)
            .map(|change| format!("{}:{:?}", change.path.to_string_lossy(), change.kind))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Tracked changes: {}", files));
    }
    let event_count = session.timeline.len();
    let turn_count = session.turn_checkpoints.len();
    let recent = session
        .timeline
        .iter()
        .rev()
        .filter(|item| !item.text.trim().is_empty())
        .take(6)
        .map(|item| format!("{:?}: {}", item.kind, compact_summary_text(&item.text)))
        .collect::<Vec<_>>();
    if !recent.is_empty() {
        lines.push("Recent timeline:".to_string());
        for item in recent.into_iter().rev() {
            lines.push(format!("- {item}"));
        }
    }
    AgentSessionContextSummary {
        text: lines.join("\n"),
        created_at_ms,
        source_events: event_count,
        source_turns: turn_count,
    }
}

fn compact_summary_text(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const MAX_LEN: usize = 180;
    if compact.len() <= MAX_LEN {
        compact
    } else {
        let truncated = compact.chars().take(MAX_LEN).collect::<String>();
        format!("{truncated}...")
    }
}

fn run_plan_mode_host_command(
    app: &AppHandle,
    registry: &Mutex<AgentSessionRegistry>,
    session: &AgentSession,
    command_name: &str,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let raw = argument.unwrap_or_default();
    let argument = raw.trim().to_lowercase();
    let current = session
        .plan_mode
        .as_ref()
        .map(|plan_mode| plan_mode.enabled)
        .unwrap_or(false);
    if argument.is_empty() {
        return Ok(AgentHostCommandResult {
            command: command_name.to_string(),
            status: AgentHostCommandStatus::Completed,
            message: format!(
                "Plan mode is currently {}. Use /plan on, /plan off, or /plan toggle.",
                if current { "enabled" } else { "disabled" }
            ),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        });
    }
    let next = match argument.as_str() {
        "on" | "true" | "yes" | "enable" | "enabled" => true,
        "off" | "false" | "no" | "disable" | "disabled" | "clear" | "reset" => false,
        "toggle" => !current,
        _ => {
            return Ok(AgentHostCommandResult {
                command: command_name.to_string(),
                status: AgentHostCommandStatus::Unavailable,
                message: "Use /plan on, /plan off, or /plan toggle.".to_string(),
                session_id: None,
                repo: None,
                agent_type: None,
                review_summary: None,
                review_findings: None,
            });
        }
    };
    let mut registry = lock_registry(registry)?;
    let updated = registry
        .set_session_plan_mode(&session.id, next, now_ms())
        .map_err(CommandError::from)?;
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(app, &sessions);
    let enabled = updated
        .plan_mode
        .as_ref()
        .map(|plan_mode| plan_mode.enabled)
        .unwrap_or(false);
    Ok(AgentHostCommandResult {
        command: command_name.to_string(),
        status: AgentHostCommandStatus::Completed,
        message: format!(
            "Plan mode {} for this session.",
            if enabled { "enabled" } else { "disabled" }
        ),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

fn run_goal_host_command(
    app: &AppHandle,
    registry: &Mutex<AgentSessionRegistry>,
    session: &AgentSession,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let action = parse_goal_command(argument.as_deref())?;
    if matches!(action, GoalCommand::Inspect | GoalCommand::Edit) {
        let message = session
            .goal
            .as_ref()
            .map(|goal| format!("Objetivo {}: {}", goal_status_label(goal.status), goal.text))
            .unwrap_or_else(|| "Esta conversación no tiene un objetivo activo.".to_string());
        return Ok(AgentHostCommandResult {
            command: "goal".to_string(),
            status: AgentHostCommandStatus::Completed,
            message,
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        });
    }
    let mut registry = lock_registry(registry)?;
    let updated = match action {
        GoalCommand::Inspect | GoalCommand::Edit => {
            unreachable!("inspection and editing return before mutation")
        }
        GoalCommand::Clear => registry
            .clear_session_goal(&session.id)
            .map_err(CommandError::from)?,
        GoalCommand::Set(goal) => registry
            .set_session_goal(&session.id, goal, now_ms())
            .map_err(CommandError::from)?,
        GoalCommand::Pause => registry
            .set_session_goal_status(&session.id, AgentSessionGoalStatus::Paused, now_ms())
            .map_err(CommandError::from)?,
        GoalCommand::Resume => registry
            .set_session_goal_status(&session.id, AgentSessionGoalStatus::Active, now_ms())
            .map_err(CommandError::from)?,
    };
    persist_session_snapshot(app, &updated)?;
    let sessions = registry.list_sessions();
    drop(registry);
    emit_sessions_snapshot(app, &sessions);
    Ok(AgentHostCommandResult {
        command: "goal".to_string(),
        status: AgentHostCommandStatus::Completed,
        message: updated
            .goal
            .as_ref()
            .map(|goal| format!("Objetivo {}: {}", goal_status_label(goal.status), goal.text))
            .unwrap_or_else(|| "Objetivo eliminado de esta conversación.".to_string()),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum GoalCommand {
    Inspect,
    Edit,
    Clear,
    Pause,
    Resume,
    Set(String),
}

fn parse_goal_command(argument: Option<&str>) -> Result<GoalCommand, CommandError> {
    let normalized = argument
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return Ok(GoalCommand::Inspect);
    }
    if matches!(
        normalized.to_lowercase().as_str(),
        "clear" | "borrar" | "limpiar"
    ) {
        return Ok(GoalCommand::Clear);
    }
    match normalized.to_lowercase().as_str() {
        "edit" | "editar" => return Ok(GoalCommand::Edit),
        "pause" | "pausar" => return Ok(GoalCommand::Pause),
        "resume" | "reanudar" | "continuar" => return Ok(GoalCommand::Resume),
        _ => {}
    }
    let chars = normalized.chars().count();
    if chars > MAX_SESSION_GOAL_CHARS {
        return Err(CommandError::new(
            "goal_too_long",
            format!("el objetivo tiene {chars} caracteres; el máximo es {MAX_SESSION_GOAL_CHARS}"),
        ));
    }
    Ok(GoalCommand::Set(normalized))
}

fn goal_status_label(status: AgentSessionGoalStatus) -> &'static str {
    match status {
        AgentSessionGoalStatus::Active => "activo",
        AgentSessionGoalStatus::Paused => "en pausa",
        AgentSessionGoalStatus::Blocked => "bloqueado",
        AgentSessionGoalStatus::UsageLimited => "limitado por uso",
        AgentSessionGoalStatus::BudgetLimited => "sin presupuesto",
        AgentSessionGoalStatus::Complete => "completado",
    }
}

fn persist_session_snapshot(app: &AppHandle, session: &AgentSession) -> Result<(), CommandError> {
    let journal = app.try_state::<Mutex<AgentJournal>>().ok_or_else(|| {
        CommandError::new(
            "agent_journal_unavailable",
            "no se pudo acceder al historial de la conversación",
        )
    })?;
    let journal = lock_journal(&journal)?;
    journal
        .record_session(session)
        .map_err(|error| CommandError::new("agent_journal_failed", error.to_string()))
}

fn run_personality_host_command(
    app: &AppHandle,
    registry: &Mutex<AgentSessionRegistry>,
    session: &AgentSession,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let raw = argument.unwrap_or_default();
    let personality = raw.trim();
    if personality.is_empty() {
        let message = session
            .personality
            .as_ref()
            .map(|personality| format!("Current personality: {}", personality.name))
            .unwrap_or_else(|| {
                "No personality is set for this session. Try /personality precise, /personality friendly, or /personality concise.".to_string()
            });
        return Ok(AgentHostCommandResult {
            command: "personality".to_string(),
            status: AgentHostCommandStatus::Completed,
            message,
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        });
    }
    let mut registry = lock_registry(registry)?;
    let updated = if matches!(
        personality.to_lowercase().as_str(),
        "clear" | "reset" | "none" | "off"
    ) {
        registry
            .clear_session_personality(&session.id)
            .map_err(CommandError::from)?
    } else {
        registry
            .set_session_personality(&session.id, personality.to_string(), now_ms())
            .map_err(CommandError::from)?
    };
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(app, &sessions);
    Ok(AgentHostCommandResult {
        command: "personality".to_string(),
        status: AgentHostCommandStatus::Completed,
        message: updated
            .personality
            .as_ref()
            .map(|personality| format!("Personality set: {}", personality.name))
            .unwrap_or_else(|| "Personality cleared for this session.".to_string()),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

fn run_feedback_host_command(
    app: &AppHandle,
    registry: &Mutex<AgentSessionRegistry>,
    session: &AgentSession,
    command_name: &str,
    argument: Option<String>,
) -> Result<AgentHostCommandResult, CommandError> {
    let raw = argument.unwrap_or_default();
    let text = raw.trim();
    let kind = if command_name == "comments" {
        "comment"
    } else {
        "feedback"
    };
    if text.is_empty() {
        let message = session.feedback.last().map_or_else(
            || format!("No {kind} notes are saved for this session."),
            |feedback| {
                format!(
                    "{} saved note(s). Latest {}: {}",
                    session.feedback.len(),
                    feedback.kind,
                    feedback.text
                )
            },
        );
        return Ok(AgentHostCommandResult {
            command: command_name.to_string(),
            status: AgentHostCommandStatus::Completed,
            message,
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: None,
            review_findings: None,
        });
    }
    let mut registry = lock_registry(registry)?;
    let updated = if matches!(
        text.to_lowercase().as_str(),
        "clear" | "reset" | "none" | "off"
    ) {
        registry
            .clear_session_feedback(&session.id)
            .map_err(CommandError::from)?
    } else {
        registry
            .add_session_feedback(
                &session.id,
                AgentSessionFeedback {
                    kind: kind.to_string(),
                    text: text.to_string(),
                    created_at_ms: now_ms(),
                },
            )
            .map_err(CommandError::from)?
    };
    let sessions = registry.list_sessions();
    emit_sessions_snapshot(app, &sessions);
    Ok(AgentHostCommandResult {
        command: command_name.to_string(),
        status: AgentHostCommandStatus::Completed,
        message: updated.feedback.last().map_or_else(
            || "Feedback notes cleared for this session.".to_string(),
            |feedback| format!("Saved {}: {}", feedback.kind, feedback.text),
        ),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: None,
        review_findings: None,
    })
}

async fn run_review_host_command(
    bus: &BusHandle,
    session: &AgentSession,
    command_name: &str,
) -> Result<AgentHostCommandResult, CommandError> {
    let resolved = ensure_known_agent_repo(bus, &session.repo).await?;
    if resolved.source == RepoSource::Wsl {
        let distro = resolved
            .distro
            .ok_or_else(|| CommandError::new("missing_distro", "repo WSL sin distro"))?;
        let (summary, findings) = match request_wsl_agent(
            &distro,
            &AgentRequest::GitReviewSummary {
                protocol_version: PROTOCOL_VERSION,
                repo: resolved.path,
                allowed_repos: resolved.wsl_repos,
            },
        ) {
            Ok(AgentResponse::GitReviewSummary { summary }) => {
                let findings = agent_review_findings(&summary);
                (agent_review_summary(summary), findings)
            }
            Ok(AgentResponse::Error { category, message }) => {
                return Err(CommandError::new(category, message));
            }
            Ok(_) => {
                return Err(CommandError::new(
                    "malformed_response",
                    "respuesta inesperada del agente WSL",
                ));
            }
            Err(error) => return Err(CommandError::new(error.safe_category(), error.message)),
        };
        return Ok(AgentHostCommandResult {
            command: command_name.to_string(),
            status: AgentHostCommandStatus::Completed,
            message: git_review_summary_message(&summary),
            session_id: None,
            repo: None,
            agent_type: None,
            review_summary: Some(summary),
            review_findings: Some(findings),
        });
    }
    let (summary, findings) = local_git_review(&resolved.path)?;
    Ok(AgentHostCommandResult {
        command: command_name.to_string(),
        status: AgentHostCommandStatus::Completed,
        message: git_review_summary_message(&summary),
        session_id: None,
        repo: None,
        agent_type: None,
        review_summary: Some(summary),
        review_findings: Some(findings),
    })
}

fn host_status_message(session: &AgentSession) -> String {
    let runtime = if session.runtime_options.is_empty() {
        "runtime auto".to_string()
    } else {
        format!(
            "model {}, reasoning {}, speed {}",
            session.runtime_options.model.as_deref().unwrap_or("auto"),
            session
                .runtime_options
                .reasoning_effort
                .as_deref()
                .unwrap_or("auto"),
            session
                .runtime_options
                .speed
                .as_deref()
                .unwrap_or("standard")
        )
    };
    let context_summary = session
        .context_summary
        .as_ref()
        .map(|summary| format!("saved from {} events", summary.source_events))
        .unwrap_or_else(|| "not saved".to_string());
    format!(
        "Session {}: {:?}; agent {}; repo {}; {} turns; {} tracked changes; {}; goal {}; personality {}; plan mode {}; feedback notes {}; context summary {}.",
        session.id,
        session.status,
        session.agent_type,
        session.repo.to_string_lossy(),
        session.turn_checkpoints.len(),
        session.change_log.len(),
        runtime,
        session
            .goal
            .as_ref()
            .map(|goal| goal.text.as_str())
            .unwrap_or("not set"),
        session
            .personality
            .as_ref()
            .map(|personality| personality.name.as_str())
            .unwrap_or("not set"),
        session
            .plan_mode
            .as_ref()
            .map(|plan_mode| if plan_mode.enabled { "enabled" } else { "disabled" })
            .unwrap_or("disabled"),
        session.feedback.len(),
        context_summary
    )
}

fn lock_registry(
    registry: &Mutex<AgentSessionRegistry>,
) -> Result<std::sync::MutexGuard<'_, AgentSessionRegistry>, CommandError> {
    registry
        .lock()
        .map_err(|_| CommandError::new("lock_poisoned", "el registro de agentes fallo"))
}

fn lock_workbenches(
    workbenches: &Mutex<WorkbenchStore>,
) -> Result<std::sync::MutexGuard<'_, WorkbenchStore>, CommandError> {
    workbenches
        .lock()
        .map_err(|_| CommandError::new("lock_poisoned", "el store de workbenches fallo"))
}

fn lock_journal(
    journal: &Mutex<AgentJournal>,
) -> Result<std::sync::MutexGuard<'_, AgentJournal>, CommandError> {
    journal
        .lock()
        .map_err(|_| CommandError::new("lock_poisoned", "el diario de agentes fallo"))
}

fn map_workbench_error(error: WorkbenchError) -> CommandError {
    let value = serde_json::to_value(&error).unwrap_or_default();
    let category = value
        .get("kind")
        .and_then(|kind| kind.as_str())
        .unwrap_or("workbench_error");
    CommandError::new(category, error.to_string())
}

fn git_has_head(repo: &Path) -> Result<bool, CommandError> {
    let output = git_command(repo)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map_err(|error| CommandError::new("git_unavailable", error.to_string()))?;
    Ok(output.status.success())
}

fn local_git_review(
    repo: &Path,
) -> Result<(AgentReviewSummary, Vec<AgentReviewFinding>), CommandError> {
    let branch = git_stdout(repo, &["branch", "--show-current"], "git_review_failed")?;
    let branch = if branch.trim().is_empty() {
        "detached HEAD".to_string()
    } else {
        branch.trim().to_string()
    };
    let status = git_stdout(repo, &["status", "--short"], "git_review_failed")?;
    let status_lines = status
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let files = status_lines.iter().take(12).cloned().collect::<Vec<_>>();
    let total = status_lines.len();
    let working_shortstat = git_stdout(repo, &["diff", "--shortstat"], "git_review_failed")?
        .trim()
        .to_string();
    let staged_shortstat = git_stdout(
        repo,
        &["diff", "--cached", "--shortstat"],
        "git_review_failed",
    )?
    .trim()
    .to_string();
    let truncated_count = total.saturating_sub(files.len());
    let findings = local_git_review_findings(repo, &status_lines);
    Ok((
        AgentReviewSummary {
            branch,
            changed_files: total,
            working_shortstat: empty_to_none(working_shortstat),
            staged_shortstat: empty_to_none(staged_shortstat),
            files,
            truncated_count,
        },
        findings,
    ))
}

fn local_git_review_findings(repo: &Path, status_lines: &[String]) -> Vec<AgentReviewFinding> {
    let mut findings = Vec::new();
    let changed_paths = changed_paths_from_status(repo, status_lines);
    let has_package_json = changed_paths
        .iter()
        .any(|path| path == Path::new("package.json"));
    if changed_paths
        .iter()
        .any(|path| path == Path::new("package-lock.json"))
        && !has_package_json
    {
        findings.push(AgentReviewFinding {
            severity: "medium".to_string(),
            title: "Lockfile changed without package manifest".to_string(),
            detail: "package-lock.json changed but package.json did not; verify the lockfile was intentionally regenerated without dependency metadata changes.".to_string(),
            path: Some(PathBuf::from("package-lock.json")),
            line: None,
        });
    }
    for path in changed_paths {
        if sensitive_review_path(&path) {
            findings.push(AgentReviewFinding {
                severity: "high".to_string(),
                title: "Sensitive path changed".to_string(),
                detail: format!(
                    "{} looks like an environment, credential, or secret-bearing path; verify no secrets are committed.",
                    path.display()
                ),
                path: Some(path.clone()),
                line: None,
            });
        }
        if let Some(line) = conflict_marker_line(repo, &path) {
            findings.push(AgentReviewFinding {
                severity: "high".to_string(),
                title: "Conflict marker present".to_string(),
                detail: format!(
                    "{} still contains a merge conflict marker; resolve it before review.",
                    path.display()
                ),
                path: Some(path),
                line: Some(line),
            });
        }
    }
    findings
}

fn agent_review_findings(summary: &GitReviewSummary) -> Vec<AgentReviewFinding> {
    summary
        .findings
        .iter()
        .map(|finding| AgentReviewFinding {
            severity: finding.severity.clone(),
            title: finding.title.clone(),
            detail: finding.detail.clone(),
            path: finding.path.clone(),
            line: finding.line,
        })
        .collect()
}

fn agent_review_summary(summary: GitReviewSummary) -> AgentReviewSummary {
    AgentReviewSummary {
        branch: summary.branch,
        changed_files: summary.changed_files,
        working_shortstat: summary.working_shortstat,
        staged_shortstat: summary.staged_shortstat,
        files: summary.files,
        truncated_count: summary.truncated_count,
    }
}

fn changed_paths_from_status(repo: &Path, status_lines: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in status_lines {
        let Some(path) = line.get(2..).map(str::trim) else {
            continue;
        };
        let path = path.rsplit_once(" -> ").map(|(_, new)| new).unwrap_or(path);
        if path.is_empty() {
            continue;
        }
        let path_buf = PathBuf::from(path);
        paths.push(path_buf.clone());
        if path.ends_with('/') {
            append_changed_directory_paths(repo, &path_buf, &mut paths, 64);
        }
    }
    paths
}

fn append_changed_directory_paths(
    repo: &Path,
    relative_dir: &Path,
    paths: &mut Vec<PathBuf>,
    limit: usize,
) {
    let mut added = 0;
    let mut stack = vec![relative_dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if added >= limit {
            return;
        }
        let abs = repo.join(&current);
        let Ok(entries) = std::fs::read_dir(abs) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if added >= limit {
                return;
            }
            let name = entry.file_name();
            if name == ".git" {
                continue;
            }
            let child = current.join(name);
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(child);
            } else if file_type.is_file() {
                paths.push(child);
                added += 1;
            }
        }
    }
}

fn sensitive_review_path(path: &Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name == "id_rsa"
        || file_name == "id_ed25519"
        || lower.contains("/secrets/")
        || lower.contains("/secret/")
}

fn conflict_marker_line(repo: &Path, path: &Path) -> Option<usize> {
    let abs = repo.join(path);
    if !abs.is_file() {
        return None;
    }
    let metadata = std::fs::metadata(&abs).ok()?;
    if metadata.len() > 512 * 1024 {
        return None;
    }
    let bytes = std::fs::read(&abs).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    text.lines().enumerate().find_map(|(index, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("<<<<<<< ")
            || trimmed.starts_with("=======")
            || trimmed.starts_with(">>>>>>> ")
        {
            Some(index + 1)
        } else {
            None
        }
    })
}

fn git_review_summary_message(summary: &AgentReviewSummary) -> String {
    if summary.changed_files == 0 {
        return format!(
            "Review summary for branch {}: no local changes detected.",
            summary.branch
        );
    }
    let file_list = if summary.files.is_empty() {
        "no file list available".to_string()
    } else {
        summary.files.join("; ")
    };
    let overflow_text = if summary.truncated_count > 0 {
        format!("; plus {} more", summary.truncated_count)
    } else {
        String::new()
    };
    let working = summary
        .working_shortstat
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "no unstaged line diff".to_string());
    let staged = summary
        .staged_shortstat
        .as_ref()
        .cloned()
        .unwrap_or_else(|| "no staged line diff".to_string());
    format!(
        "Review summary for branch {}: {} changed file(s). Working tree: {}. Staged: {}. Files: {}{}.",
        summary.branch, summary.changed_files, working, staged, file_list, overflow_text
    )
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn git_stdout(repo: &Path, args: &[&str], category: &str) -> Result<String, CommandError> {
    let output = git_command(repo)
        .args(args)
        .output()
        .map_err(|error| CommandError::new(category, error.to_string()))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(CommandError::new(category, git_output_message(&output)))
    }
}

fn create_git_worktree(source_repo: &Path, target: &Path) -> Result<(), CommandError> {
    let output = git_command(source_repo)
        .args(["worktree", "add", "--detach"])
        .arg(target)
        .arg("HEAD")
        .output()
        .map_err(|error| CommandError::new("worktree_create_failed", error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::new(
            "worktree_create_failed",
            git_output_message(&output),
        ))
    }
}

fn remove_git_worktree(source_repo: &Path, target: &Path) -> Result<(), CommandError> {
    let output = git_command(source_repo)
        .args(["worktree", "remove", "--force"])
        .arg(target)
        .output()
        .map_err(|error| CommandError::new("worktree_remove_failed", error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::new(
            "worktree_remove_failed",
            git_output_message(&output),
        ))
    }
}

fn git_output_message(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        stdout
    } else {
        format!("git exited with status {}", output.status)
    }
}

fn git_command(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo);
    #[cfg(target_os = "windows")]
    {
        hide_console(&mut command);
    }
    command
}

fn local_fork_worktree_path(source_repo: &Path, session_id: &str) -> Result<PathBuf, CommandError> {
    let home = crate::runtime_paths::user_home_dir().ok_or_else(|| {
        CommandError::new("worktree_home_unavailable", "home directory unavailable")
    })?;
    Ok(home
        .join(".tinto")
        .join("worktrees")
        .join(path_hash(source_repo))
        .join(format!(
            "fork-{}-{}",
            short_session_id(session_id),
            now_ms()
        )))
}

fn path_hash(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn short_session_id(session_id: &str) -> String {
    let short = session_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>();
    if short.is_empty() {
        "session".to_string()
    } else {
        short
    }
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
    if let Some(frame) = parse_timeline_frame(chunk) {
        emit_timeline_text(app, session_id, frame.kind, Some(frame.text), timestamp_ms);
        return;
    }
    let payload = AgentSessionOutput {
        session_id: session_id.to_string(),
        chunk_base64: STANDARD.encode(chunk),
        timestamp_ms,
    };
    let _ = app.emit(EVENT_AGENT_SESSION_OUTPUT, payload);
    emit_timeline_text(
        app,
        session_id,
        AgentSessionTimelineKind::AgentMessage,
        timeline_text_from_output(chunk),
        timestamp_ms,
    );
}

fn parse_timeline_frame(chunk: &[u8]) -> Option<TimelineFrame> {
    let payload = chunk.strip_prefix(TIMELINE_FRAME_PREFIX)?;
    let payload = payload.strip_suffix(b"\n").unwrap_or(payload);
    serde_json::from_slice(payload).ok()
}

fn emit_timeline_text(
    app: &AppHandle,
    session_id: &str,
    kind: AgentSessionTimelineKind,
    text: Option<String>,
    timestamp_ms: u64,
) {
    emit_timeline_item(app, session_id, kind, text, timestamp_ms, Vec::new());
}

fn emit_timeline_item(
    app: &AppHandle,
    session_id: &str,
    kind: AgentSessionTimelineKind,
    text: Option<String>,
    timestamp_ms: u64,
    attachments: Vec<AgentSessionAttachment>,
) {
    let Some(text) = text else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let payload = AgentSessionTimelineItem {
        session_id: session_id.to_string(),
        id: format!(
            "{}:{}:{}",
            session_id,
            timestamp_ms,
            TIMELINE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
        kind,
        text,
        timestamp_ms,
        attachments,
    };
    record_timeline_item(app, payload.clone());
    let _ = app.emit(EVENT_AGENT_SESSION_TIMELINE, payload);
}

fn record_timeline_item(app: &AppHandle, item: AgentSessionTimelineItem) {
    let mut session_snapshot = None;
    if let Some(registry) = app.try_state::<Mutex<AgentSessionRegistry>>() {
        if let Ok(mut registry) = registry.lock() {
            let _ = registry.record_session_timeline_item(item.clone());
            session_snapshot = registry.get_session(&item.session_id);
        }
    }
    if let Some(journal) = app.try_state::<Mutex<AgentJournal>>() {
        if let Ok(journal) = journal.lock() {
            if let Some(session) = session_snapshot.as_ref() {
                let _ = journal.record_session(session);
            }
            let _ = journal.record_timeline_item(&item);
        }
    }
}

fn timeline_text_from_input(input: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(input)
        .replace('\r', "\n")
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn timeline_text_from_output(output: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(output)
        .replace('\r', "\n")
        .trim()
        .to_string();
    if text.is_empty() || text == ">" {
        None
    } else {
        Some(strip_ansi(&text))
    }
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
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
    persist_session_snapshots(app, sessions);
    let _ = app.emit(EVENT_AGENT_SESSIONS_CHANGED, sessions);
}

fn persist_session_snapshots(app: &AppHandle, sessions: &[AgentSession]) {
    let Some(journal) = app.try_state::<Mutex<AgentJournal>>() else {
        return;
    };
    let Ok(journal) = journal.lock() else {
        return;
    };
    for session in sessions {
        let _ = journal.record_session(session);
    }
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
    fn branch_context_stops_before_the_selected_user_message() {
        let item =
            |id: &str, kind: AgentSessionTimelineKind, text: &str| AgentSessionTimelineItem {
                session_id: "session-1".to_string(),
                id: id.to_string(),
                kind,
                text: text.to_string(),
                timestamp_ms: 1,
                attachments: Vec::new(),
            };
        let timeline = vec![
            item("user-1", AgentSessionTimelineKind::UserMessage, "Primero"),
            item(
                "agent-1",
                AgentSessionTimelineKind::AgentMessage,
                "Respuesta",
            ),
            item("user-2", AgentSessionTimelineKind::UserMessage, "Segundo"),
        ];

        let context = timeline_before_user_message(&timeline, "user-2").expect("context");

        assert_eq!(context.len(), 2);
        assert_eq!(context[0].id, "user-1");
        assert_eq!(context[1].id, "agent-1");
        assert_eq!(
            timeline_before_user_message(&timeline, "missing")
                .unwrap_err()
                .category,
            "turn_not_found"
        );
    }

    #[test]
    fn goal_command_normalizes_text_and_supports_explicit_clear_aliases() {
        assert_eq!(
            parse_goal_command(Some("  Ship\n  the\tfeature  ")).unwrap(),
            GoalCommand::Set("Ship the feature".to_string())
        );
        assert_eq!(parse_goal_command(None).unwrap(), GoalCommand::Inspect);
        assert_eq!(parse_goal_command(Some("edit")).unwrap(), GoalCommand::Edit);
        assert_eq!(
            parse_goal_command(Some("pausar")).unwrap(),
            GoalCommand::Pause
        );
        assert_eq!(
            parse_goal_command(Some("reanudar")).unwrap(),
            GoalCommand::Resume
        );
        assert_eq!(
            parse_goal_command(Some("borrar")).unwrap(),
            GoalCommand::Clear
        );
        assert_eq!(
            parse_goal_command(Some("LIMPIAR")).unwrap(),
            GoalCommand::Clear
        );
        assert_eq!(
            parse_goal_command(Some("Keep the service off")).unwrap(),
            GoalCommand::Set("Keep the service off".to_string())
        );
    }

    #[test]
    fn goal_command_rejects_values_larger_than_codex_allows() {
        let error = parse_goal_command(Some(&"x".repeat(MAX_SESSION_GOAL_CHARS + 1))).unwrap_err();

        assert_eq!(error.category, "goal_too_long");
        assert!(error.message.contains("4001"));
        assert!(error.message.contains("4000"));
    }

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
    fn attachment_validation_accepts_generic_regular_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("brief.pdf");
        std::fs::write(&path, b"%PDF fixture").unwrap();

        let attachment = validate_agent_attachment_path(path.clone()).unwrap();

        assert_eq!(attachment.path, path);
        assert!(!attachment.is_image);
    }

    #[test]
    fn attachment_validation_classifies_supported_images() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("screen.webp");
        std::fs::write(&path, b"image fixture").unwrap();

        let attachment = validate_agent_attachment_path(path).unwrap();

        assert!(attachment.is_image);
    }

    #[test]
    fn timeline_frame_parses_native_command_output() {
        let mut frame = TIMELINE_FRAME_PREFIX.to_vec();
        frame.extend(br#"{"kind":"command_output","text":"cargo test"}"#);
        frame.push(b'\n');

        let parsed = parse_timeline_frame(&frame).unwrap();

        assert_eq!(parsed.kind, AgentSessionTimelineKind::CommandOutput);
        assert_eq!(parsed.text, "cargo test");
    }

    #[test]
    fn ordinary_output_is_not_a_timeline_frame() {
        assert!(parse_timeline_frame(b"plain output").is_none());
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

    #[test]
    fn git_has_head_reports_unborn_repo() {
        let root = tempfile::tempdir().unwrap();
        run_git_test_command(root.path(), &["init"]);

        assert!(!git_has_head(root.path()).unwrap());
    }

    #[test]
    fn create_git_worktree_creates_detached_checkout() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let target = root.path().join("fork");
        std::fs::create_dir_all(&source).unwrap();
        run_git_test_command(&source, &["init"]);
        std::fs::write(source.join("base.txt"), "hello\n").unwrap();
        run_git_test_command(&source, &["add", "base.txt"]);
        run_git_test_command(
            &source,
            &[
                "-c",
                "user.email=tinto@example.invalid",
                "-c",
                "user.name=Tinto",
                "commit",
                "-m",
                "initial",
            ],
        );

        assert!(git_has_head(&source).unwrap());
        create_git_worktree(&source, &target).unwrap();

        let content = std::fs::read_to_string(target.join("base.txt")).unwrap();
        assert_eq!(content.replace("\r\n", "\n"), "hello\n");
        remove_git_worktree(&source, &target).unwrap();
    }

    #[test]
    fn local_git_review_summary_reports_changed_files() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path();
        run_git_test_command(repo, &["init"]);
        std::fs::write(repo.join("base.txt"), "hello\n").unwrap();
        run_git_test_command(repo, &["add", "base.txt"]);
        run_git_test_command(
            repo,
            &[
                "-c",
                "user.email=tinto@example.invalid",
                "-c",
                "user.name=Tinto",
                "commit",
                "-m",
                "initial",
            ],
        );
        std::fs::write(repo.join("base.txt"), "hello\nworld\n").unwrap();
        std::fs::write(repo.join("new.txt"), "new\n").unwrap();

        let (summary, findings) = local_git_review(repo).unwrap();
        let message = git_review_summary_message(&summary);

        assert_eq!(summary.changed_files, 2);
        assert!(findings.is_empty());
        assert!(summary.files.iter().any(|file| file.contains("M base.txt")));
        assert!(summary.files.iter().any(|file| file.contains("?? new.txt")));
        assert!(message.contains("Review summary for branch"));
        assert!(message.contains("2 changed file(s)"));
    }

    #[test]
    fn local_git_review_reports_deterministic_findings() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path();
        run_git_test_command(repo, &["init"]);
        std::fs::write(repo.join("base.txt"), "hello\n").unwrap();
        run_git_test_command(repo, &["add", "base.txt"]);
        run_git_test_command(
            repo,
            &[
                "-c",
                "user.email=tinto@example.invalid",
                "-c",
                "user.name=Tinto",
                "commit",
                "-m",
                "initial",
            ],
        );
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src").join("App.tsx"), "<<<<<<< HEAD\nconflict\n").unwrap();
        std::fs::write(repo.join(".env"), "TOKEN=value\n").unwrap();
        std::fs::write(repo.join("package-lock.json"), "{}\n").unwrap();

        let (_, findings) = local_git_review(repo).unwrap();

        assert!(findings.iter().any(|finding| {
            finding.title == "Conflict marker present"
                && finding.path.as_deref() == Some(Path::new("src/App.tsx"))
                && finding.line == Some(1)
        }));
        assert!(findings.iter().any(|finding| {
            finding.title == "Sensitive path changed"
                && finding.path.as_deref() == Some(Path::new(".env"))
        }));
        assert!(findings.iter().any(|finding| {
            finding.title == "Lockfile changed without package manifest"
                && finding.path.as_deref() == Some(Path::new("package-lock.json"))
        }));
    }

    #[test]
    fn git_review_summary_message_reports_truncated_remote_summary() {
        let message = git_review_summary_message(&agent_review_summary(GitReviewSummary {
            branch: "main".to_string(),
            changed_files: 14,
            working_shortstat: Some("1 file changed, 2 insertions(+)".to_string()),
            staged_shortstat: None,
            files: vec!["M src/a.ts".to_string(), "?? src/b.ts".to_string()],
            truncated_count: 12,
            findings: Vec::new(),
        }));

        assert!(message.contains("Review summary for branch main"));
        assert!(message.contains("14 changed file(s)"));
        assert!(message.contains("no staged line diff"));
        assert!(message.contains("M src/a.ts; ?? src/b.ts; plus 12 more"));
    }

    #[test]
    fn mcp_servers_from_codex_config_lists_servers_without_secret_details() {
        let root = tempfile::tempdir().unwrap();
        let command = root.path().join(if cfg!(target_os = "windows") {
            "server.exe"
        } else {
            "server"
        });
        std::fs::write(&command, "fake").unwrap();
        let raw = format!(
            r#"
[mcp_servers.node_repl]
command = "{}"
args = ["--secret-token", "do-not-render"]

[mcp_servers.remote]
command = "npx"

[mcpServers.node_repl]
command = "duplicate"
"#,
            command.to_string_lossy().replace('\\', "\\\\")
        );

        let servers = mcp_servers_from_codex_config(&raw).unwrap();

        assert_eq!(
            servers,
            vec![
                McpServerSummary {
                    name: "node_repl".to_string(),
                    command_found: Some(true),
                },
                McpServerSummary {
                    name: "remote".to_string(),
                    command_found: None,
                },
            ]
        );
    }

    #[test]
    fn command_availability_reports_missing_absolute_path() {
        let missing = tempfile::tempdir()
            .unwrap()
            .path()
            .join("missing-mcp-server");

        assert_eq!(
            command_availability(missing.to_string_lossy().as_ref()),
            Some(false)
        );
    }

    fn run_git_test_command(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .expect("git command runs");
        assert!(status.success(), "git {args:?} failed with {status}");
    }
}
