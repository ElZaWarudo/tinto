use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::bus::contract::{
    AgentRuntimeCatalog, AgentSession, AgentSessionAcpPermission, AgentSessionAcpRuntime,
    AgentSessionChange, AgentSessionContextSummary, AgentSessionContextUsage, AgentSessionError,
    AgentSessionFeedback, AgentSessionGoal, AgentSessionGoalStatus, AgentSessionPermissionMode,
    AgentSessionPersonality, AgentSessionPlanMode, AgentSessionRuntimeOptions, AgentSessionStatus,
    AgentSessionTimelineItem, AgentSessionTurnCheckpoint, AgentSessionTurnStatus,
};
use crate::wsl_agent::{
    launcher::{request_wsl_agent, windows_path_to_wsl_mount},
    protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION},
};

use super::{
    checkpoint::{
        create_checkpoint, create_ephemeral_checkpoint, remove_ephemeral_checkpoint,
        revert_checkpoint, revert_checkpoint_file, scan_change_log, CheckpointConfig,
        CheckpointRecord,
    },
    pty::{AgentProcess, AgentProcessEvent, AgentTurnAttachment},
    AgentConsoleError,
};

const OUTPUT_QUIET_MS: u64 = 2_000;
const FILESYSTEM_QUIET_MS: u64 = 1_500;
const WSL_TURN_SCAN_INTERVAL_MS: u64 = 5_000;
const MAX_TIMELINE_ITEMS_PER_SESSION: usize = 2_000;
static SAFETY_CHECKPOINT_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct AgentSessionRecord {
    id: String,
    repo: PathBuf,
    agent_type: String,
    permission_mode: AgentSessionPermissionMode,
    provider_session_id: Option<String>,
    acp_runtime: Option<AgentSessionAcpRuntime>,
    acp_permissions: Vec<AgentSessionAcpPermission>,
    status: AgentSessionStatus,
    pid: Option<u32>,
    started_at_ms: u64,
    ended_at_ms: Option<u64>,
    exit_code: Option<i32>,
    error: Option<AgentConsoleError>,
    process: Option<Box<dyn AgentProcess>>,
    checkpoint: Option<CheckpointRecord>,
    checkpoint_config: CheckpointConfig,
    checkpoint_backend: CheckpointBackend,
    wsl_distro: Option<String>,
    change_log: Vec<AgentSessionChange>,
    turn_status: AgentSessionTurnStatus,
    turn_checkpoints: Vec<AgentTurnCheckpointRecord>,
    turn_baseline: Option<CheckpointRecord>,
    turn_started_at_ms: Option<u64>,
    last_output_at_ms: Option<u64>,
    pending_turn_signature: Option<Vec<(PathBuf, String)>>,
    pending_turn_seen_at_ms: Option<u64>,
    last_turn_scan_at_ms: Option<u64>,
    next_turn_index: u32,
    timeline: Vec<AgentSessionTimelineItem>,
    runtime_options: AgentSessionRuntimeOptions,
    goal: Option<AgentSessionGoal>,
    personality: Option<AgentSessionPersonality>,
    plan_mode: Option<AgentSessionPlanMode>,
    feedback: Vec<AgentSessionFeedback>,
    context_summary: Option<AgentSessionContextSummary>,
    context_usage: Option<AgentSessionContextUsage>,
    reverted_at_ms: Option<u64>,
    restored_to_turn_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointBackend {
    Local,
    Wsl,
}

impl AgentSessionRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        repo: PathBuf,
        agent_type: String,
        permission_mode: AgentSessionPermissionMode,
        started_at_ms: u64,
        checkpoint: Option<CheckpointRecord>,
        checkpoint_config: CheckpointConfig,
        checkpoint_backend: CheckpointBackend,
    ) -> Self {
        Self {
            id,
            repo,
            agent_type,
            permission_mode,
            provider_session_id: None,
            acp_runtime: None,
            acp_permissions: Vec::new(),
            status: AgentSessionStatus::Starting,
            pid: None,
            started_at_ms,
            ended_at_ms: None,
            exit_code: None,
            error: None,
            process: None,
            turn_baseline: checkpoint.clone(),
            checkpoint,
            checkpoint_config,
            checkpoint_backend,
            wsl_distro: None,
            change_log: Vec::new(),
            turn_status: AgentSessionTurnStatus::Waiting,
            turn_checkpoints: Vec::new(),
            turn_started_at_ms: None,
            last_output_at_ms: None,
            pending_turn_signature: None,
            pending_turn_seen_at_ms: None,
            last_turn_scan_at_ms: None,
            next_turn_index: 1,
            timeline: Vec::new(),
            runtime_options: AgentSessionRuntimeOptions::default(),
            goal: None,
            personality: None,
            plan_mode: None,
            feedback: Vec::new(),
            context_summary: None,
            context_usage: None,
            reverted_at_ms: None,
            restored_to_turn_index: None,
        }
    }

    pub fn set_wsl_distro(&mut self, distro: String) {
        self.wsl_distro = Some(distro);
    }

    pub fn set_provider_session_id(&mut self, provider_session_id: String) {
        self.provider_session_id = Some(provider_session_id);
    }

    pub fn continue_turn_sequence_after(&mut self, completed_turns: u32) {
        self.next_turn_index = self.next_turn_index.max(completed_turns.saturating_add(1));
    }

    pub fn start(&mut self, process: Box<dyn AgentProcess>) -> Result<(), AgentConsoleError> {
        if self.status != AgentSessionStatus::Starting {
            return Err(AgentConsoleError::new(
                "invalid_session_state",
                "la sesion ya fue iniciada",
            ));
        }
        self.pid = process.pid();
        self.acp_runtime = process.acp_runtime();
        self.acp_permissions = process.acp_permissions();
        self.process = Some(process);
        self.status = AgentSessionStatus::Running;
        self.error = None;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), AgentConsoleError> {
        if self.status == AgentSessionStatus::Exited {
            return Ok(());
        }
        if self.status == AgentSessionStatus::Completed
            || self.status == AgentSessionStatus::Failed
            || self.status == AgentSessionStatus::Reverted
        {
            return Ok(());
        }

        if let Some(process) = self.process.as_mut() {
            if let Some(provider_session_id) = process.provider_session_id() {
                self.provider_session_id = Some(provider_session_id);
            }
            self.acp_runtime = process.acp_runtime();
            self.acp_permissions = process.acp_permissions();
            let kill_result = process.kill();
            if let Some(provider_session_id) = process.provider_session_id() {
                self.provider_session_id = Some(provider_session_id);
            }
            self.acp_runtime = process.acp_runtime();
            self.acp_permissions = process.acp_permissions();
            if let Err(error) = kill_result {
                self.status = AgentSessionStatus::Error;
                self.error = Some(error.clone());
                return Err(error);
            }
            match process.try_exit_code() {
                Ok(exit_code) => {
                    self.exit_code = exit_code;
                }
                Err(error) => {
                    self.status = AgentSessionStatus::Error;
                    self.error = Some(error.clone());
                    return Err(error);
                }
            }
        }

        self.ended_at_ms = Some(now_ms());
        self.status = status_from_exit_code(self.exit_code);
        self.process = None;
        self.error = self.exit_code.and_then(process_exit_error);
        self.refresh_turn_checkpoints(now_ms(), true)?;
        self.refresh_change_log()?;
        Ok(())
    }

    pub fn write_input(
        &mut self,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.error = None;
        let planned_input = plan_mode_input(self.plan_mode.as_ref(), input);
        let input = planned_input.as_deref().unwrap_or(input);
        let native_goal = self
            .process
            .as_ref()
            .is_some_and(|process| process.supports_goals());
        let contextual_input = turn_context_input(
            (!native_goal).then_some(self.goal.as_ref()).flatten(),
            self.personality.as_ref(),
            self.context_summary.as_ref(),
            input,
        );
        let input = contextual_input.as_deref().unwrap_or(input);
        let result = if let Some(options) = options {
            self.runtime_options = options.clone();
            let process = self.running_process_mut()?;
            process.write_input_with_options(input, Some(options))
        } else {
            let process = self.running_process_mut()?;
            process.write_input(input)
        };
        if result.is_ok() {
            self.note_turn_activity(now_ms());
        }
        result
    }

    pub fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.error = None;
        let mut input = text.as_bytes().to_vec();
        input.push(b'\r');
        let planned_input = plan_mode_input(self.plan_mode.as_ref(), &input);
        let input = planned_input.as_deref().unwrap_or(&input);
        let native_goal = self
            .process
            .as_ref()
            .is_some_and(|process| process.supports_goals());
        let contextual_input = turn_context_input(
            (!native_goal).then_some(self.goal.as_ref()).flatten(),
            self.personality.as_ref(),
            self.context_summary.as_ref(),
            input,
        );
        let input = contextual_input.as_deref().unwrap_or(input);
        let text = String::from_utf8_lossy(input)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        let runtime_attachments = if let Some(distro) = self.wsl_distro.as_deref() {
            attachments
                .iter()
                .map(|attachment| {
                    attachment_path_for_wsl(&attachment.path, distro).map(|path| {
                        AgentTurnAttachment {
                            path,
                            is_image: attachment.is_image,
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            attachments.to_vec()
        };
        if let Some(options) = options.as_ref() {
            self.runtime_options = options.clone();
        }
        let process = self.running_process_mut()?;
        let result = process.write_turn(&text, &runtime_attachments, options);
        if result.is_ok() {
            self.note_turn_activity(now_ms());
        }
        result
    }

    pub fn set_permission_mode(
        &mut self,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<(), AgentConsoleError> {
        self.error = None;
        if permission_mode == AgentSessionPermissionMode::FullAccess
            && !self.agent_type.eq_ignore_ascii_case("codex")
        {
            return Err(AgentConsoleError::new(
                "permission_mode_unsupported",
                "el acceso completo solo esta disponible para Codex",
            ));
        }
        if self.status != AgentSessionStatus::Running {
            return Err(AgentConsoleError::new(
                "session_not_running",
                "la sesion no esta ejecutandose",
            ));
        }
        let process = self.process.as_mut().ok_or_else(|| {
            AgentConsoleError::new("session_not_running", "la sesion no esta ejecutandose")
        })?;
        if !process.supports_permission_mode_change() {
            return Err(AgentConsoleError::new(
                "permission_mode_unsupported",
                "este runtime no admite cambiar el acceso por turno",
            ));
        }
        process.set_permission_mode(permission_mode)?;
        self.permission_mode = permission_mode;
        Ok(())
    }

    pub fn steer_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        self.error = None;
        if self.turn_status != AgentSessionTurnStatus::Working {
            return Err(AgentConsoleError::new(
                "steer_unavailable",
                "no hay un turno activo que intervenir",
            ));
        }
        let runtime_attachments = if let Some(distro) = self.wsl_distro.as_deref() {
            attachments
                .iter()
                .map(|attachment| {
                    attachment_path_for_wsl(&attachment.path, distro).map(|path| {
                        AgentTurnAttachment {
                            path,
                            is_image: attachment.is_image,
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            attachments.to_vec()
        };
        self.running_process_mut()?
            .steer_turn(text, &runtime_attachments)
    }

    pub fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        self.error = None;
        if self.turn_status == AgentSessionTurnStatus::Waiting {
            return Err(AgentConsoleError::new(
                "interrupt_unavailable",
                "no hay un turno activo que interrumpir",
            ));
        }
        self.running_process_mut()?.interrupt_turn()
    }

    pub fn supports_context_compaction(&self) -> bool {
        self.status == AgentSessionStatus::Running
            && self
                .process
                .as_ref()
                .is_some_and(|process| process.supports_context_compaction())
    }

    pub fn compact_context(&mut self) -> Result<(), AgentConsoleError> {
        if self.turn_status != AgentSessionTurnStatus::Waiting {
            return Err(AgentConsoleError::new(
                "compact_unavailable",
                "espera a que termine el turno activo antes de compactar",
            ));
        }
        self.running_process_mut()?.compact_context()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        let process = self.running_process_mut()?;
        process.resize(cols, rows)
    }

    pub fn retry_acp(&mut self, confirmed: bool) -> Result<(), AgentConsoleError> {
        let turn_idle = self.turn_status == AgentSessionTurnStatus::Waiting;
        self.running_process_mut()?.retry_acp(confirmed, turn_idle)
    }

    pub fn respond_acp_permission(
        &mut self,
        permission_id: &str,
        option_id: Option<&str>,
        deny: bool,
    ) -> Result<(), AgentConsoleError> {
        self.running_process_mut()?
            .respond_acp_permission(permission_id, option_id, deny)
    }

    pub fn set_acp_config_option(
        &mut self,
        config_id: &str,
        value_id: &str,
    ) -> Result<(), AgentConsoleError> {
        self.running_process_mut()?
            .set_acp_config_option(config_id, value_id)
    }

    pub fn runtime_catalog(&self) -> Option<AgentRuntimeCatalog> {
        self.process
            .as_ref()
            .and_then(|process| process.runtime_catalog())
    }

    pub fn refresh_runtime_catalog(
        &mut self,
    ) -> Result<Option<AgentRuntimeCatalog>, AgentConsoleError> {
        let process = self.running_process_mut()?;
        process.refresh_runtime_catalog()
    }

    pub fn refresh_status(&mut self) -> Result<(), AgentConsoleError> {
        if self.status != AgentSessionStatus::Running {
            return Ok(());
        }

        if let Some(process) = self.process.as_mut() {
            let events = process.drain_events();
            self.pid = process.pid();
            let provider_session_id = process.provider_session_id();
            let acp_runtime = process.acp_runtime();
            let acp_permissions = process.acp_permissions();
            let exit_code = process.try_exit_code()?;
            if provider_session_id.is_some() {
                self.provider_session_id = provider_session_id;
            }
            if acp_runtime.is_some() {
                self.acp_runtime = acp_runtime;
            }
            self.acp_permissions = acp_permissions;
            for event in events {
                self.apply_process_event(event)?;
            }
            if let Some(exit_code) = exit_code {
                self.exit_code = Some(exit_code);
                self.ended_at_ms = Some(now_ms());
                self.status = status_from_exit_code(self.exit_code);
                if self.error.is_none() {
                    self.error = process_exit_error(exit_code);
                }
                self.process = None;
                self.refresh_turn_checkpoints(now_ms(), true)?;
                self.refresh_change_log()?;
            }
        }

        Ok(())
    }

    pub fn set_goal(&mut self, text: String, updated_at_ms: u64) -> Result<(), AgentConsoleError> {
        let process = self.running_process_mut()?;
        process.update_goal(
            Some(text.trim()),
            Some(AgentSessionGoalStatus::Active),
            None,
        )?;
        let previous = self.goal.as_ref();
        self.goal = Some(AgentSessionGoal {
            text: text.trim().to_string(),
            status: AgentSessionGoalStatus::Active,
            token_budget: previous.and_then(|goal| goal.token_budget),
            tokens_used: previous.map_or(0, |goal| goal.tokens_used),
            time_used_seconds: previous.map_or(0, |goal| goal.time_used_seconds),
            created_at_ms: previous.map_or(updated_at_ms, |goal| goal.created_at_ms),
            updated_at_ms,
        });
        Ok(())
    }

    pub fn restore_goal(&mut self, goal: AgentSessionGoal) -> Result<(), AgentConsoleError> {
        let native_goal = self.running_process_mut()?.supports_goals();
        if native_goal {
            self.running_process_mut()?.update_goal(
                Some(&goal.text),
                Some(goal.status),
                Some(goal.token_budget),
            )?;
        }
        self.goal = Some(goal);
        Ok(())
    }

    pub fn set_goal_status(
        &mut self,
        status: AgentSessionGoalStatus,
        updated_at_ms: u64,
    ) -> Result<(), AgentConsoleError> {
        if self.goal.is_none() {
            return Err(AgentConsoleError::new(
                "goal_not_set",
                "esta conversaciÃ³n no tiene un objetivo",
            ));
        }
        self.running_process_mut()?
            .update_goal(None, Some(status), None)?;
        if let Some(goal) = self.goal.as_mut() {
            goal.status = status;
            goal.updated_at_ms = updated_at_ms;
        }
        Ok(())
    }

    pub fn clear_goal(&mut self) -> Result<(), AgentConsoleError> {
        self.running_process_mut()?.clear_goal()?;
        self.goal = None;
        Ok(())
    }

    pub fn set_personality(&mut self, name: String, updated_at_ms: u64) {
        self.personality = Some(AgentSessionPersonality {
            name,
            updated_at_ms,
        });
    }

    pub fn clear_personality(&mut self) {
        self.personality = None;
    }

    pub fn set_plan_mode(&mut self, enabled: bool, updated_at_ms: u64) {
        self.plan_mode = Some(AgentSessionPlanMode {
            enabled,
            updated_at_ms,
        });
    }

    pub fn add_feedback(&mut self, feedback: AgentSessionFeedback) {
        self.feedback.push(feedback);
    }

    pub fn clear_feedback(&mut self) {
        self.feedback.clear();
    }

    pub fn set_context_summary(&mut self, summary: AgentSessionContextSummary) {
        self.context_summary = Some(summary);
    }

    pub fn set_runtime_options(&mut self, options: AgentSessionRuntimeOptions) {
        self.runtime_options = options;
    }

    pub fn clear_context_summary(&mut self) {
        self.context_summary = None;
    }

    fn apply_process_event(&mut self, event: AgentProcessEvent) -> Result<(), AgentConsoleError> {
        match event {
            AgentProcessEvent::FileActivity { timestamp_ms } => {
                self.note_turn_activity(timestamp_ms);
                Ok(())
            }
            AgentProcessEvent::TurnCompleted { timestamp_ms } => {
                self.record_turn_done(timestamp_ms)
            }
            AgentProcessEvent::Error { error } => {
                self.error = Some(error);
                self.turn_started_at_ms = None;
                self.turn_status = AgentSessionTurnStatus::Waiting;
                Ok(())
            }
            AgentProcessEvent::GoalUpdated { goal } => {
                self.goal = Some(goal);
                Ok(())
            }
            AgentProcessEvent::GoalCleared => {
                self.goal = None;
                Ok(())
            }
            AgentProcessEvent::ResumeContextRequired { summary } => {
                self.context_summary = Some(summary);
                Ok(())
            }
            AgentProcessEvent::ContextUsageUpdated {
                used_tokens,
                model_context_window,
            } => {
                self.context_usage = Some(AgentSessionContextUsage {
                    used_tokens,
                    model_context_window,
                });
                Ok(())
            }
        }
    }

    pub fn enforce_lifetime(
        &mut self,
        now_ms: u64,
        max_lifetime_ms: u64,
    ) -> Result<(), AgentConsoleError> {
        if !self.is_active() || max_lifetime_ms == 0 {
            return Ok(());
        }
        if now_ms.saturating_sub(self.started_at_ms) <= max_lifetime_ms {
            return Ok(());
        }
        let _ = self.stop();
        self.status = AgentSessionStatus::Failed;
        self.error = Some(AgentConsoleError::new(
            "session_lifetime_exceeded",
            "session exceeded the configured lifetime limit",
        ));
        self.ended_at_ms = Some(now_ms);
        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), AgentConsoleError> {
        self.refresh_status()?;
        if self.status == AgentSessionStatus::Running || self.status == AgentSessionStatus::Starting
        {
            return Err(AgentConsoleError::new(
                "session_still_running",
                "stop the session before reverting it",
            ));
        }
        if self.status == AgentSessionStatus::Reverted {
            return Ok(());
        }
        self.refresh_change_log()?;
        let checkpoint = self.checkpoint.clone().ok_or_else(|| {
            AgentConsoleError::new(
                "checkpoint_unsupported",
                "esta sesion no tiene checkpoint reversible",
            )
        })?;
        self.apply_checkpoint_transaction(CheckpointMutation::Full(checkpoint))?;
        self.reverted_at_ms = Some(now_ms());
        self.status = AgentSessionStatus::Reverted;
        Ok(())
    }

    pub fn revert_turn_file(
        &mut self,
        turn_checkpoint_id: &str,
        path: &std::path::Path,
    ) -> Result<(), AgentConsoleError> {
        self.refresh_status()?;
        if self.status == AgentSessionStatus::Running || self.status == AgentSessionStatus::Starting
        {
            return Err(AgentConsoleError::new(
                "session_still_running",
                "stop the session before reverting files from a turn checkpoint",
            ));
        }
        let checkpoint = self
            .turn_checkpoints
            .iter()
            .find(|turn| turn.id == turn_checkpoint_id)
            .map(|turn| turn.checkpoint.clone())
            .ok_or_else(|| {
                AgentConsoleError::new(
                    "turn_checkpoint_not_found",
                    "no existe el checkpoint del turno",
                )
            })?;
        self.apply_checkpoint_transaction(CheckpointMutation::File(checkpoint, path.to_path_buf()))
    }

    pub fn restore_to_turn(&mut self, turn_checkpoint_id: &str) -> Result<(), AgentConsoleError> {
        self.refresh_status()?;
        if self.status == AgentSessionStatus::Running || self.status == AgentSessionStatus::Starting
        {
            return Err(AgentConsoleError::new(
                "session_still_running",
                "stop the session before restoring a turn checkpoint",
            ));
        }
        let turn = self
            .turn_checkpoints
            .iter()
            .find(|turn| turn.id == turn_checkpoint_id)
            .ok_or_else(|| {
                AgentConsoleError::new(
                    "turn_checkpoint_not_found",
                    "no existe el checkpoint del turno",
                )
            })?;
        let checkpoint = turn.restore_checkpoint.clone().ok_or_else(|| {
            AgentConsoleError::new(
                "turn_restore_checkpoint_not_found",
                "no existe el checkpoint posterior del turno",
            )
        })?;
        let turn_index = turn.index;
        self.apply_checkpoint_transaction(CheckpointMutation::Full(checkpoint))?;
        self.restored_to_turn_index = Some(turn_index);
        Ok(())
    }

    pub fn record_output_activity(&mut self, timestamp_ms: u64) {
        self.last_output_at_ms = Some(timestamp_ms);
        self.note_turn_activity(timestamp_ms);
    }

    pub fn record_timeline_item(&mut self, item: AgentSessionTimelineItem) {
        if self.timeline.iter().any(|existing| existing.id == item.id) {
            return;
        }
        self.timeline.push(item);
        if self.timeline.len() > MAX_TIMELINE_ITEMS_PER_SESSION {
            let overflow = self.timeline.len() - MAX_TIMELINE_ITEMS_PER_SESSION;
            self.timeline.drain(0..overflow);
        }
    }

    pub fn record_turn_done(&mut self, timestamp_ms: u64) -> Result<(), AgentConsoleError> {
        if self.turn_started_at_ms.is_none() && self.turn_status == AgentSessionTurnStatus::Waiting
        {
            return Ok(());
        }
        if let Err(error) = self.refresh_turn_checkpoints(timestamp_ms, true) {
            self.pending_turn_signature = None;
            self.pending_turn_seen_at_ms = None;
            self.turn_started_at_ms = None;
            self.turn_status = AgentSessionTurnStatus::Waiting;
            self.error = Some(error);
        }
        Ok(())
    }

    fn refresh_change_log(&mut self) -> Result<(), AgentConsoleError> {
        self.change_log = self.scan_changes(self.checkpoint.as_ref(), now_ms())?;
        Ok(())
    }

    fn apply_checkpoint_transaction(
        &mut self,
        mutation: CheckpointMutation,
    ) -> Result<(), AgentConsoleError> {
        let backend = self.checkpoint_backend;
        let distro = self.wsl_distro.clone();
        let baseline = self.checkpoint.clone();
        let safety_id = format!(
            "{}-safety-{}-{}",
            self.id,
            now_ms(),
            SAFETY_CHECKPOINT_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let changes = execute_backend_checkpoint_transaction(
            backend,
            distro.as_deref(),
            &self.repo,
            &safety_id,
            &self.checkpoint_config,
            || apply_checkpoint_mutation(backend, distro.as_deref(), &mutation),
            || scan_checkpoint_changes(backend, distro.as_deref(), baseline.as_ref(), now_ms()),
        )?;
        self.change_log = changes;
        Ok(())
    }

    pub fn refresh_turn_checkpoints(
        &mut self,
        now_ms: u64,
        force_close: bool,
    ) -> Result<(), AgentConsoleError> {
        if self.turn_baseline.is_none() {
            if force_close && self.turn_started_at_ms.is_some() {
                self.next_turn_index = self.next_turn_index.saturating_add(1);
                self.turn_started_at_ms = None;
            }
            self.turn_status = AgentSessionTurnStatus::Waiting;
            return Ok(());
        }
        if !force_close
            && self
                .last_output_at_ms
                .is_some_and(|last| now_ms.saturating_sub(last) < OUTPUT_QUIET_MS)
        {
            self.turn_status = AgentSessionTurnStatus::Working;
            return Ok(());
        }

        if !force_close
            && self.checkpoint_backend == CheckpointBackend::Wsl
            && self
                .last_turn_scan_at_ms
                .is_some_and(|last| now_ms.saturating_sub(last) < WSL_TURN_SCAN_INTERVAL_MS)
        {
            return Ok(());
        }

        let changes = self.scan_changes(self.turn_baseline.as_ref(), now_ms)?;
        self.last_turn_scan_at_ms = Some(now_ms);
        if changes.is_empty() {
            self.pending_turn_signature = None;
            self.pending_turn_seen_at_ms = None;
            if force_close {
                if self.turn_started_at_ms.is_some() {
                    self.next_turn_index = self.next_turn_index.saturating_add(1);
                }
                self.turn_started_at_ms = None;
                self.turn_status = AgentSessionTurnStatus::Waiting;
            }
            return Ok(());
        }

        let signature = change_signature(&changes);
        if self.pending_turn_signature.as_ref() != Some(&signature) {
            self.pending_turn_signature = Some(signature);
            self.pending_turn_seen_at_ms = Some(now_ms);
            if !force_close {
                self.note_turn_activity(now_ms);
                self.turn_status = AgentSessionTurnStatus::Settling;
                return Ok(());
            }
        }

        if !force_close
            && self
                .pending_turn_seen_at_ms
                .is_some_and(|seen| now_ms.saturating_sub(seen) < FILESYSTEM_QUIET_MS)
        {
            self.turn_status = AgentSessionTurnStatus::Settling;
            return Ok(());
        }

        if !force_close {
            self.turn_status = AgentSessionTurnStatus::Settling;
            return Ok(());
        }

        let Some(checkpoint) = self.turn_baseline.clone() else {
            return Ok(());
        };
        let index = self.next_turn_index;
        let restore_checkpoint = self.create_followup_checkpoint(index, now_ms)?;
        self.turn_checkpoints.push(AgentTurnCheckpointRecord {
            id: format!("{}:turn-{index}", self.id),
            index,
            started_at_ms: self.turn_started_at_ms.unwrap_or(now_ms),
            ended_at_ms: now_ms,
            checkpoint,
            restore_checkpoint: Some(restore_checkpoint.clone()),
            changes,
        });
        self.next_turn_index = self.next_turn_index.saturating_add(1);
        self.turn_baseline = Some(restore_checkpoint);
        self.pending_turn_signature = None;
        self.pending_turn_seen_at_ms = None;
        self.turn_started_at_ms = None;
        self.turn_status = AgentSessionTurnStatus::Waiting;
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            AgentSessionStatus::Starting | AgentSessionStatus::Running
        )
    }

    pub fn repo(&self) -> &std::path::Path {
        &self.repo
    }

    fn running_process_mut(&mut self) -> Result<&mut Box<dyn AgentProcess>, AgentConsoleError> {
        if self.status != AgentSessionStatus::Running {
            return Err(AgentConsoleError::new(
                "session_not_running",
                "la sesion no esta ejecutandose",
            ));
        }
        self.process.as_mut().ok_or_else(|| {
            AgentConsoleError::new("session_not_running", "la sesion no esta ejecutandose")
        })
    }

    pub fn to_contract(&self) -> AgentSession {
        self.to_contract_with_telemetry(0, now_ms())
    }

    pub fn to_contract_with_telemetry(&self, active_sessions: usize, now_ms: u64) -> AgentSession {
        AgentSession {
            id: self.id.clone(),
            repo: self.repo.clone(),
            agent_type: self.agent_type.clone(),
            permission_mode: self
                .agent_type
                .eq_ignore_ascii_case("codex")
                .then_some(self.permission_mode),
            permission_mode_change_supported: self.status == AgentSessionStatus::Running
                && self
                    .process
                    .as_ref()
                    .is_some_and(|process| process.supports_permission_mode_change()),
            provider_session_id: self.provider_session_id.clone(),
            acp_runtime: self.acp_runtime.clone(),
            acp_permissions: self.acp_permissions.clone(),
            wsl_distro: self.wsl_distro.clone(),
            status: self.status,
            pid: self.pid,
            started_at_ms: self.started_at_ms,
            ended_at_ms: self.ended_at_ms,
            exit_code: self.exit_code,
            error: self.error.clone().map(AgentSessionError::from),
            checkpoint: self
                .checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.contract.clone()),
            change_log: self.change_log.clone(),
            turn_status: self.turn_status,
            turn_checkpoints: self
                .turn_checkpoints
                .iter()
                .map(AgentTurnCheckpointRecord::to_contract)
                .collect(),
            timeline: self.timeline.clone(),
            runtime_options: self.runtime_options.clone(),
            goal: self.goal.clone(),
            personality: self.personality.clone(),
            plan_mode: self.plan_mode.clone(),
            feedback: self.feedback.clone(),
            context_summary: self.context_summary.clone(),
            context_usage: self.context_usage.clone(),
            turn_interrupt_supported: self.status == AgentSessionStatus::Running
                && self
                    .process
                    .as_ref()
                    .is_some_and(|process| process.supports_turn_interrupt()),
            reverted_at_ms: self.reverted_at_ms,
            restored_to_turn_index: self.restored_to_turn_index,
            active_sessions,
            age_ms: now_ms.saturating_sub(self.started_at_ms),
            output_bytes_per_second: None,
        }
    }

    fn note_turn_activity(&mut self, timestamp_ms: u64) {
        if self.turn_started_at_ms.is_none() {
            self.turn_started_at_ms = Some(timestamp_ms);
        }
        self.turn_status = AgentSessionTurnStatus::Working;
    }

    fn scan_changes(
        &self,
        checkpoint: Option<&CheckpointRecord>,
        timestamp_ms: u64,
    ) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
        scan_checkpoint_changes(
            self.checkpoint_backend,
            self.wsl_distro.as_deref(),
            checkpoint,
            timestamp_ms,
        )
    }

    fn create_followup_checkpoint(
        &self,
        index: u32,
        now_ms: u64,
    ) -> Result<CheckpointRecord, AgentConsoleError> {
        let checkpoint_id = format!("{}-turn-{index}-after", self.id);
        match self.checkpoint_backend {
            CheckpointBackend::Local => {
                create_checkpoint(&self.repo, &checkpoint_id, now_ms, &self.checkpoint_config)
            }
            CheckpointBackend::Wsl => create_wsl_checkpoint(
                self.wsl_distro.as_deref(),
                &self.repo,
                &checkpoint_id,
                now_ms,
            ),
        }
    }
}

enum CheckpointMutation {
    Full(CheckpointRecord),
    File(CheckpointRecord, PathBuf),
}

fn apply_checkpoint_mutation(
    backend: CheckpointBackend,
    distro: Option<&str>,
    mutation: &CheckpointMutation,
) -> Result<(), AgentConsoleError> {
    match (backend, mutation) {
        (CheckpointBackend::Local, CheckpointMutation::Full(checkpoint)) => {
            revert_checkpoint(checkpoint)
        }
        (CheckpointBackend::Local, CheckpointMutation::File(checkpoint, path)) => {
            revert_checkpoint_file(checkpoint, path)
        }
        (CheckpointBackend::Wsl, CheckpointMutation::Full(checkpoint)) => {
            revert_wsl_checkpoint(distro, checkpoint)
        }
        (CheckpointBackend::Wsl, CheckpointMutation::File(checkpoint, path)) => {
            revert_wsl_checkpoint_file(distro, checkpoint, path)
        }
    }
}

fn scan_checkpoint_changes(
    backend: CheckpointBackend,
    distro: Option<&str>,
    checkpoint: Option<&CheckpointRecord>,
    timestamp_ms: u64,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    match (checkpoint, backend) {
        (Some(checkpoint), CheckpointBackend::Local) => scan_change_log(checkpoint, timestamp_ms),
        (Some(checkpoint), CheckpointBackend::Wsl) => scan_wsl_change_log(distro, checkpoint),
        (None, _) => Ok(Vec::new()),
    }
}

fn execute_backend_checkpoint_transaction(
    backend: CheckpointBackend,
    distro: Option<&str>,
    repo: &Path,
    safety_id: &str,
    config: &CheckpointConfig,
    apply: impl FnOnce() -> Result<(), AgentConsoleError>,
    refresh: impl FnOnce() -> Result<Vec<AgentSessionChange>, AgentConsoleError>,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    execute_checkpoint_transaction(
        || match backend {
            CheckpointBackend::Local => {
                create_ephemeral_checkpoint(repo, safety_id, now_ms(), config)
            }
            CheckpointBackend::Wsl => {
                create_wsl_ephemeral_checkpoint(distro, repo, safety_id, now_ms())
            }
        },
        apply,
        refresh,
        |safety| match backend {
            CheckpointBackend::Local => revert_checkpoint(safety),
            CheckpointBackend::Wsl => revert_wsl_checkpoint(distro, safety),
        },
        |safety| match backend {
            CheckpointBackend::Local => remove_ephemeral_checkpoint(safety),
            CheckpointBackend::Wsl => remove_wsl_ephemeral_checkpoint(distro, safety),
        },
    )
}

fn execute_checkpoint_transaction<S>(
    create_safety: impl FnOnce() -> Result<S, AgentConsoleError>,
    apply: impl FnOnce() -> Result<(), AgentConsoleError>,
    refresh: impl FnOnce() -> Result<Vec<AgentSessionChange>, AgentConsoleError>,
    rollback: impl FnOnce(&S) -> Result<(), AgentConsoleError>,
    cleanup: impl FnOnce(&S) -> Result<(), AgentConsoleError>,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let safety = create_safety()?;
    match apply().and_then(|()| refresh()) {
        Ok(changes) => {
            let _ = cleanup(&safety);
            Ok(changes)
        }
        Err(operation_error) => match rollback(&safety) {
            Ok(()) => {
                let _ = cleanup(&safety);
                Err(operation_error)
            }
            Err(rollback_error) => Err(AgentConsoleError::new(
                "checkpoint_transaction_partial",
                format!(
                    "checkpoint operation failed ({}: {}); rollback also failed ({}: {}); repository state may be partial",
                    operation_error.category,
                    operation_error.message,
                    rollback_error.category,
                    rollback_error.message
                ),
            )),
        },
    }
}

#[derive(Debug, Clone)]
struct AgentTurnCheckpointRecord {
    id: String,
    index: u32,
    started_at_ms: u64,
    ended_at_ms: u64,
    checkpoint: CheckpointRecord,
    restore_checkpoint: Option<CheckpointRecord>,
    changes: Vec<AgentSessionChange>,
}

impl AgentTurnCheckpointRecord {
    fn to_contract(&self) -> AgentSessionTurnCheckpoint {
        AgentSessionTurnCheckpoint {
            id: self.id.clone(),
            index: self.index,
            started_at_ms: self.started_at_ms,
            ended_at_ms: self.ended_at_ms,
            checkpoint: self.checkpoint.contract.clone(),
            restore_checkpoint: self
                .restore_checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.contract.clone()),
            changes: self.changes.clone(),
        }
    }
}

fn scan_wsl_change_log(
    distro: Option<&str>,
    checkpoint: &CheckpointRecord,
) -> Result<Vec<AgentSessionChange>, AgentConsoleError> {
    let distro =
        distro.ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointScan {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
            timestamp_ms: now_ms(),
        },
    )
    .map_err(map_wsl_agent_error)?;

    match response {
        AgentResponse::AgentChangeLog { changes } => Ok(changes),
        AgentResponse::Error { category, message } => {
            Err(AgentConsoleError::new(category, message))
        }
        _ => Err(unexpected_wsl_response()),
    }
}

fn revert_wsl_checkpoint(
    distro: Option<&str>,
    checkpoint: &CheckpointRecord,
) -> Result<(), AgentConsoleError> {
    let distro =
        distro.ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointRevert {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
        },
    )
    .map_err(map_wsl_agent_error)?;

    match response {
        AgentResponse::Unit => Ok(()),
        AgentResponse::Error { category, message } => {
            Err(AgentConsoleError::new(category, message))
        }
        _ => Err(unexpected_wsl_response()),
    }
}

fn revert_wsl_checkpoint_file(
    distro: Option<&str>,
    checkpoint: &CheckpointRecord,
    path: &std::path::Path,
) -> Result<(), AgentConsoleError> {
    let distro =
        distro.ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointRevertFile {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
            path: path.to_path_buf(),
        },
    )
    .map_err(map_wsl_agent_error)?;

    match response {
        AgentResponse::Unit => Ok(()),
        AgentResponse::Error { category, message } => {
            Err(AgentConsoleError::new(category, message))
        }
        _ => Err(unexpected_wsl_response()),
    }
}

fn create_wsl_checkpoint(
    distro: Option<&str>,
    repo: &std::path::Path,
    session_id: &str,
    created_at_ms: u64,
) -> Result<CheckpointRecord, AgentConsoleError> {
    create_wsl_checkpoint_with_mode(distro, repo, session_id, created_at_ms, false)
}

fn create_wsl_ephemeral_checkpoint(
    distro: Option<&str>,
    repo: &std::path::Path,
    session_id: &str,
    created_at_ms: u64,
) -> Result<CheckpointRecord, AgentConsoleError> {
    create_wsl_checkpoint_with_mode(distro, repo, session_id, created_at_ms, true)
}

fn create_wsl_checkpoint_with_mode(
    distro: Option<&str>,
    repo: &std::path::Path,
    session_id: &str,
    created_at_ms: u64,
    ephemeral: bool,
) -> Result<CheckpointRecord, AgentConsoleError> {
    let distro =
        distro.ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointCreate {
            protocol_version: PROTOCOL_VERSION,
            repo: repo.to_path_buf(),
            allowed_repos: vec![repo.to_path_buf()],
            session_id: session_id.into(),
            created_at_ms,
            ephemeral,
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

fn remove_wsl_ephemeral_checkpoint(
    distro: Option<&str>,
    checkpoint: &CheckpointRecord,
) -> Result<(), AgentConsoleError> {
    let distro =
        distro.ok_or_else(|| AgentConsoleError::new("missing_distro", "repo WSL sin distro"))?;
    let response = request_wsl_agent(
        distro,
        &AgentRequest::AgentCheckpointRemove {
            protocol_version: PROTOCOL_VERSION,
            allowed_repos: vec![checkpoint.repo.clone()],
            checkpoint: checkpoint.clone(),
        },
    )
    .map_err(map_wsl_agent_error)?;

    match response {
        AgentResponse::Unit => Ok(()),
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

fn status_from_exit_code(exit_code: Option<i32>) -> AgentSessionStatus {
    match exit_code {
        Some(0) | None => AgentSessionStatus::Completed,
        Some(_) => AgentSessionStatus::Failed,
    }
}

fn process_exit_error(exit_code: i32) -> Option<AgentConsoleError> {
    (exit_code != 0).then(|| {
        AgentConsoleError::new(
            "agent_process_failed",
            format!(
                "el proceso del agente termino inesperadamente con codigo {exit_code}; revisa la salida de la sesion"
            ),
        )
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn change_signature(changes: &[AgentSessionChange]) -> Vec<(PathBuf, String)> {
    changes
        .iter()
        .map(|change| {
            (
                change.path.clone(),
                match change.kind {
                    crate::bus::contract::AgentSessionChangeKind::Created => "created",
                    crate::bus::contract::AgentSessionChangeKind::Modified => "modified",
                    crate::bus::contract::AgentSessionChangeKind::Removed => "removed",
                }
                .to_string(),
            )
        })
        .collect()
}

const PLAN_MODE_PREFIX: &str = "Tinto plan mode is enabled for this turn: before editing files, first provide a concise implementation plan and wait for the plan to be accepted or revised. User request: ";
const HOST_CONTEXT_HEADER: &str = "Tinto host context for this turn:";
const HOST_CONTEXT_USER_REQUEST: &str = "User request:";
const MAX_HOST_CONTEXT_VALUE_CHARS: usize = 1200;

fn turn_context_input(
    goal: Option<&AgentSessionGoal>,
    personality: Option<&AgentSessionPersonality>,
    context_summary: Option<&AgentSessionContextSummary>,
    input: &[u8],
) -> Option<Vec<u8>> {
    let mut lines = Vec::new();
    if let Some(goal) = goal.and_then(|goal| trimmed_context_value(&goal.text)) {
        lines.push(format!("- Goal: {}", bounded_context_value(goal)));
    }
    if let Some(personality) =
        personality.and_then(|personality| trimmed_context_value(&personality.name))
    {
        lines.push(format!(
            "- Personality: {}",
            bounded_context_value(personality)
        ));
    }
    if let Some(summary) = context_summary.and_then(|summary| trimmed_context_value(&summary.text))
    {
        lines.push(format!(
            "- Compact context: {}",
            bounded_context_value(summary)
        ));
    }
    if lines.is_empty() {
        return None;
    }

    let line_end = input
        .iter()
        .rposition(|byte| *byte == b'\r' || *byte == b'\n')?;
    let (message, suffix) = input.split_at(line_end);
    if message.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(message);
    if text.starts_with(HOST_CONTEXT_HEADER) {
        return None;
    }

    let mut context = String::new();
    context.push_str(HOST_CONTEXT_HEADER);
    context.push('\n');
    context.push_str(&lines.join("\n"));
    context.push_str("\n\n");
    context.push_str(HOST_CONTEXT_USER_REQUEST);
    context.push(' ');

    let mut contextual = Vec::with_capacity(context.len() + input.len());
    contextual.extend_from_slice(context.as_bytes());
    contextual.extend_from_slice(message);
    contextual.extend_from_slice(suffix);
    Some(contextual)
}

fn attachment_path_for_wsl(path: &Path, distro: &str) -> Result<PathBuf, AgentConsoleError> {
    let raw = path.to_string_lossy();
    if raw.starts_with('/') {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = wsl_unc_path(&raw, distro)? {
        return Ok(PathBuf::from(path));
    }
    windows_path_to_wsl_mount(path)
        .map(PathBuf::from)
        .map_err(|error| AgentConsoleError::new(error.category.as_str(), error.message))
}

fn wsl_unc_path(raw: &str, expected_distro: &str) -> Result<Option<String>, AgentConsoleError> {
    let normalized = raw.replace('\\', "/");
    let normalized = normalized
        .strip_prefix("//?/UNC/")
        .map(|path| format!("//{path}"))
        .unwrap_or(normalized);
    let without_host = ["//wsl.localhost/", "//wsl$/"].iter().find_map(|prefix| {
        normalized
            .get(..prefix.len())
            .filter(|value| value.eq_ignore_ascii_case(prefix))
            .map(|_| &normalized[prefix.len()..])
    });
    let Some(without_host) = without_host else {
        return Ok(None);
    };
    let (distro, rest) = without_host.split_once('/').unwrap_or((without_host, ""));
    if !distro.eq_ignore_ascii_case(expected_distro) {
        return Err(AgentConsoleError::new(
            "attachment_wsl_distro_mismatch",
            format!(
                "el archivo pertenece a la distro WSL {distro}, pero la sesion usa {expected_distro}"
            ),
        ));
    }
    Ok(Some(format!("/{}", rest.trim_start_matches('/'))))
}

fn trimmed_context_value(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn bounded_context_value(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX_HOST_CONTEXT_VALUE_CHARS {
        return normalized;
    }
    let mut bounded = normalized
        .chars()
        .take(MAX_HOST_CONTEXT_VALUE_CHARS)
        .collect::<String>();
    bounded.push_str("...");
    bounded
}

fn plan_mode_input(plan_mode: Option<&AgentSessionPlanMode>, input: &[u8]) -> Option<Vec<u8>> {
    if !plan_mode.is_some_and(|mode| mode.enabled) {
        return None;
    }
    let line_end = input
        .iter()
        .rposition(|byte| *byte == b'\r' || *byte == b'\n')?;
    let (message, suffix) = input.split_at(line_end);
    if message.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(message);
    if text.starts_with("Tinto plan mode is enabled for this turn.") {
        return None;
    }
    let mut planned = Vec::with_capacity(PLAN_MODE_PREFIX.len() + input.len());
    planned.extend_from_slice(PLAN_MODE_PREFIX.as_bytes());
    planned.extend_from_slice(message);
    planned.extend_from_slice(suffix);
    Some(planned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::contract::{
        AgentSessionCheckpoint, AgentSessionCheckpointType, AgentSessionTimelineKind,
    };
    use crate::git::test_fixtures::TempRepo;
    use std::io::Read;
    use std::sync::{Arc, Mutex};

    #[test]
    fn attachment_path_for_wsl_translates_windows_drive_paths() {
        assert_eq!(
            attachment_path_for_wsl(&PathBuf::from(r"C:\Users\User\brief.pdf"), "Ubuntu").unwrap(),
            PathBuf::from("/mnt/c/Users/User/brief.pdf")
        );
    }

    #[test]
    fn attachment_path_for_wsl_accepts_matching_wsl_unc_paths() {
        assert_eq!(
            attachment_path_for_wsl(
                &PathBuf::from(r"\\wsl.localhost\Ubuntu\home\tet\brief.pdf"),
                "Ubuntu",
            )
            .unwrap(),
            PathBuf::from("/home/tet/brief.pdf")
        );
    }

    #[test]
    fn attachment_path_for_wsl_rejects_a_different_distro() {
        let error = attachment_path_for_wsl(
            &PathBuf::from(r"\\wsl$\Debian\home\tet\brief.pdf"),
            "Ubuntu",
        )
        .unwrap_err();
        assert_eq!(error.category, "attachment_wsl_distro_mismatch");
    }

    #[derive(Debug)]
    struct FakeProcess {
        pid: Option<u32>,
        exit_code: Option<i32>,
        status_error: Option<AgentConsoleError>,
        writes: Option<Arc<Mutex<Vec<Vec<u8>>>>>,
    }

    impl AgentProcess for FakeProcess {
        fn pid(&self) -> Option<u32> {
            self.pid
        }

        fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
            if let Some(error) = self.status_error.clone() {
                return Err(error);
            }
            Ok(self.exit_code)
        }

        fn kill(&mut self) -> Result<(), AgentConsoleError> {
            self.exit_code = Some(0);
            Ok(())
        }

        fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
            if let Some(writes) = self.writes.as_ref() {
                writes.lock().unwrap().push(input.to_vec());
            }
            Ok(())
        }

        fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
            None
        }

        fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
            Vec::new()
        }
    }

    #[derive(Debug)]
    struct ProjectionClearingProcess {
        killed: bool,
    }

    impl AgentProcess for ProjectionClearingProcess {
        fn pid(&self) -> Option<u32> {
            Some(7)
        }

        fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
            Ok(self.killed.then_some(0))
        }

        fn kill(&mut self) -> Result<(), AgentConsoleError> {
            self.killed = true;
            Ok(())
        }

        fn write_input(&mut self, _input: &[u8]) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
            None
        }

        fn provider_session_id(&self) -> Option<String> {
            Some(
                if self.killed {
                    "provider-after-stop"
                } else {
                    "provider-before-stop"
                }
                .to_string(),
            )
        }

        fn acp_runtime(&self) -> Option<AgentSessionAcpRuntime> {
            Some(
                serde_json::from_value(if self.killed {
                    serde_json::json!({
                        "state": "failed",
                        "detail": "stopped",
                        "retry_available": false
                    })
                } else {
                    serde_json::json!({
                        "state": "acp_ready",
                        "mode": "acp",
                        "retry_available": false
                    })
                })
                .unwrap(),
            )
        }

        fn acp_permissions(&self) -> Vec<AgentSessionAcpPermission> {
            if self.killed {
                return Vec::new();
            }
            vec![serde_json::from_value(serde_json::json!({
                "id": "permission-1",
                "generation": 1,
                "provider_session_id": "provider-before-stop",
                "turn_id": "turn-1",
                "tool_call_id": "tool-1",
                "title": "Run command",
                "options": [],
                "state": "pending",
                "expires_at_ms": 10
            }))
            .unwrap()]
        }
    }

    struct RetryProcess {
        retries: Arc<Mutex<Vec<(bool, bool)>>>,
        stopped: bool,
    }

    impl AgentProcess for RetryProcess {
        fn pid(&self) -> Option<u32> {
            Some(7)
        }

        fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
            Ok(self.stopped.then_some(0))
        }

        fn kill(&mut self) -> Result<(), AgentConsoleError> {
            self.stopped = true;
            Ok(())
        }

        fn write_input(&mut self, _input: &[u8]) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
            None
        }

        fn retry_acp(&mut self, confirmed: bool, turn_idle: bool) -> Result<(), AgentConsoleError> {
            self.retries.lock().unwrap().push((confirmed, turn_idle));
            Ok(())
        }
    }

    struct PermissionProcess {
        result: Option<AgentConsoleError>,
    }

    impl AgentProcess for PermissionProcess {
        fn pid(&self) -> Option<u32> {
            Some(42)
        }

        fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
            Ok(None)
        }

        fn kill(&mut self) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn write_input(&mut self, _input: &[u8]) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
            Ok(())
        }

        fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
            None
        }

        fn supports_permission_mode_change(&self) -> bool {
            true
        }

        fn set_permission_mode(
            &mut self,
            _permission_mode: AgentSessionPermissionMode,
        ) -> Result<(), AgentConsoleError> {
            self.result.take().map_or(Ok(()), Err)
        }
    }

    fn session_record() -> (tempfile::TempDir, tempfile::TempDir, AgentSessionRecord) {
        let repo = tempfile::tempdir().unwrap();
        let checkpoint_dir = tempfile::tempdir().unwrap();
        let checkpoint = CheckpointRecord {
            contract: AgentSessionCheckpoint {
                checkpoint_type: AgentSessionCheckpointType::FsSnapshot,
                git_hash: None,
                snapshot_files: Vec::new(),
            },
            repo: repo.path().to_path_buf(),
            session_id: "s1".into(),
            checkpoint_dir: checkpoint_dir.path().to_path_buf(),
            created_at_ms: 1,
            ephemeral: false,
        };
        let record = AgentSessionRecord::new(
            "s1".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        (repo, checkpoint_dir, record)
    }

    #[test]
    fn session_transitions_from_starting_to_running_to_exited() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        assert_eq!(session.to_contract().status, AgentSessionStatus::Starting);

        session
            .start(Box::new(FakeProcess {
                pid: Some(42),
                exit_code: None,
                status_error: None,
                writes: None,
            }))
            .unwrap();
        let contract = session.to_contract();
        assert_eq!(contract.status, AgentSessionStatus::Running);
        assert_eq!(contract.pid, Some(42));

        session.stop().unwrap();
        let contract = session.to_contract();
        assert_eq!(contract.status, AgentSessionStatus::Completed);
        assert_eq!(contract.exit_code, Some(0));
    }

    #[test]
    fn permission_mode_changes_only_after_process_accepts_it() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(PermissionProcess { result: None }))
            .unwrap();

        assert!(session.to_contract().permission_mode_change_supported);
        session
            .set_permission_mode(AgentSessionPermissionMode::FullAccess)
            .unwrap();
        assert_eq!(
            session.to_contract().permission_mode,
            Some(AgentSessionPermissionMode::FullAccess)
        );
    }

    #[test]
    fn rejected_permission_mode_change_does_not_mutate_session() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(PermissionProcess {
                result: Some(AgentConsoleError::new(
                    "provider_control_failed",
                    "rejected",
                )),
            }))
            .unwrap();

        let error = session
            .set_permission_mode(AgentSessionPermissionMode::FullAccess)
            .unwrap_err();
        assert_eq!(error.category, "provider_control_failed");
        assert_eq!(
            session.to_contract().permission_mode,
            Some(AgentSessionPermissionMode::Workspace)
        );
    }

    #[test]
    fn contract_exposes_permission_mode_only_for_codex_sessions() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        assert_eq!(
            session.to_contract().permission_mode,
            Some(AgentSessionPermissionMode::Workspace)
        );

        session.agent_type = "kimi".to_string();
        assert_eq!(session.to_contract().permission_mode, None);
    }

    #[test]
    fn acp_retry_preserves_the_session_transcript_and_checkpoint() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        let retries = Arc::new(Mutex::new(Vec::new()));
        session
            .start(Box::new(RetryProcess {
                retries: Arc::clone(&retries),
                stopped: false,
            }))
            .unwrap();
        session.record_timeline_item(AgentSessionTimelineItem {
            session_id: "s1".to_owned(),
            id: "timeline-1".to_owned(),
            kind: AgentSessionTimelineKind::Lifecycle,
            text: "PTY transcript".to_owned(),
            timestamp_ms: 2,
            attachments: Vec::new(),
        });
        let before = session.to_contract();

        session.retry_acp(true).unwrap();

        let after = session.to_contract();
        assert_eq!(after.id, before.id);
        assert_eq!(after.timeline, before.timeline);
        assert_eq!(after.checkpoint, before.checkpoint);
        assert_eq!(*retries.lock().unwrap(), vec![(true, true)]);
    }

    #[test]
    fn stop_reloads_acp_projection_after_process_cleanup() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(ProjectionClearingProcess { killed: false }))
            .unwrap();
        let running = session.to_contract();
        assert_eq!(running.acp_permissions.len(), 1);

        session.stop().unwrap();

        let stopped = session.to_contract();
        assert_eq!(
            stopped.provider_session_id.as_deref(),
            Some("provider-after-stop")
        );
        assert_eq!(
            stopped
                .acp_runtime
                .as_ref()
                .and_then(|runtime| runtime.detail.as_deref()),
            Some("stopped")
        );
        assert!(stopped.acp_permissions.is_empty());
    }

    #[test]
    fn refresh_marks_nonzero_exit_as_failed_with_error() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(FakeProcess {
                pid: Some(42),
                exit_code: Some(17),
                status_error: None,
                writes: None,
            }))
            .unwrap();

        session.refresh_status().unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.status, AgentSessionStatus::Failed);
        assert_eq!(contract.exit_code, Some(17));
        let error = contract.error.expect("failed process exposes its error");
        assert_eq!(error.category, "agent_process_failed");
        assert!(error.message.contains("17"));
    }

    #[test]
    fn starting_twice_returns_structured_error() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
                writes: None,
            }))
            .unwrap();

        let error = session
            .start(Box::new(FakeProcess {
                pid: Some(2),
                exit_code: None,
                status_error: None,
                writes: None,
            }))
            .unwrap_err();

        assert_eq!(error.category, "invalid_session_state");
    }

    #[test]
    fn stop_records_error_when_exit_poll_fails() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: Some(AgentConsoleError::new(
                    "process_status_failed",
                    "fallo leyendo estado",
                )),
                writes: None,
            }))
            .unwrap();

        let error = session.stop().unwrap_err();

        let contract = session.to_contract();
        assert_eq!(error.category, "process_status_failed");
        assert_eq!(contract.status, AgentSessionStatus::Error);
        assert_eq!(contract.error.unwrap().category, "process_status_failed");
    }

    #[test]
    fn plan_mode_prefixes_visible_instruction_before_turn_input() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        let writes = Arc::new(Mutex::new(Vec::new()));
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
                writes: Some(writes.clone()),
            }))
            .unwrap();
        session.set_plan_mode(true, 20);

        session
            .write_input(b"implement feature\r", None)
            .expect("write");

        let writes = writes.lock().unwrap();
        let written = String::from_utf8(writes[0].clone()).unwrap();
        assert!(written.starts_with("Tinto plan mode is enabled for this turn:"));
        assert!(written.contains("before editing files"));
        assert!(written.ends_with("implement feature\r"));
    }

    #[test]
    fn restored_goal_uses_host_context_when_process_has_no_native_goals() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        let writes = Arc::new(Mutex::new(Vec::new()));
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
                writes: Some(writes.clone()),
            }))
            .unwrap();
        session
            .restore_goal(AgentSessionGoal {
                text: "Preserve the archived objective".into(),
                status: AgentSessionGoalStatus::Active,
                token_budget: Some(1_000),
                tokens_used: 100,
                time_used_seconds: 10,
                created_at_ms: 1,
                updated_at_ms: 2,
            })
            .unwrap();

        session.write_input(b"continue\r", None).unwrap();

        let writes = writes.lock().unwrap();
        let written = String::from_utf8(writes[0].clone()).unwrap();
        assert!(written.contains("Preserve the archived objective"));
        assert!(written.ends_with("continue\r"));
    }

    #[test]
    fn host_context_prefixes_goal_personality_and_summary_before_turn_input() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        let writes = Arc::new(Mutex::new(Vec::new()));
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
                writes: Some(writes.clone()),
            }))
            .unwrap();
        session.goal = Some(AgentSessionGoal {
            text: "Ship the Codex harness".into(),
            status: AgentSessionGoalStatus::Active,
            token_budget: None,
            tokens_used: 0,
            time_used_seconds: 0,
            created_at_ms: 20,
            updated_at_ms: 20,
        });
        session.set_personality("precise".into(), 21);
        session.set_context_summary(AgentSessionContextSummary {
            text: "Recent work: /review now returns findings.\nNext: steer context.".into(),
            created_at_ms: 22,
            source_events: 3,
            source_turns: 2,
        });

        session.write_input(b"continue\r", None).expect("write");

        let writes = writes.lock().unwrap();
        let written = String::from_utf8(writes[0].clone()).unwrap();
        assert!(written.starts_with("Tinto host context for this turn:\n"));
        assert!(written.contains("- Goal: Ship the Codex harness"));
        assert!(written.contains("- Personality: precise"));
        assert!(written.contains(
            "- Compact context: Recent work: /review now returns findings. Next: steer context."
        ));
        assert!(written.ends_with("User request: continue\r"));
    }

    #[test]
    fn host_context_wraps_plan_mode_instruction_when_both_are_enabled() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        let writes = Arc::new(Mutex::new(Vec::new()));
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
                writes: Some(writes.clone()),
            }))
            .unwrap();
        session.goal = Some(AgentSessionGoal {
            text: "Keep the turn scoped".into(),
            status: AgentSessionGoalStatus::Active,
            token_budget: None,
            tokens_used: 0,
            time_used_seconds: 0,
            created_at_ms: 20,
            updated_at_ms: 20,
        });
        session.set_plan_mode(true, 21);

        session.write_input(b"edit files\r", None).expect("write");

        let writes = writes.lock().unwrap();
        let written = String::from_utf8(writes[0].clone()).unwrap();
        assert!(written.starts_with("Tinto host context for this turn:\n"));
        assert!(written.contains("- Goal: Keep the turn scoped"));
        assert!(written.contains("User request: Tinto plan mode is enabled for this turn:"));
        assert!(written.ends_with("edit files\r"));
    }

    #[test]
    fn changed_turn_closes_only_after_explicit_provider_completion() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "before\n");
        repo.write("other.txt", "other before\n");
        let checkpoint =
            create_checkpoint(repo.path(), "turn-session", 1, &CheckpointConfig::default())
                .unwrap();
        let mut session = AgentSessionRecord::new(
            "turn-session".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        session.status = AgentSessionStatus::Running;
        session.record_output_activity(10);

        repo.write("base.txt", "after\n");
        repo.write("other.txt", "other after\n");
        session
            .refresh_turn_checkpoints(10 + OUTPUT_QUIET_MS + 1, false)
            .unwrap();
        assert!(session.to_contract().turn_checkpoints.is_empty());
        assert_eq!(
            session.to_contract().turn_status,
            AgentSessionTurnStatus::Settling
        );

        session
            .refresh_turn_checkpoints(10 + OUTPUT_QUIET_MS + FILESYSTEM_QUIET_MS + 2, false)
            .unwrap();

        assert!(session.to_contract().turn_checkpoints.is_empty());
        assert_eq!(
            session.to_contract().turn_status,
            AgentSessionTurnStatus::Settling
        );
        session
            .record_turn_done(10 + OUTPUT_QUIET_MS + FILESYSTEM_QUIET_MS + 3)
            .unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.turn_checkpoints.len(), 1);
        assert_eq!(contract.turn_checkpoints[0].index, 1);
        assert!(contract.turn_checkpoints[0].restore_checkpoint.is_some());
        assert!(contract.turn_checkpoints[0]
            .changes
            .iter()
            .any(|change| change.path == std::path::Path::new("base.txt")));

        session.status = AgentSessionStatus::Completed;
        let turn_id = contract.turn_checkpoints[0].id.clone();
        session
            .revert_turn_file(&turn_id, std::path::Path::new("base.txt"))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "before\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("other.txt")).unwrap(),
            "other after\n"
        );

        session.restore_to_turn(&turn_id).unwrap();
        assert_eq!(session.to_contract().restored_to_turn_index, Some(1));
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "after\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("other.txt")).unwrap(),
            "other after\n"
        );
    }

    #[test]
    fn explicit_turn_done_closes_changed_turn_without_quiet_delay() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "before\n");
        let checkpoint = create_checkpoint(
            repo.path(),
            "explicit-turn-session",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        let mut session = AgentSessionRecord::new(
            "explicit-turn-session".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        session.status = AgentSessionStatus::Running;
        session.record_output_activity(10);
        repo.write("base.txt", "after\n");

        session.record_turn_done(11).unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.turn_status, AgentSessionTurnStatus::Waiting);
        assert_eq!(contract.turn_checkpoints.len(), 1);
        assert_eq!(contract.turn_checkpoints[0].ended_at_ms, 11);
        assert!(contract.turn_checkpoints[0]
            .changes
            .iter()
            .any(|change| change.path == std::path::Path::new("base.txt")));
    }

    #[test]
    fn output_only_turn_stays_working_until_provider_completes_it() {
        let repo = TempRepo::with_initial_commit();
        let checkpoint = create_checkpoint(
            repo.path(),
            "empty-turn-session",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        let mut session = AgentSessionRecord::new(
            "empty-turn-session".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        session.record_output_activity(10);

        session
            .refresh_turn_checkpoints(10 + OUTPUT_QUIET_MS + FILESYSTEM_QUIET_MS + 1, false)
            .unwrap();

        let contract = session.to_contract();
        assert!(contract.turn_checkpoints.is_empty());
        assert_eq!(contract.turn_status, AgentSessionTurnStatus::Working);
    }

    #[test]
    fn explicit_turn_done_without_changes_returns_to_waiting() {
        let repo = TempRepo::with_initial_commit();
        let checkpoint = create_checkpoint(
            repo.path(),
            "explicit-empty-turn-session",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        let mut session = AgentSessionRecord::new(
            "explicit-empty-turn-session".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        session.record_output_activity(10);

        session.record_turn_done(11).unwrap();

        let contract = session.to_contract();
        assert!(contract.turn_checkpoints.is_empty());
        assert_eq!(contract.turn_status, AgentSessionTurnStatus::Waiting);

        session.record_output_activity(12);
        repo.write("base.txt", "second turn\n");
        session.record_turn_done(13).unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.turn_checkpoints.len(), 1);
        assert_eq!(contract.turn_checkpoints[0].index, 2);
    }

    #[test]
    fn process_context_usage_is_projected() {
        let (_repo, _checkpoint_dir, mut session) = session_record();

        session
            .apply_process_event(AgentProcessEvent::ContextUsageUpdated {
                used_tokens: 150_000,
                model_context_window: 128_000,
            })
            .unwrap();

        assert_eq!(
            session.to_contract().context_usage,
            Some(AgentSessionContextUsage {
                used_tokens: 150_000,
                model_context_window: 128_000,
            })
        );
        assert!(!session.to_contract().turn_interrupt_supported);
    }

    #[test]
    fn checkpoint_failure_does_not_leave_a_completed_turn_working() {
        let repo = TempRepo::with_initial_commit();
        let checkpoint = create_checkpoint(
            repo.path(),
            "missing-repo-turn-session",
            1,
            &CheckpointConfig::default(),
        )
        .unwrap();
        let mut session = AgentSessionRecord::new(
            "missing-repo-turn-session".into(),
            repo.path().to_path_buf(),
            "codex".into(),
            AgentSessionPermissionMode::Workspace,
            1,
            Some(checkpoint),
            CheckpointConfig::default(),
            CheckpointBackend::Local,
        );
        session.record_output_activity(10);
        std::fs::remove_dir_all(repo.path()).unwrap();

        session.record_turn_done(11).unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.turn_status, AgentSessionTurnStatus::Waiting);
        assert!(contract.error.is_some());
    }

    #[test]
    fn checkpoint_transaction_rolls_back_an_intermediate_local_failure() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "current base\n");
        repo.write("other.txt", "current other\n");
        let config = CheckpointConfig::default();
        let anchor = create_checkpoint(repo.path(), "transaction-anchor", 1, &config).unwrap();
        let safety_id = "transaction-mid-failure";

        let error = execute_backend_checkpoint_transaction(
            CheckpointBackend::Local,
            None,
            repo.path(),
            safety_id,
            &config,
            || {
                repo.write("base.txt", "partially restored\n");
                std::fs::remove_file(repo.path().join("other.txt")).unwrap();
                Err(AgentConsoleError::new(
                    "revert_failed",
                    "injected intermediate failure",
                ))
            },
            || panic!("refresh must not run after apply fails"),
        )
        .unwrap_err();

        assert_eq!(error.category, "revert_failed");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "current base\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("other.txt")).unwrap(),
            "current other\n"
        );
        assert!(!anchor
            .checkpoint_dir
            .parent()
            .unwrap()
            .join(safety_id)
            .exists());
    }

    #[test]
    fn checkpoint_transaction_rolls_back_when_refresh_fails() {
        let repo = TempRepo::with_initial_commit();
        repo.write("base.txt", "current\n");
        let config = CheckpointConfig::default();

        let error = execute_backend_checkpoint_transaction(
            CheckpointBackend::Local,
            None,
            repo.path(),
            "transaction-refresh-failure",
            &config,
            || {
                repo.write("base.txt", "restored target\n");
                Ok(())
            },
            || {
                Err(AgentConsoleError::new(
                    "checkpoint_scan_failed",
                    "injected refresh failure",
                ))
            },
        )
        .unwrap_err();

        assert_eq!(error.category, "checkpoint_scan_failed");
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "current\n"
        );
    }

    #[test]
    fn checkpoint_transaction_declares_partial_state_when_rollback_fails() {
        let error = execute_checkpoint_transaction(
            || Ok(()),
            || Err(AgentConsoleError::new("revert_failed", "target failed")),
            || Ok(Vec::new()),
            |_| Err(AgentConsoleError::new("rollback_failed", "safety failed")),
            |_| Ok(()),
        )
        .unwrap_err();

        assert_eq!(error.category, "checkpoint_transaction_partial");
        assert!(error.message.contains("target failed"));
        assert!(error.message.contains("safety failed"));
        assert!(error.message.contains("state may be partial"));
    }
}
