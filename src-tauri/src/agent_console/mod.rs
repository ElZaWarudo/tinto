//! Runtime interno para sesiones de agentes en PTY. Los comandos Tauri se
//! conectan en el siguiente review unit; este modulo mantiene el lifecycle
//! testeable sin exponer todavia nueva superficie IPC.

pub mod checkpoint;
pub mod commands;
pub mod pty;
pub mod session;
pub mod validation;

use std::{
    collections::HashMap,
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::bus::contract::{AgentSession, AgentSessionError};
use checkpoint::{create_checkpoint, CheckpointConfig};
use pty::{AgentProcessFactory, PortablePtyFactory};
use session::AgentSessionRecord;
use validation::resolve_agent_binary;

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
}

pub struct StartedAgentSession {
    pub id: String,
    pub output_reader: Option<Box<dyn Read + Send>>,
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
        }
    }

    #[cfg(test)]
    pub fn with_checkpoint_config(mut self, checkpoint_config: CheckpointConfig) -> Self {
        self.checkpoint_config = checkpoint_config;
        self
    }

    pub fn start_session(
        &mut self,
        repo: PathBuf,
        agent_type: String,
    ) -> Result<String, AgentConsoleError> {
        Ok(self.start_session_with_output(repo, agent_type)?.id)
    }

    pub fn start_session_with_output(
        &mut self,
        repo: PathBuf,
        agent_type: String,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        let repo = canonical_repo(&repo)?;
        let binary_path = resolve_agent_binary(&agent_type)?;
        self.start_session_with_binary_and_output(repo, agent_type, binary_path)
    }

    #[cfg(test)]
    fn start_session_with_binary(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        binary_path: PathBuf,
    ) -> Result<String, AgentConsoleError> {
        Ok(self
            .start_session_with_binary_and_output(repo, agent_type, binary_path)?
            .id)
    }

    fn start_session_with_binary_and_output(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        binary_path: PathBuf,
    ) -> Result<StartedAgentSession, AgentConsoleError> {
        let repo = canonical_repo(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let started_at_ms = now_ms();
        let checkpoint = create_checkpoint(&repo, &id, started_at_ms, &self.checkpoint_config)?;
        let mut process = self.process_factory.spawn_agent(&binary_path, &repo)?;
        let output_reader = process.take_output_reader();
        let mut session =
            AgentSessionRecord::new(id.clone(), repo, agent_type, started_at_ms, checkpoint);
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(StartedAgentSession { id, output_reader })
    }

    pub fn stop_session(&mut self, session_id: &str) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.stop()
    }

    pub fn refresh_session_statuses(&mut self) -> Result<(), AgentConsoleError> {
        for session in self.sessions.values_mut() {
            session.refresh_status()?;
        }
        Ok(())
    }

    pub fn write_session_input(
        &mut self,
        session_id: &str,
        input: &[u8],
    ) -> Result<(), AgentConsoleError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| AgentConsoleError::session_not_found(session_id))?;
        session.write_input(input)
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

    pub fn list_sessions(&self) -> Vec<AgentSession> {
        let mut sessions = self
            .sessions
            .values()
            .map(AgentSessionRecord::to_contract)
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
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agent_console::pty::AgentProcess, bus::contract::AgentSessionStatus};
    use std::{
        io::{Cursor, Read},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    #[derive(Default)]
    struct FakeProcessFactory {
        next_pid: AtomicUsize,
        spawned: Mutex<Vec<PathBuf>>,
        writes: Arc<Mutex<Vec<Vec<u8>>>>,
        resizes: Arc<Mutex<Vec<(u16, u16)>>>,
        output: Mutex<Option<Vec<u8>>>,
    }

    impl AgentProcessFactory for FakeProcessFactory {
        fn spawn_agent(
            &self,
            binary_path: &Path,
            _working_dir: &Path,
        ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
            self.spawned.lock().unwrap().push(binary_path.to_path_buf());
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst) as u32 + 100;
            Ok(Box::new(FakeProcess {
                pid,
                exit_code: None,
                killed: false,
                writes: Arc::clone(&self.writes),
                resizes: Arc::clone(&self.resizes),
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
    fn registry_routes_input_and_resize_to_running_process() {
        let factory = Arc::new(FakeProcessFactory::default());
        let mut registry = AgentSessionRegistry::with_process_factory(factory.clone());
        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();

        registry.write_session_input(&id, b"hello\r").unwrap();
        registry.resize_session(&id, 120, 36).unwrap();

        assert_eq!(
            factory.writes.lock().unwrap().as_slice(),
            &[b"hello\r".to_vec()]
        );
        assert_eq!(factory.resizes.lock().unwrap().as_slice(), &[(120, 36)]);
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

        let missing = registry.write_session_input("missing", b"x").unwrap_err();
        assert_eq!(missing.category, "session_not_found");

        let repo = tempfile::tempdir().unwrap();
        let binary = repo.path().join("codex-bin");
        std::fs::write(&binary, "fake").unwrap();
        let id = registry
            .start_session_with_binary(repo.path().into(), "codex".into(), binary)
            .unwrap();
        registry.stop_session(&id).unwrap();

        let stopped = registry.write_session_input(&id, b"x").unwrap_err();
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
            .start_session_with_binary_and_output(repo.path().into(), "codex".into(), binary)
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
}
