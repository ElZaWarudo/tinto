use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::bus::contract::{AgentSession, AgentSessionError, AgentSessionStatus};

use super::{pty::AgentProcess, AgentConsoleError};

pub struct AgentSessionRecord {
    id: String,
    repo: PathBuf,
    agent_type: String,
    status: AgentSessionStatus,
    pid: Option<u32>,
    started_at_ms: u64,
    exit_code: Option<i32>,
    error: Option<AgentConsoleError>,
    process: Option<Box<dyn AgentProcess>>,
}

impl AgentSessionRecord {
    pub fn new(id: String, repo: PathBuf, agent_type: String) -> Self {
        Self {
            id,
            repo,
            agent_type,
            status: AgentSessionStatus::Starting,
            pid: None,
            started_at_ms: now_ms(),
            exit_code: None,
            error: None,
            process: None,
        }
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

        self.status = AgentSessionStatus::Exited;
        self.process = None;
        self.error = None;
        Ok(())
    }

    pub fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
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
            if let Some(exit_code) = process.try_exit_code()? {
                self.exit_code = Some(exit_code);
                self.status = AgentSessionStatus::Exited;
                self.process = None;
            }
        }

        Ok(())
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
        AgentSession {
            id: self.id.clone(),
            repo: self.repo.clone(),
            agent_type: self.agent_type.clone(),
            status: self.status,
            pid: self.pid,
            started_at_ms: self.started_at_ms,
            exit_code: self.exit_code,
            error: self.error.clone().map(AgentSessionError::from),
        }
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
    }

    #[test]
    fn session_transitions_from_starting_to_running_to_exited() {
        let mut session = AgentSessionRecord::new("s1".into(), "/repo".into(), "codex".into());
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
        assert_eq!(contract.status, AgentSessionStatus::Exited);
        assert_eq!(contract.exit_code, Some(0));
    }

    #[test]
    fn refresh_marks_completed_process_as_exited() {
        let mut session = AgentSessionRecord::new("s1".into(), "/repo".into(), "codex".into());
        session
            .start(Box::new(FakeProcess {
                pid: Some(42),
                exit_code: Some(17),
                status_error: None,
            }))
            .unwrap();

        session.refresh_status().unwrap();

        let contract = session.to_contract();
        assert_eq!(contract.status, AgentSessionStatus::Exited);
        assert_eq!(contract.exit_code, Some(17));
    }

    #[test]
    fn starting_twice_returns_structured_error() {
        let mut session = AgentSessionRecord::new("s1".into(), "/repo".into(), "codex".into());
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
        let mut session = AgentSessionRecord::new("s1".into(), "/repo".into(), "codex".into());
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
}
