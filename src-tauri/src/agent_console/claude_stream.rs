//! Claude Code conversations over its stream-json protocol:
//! `claude -p --input-format stream-json --output-format stream-json`
//! keeps one conversation per process, takes each user turn as a JSON line
//! on stdin and reports its messages, tool calls, hook outcomes and the end
//! of each turn as JSON lines.
//!
//! - Turns are projected onto the same timeline frames and process events
//!   Codex and Muse produce, so the conversation never shows Claude Code's
//!   terminal screen.
//! - Permission questions come back on the same channel
//!   (`--permission-prompt-tool stdio`) and wait for the user in Tinto, as
//!   ACP permissions do.
//! - A hook that fails (the user's own, from their settings) is said once
//!   in the conversation instead of being lost in the screen.

use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
};

use serde_json::{json, Value};

use super::{
    app_server::{
        buffered_turn_text, now_ms, spawn_command, timeline_frame_for_thread,
        timeline_frame_for_thread_with_turn_done, ChannelReader,
    },
    pty::{
        claude_launch_args, kill_process_tree, prompt_with_file_attachments, AgentProcess,
        AgentProcessEvent, AgentTurnAttachment,
    },
    sanitize_provider_timeline_text, AgentConsoleError,
};
use crate::bus::contract::{
    AgentSessionAcpPermission, AgentSessionAcpPermissionKind, AgentSessionAcpPermissionOption,
    AgentSessionAcpPermissionState, AgentSessionRuntimeOptions, AgentSessionTimelineKind,
};
use crate::wsl_agent::shell_env::agent_console_script;

const ALLOW: &str = "allow";
const DENY: &str = "deny";
/// The most of a command's output a turn shows.
const OUTPUT_CHARS: usize = 4000;
/// The most of a tool's detail an activity line shows.
const DETAIL_CHARS: usize = 160;

/// The flags of a stream-json conversation, before the user's own.
fn stream_args(resume: Option<&str>, wsl: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-prompt-tool",
        "stdio",
    ]
    .map(str::to_string)
    .to_vec();
    if let Some(session_id) = resume {
        args.extend(["--resume".to_string(), session_id.to_string()]);
    }
    args.extend(crate::delivery::wiring::claude_args(wsl));
    args.extend(claude_launch_args());
    args
}

fn build_command(binary_path: &Path, working_dir: &Path, resume: Option<&str>) -> Command {
    let mut command = Command::new(binary_path);
    command
        .current_dir(working_dir)
        .args(stream_args(resume, false))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn build_wsl_command(
    distro: &str,
    working_dir: &Path,
    resume: Option<&str>,
) -> Result<Command, AgentConsoleError> {
    if distro.trim().is_empty() {
        return Err(AgentConsoleError::new(
            "missing_distro",
            "no se configuro la distro WSL",
        ));
    }
    let mut command = Command::new("wsl.exe");
    command
        .arg("-d")
        .arg(distro)
        .arg("--exec")
        .arg("bash")
        .arg("-lc")
        .arg(agent_console_script())
        .arg("tinto-agent-console-claude-stream")
        .arg(working_dir.as_os_str())
        .arg("claude")
        .args(stream_args(resume, true))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

/// A permission Claude Code waits on, with the input to run the tool with.
struct PendingPermission {
    permission: AgentSessionAcpPermission,
    input: Value,
}

/// State shared between the handle and the thread reading Claude Code.
struct Shared {
    stdin: Mutex<Box<dyn Write + Send>>,
    output_tx: Sender<Vec<u8>>,
    event_tx: Sender<AgentProcessEvent>,
    session_id: Mutex<Option<String>>,
    /// The last text of the running turn: its answer if the result has none.
    last_text: Mutex<Option<String>>,
    /// Each tool call's name and label, by id, for its result.
    tools: Mutex<HashMap<String, (String, String)>>,
    permissions: Mutex<Vec<PendingPermission>>,
    next_request: AtomicU64,
    /// The settings files that may define the user's hooks, to name the one
    /// that failed; none in WSL, whose files Tinto does not read.
    settings_files: Vec<PathBuf>,
    /// The hooks already said to fail: one that fails on every prompt is
    /// said once.
    failed_hooks: Mutex<HashSet<String>>,
}

impl Shared {
    fn new(
        stdin: Box<dyn Write + Send>,
        output_tx: Sender<Vec<u8>>,
        event_tx: Sender<AgentProcessEvent>,
        settings_files: Vec<PathBuf>,
    ) -> Self {
        Self {
            stdin: Mutex::new(stdin),
            output_tx,
            event_tx,
            session_id: Mutex::new(None),
            last_text: Mutex::new(None),
            tools: Mutex::new(HashMap::new()),
            permissions: Mutex::new(Vec::new()),
            next_request: AtomicU64::new(1),
            settings_files,
            failed_hooks: Mutex::new(HashSet::new()),
        }
    }

    fn timeline(&self, kind: AgentSessionTimelineKind, text: &str) {
        let _ = self
            .output_tx
            .send(timeline_frame_for_thread(kind, text, None));
    }

    fn send(&self, value: &Value) -> Result<(), AgentConsoleError> {
        let mut line = serde_json::to_vec(value)
            .map_err(|e| AgentConsoleError::new("claude_encode_failed", e.to_string()))?;
        line.push(b'\n');
        let mut stdin = self.stdin.lock().map_err(|_| lock_error())?;
        stdin
            .write_all(&line)
            .and_then(|_| stdin.flush())
            .map_err(|e| AgentConsoleError::new("claude_write_failed", e.to_string()))
    }

    /// Answers a permission Claude Code waits on.
    fn answer_permission(&self, id: &str, allow: bool) -> Result<(), AgentConsoleError> {
        let pending = {
            let mut permissions = self.permissions.lock().map_err(|_| lock_error())?;
            let at = permissions
                .iter()
                .position(|p| p.permission.id == id)
                .ok_or_else(|| {
                    AgentConsoleError::new(
                        "acp_permission_unavailable",
                        "ese permiso ya no está pendiente",
                    )
                })?;
            permissions.remove(at)
        };
        self.send(&permission_answer(id, allow, &pending.input))
    }

    fn request_id(&self) -> String {
        format!("tinto-{}", self.next_request.fetch_add(1, Ordering::SeqCst))
    }
}

fn lock_error() -> AgentConsoleError {
    AgentConsoleError::new("claude_lock_poisoned", "Claude session lock failed")
}

pub struct ClaudeStreamHandle {
    child: Child,
    shared: Arc<Shared>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<ChannelReader>,
    line_buffer: Vec<u8>,
}

impl ClaudeStreamHandle {
    pub fn spawn(
        binary_path: &Path,
        working_dir: &Path,
        resume: Option<&str>,
    ) -> Result<Self, AgentConsoleError> {
        Self::from_child(
            spawn_command(build_command(binary_path, working_dir, resume))?,
            settings_files(working_dir),
        )
    }

    pub fn spawn_wsl(
        distro: &str,
        working_dir: &Path,
        resume: Option<&str>,
    ) -> Result<Self, AgentConsoleError> {
        Self::from_child(
            spawn_command(build_wsl_command(distro, working_dir, resume)?)?,
            Vec::new(),
        )
    }

    fn from_child(
        mut child: Child,
        settings_files: Vec<PathBuf>,
    ) -> Result<Self, AgentConsoleError> {
        let unavailable = |what: &str| {
            AgentConsoleError::new("claude_spawn_failed", format!("{what} unavailable"))
        };
        let stdin = child.stdin.take().ok_or_else(|| unavailable("stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| unavailable("stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| unavailable("stderr"))?;
        let (output_tx, output_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let shared = Arc::new(Shared::new(
            Box::new(stdin),
            output_tx,
            event_tx,
            settings_files,
        ));
        let reader = Arc::clone(&shared);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(message) = serde_json::from_str::<Value>(&line) {
                    handle_message(&message, &reader);
                }
            }
        });
        // What Claude Code says before it can speak JSON (an unknown
        // conversation to resume, a bad flag) is the only word of why it ended.
        let errors = Arc::clone(&shared);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let line = line.trim();
                if !line.is_empty() {
                    errors.timeline(
                        AgentSessionTimelineKind::Lifecycle,
                        &format!("Claude Code: {line}"),
                    );
                }
            }
        });
        Ok(Self {
            child,
            shared,
            event_rx,
            output_reader: Some(ChannelReader::new(output_rx)),
            line_buffer: Vec::new(),
        })
    }

    fn submit_turn(&self, text: String) -> Result<(), AgentConsoleError> {
        self.shared.send(&json!({
            "type": "user",
            "message": { "role": "user", "content": text },
        }))
    }
}

impl AgentProcess for ClaudeStreamHandle {
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        self.child
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or_default()))
            .map_err(|error| AgentConsoleError::new("claude_status_failed", error.to_string()))
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        if kill_process_tree(self.child.id()).is_ok() {
            let _ = self.child.kill();
            return Ok(());
        }
        self.child
            .kill()
            .map_err(|error| AgentConsoleError::new("claude_kill_failed", error.to_string()))
    }

    /// Keys typed into the conversation: a line is a turn.
    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        for byte in input {
            match *byte {
                b'\r' => {
                    let text = buffered_turn_text(&self.line_buffer);
                    self.line_buffer.clear();
                    if !text.is_empty() {
                        self.submit_turn(text)?;
                    }
                }
                b'\n' => self.line_buffer.push(*byte),
                0x08 | 0x7f => {
                    let _ = self.line_buffer.pop();
                }
                byte if byte.is_ascii_control() => {}
                byte => self.line_buffer.push(byte),
            }
        }
        Ok(())
    }

    fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        _options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        // Claude Code reads the listed files itself, images included; the
        // model and reasoning are launch flags.
        self.submit_turn(prompt_with_file_attachments(text, attachments, true))
    }

    fn reports_turns(&self) -> bool {
        true
    }

    fn supports_turn_interrupt(&self) -> bool {
        true
    }

    fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        self.shared.send(&json!({
            "type": "control_request",
            "request_id": self.shared.request_id(),
            "request": { "subtype": "interrupt" },
        }))
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
        Ok(())
    }

    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.output_reader
            .take()
            .map(|reader| Box::new(reader) as Box<dyn Read + Send>)
    }

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        self.event_rx.try_iter().collect()
    }

    fn provider_session_id(&self) -> Option<String> {
        self.shared.session_id.lock().ok().and_then(|id| id.clone())
    }

    fn acp_permissions(&self) -> Vec<AgentSessionAcpPermission> {
        self.shared
            .permissions
            .lock()
            .map(|pending| pending.iter().map(|p| p.permission.clone()).collect())
            .unwrap_or_default()
    }

    fn respond_acp_permission(
        &mut self,
        permission_id: &str,
        option_id: Option<&str>,
        deny: bool,
    ) -> Result<(), AgentConsoleError> {
        let allow = !deny && option_id != Some(DENY);
        self.shared.answer_permission(permission_id, allow)
    }
}

/// Claude Code's answer to `can_use_tool`.
fn permission_answer(request_id: &str, allow: bool, input: &Value) -> Value {
    let decision = if allow {
        json!({ "behavior": "allow", "updatedInput": input })
    } else {
        json!({ "behavior": "deny", "message": "El usuario lo rechazó en Tinto." })
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": decision },
    })
}

fn handle_message(message: &Value, shared: &Shared) {
    match message["type"].as_str() {
        Some("system") => handle_system(message, shared),
        Some("assistant") => {
            for block in content(message) {
                match block["type"].as_str() {
                    Some("text") => {
                        let text = sanitize_provider_timeline_text(
                            block["text"].as_str().unwrap_or_default().trim(),
                        );
                        if !text.is_empty() {
                            shared.timeline(AgentSessionTimelineKind::AgentProgress, &text);
                            if let Ok(mut last) = shared.last_text.lock() {
                                *last = Some(text);
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = block["name"].as_str().unwrap_or("una herramienta");
                        let label = tool_label(name, &block["input"]);
                        // The conversation pairs a command with its output
                        // by the whole command, as Codex reports it.
                        let activity = match block["input"]["command"].as_str() {
                            Some(command) if name == "Bash" => format!("Ejecutando {command}"),
                            _ => format!("Usando {label}"),
                        };
                        shared.timeline(AgentSessionTimelineKind::Activity, &activity);
                        if let (Some(id), Ok(mut tools)) =
                            (block["id"].as_str(), shared.tools.lock())
                        {
                            tools.insert(id.to_string(), (name.to_string(), label));
                        }
                    }
                    _ => {}
                }
            }
        }
        Some("user") => {
            for block in content(message) {
                if block["type"] != "tool_result" {
                    continue;
                }
                let Some((name, label)) = block["tool_use_id"]
                    .as_str()
                    .and_then(|id| shared.tools.lock().ok()?.remove(id))
                else {
                    continue;
                };
                let output = result_text(&block["content"]);
                if block["is_error"] == true {
                    let reason = output.lines().next().unwrap_or("sin detalle");
                    shared.timeline(
                        AgentSessionTimelineKind::Activity,
                        &cut(&format!("Falló {label}: {reason}"), DETAIL_CHARS + 60),
                    );
                } else if name == "Bash" && !output.trim().is_empty() {
                    shared.timeline(
                        AgentSessionTimelineKind::CommandOutput,
                        &cut(&output, OUTPUT_CHARS),
                    );
                }
            }
        }
        Some("result") => {
            let last = shared
                .last_text
                .lock()
                .ok()
                .and_then(|mut last| last.take());
            let answer = message["result"]
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .or(last);
            if let Some(answer) = answer {
                shared.timeline(AgentSessionTimelineKind::AgentMessage, &answer);
            }
            if message["is_error"] == true {
                let reason = message["subtype"].as_str().unwrap_or("sin detalle");
                shared.timeline(
                    AgentSessionTimelineKind::Lifecycle,
                    &format!("El turno falló: {reason}"),
                );
            }
            let _ = shared
                .output_tx
                .send(timeline_frame_for_thread_with_turn_done(
                    AgentSessionTimelineKind::Lifecycle,
                    "",
                    None,
                    true,
                ));
            let _ = shared.event_tx.send(AgentProcessEvent::TurnCompleted {
                timestamp_ms: now_ms(),
            });
        }
        Some("control_request") => handle_control_request(message, shared),
        Some("control_cancel_request") => {
            if let (Some(id), Ok(mut permissions)) =
                (message["request_id"].as_str(), shared.permissions.lock())
            {
                permissions.retain(|p| p.permission.id != id);
            }
        }
        _ => {}
    }
}

fn handle_system(message: &Value, shared: &Shared) {
    match message["subtype"].as_str() {
        Some("init") => {
            if let (Some(id), Ok(mut slot)) =
                (message["session_id"].as_str(), shared.session_id.lock())
            {
                *slot = Some(id.to_string());
            }
        }
        Some("hook_response") => {
            let name = message["hook_name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let first = shared
                .failed_hooks
                .lock()
                .is_ok_and(|failed| !failed.contains(&name));
            if !first {
                return;
            }
            let settings: Vec<(String, Value)> = shared
                .settings_files
                .iter()
                .filter_map(|path| {
                    let text = std::fs::read_to_string(path).ok()?;
                    Some((
                        path.display().to_string(),
                        serde_json::from_str(&text).ok()?,
                    ))
                })
                .collect();
            if let Some(notice) = hook_notice(message, &settings) {
                shared.timeline(AgentSessionTimelineKind::Lifecycle, &notice);
                if let Ok(mut failed) = shared.failed_hooks.lock() {
                    failed.insert(name);
                }
            }
        }
        _ => {}
    }
}

fn handle_control_request(message: &Value, shared: &Shared) {
    let request_id = message["request_id"].as_str().unwrap_or_default();
    let request = &message["request"];
    if request["subtype"] != "can_use_tool" {
        // A request Tinto does not serve: answer so Claude Code never waits.
        let _ = shared.send(&json!({
            "type": "control_response",
            "response": {
                "subtype": "error",
                "request_id": request_id,
                "error": "not supported by Tinto",
            },
        }));
        return;
    }
    let name = request["tool_name"].as_str().unwrap_or("una herramienta");
    let label = tool_label(name, &request["input"]);
    let option = |id: &str, label: &str, kind| AgentSessionAcpPermissionOption {
        id: id.to_string(),
        label: label.to_string(),
        kind,
    };
    let permission = AgentSessionAcpPermission {
        id: request_id.to_string(),
        generation: 0,
        provider_session_id: shared
            .session_id
            .lock()
            .ok()
            .and_then(|id| id.clone())
            .unwrap_or_default(),
        turn_id: String::new(),
        tool_call_id: request["tool_use_id"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        title: label.clone(),
        options: vec![
            option(ALLOW, "Permitir", AgentSessionAcpPermissionKind::AllowOnce),
            option(DENY, "Rechazar", AgentSessionAcpPermissionKind::RejectOnce),
        ],
        state: AgentSessionAcpPermissionState::Pending,
        reason: request["decision_reason"]
            .as_str()
            .or_else(|| request["description"].as_str())
            .map(str::to_string),
        // Claude Code waits as long as the user takes.
        expires_at_ms: u64::MAX,
    };
    if let Ok(mut permissions) = shared.permissions.lock() {
        permissions.push(PendingPermission {
            permission,
            input: request["input"].clone(),
        });
    }
    shared.timeline(
        AgentSessionTimelineKind::Activity,
        &format!("Pide permiso para {label}"),
    );
}

fn content(message: &Value) -> &[Value] {
    message["message"]["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// A tool result's text, whether one string or a list of text blocks.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Where the user's hooks may be: their own settings and the project's.
fn settings_files(working_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = crate::runtime_paths::user_home_dir()
        .map(|home| home.join(".claude").join("settings.json"))
        .into_iter()
        .collect();
    let project = working_dir.join(".claude");
    files.extend([
        project.join("settings.json"),
        project.join("settings.local.json"),
    ]);
    files
}

/// The commands the settings run on `event`, each with the file it is in.
fn hook_commands(event: &str, settings: &[(String, Value)]) -> Vec<String> {
    settings
        .iter()
        .flat_map(|(file, value)| {
            value["hooks"][event]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|entry| entry["hooks"].as_array().into_iter().flatten())
                .filter_map(|hook| hook["command"].as_str())
                .map(move |command| format!("{file}: {command}"))
        })
        .collect()
}

/// A failed hook, in a line: which one, its exit code, what it said and,
/// when the settings name it, the command and the file that defines it.
fn hook_notice(message: &Value, settings: &[(String, Value)]) -> Option<String> {
    let exit_code = message["exit_code"].as_i64().unwrap_or_default();
    if message["outcome"] != "error" && exit_code == 0 {
        return None;
    }
    let name = message["hook_name"].as_str().unwrap_or("un hook");
    let said = message["stderr"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .or_else(|| message["output"].as_str())
        .and_then(|text| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or("sin salida");
    let said = cut(said, DETAIL_CHARS);
    let mut notice = format!(
        "Falló el hook {name} de tu configuración de Claude Code (código {exit_code}): {said}"
    );
    let defined = hook_commands(message["hook_event"].as_str().unwrap_or_default(), settings);
    if !defined.is_empty() {
        notice.push_str(&format!(
            ". Corrígelo o quítalo donde está definido: {}. Tinto no lo cambia por ti.",
            defined.join("; ")
        ));
    }
    Some(notice)
}

/// What a tool call does, in a few words: the tool and its main argument.
fn tool_label(name: &str, input: &Value) -> String {
    let detail = match name {
        "Bash" => input["command"].as_str(),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => input["file_path"]
            .as_str()
            .or_else(|| input["notebook_path"].as_str())
            .map(file_name),
        "Grep" | "Glob" => input["pattern"].as_str(),
        "WebFetch" => input["url"].as_str(),
        "WebSearch" => input["query"].as_str(),
        "Task" | "Agent" => input["description"].as_str(),
        _ => None,
    };
    // `mcp__tinto-delivery__wait_run` reads as `wait_run`.
    let name = name
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
        .map_or(name, |(_, tool)| tool);
    match detail.map(|detail| detail.lines().next().unwrap_or_default().trim()) {
        Some(detail) if !detail.is_empty() => cut(&format!("{name}: {detail}"), DETAIL_CHARS),
        _ => name.to_string(),
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The first `limit` characters of `text`, with an ellipsis when cut.
fn cut(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    /// A writer the test reads back: what Tinto sent Claude Code.
    #[derive(Clone, Default)]
    struct Sent(Arc<Mutex<Vec<u8>>>);

    impl Write for Sent {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sent {
        fn lines(&self) -> Vec<Value> {
            let bytes = self.0.lock().unwrap().clone();
            String::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        }
    }

    fn shared() -> (Shared, Sent, Receiver<Vec<u8>>, Receiver<AgentProcessEvent>) {
        let sent = Sent::default();
        let (output_tx, output) = mpsc::channel();
        let (event_tx, events) = mpsc::channel();
        (
            Shared::new(Box::new(sent.clone()), output_tx, event_tx, Vec::new()),
            sent,
            output,
            events,
        )
    }

    /// The conversation's items, as `kind: text`.
    fn shown(output: &Receiver<Vec<u8>>) -> Vec<String> {
        output
            .try_iter()
            .map(|frame| {
                let json = frame.splitn(2, |byte| *byte == b' ').nth(1).unwrap();
                let frame: Value = serde_json::from_slice(json.trim_ascii_end()).unwrap();
                let text = STANDARD
                    .decode(frame["text_base64"].as_str().unwrap())
                    .unwrap();
                let done = if frame["turn_done"] == true {
                    " (turn done)"
                } else {
                    ""
                };
                format!(
                    "{}: {}{done}",
                    frame["kind"].as_str().unwrap(),
                    String::from_utf8(text).unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn a_turn_shows_its_answer_tools_permissions_and_failed_hooks_without_the_screen() {
        let (shared, sent, output, events) = shared();
        let write = json!({"file_path": "C:\\repo\\src\\hello.txt", "content": "hi"});
        // Lines as Claude Code 2.1.312 writes them, trimmed to what Tinto reads.
        for line in [
            json!({"type": "system", "subtype": "hook_started", "hook_name": "SessionStart:startup"}),
            json!({"type": "system", "subtype": "hook_response", "hook_name": "SessionStart:startup",
                "exit_code": 49, "outcome": "error", "stdout": "",
                "stderr": "Python was not found; run without arguments to install"}),
            // A hook that fails on every prompt is said once.
            json!({"type": "system", "subtype": "hook_response", "hook_name": "SessionStart:startup",
                "exit_code": 49, "outcome": "error", "stderr": "Python was not found"}),
            json!({"type": "system", "subtype": "init", "session_id": "s-1"}),
            json!({"type": "assistant", "message": {"content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "Lo miro."},
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "npm test\n--watch"}},
                {"type": "tool_use", "id": "t2", "name": "Write", "input": write},
            ]}}),
            json!({"type": "control_request", "request_id": "p1", "request": {
                "subtype": "can_use_tool", "tool_name": "Write", "tool_use_id": "t2", "input": write}}),
            json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "12 passed"},
                {"type": "tool_result", "tool_use_id": "t2", "is_error": true,
                    "content": [{"type": "text", "text": "El usuario lo rechazó en Tinto."}]},
            ]}}),
            json!({"type": "result", "subtype": "success", "is_error": false, "result": "Hecho."}),
        ] {
            handle_message(&line, &shared);
        }

        assert_eq!(
            shown(&output),
            [
                "lifecycle: Falló el hook SessionStart:startup de tu configuración de Claude Code (código 49): Python was not found; run without arguments to install",
                "agent_progress: Lo miro.",
                "activity: Ejecutando npm test\n--watch",
                "activity: Usando Write: hello.txt",
                "activity: Pide permiso para Write: hello.txt",
                "command_output: 12 passed",
                "activity: Falló Write: hello.txt: El usuario lo rechazó en Tinto.",
                "agent_message: Hecho.",
                "lifecycle:  (turn done)",
            ]
        );
        assert!(matches!(
            events.try_iter().collect::<Vec<_>>().as_slice(),
            [AgentProcessEvent::TurnCompleted { .. }]
        ));
        assert_eq!(
            shared.session_id.lock().unwrap().as_deref(),
            Some("s-1"),
            "the conversation resumes by it"
        );

        // The permission waits for the user, who answers it once.
        let pending: Vec<String> = shared
            .permissions
            .lock()
            .unwrap()
            .iter()
            .map(|p| format!("{} {}", p.permission.id, p.permission.title))
            .collect();
        assert_eq!(pending, ["p1 Write: hello.txt"]);
        shared.answer_permission("p1", true).unwrap();
        assert!(shared.answer_permission("p1", true).is_err());
        assert_eq!(
            sent.lines(),
            [
                json!({"type": "control_response", "response": {"subtype": "success",
                "request_id": "p1", "response": {"behavior": "allow", "updatedInput": write}}})
            ]
        );
    }

    #[test]
    fn a_failed_hook_names_the_command_and_the_file_that_defines_it() {
        let settings = [(
            "C:/Users/me/.claude/settings.json".to_string(),
            json!({"hooks": {
                "SessionStart": [{"hooks": [{"type": "command", "command": "python watch.py check"}]}],
                "Stop": [{"hooks": [{"type": "command", "command": "other"}]}],
            }}),
        )];
        let failed = json!({"hook_name": "SessionStart:startup", "hook_event": "SessionStart",
            "exit_code": 49, "outcome": "error", "stderr": "Python was not found"});
        assert_eq!(
            hook_notice(&failed, &settings).unwrap(),
            "Falló el hook SessionStart:startup de tu configuración de Claude Code (código 49): Python was not found. Corrígelo o quítalo donde está definido: C:/Users/me/.claude/settings.json: python watch.py check. Tinto no lo cambia por ti."
        );
        let passed = json!({"hook_name": "Stop", "hook_event": "Stop", "exit_code": 0, "outcome": "success"});
        assert_eq!(hook_notice(&passed, &settings), None);
    }

    #[test]
    fn requests_tinto_does_not_serve_are_answered_so_claude_never_waits() {
        let (shared, sent, _output, _events) = shared();
        handle_message(
            &json!({"type": "control_request", "request_id": "x", "request": {"subtype": "hook_callback"}}),
            &shared,
        );
        assert_eq!(sent.lines()[0]["response"]["subtype"], "error");
        assert!(shared.permissions.lock().unwrap().is_empty());
    }
}
