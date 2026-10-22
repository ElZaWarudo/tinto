use std::{
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use super::claude_hooks::{forward_slashes, ClaudeHooks};
use super::{
    app_server::CodexAppServerHandle,
    msp::{is_muse_binary, muse_program, MuseServeHandle},
    AgentConsoleError, ResumeResultReceiver,
};
use crate::{
    bus::contract::{
        AgentRuntimeCatalog, AgentSessionAcpPermission, AgentSessionAcpRuntime,
        AgentSessionGoalStatus, AgentSessionPermissionMode, AgentSessionRuntimeOptions,
        AgentSubagentThread,
    },
    wsl_agent::shell_env::agent_console_script,
};

thread_local! {
    static LAUNCH_OPTIONS: std::cell::RefCell<Option<AgentSessionRuntimeOptions>> =
        const { std::cell::RefCell::new(None) };
}

/// Runs `spawn` with the model and reasoning a new conversation starts on.
/// Claude Code takes them as launch flags; Codex and Muse take them per turn.
pub fn with_launch_options<T>(
    options: Option<AgentSessionRuntimeOptions>,
    spawn: impl FnOnce() -> T,
) -> T {
    LAUNCH_OPTIONS.with(|slot| *slot.borrow_mut() = options);
    let spawned = spawn();
    LAUNCH_OPTIONS.with(|slot| *slot.borrow_mut() = None);
    spawned
}

/// `--model` and `--effort` for the Claude Code conversation being launched.
fn claude_launch_args() -> Vec<String> {
    // With its Chrome extension installed, Claude Code opens with a question
    // about using the user's browser and waits for a key nobody in Tinto
    // presses. Tinto has its own browser for QA: keep Chrome's tools off.
    let mut args = vec!["--no-chrome".to_string()];
    let Some(options) = LAUNCH_OPTIONS.with(|slot| slot.borrow().clone()) else {
        return args;
    };
    if let Some(model) = options.model.as_deref().filter(|model| !model.is_empty()) {
        args.extend(["--model".to_string(), model.to_string()]);
    }
    if let Some(effort) = options
        .reasoning_effort
        .as_deref()
        .filter(|effort| !effort.is_empty())
    {
        args.extend(["--effort".to_string(), effort.to_string()]);
    }
    args
}

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

#[cfg(any(unix, windows))]
use std::process::Stdio;

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
    /// Whether the provider says when its turns start and end. Without it a
    /// session has no turns: Tinto does not guess them from the output.
    fn reports_turns(&self) -> bool {
        false
    }
    /// Whether sending a turn starts one. Claude Code says so itself when it
    /// accepts the prompt; a local command such as `/help` starts none.
    fn turn_starts_on_send(&self) -> bool {
        self.reports_turns()
    }
    /// The file where the provider's hooks report its turns, to watch it.
    fn hook_events_path(&self) -> Option<PathBuf> {
        None
    }
    fn supports_turn_interrupt(&self) -> bool {
        false
    }
    fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "interrupt_unsupported",
            "este runtime no admite interrumpir el turno activo",
        ))
    }
    fn supports_context_compaction(&self) -> bool {
        false
    }
    fn compact_context(&mut self) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "compact_unsupported",
            "este runtime no admite compactar el contexto de forma nativa",
        ))
    }
    fn supports_permission_mode_change(&self) -> bool {
        false
    }
    fn set_permission_mode(
        &mut self,
        _permission_mode: AgentSessionPermissionMode,
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "permission_mode_unsupported",
            "este runtime no admite cambiar el acceso por turno",
        ))
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError>;
    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>>;
    fn take_resume_result(&mut self) -> Option<ResumeResultReceiver> {
        None
    }
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

    /// Optional provider-owned descendant projection. PTY and ACP runtimes
    /// retain the safe no-op defaults; Codex app-server overrides these hooks.
    fn supports_subagents(&self) -> bool {
        false
    }
    fn discover_subagents(&mut self) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagents_unsupported",
            "este runtime no admite subagentes nativos",
        ))
    }
    fn write_subagent_turn(
        &mut self,
        _thread_id: &str,
        _text: &str,
        _attachments: &[AgentTurnAttachment],
        _options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagent_unsupported",
            "este runtime no admite entrada directa a subagentes",
        ))
    }
    fn write_subagent_turn_with_metadata(
        &mut self,
        thread_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
        _permission_mode: AgentSessionPermissionMode,
        _approval_policy: Option<&str>,
    ) -> Result<(), AgentConsoleError> {
        self.write_subagent_turn(thread_id, text, attachments, options)
    }
    fn steer_subagent_turn(
        &mut self,
        _thread_id: &str,
        _text: &str,
        _attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagent_steer_unsupported",
            "este runtime no admite intervenir en subagentes",
        ))
    }
    fn interrupt_subagent_turn(&mut self, _thread_id: &str) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagent_interrupt_unsupported",
            "este runtime no admite interrumpir subagentes",
        ))
    }
    fn wait_subagent(&mut self, _thread_id: &str) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagent_wait_unsupported",
            "este runtime no admite esperar subagentes",
        ))
    }
    fn close_subagent(&mut self, _thread_id: &str) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "subagent_close_unsupported",
            "este runtime no admite cerrar subagentes",
        ))
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProcessEvent {
    TurnStarted {
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
    ContextUsageUpdated {
        used_tokens: u64,
        model_context_window: u64,
    },
    SubagentUpdated {
        subagent: AgentSubagentThread,
    },
    SubagentTimeline {
        thread_id: String,
        item: crate::bus::contract::AgentSessionTimelineItem,
    },
    SubagentDiscoveryFailed {
        error: AgentConsoleError,
    },
}

pub trait AgentProcessFactory: Send + Sync {
    fn spawn_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError>;

    fn spawn_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (agent_type, distro, working_dir, permission_mode);
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
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (
            binary_path,
            working_dir,
            provider_session_id,
            permission_mode,
        );
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
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        let _ = (
            agent_type,
            distro,
            working_dir,
            provider_session_id,
            permission_mode,
        );
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
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if is_muse_binary(binary_path) {
            // Like Codex, prefer the structured protocol and keep Muse's own
            // terminal UI as the fallback.
            if let Ok(handle) = MuseServeHandle::spawn(binary_path, working_dir, permission_mode) {
                return Ok(Box::new(handle));
            }
        }
        if is_codex_binary(binary_path) {
            match CodexAppServerHandle::spawn(binary_path, working_dir, permission_mode) {
                Ok(handle) => return Ok(Box::new(handle)),
                Err(_error) => {
                    // App-server is the preferred Codex runtime, but it is experimental.
                    // Preserve the terminal path when the local Codex build cannot host it.
                }
            }
        }
        Ok(Box::new(PtyHandle::spawn_with_permission(
            binary_path,
            working_dir,
            permission_mode,
        )?))
    }

    fn spawn_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if agent_type.eq_ignore_ascii_case("muse") {
            if let Ok(handle) = MuseServeHandle::spawn_wsl(distro, working_dir, permission_mode) {
                return Ok(Box::new(handle));
            }
        }
        if agent_type.eq_ignore_ascii_case("codex") {
            match CodexAppServerHandle::spawn_wsl(distro, working_dir, permission_mode) {
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
            permission_mode,
        )?))
    }

    fn resume_agent(
        &self,
        binary_path: &Path,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if is_claude_binary(binary_path) {
            return Ok(Box::new(PtyHandle::resume_claude(
                binary_path,
                working_dir,
                provider_session_id,
                permission_mode,
            )?));
        }
        if is_muse_binary(binary_path) {
            return Ok(Box::new(MuseServeHandle::resume(
                binary_path,
                working_dir,
                provider_session_id,
                permission_mode,
            )?));
        }
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
            permission_mode,
        )?))
    }

    fn resume_wsl_agent(
        &self,
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Box<dyn AgentProcess>, AgentConsoleError> {
        if agent_type.eq_ignore_ascii_case("claude") {
            return Ok(Box::new(PtyHandle::resume_wsl_claude(
                distro,
                working_dir,
                provider_session_id,
                permission_mode,
            )?));
        }
        if agent_type.eq_ignore_ascii_case("muse") {
            return Ok(Box::new(MuseServeHandle::resume_wsl(
                distro,
                working_dir,
                provider_session_id,
                permission_mode,
            )?));
        }
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
            permission_mode,
        )?))
    }
}

pub struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    reader: Option<Box<dyn Read + Send>>,
    /// Shared with the reader, which answers the console's cursor query.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    exit_code: Option<i32>,
    claude_hooks: Option<ClaudeHooks>,
    /// Whether the agent has been seen reading keys (see `wait_until_reading_keys`).
    #[cfg(unix)]
    reads_keys: bool,
}

/// Windows' console host asks the terminal where its cursor is (`ESC[6n`)
/// when an agent starts, and holds the agent until it hears back. No
/// terminal of Tinto's answers it (its views do not send keys), so Claude
/// Code sat silent forever: the reader answers with the top-left corner.
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";

struct AnswersCursorQuery {
    inner: Box<dyn Read + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl Read for AnswersCursorQuery {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if buf[..read]
            .windows(CURSOR_QUERY.len())
            .any(|window| window == CURSOR_QUERY)
        {
            if let Ok(mut writer) = self.writer.lock() {
                let _ = writer.write_all(CURSOR_REPLY);
                let _ = writer.flush();
            }
        }
        Ok(read)
    }
}

/// The pause between a message and the Enter that sends it.
const ENTER_DELAY: Duration = Duration::from_millis(200);
/// How long the first write waits for an agent that just started to read keys.
#[cfg(unix)]
const READS_KEYS_TIMEOUT: Duration = Duration::from_secs(5);

/// Writes a message and the Enter that sends it as two writes, `delay`
/// apart. In one write a TUI on a Unix PTY reads both in one chunk, takes
/// it for pasted text and leaves the message unsent. A lone key (Enter,
/// Escape, an arrow) stays one write.
fn write_with_separate_enter(
    writer: &mut dyn Write,
    input: &[u8],
    delay: Duration,
) -> std::io::Result<()> {
    match input.split_last() {
        Some((b'\r', text)) if !text.is_empty() => {
            writer.write_all(text)?;
            writer.flush()?;
            thread::sleep(delay);
            writer.write_all(b"\r")
        }
        _ => writer.write_all(input),
    }
}

impl PtyHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        Self::spawn_with_permission(
            binary_path,
            working_dir,
            AgentSessionPermissionMode::Workspace,
        )
    }

    pub fn spawn_with_permission(
        binary_path: &Path,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_local(binary_path, working_dir, permission_mode, None)
    }

    /// Claude Code continuing one of its conversations.
    pub fn resume_claude(
        binary_path: &Path,
        working_dir: &Path,
        claude_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_local(
            binary_path,
            working_dir,
            permission_mode,
            Some(claude_session_id),
        )
    }

    fn spawn_local(
        binary_path: &Path,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
        resume: Option<&str>,
    ) -> Result<Self, AgentConsoleError> {
        let mut command =
            build_agent_command_with_permission(binary_path, working_dir, permission_mode);
        let hooks = is_claude_binary(binary_path)
            .then(|| with_claude_hooks(&mut command, false, resume))
            .flatten();
        let mut handle = Self::spawn_command_builder(command)?;
        handle.claude_hooks = hooks;
        Ok(handle)
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
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_wsl(agent_type, distro, working_dir, permission_mode, None)
    }

    /// Claude Code in WSL continuing one of its conversations.
    pub fn resume_wsl_claude(
        distro: &str,
        working_dir: &Path,
        claude_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_wsl(
            "claude",
            distro,
            working_dir,
            permission_mode,
            Some(claude_session_id),
        )
    }

    fn spawn_wsl(
        agent_type: &str,
        distro: &str,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
        resume: Option<&str>,
    ) -> Result<Self, AgentConsoleError> {
        let mut command =
            build_wsl_agent_command(distro, agent_type, working_dir, permission_mode)?;
        let hooks = agent_type
            .eq_ignore_ascii_case("claude")
            .then(|| with_claude_hooks(&mut command, true, resume))
            .flatten();
        let mut handle = Self::spawn_command_builder(command)?;
        handle.claude_hooks = hooks;
        Ok(handle)
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
        let writer = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .map_err(|e| spawn_error(e.to_string()))?,
        ));
        let reader: Box<dyn Read + Send> = if cfg!(windows) {
            Box::new(AnswersCursorQuery {
                inner: reader,
                writer: Arc::clone(&writer),
            })
        } else {
            reader
        };

        Ok(Self {
            master: pair.master,
            child,
            reader: Some(reader),
            writer,
            exit_code: None,
            claude_hooks: None,
            #[cfg(unix)]
            reads_keys: false,
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
        self.wait_until_reading_keys();
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| AgentConsoleError::new("pty_write_failed", "terminal bloqueada"))?;
        write_with_separate_enter(&mut **writer, input, ENTER_DELAY)
            .map_err(|e| AgentConsoleError::new("pty_write_failed", e.to_string()))
    }

    /// An agent that has just started reads keys once its TUI puts the
    /// terminal in raw mode. What arrives before waits in the line buffer and
    /// reaches it as one chunk it does not send: a resumed conversation's
    /// first message, written as soon as the agent starts. The first write
    /// waits for raw mode, a few seconds at most.
    #[cfg(unix)]
    fn wait_until_reading_keys(&mut self) {
        use nix::sys::termios::LocalFlags;
        if self.reads_keys {
            return;
        }
        let deadline = Instant::now() + READS_KEYS_TIMEOUT;
        while Instant::now() < deadline
            && self
                .master
                .get_termios()
                .is_some_and(|termios| termios.local_flags.contains(LocalFlags::ICANON))
        {
            thread::sleep(Duration::from_millis(50));
        }
        self.reads_keys = true;
    }

    /// ConPTY delivers keys as key events from the start.
    #[cfg(not(unix))]
    fn wait_until_reading_keys(&mut self) {}

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

    fn reports_turns(&self) -> bool {
        self.claude_hooks.as_ref().is_some_and(ClaudeHooks::active)
    }

    fn turn_starts_on_send(&self) -> bool {
        false
    }

    fn hook_events_path(&self) -> Option<PathBuf> {
        self.claude_hooks
            .as_ref()
            .map(|hooks| hooks.path().to_path_buf())
    }

    fn provider_session_id(&self) -> Option<String> {
        self.claude_hooks
            .as_ref()
            .and_then(|hooks| hooks.session_id())
            .map(str::to_owned)
    }

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        self.claude_hooks
            .as_mut()
            .map(ClaudeHooks::drain)
            .unwrap_or_default()
    }

    fn supports_turn_interrupt(&self) -> bool {
        self.reports_turns()
    }

    fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        if !self.reports_turns() {
            return Err(AgentConsoleError::new(
                "interrupt_unsupported",
                "este runtime no admite interrumpir el turno activo",
            ));
        }
        self.write_input_pty(b"")?;
        // Claude reports nothing when interrupted; the next drain ends the turn.
        if let Some(hooks) = self.claude_hooks.as_mut() {
            hooks.mark_interrupted();
        }
        Ok(())
    }
}

/// Adds Claude Code's turn hooks to its command. Without them (no config
/// folder, no path to this executable) the session simply has no turns.
fn with_claude_hooks(
    command: &mut CommandBuilder,
    wsl: bool,
    resume: Option<&str>,
) -> Option<ClaudeHooks> {
    if let Some(claude_session_id) = resume {
        command.arg("--resume");
        command.arg(claude_session_id);
    }
    let hooks = ClaudeHooks::create(resume).ok()?;
    let exe = std::env::current_exe().ok()?;
    let hook_command = if wsl {
        crate::wsl_agent::launcher::windows_path_to_wsl_mount(&exe).ok()?
    } else {
        forward_slashes(&exe)
    };
    command.arg("--settings");
    command.arg(hooks.settings(&hook_command));
    Some(hooks)
}

pub(crate) fn build_agent_command(binary_path: &Path, working_dir: &Path) -> CommandBuilder {
    build_agent_command_with_permission(
        binary_path,
        working_dir,
        AgentSessionPermissionMode::Workspace,
    )
}

pub(crate) fn build_agent_command_with_permission(
    binary_path: &Path,
    working_dir: &Path,
    permission_mode: AgentSessionPermissionMode,
) -> CommandBuilder {
    let (program, prefix_args) = if is_muse_binary(binary_path) {
        muse_program(binary_path)
    } else {
        (binary_path.to_path_buf(), Vec::new())
    };
    let mut command = CommandBuilder::new(program.as_os_str());
    command.cwd(working_dir.as_os_str());
    for arg in prefix_args {
        command.arg(arg);
    }
    for arg in default_agent_args(binary_path) {
        command.arg(arg);
    }
    append_codex_permission_args(&mut command, is_codex_binary(binary_path), permission_mode);
    append_muse_permission_args(&mut command, is_muse_binary(binary_path), permission_mode);
    if is_claude_binary(binary_path) {
        for arg in crate::delivery::wiring::claude_args(false) {
            command.arg(arg);
        }
        for arg in claude_launch_args() {
            command.arg(arg);
        }
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
    permission_mode: AgentSessionPermissionMode,
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
    append_codex_permission_args(
        &mut command,
        agent_type.eq_ignore_ascii_case("codex"),
        permission_mode,
    );
    append_muse_permission_args(
        &mut command,
        agent_type.eq_ignore_ascii_case("muse"),
        permission_mode,
    );
    if agent_type.eq_ignore_ascii_case("claude") {
        for arg in crate::delivery::wiring::claude_args(true) {
            command.arg(arg);
        }
        for arg in claude_launch_args() {
            command.arg(arg);
        }
    }
    apply_terminal_env(&mut command);
    Ok(command)
}

fn append_codex_permission_args(
    command: &mut CommandBuilder,
    is_codex: bool,
    permission_mode: AgentSessionPermissionMode,
) {
    if !is_codex {
        return;
    }
    command.arg("--ask-for-approval");
    command.arg("never");
    command.arg("--sandbox");
    command.arg(match permission_mode {
        AgentSessionPermissionMode::Workspace => "workspace-write",
        AgentSessionPermissionMode::FullAccess => "danger-full-access",
    });
}

/// Muse's own terminal asks before acting and sandboxes its shell; full
/// access turns both off, as Codex's danger-full-access does.
fn append_muse_permission_args(
    command: &mut CommandBuilder,
    is_muse: bool,
    permission_mode: AgentSessionPermissionMode,
) {
    if is_muse && permission_mode == AgentSessionPermissionMode::FullAccess {
        command.arg("--yolo");
    }
}

fn default_agent_args(binary_path: &Path) -> &'static [&'static str] {
    if is_codex_binary(binary_path) {
        default_agent_args_for_name("codex")
    } else {
        &[]
    }
}

fn is_claude_binary(binary_path: &Path) -> bool {
    binary_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("claude"))
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

/// Stops the process group `pid` leads, and the groups its descendants lead
/// apart: Codex runs each command in a group of its own, which a signal to
/// the tree's group misses and which outlives it, adopted by init.
#[cfg(unix)]
pub(crate) fn kill_process_tree(pid: u32) -> Result<(), AgentConsoleError> {
    // Read before signalling: once a parent dies its children's link to the tree is gone.
    let detached = descendant_group_leaders(pid);
    let process_group = format!("-{pid}");
    let term = signal_group("-TERM", &process_group)
        .map_err(|e| AgentConsoleError::new("process_tree_kill_failed", e.to_string()))?;
    if !term.success() {
        if is_unreaped(pid) {
            return Ok(());
        }
        return Err(AgentConsoleError::new(
            "process_tree_kill_failed",
            "no se pudo senalizar el grupo de proceso",
        ));
    }
    for leader in &detached {
        let _ = signal_group("-TERM", &format!("-{leader}"));
    }

    // A moment to exit on its own (Codex closes its connection), then for sure.
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline && !has_exited(pid) {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = signal_group("-KILL", &process_group);
    for leader in &detached {
        let _ = signal_group("-KILL", &format!("-{leader}"));
    }
    Ok(())
}

/// Sends `signal` to a process group; a group already gone is not an error worth a log line.
#[cfg(unix)]
fn signal_group(signal: &str, group: &str) -> std::io::Result<std::process::ExitStatus> {
    Command::new("kill")
        .args([signal, "--", group])
        .stderr(Stdio::null())
        .status()
}

/// The descendants of `pid` that lead a process group of their own.
#[cfg(unix)]
fn descendant_group_leaders(pid: u32) -> Vec<u32> {
    let Ok(output) = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,pgid="])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let processes: Vec<[u32; 3]> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(|field| field.parse().ok());
            Some([fields.next()??, fields.next()??, fields.next()??])
        })
        .collect();
    let mut tree = vec![pid];
    let mut leaders = Vec::new();
    let mut index = 0;
    while index < tree.len() {
        let parent = tree[index];
        for &[child, ppid, pgid] in &processes {
            if ppid == parent && !tree.contains(&child) {
                tree.push(child);
                if pgid == child {
                    leaders.push(child);
                }
            }
        }
        index += 1;
    }
    leaders
}

/// Whether the process is gone or only waits to be reaped.
#[cfg(unix)]
fn has_exited(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .stderr(Stdio::null())
        .output()
        .map_or(true, |output| {
            let stat = output.stdout.trim_ascii_start();
            stat.is_empty() || stat.starts_with(b"Z")
        })
}

/// Whether the group leader has exited and waits to be reaped. macOS refuses
/// to signal a group left with only that leader (EPERM) where Linux delivers
/// the signal; either way nothing in the group is running.
#[cfg(unix)]
fn is_unreaped(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .is_ok_and(|output| output.stdout.trim_ascii_start().starts_with(b"Z"))
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

    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_reader_answers_the_console_cursor_query_and_passes_output_on() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let writer: Box<dyn Write + Send> = Box::new(Shared(Arc::clone(&sent)));
        let mut reader = AnswersCursorQuery {
            inner: Box::new(std::io::Cursor::new(b"\x1b[6nClaude Code".to_vec())),
            writer: Arc::new(Mutex::new(writer)),
        };
        let mut output = String::new();
        reader.read_to_string(&mut output).unwrap();
        assert_eq!(output, "\x1b[6nClaude Code");
        assert_eq!(sent.lock().unwrap().as_slice(), CURSOR_REPLY);
    }

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
            &[
                OsString::from("codex"),
                OsString::from("--no-alt-screen"),
                OsString::from("--ask-for-approval"),
                OsString::from("never"),
                OsString::from("--sandbox"),
                OsString::from("workspace-write"),
            ]
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

    /// Keeps each write apart, as the PTY receives them.
    #[derive(Default)]
    struct RecordingWriter(Vec<Vec<u8>>);

    impl Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_message_and_the_enter_that_sends_it_go_in_two_writes() {
        let mut writer = RecordingWriter::default();
        for input in [&b"hola\r"[..], b"a\nb\r", b"\r", b"\x1b[B"] {
            write_with_separate_enter(&mut writer, input, Duration::ZERO).unwrap();
        }
        let expected: Vec<&[u8]> = vec![b"hola", b"\r", b"a\nb", b"\r", b"\r", b"\x1b[B"];
        assert_eq!(writer.0, expected);
    }

    #[cfg(unix)]
    #[test]
    fn the_first_write_waits_until_the_agent_reads_keys() {
        let dir = tempfile::tempdir().unwrap();
        let keys = dir.path().join("keys");
        // Like a TUI that takes a moment to start: raw mode, then read.
        let mut command = CommandBuilder::new("sh");
        command.arg("-c");
        command.arg(format!(
            "sleep 0.5; stty raw -echo; head -c 5 > '{}'",
            keys.display()
        ));
        let mut handle = PtyHandle::spawn_command_builder(command).unwrap();

        handle.write_input(b"hola\r").unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while handle.try_exit_code().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the agent never read its keys");
            thread::sleep(Duration::from_millis(20));
        }
        // Written before raw mode, the line buffer would have turned the
        // Enter into a newline and delivered the line as one chunk.
        assert_eq!(std::fs::read(&keys).unwrap(), b"hola\r");
    }

    #[cfg(unix)]
    #[test]
    fn process_tree_kill_reaches_commands_running_in_their_own_group() {
        use std::os::unix::process::CommandExt;

        let dir = tempfile::tempdir().unwrap();
        // One command in the tree's group and one in its own, as Codex runs them.
        let mut agent = Command::new("sh")
            .current_dir(dir.path())
            .args([
                "-c",
                "sleep 60 & echo $! > same.pid; \
                 perl -e 'setpgrp(0, 0); exec \"sleep\", \"60\"' & echo $! > own.pid; wait",
            ])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid_in = |name: &str| -> u32 {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Ok(text) = std::fs::read_to_string(dir.path().join(name)) {
                    if let Ok(pid) = text.trim().parse() {
                        return pid;
                    }
                }
                assert!(Instant::now() < deadline, "{name} never appeared");
                thread::sleep(Duration::from_millis(10));
            }
        };
        let commands = [pid_in("same.pid"), pid_in("own.pid")];
        let own_group = commands[1].to_string();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Command::new("ps")
            .args(["-o", "pgid=", "-p", &own_group])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).trim() != own_group)
        {
            assert!(
                Instant::now() < deadline,
                "the command never left the group"
            );
            thread::sleep(Duration::from_millis(10));
        }

        kill_process_tree(agent.id()).unwrap();
        agent.wait().unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        while !commands.iter().all(|&pid| has_exited(pid)) {
            assert!(Instant::now() < deadline, "a command outlived the stop");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_tree_kill_accepts_a_group_whose_leader_already_exited() {
        use std::os::unix::process::CommandExt;

        let mut child = Command::new("sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !is_unreaped(child.id()) {
            assert!(Instant::now() < deadline, "the leader never exited");
            std::thread::sleep(Duration::from_millis(10));
        }

        kill_process_tree(child.id()).unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn build_wsl_agent_command_passes_repo_and_agent_as_argv() {
        let command = build_wsl_agent_command(
            "Ubuntu",
            "codex",
            Path::new("/home/me/repo"),
            AgentSessionPermissionMode::Workspace,
        )
        .unwrap();

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
                OsString::from("--ask-for-approval"),
                OsString::from("never"),
                OsString::from("--sandbox"),
                OsString::from("workspace-write"),
            ]
        );
        assert!(command.get_cwd().is_none());
        assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
        assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
        assert_eq!(command.get_env("TINTO_IADE"), Some(OsStr::new("1")));
    }

    #[test]
    fn full_access_fallback_uses_explicit_codex_flags() {
        let command = build_agent_command_with_permission(
            Path::new("codex"),
            Path::new("/tmp/repo"),
            AgentSessionPermissionMode::FullAccess,
        );

        assert_eq!(
            &command.get_argv()[2..],
            [
                OsString::from("--ask-for-approval"),
                OsString::from("never"),
                OsString::from("--sandbox"),
                OsString::from("danger-full-access"),
            ]
        );
    }

    #[test]
    fn claude_starts_on_the_chosen_model_and_reasoning() {
        let options = AgentSessionRuntimeOptions {
            model: Some("opus".into()),
            reasoning_effort: Some("high".into()),
            speed: None,
        };
        let command = with_launch_options(Some(options), || {
            build_agent_command_with_permission(
                Path::new("claude"),
                Path::new("/tmp/repo"),
                AgentSessionPermissionMode::Workspace,
            )
        });
        let argv = command.get_argv();
        assert!(argv.ends_with(&[
            OsString::from("--model"),
            OsString::from("opus"),
            OsString::from("--effort"),
            OsString::from("high"),
        ]));

        // Only the conversation launched inside: the next one starts on the defaults.
        let next = build_agent_command_with_permission(
            Path::new("claude"),
            Path::new("/tmp/repo"),
            AgentSessionPermissionMode::Workspace,
        );
        assert!(!next.get_argv().contains(&OsString::from("--model")));
    }

    #[test]
    fn wsl_full_access_fallback_uses_explicit_codex_flags() {
        let command = build_wsl_agent_command(
            "Ubuntu",
            "codex",
            Path::new("/home/me/repo"),
            AgentSessionPermissionMode::FullAccess,
        )
        .unwrap();

        assert_eq!(
            &command.get_argv()[11..],
            [
                OsString::from("--ask-for-approval"),
                OsString::from("never"),
                OsString::from("--sandbox"),
                OsString::from("danger-full-access"),
            ]
        );
    }

    #[test]
    fn default_process_permission_change_is_explicitly_unsupported() {
        struct CapabilityProbe;

        impl AgentProcess for CapabilityProbe {
            fn pid(&self) -> Option<u32> {
                None
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
        }

        let mut process = CapabilityProbe;
        assert!(!process.supports_permission_mode_change());
        let error = process
            .set_permission_mode(AgentSessionPermissionMode::FullAccess)
            .unwrap_err();
        assert_eq!(error.category, "permission_mode_unsupported");
    }
}
