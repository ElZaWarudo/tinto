//! Muse Code sessions over the Muse Session Protocol (MSP): `muse serve`
//! speaks JSON-RPC 2.0 on stdio, one session per process.
//!
//! Like Codex's app-server, the session runs without approval prompts
//! (`allowAll`) inside Muse's sandbox, which full access turns off. Turns,
//! items and context usage are projected onto the same timeline frames and
//! process events the Codex runtime produces.

use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
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
        timeline_frame_for_thread_with_turn_done, write_json, ChannelReader,
    },
    pty::{
        kill_process_tree, prompt_with_file_attachments, AgentProcess, AgentProcessEvent,
        AgentTurnAttachment,
    },
    sanitize_provider_timeline_text, AgentConsoleError, ResumeResultReceiver,
};
use crate::bus::contract::{
    AgentRuntimeCatalog, AgentRuntimeCatalogStatus, AgentRuntimeModel, AgentRuntimeReasoningEffort,
    AgentSessionPermissionMode, AgentSessionResumeMode, AgentSessionRuntimeOptions,
    AgentSessionTimelineKind,
};
use crate::wsl_agent::shell_env::agent_console_script;

const INITIALIZE_REQUEST_ID: u64 = 1;
const MODEL_LIST_REQUEST_ID: u64 = 2;
const SESSION_REQUEST_ID: u64 = 3;
const FIRST_COMMAND_REQUEST_ID: u64 = 100;
const CATALOG_SOURCE: &str = "muse_serve";

/// How to start Muse Code. The installer's `muse.cmd` runs a PowerShell
/// launcher that checks for updates and then runs `muse-bin-<version>.exe`,
/// the version `.muse-version` names. Tinto runs that binary itself, as the
/// launcher does: Windows PowerShell drops the quotes inside arguments it
/// forwards, which breaks Muse's JSON `-c` values. Muse still updates
/// whenever `muse` runs, Tinto's version checks included.
pub(crate) fn muse_program(binary_path: &Path) -> (PathBuf, Vec<OsString>) {
    let is_cmd = binary_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"));
    if !is_cmd {
        return (binary_path.to_path_buf(), Vec::new());
    }
    if let Some(active) = active_binary(binary_path) {
        return (active, Vec::new());
    }
    let launcher = binary_path.with_file_name(".muse-launcher.ps1");
    if launcher.is_file() {
        return (
            PathBuf::from("powershell.exe"),
            vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                launcher.into_os_string(),
            ],
        );
    }
    (binary_path.to_path_buf(), Vec::new())
}

/// The binary the launcher next to `muse.cmd` would run.
fn active_binary(launcher_cmd: &Path) -> Option<PathBuf> {
    let version = std::fs::read_to_string(launcher_cmd.with_file_name(".muse-version")).ok()?;
    let version = version.trim();
    let well_formed = !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'));
    let binary = launcher_cmd.with_file_name(format!("muse-bin-{version}.exe"));
    (well_formed && binary.is_file()).then_some(binary)
}

pub(crate) fn is_muse_binary(binary_path: &Path) -> bool {
    binary_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("muse"))
}

/// `muse serve` keeps its sandbox for the whole process; full access runs
/// it without one. The repo is trusted so Muse follows its rules
/// (AGENTS.md), as Codex does.
fn serve_args(permission_mode: AgentSessionPermissionMode) -> &'static [&'static str] {
    match permission_mode {
        AgentSessionPermissionMode::Workspace => &["serve", "--trust-workspace"],
        AgentSessionPermissionMode::FullAccess => {
            &["serve", "--trust-workspace", "--disable-sandbox"]
        }
    }
}

fn build_serve_command(
    binary_path: &Path,
    working_dir: &Path,
    permission_mode: AgentSessionPermissionMode,
) -> Command {
    let (program, prefix) = muse_program(binary_path);
    let (mcp_args, mcp_env) = crate::delivery::wiring::muse_args(false);
    let mut command = Command::new(program);
    command
        .args(prefix)
        .args(serve_args(permission_mode))
        .args(mcp_args)
        .envs(mcp_env)
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

fn build_wsl_serve_command(
    distro: &str,
    working_dir: &Path,
    permission_mode: AgentSessionPermissionMode,
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
        .arg("tinto-agent-console-muse-serve")
        .arg(working_dir.as_os_str())
        .arg("muse")
        .args(serve_args(permission_mode))
        .args(crate::delivery::wiring::muse_args(true).0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(command)
}

/// A UUIDv7, the idempotency handle MSP requires on every command.
fn command_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

struct PendingTurn {
    text: String,
    options: Option<AgentSessionRuntimeOptions>,
}

/// State shared between the handle and the thread reading `muse serve`.
struct Shared {
    stdin: Arc<Mutex<ChildStdin>>,
    output_tx: Sender<Vec<u8>>,
    event_tx: Sender<AgentProcessEvent>,
    session_id: Mutex<Option<String>>,
    active_turn_id: Mutex<Option<String>>,
    pending_turns: Mutex<VecDeque<PendingTurn>>,
    /// The last agent message of the running turn: the turn's answer.
    last_agent_message: Mutex<Option<String>>,
    /// Item kinds by id, to tell tool output deltas from message deltas.
    item_kinds: Mutex<HashMap<String, String>>,
    runtime_catalog: Mutex<AgentRuntimeCatalog>,
    /// The model last selected with `session/setModel`.
    applied_model: Mutex<Option<String>>,
    next_request_id: AtomicU64,
    resume_tx: Mutex<Option<Sender<Result<AgentSessionResumeMode, AgentConsoleError>>>>,
}

impl Shared {
    fn next_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::SeqCst)
    }

    fn send(&self, method: &str, params: Value) -> Result<(), AgentConsoleError> {
        write_json(&self.stdin, &request(self.next_id(), method, params))
    }

    fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }

    fn timeline(&self, kind: AgentSessionTimelineKind, text: &str) {
        let _ = self
            .output_tx
            .send(timeline_frame_for_thread(kind, text, None));
    }

    fn start_turn(&self, session_id: &str, turn: PendingTurn) -> Result<(), AgentConsoleError> {
        let options = turn.options.unwrap_or_default();
        if let Some(model) = options.model.filter(|model| !model.is_empty()) {
            let mut applied = self.applied_model.lock().map_err(|_| lock_error())?;
            if applied.as_deref() != Some(model.as_str()) {
                self.send(
                    "session/setModel",
                    json!({
                        "commandId": command_id(),
                        "sessionId": session_id,
                        "model": { "modelId": model },
                    }),
                )?;
                *applied = Some(model);
            }
        }
        let mut params = json!({
            "commandId": command_id(),
            "sessionId": session_id,
            "input": [{ "type": "text", "text": turn.text }],
            "ifBusy": "queue",
        });
        if let Some(effort) = options.reasoning_effort.filter(|effort| !effort.is_empty()) {
            params["reasoningEffort"] = json!(effort);
        }
        self.send("turn/start", params)
    }

    fn resolve_resume(&self, result: Result<AgentSessionResumeMode, AgentConsoleError>) {
        if let Some(resume_tx) = self.resume_tx.lock().ok().and_then(|mut tx| tx.take()) {
            let _ = resume_tx.send(result);
        }
    }
}

fn lock_error() -> AgentConsoleError {
    AgentConsoleError::new("muse_lock_poisoned", "Muse session lock failed")
}

pub struct MuseServeHandle {
    child: Child,
    shared: Arc<Shared>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<ChannelReader>,
    resume_result: Option<ResumeResultReceiver>,
    line_buffer: Vec<u8>,
    pending_options: Option<AgentSessionRuntimeOptions>,
}

impl MuseServeHandle {
    pub fn spawn(
        binary_path: &Path,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let child = spawn_command(build_serve_command(
            binary_path,
            working_dir,
            permission_mode,
        ))?;
        Self::from_child(child, working_dir, None)
    }

    pub fn resume(
        binary_path: &Path,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let child = spawn_command(build_serve_command(
            binary_path,
            working_dir,
            permission_mode,
        ))?;
        Self::from_child(child, working_dir, Some(provider_session_id))
    }

    pub fn spawn_wsl(
        distro: &str,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let command = build_wsl_serve_command(distro, working_dir, permission_mode)?;
        Self::from_child(spawn_command(command)?, working_dir, None)
    }

    pub fn resume_wsl(
        distro: &str,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let command = build_wsl_serve_command(distro, working_dir, permission_mode)?;
        Self::from_child(
            spawn_command(command)?,
            working_dir,
            Some(provider_session_id),
        )
    }

    fn from_child(
        mut child: Child,
        working_dir: &Path,
        resume_session_id: Option<&str>,
    ) -> Result<Self, AgentConsoleError> {
        let stdin = child.stdin.take().ok_or_else(|| {
            AgentConsoleError::new("muse_spawn_failed", "muse serve stdin unavailable")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentConsoleError::new("muse_spawn_failed", "muse serve stdout unavailable")
        })?;
        let (output_tx, output_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (resume_tx, resume_result) = match resume_session_id {
            Some(_) => {
                let (tx, rx) = mpsc::channel();
                (Some(tx), Some(rx))
            }
            None => (None, None),
        };
        let shared = Arc::new(Shared {
            stdin: Arc::new(Mutex::new(stdin)),
            output_tx,
            event_tx,
            session_id: Mutex::new(None),
            active_turn_id: Mutex::new(None),
            pending_turns: Mutex::new(VecDeque::new()),
            last_agent_message: Mutex::new(None),
            item_kinds: Mutex::new(HashMap::new()),
            runtime_catalog: Mutex::new(AgentRuntimeCatalog {
                status: AgentRuntimeCatalogStatus::Loading,
                source: CATALOG_SOURCE.to_string(),
                models: Vec::new(),
                default_model: None,
                error: None,
                updated_at_ms: now_ms(),
            }),
            applied_model: Mutex::new(None),
            next_request_id: AtomicU64::new(FIRST_COMMAND_REQUEST_ID),
            resume_tx: Mutex::new(resume_tx),
        });

        write_json(
            &shared.stdin,
            &request(
                INITIALIZE_REQUEST_ID,
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "tinto",
                        "title": "Tinto",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            ),
        )?;
        write_json(
            &shared.stdin,
            &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
        )?;
        write_json(
            &shared.stdin,
            &request(MODEL_LIST_REQUEST_ID, "model/list", json!({})),
        )?;
        write_json(
            &shared.stdin,
            &session_request(working_dir, resume_session_id),
        )?;

        let reader_shared = Arc::clone(&shared);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<Value>(&line) {
                    Ok(message) => handle_message(&message, &reader_shared),
                    Err(_) if !line.trim().is_empty() => {
                        let _ = reader_shared
                            .output_tx
                            .send(format!("\r\n{line}\r\n").into_bytes());
                    }
                    Err(_) => {}
                }
            }
        });

        Ok(Self {
            child,
            shared,
            event_rx,
            output_reader: Some(ChannelReader::new(output_rx)),
            resume_result,
            line_buffer: Vec::new(),
            pending_options: None,
        })
    }

    fn submit_turn(
        &mut self,
        text: String,
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let turn = PendingTurn { text, options };
        match self.shared.session_id() {
            Some(session_id) => self.shared.start_turn(&session_id, turn),
            None => {
                self.shared
                    .pending_turns
                    .lock()
                    .map_err(|_| lock_error())?
                    .push_back(turn);
                let _ = self
                    .shared
                    .output_tx
                    .send(b"\r\nMuse Code is still starting; queued turn.\r\n".to_vec());
                Ok(())
            }
        }
    }

    fn require_session(&self, category: &str) -> Result<String, AgentConsoleError> {
        self.shared
            .session_id()
            .ok_or_else(|| AgentConsoleError::new(category, "Muse Code aún está iniciando"))
    }

    fn require_active_turn(&self, category: &str) -> Result<String, AgentConsoleError> {
        self.shared
            .active_turn_id
            .lock()
            .ok()
            .and_then(|turn| turn.clone())
            .ok_or_else(|| AgentConsoleError::new(category, "no hay un turno activo de Muse Code"))
    }
}

fn session_request(working_dir: &Path, resume_session_id: Option<&str>) -> Value {
    match resume_session_id {
        Some(session_id) => request(
            SESSION_REQUEST_ID,
            "session/resume",
            json!({
                "commandId": command_id(),
                "sessionId": session_id,
                "excludeItems": true,
            }),
        ),
        None => request(
            SESSION_REQUEST_ID,
            "session/start",
            json!({
                "commandId": command_id(),
                "workspaceRoot": working_dir.to_string_lossy(),
                "approvalMode": "allowAll",
            }),
        ),
    }
}

impl AgentProcess for MuseServeHandle {
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        self.child
            .try_wait()
            .map(|status| status.map(|status| status.code().unwrap_or_default()))
            .map_err(|error| AgentConsoleError::new("muse_status_failed", error.to_string()))
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        if kill_process_tree(self.child.id()).is_ok() {
            let _ = self.child.kill();
            return Ok(());
        }
        self.child
            .kill()
            .map_err(|error| AgentConsoleError::new("muse_kill_failed", error.to_string()))
    }

    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.write_input_with_options(input, None)
    }

    fn write_input_with_options(
        &mut self,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        if options.is_some() {
            self.pending_options = options;
        }
        for byte in input {
            match *byte {
                b'\r' => {
                    let text = buffered_turn_text(&self.line_buffer);
                    let options = self.pending_options.take();
                    self.line_buffer.clear();
                    if !text.is_empty() {
                        self.submit_turn(text, options)?;
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
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        // Muse reads the listed files itself, images included.
        self.submit_turn(
            prompt_with_file_attachments(text, attachments, true),
            options,
        )
    }

    fn steer_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        let session_id = self.require_session("steer_unavailable")?;
        let turn_id = self.require_active_turn("steer_unavailable")?;
        self.shared.send(
            "turn/steer",
            json!({
                "commandId": command_id(),
                "sessionId": session_id,
                "expectedTurnId": turn_id,
                "input": [{
                    "type": "text",
                    "text": prompt_with_file_attachments(text, attachments, true),
                }],
            }),
        )
    }

    fn reports_turns(&self) -> bool {
        true
    }

    fn supports_turn_interrupt(&self) -> bool {
        true
    }

    fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        let session_id = self.require_session("interrupt_unavailable")?;
        let turn_id = self.require_active_turn("interrupt_unavailable")?;
        self.shared.send(
            "turn/interrupt",
            json!({
                "commandId": command_id(),
                "sessionId": session_id,
                "turnId": turn_id,
            }),
        )
    }

    fn supports_context_compaction(&self) -> bool {
        true
    }

    fn compact_context(&mut self) -> Result<(), AgentConsoleError> {
        let session_id = self.require_session("compact_unavailable")?;
        self.shared.send(
            "session/compact",
            json!({ "commandId": command_id(), "sessionId": session_id }),
        )
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), AgentConsoleError> {
        Ok(())
    }

    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.output_reader
            .take()
            .map(|reader| Box::new(reader) as Box<dyn Read + Send>)
    }

    fn take_resume_result(&mut self) -> Option<ResumeResultReceiver> {
        self.resume_result.take()
    }

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        self.event_rx.try_iter().collect()
    }

    fn runtime_catalog(&self) -> Option<AgentRuntimeCatalog> {
        self.shared
            .runtime_catalog
            .lock()
            .ok()
            .map(|catalog| catalog.clone())
    }

    fn refresh_runtime_catalog(
        &mut self,
    ) -> Result<Option<AgentRuntimeCatalog>, AgentConsoleError> {
        write_json(
            &self.shared.stdin,
            &request(MODEL_LIST_REQUEST_ID, "model/list", json!({})),
        )?;
        Ok(self.runtime_catalog())
    }

    fn provider_session_id(&self) -> Option<String> {
        self.shared.session_id()
    }
}

fn handle_message(message: &Value, shared: &Shared) {
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        if message.get("method").is_some() {
            // A server request Tinto does not serve; answer so Muse never waits.
            let _ = write_json(
                &shared.stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": "not supported by Tinto" },
                }),
            );
            return;
        }
        handle_response(id, message, shared);
        return;
    }
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    let params = message.get("params").unwrap_or(&Value::Null);
    let ours = match (
        params.get("sessionId").and_then(Value::as_str),
        shared.session_id(),
    ) {
        (Some(session_id), Some(current)) => session_id == current,
        (Some(_), None) => false,
        (None, _) => true,
    };
    if !ours {
        return;
    }
    match method {
        "turn/started" => {
            if let Ok(mut turn) = shared.active_turn_id.lock() {
                *turn = params
                    .get("turnId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            if let Ok(mut message) = shared.last_agent_message.lock() {
                *message = None;
            }
            let _ = shared.event_tx.send(AgentProcessEvent::TurnStarted {
                timestamp_ms: now_ms(),
            });
        }
        "item/started" => {
            if let Some(item) = params.get("item") {
                remember_item_kind(shared, item);
                if let Some(text) = activity_text(item, false) {
                    shared.timeline(AgentSessionTimelineKind::Activity, &text);
                }
            }
        }
        "item/completed" => {
            let Some(item) = params.get("item") else {
                return;
            };
            if let Some(text) = activity_text(item, true) {
                shared.timeline(AgentSessionTimelineKind::Activity, &text);
            }
            if item.get("kind").and_then(Value::as_str) == Some("agentMessage") {
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| sanitize_provider_timeline_text(text.trim()))
                    .unwrap_or_default();
                if !text.is_empty() {
                    shared.timeline(AgentSessionTimelineKind::AgentProgress, &text);
                    if let Ok(mut last) = shared.last_agent_message.lock() {
                        *last = Some(text);
                    }
                }
            }
            if let Some(item_id) = item.get("itemId").and_then(Value::as_str) {
                if let Ok(mut kinds) = shared.item_kinds.lock() {
                    kinds.remove(item_id);
                }
            }
        }
        "item/delta" => {
            let is_tool_output = params
                .get("itemId")
                .and_then(Value::as_str)
                .and_then(|item_id| {
                    shared
                        .item_kinds
                        .lock()
                        .ok()
                        .and_then(|kinds| kinds.get(item_id).cloned())
                })
                .is_some_and(|kind| kind == "toolCall" || kind == "userShell");
            if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                if is_tool_output {
                    shared.timeline(AgentSessionTimelineKind::CommandOutput, delta);
                }
            }
        }
        "turn/completed" => {
            if let Some(text) = shared
                .last_agent_message
                .lock()
                .ok()
                .and_then(|mut m| m.take())
            {
                shared.timeline(AgentSessionTimelineKind::AgentMessage, &text);
            }
            if let Some(notice) = failed_turn_notice(params) {
                shared.timeline(AgentSessionTimelineKind::Lifecycle, &notice);
            }
            let _ = shared
                .output_tx
                .send(timeline_frame_for_thread_with_turn_done(
                    AgentSessionTimelineKind::Lifecycle,
                    "",
                    None,
                    true,
                ));
            if let Ok(mut turn) = shared.active_turn_id.lock() {
                *turn = None;
            }
            let _ = shared.event_tx.send(AgentProcessEvent::TurnCompleted {
                timestamp_ms: now_ms(),
            });
        }
        "session/contextUsage" => {
            let used = params.get("usedTokens").and_then(Value::as_u64);
            let window = params.get("windowTokens").and_then(Value::as_u64);
            if let (Some(used_tokens), Some(model_context_window)) = (used, window) {
                let _ = shared
                    .event_tx
                    .send(AgentProcessEvent::ContextUsageUpdated {
                        used_tokens,
                        model_context_window,
                    });
            }
        }
        "session/modelChanged" => {
            if let (Ok(mut applied), Some(model)) = (
                shared.applied_model.lock(),
                params.get("modelId").and_then(Value::as_str),
            ) {
                *applied = Some(model.to_string());
            }
        }
        "approval/requested" => approve_once(params, shared),
        "userInput/requested" => cancel_user_input(params, shared),
        _ => {}
    }
}

fn handle_response(id: u64, message: &Value, shared: &Shared) {
    let error = message
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(sanitize_provider_timeline_text);
    match id {
        INITIALIZE_REQUEST_ID => {}
        MODEL_LIST_REQUEST_ID => {
            if let Ok(mut catalog) = shared.runtime_catalog.lock() {
                apply_model_list(&mut catalog, message, error.as_deref());
            }
        }
        SESSION_REQUEST_ID => {
            let session_id = message
                .pointer("/result/session/sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
            match (session_id, error) {
                (Some(session_id), _) => {
                    if let Ok(mut slot) = shared.session_id.lock() {
                        *slot = Some(session_id.clone());
                    }
                    let pending: Vec<PendingTurn> = shared
                        .pending_turns
                        .lock()
                        .map(|mut turns| turns.drain(..).collect())
                        .unwrap_or_default();
                    for turn in pending {
                        if let Err(error) = shared.start_turn(&session_id, turn) {
                            let _ = shared.event_tx.send(AgentProcessEvent::Error { error });
                        }
                    }
                    shared.resolve_resume(Ok(AgentSessionResumeMode::Native));
                }
                (None, error) => {
                    let error = error.unwrap_or_else(|| "Muse no inició la sesión".to_string());
                    shared.resolve_resume(Err(AgentConsoleError::new(
                        "agent_resume_failed",
                        error.clone(),
                    )));
                    report_error(shared, &error);
                }
            }
        }
        _ => {
            if let Some(error) = error {
                report_error(shared, &error);
            }
        }
    }
}

fn report_error(shared: &Shared, error: &str) {
    let _ = shared.event_tx.send(AgentProcessEvent::Error {
        error: AgentConsoleError::new("provider_error", error),
    });
    let _ = shared
        .output_tx
        .send(format!("\r\nMuse Code error: {error}\r\n> ").into_bytes());
}

fn remember_item_kind(shared: &Shared, item: &Value) {
    if let (Some(item_id), Some(kind)) = (
        item.get("itemId").and_then(Value::as_str),
        item.get("kind").and_then(Value::as_str),
    ) {
        if let Ok(mut kinds) = shared.item_kinds.lock() {
            kinds.insert(item_id.to_string(), kind.to_string());
        }
    }
}

/// Muse runs with `allowAll`, so it should not ask. If a protected write
/// still needs a decision, approve it once, as Codex's `never` policy would
/// have run it, instead of leaving the turn waiting on a prompt nobody sees.
fn approve_once(params: &Value, shared: &Shared) {
    let choice = params
        .get("availableChoices")
        .and_then(Value::as_array)
        .and_then(|choices| {
            choices.iter().find(|choice| {
                choice.get("decision").and_then(Value::as_str) == Some("approved")
                    && choice.get("scope").and_then(Value::as_str) == Some("once")
            })
        })
        .and_then(|choice| choice.get("choiceId"));
    let (Some(session_id), Some(approval_id), Some(choice_id), Some(requirement)) = (
        params.get("sessionId"),
        params.get("approvalId"),
        choice,
        params.get("currentRequirementId"),
    ) else {
        return;
    };
    let _ = shared.send(
        "approval/decide",
        json!({
            "commandId": command_id(),
            "sessionId": session_id,
            "approvalId": approval_id,
            "choiceId": choice_id,
            "requirementId": requirement,
        }),
    );
}

/// Tinto has no dialog for Muse's questions; cancel them so the turn goes on.
fn cancel_user_input(params: &Value, shared: &Shared) {
    if let (Some(session_id), Some(user_input_id)) =
        (params.get("sessionId"), params.get("userInputId"))
    {
        let _ = shared.send(
            "userInput/cancel",
            json!({
                "commandId": command_id(),
                "sessionId": session_id,
                "userInputId": user_input_id,
                "reason": "Tinto no admite preguntas de Muse Code; continúa con tu mejor criterio",
            }),
        );
    }
}

fn apply_model_list(catalog: &mut AgentRuntimeCatalog, message: &Value, error: Option<&str>) {
    catalog.updated_at_ms = now_ms();
    if let Some(error) = error {
        catalog.status = AgentRuntimeCatalogStatus::Error;
        catalog.error = Some(error.to_string());
        return;
    }
    let Some(models) = message.pointer("/result/models").and_then(Value::as_array) else {
        catalog.status = AgentRuntimeCatalogStatus::Error;
        catalog.error = Some("Muse devolvió un catálogo de modelos no válido".to_string());
        return;
    };
    catalog.models = models.iter().filter_map(runtime_model).collect();
    catalog.default_model = catalog
        .models
        .iter()
        .find(|model| model.is_default)
        .map(|model| model.id.clone());
    catalog.status = AgentRuntimeCatalogStatus::Ready;
    catalog.error = None;
}

fn runtime_model(value: &Value) -> Option<AgentRuntimeModel> {
    let id = value.get("modelId")?.as_str()?.to_string();
    let display_name = value
        .get("displayLabel")
        .and_then(Value::as_str)
        .unwrap_or(&id)
        .to_string();
    // `variants` lists the efforts by tier; `reasoningEffortVariants` adds
    // descriptions when Muse has them.
    let described: HashMap<&str, &str> = value
        .get("reasoningEffortVariants")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|variant| {
            Some((
                variant.get("tier")?.as_str()?,
                variant
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ))
        })
        .collect();
    let tiers: Vec<&str> = match value.get("variants").and_then(Value::as_array) {
        Some(variants) => variants.iter().filter_map(Value::as_str).collect(),
        None => described.keys().copied().collect(),
    };
    Some(AgentRuntimeModel {
        model: id.clone(),
        display_name,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        supported_reasoning_efforts: tiers
            .iter()
            .map(|tier| AgentRuntimeReasoningEffort {
                value: tier.to_string(),
                description: described.get(tier).copied().unwrap_or("").to_string(),
            })
            .collect(),
        default_reasoning_effort: value
            .get("defaultReasoningEffort")
            .and_then(Value::as_str)
            .unwrap_or("high")
            .to_string(),
        service_tiers: Vec::new(),
        default_service_tier: None,
        is_default: value
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        id,
    })
}

fn activity_text(item: &Value, completed: bool) -> Option<String> {
    let kind = item.get("kind")?.as_str()?;
    let failed = matches!(
        item.get("status").and_then(Value::as_str),
        Some("failed" | "rejected" | "timedOut")
    );
    let text = match kind {
        "toolCall" if completed && failed => {
            let reason = item
                .get("failureReason")
                .and_then(Value::as_str)
                .unwrap_or("sin detalle");
            format!("Falló {}: {reason}", tool_label(item))
        }
        "toolCall" if !completed => format!("Usando {}", tool_label(item)),
        "userShell" if !completed => format!(
            "Ejecutando {}",
            item.get("commandText")
                .and_then(Value::as_str)
                .unwrap_or("un comando")
        ),
        "reasoning" if !completed => "Analizando el siguiente paso".to_string(),
        "compaction" if !completed => "Compactando el contexto".to_string(),
        "webSearch" if !completed => "Buscando en la web".to_string(),
        "subagent" if !completed => "Delegando en un subagente".to_string(),
        _ => return None,
    };
    Some(text.chars().take(220).collect())
}

fn tool_label(item: &Value) -> String {
    item.pointer("/displayLabel/neutral")
        .or_else(|| item.get("tool"))
        .and_then(Value::as_str)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or("una herramienta")
        .to_string()
}

fn failed_turn_notice(params: &Value) -> Option<String> {
    let terminal = params.get("terminal").and_then(Value::as_str)?;
    if terminal != "failed" {
        return None;
    }
    let reason = params
        .pointer("/error/message")
        .or_else(|| params.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or("sin detalle del proveedor");
    Some(format!("El turno falló: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_keeps_the_sandbox_and_full_access_disables_it() {
        assert_eq!(
            serve_args(AgentSessionPermissionMode::Workspace),
            ["serve", "--trust-workspace"]
        );
        assert_eq!(
            serve_args(AgentSessionPermissionMode::FullAccess),
            ["serve", "--trust-workspace", "--disable-sandbox"]
        );
    }

    #[test]
    fn new_sessions_run_without_prompts_in_the_repo() {
        let message = session_request(Path::new("/work/api"), None);
        assert_eq!(message["method"], "session/start");
        assert_eq!(message["params"]["approvalMode"], "allowAll");
        assert_eq!(message["params"]["workspaceRoot"], "/work/api");
        assert_eq!(message["jsonrpc"], "2.0");
    }

    #[test]
    fn resumed_sessions_name_the_muse_session() {
        let message = session_request(Path::new("/work/api"), Some("01a1-session"));
        assert_eq!(message["method"], "session/resume");
        assert_eq!(message["params"]["sessionId"], "01a1-session");
    }

    #[test]
    fn command_ids_are_uuid_v7() {
        let id = uuid::Uuid::parse_str(&command_id()).unwrap();
        assert_eq!(id.get_version_num(), 7);
    }

    #[test]
    fn model_list_becomes_the_runtime_catalog() {
        let mut catalog = AgentRuntimeCatalog {
            status: AgentRuntimeCatalogStatus::Loading,
            source: CATALOG_SOURCE.to_string(),
            models: Vec::new(),
            default_model: None,
            error: None,
            updated_at_ms: 0,
        };
        let message = json!({"jsonrpc":"2.0","id":2,"result":{"providerId":"meta","models":[
            {"modelId":"muse-wonder-1.0","displayLabel":"muse-wonder-1.0","isDefault":false,
             "variants":["low","high"],"defaultReasoningEffort":"high","description":null},
            {"modelId":"muse-spark-1.3","displayLabel":"muse-spark-1.3","isDefault":true,
             "variants":["minimal","max"],"reasoningEffortVariants":[{"tier":"max","description":"deepest"}]}
        ]}});
        apply_model_list(&mut catalog, &message, None);
        assert_eq!(catalog.status, AgentRuntimeCatalogStatus::Ready);
        assert_eq!(catalog.default_model.as_deref(), Some("muse-spark-1.3"));
        assert_eq!(catalog.models.len(), 2);
        let spark = &catalog.models[1];
        assert_eq!(spark.supported_reasoning_efforts.len(), 2);
        assert_eq!(spark.supported_reasoning_efforts[1].description, "deepest");
        assert_eq!(spark.default_reasoning_effort, "high");
    }

    #[test]
    fn failed_turns_say_why() {
        let params = json!({"terminal":"failed","reason":"x",
            "error":{"kind":"modelError","message":"API error 402: billing","retryable":false}});
        assert_eq!(
            failed_turn_notice(&params).as_deref(),
            Some("El turno falló: API error 402: billing")
        );
        assert_eq!(failed_turn_notice(&json!({"terminal":"completed"})), None);
    }

    #[test]
    fn tool_calls_show_their_label() {
        let started = json!({"kind":"toolCall","status":"inProgress","tool":"read_file",
            "displayLabel":{"active":"Reading","done":"Read","neutral":"Read file"}});
        assert_eq!(
            activity_text(&started, false).as_deref(),
            Some("Usando Read file")
        );
        let failed = json!({"kind":"toolCall","status":"failed","tool":"edit_file",
            "failureReason":"no exact match"});
        assert_eq!(
            activity_text(&failed, true).as_deref(),
            Some("Falló edit_file: no exact match")
        );
        assert_eq!(
            activity_text(&json!({"kind":"toolCall","status":"completed"}), true),
            None
        );
    }

    #[test]
    fn muse_runs_the_installed_binary_or_else_its_launcher() {
        let dir = std::env::temp_dir().join(format!("tinto-muse-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cmd = dir.join("muse.cmd");
        std::fs::write(&cmd, "@echo off").unwrap();
        assert_eq!(muse_program(&cmd), (cmd.clone(), Vec::new()));
        let launcher = dir.join(".muse-launcher.ps1");
        std::fs::write(&launcher, "").unwrap();
        let (program, args) = muse_program(&cmd);
        assert_eq!(program, PathBuf::from("powershell.exe"));
        assert_eq!(args.last(), Some(&launcher.into_os_string()));
        // Once installed, the binary the launcher would run, run directly.
        std::fs::write(
            dir.join(".muse-version"),
            "1.4.10-R6427.1
",
        )
        .unwrap();
        let binary = dir.join("muse-bin-1.4.10-R6427.1.exe");
        std::fs::write(&binary, "").unwrap();
        assert_eq!(muse_program(&cmd), (binary, Vec::new()));
        std::fs::write(dir.join(".muse-version"), "../evil").unwrap();
        assert_eq!(muse_program(&cmd).0, PathBuf::from("powershell.exe"));
        std::fs::remove_dir_all(dir).unwrap();
        assert!(is_muse_binary(Path::new("/usr/local/bin/muse")));
        assert!(!is_muse_binary(Path::new("/usr/local/bin/codex")));
    }
}
