use std::{
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::{app_server::CodexAppServerHandle, AgentConsoleError};
use crate::{
    bus::contract::{
        AgentRuntimeCatalog, AgentSessionAcpPermission, AgentSessionAcpRuntime,
        AgentSessionGoalStatus, AgentSessionRuntimeOptions,
    },
    wsl_agent::shell_env::agent_console_script,
};

pub const TINTO_TURN_DONE_MARKER: &str = "::tinto-turn-done::";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTurnAttachment {
    pub path: PathBuf,
    pub is_image: bool,
}

pub fn prompt_with_file_attachments(
    text: &str,
    attachments: &[AgentTurnAttachment],
    include_images: bool,
) -> String {
    let paths = attachments
        .iter()
        .filter(|attachment| include_images || !attachment.is_image)
        .map(|attachment| format!("- {}", attachment.path.to_string_lossy()))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return text.to_string();
    }
    format!(
        "# Files mentioned by the user:\n{}\n\n{}",
        paths.join("\n"),
        text
    )
}

#[cfg(windows)]
use crate::windows_process::hide_console;

pub trait AgentProcess: Send {
    fn pid(&self) -> Option<u32>;
    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError>;
    fn kill(&mut self) -> Result<(), AgentConsoleError>;
    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError>;
    fn write_input_with_options(
        &mut self,
        input: &[u8],
        _options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.write_input(input)
    }
    fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let prompt = prompt_with_file_attachments(text, attachments, true);
        let mut input = prompt.into_bytes();
        input.push(b'\r');
        self.write_input_with_options(&input, options)
    }
    fn steer_turn(
        &mut self,
        _text: &str,
        _attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "steer_unsupported",
            "este runtime no admite intervenir en el turno activo",
        ))
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError>;
    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>>;
    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        Vec::new()
    }
    fn runtime_catalog(&self) -> Option<AgentRuntimeCatalog> {
        None
    }
    fn provider_session_id(&self) -> Option<String> {
        None
    }
    fn acp_runtime(&self) -> Option<AgentSessionAcpRuntime> {
        None
    }
    fn acp_permissions(&self) -> Vec<AgentSessionAcpPermission> {
        Vec::new()
    }
    fn respond_acp_permission(
        &mut self,
        _permission_id: &str,
        _option_id: Option<&str>,
        _deny: bool,
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "acp_permission_unavailable",
            "esta sesión no tiene permisos ACP pendientes",
        ))
    }
    fn set_acp_config_option(
        &mut self,
        _config_id: &str,
        _value_id: &str,
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "acp_option_unavailable",
            "esta sesión no tiene esta opción ACP negociada",
        ))
    }
    fn retry_acp(&mut self, _confirmed: bool, _turn_idle: bool) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "acp_retry_unavailable",
            "esta sesión no admite reintentar ACP",
        ))
    }
    fn refresh_runtime_catalog(
        &mut self,
    ) -> Result<Option<AgentRuntimeCatalog>, AgentConsoleError> {
        Ok(None)
    }
    fn supports_goals(&self) -> bool {
        false
    }
    fn update_goal(
        &mut self,
        _objective: Option<&str>,
        _status: Option<AgentSessionGoalStatus>,
        _token_budget: Option<Option<u64>>,
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "goal_unsupported",
            "este runtime no admite objetivos persistentes",
        ))
    }
    fn clear_goal(&mut self) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "goal_unsupported",
            "este runtime no admite objetivos persistentes",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProcessEvent {
    FileActivity {
        timestamp_ms: u64,
    },
    TurnCompleted {
        timestamp_ms: u64,
    },
    Error {
        error: AgentConsoleError,
    },
    GoalUpdated {
        goal: crate::bus::contract::AgentSessionGoal,
    },
    GoalCleared,
    ResumeContextRequired {
        summary: crate::bus::contract::AgentSessionContextSummary,
    },
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

    fn resume_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
        provider_session_id: &str,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (binary_path, working_dir, provider_session_id);
        Err(AgentConsoleError::new(
            "agent_resume_unsupported",
            "este runtime no puede reanudar conversaciones nativas",
        ))
    }

    fn resume_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        provider_session_id: &str,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (agent_type, distro, working_dir, provider_session_id);
        Err(AgentConsoleError::new(
            "agent_resume_unsupported",
            "este runtime WSL no puede reanudar conversaciones nativas",
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
        if agent_type.eq_ignore_ascii_case("codex") {
            match CodexAppServerHandle::spawn_wsl(distro, working_dir) {
                Ok(handle) => return Ok(Box::new(handle)),
                Err(_error) => {
                    // Keep the WSL-native terminal path as a compatibility fallback for
                    // Codex builds that do not expose app-server yet.
                }
            }
        }
        Ok(Box::new(PtyHandle::spawn_wsl_agent(
            agent_type,
            distro,
            working_dir,
        )?))
    }

    fn resume_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
        provider_session_id: &str,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if !is_codex_binary(binary_path) {
            return Err(AgentConsoleError::new(
                "agent_resume_unsupported",
                "el proveedor no admite reanudacion nativa todavia",
            ));
        }
        Ok(Box::new(CodexAppServerHandle::resume(
            binary_path,
            working_dir,
            provider_session_id,
        )?))
    }

    fn resume_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        provider_session_id: &str,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if !agent_type.eq_ignore_ascii_case("codex") {
            return Err(AgentConsoleError::new(
                "agent_resume_unsupported",
                "el proveedor no admite reanudacion nativa todavia",
            ));
        }
        Ok(Box::new(CodexAppServerHandle::resume_wsl(
            distro,
            working_dir,
            provider_session_id,
        )?))
    }
}

pub struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    reader: Option<Box<dyn Read + Send>>,
    writer: Box<dyn Write + Send>,
    exit_code: Option<i32>,
}

impl PtyHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        Self::spawn_command_builder(build_agent_command(binary_path, working_dir))
    }

    pub fn spawn_with_env_allowlist(
        binary_path: &Path,
        working_dir: &Path,
        allowed_env: &[&str],
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_command_builder(build_agent_command_with_env_allowlist(
            binary_path,
            working_dir,
            allowed_env,
        ))
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
            exit_code: None,
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
        if self.exit_code.is_some() {
            return Ok(self.exit_code);
        }
        let exit_code = self
            .child
            .try_wait()
            .map(|status| status.map(|s| s.exit_code() as i32))
            .map_err(|e| AgentConsoleError::new("process_status_failed", e.to_string()))?;
        if exit_code.is_some() {
            self.exit_code = exit_code;
        }
        Ok(exit_code)
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        let tree_result = self
            .child
            .process_id()
            .map(kill_process_tree)
            .unwrap_or(Ok(()));
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) | Err(_) => {
                    return Err(AgentConsoleError::new(
                        "process_reap_timeout",
                        "el proceso PTY no terminó dentro del plazo",
                    ));
                }
            }
        };
        self.exit_code = Some(status.exit_code() as i32);
        tree_result?;
        Ok(())
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

pub(crate) fn build_agent_command_with_env_allowlist(
    binary_path: &Path,
    working_dir: &Path,
    allowed_env: &[&str],
) -> CommandBuilder {
    let mut command = build_agent_command(binary_path, working_dir);
    command.env_clear();
    for name in allowed_env {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
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
    command.arg(agent_console_script());
    command.arg("tinto-agent-console");
    command.arg(working_dir.as_os_str());
    command.arg(agent_type);
    for arg in default_agent_args_for_name(agent_type) {
        command.arg(arg);
    }
    apply_terminal_env(&mut command);
    Ok(command)
}

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
pub(crate) fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
    let mut command = Command::new("taskkill");
    let mut child = hide_console(
        command
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .spawn()
    .map_err(|e| AgentConsoleError::new("process_tree_kill_failed", e.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(AgentConsoleError::new(
                    "process_tree_kill_failed",
                    format!("taskkill terminó con {status}"),
                ))
            }
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                return Err(AgentConsoleError::new(
                    "process_tree_kill_timeout",
                    "taskkill no terminó dentro del plazo",
                ));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(AgentConsoleError::new(
                    "process_tree_kill_failed",
                    error.to_string(),
                ));
            }
        }
    }
}

#[cfg(unix)]
pub(crate) fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
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
pub(crate) fn kill_process_tree(_pid: u32) -> Result<(), AgentConsoleError> {
    Err(AgentConsoleError::new(
        "process_tree_kill_unsupported",
        "process tree kill no esta soportado en esta plataforma",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::sync::Mutex;

    static ENV_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn fallback_prompt_mentions_every_attachment_path() {
        let prompt = prompt_with_file_attachments(
            "review these",
            &[
                AgentTurnAttachment {
                    path: PathBuf::from("/tmp/screen.png"),
                    is_image: true,
                },
                AgentTurnAttachment {
                    path: PathBuf::from("/tmp/brief.pdf"),
                    is_image: false,
                },
            ],
            true,
        );

        assert!(prompt.contains("- /tmp/screen.png"));
        assert!(prompt.contains("- /tmp/brief.pdf"));
        assert!(prompt.ends_with("review these"));
    }

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
    fn fallback_allowlist_drops_secret_environment_canary() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        const CANARY: &str = "TINTO_ACP_SECRET_CANARY";
        let previous = env::var_os(CANARY);
        env::set_var(CANARY, "must-not-reach-fallback");
        let command = build_agent_command_with_env_allowlist(
            Path::new("kimi"),
            Path::new("/tmp/repo"),
            &["PATH"],
        );
        if let Some(previous) = previous {
            env::set_var(CANARY, previous);
        } else {
            env::remove_var(CANARY);
        }

        assert!(command.get_env(CANARY).is_none());
        assert_eq!(command.get_env("PATH"), env::var_os("PATH").as_deref());
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
        assert!(argv[6].to_string_lossy().contains("$nvm_dir/nvm.sh"));
        assert!(argv[6].to_string_lossy().contains("readlink -f"));
        assert!(argv[6].to_string_lossy().contains("--version"));
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
