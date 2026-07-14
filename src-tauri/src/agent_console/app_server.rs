use std::{
    collections::{HashSet, VecDeque},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
};

use serde_json::{json, Value};

use super::AgentConsoleError;
use super::{
    commands::TIMELINE_FRAME_PREFIX,
    pty::{kill_process_tree, AgentProcess, AgentProcessEvent},
};
use crate::bus::contract::{
    AgentRuntimeCatalog, AgentRuntimeCatalogStatus, AgentRuntimeModel, AgentRuntimeReasoningEffort,
    AgentRuntimeServiceTier, AgentSessionRuntimeOptions, AgentSessionTimelineKind,
};
use crate::wsl_agent::shell_env::agent_console_script;

#[cfg(windows)]
use crate::windows_process::hide_console;

const INITIAL_REQUEST_ID: u64 = 1;
const THREAD_START_REQUEST_ID: u64 = 2;
const FS_WATCH_REQUEST_ID: u64 = 3;
const MODEL_LIST_REQUEST_ID: u64 = 4;
const FIRST_TURN_REQUEST_ID: u64 = 100;

pub struct CodexAppServerHandle {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    output_tx: Sender<Vec<u8>>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<ChannelReader>,
    line_buffer: Vec<u8>,
    pending_options: Option<AgentSessionRuntimeOptions>,
    thread_id: Arc<Mutex<Option<String>>>,
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

impl CodexAppServerHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        let child = spawn_command(build_app_server_command(binary_path, working_dir))?;
        Self::from_child(child, working_dir, "codex_app_server")
    }

    pub fn spawn_wsl(distro: &str, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        let command = build_wsl_app_server_command(distro, working_dir)?;
        let child = spawn_command(command)?;
        let mut handle = Self::from_child(child, working_dir, "codex_app_server_wsl")?;
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

    fn from_child(
        mut child: Child,
        working_dir: &Path,
        catalog_source: &str,
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
        let pending_turns = Arc::new(Mutex::new(VecDeque::new()));
        let runtime_catalog = Arc::new(Mutex::new(AgentRuntimeCatalog {
            status: AgentRuntimeCatalogStatus::Loading,
            source: catalog_source.to_string(),
            models: Vec::new(),
            default_model: None,
            error: None,
            updated_at_ms: now_ms(),
        }));
        let pending_model_requests = Arc::new(Mutex::new(HashSet::from([MODEL_LIST_REQUEST_ID])));
        let next_request_id = Arc::new(AtomicU64::new(FIRST_TURN_REQUEST_ID));
        let cwd = working_dir.to_path_buf();

        send_initial_requests(&stdin, &cwd)?;
        spawn_stdout_thread(
            stdout,
            ServerRuntimeContext {
                output_tx: output_tx.clone(),
                event_tx,
                stdin: Arc::clone(&stdin),
                thread_id: Arc::clone(&thread_id),
                pending_turns: Arc::clone(&pending_turns),
                runtime_catalog: Arc::clone(&runtime_catalog),
                pending_model_requests: Arc::clone(&pending_model_requests),
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
            pending_turns,
            runtime_catalog,
            pending_model_requests,
            next_request_id,
            cwd,
        })
    }

    fn submit_turn(
        &mut self,
        text: String,
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
                &self.cwd,
                options.as_ref(),
            )?;
        } else if let Ok(mut pending) = self.pending_turns.lock() {
            pending.push_back(PendingTurn { text, options });
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
                        self.submit_turn(text, options)?;
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
        &json!({
            "method": "thread/start",
            "id": THREAD_START_REQUEST_ID,
            "params": {
                "cwd": cwd.to_string_lossy(),
                "runtimeWorkspaceRoots": [cwd.to_string_lossy()],
                "ephemeral": true
            }
        }),
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
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

struct PendingTurn {
    text: String,
    options: Option<AgentSessionRuntimeOptions>,
}

fn handle_server_message(message: &Value, context: &ServerRuntimeContext) {
    if handle_model_catalog_response(message, context) {
        return;
    }

    if message.get("id").and_then(Value::as_u64) == Some(THREAD_START_REQUEST_ID) {
        if let Some(id) = message
            .pointer("/result/thread/id")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if let Ok(mut slot) = context.thread_id.lock() {
                *slot = Some(id.clone());
            }
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
        let _ = context
            .output_tx
            .send(format!("\r\nCodex app-server error: {error}\r\n> ").into_bytes());
        return;
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
            }
        }
        "item/agentMessage/delta" => {
            if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                let _ = context.output_tx.send(timeline_frame(
                    AgentSessionTimelineKind::AgentMessage,
                    delta,
                ));
            }
        }
        "item/commandExecution/outputDelta" => {
            if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                let _ = context.output_tx.send(timeline_frame(
                    AgentSessionTimelineKind::CommandOutput,
                    delta,
                ));
            }
        }
        "turn/completed" => {
            let _ = context.event_tx.send(AgentProcessEvent::TurnCompleted {
                timestamp_ms: now_ms(),
            });
        }
        "turn/started" | "turn/diff/updated" | "item/fileChange/patchUpdated" | "fs/changed" => {
            let _ = context.event_tx.send(AgentProcessEvent::FileActivity {
                timestamp_ms: now_ms(),
            });
        }
        _ => {}
    }
}

fn activity_text_from_item(item: &Value, completed: bool) -> Option<String> {
    let item_type = item.get("type")?.as_str()?;
    let normalized = item_type.replace(['_', '-'], "").to_ascii_lowercase();
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
        "websearch" if !completed => "Buscando en la web".to_string(),
        "filechange" if !completed => "Aplicando cambios en archivos".to_string(),
        "plan" | "planupdate" if !completed => "Actualizando el plan".to_string(),
        _ => return None,
    };
    Some(truncate_activity_text(&text, 220))
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
            "text": text
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
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
) -> Result<(), AgentConsoleError> {
    write_json(
        stdin,
        &turn_start_message(request_id, thread_id, text, cwd, options),
    )
}

fn turn_start_message(
    request_id: u64,
    thread_id: &str,
    text: &str,
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
) -> Value {
    let mut params = json!({
        "threadId": thread_id,
        "cwd": cwd.to_string_lossy(),
        "input": [{ "type": "text", "text": text }]
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
            pending_turns: Arc::new(Mutex::new(VecDeque::new())),
            runtime_catalog: Arc::new(Mutex::new(AgentRuntimeCatalog {
                status: AgentRuntimeCatalogStatus::Loading,
                source: "codex_app_server".into(),
                models: Vec::new(),
                default_model: None,
                error: None,
                updated_at_ms: 0,
            })),
            pending_model_requests: Arc::new(Mutex::new(HashSet::new())),
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
    fn agent_delta_is_forwarded_as_timeline_frame() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"item/agentMessage/delta","params":{"delta":"hello"}}),
            &context,
        );

        let frame = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(frame.starts_with("\u{1d}TINTO_TIMELINE "));
        assert!(frame.contains("\"kind\":\"agent_message\""));
        assert!(frame.contains("\"text\":\"hello\""));
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
        assert!(frame.contains("\"text\":\"cargo test\""));
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
        assert!(frame.contains("Ejecutando npm test"));
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
        assert!(frame.contains("Analizando: Comprobando el contrato del agente"));
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
        let message =
            turn_start_message(100, "thread-1", "ship it", Path::new("/home/me/repo"), None);

        assert_eq!(message["params"]["cwd"], "/home/me/repo");
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
