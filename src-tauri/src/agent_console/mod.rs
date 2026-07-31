//! Runtime interno para sesiones de agentes en PTY. Los comandos Tauri se
//! conectan en el siguiente review unit; este modulo mantiene el lifecycle
//! testeable sin exponer todavia nueva superficie IPC.

pub mod acp;
pub mod app_server;
pub mod checkpoint;
pub mod commands;
pub mod install;
pub mod journal;
pub mod pty;
pub mod session;
pub mod validation;

use std::{
    collections::HashMap,
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::{mpsc::Receiver, Arc},
};

use crate::bus::contract::{
    AgentRuntimeCatalog, AgentSession, AgentSessionContextSummary, AgentSessionError,
    AgentSessionFeedback, AgentSessionLimits, AgentSessionPermissionMode, AgentSessionResumeMode,
    AgentSessionRuntimeOptions, AgentSessionTimelineItem,
};
use crate::wsl_agent::{
    launcher::request_wsl_agent,
    protocol::{AgentError, AgentRequest, AgentResponse, PROTOCOL_VERSION},
};
use acp::{AcpLaunchIntent, AcpProcessSupervisor, PtyCompatibilityProcess};
use checkpoint::{create_checkpoint, CheckpointConfig, CheckpointRecord};
use pty::{AgentProcess, AgentProcessFactory, AgentTurnAttachment, PortablePtyFactory};
use session::{AgentSessionRecord, CheckpointBackend};
use validation::{resolve_agent_binary, validate_agent_type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConsoleError {
    pub category: String,
    pub message: String,
}

impl AgentConsoleError {
    pub fn new(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            message: message.into(),
        }
    }

    pub fn repo_not_found() -> Self {
        Self::new("repository_not_found", "el repo no existe")
    }

    pub fn session_not_found(session_id: &str) -> Self {
        Self::new(
            "session_not_found",
            format!("no existe la sesion de agente '{session_id}'"),
        )
    }
}

impl fmt::Display for AgentConsoleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.category, self.message)
    }
}

impl std::error::Error for AgentConsoleError {}

impl From<AgentConsoleError> for AgentSessionError {
    fn from(error: AgentConsoleError) -> Self {
        Self {
            category: error.category,
            message: error.message,
        }
    }
}

pub struct AgentSessionRegistry {
    sessions: HashMap<String, AgentSessionRecord>,
    process_factory: Arc<dyn AgentProcessFactory>,
    checkpoint_config: CheckpointConfig,
    limits: AgentSessionLimits,
}

pub(crate) type ResumeResultReceiver = Receiver<Result<AgentSessionResumeMode, AgentConsoleError>>;

fn validate_permission_mode(
    agent_type: &str,
    permission_mode: AgentSessionPermissionMode,
) -> Result<(), AgentConsoleError> {
    if permission_mode == AgentSessionPermissionMode::FullAccess
        && !agent_type.eq_ignore_ascii_case("codex")
    {
        return Err(AgentConsoleError::new(
            "permission_mode_unsupported",
            "el acceso completo solo esta disponible para Codex",
        ));
    }
    Ok(())
}

pub struct StartedAgentSession {
    pub id: String,
    pub output_reader: Option<Box<dyn Read + Send>>,
    pub(crate) resume_result: Option<ResumeResultReceiver>,
}

impl AgentSessionRegistry {
    pub fn new() -> Self {
        Self::with_process_factory(Arc::new(PortablePtyFactory))
    }

    pub fn with_process_factory(process_factory: Arc<dyn AgentProcessFactory>) -> Self {
        Self {
            sessions: HashMap::new(),
            process_factory,
            checkpoint_config: CheckpointConfig::default(),
            limits: default_limits(),
        }
    }

    #[cfg(test)]
    pub fn with_checkpoint_config(mut self, checkpoint_config: CheckpointConfig) -> Self {
        self.checkpoint_config = checkpoint_config;
        self
    }

    #[cfg(test)]
    pub fn with_limits(mut self, limits: AgentSessionLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn start_session(
        &mut self,
        repo: PathBuf,
        agent_type: String,
    ) -> Result<String, AgentConsoleError> {
        Ok(self
            .start_session_with_output(repo, agent_type, AgentSessionPermissionMode::Workspace)?
            .id)
    }

    pub fn start_session_with_output(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        validate_permission_mode(&agent_type, permission_mode)?;
        let repo = canonical_repo(&repo)?;
        let binary_path = resolve_agent_binary(&agent_type)?;
        self.start_session_with_binary_and_output(repo, agent_type, binary_path, permission_mode)
    }

    pub fn resume_session_with_output(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        provider_session_id: String,
        fallback_context: AgentSessionContextSummary,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        validate_permission_mode(&agent_type, permission_mode)?;
        let permission_mode = if agent_type.eq_ignore_ascii_case("codex") {
            permission_mode
        } else {
            AgentSessionPermissionMode::Workspace
        };
        let repo = canonical_repo(&repo)?;
        let binary_path = resolve_agent_binary(&agent_type)?;
        self.refresh_session_statuses()?;
        self.ensure_capacity(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let started_at_ms = now_ms();
        let checkpoint = Some(create_checkpoint(
            &repo,
            &id,
            started_at_ms,
            &self.checkpoint_config,
        )?);
        let acp_provider = matches!(agent_type.as_str(), "kimi" | "opencode");
        let mut process: Box<dyn AgentProcess> = if acp_provider {
            let intent = AcpLaunchIntent::LoadSession {
                provider_session_id: provider_session_id.clone(),
                fallback_context,
            };
            let supervisor = match agent_type.as_str() {
                "kimi" => AcpProcessSupervisor::spawn(
                    binary_path.clone(),
                    repo.clone(),
                    id.clone(),
                    intent,
                )?,
                "opencode" => AcpProcessSupervisor::spawn_opencode(
                    binary_path.clone(),
                    repo.clone(),
                    id.clone(),
                    intent,
                )?,
                _ => unreachable!("ACP provider checked above"),
            };
            Box::new(supervisor)
        } else {
            self.process_factory.resume_agent(
                &binary_path,
                &repo,
                &provider_session_id,
                permission_mode,
            )?
        };
        let resume_result = process.take_resume_result();
        let output_reader = process.take_output_reader();
        let mut session = AgentSessionRecord::new(
            id.clone(),
            repo,
            agent_type,
            permission_mode,
            started_at_ms,
            checkpoint,
            self.checkpoint_config.clone(),
            CheckpointBackend::Local,
        );
        if !acp_provider {
            session.set_provider_session_id(provider_session_id);
        }
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(StartedAgentSession {
            id,
            output_reader,
            resume_result,
        })
    }

    #[cfg(test)]
    fn start_session_with_binary(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        binary_path: PathBuf,
    ) -> Result<String, AgentConsoleError> {
        Ok(self
            .start_session_with_binary_and_output(
                repo,
                agent_type,
                binary_path,
                AgentSessionPermissionMode::Workspace,
            )?
            .id)
    }

    fn start_session_with_binary_and_output(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        binary_path: PathBuf,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        let repo = canonical_repo(&repo)?;
        self.refresh_session_statuses()?;
        self.ensure_capacity(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let started_at_ms = now_ms();
        let checkpoint = Some(create_checkpoint(
            &repo,
            &id,
            started_at_ms,
            &self.checkpoint_config,
        )?);
        let mut process: Box<dyn AgentProcess> = match agent_type.as_str() {
            "kimi" => Box::new(AcpProcessSupervisor::spawn(
                binary_path.clone(),
                repo.clone(),
                id.clone(),
                AcpLaunchIntent::NewSession,
            )?),
            "opencode" => Box::new(AcpProcessSupervisor::spawn_opencode(
                binary_path.clone(),
                repo.clone(),
                id.clone(),
                AcpLaunchIntent::NewSession,
            )?),
            _ => self
                .process_factory
                .spawn_agent(&binary_path, &repo, permission_mode)?,
        };
        let output_reader = process.take_output_reader();
        let mut session = AgentSessionRecord::new(
            id.clone(),
            repo,
            agent_type,
            permission_mode,
            started_at_ms,
            checkpoint,
            self.checkpoint_config.clone(),
            CheckpointBackend::Local,
        );
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(StartedAgentSession {
            id,
            output_reader,
            resume_result: None,
        })
    }

    pub fn start_wsl_session_with_output(
        &mut self,
        repo: PathBuf,
        distro: String,
        agent_type: String,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        self.start_wsl_session_with_output_inner(
            repo,
            distro,
            agent_type,
            permission_mode,
            true,
            true,
        )
    }

    pub fn resume_wsl_session_with_output(
        &mut self,
        repo: PathBuf,
        distro: String,
        agent_type: String,
        provider_session_id: String,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        validate_permission_mode(&agent_type, permission_mode)?;
        let permission_mode = if agent_type.eq_ignore_ascii_case("codex") {
            permission_mode
        } else {
            AgentSessionPermissionMode::Workspace
        };
        validate_wsl_repo(&repo)?;
        validate_agent_type(&agent_type)?;
        ensure_wsl_agent_binary_via_agent(&distro, &agent_type)?;
        self.refresh_session_statuses()?;
        self.ensure_capacity(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let started_at_ms = now_ms();
        let checkpoint = Some(create_wsl_checkpoint(&repo, &distro, &id, started_at_ms)?);
        let mut process = self.process_factory.resume_wsl_agent(
            &agent_type,
            &distro,
            &repo,
            &provider_session_id,
            permission_mode,
        )?;
        let resume_result = process.take_resume_result();
        let output_reader = process.take_output_reader();
        let mut session = AgentSessionRecord::new(
            id.clone(),
            repo,
            agent_type,
            permission_mode,
            started_at_ms,
            checkpoint,
            self.checkpoint_config.clone(),
            CheckpointBackend::Wsl,
        );
        session.set_wsl_distro(distro);
        session.set_provider_session_id(provider_session_id);
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(StartedAgentSession {
            id,
            output_reader,
            resume_result,
        })
    }

    #[cfg(test)]
    fn start_wsl_session_with_output_for_test(
        &mut self,
        repo: PathBuf,
        distro: String,
        agent_type: String,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        self.start_wsl_session_with_output_inner(
            repo,
            distro,
            agent_type,
            AgentSessionPermissionMode::Workspace,
            false,
            false,
        )
    }

    fn start_wsl_session_with_output_inner(
        &mut self,
        repo: PathBuf,
        distro: String,
        agent_type: String,
        permission_mode: AgentSessionPermissionMode,
        check_binary: bool,
        create_remote_checkpoint: bool,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        validate_wsl_repo(&repo)?;
        validate_agent_type(&agent_type)?;
        validate_permission_mode(&agent_type, permission_mode)?;
        if check_binary {
            ensure_wsl_agent_binary_via_agent(&distro, &agent_type)?;
        }
        self.refresh_session_statuses()?;
        self.ensure_capacity(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let started_at_ms = now_ms();
        let checkpoint = if create_remote_checkpoint {
            Some(create_wsl_checkpoint(&repo, &distro, &id, started_at_ms)?)
        } else {
            None
        };
        let process =
            self.process_factory
                .spawn_wsl_agent(&agent_type, &distro, &repo, permission_mode)?;
        let mut process: Box<dyn AgentProcess> = match agent_type.as_str() {
            "kimi" => Box::new(PtyCompatibilityProcess::new(
                process,
                "Kimi ACP en WSL no tiene recolección de grupo de proceso verificada; se usa PTY.",
            )),
            "opencode" => Box::new(PtyCompatibilityProcess::new(
                process,
                "OpenCode ACP en WSL no tiene contención y recolección verificadas; se usa PTY.",
            )),
            _ => process,
        };
        let output_reader = process.take_output_reader();
        let mut session = AgentSessionRecord::new(
            id.clone(),
            repo,
            agent_type,
            permission_mode,
            started_at_ms,
            checkpoint,
            self.checkpoint_config.clone(),
            CheckpointBackend::Wsl,
        );
        session.set_wsl_distro(distro);
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(StartedAgentSession {
            id,
            output_reader,
            resume_result: None,
        })
    }

    pub fn stop_session(&mut self, session_id: &str) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.stop()
    }

    pub fn retry_session_acp(
        &mut self,
        session_id: &str,
        confirmed: bool,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.retry_acp(confirmed)
    }

    pub fn respond_session_acp_permission(
        &mut self,
        session_id: &str,
        permission_id: &str,
        option_id: Option<&str>,
        deny: bool,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.respond_acp_permission(permission_id, option_id, deny)
    }

    pub fn set_session_acp_config_option(
        &mut self,
        session_id: &str,
        config_id: &str,
        value_id: &str,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_acp_config_option(config_id, value_id)
    }

    pub fn refresh_session_statuses(&mut self) -> Result<(), AgentConsoleError> {
        let now = now_ms();
        for session in self.sessions.values_mut() {
            session.refresh_status()?;
            session.refresh_turn_checkpoints(now, false)?;
            session.enforce_lifetime(now, self.limits.max_lifetime_ms)?;
        }
        Ok(())
    }

    pub fn write_session_input(
        &mut self,
        session_id: &str,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.write_input(input, options)
    }

    pub fn write_session_turn(
        &mut self,
        session_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.write_turn(text, attachments, options)
    }

    pub fn steer_session_turn(
        &mut self,
        session_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.steer_turn(text, attachments)
    }

    pub fn interrupt_session_turn(&mut self, session_id: &str) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.interrupt_turn()
    }

    pub fn session_supports_context_compaction(
        &self,
        session_id: &str,
    ) -> Result<bool, AgentConsoleError> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        Ok(session.supports_context_compaction())
    }

    pub fn compact_session_context(&mut self, session_id: &str) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.compact_context()
    }

    pub fn session_runtime_catalog(
        &mut self,
        session_id: &str,
        refresh: bool,
    ) -> Result<Option<AgentRuntimeCatalog>, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        if refresh {
            session.refresh_runtime_catalog()
        } else {
            Ok(session.runtime_catalog())
        }
    }

    pub fn set_session_goal(
        &mut self,
        session_id: &str,
        goal: String,
        updated_at_ms: u64,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_goal(goal, updated_at_ms)?;
        Ok(session.to_contract())
    }

    pub fn clear_session_goal(
        &mut self,
        session_id: &str,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.clear_goal()?;
        Ok(session.to_contract())
    }

    pub fn restore_session_goal(
        &mut self,
        session_id: &str,
        goal: crate::bus::contract::AgentSessionGoal,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.restore_goal(goal)?;
        Ok(session.to_contract())
    }

    pub fn set_session_goal_status(
        &mut self,
        session_id: &str,
        status: crate::bus::contract::AgentSessionGoalStatus,
        updated_at_ms: u64,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_goal_status(status, updated_at_ms)?;
        Ok(session.to_contract())
    }

    pub fn set_session_personality(
        &mut self,
        session_id: &str,
        personality: String,
        updated_at_ms: u64,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_personality(personality, updated_at_ms);
        Ok(session.to_contract())
    }

    pub fn clear_session_personality(
        &mut self,
        session_id: &str,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.clear_personality();
        Ok(session.to_contract())
    }

    pub fn set_session_plan_mode(
        &mut self,
        session_id: &str,
        enabled: bool,
        updated_at_ms: u64,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_plan_mode(enabled, updated_at_ms);
        Ok(session.to_contract())
    }

    pub fn add_session_feedback(
        &mut self,
        session_id: &str,
        feedback: AgentSessionFeedback,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.add_feedback(feedback);
        Ok(session.to_contract())
    }

    pub fn clear_session_feedback(
        &mut self,
        session_id: &str,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.clear_feedback();
        Ok(session.to_contract())
    }

    pub fn set_session_context_summary(
        &mut self,
        session_id: &str,
        summary: AgentSessionContextSummary,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_context_summary(summary);
        Ok(session.to_contract())
    }

    pub fn set_session_runtime_options(
        &mut self,
        session_id: &str,
        options: AgentSessionRuntimeOptions,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_runtime_options(options);
        Ok(session.to_contract())
    }

    pub fn set_session_permission_mode(
        &mut self,
        session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.set_permission_mode(permission_mode)?;
        Ok(session.to_contract())
    }

    pub fn continue_session_turn_sequence_after(
        &mut self,
        session_id: &str,
        completed_turns: u32,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.continue_turn_sequence_after(completed_turns);
        Ok(())
    }

    pub fn clear_session_context_summary(
        &mut self,
        session_id: &str,
    ) -> Result<AgentSession, AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.clear_context_summary();
        Ok(session.to_contract())
    }

    pub fn resize_session(
        &mut self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<(), AgentConsoleError> {
        if cols == 0 || rows == 0 {
            return Err(AgentConsoleError::new(
                "invalid_terminal_size",
                "cols y rows deben ser mayores que cero",
            ));
        }

        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.resize(cols, rows)
    }

    pub fn revert_session(
        &mut self,
        session_id: &str,
        user_consent: bool,
    ) -> Result<AgentSession, AgentConsoleError> {
        if !user_consent {
            return Err(AgentConsoleError::new(
                "consent_required",
                "revert requires explicit user consent",
            ));
        }
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.revert()?;
        Ok(session.to_contract())
    }

    pub fn record_session_output(
        &mut self,
        session_id: &str,
        timestamp_ms: u64,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.record_output_activity(timestamp_ms);
        Ok(())
    }

    pub fn record_session_timeline_item(
        &mut self,
        item: AgentSessionTimelineItem,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(&item.session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(&item.session_id))?;
        session.record_timeline_item(item);
        Ok(())
    }

    pub fn record_session_turn_done(
        &mut self,
        session_id: &str,
        timestamp_ms: u64,
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.record_turn_done(timestamp_ms)
    }

    pub fn revert_turn_file(
        &mut self,
        session_id: &str,
        turn_checkpoint_id: &str,
        path: &Path,
        user_consent: bool,
    ) -> Result<AgentSession, AgentConsoleError> {
        if !user_consent {
            return Err(AgentConsoleError::new(
                "consent_required",
                "revert requires explicit user consent",
            ));
        }
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.revert_turn_file(turn_checkpoint_id, path)?;
        Ok(session.to_contract())
    }

    pub fn restore_turn(
        &mut self,
        session_id: &str,
        turn_checkpoint_id: &str,
        user_consent: bool,
    ) -> Result<AgentSession, AgentConsoleError> {
        if !user_consent {
            return Err(AgentConsoleError::new(
                "consent_required",
                "restore requires explicit user consent",
            ));
        }
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.restore_to_turn(turn_checkpoint_id)?;
        Ok(session.to_contract())
    }

    pub fn list_sessions(&self) -> Vec<AgentSession> {
        let now = now_ms();
        let active = self.active_session_count();
        let mut sessions = self
            .sessions
            .values()
            .map(|session| session.to_contract_with_telemetry(active, now))
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| {
            a.started_at_ms
                .cmp(&b.started_at_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions
    }

    pub fn get_session(&self, session_id: &str) -> Option<AgentSession> {
        self.sessions
            .get(session_id)
            .map(AgentSessionRecord::to_contract)
    }

    pub fn cleanup_all(&mut self) {
        for session in self.sessions.values_mut() {
            let _ = session.stop();
        }
        self.sessions.clear();
    }

    fn ensure_capacity(&self, repo: &Path) -> Result<(), AgentConsoleError> {
        let active_total = self.active_session_count();
        if active_total >= self.limits.max_sessions {
            return Err(AgentConsoleError::new(
                "max_sessions_reached",
                format!(
                    "maximum active sessions reached ({})",
                    self.limits.max_sessions
                ),
            ));
        }
        let active_for_repo = self
            .sessions
            .values()
            .filter(|session| session.is_active() && session.repo() == repo)
            .count();
        if active_for_repo >= self.limits.max_sessions_per_repo {
            return Err(AgentConsoleError::new(
                "max_sessions_per_repo_reached",
                format!(
                    "maximum active sessions per repo reached ({})",
                    self.limits.max_sessions_per_repo
                ),
            ));
        }
        Ok(())
    }

    fn active_session_count(&self) -> usize {
        self.sessions
            .values()
            .filter(|session| session.is_active())
            .count()
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn default_limits() -> AgentSessionLimits {
    AgentSessionLimits {
        max_sessions: 5,
        max_sessions_per_repo: 1,
        max_lifetime_ms: 4 * 60 * 60 * 1000,
    }
}

impl Default for AgentSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn canonical_repo(repo: &Path) -> Result<PathBuf, AgentConsoleError> {
    let repo = repo
        .canonicalize()
        .map_err(|_| AgentConsoleError::repo_not_found())?;
    if repo.is_dir() {
        Ok(repo)
    } else {
        Err(AgentConsoleError::repo_not_found())
    }
}

fn ensure_wsl_agent_binary_via_agent(
    distro: &str,
    agent_type: &str,
) -> Result<(), AgentConsoleError> {
    ensure_wsl_agent_binary_with(distro, agent_type, |distro, request| {
        request_wsl_agent(distro, request)
    })
}

fn ensure_wsl_agent_binary_with<F>(
    distro: &str,
    agent_type: &str,
    requester: F,
) -> Result<(), AgentConsoleError>
where
    F: FnOnce(&str, &AgentRequest) -> Result<AgentResponse, AgentError>,
{
    validate_agent_type(agent_type)?;
    match requester(
        distro,
        &AgentRequest::AgentBinaryAvailable {
            protocol_version: PROTOCOL_VERSION,
            agent_type: agent_type.to_string(),
        },
    ) {
        Ok(AgentResponse::AgentBinaryAvailable { available: true }) => Ok(()),
        Ok(AgentResponse::AgentBinaryAvailable { available: false }) => {
            Err(AgentConsoleError::new(
                "binary_not_found",
                format!(
                    "no se encontro el binario '{agent_type}' en PATH dentro de WSL ({distro}); instala el agente en esa distro o crea un enlace en ~/.local/bin"
                ),
            ))
        }
        Ok(AgentResponse::Error { category, message }) => {
            Err(AgentConsoleError::new(category, message))
        }
        Ok(_) => Err(AgentConsoleError::new(
            "malformed_response",
            "respuesta inesperada del agente WSL",
        )),
        Err(error) => Err(AgentConsoleError::new(error.safe_category(), error.message)),
    }
}

fn validate_wsl_repo(repo: &Path) -> Result<(), AgentConsoleError> {
    let text = repo.to_string_lossy();
    if text.is_empty() || !text.starts_with('/') || text.contains('\\') {
        return Err(AgentConsoleError::repo_not_found());
    }
    if text.split('/').any(|part| part == "." || part == "..") {
        return Err(AgentConsoleError::new(
            "path_traversal",
            "el path WSL no puede contener navegacion",
        ));
    }
    Ok(())
}

fn create_wsl_checkpoint(
    repo: &Path,
    distro: &str,
    session_id: &str,
    created_at_ms: u64,
) -> Result<CheckpointRecord, AgentConsoleError> {
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointCreate {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.to_path_buf(),
            allowed_repos: vec![repo.to_path_buf()],
            session_id: session_id.into(),
            created_at_ms,
            ephemeral: false,
        },
    )
    .map_err(map_wsl_agent_error)?;

    match response {
        AgentResponse::AgentCheckpoint { checkpoint } => Ok(checkpoint),
        AgentResponse::Error { category, message } => {
            Err(AgentConsoleError::new(category, message))
        }
        _ => Err(unexpected_wsl_response()),
    }
}

fn map_wsl_agent_error(error: crate::wsl_agent::protocol::AgentError) -> AgentConsoleError {
    AgentConsoleError::new(error.safe_category(), error.message)
}

fn unexpected_wsl_response() -> AgentConsoleError {
    AgentConsoleError::new("malformed_response", "respuesta inesperada del agente WSL")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agent_console::pty::AgentProcess,
        bus::contract::{AgentSessionLimits, AgentSessionStatus},
    };
    use std::{
        io::{Cursor, Read},
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    #[test]
    fn full_access_is_only_supported_by_codex() {
        assert!(validate_permission_mode("codex", AgentSessionPermissionMode::FullAccess).is_ok());
        let error =
            validate_permission_mode("claude", AgentSessionPermissionMode::FullAccess).unwrap_err();
        assert_eq!(error.category, "permission_mode_unsupported");
    }

    #[derive(Default)]
    struct FakeProcessFactory {
        next_pid: AtomicUsize,
        spawned: Mutex<Vec<PathBuf>>,
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
        resizes: Arc<Mutex<Vec<(u16, u16)>>>,
        output: Mutex<Option<Vec<u8>>>,
        supports_controls: AtomicBool,
        interrupts: Arc<AtomicUsize>,
        compactions: Arc<AtomicUsize>,
    }

    impl AgentProcessFactory for FakeProcessFactory {
        fn spawn_agent(
            &self,
            binary_path: &Path,
            _working_dir: &Path,
            _permission_mode: AgentSessionPermissionMode,
        ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
            self.spawned.lock().unwrap().push(binary_path.to_path_buf());
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst) as u32 + 100;
            Ok(Box::new(FakeProcess {
                pid,
                exit_code: None,
                killed: false,
                writes: Arc::clone(&self.writes),
                resizes: Arc::clone(&self.resizes),
                supports_controls: self.supports_controls.load(Ordering::SeqCst),
                interrupts: Arc::clone(&self.interrupts),
                compactions: Arc::clone(&self.compactions),
                output: self
                    .output
                    .lock()
                    .unwrap()
                    .take()
                    .map(|bytes| Box::new(Cursor::new(bytes)) as Box<dyn Read + Send>),
            }))
        }

        fn spawn_wsl_agent(
            &self,
            agent_type: &str,
            distro: &str,
            working_dir: &Path,
            _permission_mode: AgentSessionPermissionMode,
        ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
            self.spawned.lock().unwrap().push(PathBuf::from(format!(
                "{distro}:{agent_type}:{}",
                working_dir.display()
            )));
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst) as u32 + 100;
            Ok(Box::new(FakeProcess {
                pid,
                exit_code: None,
                killed: false,
                writes: Arc::clone(&self.writes),
                resizes: Arc::clone(&self.resizes),
                supports_controls: self.supports_controls.load(Ordering::SeqCst),
                interrupts: Arc::clone(&self.interrupts),
                compactions: Arc::clone(&self.compactions),
                output: self
                    .output
                    .lock()
                    .unwrap()
                    .take()
                    .map(|bytes| Box::new(Cursor::new(bytes)) as Box<dyn Read + Send>),
            }))
        }
    }

    struct FakeProcess {
        pid: u32,
        exit_code: Option<i32>,
        killed: bool,
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
        resizes: Arc<Mutex<Vec<(u16, u16)>>>,
        supports_controls: bool,
        interrupts: Arc<AtomicUsize>,
        compactions: Arc<AtomicUsize>,
        output: Option<Box<dyn Read + Send>>,
    }

    impl AgentProcess for FakeProcess {
        fn pid(&self) -> Option<u32> {
            Some(self.pid)
        }

        fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
            Ok(self.exit_code)
        }

        fn kill(&mut self) -> Result<(), AgentConsoleError> {
            self.killed = true;
            self.exit_code = Some(0);
            Ok(())
        }

        fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
            self.writes.lock().unwrap().push(input.to_vec());
            Ok(())
        }

        fn supports_turn_interrupt(&self) -> bool {
            self.supports_controls
        }

        fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
            self.interrupts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn supports_context_compaction(&self) -> bool {
            self.supports_controls
        }

        fn compact_context(&mut self) -> Result<(), AgentConsoleError> {
            self.compactions.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
            self.resizes.lock().unwrap().push((cols, rows));
            Ok(())
        }

        fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
            self.output.take()
        }
    }

    #[test]
    fn registry_start_session_records_running_session() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory.clone());
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();

        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary.clone())
            .unwrap();

        let sessions = registry.list_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].agent_type, "codex");
        assert_eq!(sessions[0].status, AgentSessionStatus::Running);
        assert_eq!(sessions[0].pid, Some(100));
        assert_eq!(factory.spawned.lock().unwrap().as_slice(), &[binary]);
    }

    #[test]
    fn registry_stop_session_is_idempotent_at_session_level() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        registry.stop_session(&id).unwrap();
        registry.stop_session(&id).unwrap();

        let session = registry.get_session(&id).unwrap();
        assert_eq!(session.status, AgentSessionStatus::Completed);
        assert_eq!(session.exit_code, Some(0));
    }

    #[test]
    fn registry_revert_requires_user_consent() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();
        registry.stop_session(&id).unwrap();

        let error = registry.revert_session(&id, false).unwrap_err();

        assert_eq!(error.category, "consent_required");
    }

    #[test]
    fn registry_rejects_revert_for_running_session() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        let error = registry.revert_session(&id, true).unwrap_err();

        assert_eq!(error.category, "session_still_running");
    }

    #[test]
    fn registry_enforces_total_session_limit() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry =
            AgentSessionRegistry::with_process_factory(factory).with_limits(AgentSessionLimits {
                max_sessions: 1,
                max_sessions_per_repo: 1,
                max_lifetime_ms: 60_000,
            });
        let repo_a = tempfile::tempdir().unwrap();
        let binary_a = repo_a.path().join("codex-bin");
        std::fs::write(&binary_a, "fake").unwrap();
        registry
            .start_session_with_binary(repo_a.path().into(), "codex".into(), binary_a)
            .unwrap();

        let repo_b = tempfile::tempdir().unwrap();
        let binary_b = repo_b.path().join("codex-bin");
        std::fs::write(&binary_b, "fake").unwrap();
        let error = registry
            .start_session_with_binary(repo_b.path().into(), "codex".into(), binary_b)
            .unwrap_err();

        assert_eq!(error.category, "max_sessions_reached");
    }

    #[test]
    fn registry_enforces_per_repo_session_limit() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry =
            AgentSessionRegistry::with_process_factory(factory).with_limits(AgentSessionLimits {
                max_sessions: 5,
                max_sessions_per_repo: 1,
                max_lifetime_ms: 60_000,
            });
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary.clone())
            .unwrap();

        let error = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap_err();

        assert_eq!(error.category, "max_sessions_per_repo_reached");
    }

    #[test]
    fn registry_lifetime_timeout_marks_failed_and_releases_capacity() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry =
            AgentSessionRegistry::with_process_factory(factory).with_limits(AgentSessionLimits {
                max_sessions: 1,
                max_sessions_per_repo: 1,
                max_lifetime_ms: 1,
            });
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary.clone())
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(5));
        registry.refresh_session_statuses().unwrap();

        let session = registry.get_session(&id).unwrap();
        assert_eq!(session.status, AgentSessionStatus::Failed);
        assert_eq!(
            session.error.as_ref().map(|e| e.category.as_str()),
            Some("session_lifetime_exceeded")
        );

        let repo_b = tempfile::tempdir().unwrap();
        let binary_b = repo_b.path().join("codex-bin");
        std::fs::write(&binary_b, "fake").unwrap();
        registry
            .start_session_with_binary(repo_b.path().into(), "codex".into(), binary_b)
            .unwrap();
    }

    #[test]
    fn registry_routes_input_and_resize_to_running_process() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory.clone());
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        registry.write_session_input(&id, b"hello\r", None).unwrap();
        registry.resize_session(&id, 120, 36).unwrap();

        assert_eq!(
            factory.writes.lock().unwrap().as_slice(),
            &[b"hello\r".to_vec()]
        );
        assert_eq!(factory.resizes.lock().unwrap().as_slice(), &[(120, 36)]);
    }

    #[test]
    fn registry_rejects_permission_change_when_process_cannot_apply_it() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        let error = registry
            .set_session_permission_mode(&id, AgentSessionPermissionMode::FullAccess)
            .unwrap_err();

        assert_eq!(error.category, "permission_mode_unsupported");
        let session = registry.get_session(&id).unwrap();
        assert_eq!(
            session.permission_mode,
            Some(AgentSessionPermissionMode::Workspace)
        );
        assert!(!session.permission_mode_change_supported);
    }

    #[test]
    fn registry_routes_interrupt_and_native_compaction_with_truthful_capability() {
        let factory = Arc::new(FakeProcessFactory::default());
        factory.supports_controls.store(true, Ordering::SeqCst);
        let mut registry = AgentSessionRegistry::with_process_factory(factory.clone());
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        assert!(registry.get_session(&id).unwrap().turn_interrupt_supported);
        assert!(registry.session_supports_context_compaction(&id).unwrap());

        registry.compact_session_context(&id).unwrap();
        registry
            .write_session_input(&id, b"working\r", None)
            .unwrap();
        registry.interrupt_session_turn(&id).unwrap();

        assert_eq!(factory.interrupts.load(Ordering::SeqCst), 1);
        assert_eq!(factory.compactions.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn registry_rejects_invalid_terminal_size() {
        let mut registry =
            AgentSessionRegistry::with_process_factory(Arc::new(FakeProcessFactory::default()));

        let error = registry.resize_session("missing", 0, 24).unwrap_err();

        assert_eq!(error.category, "invalid_terminal_size");
    }

    #[test]
    fn registry_rejects_input_for_missing_or_stopped_session() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);

        let missing = registry
            .write_session_input("missing", b"x", None)
            .unwrap_err();
        assert_eq!(missing.category, "session_not_found");

        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();
        registry.stop_session(&id).unwrap();

        let stopped = registry.write_session_input(&id, b"x", None).unwrap_err();
        assert_eq!(stopped.category, "session_not_running");
    }

    #[test]
    fn registry_start_session_can_return_output_reader() {
        let factory = Arc::new(FakeProcessFactory::default());
        *factory.output.lock().unwrap() = Some(b"ready".to_vec());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();

        let mut started = registry
            .start_session_with_binary_and_output(
                repo.path().into(),
                "codex".into(),
                binary,
                AgentSessionPermissionMode::Workspace,
            )
            .unwrap();
        let mut output = Vec::new();
        started
            .output_reader
            .as_mut()
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();

        assert_eq!(output, b"ready");
        assert!(registry.get_session(&started.id).is_some());
    }

    #[test]
    fn registry_local_start_creates_initial_checkpoint() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("base.txt"), "before\n").unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();

        let started = registry
            .start_session_with_binary_and_output(
                repo.path().into(),
                "codex".into(),
                binary,
                AgentSessionPermissionMode::Workspace,
            )
            .unwrap();

        let session = registry.get_session(&started.id).unwrap();
        assert!(session.checkpoint.is_some());
    }

    #[test]
    fn registry_local_revert_restores_initial_checkpoint() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("base.txt"), "before\n").unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();

        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();
        std::fs::write(repo.path().join("base.txt"), "after\n").unwrap();
        std::fs::write(repo.path().join("created.txt"), "new\n").unwrap();
        registry.stop_session(&id).unwrap();

        let reverted = registry.revert_session(&id, true).unwrap();

        assert_eq!(reverted.status, AgentSessionStatus::Reverted);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "before\n"
        );
        assert!(!repo.path().join("created.txt").exists());
    }

    #[test]
    fn registry_local_turn_checkpoint_can_restore_completed_session_to_post_turn_state() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("base.txt"), "before\n").unwrap();
        let binary = tempfile::NamedTempFile::new().unwrap();

        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary.path().into())
            .unwrap();
        registry
            .record_session_timeline_item(AgentSessionTimelineItem {
                session_id: id.clone(),
                id: "turn-user-1".into(),
                kind: crate::bus::contract::AgentSessionTimelineKind::UserMessage,
                text: "modify the temp repo".into(),
                timestamp_ms: 10,
                attachments: Vec::new(),
            })
            .unwrap();
        std::fs::write(repo.path().join("base.txt"), "after\n").unwrap();
        std::fs::write(repo.path().join("created.txt"), "new\n").unwrap();
        registry.record_session_turn_done(&id, 11).unwrap();
        registry.stop_session(&id).unwrap();

        let completed = registry.get_session(&id).unwrap();
        assert_eq!(completed.status, AgentSessionStatus::Completed);
        assert_eq!(completed.turn_checkpoints.len(), 1);
        let turn = completed.turn_checkpoints[0].clone();
        assert_eq!(turn.index, 1);
        assert!(turn.restore_checkpoint.is_some());
        assert!(turn
            .changes
            .iter()
            .any(|change| change.path.as_path() == Path::new("base.txt")));
        assert!(turn
            .changes
            .iter()
            .any(|change| change.path.as_path() == Path::new("created.txt")));

        std::fs::write(repo.path().join("base.txt"), "later\n").unwrap();
        std::fs::write(repo.path().join("late.txt"), "late\n").unwrap();

        let restored = registry.restore_turn(&id, &turn.id, true).unwrap();

        assert_eq!(restored.restored_to_turn_index, Some(1));
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "after\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("created.txt")).unwrap(),
            "new\n"
        );
        assert!(!repo.path().join("late.txt").exists());
    }

    #[test]
    fn registry_reports_missing_session() {
        let mut registry =
            AgentSessionRegistry::with_process_factory(Arc::new(FakeProcessFactory::default()));

        let error = registry.stop_session("missing").unwrap_err();

        assert_eq!(error.category, "session_not_found");
    }

    #[test]
    fn cleanup_all_stops_and_clears_sessions() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        registry.cleanup_all();

        assert!(registry.list_sessions().is_empty());
    }

    #[test]
    fn registry_starts_wsl_session_without_fake_checkpoint() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory.clone());

        let started = registry
            .start_wsl_session_with_output_for_test(
                PathBuf::from("/home/me/repo"),
                "Ubuntu".into(),
                "codex".into(),
            )
            .unwrap();

        let session = registry.get_session(&started.id).unwrap();
        assert_eq!(session.repo, PathBuf::from("/home/me/repo"));
        assert_eq!(session.checkpoint, None);
        assert_eq!(
            factory.spawned.lock().unwrap().as_slice(),
            &[PathBuf::from("Ubuntu:codex:/home/me/repo")]
        );
    }

    #[test]
    fn wsl_session_binary_check_uses_agent_protocol() {
        ensure_wsl_agent_binary_with("Ubuntu", "codex", |distro, request| {
            assert_eq!(distro, "Ubuntu");
            assert_eq!(
                request,
                &AgentRequest::AgentBinaryAvailable {
                    protocol_version: PROTOCOL_VERSION,
                    agent_type: "codex".into(),
                }
            );
            Ok(AgentResponse::AgentBinaryAvailable { available: true })
        })
        .unwrap();
    }

    #[test]
    fn wsl_session_binary_check_maps_missing_binary() {
        let error = ensure_wsl_agent_binary_with("Ubuntu", "codex", |_, _| {
            Ok(AgentResponse::AgentBinaryAvailable { available: false })
        })
        .unwrap_err();

        assert_eq!(error.category, "binary_not_found");
        assert!(error.message.contains("codex"));
    }

    #[test]
    fn registry_rejects_wsl_revert_without_checkpoint() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory);
        let started = registry
            .start_wsl_session_with_output_for_test(
                PathBuf::from("/home/me/repo"),
                "Ubuntu".into(),
                "codex".into(),
            )
            .unwrap();
        registry.stop_session(&started.id).unwrap();

        let error = registry.revert_session(&started.id, true).unwrap_err();

        assert_eq!(error.category, "checkpoint_unsupported");
    }
}
