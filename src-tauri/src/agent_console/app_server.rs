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

use super::{
    commands::TIMELINE_FRAME_PREFIX,
    pty::{
        kill_process_tree, prompt_with_file_attachments, AgentProcess, AgentProcessEvent,
        AgentTurnAttachment,
    },
    sanitize_provider_timeline_text, AgentConsoleError, ResumeResultReceiver, MAX_SUBAGENT_THREADS,
};
use crate::bus::contract::{
    AgentRuntimeCatalog, AgentRuntimeCatalogStatus, AgentRuntimeModel, AgentRuntimeReasoningEffort,
    AgentRuntimeServiceTier, AgentSessionGoal, AgentSessionGoalStatus, AgentSessionPermissionMode,
    AgentSessionResumeMode, AgentSessionRuntimeOptions, AgentSessionTimelineKind,
    AgentSubagentActivity, AgentSubagentCapabilities, AgentSubagentResult, AgentSubagentThread,
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
const MAX_DISCOVERY_PAGES: usize = 128;
const MAX_COMPLETED_MESSAGES_PER_THREAD: usize = 256;
const MAX_DISCOVERED_CHILDREN: usize = MAX_SUBAGENT_THREADS;
const MAX_TRACKED_THREAD_STATES: usize = MAX_DISCOVERED_CHILDREN + 1;
const MAX_DISCOVERY_CURSOR_CHARS: usize = 512;
const MAX_SUBAGENT_ID_CHARS: usize = 256;

type PendingControlRequests = Arc<Mutex<HashMap<u64, Sender<Result<(), AgentConsoleError>>>>>;

#[derive(Debug, Clone)]
struct PendingDiscovery {
    root_thread_id: String,
    page: usize,
    run_id: u64,
}

type PendingDiscoveryRequests = Arc<Mutex<HashMap<u64, PendingDiscovery>>>;

fn completed_messages_for_thread(
    messages: &mut HashMap<String, Vec<String>>,
    thread_id: String,
    is_root_thread: bool,
) -> Option<&mut Vec<String>> {
    if !messages.contains_key(&thread_id) {
        let limit = if is_root_thread {
            MAX_TRACKED_THREAD_STATES
        } else {
            MAX_DISCOVERED_CHILDREN
        };
        if messages.len() >= limit {
            return None;
        }
    }
    Some(messages.entry(thread_id).or_default())
}

fn track_active_turn(
    active_turn_ids: &mut HashMap<String, String>,
    thread_id: &str,
    turn_id: &str,
) -> bool {
    if !active_turn_ids.contains_key(thread_id) && active_turn_ids.len() >= MAX_DISCOVERED_CHILDREN
    {
        return false;
    }
    active_turn_ids.insert(thread_id.to_string(), turn_id.to_string());
    true
}
type PendingCollaborations = Arc<Mutex<HashMap<String, String>>>;

#[derive(Debug, Default)]
struct DiscoveryState {
    run_id: u64,
    seen_cursors: HashSet<String>,
    child_ids: HashSet<String>,
}

pub struct CodexAppServerHandle {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    output_tx: Sender<Vec<u8>>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<ChannelReader>,
    resume_result: Option<ResumeResultReceiver>,
    line_buffer: Vec<u8>,
    pending_options: Option<AgentSessionRuntimeOptions>,
    permission_mode: Arc<Mutex<AgentSessionPermissionMode>>,
    thread_id: Arc<Mutex<Option<String>>>,
    active_turn_id: Arc<Mutex<Option<String>>>,
    active_turn_ids: Arc<Mutex<HashMap<String, String>>>,
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    pending_goal_updates: Arc<Mutex<VecDeque<PendingGoalUpdate>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    pending_control_requests: PendingControlRequests,
    pending_thread_requests: Arc<Mutex<HashMap<u64, String>>>,
    pending_discovery_requests: PendingDiscoveryRequests,
    discovery_state: Arc<Mutex<DiscoveryState>>,
    pending_collaborations: PendingCollaborations,
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
        let completed_agent_messages = Arc::new(Mutex::new(HashMap::new()));
        let active_turn_ids = Arc::new(Mutex::new(HashMap::new()));
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
        let pending_thread_requests = Arc::new(Mutex::new(HashMap::new()));
        let pending_discovery_requests = Arc::new(Mutex::new(HashMap::new()));
        let discovery_state = Arc::new(Mutex::new(DiscoveryState::default()));
        let pending_collaborations = Arc::new(Mutex::new(HashMap::new()));
        let next_request_id = Arc::new(AtomicU64::new(FIRST_TURN_REQUEST_ID));
        let cwd = working_dir.to_path_buf();
        let (resume_tx, resume_result) = if resume_thread_id.is_some() {
            let (tx, rx) = mpsc::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        send_initial_requests(&stdin, &cwd, resume_thread_id, permission_mode)?;
        let permission_mode = Arc::new(Mutex::new(permission_mode));
        spawn_stdout_thread(
            stdout,
            ServerRuntimeContext {
                output_tx: output_tx.clone(),
                event_tx,
                stdin: Arc::clone(&stdin),
                thread_id: Arc::clone(&thread_id),
                active_turn_id: Arc::clone(&active_turn_id),
                active_turn_ids: Arc::clone(&active_turn_ids),
                completed_agent_messages: Arc::clone(&completed_agent_messages),
                pending_turns: Arc::clone(&pending_turns),
                pending_goal_updates: Arc::clone(&pending_goal_updates),
                runtime_catalog: Arc::clone(&runtime_catalog),
                pending_model_requests: Arc::clone(&pending_model_requests),
                pending_control_requests: Arc::clone(&pending_control_requests),
                pending_thread_requests: Arc::clone(&pending_thread_requests),
                pending_discovery_requests: Arc::clone(&pending_discovery_requests),
                discovery_state: Arc::clone(&discovery_state),
                pending_collaborations: Arc::clone(&pending_collaborations),
                resume_tx: Mutex::new(resume_tx),
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
            resume_result,
            line_buffer: Vec::new(),
            pending_options: None,
            permission_mode,
            thread_id,
            active_turn_id,
            active_turn_ids,
            pending_turns,
            pending_goal_updates,
            runtime_catalog,
            pending_model_requests,
            pending_control_requests,
            pending_thread_requests,
            pending_discovery_requests,
            discovery_state,
            pending_collaborations,
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
        let permission_mode = self.permission_mode.lock().map(|mode| *mode).map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "permission mode lock failed")
        })?;
        let thread = self.thread_id.lock().ok().and_then(|thread| thread.clone());
        let turn = PendingTurn {
            text,
            attachments,
            options,
            permission_mode,
        };
        if let Some(thread_id) = thread {
            let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
            send_turn_start(&self.stdin, request_id, &thread_id, &turn, &self.cwd)?;
        } else if let Ok(mut pending) = self.pending_turns.lock() {
            pending.push_back(turn);
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

    fn supports_subagents(&self) -> bool {
        true
    }

    fn discover_subagents(&mut self) -> Result<(), AgentConsoleError> {
        let root_thread_id = self
            .thread_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("subagent_discovery_unavailable", "Codex aún está iniciando")
            })?;
        request_subagent_discovery(
            &self.stdin,
            &self.pending_discovery_requests,
            &self.discovery_state,
            &self.next_request_id,
            &root_thread_id,
            None,
            1,
            None,
        )
    }

    fn write_subagent_turn(
        &mut self,
        thread_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        let permission_mode = self.permission_mode.lock().map(|mode| *mode).map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "permission mode lock failed")
        })?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        send_turn_start(
            &self.stdin,
            request_id,
            thread_id,
            &PendingTurn {
                text: text.to_string(),
                attachments: attachments.to_vec(),
                options,
                permission_mode,
            },
            &self.cwd,
        )
    }

    fn write_subagent_turn_with_metadata(
        &mut self,
        thread_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
        permission_mode: AgentSessionPermissionMode,
        approval_policy: Option<&str>,
    ) -> Result<(), AgentConsoleError> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let mut pending_requests = self.pending_thread_requests.lock().map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "request lock failed")
        })?;
        pending_requests.retain(|pending_id, _| pending_id.saturating_add(64) >= request_id);
        pending_requests.insert(request_id, thread_id.to_string());
        drop(pending_requests);
        let result = send_turn_start_with_approval(
            &self.stdin,
            request_id,
            thread_id,
            &PendingTurn {
                text: text.to_string(),
                attachments: attachments.to_vec(),
                options,
                permission_mode,
            },
            &self.cwd,
            approval_policy,
        );
        if result.is_err() {
            if let Ok(mut requests) = self.pending_thread_requests.lock() {
                requests.remove(&request_id);
            }
        }
        result
    }

    fn steer_subagent_turn(
        &mut self,
        thread_id: &str,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        let turn_id = self
            .active_turn_ids
            .lock()
            .ok()
            .and_then(|turns| turns.get(thread_id).cloned())
            .ok_or_else(|| {
                AgentConsoleError::new(
                    "subagent_steer_unavailable",
                    "Codex no confirmó un turno activo para este subagente",
                )
            })?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        self.send_control_request(
            request_id,
            turn_steer_message(request_id, thread_id, &turn_id, text, attachments),
        )
    }

    fn interrupt_subagent_turn(&mut self, thread_id: &str) -> Result<(), AgentConsoleError> {
        let turn_id = self
            .active_turn_ids
            .lock()
            .ok()
            .and_then(|turns| turns.get(thread_id).cloned())
            .ok_or_else(|| {
                AgentConsoleError::new(
                    "subagent_interrupt_unavailable",
                    "Codex no confirmó un turno activo para este subagente",
                )
            })?;
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        self.send_control_request(
            request_id,
            turn_interrupt_message(request_id, thread_id, &turn_id),
        )
    }

    fn wait_subagent(&mut self, thread_id: &str) -> Result<(), AgentConsoleError> {
        self.send_parent_collaboration_instruction("wait", thread_id)
    }

    fn close_subagent(&mut self, thread_id: &str) -> Result<(), AgentConsoleError> {
        self.send_parent_collaboration_instruction("close", thread_id)
    }

    fn supports_permission_mode_change(&self) -> bool {
        true
    }

    fn set_permission_mode(
        &mut self,
        permission_mode: AgentSessionPermissionMode,
    ) -> Result<(), AgentConsoleError> {
        if let Some(status) = self.child.try_wait().map_err(|error| {
            AgentConsoleError::new("app_server_status_failed", error.to_string())
        })? {
            return Err(AgentConsoleError::new(
                "permission_mode_unavailable",
                format!("Codex app-server ya terminó ({status})"),
            ));
        }
        let mut selected = self.permission_mode.lock().map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "permission mode lock failed")
        })?;
        *selected = permission_mode;
        Ok(())
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

    fn take_resume_result(&mut self) -> Option<ResumeResultReceiver> {
        self.resume_result.take()
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
    fn send_parent_collaboration_instruction(
        &mut self,
        action: &str,
        child_thread_id: &str,
    ) -> Result<(), AgentConsoleError> {
        if !valid_provider_thread_id(child_thread_id) {
            return Err(AgentConsoleError::new(
                "invalid_subagent_id",
                "id de subagente no valido",
            ));
        }
        let parent_thread_id = self
            .thread_id
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .ok_or_else(|| {
                AgentConsoleError::new("subagent_control_unavailable", "Codex aún está iniciando")
            })?;
        let text = match action {
            "wait" => format!(
                "Use the Codex waitAgent collaboration action for child thread {child_thread_id}, then report its result."
            ),
            "close" => format!(
                "Use the Codex closeAgent collaboration action for child thread {child_thread_id}. Keep its transcript inspectable and do not archive or delete the thread."
            ),
            _ => return Err(AgentConsoleError::new("subagent_control_invalid", "acción no soportada")),
        };
        if parent_thread_id == child_thread_id {
            return Err(AgentConsoleError::new(
                "subagent_control_invalid",
                "el hilo raiz no puede controlarse como descendiente",
            ));
        }
        let permission_mode = self.permission_mode.lock().map(|mode| *mode).map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "permission mode lock failed")
        })?;
        if let Some(turn_id) = self
            .active_turn_id
            .lock()
            .ok()
            .and_then(|turn_id| turn_id.clone())
        {
            let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
            let result = self.send_control_request(
                request_id,
                turn_steer_message(request_id, &parent_thread_id, &turn_id, &text, &[]),
            );
            if result.is_ok() {
                if let Ok(mut pending) = self.pending_collaborations.lock() {
                    pending.insert(child_thread_id.to_string(), action.to_string());
                }
            }
            return result;
        }
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let result = send_turn_start(
            &self.stdin,
            request_id,
            &parent_thread_id,
            &PendingTurn {
                text,
                attachments: Vec::new(),
                options: None,
                permission_mode,
            },
            &self.cwd,
        );
        if result.is_ok() {
            if let Ok(mut pending) = self.pending_collaborations.lock() {
                pending.insert(child_thread_id.to_string(), action.to_string());
            }
        }
        result
    }

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
        AgentSessionPermissionMode::Workspace => "workspace-write",
        AgentSessionPermissionMode::FullAccess => "danger-full-access",
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
    active_turn_ids: Arc<Mutex<HashMap<String, String>>>,
    completed_agent_messages: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pending_turns: Arc<Mutex<VecDeque<PendingTurn>>>,
    pending_goal_updates: Arc<Mutex<VecDeque<PendingGoalUpdate>>>,
    runtime_catalog: Arc<Mutex<AgentRuntimeCatalog>>,
    pending_model_requests: Arc<Mutex<HashSet<u64>>>,
    pending_control_requests: PendingControlRequests,
    pending_thread_requests: Arc<Mutex<HashMap<u64, String>>>,
    pending_discovery_requests: PendingDiscoveryRequests,
    discovery_state: Arc<Mutex<DiscoveryState>>,
    pending_collaborations: PendingCollaborations,
    resume_tx: Mutex<Option<Sender<Result<AgentSessionResumeMode, AgentConsoleError>>>>,
    next_request_id: Arc<AtomicU64>,
    cwd: PathBuf,
}

struct PendingTurn {
    text: String,
    attachments: Vec<AgentTurnAttachment>,
    options: Option<AgentSessionRuntimeOptions>,
    permission_mode: AgentSessionPermissionMode,
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
    if handle_subagent_discovery_response(message, context) {
        return;
    }

    if message.get("id").and_then(Value::as_u64) == Some(THREAD_REQUEST_ID) {
        if let Some(id) = message
            .pointer("/result/thread/id")
            .and_then(|value| provider_string(Some(value), MAX_SUBAGENT_ID_CHARS))
            .filter(|id| valid_provider_thread_id(id))
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
            let _ = request_subagent_discovery(
                &context.stdin,
                &context.pending_discovery_requests,
                &context.discovery_state,
                &context.next_request_id,
                &id,
                None,
                1,
                None,
            );
            resolve_resume_result(context, Ok(AgentSessionResumeMode::Native));
        } else if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
            resolve_resume_result(
                context,
                Err(AgentConsoleError::new("agent_resume_failed", error)),
            );
        }
    }

    let target_thread_id = message_thread_id(message, context);
    if let Some(request_id) = message.get("id").and_then(Value::as_u64) {
        if let Ok(mut requests) = context.pending_thread_requests.lock() {
            requests.remove(&request_id);
        }
    }
    let root_thread_id = context
        .thread_id
        .lock()
        .ok()
        .and_then(|thread| thread.clone());
    let has_explicit_thread_id = message.pointer("/params/threadId").is_some()
        || message.pointer("/params/thread/id").is_some()
        || message.pointer("/params/item/threadId").is_some()
        || message.pointer("/params/request/threadId").is_some()
        || message.pointer("/params/request/thread_id").is_some()
        || message.pointer("/params/request/thread/id").is_some()
        || message.pointer("/params/request/params/threadId").is_some()
        || message
            .pointer("/params/request/params/thread_id")
            .is_some()
        || message.pointer("/params/serverRequest/threadId").is_some()
        || message.pointer("/params/serverRequest/thread_id").is_some()
        || message.pointer("/params/serverRequest/thread/id").is_some();
    let is_root_thread = if has_explicit_thread_id {
        target_thread_id
            .as_deref()
            .is_some_and(|target| root_thread_id.as_deref() == Some(target))
    } else {
        target_thread_id
            .as_deref()
            .map(|target| root_thread_id.as_deref() == Some(target))
            .unwrap_or(true)
    };
    if has_explicit_thread_id && target_thread_id.is_none() {
        return;
    }

    if let Some(error) = message
        .pointer("/error/message")
        .and_then(|value| provider_string(Some(value), 4_000))
    {
        if is_root_thread {
            let _ = context.event_tx.send(AgentProcessEvent::Error {
                error: AgentConsoleError::new("provider_error", error.clone()),
            });
            let _ = context
                .output_tx
                .send(format!("\r\nCodex app-server error: {error}\r\n> ").into_bytes());
        } else if let Some(thread_id) = target_thread_id {
            let _ = context.event_tx.send(AgentProcessEvent::SubagentUpdated {
                subagent: errored_subagent(thread_id, &error),
            });
        }
        return;
    }

    if is_root_thread {
        if let Some(goal) = message.pointer("/result/goal") {
            if goal.is_null() {
                let _ = context.event_tx.send(AgentProcessEvent::GoalCleared);
            } else if let Some(goal) = agent_goal_from_value(goal) {
                let _ = context
                    .event_tx
                    .send(AgentProcessEvent::GoalUpdated { goal });
            }
        }
    }

    let Some(method) = message.get("method").and_then(Value::as_str) else {
        return;
    };
    match method {
        "item/started" => {
            if let Some(item) = message.pointer("/params/item") {
                emit_subagent_collaboration(context, item, false);
                if let Some(text) = activity_text_from_item(item, false) {
                    let _ = context.output_tx.send(timeline_frame_for_thread(
                        AgentSessionTimelineKind::Activity,
                        &text,
                        target_thread_id.as_deref().filter(|_| !is_root_thread),
                    ));
                }
                emit_subagent_activity(
                    context,
                    target_thread_id.as_deref().filter(|_| !is_root_thread),
                    item,
                    false,
                );
            }
        }
        "item/completed" => {
            if let Some(item) = message.pointer("/params/item") {
                emit_subagent_collaboration(context, item, true);
                if let Some(text) = activity_text_from_item(item, true) {
                    let _ = context.output_tx.send(timeline_frame_for_thread(
                        AgentSessionTimelineKind::Activity,
                        &text,
                        target_thread_id.as_deref().filter(|_| !is_root_thread),
                    ));
                }
                emit_subagent_activity(
                    context,
                    target_thread_id.as_deref().filter(|_| !is_root_thread),
                    item,
                    true,
                );
                if normalized_item_type(item).as_deref() == Some("agentmessage") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        let text = sanitize_provider_timeline_text(text.trim());
                        if !text.is_empty() {
                            if let Ok(mut messages) = context.completed_agent_messages.lock() {
                                let key = target_thread_id
                                    .clone()
                                    .or_else(|| {
                                        context.thread_id.lock().ok().and_then(|id| id.clone())
                                    })
                                    .unwrap_or_default();
                                if let Some(thread_messages) = completed_messages_for_thread(
                                    &mut messages,
                                    key,
                                    is_root_thread,
                                ) {
                                    thread_messages.push(text.clone());
                                    if thread_messages.len() > MAX_COMPLETED_MESSAGES_PER_THREAD {
                                        let overflow = thread_messages.len()
                                            - MAX_COMPLETED_MESSAGES_PER_THREAD;
                                        thread_messages.drain(0..overflow);
                                    }
                                }
                            }
                            let _ = context.output_tx.send(timeline_frame_for_thread(
                                AgentSessionTimelineKind::AgentProgress,
                                &text,
                                target_thread_id.as_deref().filter(|_| !is_root_thread),
                            ));
                        }
                    }
                }
            }
        }
        "item/agentMessage/delta" => {
            if let (Some(thread_id), Some(delta)) = (
                target_thread_id.as_deref().filter(|_| !is_root_thread),
                message.pointer("/params/delta").and_then(Value::as_str),
            ) {
                emit_subagent_delta(
                    context,
                    thread_id,
                    message.pointer("/params/itemId").and_then(Value::as_str),
                    delta,
                );
                let _ = context.output_tx.send(timeline_frame_for_thread(
                    AgentSessionTimelineKind::AgentProgress,
                    delta,
                    Some(thread_id),
                ));
            }
        }
        "item/commandExecution/outputDelta" => {
            if let Some(delta) = message.pointer("/params/delta").and_then(Value::as_str) {
                let _ = context.output_tx.send(timeline_frame_for_thread(
                    AgentSessionTimelineKind::CommandOutput,
                    delta,
                    target_thread_id.as_deref().filter(|_| !is_root_thread),
                ));
            }
        }
        "turn/completed" => {
            let key = target_thread_id
                .clone()
                .or_else(|| context.thread_id.lock().ok().and_then(|id| id.clone()))
                .unwrap_or_default();
            let final_message =
                context
                    .completed_agent_messages
                    .lock()
                    .ok()
                    .and_then(|mut messages| {
                        let value = messages.get_mut(&key).and_then(|items| items.pop());
                        if messages.get(&key).is_some_and(Vec::is_empty) {
                            messages.remove(&key);
                        }
                        value
                    });
            if let Some(text) = final_message {
                let _ = context.output_tx.send(timeline_frame_for_thread(
                    AgentSessionTimelineKind::AgentMessage,
                    &text,
                    target_thread_id.as_deref().filter(|_| !is_root_thread),
                ));
            }
            if is_root_thread {
                if let Ok(mut active_turn_id) = context.active_turn_id.lock() {
                    *active_turn_id = None;
                }
            }
            if let Some(thread_id) = target_thread_id.as_deref().filter(|_| !is_root_thread) {
                if let Ok(mut active_turn_ids) = context.active_turn_ids.lock() {
                    active_turn_ids.remove(thread_id);
                }
                let turn = message.pointer("/params/turn");
                let status = turn
                    .and_then(|turn| turn.get("status"))
                    .map(|status| provider_status(Some(status), "completed"))
                    .unwrap_or_else(|| "completed".to_string());
                let mut update = subagent_status_update(thread_id, "unknown", &status);
                update.result = turn.and_then(result_from_turn);
                let _ = context
                    .event_tx
                    .send(AgentProcessEvent::SubagentUpdated { subagent: update });
            }
            if is_root_thread {
                let _ = context.event_tx.send(AgentProcessEvent::TurnCompleted {
                    timestamp_ms: now_ms(),
                });
            }
        }
        "thread/goal/updated" => {
            if !is_root_thread {
                return;
            }
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
            if !is_root_thread {
                return;
            }
            let _ = context.event_tx.send(AgentProcessEvent::GoalCleared);
        }
        "turn/started" => {
            let key = target_thread_id
                .clone()
                .or_else(|| context.thread_id.lock().ok().and_then(|id| id.clone()))
                .unwrap_or_default();
            if let Ok(mut messages) = context.completed_agent_messages.lock() {
                if let Some(thread_messages) =
                    completed_messages_for_thread(&mut messages, key, is_root_thread)
                {
                    thread_messages.clear();
                }
            }
            if let Some(turn_id) = message.pointer("/params/turn/id").and_then(Value::as_str) {
                if is_root_thread {
                    if let Ok(mut active_turn_id) = context.active_turn_id.lock() {
                        *active_turn_id = Some(turn_id.to_string());
                    }
                }
                if let Some(thread_id) = target_thread_id.as_deref().filter(|_| !is_root_thread) {
                    if let Ok(mut active_turn_ids) = context.active_turn_ids.lock() {
                        track_active_turn(&mut active_turn_ids, thread_id, turn_id);
                    }
                }
            }
            if is_root_thread {
                let _ = context.event_tx.send(AgentProcessEvent::FileActivity {
                    timestamp_ms: now_ms(),
                });
            } else if let Some(thread_id) = target_thread_id {
                let _ = context.event_tx.send(AgentProcessEvent::SubagentUpdated {
                    subagent: subagent_status_update(&thread_id, "running", "working"),
                });
            }
        }
        "thread/tokenUsage/updated" => {
            if !is_root_thread {
                return;
            }
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
            if is_root_thread {
                let _ = context.event_tx.send(AgentProcessEvent::FileActivity {
                    timestamp_ms: now_ms(),
                });
            }
        }
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/mcpToolCall/requestApproval"
        | "item/permissions/requestApproval" => {
            if let Some(thread_id) = target_thread_id.as_deref().filter(|_| !is_root_thread) {
                emit_subagent_approval(context, thread_id, message);
            }
        }
        "serverRequest" => {
            if is_server_approval_request(message) {
                if let Some(thread_id) = target_thread_id.as_deref().filter(|_| !is_root_thread) {
                    emit_subagent_approval(context, thread_id, message);
                }
            }
        }
        "serverRequest/resolved" => {
            if let Some(thread_id) = target_thread_id.as_deref().filter(|_| !is_root_thread) {
                emit_subagent_approval_resolved(context, thread_id, message);
            }
        }
        "thread/started" | "thread/status/changed" | "thread/closed" => {
            if let Some(subagent) = subagent_from_message(message, method) {
                if root_thread_id.as_deref() == Some(subagent.id.as_str()) {
                    return;
                }
                let _ = context
                    .event_tx
                    .send(AgentProcessEvent::SubagentUpdated { subagent });
            }
        }
        _ => {}
    }
}

fn resolve_resume_result(
    context: &ServerRuntimeContext,
    result: Result<AgentSessionResumeMode, AgentConsoleError>,
) {
    if let Ok(mut resume_tx) = context.resume_tx.lock() {
        if let Some(resume_tx) = resume_tx.take() {
            let _ = resume_tx.send(result);
        }
    }
}

fn handle_subagent_discovery_response(message: &Value, context: &ServerRuntimeContext) -> bool {
    if message.get("method").is_some() {
        return false;
    }
    let Some(request_id) = message.get("id").and_then(Value::as_u64) else {
        return false;
    };
    let pending = context
        .pending_discovery_requests
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&request_id));
    let Some(pending) = pending else {
        return false;
    };
    let root = Some(pending.root_thread_id.clone());
    let run_is_current = context
        .discovery_state
        .lock()
        .ok()
        .is_some_and(|state| state.run_id == pending.run_id);
    if !run_is_current {
        // A new explicit discovery supersedes all older pages. Do not let a
        // delayed response consume the new run's cursor/capacity budget.
        return true;
    }
    if let Some(error) = message
        .pointer("/error/message")
        .and_then(|value| provider_string(Some(value), 4_000))
    {
        let _ = context
            .event_tx
            .send(AgentProcessEvent::SubagentDiscoveryFailed {
                error: AgentConsoleError::new("subagent_discovery_failed", error),
            });
        return true;
    }
    let threads = message
        .pointer("/result/data")
        .or_else(|| message.pointer("/result/threads"))
        .and_then(Value::as_array);
    let Some(threads) = threads else {
        let _ = context
            .event_tx
            .send(AgentProcessEvent::SubagentDiscoveryFailed {
                error: AgentConsoleError::new(
                    "subagent_discovery_failed",
                    "Codex devolvió un listado de subagentes no válido",
                ),
            });
        return true;
    };
    let discovered = threads
        .iter()
        .filter_map(|thread| subagent_from_thread(thread, None))
        .filter(|subagent| {
            subagent.id != pending.root_thread_id
                && subagent.id != root.as_deref().unwrap_or_default()
        })
        .collect::<Vec<_>>();
    let total = {
        let Ok(mut state) = context.discovery_state.lock() else {
            return true;
        };
        let mut new_count = 0usize;
        for subagent in &discovered {
            if state.child_ids.insert(subagent.id.clone()) {
                new_count = new_count.saturating_add(1);
            }
        }
        state.child_ids.len().max(new_count)
    };
    if total > MAX_DISCOVERED_CHILDREN {
        let _ = context
            .event_tx
            .send(AgentProcessEvent::SubagentDiscoveryFailed {
                error: AgentConsoleError::new(
                    "subagent_capacity_exceeded",
                    "Codex devolvio mas descendientes de los permitidos",
                ),
            });
        return true;
    }
    for subagent in discovered {
        let _ = context
            .event_tx
            .send(AgentProcessEvent::SubagentUpdated { subagent });
    }
    let next_cursor = message
        .pointer("/result/nextCursor")
        .or_else(|| message.pointer("/result/next_cursor"))
        .and_then(|cursor| provider_string(Some(cursor), MAX_DISCOVERY_CURSOR_CHARS));
    if let (Some(root), Some(cursor)) = (root.as_deref(), next_cursor) {
        if let Err(error) = request_subagent_discovery(
            &context.stdin,
            &context.pending_discovery_requests,
            &context.discovery_state,
            &context.next_request_id,
            root,
            Some(&cursor),
            pending.page.saturating_add(1),
            Some(pending.run_id),
        ) {
            let _ = context
                .event_tx
                .send(AgentProcessEvent::SubagentDiscoveryFailed { error });
        }
    }
    true
}

fn message_thread_id(message: &Value, context: &ServerRuntimeContext) -> Option<String> {
    let explicit = message
        .pointer("/params/threadId")
        .or_else(|| message.pointer("/params/thread/id"))
        .or_else(|| message.pointer("/params/item/threadId"))
        .or_else(|| message.pointer("/params/request/threadId"))
        .or_else(|| message.pointer("/params/request/thread_id"))
        .or_else(|| message.pointer("/params/request/thread/id"))
        .or_else(|| message.pointer("/params/request/params/threadId"))
        .or_else(|| message.pointer("/params/request/params/thread_id"))
        .or_else(|| message.pointer("/params/serverRequest/threadId"))
        .or_else(|| message.pointer("/params/serverRequest/thread_id"))
        .or_else(|| message.pointer("/params/serverRequest/thread/id"));
    match explicit {
        Some(value) => provider_string(Some(value), MAX_SUBAGENT_ID_CHARS)
            .filter(|id| valid_provider_thread_id(id)),
        None => message
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|request_id| {
                context
                    .pending_thread_requests
                    .lock()
                    .ok()
                    .and_then(|requests| requests.get(&request_id).cloned())
            })
            .or_else(|| {
                context
                    .thread_id
                    .lock()
                    .ok()
                    .and_then(|thread| thread.clone())
            }),
    }
}

fn provider_string(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text.chars().take(max_chars).collect())
}

fn provider_identifier(value: Option<&Value>, max_chars: usize) -> Option<String> {
    provider_string(value, max_chars).or_else(|| {
        value
            .and_then(Value::as_u64)
            .map(|number| number.to_string())
    })
}

fn provider_status(value: Option<&Value>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_string();
    };
    if let Some(status) = provider_string(Some(value), 96) {
        return status;
    }
    for key in ["type", "status", "state", "kind"] {
        if let Some(status) = provider_string(value.get(key), 96) {
            return status;
        }
    }
    fallback.to_string()
}

fn provider_timestamp_ms(value: Option<&Value>) -> Option<u64> {
    let timestamp = value?.as_u64()?;
    // Codex Thread.updatedAt is seconds; notification fixtures may already
    // use milliseconds. Avoid overflow and retain the provider ordering.
    Some(if timestamp < 10_000_000_000 {
        timestamp.saturating_mul(1_000)
    } else {
        timestamp
    })
}

fn is_terminal_provider_status(status: &str) -> bool {
    matches!(
        status,
        "closed" | "completed" | "errored" | "failed" | "interrupted" | "exited" | "systemError"
    )
}

fn valid_provider_thread_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= MAX_SUBAGENT_ID_CHARS
        && !id.chars().any(char::is_control)
        && !id.contains('/')
        && !id.contains('\\')
}

fn explicit_bool(value: Option<&Value>, key: &str) -> Option<bool> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
}

fn subagent_capabilities(
    value: Option<&Value>,
    active: bool,
    can_accept_direct_input: Option<bool>,
) -> AgentSubagentCapabilities {
    let capabilities = value.and_then(|value| {
        value
            .get("capabilities")
            .or_else(|| value.is_object().then_some(value))
    });
    let terminal = value
        .and_then(|value| value.get("status").or_else(|| value.get("threadStatus")))
        .map(|status| is_terminal_provider_status(&provider_status(Some(status), "unknown")))
        .unwrap_or(false);
    let direct_input = explicit_bool(capabilities, "directInput")
        .or_else(|| explicit_bool(capabilities, "direct_input"))
        .or(can_accept_direct_input)
        .unwrap_or(active);
    AgentSubagentCapabilities {
        inspect: explicit_bool(capabilities, "inspect").unwrap_or(true),
        direct_input: direct_input && !terminal,
        steer: explicit_bool(capabilities, "steer").unwrap_or(active) && !terminal,
        interrupt: explicit_bool(capabilities, "interrupt").unwrap_or(active) && !terminal,
        wait: explicit_bool(capabilities, "wait").unwrap_or(active) && !terminal,
        close: explicit_bool(capabilities, "close").unwrap_or(active) && !terminal,
    }
}

fn subagent_from_thread(
    value: &Value,
    _fallback_parent: Option<&str>,
) -> Option<AgentSubagentThread> {
    let id = provider_string(value.get("id"), MAX_SUBAGENT_ID_CHARS)?;
    if !valid_provider_thread_id(&id) {
        return None;
    }
    let thread_status = provider_status(
        value.get("status").or_else(|| value.get("threadStatus")),
        "unknown",
    );
    let active = matches!(thread_status.as_str(), "starting" | "running" | "active");
    let source = value.get("source");
    let nested_subagent_source = source.and_then(|source| source.get("subAgent"));
    let source_label =
        provider_string(nested_subagent_source, 96).or_else(|| provider_string(source, 96));
    let spawn_source = source
        .and_then(|source| source.get("subAgent"))
        .and_then(|source| source.get("thread_spawn"));
    // `threadSource` is Codex's installed, authoritative classification. The
    // nested `source` shape remains the metadata source/fallback for older
    // app-server builds.
    let source_kind = provider_string(value.get("threadSource"), 96)
        .or_else(|| provider_string(source.and_then(|source| source.get("kind")), 96))
        .or_else(|| provider_string(value.get("sourceKind"), 96))
        .or_else(|| spawn_source.map(|_| "subAgentThreadSpawn".to_string()))
        .or_else(|| match source_label.as_deref() {
            Some("review") => Some("subAgentReview".to_string()),
            Some("compact") => Some("subAgentCompact".to_string()),
            Some("memory_consolidation") => Some("subAgentOther".to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "subAgent".to_string());
    let parent_id = provider_string(
        value
            .get("parentThreadId")
            .or_else(|| value.get("parentId"))
            .or_else(|| source.and_then(|source| source.get("parent_thread_id")))
            .or_else(|| spawn_source.and_then(|source| source.get("parent_thread_id"))),
        MAX_SUBAGENT_ID_CHARS,
    );
    let path = value
        .get("agentPath")
        .or_else(|| source.and_then(|source| source.get("agent_path")))
        .or_else(|| spawn_source.and_then(|source| source.get("agent_path")))
        .map(|path| match path {
            Value::Array(values) => values
                .iter()
                .filter_map(|value| provider_string(Some(value), 96))
                .take(32)
                .collect::<Vec<_>>(),
            value => provider_string(Some(value), 96)
                .map(|path| vec![path])
                .unwrap_or_default(),
        })
        .unwrap_or_default();
    let depth = value
        .get("depth")
        .or_else(|| source.and_then(|source| source.get("depth")))
        .or_else(|| spawn_source.and_then(|source| source.get("depth")))
        .and_then(Value::as_u64)
        .and_then(|depth| u32::try_from(depth).ok())
        .unwrap_or(path.len().saturating_sub(1) as u32);
    let turn_status = provider_status(
        value.get("turnStatus").or_else(|| value.get("turn_status")),
        if active { "working" } else { "waiting" },
    );
    let collaboration_status = provider_string(
        value
            .get("collaborationStatus")
            .or_else(|| value.get("collaboration_status")),
        96,
    )
    .or_else(|| {
        value
            .get("status")
            .and_then(|status| status.get("activeFlags"))
            .and_then(Value::as_array)
            .and_then(|flags| flags.first())
            .and_then(|flag| provider_string(Some(flag), 96))
    });
    let runtime_state = provider_string(
        value
            .get("runtimeState")
            .or_else(|| value.get("runtime_state")),
        96,
    );
    let result = value
        .get("result")
        .and_then(result_from_value)
        .or_else(|| value.get("outcome").and_then(result_from_value));
    let nickname = provider_string(
        value.get("agentNickname").or_else(|| value.get("nickname")),
        96,
    )
    .or_else(|| {
        provider_string(
            spawn_source.and_then(|source| source.get("agent_nickname")),
            96,
        )
    });
    let role =
        provider_string(value.get("agentRole").or_else(|| value.get("role")), 160).or_else(|| {
            provider_string(
                spawn_source.and_then(|source| source.get("agent_role")),
                160,
            )
        });
    let can_accept_direct_input = value.get("canAcceptDirectInput").and_then(Value::as_bool);
    let collaboration_tool = provider_string(
        value
            .get("collaborationTool")
            .or_else(|| value.get("collaboration_tool")),
        96,
    );
    let consolidation_id = provider_string(
        value
            .get("consolidationId")
            .or_else(|| value.get("consolidation_id"))
            .or_else(|| value.get("consolidatesThreadId")),
        MAX_SUBAGENT_ID_CHARS,
    )
    .or_else(|| {
        (source_label.as_deref() == Some("memory_consolidation"))
            .then(|| provider_string(value.get("parentThreadId"), MAX_SUBAGENT_ID_CHARS))
            .flatten()
    });
    let approval_request_id = provider_string(
        value
            .get("approvalRequestId")
            .or_else(|| value.get("approval_request_id")),
        MAX_SUBAGENT_ID_CHARS,
    );
    Some(AgentSubagentThread {
        id,
        parent_id,
        source_kind,
        depth,
        agent_path: path,
        nickname,
        role,
        model: provider_string(value.get("model"), 160)
            .or_else(|| provider_string(value.get("modelProvider"), 160)),
        reasoning_effort: provider_string(
            value
                .get("reasoningEffort")
                .or_else(|| value.get("reasoning_effort")),
            96,
        ),
        runtime: provider_string(value.get("runtime"), 160),
        approval_policy: provider_string(
            value
                .get("approvalPolicy")
                .or_else(|| value.get("approval_policy")),
            96,
        ),
        permission_mode: provider_string(
            value
                .get("permissionMode")
                .or_else(|| value.get("permission_mode")),
            96,
        ),
        capacity: value
            .get("capacity")
            .or_else(|| value.get("remainingCapacity"))
            .and_then(Value::as_u64)
            .and_then(|capacity| u32::try_from(capacity).ok()),
        thread_status,
        turn_status,
        collaboration_status,
        collaboration_tool,
        consolidation_id,
        runtime_state,
        approval_request_id,
        prompt: provider_string(value.get("prompt"), 16_384).or_else(|| {
            provider_string(spawn_source.and_then(|source| source.get("prompt")), 16_384)
        }),
        preview: provider_string(value.get("preview"), 16_384),
        capabilities: subagent_capabilities(Some(value), active, can_accept_direct_input),
        activities: Vec::new(),
        result,
        timeline: Vec::new(),
        updated_at_ms: provider_timestamp_ms(
            value
                .get("updatedAt")
                .or_else(|| value.get("updated_at_ms")),
        )
        .unwrap_or_else(now_ms),
    })
}

fn result_from_value(value: &Value) -> Option<AgentSubagentResult> {
    if !value.is_object() {
        return None;
    }
    Some(AgentSubagentResult {
        status: provider_status(value.get("status"), "unknown"),
        summary: value
            .get("summary")
            .or_else(|| value.get("message"))
            .and_then(|value| {
                provider_string(Some(value), 16_384).or_else(|| {
                    value_as_text(Some(value)).map(|text| truncate_activity_text(&text, 16_384))
                })
            }),
        error: value.get("error").and_then(|error| {
            provider_string(Some(error), 4_000)
                .or_else(|| provider_string(error.get("message"), 4_000))
        }),
        updated_at_ms: now_ms(),
    })
}

fn result_from_turn(turn: &Value) -> Option<AgentSubagentResult> {
    let status = provider_status(turn.get("status"), "completed");
    let error = turn.get("error").and_then(|error| {
        provider_string(error.get("message"), 4_000).or_else(|| provider_string(Some(error), 4_000))
    });
    Some(AgentSubagentResult {
        status,
        summary: None,
        error,
        updated_at_ms: now_ms(),
    })
}

fn subagent_from_message(message: &Value, method: &str) -> Option<AgentSubagentThread> {
    let thread = message.pointer("/params/thread").unwrap_or(&Value::Null);
    let id = thread
        .get("id")
        .and_then(|value| provider_string(Some(value), MAX_SUBAGENT_ID_CHARS))
        .or_else(|| provider_string(message.pointer("/params/threadId"), MAX_SUBAGENT_ID_CHARS))?;
    if !valid_provider_thread_id(&id) || method == "thread/started" && thread.is_null() {
        return None;
    }
    let mut subagent = if thread.is_object() {
        subagent_from_thread(thread, None)?
    } else {
        subagent_status_update(&id, "unknown", "unknown")
    };
    if method == "thread/status/changed" {
        let status = message.pointer("/params/status");
        subagent.thread_status = provider_status(status, "unknown");
        subagent.turn_status = "unknown".to_string();
        // A status notification is commonly a sparse projection. Only
        // replace capabilities when Codex explicitly reports them; the
        // session merge then retains the last known capability set for an
        // idle thread that can still accept a follow-up.
        let capability_value = message
            .pointer("/params/capabilities")
            .or_else(|| thread.get("capabilities"));
        let can_accept_direct_input = message
            .pointer("/params/canAcceptDirectInput")
            .and_then(Value::as_bool)
            .or_else(|| thread.get("canAcceptDirectInput").and_then(Value::as_bool));
        if capability_value.is_some() || can_accept_direct_input.is_some() {
            subagent.capabilities = subagent_capabilities(
                capability_value,
                matches!(
                    subagent.thread_status.as_str(),
                    "starting" | "running" | "active"
                ),
                can_accept_direct_input,
            );
        } else if thread.is_null() {
            let active = matches!(
                subagent.thread_status.as_str(),
                "starting" | "running" | "active"
            );
            subagent.capabilities = AgentSubagentCapabilities {
                inspect: true,
                direct_input: active,
                steer: active,
                interrupt: active,
                wait: active,
                close: active,
            };
        }
        if let Some(flags) = status
            .and_then(|status| status.get("activeFlags"))
            .and_then(Value::as_array)
            .and_then(|flags| flags.first())
            .and_then(|flag| provider_string(Some(flag), 96))
        {
            subagent.collaboration_status = Some(flags);
        }
        subagent.updated_at_ms = provider_timestamp_ms(
            message
                .pointer("/params/updatedAt")
                .or_else(|| message.pointer("/params/updated_at_ms")),
        )
        .unwrap_or_else(now_ms);
    }
    if let Some(result) = message
        .pointer("/params/result")
        .and_then(result_from_value)
        .or_else(|| {
            message
                .pointer("/params/outcome")
                .and_then(result_from_value)
        })
    {
        subagent.result = Some(result);
        subagent.collaboration_status = provider_string(
            message
                .pointer("/params/collaborationStatus")
                .or_else(|| message.pointer("/params/collaboration_status")),
            96,
        )
        .or(subagent.collaboration_status);
    }
    subagent.collaboration_tool = provider_string(
        message
            .pointer("/params/collaborationTool")
            .or_else(|| message.pointer("/params/collaboration_tool")),
        96,
    );
    subagent.consolidation_id = provider_string(
        message
            .pointer("/params/consolidationId")
            .or_else(|| message.pointer("/params/consolidation_id")),
        MAX_SUBAGENT_ID_CHARS,
    );
    subagent.approval_request_id = provider_string(
        message
            .pointer("/params/approvalRequestId")
            .or_else(|| message.pointer("/params/approval_request_id")),
        MAX_SUBAGENT_ID_CHARS,
    );
    if method == "thread/closed" {
        subagent.thread_status = "closed".to_string();
        subagent.turn_status = "waiting".to_string();
        subagent.runtime_state = Some("shutdown".to_string());
        subagent.capabilities.direct_input = false;
        subagent.capabilities.steer = false;
        subagent.capabilities.interrupt = false;
        subagent.capabilities.wait = false;
        subagent.capabilities.close = false;
    }
    Some(subagent)
}

fn subagent_status_update(id: &str, thread_status: &str, turn_status: &str) -> AgentSubagentThread {
    let active_controls = matches!(thread_status, "starting" | "running" | "active")
        || matches!(turn_status, "working" | "inProgress" | "in_progress");
    AgentSubagentThread {
        id: id.to_string(),
        parent_id: None,
        source_kind: "subAgent".to_string(),
        depth: 0,
        agent_path: Vec::new(),
        nickname: None,
        role: None,
        model: None,
        reasoning_effort: None,
        runtime: None,
        approval_policy: None,
        permission_mode: None,
        capacity: None,
        thread_status: thread_status.to_string(),
        turn_status: turn_status.to_string(),
        collaboration_status: None,
        collaboration_tool: None,
        consolidation_id: None,
        runtime_state: None,
        approval_request_id: None,
        prompt: None,
        preview: None,
        capabilities: AgentSubagentCapabilities {
            inspect: true,
            direct_input: active_controls,
            steer: active_controls,
            interrupt: active_controls,
            wait: active_controls,
            close: active_controls,
        },
        activities: Vec::new(),
        result: None,
        timeline: Vec::new(),
        updated_at_ms: now_ms(),
    }
}

fn errored_subagent(id: String, message: &str) -> AgentSubagentThread {
    let mut subagent = subagent_status_update(&id, "errored", "waiting");
    subagent.runtime_state = Some("errored".to_string());
    subagent.result = Some(AgentSubagentResult {
        status: "failed".to_string(),
        summary: None,
        error: provider_string(Some(&Value::String(message.to_string())), 4_000),
        updated_at_ms: now_ms(),
    });
    subagent
}

fn emit_subagent_collaboration(context: &ServerRuntimeContext, item: &Value, completed: bool) {
    if normalized_item_type(item).as_deref() != Some("collabagenttoolcall") {
        return;
    }
    let tool =
        provider_string(item.get("tool"), 96).or_else(|| provider_string(item.get("toolName"), 96));
    let Some(tool) = tool else { return };
    if !matches!(tool.as_str(), "spawnAgent" | "wait" | "closeAgent") {
        return;
    }
    let mut receiver_ids = item
        .get("receiverThreadIds")
        .or_else(|| item.get("receiver_thread_ids"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| provider_string(Some(value), MAX_SUBAGENT_ID_CHARS))
        .filter(|id| valid_provider_thread_id(id))
        .collect::<Vec<_>>();
    if receiver_ids.is_empty() {
        if let Some(id) = provider_string(
            item.get("agentThreadId")
                .or_else(|| item.get("agent_thread_id")),
            MAX_SUBAGENT_ID_CHARS,
        )
        .filter(|id| valid_provider_thread_id(id))
        {
            receiver_ids.push(id);
        }
    }
    let item_id = provider_string(item.get("id"), MAX_SUBAGENT_ID_CHARS)
        .unwrap_or_else(|| format!("collaboration-{}", now_ms()));
    let item_status = provider_status(
        item.get("status"),
        if completed {
            "completed"
        } else {
            "in_progress"
        },
    );
    let prompt = provider_string(item.get("prompt"), 16_384)
        .unwrap_or_else(|| format!("Codex {} el subagente", tool));
    let states = item
        .get("agentsStates")
        .or_else(|| item.get("agents_states"));
    for thread_id in receiver_ids {
        let pending_action = context
            .pending_collaborations
            .lock()
            .ok()
            .and_then(|pending| pending.get(&thread_id).cloned());
        if pending_action
            .as_deref()
            .is_some_and(|action| action != tool.as_str())
        {
            continue;
        }
        let state = states.and_then(|states| states.get(&thread_id));
        let state_status = state
            .and_then(|state| state.get("status"))
            .and_then(|status| provider_string(Some(status), 96));
        let thread_status = state_status.as_deref().unwrap_or("unknown");
        let mut update = subagent_status_update(
            &thread_id,
            thread_status,
            if completed { "waiting" } else { "working" },
        );
        update.collaboration_tool = Some(tool.clone());
        update.collaboration_status = Some(item_status.clone());
        update.prompt = Some(prompt.clone());
        update.model = provider_string(item.get("model"), 160);
        update.reasoning_effort = provider_string(
            item.get("reasoningEffort")
                .or_else(|| item.get("reasoning_effort")),
            96,
        );
        update.activities = vec![AgentSubagentActivity {
            id: item_id.clone(),
            kind: "collab_agent_tool_call".to_string(),
            status: item_status.clone(),
            text: prompt.clone(),
            timestamp_ms: now_ms(),
        }];
        if let Some(state) = state {
            update.result = collab_result_from_state(state);
        }
        let _ = context
            .event_tx
            .send(AgentProcessEvent::SubagentUpdated { subagent: update });
        if completed {
            if let Ok(mut pending) = context.pending_collaborations.lock() {
                pending.remove(&thread_id);
            }
        }
    }
}

fn collab_result_from_state(value: &Value) -> Option<AgentSubagentResult> {
    let status = provider_string(value.get("status"), 96)?;
    Some(AgentSubagentResult {
        status,
        summary: provider_string(value.get("message"), 16_384),
        error: None,
        updated_at_ms: now_ms(),
    })
}

fn emit_subagent_activity(
    context: &ServerRuntimeContext,
    thread_id: Option<&str>,
    item: &Value,
    completed: bool,
) {
    let Some(thread_id) = thread_id else { return };
    let Some(id) = provider_string(item.get("id"), 256) else {
        return;
    };
    let kind = normalized_item_type(item).unwrap_or_else(|| "item".to_string());
    let status = provider_status(
        item.get("status"),
        if completed {
            "completed"
        } else {
            "in_progress"
        },
    );
    let text = activity_text_from_item(item, completed).unwrap_or_else(|| {
        provider_string(item.get("prompt").or_else(|| item.get("text")), 512)
            .unwrap_or_else(|| "Actividad del subagente".to_string())
    });
    let collaboration_tool = provider_string(
        item.get("toolName")
            .or_else(|| item.get("tool_name"))
            .or_else(|| item.get("name"))
            .or_else(|| item.get("server")),
        96,
    );
    let consolidation_id = provider_string(
        item.get("consolidationId")
            .or_else(|| item.get("consolidation_id"))
            .or_else(|| item.get("consolidatedThreadId")),
        MAX_SUBAGENT_ID_CHARS,
    );
    let _ = context.event_tx.send(AgentProcessEvent::SubagentUpdated {
        subagent: AgentSubagentThread {
            collaboration_tool,
            consolidation_id,
            activities: vec![AgentSubagentActivity {
                id,
                kind,
                status,
                text,
                timestamp_ms: now_ms(),
            }],
            ..subagent_status_update(thread_id, "running", "working")
        },
    });
}

fn emit_subagent_delta(
    context: &ServerRuntimeContext,
    thread_id: &str,
    item_id: Option<&str>,
    delta: &str,
) {
    let Some(id) = item_id.and_then(|id| {
        provider_string(Some(&Value::String(id.to_string())), MAX_SUBAGENT_ID_CHARS)
    }) else {
        return;
    };
    let Some(text) = provider_string(Some(&Value::String(delta.to_string())), 16_384) else {
        return;
    };
    let _ = context.event_tx.send(AgentProcessEvent::SubagentUpdated {
        subagent: AgentSubagentThread {
            activities: vec![AgentSubagentActivity {
                id,
                kind: "agent_message".to_string(),
                status: "in_progress".to_string(),
                text,
                timestamp_ms: now_ms(),
            }],
            ..subagent_status_update(thread_id, "unknown", "working")
        },
    });
}

fn emit_subagent_approval(context: &ServerRuntimeContext, thread_id: &str, message: &Value) {
    let request = message
        .pointer("/params/request")
        .or_else(|| message.pointer("/params/serverRequest"));
    let request_id = request
        .and_then(|request| request.get("id"))
        .or_else(|| message.pointer("/params/requestId"))
        .or_else(|| message.pointer("/params/id"))
        .or_else(|| message.pointer("/id"))
        .and_then(|value| provider_identifier(Some(value), MAX_SUBAGENT_ID_CHARS));
    let text = request
        .and_then(|request| request.get("reason").or_else(|| request.get("message")))
        .or_else(|| message.pointer("/params/request/params/reason"))
        .or_else(|| message.pointer("/params/request/params/message"))
        .or_else(|| message.pointer("/params/reason"))
        .or_else(|| message.pointer("/params/message"))
        .and_then(|value| provider_string(Some(value), 512))
        .unwrap_or_else(|| "El subagente espera una aprobacion".to_string());
    let mut update = subagent_status_update(thread_id, "unknown", "waiting");
    update.collaboration_status = Some("waiting_on_approval".to_string());
    update.approval_request_id = request_id.clone();
    update.activities = vec![AgentSubagentActivity {
        id: request_id.unwrap_or_else(|| format!("approval-{}", now_ms())),
        kind: "approval_request".to_string(),
        status: "pending".to_string(),
        text,
        timestamp_ms: now_ms(),
    }];
    let _ = context
        .event_tx
        .send(AgentProcessEvent::SubagentUpdated { subagent: update });
}

fn is_server_approval_request(message: &Value) -> bool {
    let nested_method = message
        .pointer("/params/request/method")
        .or_else(|| message.pointer("/params/serverRequest/method"))
        .or_else(|| message.pointer("/params/method"))
        .and_then(Value::as_str);
    nested_method.is_some_and(|method| method.ends_with("/requestApproval"))
        || message.pointer("/params/request/approval").is_some()
        || message.pointer("/params/serverRequest/approval").is_some()
        || message.pointer("/params/approval").is_some()
}

fn emit_subagent_approval_resolved(
    context: &ServerRuntimeContext,
    thread_id: &str,
    message: &Value,
) {
    let request_id = message
        .pointer("/params/requestId")
        .or_else(|| message.pointer("/params/id"))
        .and_then(|value| provider_identifier(Some(value), MAX_SUBAGENT_ID_CHARS));
    let mut update = subagent_status_update(thread_id, "unknown", "waiting");
    update.collaboration_status = Some("approval_resolved".to_string());
    update.approval_request_id = None;
    update.activities = vec![AgentSubagentActivity {
        id: request_id.unwrap_or_else(|| format!("approval-resolved-{}", now_ms())),
        kind: "approval_request".to_string(),
        status: "resolved".to_string(),
        text: "La aprobacion del subagente fue resuelta".to_string(),
        timestamp_ms: now_ms(),
    }];
    let _ = context
        .event_tx
        .send(AgentProcessEvent::SubagentUpdated { subagent: update });
}

fn handle_control_response(message: &Value, context: &ServerRuntimeContext) -> bool {
    if message.get("method").is_some() {
        return false;
    }
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
    let result = match message
        .pointer("/error/message")
        .and_then(|value| provider_string(Some(value), 4_000))
    {
        Some(error) => Err(AgentConsoleError::new("provider_control_failed", error)),
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
                .and_then(Value::as_str);
            match (
                normalize_activity_identity(server),
                normalize_activity_identity(tool),
            ) {
                (Some(server), Some(tool)) => format!("Usando {server} / {tool}"),
                // Without both explicit identities this remains a generic
                // activity and cannot imply MCP attribution.
                _ => "Usando una herramienta".to_string(),
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

fn normalize_activity_identity(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.chars().any(char::is_control) {
        return None;
    }
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() || cleaned.chars().count() > 96 {
        return None;
    }
    if !cleaned.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-' | '.')
    }) {
        return None;
    }

    let lower = cleaned.to_ascii_lowercase();
    let normalized = lower.replace('_', "-");
    let forbidden = [
        "token",
        "secret",
        "password",
        "credential",
        "authorization",
        "cookie",
        "header",
        "headers",
        "http",
        "https",
        "url",
        "uri",
    ];
    if normalized.contains("api-key")
        || forbidden.iter().any(|marker| {
            normalized
                .split(|character: char| {
                    character.is_whitespace() || matches!(character, '-' | '.')
                })
                .any(|segment| segment == *marker)
        })
    {
        return None;
    }
    Some(cleaned)
}

fn handle_model_catalog_response(message: &Value, context: &ServerRuntimeContext) -> bool {
    if message.get("method").is_some() {
        return false;
    }
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

const CODEX_SUBAGENT_SOURCE_KINDS: &[&str] = &[
    "subAgent",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "subAgentOther",
];

#[allow(clippy::too_many_arguments)]
fn request_subagent_discovery(
    stdin: &Arc<Mutex<ChildStdin>>,
    pending_requests: &PendingDiscoveryRequests,
    discovery_state: &Arc<Mutex<DiscoveryState>>,
    next_request_id: &Arc<AtomicU64>,
    root_thread_id: &str,
    cursor: Option<&str>,
    page: usize,
    run_id: Option<u64>,
) -> Result<(), AgentConsoleError> {
    let root_thread_id = provider_string(
        Some(&Value::String(root_thread_id.to_string())),
        MAX_SUBAGENT_ID_CHARS,
    )
    .filter(|id| valid_provider_thread_id(id))
    .ok_or_else(|| AgentConsoleError::new("invalid_subagent_id", "id de hilo Codex no valido"))?;
    if page == 0 || page > MAX_DISCOVERY_PAGES {
        return Err(AgentConsoleError::new(
            "subagent_discovery_limit",
            "Codex alcanzo el limite de paginas de descendientes",
        ));
    }
    let cursor = cursor
        .map(|cursor| cursor.trim().to_string())
        .filter(|cursor| !cursor.is_empty());
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.chars().count() > MAX_DISCOVERY_CURSOR_CHARS)
    {
        return Err(AgentConsoleError::new(
            "subagent_discovery_cursor_invalid",
            "el cursor de descendientes es demasiado largo",
        ));
    }
    let run_id = {
        let mut state = discovery_state.lock().map_err(|_| {
            AgentConsoleError::new("app_server_lock_poisoned", "discovery state lock failed")
        })?;
        if cursor.is_none() {
            if page != 1 || run_id.is_some() {
                return Err(AgentConsoleError::new(
                    "subagent_discovery_cursor_invalid",
                    "una pagina posterior requiere cursor",
                ));
            }
            state.run_id = state.run_id.saturating_add(1).max(1);
            state.seen_cursors.clear();
            state.child_ids.clear();
        } else if run_id != Some(state.run_id) {
            return Err(AgentConsoleError::new(
                "subagent_discovery_cursor_invalid",
                "la pagina de descendientes pertenece a otra ejecucion",
            ));
        }
        if let Some(cursor) = cursor.as_ref() {
            if !state.seen_cursors.insert(cursor.clone()) {
                return Err(AgentConsoleError::new(
                    "subagent_discovery_cursor_loop",
                    "Codex repitio un cursor de descendientes",
                ));
            }
        }
        state.run_id
    };
    if discovery_state
        .lock()
        .map(|state| state.child_ids.len() > MAX_DISCOVERED_CHILDREN)
        .unwrap_or(true)
    {
        return Err(AgentConsoleError::new(
            "subagent_capacity_exceeded",
            "Codex alcanzo el limite de descendientes",
        ));
    }
    let request_id = next_request_id.fetch_add(1, Ordering::SeqCst);
    pending_requests
        .lock()
        .map_err(|_| AgentConsoleError::new("app_server_lock_poisoned", "discovery lock failed"))?
        .insert(
            request_id,
            PendingDiscovery {
                root_thread_id: root_thread_id.clone(),
                page,
                run_id,
            },
        );
    let result = write_json(
        stdin,
        &json!({
            "method": "thread/list",
            "id": request_id,
            "params": {
                "cursor": cursor,
                "limit": 100,
                "sourceKinds": CODEX_SUBAGENT_SOURCE_KINDS,
                "ancestorThreadId": root_thread_id
            }
        }),
    );
    if result.is_err() {
        if let Ok(mut pending) = pending_requests.lock() {
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

fn timeline_frame_for_thread(
    kind: AgentSessionTimelineKind,
    text: &str,
    thread_id: Option<&str>,
) -> Vec<u8> {
    let text = sanitize_provider_timeline_text(text);
    let mut frame = TIMELINE_FRAME_PREFIX.to_vec();
    frame.extend(
        serde_json::to_vec(&json!({
            "kind": kind,
            "text_base64": STANDARD.encode(text.as_bytes()),
            "thread_id": thread_id,
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
    while let Some(turn) = pending.pop_front() {
        let request_id = next_request_id.fetch_add(1, Ordering::SeqCst);
        if let Err(error) = send_turn_start(stdin, request_id, thread_id, &turn, cwd) {
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
    turn: &PendingTurn,
    cwd: &Path,
) -> Result<(), AgentConsoleError> {
    send_turn_start_with_approval(stdin, request_id, thread_id, turn, cwd, Some("never"))
}

fn send_turn_start_with_approval(
    stdin: &Arc<Mutex<ChildStdin>>,
    request_id: u64,
    thread_id: &str,
    turn: &PendingTurn,
    cwd: &Path,
    approval_policy: Option<&str>,
) -> Result<(), AgentConsoleError> {
    write_json(
        stdin,
        &turn_start_message_with_approval(
            request_id,
            thread_id,
            &turn.text,
            &turn.attachments,
            cwd,
            turn.options.as_ref(),
            turn.permission_mode,
            approval_policy,
        ),
    )
}

#[cfg(test)]
fn turn_start_message(
    request_id: u64,
    thread_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
) -> Value {
    turn_start_message_with_permission(
        request_id,
        thread_id,
        text,
        attachments,
        cwd,
        options,
        AgentSessionPermissionMode::Workspace,
    )
}

#[cfg(test)]
fn turn_start_message_with_permission(
    request_id: u64,
    thread_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
    permission_mode: AgentSessionPermissionMode,
) -> Value {
    turn_start_message_with_approval(
        request_id,
        thread_id,
        text,
        attachments,
        cwd,
        options,
        permission_mode,
        Some("never"),
    )
}

#[allow(clippy::too_many_arguments)]
fn turn_start_message_with_approval(
    request_id: u64,
    thread_id: &str,
    text: &str,
    attachments: &[AgentTurnAttachment],
    cwd: &Path,
    options: Option<&AgentSessionRuntimeOptions>,
    permission_mode: AgentSessionPermissionMode,
    approval_policy: Option<&str>,
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
    if let Some(approval_policy) = approval_policy {
        params["approvalPolicy"] = json!(approval_policy);
    }
    params["sandboxPolicy"] = json!({
        "type": match permission_mode {
            AgentSessionPermissionMode::Workspace => "workspaceWrite",
            AgentSessionPermissionMode::FullAccess => "dangerFullAccess",
        }
    });
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
            active_turn_ids: Arc::new(Mutex::new(HashMap::new())),
            completed_agent_messages: Arc::new(Mutex::new(HashMap::new())),
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
            pending_thread_requests: Arc::new(Mutex::new(HashMap::new())),
            pending_discovery_requests: Arc::new(Mutex::new(HashMap::new())),
            discovery_state: Arc::new(Mutex::new(DiscoveryState::default())),
            pending_collaborations: Arc::new(Mutex::new(HashMap::new())),
            resume_tx: Mutex::new(None),
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
    fn accepted_thread_resume_resolves_only_after_the_provider_response() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        *context.resume_tx.lock().unwrap() = Some(resume_tx);

        assert!(resume_rx.try_recv().is_err());
        handle_server_message(
            &json!({
                "id": THREAD_REQUEST_ID,
                "result": {"thread": {"id": "thread-42"}}
            }),
            &context,
        );

        assert_eq!(
            resume_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Ok(crate::bus::contract::AgentSessionResumeMode::Native)
        );
        handle_server_message(
            &json!({
                "id": THREAD_REQUEST_ID,
                "error": {"message": "duplicate response"}
            }),
            &context,
        );
        assert!(matches!(
            resume_rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn rejected_thread_resume_reports_failure_instead_of_native_success() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        *context.resume_tx.lock().unwrap() = Some(resume_tx);

        handle_server_message(
            &json!({
                "id": THREAD_REQUEST_ID,
                "error": {"message": "invalid sandbox"}
            }),
            &context,
        );

        let error = resume_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap_err();
        assert_eq!(error.category, "agent_resume_failed");
        assert_eq!(error.message, "invalid sandbox");
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
        assert!(message["params"].get("sandboxPolicy").is_none());
        assert!(message["params"].get("approvalPolicy").is_none());
    }

    #[test]
    fn turn_start_uses_camel_case_sandbox_policy_for_each_permission_mode() {
        let workspace = turn_start_message_with_permission(
            100,
            "thread-1",
            "workspace turn",
            &[],
            Path::new("/tmp/repo"),
            None,
            AgentSessionPermissionMode::Workspace,
        );
        assert_eq!(workspace["params"]["approvalPolicy"], "never");
        assert_eq!(
            workspace["params"]["sandboxPolicy"]["type"],
            "workspaceWrite"
        );

        let full_access = turn_start_message_with_permission(
            101,
            "thread-1",
            "full access turn",
            &[],
            Path::new("/tmp/repo"),
            None,
            AgentSessionPermissionMode::FullAccess,
        );
        assert_eq!(full_access["params"]["approvalPolicy"], "never");
        assert_eq!(
            full_access["params"]["sandboxPolicy"]["type"],
            "dangerFullAccess"
        );
    }

    #[test]
    fn queued_turn_keeps_the_mode_captured_when_it_was_enqueued() {
        let queued = PendingTurn {
            text: "queued".into(),
            attachments: Vec::new(),
            options: None,
            permission_mode: AgentSessionPermissionMode::Workspace,
        };
        let selected_after_enqueue = AgentSessionPermissionMode::FullAccess;
        let message = turn_start_message_with_permission(
            100,
            "thread-1",
            &queued.text,
            &queued.attachments,
            Path::new("/tmp/repo"),
            queued.options.as_ref(),
            queued.permission_mode,
        );

        assert_eq!(
            selected_after_enqueue,
            AgentSessionPermissionMode::FullAccess
        );
        assert_eq!(message["params"]["sandboxPolicy"]["type"], "workspaceWrite");
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
    fn mcp_activity_requires_explicit_bounded_server_and_tool_identity() {
        let explicit = activity_text_from_item(
            &json!({
                "type": "mcptoolcall",
                "server": "fileserver",
                "tool": "read_file"
            }),
            false,
        )
        .unwrap();
        assert_eq!(explicit, "Usando fileserver / read_file");

        let generic = activity_text_from_item(
            &json!({ "type": "mcptoolcall", "tool": "read_file" }),
            false,
        )
        .unwrap();
        assert_eq!(generic, "Usando una herramienta");
    }

    #[test]
    fn mcp_activity_rejects_unsafe_identity_shapes() {
        for server in [
            "https://example.com/mcp",
            "Authorization: Bearer secret",
            r"C:\Users\token",
        ] {
            let text = activity_text_from_item(
                &json!({
                    "type": "mcptoolcall",
                    "server": server,
                    "tool": "read_file"
                }),
                false,
            )
            .unwrap();
            assert_eq!(text, "Usando una herramienta");
            assert!(!text.contains(server));
        }

        let text = activity_text_from_item(
            &json!({
                "type": "mcptoolcall",
                "server": "safe\nserver",
                "tool": "read\tfile"
            }),
            false,
        )
        .unwrap();
        assert_eq!(text, "Usando una herramienta");

        let long = "x".repeat(200);
        let text = activity_text_from_item(
            &json!({ "type": "mcptoolcall", "server": long, "tool": "read" }),
            false,
        )
        .unwrap();
        assert_eq!(text, "Usando una herramienta");
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
    fn completed_child_message_cache_is_bounded_and_sanitized_before_promotion() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        let provider_text = format!("safe\u{0}{}", "x".repeat(20_000));
        handle_server_message(
            &json!({
                "method":"item/completed",
                "params":{
                    "threadId":"child-thread",
                    "item":{"id":"message-1","type":"agentMessage","text":provider_text}
                }
            }),
            &context,
        );

        let messages = context.completed_agent_messages.lock().unwrap();
        let cached = &messages["child-thread"][0];
        assert_eq!(
            cached.chars().count(),
            crate::agent_console::MAX_PROVIDER_TIMELINE_TEXT_CHARS
        );
        assert!(!cached.chars().any(char::is_control));
    }

    #[test]
    fn incomplete_unique_child_turns_cannot_grow_thread_state_without_bound() {
        let mut messages = HashMap::new();
        let mut active_turns = HashMap::new();
        for index in 0..MAX_DISCOVERED_CHILDREN {
            let thread_id = format!("child-{index}");
            assert!(
                completed_messages_for_thread(&mut messages, thread_id.clone(), false).is_some()
            );
            assert!(track_active_turn(&mut active_turns, &thread_id, "turn"));
        }

        assert!(
            completed_messages_for_thread(&mut messages, "overflow".to_string(), false).is_none()
        );
        assert!(!track_active_turn(&mut active_turns, "overflow", "turn"));
        assert_eq!(messages.len(), MAX_DISCOVERED_CHILDREN);
        assert_eq!(active_turns.len(), MAX_DISCOVERED_CHILDREN);

        let (tx, rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        *context.completed_agent_messages.lock().unwrap() = messages;
        handle_server_message(
            &json!({
                "method":"item/completed",
                "params":{"threadId":"t","item":{"id":"root-message","type":"agentMessage","text":"root survives"}}
            }),
            &context,
        );
        let _progress = rx.recv().unwrap();
        handle_server_message(
            &json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"root-turn"}}}),
            &context,
        );
        let final_message = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(final_message.contains("\"kind\":\"agent_message\""));
        assert!(final_message.contains(&STANDARD.encode("root survives")));
    }

    #[test]
    fn child_notifications_remain_thread_scoped_and_do_not_complete_root_turn() {
        let (tx, rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);

        handle_server_message(
            &json!({
                "method": "turn/started",
                "params": {"threadId": "child-1", "turn": {"id": "child-turn"}}
            }),
            &context,
        );
        handle_server_message(
            &json!({
                "method": "item/completed",
                "params": {
                    "threadId": "child-1",
                    "item": {"id": "child-msg", "type": "agentMessage", "text": "child result"}
                }
            }),
            &context,
        );
        handle_server_message(
            &json!({
                "method": "turn/completed",
                "params": {"threadId": "child-1", "turn": {"id": "child-turn"}}
            }),
            &context,
        );

        let mut saw_child_frame = false;
        while let Ok(frame) = rx.try_recv() {
            let parsed = crate::agent_console::commands::parse_timeline_frame(&frame)
                .expect("child output should remain framed");
            assert_eq!(parsed.thread_id.as_deref(), Some("child-1"));
            saw_child_frame = true;
        }
        assert!(saw_child_frame);
        assert!(event_rx
            .try_iter()
            .all(|event| !matches!(event, AgentProcessEvent::TurnCompleted { .. })));
        assert_eq!(context.active_turn_ids.lock().unwrap().get("child-1"), None);
    }

    #[test]
    fn descendant_projection_preserves_identity_metadata_and_unknown_statuses() {
        let subagent = subagent_from_thread(
            &json!({
                "id": "grandchild",
                "parentThreadId": "child-1",
                "source": {"kind": "subAgentOther", "depth": 2, "agent_path": ["root", "child", "grandchild"]},
                "agentNickname": "reviewer",
                "agentRole": "code reviewer",
                "model": "gpt-5.6-sol",
                "reasoningEffort": "xhigh",
                "status": "futureProviderState",
                "capabilities": {"inspect": true, "directInput": false}
            }),
            Some("root"),
        )
        .unwrap();
        assert_eq!(subagent.id, "grandchild");
        assert_eq!(subagent.parent_id.as_deref(), Some("child-1"));
        assert_eq!(subagent.source_kind, "subAgentOther");
        assert_eq!(subagent.thread_status, "futureProviderState");
        assert!(!subagent.capabilities.direct_input);
        assert_eq!(subagent.model.as_deref(), Some("gpt-5.6-sol"));
    }

    #[test]
    fn codex_subagent_fixture_tolerates_out_of_order_nested_and_unknown_events() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        for line in include_str!("test_fixtures/codex-app-server-subagents-v2.jsonl").lines() {
            let message: Value = serde_json::from_str(line).expect("fixture json");
            handle_server_message(&message, &context);
        }
        let updates = event_rx
            .try_iter()
            .filter_map(|event| match event {
                AgentProcessEvent::SubagentUpdated { subagent } => Some(subagent),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(updates.iter().any(|agent| agent.id == "child-1"));
        assert!(updates.iter().any(|agent| agent.id == "grandchild-1"));
    }

    #[test]
    fn real_codex_thread_shape_keeps_object_status_source_and_result_metadata() {
        let subagent = subagent_from_thread(
            &json!({
                "id": "child-real",
                "threadSource": "subAgentReview",
                "source": {"subAgent": {"thread_spawn": {
                    "parent_thread_id": "root-real",
                    "depth": 1,
                    "agent_nickname": "reviewer",
                    "agent_role": "code reviewer",
                    "agent_path": "root-real/reviewer"
                }}},
                "status": {"type": "active", "activeFlags": ["waitingOnApproval"]},
                "canAcceptDirectInput": true,
                "preview": "review the patch",
                "prompt": "Check the backend graph",
                "result": {"status": {"type": "failed"}, "message": "denied"}
            }),
            None,
        )
        .unwrap();
        assert_eq!(subagent.parent_id.as_deref(), Some("root-real"));
        assert_eq!(subagent.source_kind, "subAgentReview");
        assert_eq!(subagent.depth, 1);
        assert_eq!(subagent.agent_path, vec!["root-real/reviewer"]);
        assert_eq!(subagent.thread_status, "active");
        assert_eq!(
            subagent.collaboration_status.as_deref(),
            Some("waitingOnApproval")
        );
        assert_eq!(
            subagent
                .result
                .as_ref()
                .map(|result| result.status.as_str()),
            Some("failed")
        );
        assert_eq!(
            subagent
                .result
                .as_ref()
                .and_then(|result| result.summary.as_deref()),
            Some("denied")
        );
        assert_eq!(subagent.prompt.as_deref(), Some("Check the backend graph"));
        assert_eq!(subagent.preview.as_deref(), Some("review the patch"));
    }

    #[test]
    fn child_goal_and_usage_notifications_never_update_root_state() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({"method":"thread/goal/updated","params":{"threadId":"child","goal":{"objective":"child"}}}),
            &context,
        );
        handle_server_message(
            &json!({"method":"thread/tokenUsage/updated","params":{"threadId":"child","tokenUsage":{"last":{"totalTokens":2},"modelContextWindow":4}}}),
            &context,
        );
        assert!(event_rx.try_iter().all(|event| !matches!(
            event,
            AgentProcessEvent::GoalUpdated { .. } | AgentProcessEvent::ContextUsageUpdated { .. }
        )));
    }

    #[test]
    fn root_thread_lifecycle_notifications_never_project_as_children() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        for message in [
            json!({"method":"thread/started","params":{"threadId":"t","thread":{"id":"t","status":{"type":"active"}}}}),
            json!({"method":"thread/status/changed","params":{"threadId":"t","status":{"type":"idle"}}}),
            json!({"method":"thread/closed","params":{"threadId":"t"}}),
        ] {
            handle_server_message(&message, &context);
        }
        assert!(event_rx
            .try_iter()
            .all(|event| !matches!(event, AgentProcessEvent::SubagentUpdated { .. })));
    }

    #[test]
    fn server_request_approval_is_thread_scoped_and_resolved_by_request_id() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "id": "server-request-1",
                "method": "serverRequest",
                "params": {"request": {
                    "id": "approval-1",
                    "method": "item/commandExecution/requestApproval",
                    "params": {"threadId":"child-server", "reason":"run command"}
                }}
            }),
            &context,
        );
        let AgentProcessEvent::SubagentUpdated { subagent } = event_rx.recv().unwrap() else {
            panic!("expected child approval");
        };
        assert_eq!(subagent.id, "child-server");
        assert_eq!(subagent.approval_request_id.as_deref(), Some("approval-1"));
        assert_eq!(
            subagent.collaboration_status.as_deref(),
            Some("waiting_on_approval")
        );

        handle_server_message(
            &json!({"method":"serverRequest/resolved","params":{"threadId":"child-server","requestId":"approval-1"}}),
            &context,
        );
        let AgentProcessEvent::SubagentUpdated { subagent } = event_rx.recv().unwrap() else {
            panic!("expected resolved child approval");
        };
        assert_eq!(subagent.approval_request_id, None);
        assert_eq!(
            subagent.collaboration_status.as_deref(),
            Some("approval_resolved")
        );
    }

    #[test]
    fn collaboration_tool_call_associates_bounded_child_result() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        handle_server_message(
            &json!({
                "method":"item/completed",
                "params":{"threadId":"t","item":{
                    "id":"collab-1", "type":"collabAgentToolCall", "tool":"wait",
                    "status":"completed", "prompt":"Wait for child result",
                    "senderThreadId":"t", "receiverThreadIds":["child-collab"],
                    "agentsStates":{"child-collab":{"status":"completed","message":"done"}}
                }}
            }),
            &context,
        );
        let AgentProcessEvent::SubagentUpdated { subagent } = event_rx.recv().unwrap() else {
            panic!("expected collaboration projection");
        };
        assert_eq!(subagent.id, "child-collab");
        assert_eq!(subagent.collaboration_tool.as_deref(), Some("wait"));
        assert_eq!(
            subagent
                .result
                .as_ref()
                .and_then(|result| result.summary.as_deref()),
            Some("done")
        );
    }

    #[test]
    fn discovery_rejects_unbounded_pages_cursors_and_provider_ids() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let state = Arc::new(Mutex::new(DiscoveryState::default()));
        let next = Arc::new(AtomicU64::new(100));
        let stdin = dummy_stdin();
        assert_eq!(
            request_subagent_discovery(
                &stdin,
                &pending,
                &state,
                &next,
                "root",
                None,
                MAX_DISCOVERY_PAGES + 1,
                None
            )
            .unwrap_err()
            .category,
            "subagent_discovery_limit"
        );
        let oversized = "x".repeat(MAX_DISCOVERY_CURSOR_CHARS + 1);
        assert_eq!(
            request_subagent_discovery(
                &stdin,
                &pending,
                &state,
                &next,
                "root",
                Some(&oversized),
                2,
                Some(1)
            )
            .unwrap_err()
            .category,
            "subagent_discovery_cursor_invalid"
        );
        assert!(request_subagent_discovery(
            &stdin, &pending, &state, &next, "bad/id", None, 1, None
        )
        .is_err());
    }

    #[test]
    fn discovery_capacity_and_cursor_are_unique_per_run() {
        let (tx, _rx) = mpsc::channel();
        let (event_tx, _event_rx) = mpsc::channel();
        let context = dummy_context(tx, event_tx);
        request_subagent_discovery(
            &context.stdin,
            &context.pending_discovery_requests,
            &context.discovery_state,
            &context.next_request_id,
            "t",
            None,
            1,
            None,
        )
        .unwrap();
        handle_server_message(
            &json!({"id":100,"result":{"data":[
                {"id":"unique-child","threadSource":"subAgent","status":{"type":"idle"}},
                {"id":"unique-child","threadSource":"subAgent","status":{"type":"idle"}}
            ],"nextCursor":"cursor-1"}}),
            &context,
        );
        assert_eq!(context.discovery_state.lock().unwrap().child_ids.len(), 1);
        assert_eq!(
            context.discovery_state.lock().unwrap().seen_cursors.len(),
            1
        );

        request_subagent_discovery(
            &context.stdin,
            &context.pending_discovery_requests,
            &context.discovery_state,
            &context.next_request_id,
            "t",
            None,
            1,
            None,
        )
        .unwrap();
        assert!(context
            .discovery_state
            .lock()
            .unwrap()
            .seen_cursors
            .is_empty());
        assert!(context.discovery_state.lock().unwrap().child_ids.is_empty());
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
    fn child_timeline_frames_are_bounded_and_strip_control_characters() {
        let provider_text = format!("safe\u{0}{}", "x".repeat(20_000));
        let frame = timeline_frame_for_thread(
            AgentSessionTimelineKind::CommandOutput,
            &provider_text,
            Some("child-thread"),
        );
        let payload: Value = serde_json::from_slice(
            &frame[TIMELINE_FRAME_PREFIX.len()..frame.len().saturating_sub(1)],
        )
        .unwrap();
        let decoded = STANDARD
            .decode(payload["text_base64"].as_str().unwrap())
            .unwrap();
        let decoded = String::from_utf8(decoded).unwrap();
        assert_eq!(
            decoded.chars().count(),
            crate::agent_console::MAX_PROVIDER_TIMELINE_TEXT_CHARS
        );
        assert!(!decoded.chars().any(char::is_control));
        assert_eq!(payload["thread_id"], "child-thread");
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
        assert_eq!(message["params"]["sandbox"], "workspace-write");
    }

    #[test]
    fn starts_full_access_threads_only_when_selected() {
        let message = thread_request_message(
            Path::new("/tmp/repo"),
            None,
            AgentSessionPermissionMode::FullAccess,
        );

        assert_eq!(message["params"]["approvalPolicy"], "never");
        assert_eq!(message["params"]["sandbox"], "danger-full-access");
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
        assert_eq!(message["params"]["sandbox"], "workspace-write");
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
