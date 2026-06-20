//! Runtime interno para sesiones de agentes en PTY. Los comandos Tauri se
//! conectan en el siguiente review unit; este modulo mantiene el lifecycle
//! testeable sin exponer todavia nueva superficie IPC.

pub mod commands;
pub mod pty;
pub mod session;
pub mod validation;

use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::bus::contract::{AgentSession, AgentSessionError};
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
}

impl AgentSessionRegistry {
    pub fn new() -> Self {
        Self::with_process_factory(Arc::new(PortablePtyFactory))
    }

    pub fn with_process_factory(process_factory: Arc<dyn AgentProcessFactory>) -> Self {
        Self {
            sessions: HashMap::new(),
            process_factory,
        }
    }

    pub fn start_session(
        &mut self,
        repo: PathBuf,
        agent_type: String,
    ) -> Result<String, AgentConsoleError> {
        let repo = canonical_repo(&repo)?;
        let binary_path = resolve_agent_binary(&agent_type)?;
        self.start_session_with_binary(repo, agent_type, binary_path)
    }

    fn start_session_with_binary(
        &mut self,
        repo: PathBuf,
        agent_type: String,
        binary_path: PathBuf,
    ) -> Result<String, AgentConsoleError> {
        let repo = canonical_repo(&repo)?;
        let id = uuid::Uuid::new_v4().to_string();
        let process = self.process_factory.spawn_agent(&binary_path, &repo)?;
        let mut session = AgentSessionRecord::new(id.clone(), repo, agent_type);
        session.start(process)?;
        self.sessions.insert(id.clone(), session);
        Ok(id)
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    #[derive(Default)]
    struct FakeProcessFactory {
        next_pid: AtomicUsize,
        spawned: Mutex<Vec<PathBuf>>,
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
            }))
        }
    }

    #[derive(Debug)]
    struct FakeProcess {
        pid: u32,
        exit_code: Option<i32>,
        killed: bool,
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
        assert_eq!(session.status, AgentSessionStatus::Exited);
        assert_eq!(session.exit_code, Some(0));
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
