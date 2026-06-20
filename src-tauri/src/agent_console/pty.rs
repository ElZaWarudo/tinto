use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::Path,
    process::Command,
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::AgentConsoleError;

pub trait AgentProcess: Send {
    fn pid(&self) -> Option<u32>;
    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError>;
    fn kill(&mut self) -> Result<(), AgentConsoleError>;
    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError>;
    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>>;
}

pub trait AgentProcessFactory: Send + Sync {
    fn spawn_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError>;
}

#[derive(Debug, Default)]
pub struct PortablePtyFactory;

impl AgentProcessFactory for PortablePtyFactory {
    fn spawn_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        Ok(Box::new(PtyHandle::spawn(binary_path, working_dir)?))
    }
}

pub struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Box<dyn Write + Send>,
}

impl PtyHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize::default())
            .map_err(|e| spawn_error(e.to_string()))?;
        let command = build_agent_command(binary_path, working_dir);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| spawn_error(e.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| spawn_error(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| spawn_error(e.to_string()))?;

        Ok(Self {
            master: pair.master,
            child,
            reader: Some(reader),
            writer,
        })
    }

    fn resize_pty(&self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AgentConsoleError::new("pty_resize_failed", e.to_string()))
    }

    fn write_input_pty(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.writer
            .write_all(input)
            .map_err(|e| AgentConsoleError::new("pty_write_failed", e.to_string()))
    }

    pub fn read_output(&mut self, output: &mut [u8]) -> Result<usize, AgentConsoleError> {
        self.reader
            .as_mut()
            .ok_or_else(|| {
                AgentConsoleError::new("pty_reader_taken", "el lector PTY ya fue movido")
            })?
            .read(output)
            .map_err(|e| AgentConsoleError::new("pty_read_failed", e.to_string()))
    }
}

impl AgentProcess for PtyHandle {
    fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        self.child
            .try_wait()
            .map(|status| status.map(|s| s.exit_code() as i32))
            .map_err(|e| AgentConsoleError::new("process_status_failed", e.to_string()))
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        if let Some(pid) = self.child.process_id() {
            if kill_process_tree(pid).is_ok() {
                let _ = self.child.kill();
                return Ok(());
            }
        }

        self.child
            .kill()
            .map_err(|e| AgentConsoleError::new("process_kill_failed", e.to_string()))
    }

    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.write_input_pty(input)
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        self.resize_pty(cols, rows)
    }

    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.reader.take()
    }
}

pub(crate) fn build_agent_command(binary_path: &Path, working_dir: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(binary_path.as_os_str());
    command.cwd(working_dir.as_os_str());
    for arg in default_agent_args(binary_path) {
        command.arg(arg);
    }
    command.env_clear();
    for key in SANITIZED_ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(key) {
            command.env(OsStr::new(key), value);
        }
    }
    if command.get_env("TERM").is_none() {
        command.env("TERM", "xterm-256color");
    }
    command
}

fn default_agent_args(binary_path: &Path) -> &'static [&'static str] {
    if binary_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("codex"))
    {
        &["--no-alt-screen"]
    } else {
        &[]
    }
}

const SANITIZED_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "CODEX_HOME",
    "USER",
    "USERNAME",
    "TMP",
    "TEMP",
    "LANG",
    "LC_ALL",
    "TERM",
];

fn spawn_error(message: String) -> AgentConsoleError {
    AgentConsoleError::new("pty_spawn_failed", message)
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
    let output = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output()
        .map_err(|e| AgentConsoleError::new("process_tree_kill_failed", e.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AgentConsoleError::new(
            "process_tree_kill_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
    let process_group = format!("-{pid}");
    let term = Command::new("kill")
        .args(["-TERM", &process_group])
        .status()
        .map_err(|e| AgentConsoleError::new("process_tree_kill_failed", e.to_string()))?;
    if !term.success() {
        return Err(AgentConsoleError::new(
            "process_tree_kill_failed",
            "no se pudo senalizar el grupo de proceso",
        ));
    }

    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = Command::new("kill")
        .args(["-KILL", &process_group])
        .status();
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn kill_process_tree(_pid: u32) -> Result<(), AgentConsoleError> {
    Err(AgentConsoleError::new(
        "process_tree_kill_unsupported",
        "process tree kill no esta soportado en esta plataforma",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn build_agent_command_uses_binary_and_working_dir() {
        let command = build_agent_command(Path::new("codex"), Path::new("/tmp/repo"));

        assert_eq!(
            command.get_argv(),
            &[OsString::from("codex"), OsString::from("--no-alt-screen")]
        );
        assert_eq!(command.get_cwd(), Some(&OsString::from("/tmp/repo")));
    }

    #[test]
    fn build_agent_command_clears_unallowlisted_environment() {
        let command = build_agent_command(Path::new("codex"), Path::new("/tmp/repo"));

        assert!(command.get_env("OPENAI_API_KEY").is_none());
        assert!(command.get_env("ANTHROPIC_API_KEY").is_none());
        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
    }

    #[test]
    fn build_agent_command_preserves_agent_profile_environment() {
        let command = build_agent_command(Path::new("codex"), Path::new("/tmp/repo"));

        for key in ["APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "CODEX_HOME"] {
            if let Some(value) = std::env::var_os(key) {
                assert_eq!(command.get_env(key), Some(value.as_os_str()));
            }
        }
    }

    #[test]
    fn unsupported_process_tree_kill_path_is_structured() {
        #[cfg(not(any(unix, windows)))]
        {
            let error = kill_process_tree(123).unwrap_err();
            assert_eq!(error.category, "process_tree_kill_unsupported");
        }
    }
}
