use std::path::PathBuf;

use crate::bus::contract::{
    AgentSession, AgentSessionChange, AgentSessionError, AgentSessionStatus,
    AgentSessionTimelineItem, AgentSessionTurnCheckpoint, AgentSessionTurnStatus,
};
use crate::wsl_agent::{
    launcher::request_wsl_agent,
    protocol::{AgentRequest, AgentResponse, PROTOCOL_VERSION},
};

use super::{
    checkpoint::{
        create_checkpoint, revert_checkpoint, revert_checkpoint_file, scan_change_log,
        CheckpointConfig, CheckpointRecord,
    },
    pty::{AgentProcess, AgentProcessEvent},
    AgentConsoleError,
};

const OUTPUT_QUIET_MS: u64 = 2_000;
const FILESYSTEM_QUIET_MS: u64 = 1_500;
const WSL_TURN_SCAN_INTERVAL_MS: u64 = 5_000;
const MAX_TIMELINE_ITEMS_PER_SESSION: usize = 2_000;

pub struct AgentSessionRecord {
    id: String,
    repo: PathBuf,
    agent_type: String,
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
    reverted_at_ms: Option<u64>,
    restored_to_turn_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointBackend {
    Local,
    Wsl,
}

impl AgentSessionRecord {
    pub fn new(
        id: String,
        repo: PathBuf,
        agent_type: String,
        started_at_ms: u64,
        checkpoint: Option<CheckpointRecord>,
        checkpoint_config: CheckpointConfig,
        checkpoint_backend: CheckpointBackend,
    ) -> Self {
        Self {
            id,
            repo,
            agent_type,
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
            reverted_at_ms: None,
            restored_to_turn_index: None,
        }
    }

    pub fn set_wsl_distro(&mut self, distro: String) {
        self.wsl_distro = Some(distro);
    }

    pub fn start(&mut self, process: Box<dyn AgentProcess>) -> Result<(), AgentConsoleError> {
        if self.status != AgentSessionStatus::Starting {
            return Err(AgentConsoleError::new(
                "invalid_session_state",
                "la sesion ya fue iniciada",
            ));
        }
        self.pid = process.pid();
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
            if let Err(error) = process.kill() {
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
        self.error = None;
        self.refresh_turn_checkpoints(now_ms(), true)?;
        self.refresh_change_log()?;
        Ok(())
    }

    pub fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.note_turn_activity(now_ms());
        let process = self.running_process_mut()?;
        process.write_input(input)
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        let process = self.running_process_mut()?;
        process.resize(cols, rows)
    }

    pub fn refresh_status(&mut self) -> Result<(), AgentConsoleError> {
        if self.status != AgentSessionStatus::Running {
            return Ok(());
        }

        if let Some(process) = self.process.as_mut() {
            let events = process.drain_events();
            let exit_code = process.try_exit_code()?;
            for event in events {
                self.apply_process_event(event)?;
            }
            if let Some(exit_code) = exit_code {
                self.exit_code = Some(exit_code);
                self.ended_at_ms = Some(now_ms());
                self.status = status_from_exit_code(self.exit_code);
                self.process = None;
                self.refresh_turn_checkpoints(now_ms(), true)?;
                self.refresh_change_log()?;
            }
        }

        Ok(())
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
        let checkpoint = self.checkpoint.as_ref().ok_or_else(|| {
            AgentConsoleError::new(
                "checkpoint_unsupported",
                "esta sesion no tiene checkpoint reversible",
            )
        })?;
        match self.checkpoint_backend {
            CheckpointBackend::Local => revert_checkpoint(checkpoint)?,
            CheckpointBackend::Wsl => {
                revert_wsl_checkpoint(self.wsl_distro.as_deref(), checkpoint)?
            }
        }
        self.reverted_at_ms = Some(now_ms());
        self.status = AgentSessionStatus::Reverted;
        self.refresh_change_log()?;
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
        match self.checkpoint_backend {
            CheckpointBackend::Local => revert_checkpoint_file(&checkpoint, path)?,
            CheckpointBackend::Wsl => {
                revert_wsl_checkpoint_file(self.wsl_distro.as_deref(), &checkpoint, path)?
            }
        }
        self.refresh_change_log()?;
        Ok(())
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
        match self.checkpoint_backend {
            CheckpointBackend::Local => revert_checkpoint(&checkpoint)?,
            CheckpointBackend::Wsl => {
                revert_wsl_checkpoint(self.wsl_distro.as_deref(), &checkpoint)?
            }
        }
        self.restored_to_turn_index = Some(turn.index);
        self.refresh_change_log()?;
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
        self.refresh_turn_checkpoints(timestamp_ms, true)
    }

    fn refresh_change_log(&mut self) -> Result<(), AgentConsoleError> {
        self.change_log = self.scan_changes(self.checkpoint.as_ref(), now_ms())?;
        Ok(())
    }

    pub fn refresh_turn_checkpoints(
        &mut self,
        now_ms: u64,
        force_close: bool,
    ) -> Result<(), AgentConsoleError> {
        if self.turn_baseline.is_none() {
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
            if force_close
                || self
                    .last_output_at_ms
                    .is_none_or(|last| now_ms.saturating_sub(last) >= OUTPUT_QUIET_MS)
            {
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
        match (checkpoint, self.checkpoint_backend) {
            (Some(checkpoint), CheckpointBackend::Local) => {
                scan_change_log(checkpoint, timestamp_ms)
            }
            (Some(checkpoint), CheckpointBackend::Wsl) => {
                scan_wsl_change_log(self.wsl_distro.as_deref(), checkpoint)
            }
            (None, _) => Ok(Vec::new()),
        }
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

fn status_from_exit_code(exit_code: Option<i32>) -> AgentSessionStatus {
    match exit_code {
        Some(0) | None => AgentSessionStatus::Completed,
        Some(_) => AgentSessionStatus::Failed,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::contract::{AgentSessionCheckpoint, AgentSessionCheckpointType};
    use crate::git::test_fixtures::TempRepo;
    use std::io::Read;

    #[derive(Debug)]
    struct FakeProcess {
        pid: Option<u32>,
        exit_code: Option<i32>,
        status_error: Option<AgentConsoleError>,
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

        fn write_input(&mut self, _input: &[u8]) -> Result<(), AgentConsoleError> {
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
        };
        let record = AgentSessionRecord::new(
            "s1".into(),
            repo.path().to_path_buf(),
            "codex".into(),
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
    fn refresh_marks_completed_process_as_exited() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(FakeProcess {
                pid: Some(42),
                exit_code: Some(17),
                status_error: None,
            }))
            .unwrap();

        session.refresh_status().unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.status, AgentSessionStatus::Failed);
        assert_eq!(contract.exit_code, Some(17));
    }

    #[test]
    fn starting_twice_returns_structured_error() {
        let (_repo, _checkpoint_dir, mut session) = session_record();
        session
            .start(Box::new(FakeProcess {
                pid: Some(1),
                exit_code: None,
                status_error: None,
            }))
            .unwrap();

        let error = session
            .start(Box::new(FakeProcess {
                pid: Some(2),
                exit_code: None,
                status_error: None,
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
            }))
            .unwrap();

        let error = session.stop().unwrap_err();

        let contract = session.to_contract();
        assert_eq!(error.category, "process_status_failed");
        assert_eq!(contract.status, AgentSessionStatus::Error);
        assert_eq!(contract.error.unwrap().category, "process_status_failed");
    }

    #[test]
    fn changed_turn_closes_after_output_and_filesystem_quiet() {
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
    fn output_only_turn_does_not_create_empty_checkpoint() {
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
        assert_eq!(contract.turn_status, AgentSessionTurnStatus::Waiting);
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
    }
}
