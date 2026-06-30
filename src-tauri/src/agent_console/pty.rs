use std::{
    io::{Read, Write},
    path::Path,
    process::Command,
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::{app_server::CodexAppServerHandle, AgentConsoleError};

pub const TINTO_TURN_DONE_MARKER: &str = "::tinto-turn-done::";

#[cfg(windows)]
use crate::windows_process::hide_console;

pub trait AgentProcess: Send {
    fn pid(&self) -> Option<u32>;
    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError>;
    fn kill(&mut self) -> Result<(), AgentConsoleError>;
    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError>;
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError>;
    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>>;
    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProcessEvent {
    FileActivity { timestamp_ms: u64 },
    TurnCompleted { timestamp_ms: u64 },
}

pub trait AgentProcessFactory: Send + Sync {
    fn spawn_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError>;

    fn spawn_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (agent_type, distro, working_dir);
        Err(AgentConsoleError::new(
            "wsl_agent_unsupported",
            "este runtime no soporta sesiones WSL",
        ))
    }
}

#[derive(Debug, Default)]
pub struct PortablePtyFactory;

impl AgentProcessFactory for PortablePtyFactory {
    fn spawn_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if is_codex_binary(binary_path) {
            match CodexAppServerHandle::spawn(binary_path, working_dir) {
                Ok(handle) => return Ok(Box::new(handle)),
                Err(_error) => {
                    // App-server is the preferred Codex runtime, but it is experimental.
                    // Preserve the terminal path when the local Codex build cannot host it.
                }
            }
        }
        Ok(Box::new(PtyHandle::spawn(binary_path, working_dir)?))
    }

    fn spawn_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        Ok(Box::new(PtyHandle::spawn_wsl_agent(
            agent_type,
            distro,
            working_dir,
        )?))
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
        Self::spawn_command_builder(build_agent_command(binary_path, working_dir))
    }

    pub fn spawn_wsl_agent(
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_command_builder(build_wsl_agent_command(distro, agent_type, working_dir)?)
    }

    fn spawn_command_builder(command: CommandBuilder) -> Result<Self, AgentConsoleError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize::default())
            .map_err(|e| spawn_error(e.to_string()))?;
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
    apply_terminal_env(&mut command);
    command
}

pub(crate) fn build_wsl_agent_command(
    distro: &str,
    agent_type: &str,
    working_dir: &Path,
) -> Result<CommandBuilder, AgentConsoleError> {
    if distro.trim().is_empty() {
        return Err(AgentConsoleError::new(
            "missing_distro",
            "no se configuro la distro WSL",
        ));
    }
    let mut command = CommandBuilder::new("wsl.exe");
    command.arg("-d");
    command.arg(distro);
    command.arg("--exec");
    command.arg("bash");
    command.arg("-lc");
    command.arg(WSL_AGENT_CONSOLE_SCRIPT);
    command.arg("tinto-agent-console");
    command.arg(working_dir.as_os_str());
    command.arg(agent_type);
    for arg in default_agent_args_for_name(agent_type) {
        command.arg(arg);
    }
    apply_terminal_env(&mut command);
    Ok(command)
}

const WSL_AGENT_CONSOLE_SCRIPT: &str = r#"set +u
for profile in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile" "$HOME/.bashrc"; do
  if [ -r "$profile" ]; then
    . "$profile" >/dev/null 2>&1 || true
  fi
done
candidate_path="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.npm-global/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
export TINTO_IADE=1
export TINTO_IADE_NAME='Integrated Agentic Development Environment'
export TINTO_TURN_DONE_MARKER='::tinto-turn-done::'
native_path=
old_ifs=$IFS
IFS=':'
for path_entry in $candidate_path; do
  case "$path_entry" in
    /mnt/[A-Za-z]/*) continue ;;
  esac
  if [ -n "$path_entry" ]; then
    native_path="${native_path:+$native_path:}$path_entry"
  fi
done
IFS=$old_ifs
export PATH="$native_path"
resolve_agent_binary() {
  agent_name=$1
  resolved=$(command -v -- "$agent_name" 2>/dev/null || true)
  if [ -n "$resolved" ]; then
    printf '%s\n' "$resolved"
    return 0
  fi
  if [ "$agent_name" = "codex" ]; then
    link="$HOME/.local/bin/codex"
    if [ -L "$link" ]; then
      target=$(readlink "$link" || true)
      root=${target%/*/*}
      if [ -n "$root" ] && [ -d "$root" ]; then
        candidate=$(find "$root" -mindepth 2 -maxdepth 2 -type f -name codex -perm -111 -printf '%T@ %p\n' 2>/dev/null | sort -nr | sed -n '1s/^[^ ]* //p')
        if [ -n "$candidate" ]; then
          printf '%s\n' "$candidate"
          return 0
        fi
      fi
    fi
  fi
  return 1
}
cd "$1" || exit 127
shift
agent_name=$1
shift
attempts=20
while [ "$attempts" -gt 0 ]; do
  if resolved_agent=$(resolve_agent_binary "$agent_name"); then
    exec "$resolved_agent" "$@"
  fi
  attempts=$((attempts - 1))
  sleep 0.25
done
if ! resolved_agent=$(resolve_agent_binary "$agent_name"); then
  printf 'Tinto: no se encontro %s en PATH dentro de WSL. Instala el agente en esta distro o crea un enlace en ~/.local/bin.\n' "$agent_name" >&2
  exit 127
fi
exec "$resolved_agent" "$@""#;

fn default_agent_args(binary_path: &Path) -> &'static [&'static str] {
    if is_codex_binary(binary_path) {
        default_agent_args_for_name("codex")
    } else {
        &[]
    }
}

fn is_codex_binary(binary_path: &Path) -> bool {
    binary_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("codex"))
}

fn default_agent_args_for_name(agent_type: &str) -> &'static [&'static str] {
    if agent_type.eq_ignore_ascii_case("codex") {
        &["--no-alt-screen"]
    } else {
        &[]
    }
}

fn apply_terminal_env(command: &mut CommandBuilder) {
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TINTO_IADE", "1");
    command.env(
        "TINTO_IADE_NAME",
        "Integrated Agentic Development Environment",
    );
    command.env("TINTO_TURN_DONE_MARKER", TINTO_TURN_DONE_MARKER);
}

fn spawn_error(message: String) -> AgentConsoleError {
    AgentConsoleError::new("pty_spawn_failed", message)
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
    let mut command = Command::new("taskkill");
    let output = hide_console(command.args(["/F", "/T", "/PID", &pid.to_string()]))
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
    use std::ffi::{OsStr, OsString};

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
    fn build_agent_command_sets_terminal_environment() {
        let command = build_agent_command(Path::new("codex"), Path::new("/tmp/repo"));

        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(command.get_env("TINTO_IADE"), Some(OsStr::new("1")));
        assert_eq!(
            command.get_env("TINTO_IADE_NAME"),
            Some(OsStr::new("Integrated Agentic Development Environment"))
        );
        assert_eq!(
            command.get_env("TINTO_TURN_DONE_MARKER"),
            Some(OsStr::new(TINTO_TURN_DONE_MARKER))
        );
    }

    #[test]
    fn unsupported_process_tree_kill_path_is_structured() {
        #[cfg(not(any(unix, windows)))]
        {
            let error = kill_process_tree(123).unwrap_err();
            assert_eq!(error.category, "process_tree_kill_unsupported");
        }
    }

    #[test]
    fn build_wsl_agent_command_passes_repo_and_agent_as_argv() {
        let command =
            build_wsl_agent_command("Ubuntu", "codex", Path::new("/home/me/repo")).unwrap();

        let argv = command.get_argv();
        assert_eq!(
            &argv[..6],
            [
                OsString::from("wsl.exe"),
                OsString::from("-d"),
                OsString::from("Ubuntu"),
                OsString::from("--exec"),
                OsString::from("bash"),
                OsString::from("-lc"),
            ]
        );
        assert!(argv[6].to_string_lossy().contains(".bashrc"));
        assert!(argv[6].to_string_lossy().contains("command -v"));
        assert_eq!(
            &argv[7..],
            [
                OsString::from("tinto-agent-console"),
                OsString::from("/home/me/repo"),
                OsString::from("codex"),
                OsString::from("--no-alt-screen"),
            ]
        );
        assert!(command.get_cwd().is_none());
        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(command.get_env("TINTO_IADE"), Some(OsStr::new("1")));
        assert_eq!(
            command.get_env("TINTO_TURN_DONE_MARKER"),
            Some(OsStr::new(TINTO_TURN_DONE_MARKER))
        );
    }
}
