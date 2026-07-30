use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use super::AgentConsoleError;
use super::{
    commands::TIMELINE_FRAME_PREFIX,
    pty::{
        kill_process_tree, prompt_with_file_attachments, AgentProcess, AgentProcessEvent,
        AgentTurnAttachment,
    },
};
use crate::bus::contract::{
    AgentRuntimeCatalog, AgentRuntimeCatalogStatus, AgentRuntimeModel, AgentRuntimeReasoningEffort,
    AgentRuntimeServiceTier, AgentSessionGoal, AgentSessionGoalStatus, AgentSessionPermissionMode,
    AgentSessionRuntimeOptions, AgentSessionTimelineKind,
};
use crate::wsl_agent::shell_env::agent_console_script;

#[cfg(windows)]
use crate::windows_process::hide_console;

const INITIAL_REQUEST_ID: u64 = 1;
const THREAD_REQUEST_ID: u64 = 2;
const FS_WATCH_REQUEST_ID: u64 = 3;
const MODEL_LIST_REQUEST_ID: u64 = 4;
const FIRST_TURN_REQUEST_ID: u64 = 100;
const CONTROL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const TURN_START_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

pub struct CodexAppServerHandle {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    output_tx: Sender<Vec<u8>>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<ChannelReader>,
    line_buffer: Vec<u8>,
    pending_options: Option<AgentSessionRuntimeOptions>,
    thread_id: Arc<Mutex<Option<String>>>,
    active_turn_id: Arc<Mutex<Option<String>>>,
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    pending_goal_updates: Arc<Mutex<VecDeque<PendingGoalUpdate>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    pending_control_requests: Arc<Mutex<HashMap<u64, Sender<Result<(), AgentConsoleError>>>>>,
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

impl CodexAppServerHandle {
    pub fn spawn(
        binary_path: &Path,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let child = spawn_command(build_app_server_command(binary_path, working_dir))?;
        Self::from_child(
            child,
            working_dir,
            "codex_app_server",
            None,
            permission_mode,
        )
    }

    pub fn resume(
        binary_path: &Path,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let child = spawn_command(build_app_server_command(binary_path, working_dir))?;
        Self::from_child(
            child,
            working_dir,
            "codex_app_server",
            Some(provider_session_id),
            permission_mode,
        )
    }

    pub fn spawn_wsl(
        distro: &str,
        working_dir: &Path,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let command = build_wsl_app_server_command(distro, working_dir)?;
        let child = spawn_command(command)?;
        let mut handle = Self::from_child(
            child,
            working_dir,
            "codex_app_server_wsl",
            None,
            permission_mode,
        )?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Some(status) = handle.child.try_wait().map_err(|error| {
            AgentConsoleError::new("app_server_status_failed", error.to_string())
        })? {
            return Err(AgentConsoleError::new(
                "app_server_exited_early",
                format!("codex app-server WSL termino durante el arranque: {status}"),
            ));
        }
        Ok(handle)
    }

    pub fn resume_wsl(
        distro: &str,
        working_dir: &Path,
        provider_session_id: &str,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let command = build_wsl_app_server_command(distro, working_dir)?;
        let child = spawn_command(command)?;
        Self::from_child(
            child,
            working_dir,
            "codex_app_server_wsl",
            Some(provider_session_id),
            permission_mode,
        )
    }

    fn from_child(
        mut child: Child,
        working_dir: &Path,
        catalog_source: &str,
        resume_thread_id: Option<&str>,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<Self, AgentConsoleError> {
        let stdin = child.stdin.take().ok_or_else(|| {
            AgentConsoleError::new(
                "app_server_spawn_failed",
                "codex app-server stdin unavailable",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentConsoleError::new(
                "app_server_spawn_failed",
                "codex app-server stdout unavailable",
            )
        })?;
        let (output_tx, output_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let stdin = Arc::new(Mutex::new(stdin));
        let thread_id = Arc::new(Mutex::new(None));
        let active_turn_id = Arc::new(Mutex::new(None));
        let completed_agent_messages = Arc::new(Mutex::new(Vec::new()));
        let pending_turns = Arc::new(Mutex::new(VecDeque::new()));
        let pending_goal_updates = Arc::new(Mutex::new(VecDeque::new()));
        let runtime_catalog = Arc::new(Mutex::new(AgentRuntimeCatalog {
            status: AgentRuntimeCatalogStatus::Loading,
            source: catalog_source.to_string(),
            models: Vec::new(),
            default_model: None,
            error: None,
            updated_at_ms: now_ms(),
        }));
        let pending_model_requests = Arc::new(Mutex::new(HashSet::from([MODEL_LIST_REQUEST_ID])));
        let pending_control_requests = Arc::new(Mutex::new(HashMap::new()));
        let next_request_id = Arc::new(AtomicU64::new(FIRST_TURN_REQUEST_ID));
        let cwd = working_dir.to_path_buf();

        send_initial_requests(&stdin, &cwd, resume_thread_id, permission_mode)?;
        spawn_stdout_thread(
            stdout,
            ServerRuntimeContext {
                output_tx: output_tx.clone(),
                event_tx,
                stdin: Arc::clone(&stdin),
                thread_id: Arc::clone(&thread_id),
                active_turn_id: Arc::clone(&active_turn_id),
                completed_agent_messages: Arc::clone(&completed_agent_messages),
                pending_turns: Arc::clone(&pending_turns),
                pending_goal_updates: Arc::clone(&pending_goal_updates),
                runtime_catalog: Arc::clone(&runtime_catalog),
                pending_model_requests: Arc::clone(&pending_model_requests),
                pending_control_requests: Arc::clone(&pending_control_requests),
                next_request_id: Arc::clone(&next_request_id),
                cwd: cwd.clone(),
            },
        );

        Ok(Self {
            child,
            stdin,
            output_tx,
            event_rx,
            output_reader: Some(ChannelReader::new(output_rx)),
            line_buffer: Vec::new(),
            pending_options: None,
            thread_id,
            active_turn_id,
            pending_turns,
            pending_goal_updates,
            runtime_catalog,
            pending_model_requests,
            pending_control_requests,
            next_request_id,
            cwd,
        })
    }

    fn submit_turn(
        &mut self,
        text: String,
        attachments: Vec<AgentTurnAttachment>,
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let thread = self.thread_id.lock().ok().and_then(|thread| thread.clone());
        if let Some(thread_id) = thread {
            let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
            send_turn_start(
                &self.stdin,
                request_id,
                &thread_id,
                &text,
                &attachments,
                &self.cwd,
                options.as_ref(),
            )?;
        } else if let Ok(mut pending) = self.pending_turns.lock() {
            pending.push_back(PendingTurn {
                text,
                attachments,
                options,
            });
            let _ = self
                .output_tx
                .send(b"\r\nCodex app-server is still initializing; queued turn.\r\n".to_vec());
        }
        Ok(())
    }

    fn write_input_inner(
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
                        self.submit_turn(text, Vec::new(), options)?;
                    }
                }
                b'\n' => {
                    self.line_buffer.push(*byte);
                }
                0x08 | 0x7f => {
                    let _ = self.line_buffer.pop();
                }
                byte if byte.is_ascii_control() => {}
                byte => {
                    self.line_buffer.push(byte);
                }
            }
        }
        Ok(())
    }

    fn submit_goal_update(&mut self, update: PendingGoalUpdate) -> Result<(), AgentConsoleError> {
        let thread = self.thread_id.lock().ok().and_then(|thread| thread.clone());
        if let Some(thread_id) = thread {
            send_goal_update(
                &self.stdin,
                self.next_request_id.fetch_add(1, Ordering::SeqCst),
                &thread_id,
                &update,
            )
        } else {
            self.pending_goal_updates
                .lock()
                .map_err(|_| {
                    AgentConsoleError::new("app_server_lock_poisoned", "goal queue lock failed")
                })?
                .push_back(update);
            Ok(())
        }
    }
}

impl AgentProcess for CodexAppServerHandle {
    fn pid(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        self.child
            .try_wait()
            .map(|status| status.map(|s| s.code().unwrap_or_default()))
            .map_err(|e| AgentConsoleError::new("app_server_status_failed", e.to_string()))
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        if kill_process_tree(self.child.id()).is_ok() {
            let _ = self.child.kill();
            return Ok(());
        }
        self.child
            .kill()
            .map_err(|e| AgentConsoleError::new("app_server_kill_failed", e.to_string()))
    }

    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.write_input_inner(input, None)
    }

    fn write_input_with_options(
        &mut self,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.write_input_inner(input, options)
    }

    fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.submit_turn(text.to_string(), attachments.to_vec(), options)
    }

    fn steer_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        let thread_id = self
            .thread_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("steer_unavailable", "Codex aún está iniciando")
            })?;
        let turn_id = self
            .active_turn_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("steer_unavailable", "no hay un turno activo que intervenir")
            })?;
        write_json(
            &self.stdin,
            &turn_steer_message(
                self.next_request_id.fetch_add(1, Ordering::SeqCst),
                &thread_id,
                &turn_id,
                text,
                attachments,
            ),
        )
    }

    fn supports_turn_interrupt(&self) -> bool {
        true
    }

    fn interrupt_turn(&mut self) -> Result<(), AgentConsoleError> {
        let thread_id = self
            .thread_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("interrupt_unavailable", "Codex aún está iniciando")
            })?;
        let turn_id = self.wait_for_active_turn_id()?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        self.send_control_request(
            request_id,
            turn_interrupt_message(request_id, &thread_id, &turn_id),
        )
    }

    fn supports_context_compaction(&self) -> bool {
        true
    }

    fn compact_context(&mut self) -> Result<(), AgentConsoleError> {
        let thread_id = self
            .thread_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("compact_unavailable", "Codex aún está iniciando")
            })?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        self.send_control_request(
            request_id,
            thread_compact_start_message(request_id, &thread_id),
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

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    fn runtime_catalog(&self) -> Option<AgentRuntimeCatalog> {
        self.runtime_catalog
            .lock()
            .ok()
            .map(|catalog| catalog.clone())
    }

    fn provider_session_id(&self) -> Option<String> {
        self.thread_id.lock().ok().and_then(|thread| thread.clone())
    }

    fn refresh_runtime_catalog(
        &mut self,
    ) -> Result<Option<AgentRuntimeCatalog>, AgentConsoleError> {
        request_model_catalog(
            &self.stdin,
            &self.runtime_catalog,
            &self.pending_model_requests,
            &self.next_request_id,
            None,
            true,
        )?;
        Ok(self.runtime_catalog())
    }

    fn supports_goals(&self) -> bool {
        true
    }

    fn update_goal(
        &mut self,
        objective: Option<&str>,
        status: Option<AgentSessionGoalStatus>,
        token_budget: Option<Option<u64>>,
    ) -> Result<(), AgentConsoleError> {
        self.submit_goal_update(PendingGoalUpdate::Set {
            objective: objective.map(str::to_string),
            status,
            token_budget,
        })
    }

    fn clear_goal(&mut self) -> Result<(), AgentConsoleError> {
        self.submit_goal_update(PendingGoalUpdate::Clear)
    }
}

impl CodexAppServerHandle {
    fn wait_for_active_turn_id(&self) -> Result<String, AgentConsoleError> {
        let deadline = Instant::now() + TURN_START_WAIT_TIMEOUT;
        loop {
            if let Some(turn_id) = self
                .active_turn_id
                .lock()
                .ok()
                .and_then(|value| value.clone())
            {
                return Ok(turn_id);
            }
            if Instant::now() >= deadline {
                return Err(AgentConsoleError::new(
                    "interrupt_unavailable",
                    "Codex no confirmó el inicio del turno",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn send_control_request(
        &self,
        request_id: u64,
        message: Value,
    ) -> Result<(), AgentConsoleError> {
        let (result_tx, result_rx) = mpsc::channel();
        let mut pending = self.pending_control_requests.lock().map_err(|_| {
            AgentConsoleError::new(
                "app_server_lock_poisoned",
                "pending control requests lock failed",
            )
        })?;
        pending.retain(|pending_id, _| pending_id.saturating_add(64) >= request_id);
        pending.insert(request_id, result_tx);
        drop(pending);
        if let Err(error) = write_json(&self.stdin, &message) {
            if let Ok(mut pending) = self.pending_control_requests.lock() {
                pending.remove(&request_id);
            }
            return Err(error);
        }
        match result_rx.recv_timeout(CONTROL_RESPONSE_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(AgentConsoleError::new(
                "provider_control_timeout",
                "Codex no confirmó la operación de control",
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(AgentConsoleError::new(
                "provider_control_failed",
                "Codex cerró la operación de control sin responder",
            )),
        }
    }
}

fn build_app_server_command(binary_path: &Path, working_dir: &Path) -> Command {
    let mut command = Command::new(binary_path);
    command
        .arg("app-server")
        .arg("--stdio")
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

pub(crate) fn build_wsl_app_server_command(
    distro: &str,
    working_dir: &Path,
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
        .arg("tinto-agent-console-app-server")
        .arg(working_dir.as_os_str())
        .arg("codex")
        .arg("app-server")
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(command)
}

fn spawn_command(mut command: Command) -> Result<Child, AgentConsoleError> {
    #[cfg(windows)]
    let command = hide_console(&mut command);
    command
        .spawn()
        .map_err(|e| AgentConsoleError::new("app_server_spawn_failed", e.to_string()))
}

fn send_initial_requests(
    stdin: &Arc<Mutex<ChildStdin>>,
    cwd: &Path,
    resume_thread_id: Option<&str>,
    permission_mode: AgentSessionPermissionMode,
) -> Result<(), AgentConsoleError> {
    write_json(
        stdin,
        &json!({
            "method": "initialize",
            "id": INITIAL_REQUEST_ID,
            "params": {
                "clientInfo": {
                    "name": "tinto",
                    "title": "Tinto",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": { "experimentalApi": true }
            }
        }),
    )?;
    write_json(stdin, &json!({ "method": "initialized", "params": {} }))?;
    write_json(
        stdin,
        &json!({
            "method": "model/list",
            "id": MODEL_LIST_REQUEST_ID,
            "params": {
                "cursor": null,
                "limit": 100,
                "includeHidden": false
            }
        }),
    )?;
    write_json(
        stdin,
        &thread_request_message(cwd, resume_thread_id, permission_mode),
    )?;
    write_json(
        stdin,
        &json!({
            "method": "fs/watch",
            "id": FS_WATCH_REQUEST_ID,
            "params": {
                "path": cwd.to_string_lossy(),
                "watchId": "tinto-session-repo"
            }
        }),
    )
}

fn thread_request_message(
    cwd: &Path,
    resume_thread_id: Option<&str>,
    permission_mode: AgentSessionPermissionMode,
) -> Value {
    let sandbox = match permission_mode {
        AgentSessionPermissionMode::Workspace => "workspaceWrite",
        AgentSessionPermissionMode::FullAccess => "dangerFullAccess",
    };
    match resume_thread_id {
        Some(thread_id) => json!({
            "method": "thread/resume",
            "id": THREAD_REQUEST_ID,
            "params": {
                "threadId": thread_id,
                "cwd": cwd.to_string_lossy(),
                "runtimeWorkspaceRoots": [cwd.to_string_lossy()],
                "approvalPolicy": "never",
                "sandbox": sandbox
            }
        }),
        None => json!({
            "method": "thread/start",
            "id": THREAD_REQUEST_ID,
            "params": {
                "cwd": cwd.to_string_lossy(),
                "runtimeWorkspaceRoots": [cwd.to_string_lossy()],
                "approvalPolicy": "never",
                "sandbox": sandbox,
                "ephemeral": false
            }
        }),
    }
}

fn spawn_stdout_thread(stdout: impl Read + Send + 'static, context: ServerRuntimeContext) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            match serde_json::from_str::<Value>(&line) {
                Ok(message) => handle_server_message(&message, &context),
                Err(_) => {
                    let _ = context
                        .output_tx
                        .send(format!("\r\n{line}\r\n").into_bytes());
                }
            }
        }
    });
}

struct ServerRuntimeContext {
    output_tx: Sender<Vec<u8>>,
    event_tx: Sender<AgentProcessEvent>,
    stdin: Arc<Mutex<ChildStdin>>,
    thread_id: Arc<Mutex<Option<String>>>,
    active_turn_id: Arc<Mutex<Option<String>>>,
    completed_agent_messages: Arc<Mutex<Vec<String>>>,
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    pending_goal_updates: Arc<Mutex<VecDeque<PendingGoalUpdate>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    pending_control_requests: Arc<Mutex<HashMap<u64, Sender<Result<(), AgentConsoleError>>>>>,
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

struct PendingTurn {
    text: String,
    attachments: Vec<AgentTurnAttachment>,
    options: Option<AgentSessionRuntimeOptions>,
}

#[derive(Debug, Clone)]
enum PendingGoalUpdate {
    Set {
        objective: Option<String>,
        status: Option<AgentSessionGoalStatus>,
        token_budget: Option<Option<u64>>,
    },
    Clear,
}

fn handle_server_message(message: &Value, context: &ServerRuntimeContext) {
    if handle_model_catalog_response(message, context) {
        return;
    }
    if handle_control_response(message, context) {
        return;
    }

    if message.get("id").and_then(Value::as_u64) == Some(THREAD_REQUEST_ID) {
        if let Some(id) = message
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if let Ok(mut slot) = context.thread_id.lock() {
                *slot = Some(id.clone());
            }
            let _ = send_goal_get(
                &context.stdin,
                context.next_request_id.fetch_add(1, Ordering::SeqCst),
                &id,
            );
            flush_pending_goal_updates(
                &context.stdin,
                &context.pending_goal_updates,
                &context.next_request_id,
                &id,
                &context.output_tx,
            );
            flush_pending_turns(
                &context.stdin,
                &context.pending_turns,
                &context.next_request_id,
                &id,
                &context.cwd,
                &context.output_tx,
            );
        }
    }

    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        let _ = context.event_tx.send(AgentProcessEvent::Error {
            error: AgentConsoleError::new("provider_error", error),
        });
        let _ = context
            .output_tx
            .send(format!("\r\nCodex app-server error: {error}\r\n> ").into_bytes());
        return;
    }

    if let Some(goal) = message.pointer("/result/goal") {
        if goal.is_null() {
            let _ = context.event_tx.send(AgentProcessEvent::GoalCleared);
        } else if let Some(goal) = agent_goal_from_value(goal) {
            let _ = context
                .event_tx
                .send(AgentProcessEvent::GoalUpdated { goal });
        }
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    match method {
        "item/started" => {
            if let Some(item) = message.pointer("/params/item") {
                if let Some(text) = activity_text_from_item(item, false) {
                    let _ = context
                        .output_tx
                        .send(timeline_frame(AgentSessionTimelineKind::Activity, &text));
                }
            }
        }
        "item/completed" => {
            if let Some(item) = message.pointer("/params/item") {
                if let Some(text) = activity_text_from_item(item, true) {
                    let _ = context
                        .output_tx
                        .send(timeline_frame(AgentSessionTimelineKind::Activity, &text));
                }
                if normalized_item_type(item).as_deref() == Some("agentmessage") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        let text = text.trim();
                        if !text.is_empty() {
                            if let Ok(mut messages) = context.completed_agent_messages.lock() {
                                messages.push(text.to_string());
                            }
                            let _ = context.output_tx.send(timeline_frame(
                                AgentSessionTimelineKind::AgentProgress,
                                text,
                            ));
                        }
                    }
                }
            }
        }
        "item/agentMessage/delta" => {}
        "item/commandExecution/outputDelta" => {
            if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                let _ = context.output_tx.send(timeline_frame(
                    AgentSessionTimelineKind::CommandOutput,
                    delta,
                ));
            }
        }
        "turn/completed" => {
            let final_message =
                context
                    .completed_agent_messages
                    .lock()
                    .ok()
                    .and_then(|mut messages| {
                        let final_message = messages.pop();
                        messages.clear();
                        final_message
                    });
            if let Some(text) = final_message {
                let _ = context.output_tx.send(timeline_frame(
                    AgentSessionTimelineKind::AgentMessage,
                    &text,
                ));
            }
            if let Ok(mut active_turn_id) = context.active_turn_id.lock() {
                *active_turn_id = None;
            }
            let _ = context.event_tx.send(AgentProcessEvent::TurnCompleted {
                timestamp_ms: now_ms(),
            });
        }
        "thread/goal/updated" => {
            if let Some(goal) = message
                .pointer("/params/goal")
                .and_then(agent_goal_from_value)
            {
                let _ = context
                    .event_tx
                    .send(AgentProcessEvent::GoalUpdated { goal });
            }
        }
        "thread/goal/cleared" => {
            let _ = context.event_tx.send(AgentProcessEvent::GoalCleared);
        }
        "turn/started" => {
            if let Ok(mut messages) = context.completed_agent_messages.lock() {
                messages.clear();
            }
            if let Some(turn_id) = message.pointer("/params/turn/id").and_then(Value::as_str) {
                if let Ok(mut active_turn_id) = context.active_turn_id.lock() {
                    *active_turn_id = Some(turn_id.to_string());
                }
            }
            let _ = context.event_tx.send(AgentProcessEvent::FileActivity {
                timestamp_ms: now_ms(),
            });
        }
        "thread/tokenUsage/updated" => {
            if let Some((used_tokens, model_context_window)) = context_usage_from_message(message) {
                let _ = context
                    .event_tx
                    .send(AgentProcessEvent::ContextUsageUpdated {
                        used_tokens,
                        model_context_window,
                    });
            }
        }
        "turn/diff/updated" | "item/fileChange/patchUpdated" | "fs/changed" => {
            let _ = context.event_tx.send(AgentProcessEvent::FileActivity {
                timestamp_ms: now_ms(),
            });
        }
        _ => {}
    }
}

fn handle_control_response(message: &Value, context: &ServerRuntimeContext) -> bool {
    let Some(request_id) = message.get("id").and_then(Value::as_u64) else {
        return false;
    };
    let sender = context
        .pending_control_requests
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&request_id));
    let Some(sender) = sender else {
        return false;
    };
    let result = match message.pointer("/error/message").and_then(Value::as_str) {
        Some(error) => Err(AgentConsoleError::new(
            "provider_control_failed",
            error.to_string(),
        )),
        None => Ok(()),
    };
    let _ = sender.send(result);
    true
}

fn activity_text_from_item(item: &Value, completed: bool) -> Option<String> {
    let normalized = normalized_item_type(item)?;
    let text = match normalized.as_str() {
        "commandexecution" if !completed => {
            let command = value_as_text(item.get("command"))?;
            format!("Ejecutando {command}")
        }
        "reasoning" if completed => reasoning_summary(item)
            .map(|summary| format!("Analizando: {summary}"))
            .unwrap_or_else(|| "Analizando el siguiente paso".to_string()),
        "reasoning" => "Analizando el siguiente paso".to_string(),
        "mcptoolcall" if !completed => {
            let server = item
                .get("server")
                .or_else(|| item.get("serverName"))
                .and_then(Value::as_str);
            let tool = item
                .get("tool")
                .or_else(|| item.get("toolName"))
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("herramienta MCP");
            match server {
                Some(server) => format!("Usando {server} / {tool}"),
                None => format!("Usando {tool}"),
            }
        }
        "contextcompaction" if !completed => "Compactando el contexto".to_string(),
        "websearch" if !completed => "Buscando en la web".to_string(),
        "filechange" if !completed => "Aplicando cambios en archivos".to_string(),
        "plan" | "planupdate" if !completed => "Actualizando el plan".to_string(),
        _ => return None,
    };
    Some(truncate_activity_text(&text, 220))
}

fn normalized_item_type(item: &Value) -> Option<String> {
    Some(
        item.get("type")?
            .as_str()?
            .replace(['_', '-'], "")
            .to_ascii_lowercase(),
    )
}

fn value_as_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Array(parts) => {
            let text = parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            (!text.trim().is_empty()).then(|| text.trim().to_string())
        }
        _ => None,
    }
}

fn reasoning_summary(item: &Value) -> Option<String> {
    let summary = item.get("summary")?;
    if let Some(text) = summary.as_str() {
        return (!text.trim().is_empty()).then(|| text.trim().to_string());
    }
    let text = summary
        .as_array()?
        .iter()
        .filter_map(|entry| {
            entry
                .as_str()
                .or_else(|| entry.get("text").and_then(Value::as_str))
        })
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.is_empty()).then_some(text)
}

fn truncate_activity_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn handle_model_catalog_response(message: &Value, context: &ServerRuntimeContext) -> bool {
    let Some(request_id) = message.get("id").and_then(Value::as_u64) else {
        return false;
    };
    let is_model_request = context
        .pending_model_requests
        .lock()
        .ok()
        .map(|mut pending| pending.remove(&request_id))
        .unwrap_or(false);
    if !is_model_request {
        return false;
    }

    if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
        update_catalog_error(&context.runtime_catalog, error);
        return true;
    }

    let Some(data) = message.pointer("/result/data").and_then(Value::as_array) else {
        update_catalog_error(
            &context.runtime_catalog,
            "Codex devolvio un catalogo de modelos no valido",
        );
        return true;
    };
    let models = data
        .iter()
        .filter_map(parse_runtime_model)
        .collect::<Vec<_>>();
    if let Ok(mut catalog) = context.runtime_catalog.lock() {
        for model in models {
            if let Some(existing) = catalog.models.iter_mut().find(|item| item.id == model.id) {
                *existing = model;
            } else {
                catalog.models.push(model);
            }
        }
        catalog.updated_at_ms = now_ms();
    }

    let next_cursor = message
        .pointer("/result/nextCursor")
        .and_then(Value::as_str)
        .filter(|cursor| !cursor.is_empty());
    if let Some(cursor) = next_cursor {
        if let Err(error) = request_model_catalog(
            &context.stdin,
            &context.runtime_catalog,
            &context.pending_model_requests,
            &context.next_request_id,
            Some(cursor),
            false,
        ) {
            update_catalog_error(&context.runtime_catalog, &error.message);
        }
        return true;
    }

    if let Ok(mut catalog) = context.runtime_catalog.lock() {
        catalog.default_model = catalog
            .models
            .iter()
            .find(|model| model.is_default)
            .map(|model| model.id.clone());
        catalog.status = AgentRuntimeCatalogStatus::Ready;
        catalog.error = None;
        catalog.updated_at_ms = now_ms();
    }
    true
}

fn request_model_catalog(
    stdin: &Arc<Mutex<ChildStdin>>,
    runtime_catalog: &Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: &Arc<Mutex<HashSet<u64>>>,
    next_request_id: &Arc<AtomicU64>,
    cursor: Option<&str>,
    refresh: bool,
) -> Result<(), AgentConsoleError> {
    let request_id = next_request_id.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut catalog) = runtime_catalog.lock() {
        catalog.status = AgentRuntimeCatalogStatus::Loading;
        catalog.error = None;
        catalog.updated_at_ms = now_ms();
        if refresh {
            catalog.models.clear();
            catalog.default_model = None;
        }
    }
    let mut pending = pending_model_requests.lock().map_err(|_| {
        AgentConsoleError::new("app_server_lock_poisoned", "model request lock failed")
    })?;
    if refresh {
        pending.clear();
    }
    pending.insert(request_id);
    drop(pending);
    let result = write_json(
        stdin,
        &json!({
            "method": "model/list",
            "id": request_id,
            "params": {
                "cursor": cursor,
                "limit": 100,
                "includeHidden": false
            }
        }),
    );
    if result.is_err() {
        if let Ok(mut pending) = pending_model_requests.lock() {
            pending.remove(&request_id);
        }
    }
    result
}

fn update_catalog_error(catalog: &Arc<Mutex<AgentRuntimeCatalog>>, message: &str) {
    if let Ok(mut catalog) = catalog.lock() {
        catalog.status = AgentRuntimeCatalogStatus::Error;
        catalog.error = Some(message.to_string());
        catalog.updated_at_ms = now_ms();
    }
}

fn parse_runtime_model(value: &Value) -> Option<AgentRuntimeModel> {
    let id = value.get("id").and_then(Value::as_str)?.to_string();
    let supported_reasoning_efforts = value
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|effort| {
            Some(AgentRuntimeReasoningEffort {
                value: effort.get("reasoningEffort")?.as_str()?.to_string(),
                description: effort
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect::<Vec<_>>();
    let mut service_tiers = Vec::new();
    for key in ["serviceTiers", "additionalSpeedTiers"] {
        for tier in value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let parsed = if let Some(id) = tier.as_str() {
                Some(AgentRuntimeServiceTier {
                    id: id.to_string(),
                    name: id.to_string(),
                    description: String::new(),
                })
            } else {
                tier.get("id")
                    .and_then(Value::as_str)
                    .map(|id| AgentRuntimeServiceTier {
                        id: id.to_string(),
                        name: tier
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .to_string(),
                        description: tier
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    })
            };
            if let Some(parsed) = parsed {
                if !service_tiers
                    .iter()
                    .any(|existing: &AgentRuntimeServiceTier| existing.id == parsed.id)
                {
                    service_tiers.push(parsed);
                }
            }
        }
    }
    let default_reasoning_effort = value
        .get("defaultReasoningEffort")
        .and_then(Value::as_str)
        .unwrap_or("medium")
        .to_string();
    Some(AgentRuntimeModel {
        model: value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string(),
        display_name: value
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        supported_reasoning_efforts,
        default_reasoning_effort,
        service_tiers,
        default_service_tier: value
            .get("defaultServiceTier")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_default: value
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        id,
    })
}

fn timeline_frame(kind: AgentSessionTimelineKind, text: &str) -> Vec<u8> {
    let mut frame = TIMELINE_FRAME_PREFIX.to_vec();
    frame.extend(
        serde_json::to_vec(&json!({
            "kind": kind,
            "text_base64": STANDARD.encode(text.as_bytes())
        }))
        .unwrap_or_else(|_| {
            b"{\"kind\":\"lifecycle\",\"text\":\"timeline encode failed\"}".to_vec()
        }),
    );
    frame.push(b'\n');
    frame
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn buffered_turn_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn agent_goal_from_value(value: &Value) -> Option<AgentSessionGoal> {
    let status = match value.get("status")?.as_str()? {
        "active" => AgentSessionGoalStatus::Active,
        "paused" => AgentSessionGoalStatus::Paused,
        "blocked" => AgentSessionGoalStatus::Blocked,
        "usageLimited" => AgentSessionGoalStatus::UsageLimited,
        "budgetLimited" => AgentSessionGoalStatus::BudgetLimited,
        "complete" => AgentSessionGoalStatus::Complete,
        _ => return None,
    };
    Some(AgentSessionGoal {
        text: value.get("objective")?.as_str()?.to_string(),
        status,
        token_budget: value
            .get("tokenBudget")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok()),
        tokens_used: value
            .get("tokensUsed")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0),
        time_used_seconds: value
            .get("timeUsedSeconds")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0),
        created_at_ms: value
            .get("createdAt")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0)
            .saturating_mul(1_000),
        updated_at_ms: value
            .get("updatedAt")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0)
            .saturating_mul(1_000),
    })
}

fn goal_status_name(status: AgentSessionGoalStatus) -> &'static str {
    match status {
        AgentSessionGoalStatus::Active => "active",
        AgentSessionGoalStatus::Paused => "paused",
        AgentSessionGoalStatus::Blocked => "blocked",
        AgentSessionGoalStatus::UsageLimited => "usageLimited",
        AgentSessionGoalStatus::BudgetLimited => "budgetLimited",
        AgentSessionGoalStatus::Complete => "complete",
    }
}

fn send_goal_get(
    stdin: &Arc<Mutex<ChildStdin>>,
    request_id: u64,
    thread_id: &str,
) -> Result<(), AgentConsoleError> {
    write_json(
        stdin,
        &json!({
            "method": "thread/goal/get",
            "id": request_id,
            "params": { "threadId": thread_id }
        }),
    )
}

fn send_goal_update(
    stdin: &Arc<Mutex<ChildStdin>>,
    request_id: u64,
    thread_id: &str,
    update: &PendingGoalUpdate,
) -> Result<(), AgentConsoleError> {
    match update {
        PendingGoalUpdate::Clear => write_json(
            stdin,
            &json!({
                "method": "thread/goal/clear",
                "id": request_id,
                "params": { "threadId": thread_id }
            }),
        ),
        PendingGoalUpdate::Set {
            objective,
            status,
            token_budget,
        } => {
            let mut params = json!({ "threadId": thread_id });
            if let Some(objective) = objective {
                params["objective"] = json!(objective);
            }
            if let Some(status) = status {
                params["status"] = json!(goal_status_name(*status));
            }
            if let Some(token_budget) = token_budget {
                params["tokenBudget"] = token_budget.map_or(Value::Null, |value| json!(value));
            }
            write_json(
                stdin,
                &json!({
                    "method": "thread/goal/set",
                    "id": request_id,
                    "params": params
                }),
            )
        }
    }
}

fn flush_pending_goal_updates(
    stdin: &Arc<Mutex<ChildStdin>>,
    pending_goal_updates: &Arc<Mutex<VecDeque<PendingGoalUpdate>>>,
    next_request_id: &Arc<AtomicU64>,
    thread_id: &str,
    output_tx: &Sender<Vec<u8>>,
) {
    let Some(mut pending) = pending_goal_updates.lock().ok() else {
        return;
    };
    while let Some(update) = pending.pop_front() {
        if let Err(error) = send_goal_update(
            stdin,
            next_request_id.fetch_add(1, Ordering::SeqCst),
            thread_id,
            &update,
        ) {
            let _ = output_tx
                .send(format!("\r\nCodex app-server error: {}\r\n> ", error.message).into_bytes());
            break;
        }
    }
}

fn flush_pending_turns(
    stdin: &Arc<Mutex<ChildStdin>>,
    pending_turns: &Arc<Mutex<VecDeque<PendingTurn>>>,
    next_request_id: &Arc<AtomicU64>,
    thread_id: &str,
    cwd: &Path,
    output_tx: &Sender<Vec<u8>>,
) {
    let Some(mut pending) = pending_turns.lock().ok() else {
        return;
    };
    while let Some(text) = pending.pop_front() {
        let request_id = next_request_id.fetch_add(1, Ordering::SeqCst);
        if let Err(error) = send_turn_start(
            stdin,
            request_id,
            thread_id,
            &text.text,
            &text.attachments,
            cwd,
            text.options.as_ref(),
        ) {
            let _ = output_tx
                .send(format!("\r\nCodex app-server error: {}\r\n> ", error.message).into_bytes());
            break;
        }
    }
}

fn send_turn_start(
    stdin: &Arc<Mutex<ChildStdin>>,
    request_id: u64,
    thread_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
) -> Result<(), AgentConsoleError> {
    write_json(
        stdin,
        &turn_start_message(request_id, thread_id, text, attachments, cwd, options),
    )
}

fn turn_start_message(
    request_id: u64,
    thread_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
) -> Value {
    let mut input = attachments
        .iter()
        .filter(|attachment| attachment.is_image)
        .map(
            |attachment| json!({ "type": "localImage", "path": attachment.path.to_string_lossy() }),
        )
        .collect::<Vec<_>>();
    input.push(json!({
        "type": "text",
        "text": prompt_with_file_attachments(text, attachments, false)
    }));
    let mut params = json!({
        "threadId": thread_id,
        "cwd": cwd.to_string_lossy(),
        "input": input
    });
    if let Some(model) = options.and_then(|options| options.model.as_deref()) {
        params["model"] = json!(model);
    }
    if let Some(effort) = options.and_then(|options| options.reasoning_effort.as_deref()) {
        params["effort"] = json!(effort);
    }
    if let Some(service_tier) = options
        .and_then(|options| options.speed.as_deref())
        .filter(|speed| *speed != "standard")
    {
        params["serviceTier"] = json!(service_tier);
    }
    json!({
        "method": "turn/start",
        "id": request_id,
        "params": params
    })
}

fn turn_steer_message(
    request_id: u64,
    thread_id: &str,
    turn_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
) -> Value {
    let mut input = attachments
        .iter()
        .filter(|attachment| attachment.is_image)
        .map(
            |attachment| json!({ "type": "localImage", "path": attachment.path.to_string_lossy() }),
        )
        .collect::<Vec<_>>();
    input.push(json!({
        "type": "text",
        "text": prompt_with_file_attachments(text, attachments, false)
    }));
    json!({
        "method": "turn/steer",
        "id": request_id,
        "params": {
            "threadId": thread_id,
            "expectedTurnId": turn_id,
            "input": input
        }
    })
}

fn turn_interrupt_message(request_id: u64, thread_id: &str, turn_id: &str) -> Value {
    json!({
        "method": "turn/interrupt",
        "id": request_id,
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
        }
    })
}

fn thread_compact_start_message(request_id: u64, thread_id: &str) -> Value {
    json!({
        "method": "thread/compact/start",
        "id": request_id,
        "params": {
            "threadId": thread_id,
        }
    })
}

fn context_usage_from_message(message: &Value) -> Option<(u64, u64)> {
    Some((
        message
            .pointer("/params/tokenUsage/last/totalTokens")?
            .as_u64()?,
        message
            .pointer("/params/tokenUsage/modelContextWindow")?
            .as_u64()?,
    ))
}

fn write_json(stdin: &Arc<Mutex<ChildStdin>>, value: &Value) -> Result<(), AgentConsoleError> {
    let mut line = serde_json::to_vec(value)
        .map_err(|e| AgentConsoleError::new("app_server_encode_failed", e.to_string()))?;
    line.push(b'\n');
    let mut stdin = stdin
        .lock()
        .map_err(|_| AgentConsoleError::new("app_server_lock_poisoned", "stdin lock failed"))?;
    stdin
        .write_all(&line)
        .and_then(|_| stdin.flush())
        .map_err(|e| AgentConsoleError::new("app_server_write_failed", e.to_string()))
}

struct ChannelReader {
    rx: Receiver<Vec<u8>>,
    buffer: VecDeque<u8>,
    closed: bool,
}

impl ChannelReader {
    fn new(rx: Receiver<Vec<u8>>) -> Self {
        Self {
            rx,
            buffer: VecDeque::new(),
            closed: false,
        }
    }
}

impl Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.buffer.is_empty() && !self.closed {
            match self.rx.recv() {
                Ok(bytes) => self.buffer.extend(bytes),
                Err(_) => self.closed = true,
            }
        }
        let mut read = 0;
        while read < out.len() {
            let Some(byte) = self.buffer.pop_front() else {
                break;
            };
            out[read] = byte;
            read += 1;
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};

    #[allow(clippy::zombie_processes)]
    fn dummy_stdin() -> Arc<Mutex<ChildStdin>> {
        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args(if cfg!(windows) {
                vec!["/C", "more"]
            } else {
                vec!["-c", "cat >/dev/null"]
            })
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        Arc::new(Mutex::new(child.stdin.take().unwrap()))
    }

    fn dummy_context(
        output_tx: Sender<Vec<u8>>,
        event_tx: Sender<AgentProcessEvent>,
    ) -> ServerRuntimeContext {
        ServerRuntimeContext {
            output_tx,
            event_tx,
            stdin: dummy_stdin(),
            thread_id: Arc::new(Mutex::new(Some("t".into()))),
            active_turn_id: Arc::new(Mutex::new(None)),
            completed_agent_messages: Arc::new(Mutex::new(Vec::new())),
            pending_turns: Arc::new(Mutex::new(VecDeque::new())),
            pending_goal_updates: Arc::new(Mutex::new(VecDeque::new())),
            runtime_catalog: Arc::new(Mutex::new(AgentRuntimeCatalog {
                status: AgentRuntimeCatalogStatus::Loading,
                source: "codex_app_server".into(),
                models: Vec::new(),
                default_model: None,
                error: None,
                updated_at_ms: 0,
            })),
            pending_model_requests: Arc::new(Mutex::new(HashSet::new())),
            pending_control_requests: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(AtomicU64::new(FIRST_TURN_REQUEST_ID)),
            cwd: PathBuf::from("/tmp/repo"),
        }
    }

    #[test]
    fn completed_turn_emits_structured_event_for_checkpoint_close() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u"}}}),
            &context,
        );

        assert!(matches!(
            event_rx.recv().unwrap(),
            AgentProcessEvent::TurnCompleted { .. }
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn turn_notifications_track_the_active_turn_for_steering() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"turn-7"}}}),
            &context,
        );
        assert_eq!(
            context.active_turn_id.lock().unwrap().as_deref(),
            Some("turn-7")
        );
        handle_server_message(
            &json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn-7"}}}),
            &context,
        );
        assert!(context.active_turn_id.lock().unwrap().is_none());
    }

    #[test]
    fn steer_message_uses_the_active_turn_precondition() {
        let message = turn_steer_message(101, "thread-1", "turn-7", "Ajusta también esto", &[]);
        assert_eq!(message["method"], "turn/steer");
        assert_eq!(message["params"]["threadId"], "thread-1");
        assert_eq!(message["params"]["expectedTurnId"], "turn-7");
        assert_eq!(message["params"]["input"][0]["text"], "Ajusta también esto");
    }

    #[test]
    fn interrupt_message_targets_the_active_thread_and_turn() {
        let message = turn_interrupt_message(102, "thread-1", "turn-7");
        assert_eq!(message["method"], "turn/interrupt");
        assert_eq!(message["params"]["threadId"], "thread-1");
        assert_eq!(message["params"]["turnId"], "turn-7");
    }

    #[test]
    fn compact_message_targets_the_active_thread() {
        let message = thread_compact_start_message(103, "thread-1");
        assert_eq!(message["method"], "thread/compact/start");
        assert_eq!(message["params"]["threadId"], "thread-1");
    }

    #[test]
    fn control_response_resolves_only_the_matching_request() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        let (result_tx, result_rx) = mpsc::channel();
        context
            .pending_control_requests
            .lock()
            .unwrap()
            .insert(103, result_tx);

        handle_server_message(&json!({"id": 103, "result": {}}), &context);

        assert_eq!(result_rx.recv().unwrap(), Ok(()));
        assert!(context.pending_control_requests.lock().unwrap().is_empty());
    }

    #[test]
    fn rejected_control_response_does_not_emit_a_session_error() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        let (result_tx, result_rx) = mpsc::channel();
        context
            .pending_control_requests
            .lock()
            .unwrap()
            .insert(104, result_tx);

        handle_server_message(
            &json!({"id": 104, "error": {"message": "turn already completed"}}),
            &context,
        );

        assert_eq!(
            result_rx.recv().unwrap(),
            Err(AgentConsoleError::new(
                "provider_control_failed",
                "turn already completed"
            ))
        );
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn token_usage_update_emits_context_usage() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);

        handle_server_message(
            &json!({
                "method": "thread/tokenUsage/updated",
                "params": {
                    "tokenUsage": {
                        "last": { "totalTokens": 120_000 },
                        "modelContextWindow": 100_000
                    }
                }
            }),
            &context,
        );

        assert_eq!(
            event_rx.recv().unwrap(),
            AgentProcessEvent::ContextUsageUpdated {
                used_tokens: 120_000,
                model_context_window: 100_000,
            }
        );
    }

    #[test]
    fn context_compaction_is_visible_activity() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);

        handle_server_message(
            &json!({
                "method": "item/started",
                "params": { "item": { "type": "contextCompaction" } }
            }),
            &context,
        );

        let frame = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(frame.contains("\"kind\":\"activity\""));
        assert!(frame.contains(&STANDARD.encode("Compactando el contexto")));
    }

    #[test]
    fn native_goal_update_is_forwarded_with_progress_and_status() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "method": "thread/goal/updated",
                "params": {
                    "threadId": "t",
                    "goal": {
                        "threadId": "t",
                        "objective": "Ship goal mode",
                        "status": "paused",
                        "tokenBudget": 200000,
                        "tokensUsed": 45000,
                        "timeUsedSeconds": 321,
                        "createdAt": 1760000000,
                        "updatedAt": 1760000321
                    }
                }
            }),
            &context,
        );

        let AgentProcessEvent::GoalUpdated { goal } = event_rx.recv().unwrap() else {
            panic!("expected goal update");
        };
        assert_eq!(goal.text, "Ship goal mode");
        assert_eq!(goal.status, AgentSessionGoalStatus::Paused);
        assert_eq!(goal.token_budget, Some(200_000));
        assert_eq!(goal.tokens_used, 45_000);
        assert_eq!(goal.time_used_seconds, 321);
        assert_eq!(goal.updated_at_ms, 1_760_000_321_000);
    }

    #[test]
    fn native_goal_clear_is_forwarded() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"thread/goal/cleared","params":{"threadId":"t"}}),
            &context,
        );

        assert!(matches!(
            event_rx.recv().unwrap(),
            AgentProcessEvent::GoalCleared
        ));
    }

    #[test]
    fn fs_changed_emits_structured_activity_event() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"fs/changed","params":{"watchId":"w","changedPaths":["/tmp/a"]}}),
            &context,
        );

        assert!(matches!(
            event_rx.recv().unwrap(),
            AgentProcessEvent::FileActivity { .. }
        ));
    }

    #[test]
    fn completed_agent_messages_are_promoted_from_progress_to_final_output() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "method":"item/completed",
                "params":{"item":{"id":"message-1","type":"agentMessage","text":"**Hello** world"}}
            }),
            &context,
        );

        let progress = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(progress.contains("\"kind\":\"agent_progress\""));
        assert!(progress.contains(&STANDARD.encode("**Hello** world")));

        handle_server_message(
            &json!({"method":"turn/completed","params":{"turn":{"id":"turn-1"}}}),
            &context,
        );

        let final_message = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(final_message.contains("\"kind\":\"agent_message\""));
        assert!(final_message.contains(&STANDARD.encode("**Hello** world")));
        assert!(matches!(
            event_rx.recv().unwrap(),
            AgentProcessEvent::TurnCompleted { .. }
        ));
    }

    #[test]
    fn command_delta_is_forwarded_as_command_timeline_frame() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"item/commandExecution/outputDelta","params":{"delta":"cargo test"}}),
            &context,
        );

        let frame = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(frame.starts_with("\u{1d}TINTO_TIMELINE "));
        assert!(frame.contains("\"kind\":\"command_output\""));
        assert!(frame.contains(&format!(
            "\"text_base64\":\"{}\"",
            STANDARD.encode("cargo test")
        )));
    }

    #[test]
    fn command_start_is_forwarded_as_provider_neutral_activity() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "method":"item/started",
                "params":{"item":{"id":"cmd-1","type":"commandExecution","command":"npm test"}}
            }),
            &context,
        );

        let frame = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(frame.contains("\"kind\":\"activity\""));
        assert!(frame.contains(&STANDARD.encode("Ejecutando npm test")));
    }

    #[test]
    fn reasoning_completion_forwards_summary_without_raw_reasoning() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "method":"item/completed",
                "params":{"item":{
                    "id":"reason-1",
                    "type":"reasoning",
                    "summary":[{"text":"Comprobando el contrato del agente"}],
                    "content":[{"type":"reasoningText","text":"contenido privado"}]
                }}
            }),
            &context,
        );

        let frame = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(frame.contains("\"kind\":\"activity\""));
        assert!(frame.contains(&STANDARD.encode("Analizando: Comprobando el contrato del agente")));
        assert!(!frame.contains("contenido privado"));
    }

    #[test]
    fn model_list_response_populates_dynamic_runtime_catalog() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        context
            .pending_model_requests
            .lock()
            .unwrap()
            .insert(MODEL_LIST_REQUEST_ID);

        handle_server_message(
            &json!({
                "id": MODEL_LIST_REQUEST_ID,
                "result": {
                    "data": [{
                        "id": "gpt-5.6-sol",
                        "model": "gpt-5.6-sol",
                        "displayName": "GPT-5.6 Sol",
                        "description": "",
                        "supportedReasoningEfforts": [
                            {"reasoningEffort": "medium", "description": ""},
                            {"reasoningEffort": "high", "description": ""}
                        ],
                        "defaultReasoningEffort": "medium",
                        "serviceTiers": [
                            {"id": "fast", "name": "Fast", "description": ""}
                        ],
                        "isDefault": true
                    }],
                    "nextCursor": null
                }
            }),
            &context,
        );

        let catalog = context.runtime_catalog.lock().unwrap().clone();
        assert_eq!(catalog.status, AgentRuntimeCatalogStatus::Ready);
        assert_eq!(catalog.default_model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(catalog.models[0].display_name, "GPT-5.6 Sol");
        assert_eq!(
            catalog.models[0].supported_reasoning_efforts[1].value,
            "high"
        );
        assert_eq!(catalog.models[0].service_tiers[0].id, "fast");
    }

    #[test]
    fn model_list_error_is_kept_out_of_the_transcript() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        context
            .pending_model_requests
            .lock()
            .unwrap()
            .insert(MODEL_LIST_REQUEST_ID);

        handle_server_message(
            &json!({"id": MODEL_LIST_REQUEST_ID, "error": {"message": "offline"}}),
            &context,
        );

        let catalog = context.runtime_catalog.lock().unwrap().clone();
        assert_eq!(catalog.status, AgentRuntimeCatalogStatus::Error);
        assert_eq!(catalog.error.as_deref(), Some("offline"));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn turn_start_forwards_model_effort_and_fast_service_tier() {
        let message = turn_start_message(
            100,
            "thread-1",
            "ship it",
            &[],
            Path::new("/tmp/repo"),
            Some(&AgentSessionRuntimeOptions {
                model: Some("gpt-5.6-sol".into()),
                reasoning_effort: Some("high".into()),
                speed: Some("fast".into()),
            }),
        );

        assert_eq!(message["params"]["model"], "gpt-5.6-sol");
        assert_eq!(message["params"]["effort"], "high");
        assert_eq!(message["params"]["serviceTier"], "fast");
    }

    #[test]
    fn wsl_app_server_command_keeps_codex_and_repo_inside_the_distro() {
        let command = build_wsl_app_server_command("Ubuntu", Path::new("/home/me/repo")).unwrap();
        let args = command
            .get_args()
            .map(OsStr::to_os_string)
            .collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("wsl.exe"));
        assert_eq!(
            &args[..5],
            [
                OsString::from("-d"),
                OsString::from("Ubuntu"),
                OsString::from("--exec"),
                OsString::from("bash"),
                OsString::from("-lc"),
            ]
        );
        assert!(args[5].to_string_lossy().contains("$nvm_dir/nvm.sh"));
        assert!(args[5]
            .to_string_lossy()
            .contains("/mnt/[A-Za-z]/*) continue"));
        assert_eq!(
            &args[6..],
            [
                OsString::from("tinto-agent-console-app-server"),
                OsString::from("/home/me/repo"),
                OsString::from("codex"),
                OsString::from("app-server"),
                OsString::from("--stdio"),
            ]
        );
        assert!(command.get_current_dir().is_none());
    }

    #[test]
    fn wsl_app_server_command_rejects_an_empty_distro() {
        let error = build_wsl_app_server_command(" ", Path::new("/home/me/repo")).unwrap_err();

        assert_eq!(error.category, "missing_distro");
    }

    #[test]
    fn turn_start_preserves_a_linux_working_directory() {
        let message = turn_start_message(
            100,
            "thread-1",
            "ship it",
            &[],
            Path::new("/home/me/repo"),
            None,
        );

        assert_eq!(message["params"]["cwd"], "/home/me/repo");
    }

    #[test]
    fn turn_start_sends_local_images_before_the_text_prompt() {
        let message = turn_start_message(
            100,
            "thread-1",
            "inspect this",
            &[AgentTurnAttachment {
                path: PathBuf::from("/tmp/screenshot.png"),
                is_image: true,
            }],
            Path::new("/tmp/repo"),
            None,
        );

        assert_eq!(message["params"]["input"][0]["type"], "localImage");
        assert_eq!(message["params"]["input"][0]["path"], "/tmp/screenshot.png");
        assert_eq!(message["params"]["input"][1]["type"], "text");
        assert_eq!(message["params"]["input"][1]["text"], "inspect this");
    }

    #[test]
    fn turn_start_mentions_generic_files_in_the_text_prompt() {
        let message = turn_start_message(
            100,
            "thread-1",
            "summarize it",
            &[AgentTurnAttachment {
                path: PathBuf::from("/tmp/brief.pdf"),
                is_image: false,
            }],
            Path::new("/tmp/repo"),
            None,
        );

        assert_eq!(message["params"]["input"].as_array().unwrap().len(), 1);
        assert_eq!(message["params"]["input"][0]["type"], "text");
        assert_eq!(
            message["params"]["input"][0]["text"],
            "# Files mentioned by the user:\n- /tmp/brief.pdf\n\nsummarize it"
        );
    }

    #[test]
    fn starts_persistent_threads_for_future_resume() {
        let message = thread_request_message(
            Path::new("/tmp/repo"),
            None,
            AgentSessionPermissionMode::Workspace,
        );

        assert_eq!(message["method"], "thread/start");
        assert_eq!(message["params"]["ephemeral"], false);
        assert_eq!(message["params"]["approvalPolicy"], "never");
        assert_eq!(message["params"]["sandbox"], "workspaceWrite");
    }

    #[test]
    fn starts_full_access_threads_only_when_selected() {
        let message = thread_request_message(
            Path::new("/tmp/repo"),
            None,
            AgentSessionPermissionMode::FullAccess,
        );

        assert_eq!(message["params"]["approvalPolicy"], "never");
        assert_eq!(message["params"]["sandbox"], "dangerFullAccess");
    }

    #[test]
    fn resumes_the_provider_thread_in_the_current_workspace() {
        let message = thread_request_message(
            Path::new("/tmp/repo"),
            Some("thread-42"),
            AgentSessionPermissionMode::Workspace,
        );

        assert_eq!(message["method"], "thread/resume");
        assert_eq!(message["params"]["threadId"], "thread-42");
        assert_eq!(
            message["params"]["cwd"],
            Path::new("/tmp/repo").to_string_lossy().as_ref()
        );
        assert_eq!(message["params"]["approvalPolicy"], "never");
        assert_eq!(message["params"]["sandbox"], "workspaceWrite");
    }

    #[test]
    fn channel_reader_streams_queued_chunks() {
        let (tx, rx) = mpsc::channel();
        tx.send(b"abc".to_vec()).unwrap();
        drop(tx);
        let mut reader = ChannelReader::new(rx);
        let mut bytes = Vec::new();

        reader.read_to_end(&mut bytes).unwrap();

        assert_eq!(bytes, b"abc");
    }

    #[test]
    fn buffered_turn_text_preserves_utf8_input() {
        assert_eq!(
            buffered_turn_text(b"caf\xc3\xa9\n").as_bytes(),
            b"caf\xc3\xa9"
        );
    }
}
