use std::{
    collections::VecDeque,
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
    pty::{AgentProcess, AgentProcessEvent},
};
use crate::bus::contract::{AgentSessionRuntimeOptions, AgentSessionTimelineKind};

#[cfg(windows)]
use crate::windows_process::hide_console;

const INITIAL_REQUEST_ID: u64 = 1;
const THREAD_START_REQUEST_ID: u64 = 2;
const FS_WATCH_REQUEST_ID: u64 = 3;
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
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

impl CodexAppServerHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, AgentConsoleError> {
        let mut child = spawn_app_server(binary_path, working_dir)?;
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
}

fn spawn_app_server(binary_path: &Path, working_dir: &Path) -> Result<Child, AgentConsoleError> {
    let mut command = Command::new(binary_path);
    command
        .arg("app-server")
        .arg("--stdio")
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
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
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

struct PendingTurn {
    text: String,
    options: Option<AgentSessionRuntimeOptions>,
}

fn handle_server_message(message: &Value, context: &ServerRuntimeContext) {
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
    write_json(
        stdin,
        &json!({
            "method": "turn/start",
            "id": request_id,
            "params": params
        }),
    )
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
