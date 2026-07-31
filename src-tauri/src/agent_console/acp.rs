use std::{
    collections::{HashMap, HashSet},
    env, fmt,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use agent_client_protocol_schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, Error as RpcError, ErrorCode,
    ImageContent, InitializeRequest, InitializeResponse, JsonRpcMessage, LoadSessionRequest,
    LoadSessionResponse, NewSessionRequest, NewSessionResponse, Notification, PermissionOptionKind,
    PromptRequest, PromptResponse, Request, RequestId, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, Response, SelectedPermissionOutcome,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigOptionValue,
    SessionConfigSelectOptions, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, AGENT_METHOD_NAMES, CLIENT_METHOD_NAMES,
};
use agent_client_protocol_schema::ProtocolVersion;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use serde_json::Value;

#[cfg(not(windows))]
use super::pty::kill_process_tree;
use super::{
    commands::TIMELINE_FRAME_PREFIX,
    pty::{
        AgentProcess, AgentProcessEvent, AgentTurnAttachment, PtyHandle, TINTO_TURN_DONE_MARKER,
    },
    AgentConsoleError,
};
use crate::bus::contract::{
    AgentSessionAcpConfigCategory, AgentSessionAcpConfigOption, AgentSessionAcpConfigValue,
    AgentSessionAcpMode, AgentSessionAcpPermission, AgentSessionAcpPermissionKind,
    AgentSessionAcpPermissionOption, AgentSessionAcpPermissionState, AgentSessionAcpRuntime,
    AgentSessionAcpState, AgentSessionContextSummary, AgentSessionResumeMode,
    AgentSessionRuntimeOptions, AgentSessionTimelineKind,
};

#[cfg(windows)]
use crate::windows_process::{hide_console, KillOnCloseJob};

const INVALID_MESSAGE: &str = "mensaje ACP no válido";
const INVALID_STATE: &str = "operación ACP no válida para el estado actual";
const LIMIT_EXCEEDED: &str = "el proveedor excedió un límite ACP";
const LEGACY_MODE_CONFIG_ID: &str = "__tinto_acp_session_mode";
const MAX_CONFIG_OPTIONS: usize = 32;
const MAX_CONFIG_VALUES: usize = 64;
const MAX_CONFIG_TEXT_BYTES: usize = 512;
const PERMISSION_ACTIVITY_TEXT: &str = "El agente solicitó una decisión de permiso.";
const KIMI_ALLOWED_ENV: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "TEMP",
    "TMP",
    "LANG",
    "LC_ALL",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcpLimits {
    pub handshake_timeout: Duration,
    pub permission_timeout: Duration,
    pub cancel_grace: Duration,
    pub frame_bytes: usize,
    pub stderr_line_bytes: usize,
    pub stderr_tail_bytes: usize,
    pub event_queue: usize,
    pub pending_requests: usize,
    pub pending_permissions: usize,
    pub updates_per_turn: usize,
    pub text_bytes_per_turn: usize,
}

impl Default for AcpLimits {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(30),
            permission_timeout: Duration::from_secs(60),
            cancel_grace: Duration::from_secs(2),
            frame_bytes: 1024 * 1024,
            stderr_line_bytes: 64 * 1024,
            stderr_tail_bytes: 256 * 1024,
            event_queue: 256,
            pending_requests: 64,
            pending_permissions: 16,
            updates_per_turn: 512,
            text_bytes_per_turn: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpPhase {
    New,
    Initializing,
    Initialized,
    CreatingSession,
    Ready,
    AuthRequired,
    Failed,
    Closed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcpCapabilities {
    pub load_session: bool,
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
    pub modes: bool,
    pub models: bool,
    pub config_options: Vec<AgentSessionAcpConfigOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpCoreError {
    pub category: &'static str,
    pub message: &'static str,
}

impl AcpCoreError {
    fn protocol() -> Self {
        Self {
            category: "acp_protocol_error",
            message: INVALID_MESSAGE,
        }
    }

    fn state() -> Self {
        Self {
            category: "acp_invalid_state",
            message: INVALID_STATE,
        }
    }

    fn limit() -> Self {
        Self {
            category: "acp_limit_exceeded",
            message: LIMIT_EXCEEDED,
        }
    }
}

impl fmt::Display for AcpCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.category, self.message)
    }
}

impl std::error::Error for AcpCoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpUpdateKind {
    UserMessage,
    AgentMessage,
    AgentThought,
    ToolCall,
    ToolCallUpdate,
    Progress,
    ModeChanged,
    ConfigurationChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpUpdate {
    pub turn_id: String,
    pub kind: AcpUpdateKind,
    pub text: String,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpPermissionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpPermissionOption {
    pub id: String,
    pub label: String,
    pub kind: AcpPermissionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpPermissionRequest {
    pub request_id: RequestId,
    pub generation: u64,
    pub tinto_session_id: String,
    pub provider_session_id: String,
    pub turn_id: String,
    pub tool_call_id: String,
    pub title: String,
    pub options: Vec<AcpPermissionOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpPermissionDecision {
    Select(String),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcpEvent {
    PhaseChanged(AcpPhase),
    ProviderSessionReady {
        provider_session_id: String,
        capabilities: AcpCapabilities,
    },
    LoadSessionUnavailable,
    Update(AcpUpdate),
    PermissionRequested(AcpPermissionRequest),
    TurnCompleted {
        turn_id: String,
        stop_reason: String,
    },
    OutboundFrame(Vec<u8>),
    AuthenticationRequired,
    Failed {
        category: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingRpcKind {
    Initialize,
    NewSession,
    LoadSession,
    Prompt,
    SetMode { value_id: String },
    SetConfig { config_id: String, value_id: String },
}

#[derive(Debug, Clone)]
struct ActiveTurn {
    id: String,
    prompt_request_id: RequestId,
    updates: usize,
    text_bytes: usize,
    cancelled: bool,
    tool_call_ids: HashSet<String>,
}

pub struct AcpConnectionCore {
    provider: String,
    tinto_session_id: String,
    generation: u64,
    limits: AcpLimits,
    phase: AcpPhase,
    next_request_id: i64,
    pending: HashMap<RequestId, PendingRpcKind>,
    pending_permissions: HashMap<RequestId, AcpPermissionRequest>,
    seen_permission_requests: HashSet<RequestId>,
    provider_session_id: Option<String>,
    provider_session_valid: bool,
    capabilities: AcpCapabilities,
    session_modes: Option<SessionModeState>,
    session_config_options: Vec<SessionConfigOption>,
    active_turn: Option<ActiveTurn>,
    load_replay_updates: usize,
    load_replay_text_bytes: usize,
}

impl AcpConnectionCore {
    pub fn new(
        provider: impl Into<String>,
        tinto_session_id: impl Into<String>,
        generation: u64,
        limits: AcpLimits,
    ) -> Self {
        Self {
            provider: provider.into(),
            tinto_session_id: tinto_session_id.into(),
            generation,
            limits,
            phase: AcpPhase::New,
            next_request_id: 1,
            pending: HashMap::new(),
            pending_permissions: HashMap::new(),
            seen_permission_requests: HashSet::new(),
            provider_session_id: None,
            provider_session_valid: false,
            capabilities: AcpCapabilities::default(),
            session_modes: None,
            session_config_options: Vec::new(),
            active_turn: None,
            load_replay_updates: 0,
            load_replay_text_bytes: 0,
        }
    }

    pub fn phase(&self) -> AcpPhase {
        self.phase
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn provider_session_id(&self) -> Option<&str> {
        self.provider_session_id.as_deref()
    }

    pub fn capabilities(&self) -> &AcpCapabilities {
        &self.capabilities
    }

    pub fn pending_permission_count(&self) -> usize {
        self.pending_permissions.len()
    }

    pub fn has_active_turn(&self) -> bool {
        self.active_turn.is_some()
    }

    pub fn initialize(&mut self) -> Result<Vec<u8>, AcpCoreError> {
        if self.phase != AcpPhase::New {
            return Err(AcpCoreError::state());
        }
        let request = InitializeRequest::new(ProtocolVersion::V1)
            .client_capabilities(ClientCapabilities::default());
        let frame = self.request(
            AGENT_METHOD_NAMES.initialize,
            request,
            PendingRpcKind::Initialize,
        )?;
        self.phase = AcpPhase::Initializing;
        Ok(frame)
    }

    pub fn new_session(&mut self, cwd: &Path) -> Result<Vec<u8>, AcpCoreError> {
        if self.phase != AcpPhase::Initialized || !cwd.is_absolute() {
            return Err(AcpCoreError::state());
        }
        let frame = self.request(
            AGENT_METHOD_NAMES.session_new,
            NewSessionRequest::new(cwd),
            PendingRpcKind::NewSession,
        )?;
        self.phase = AcpPhase::CreatingSession;
        Ok(frame)
    }

    pub fn load_session(
        &mut self,
        provider_session_id: &str,
        cwd: &Path,
    ) -> Result<Vec<u8>, AcpCoreError> {
        if self.phase != AcpPhase::Initialized
            || !self.capabilities.load_session
            || !cwd.is_absolute()
            || provider_session_id.is_empty()
        {
            return Err(AcpCoreError::state());
        }
        self.provider_session_id = Some(provider_session_id.to_owned());
        let frame = self.request(
            AGENT_METHOD_NAMES.session_load,
            LoadSessionRequest::new(provider_session_id.to_owned(), cwd),
            PendingRpcKind::LoadSession,
        )?;
        self.phase = AcpPhase::CreatingSession;
        Ok(frame)
    }

    pub fn prompt_text(
        &mut self,
        turn_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Vec<u8>, AcpCoreError> {
        let text = text.into();
        self.prompt_content(turn_id, vec![ContentBlock::from(text)])
    }

    fn prompt_content(
        &mut self,
        turn_id: impl Into<String>,
        content: Vec<ContentBlock>,
    ) -> Result<Vec<u8>, AcpCoreError> {
        if content.is_empty() {
            return Err(AcpCoreError::state());
        }
        if self.phase != AcpPhase::Ready || self.active_turn.is_some() {
            return Err(AcpCoreError::state());
        }
        let session_id = self
            .provider_session_id
            .clone()
            .ok_or_else(AcpCoreError::state)?;
        let turn_id = turn_id.into();
        if turn_id.is_empty() {
            return Err(AcpCoreError::state());
        }
        let request_id = self.next_id()?;
        let request = Request {
            id: request_id.clone(),
            method: Arc::from(AGENT_METHOD_NAMES.session_prompt),
            params: Some(PromptRequest::new(session_id, content)),
        };
        let frame = serialize_message(JsonRpcMessage::wrap(request))?;
        if frame.len() > self.limits.frame_bytes {
            return Err(AcpCoreError::limit());
        }
        self.pending
            .insert(request_id.clone(), PendingRpcKind::Prompt);
        self.active_turn = Some(ActiveTurn {
            id: turn_id,
            prompt_request_id: request_id,
            updates: 0,
            text_bytes: 0,
            cancelled: false,
            tool_call_ids: HashSet::new(),
        });
        Ok(frame)
    }

    pub fn set_config_option(
        &mut self,
        config_id: &str,
        value_id: &str,
    ) -> Result<Vec<u8>, AcpCoreError> {
        if self.phase != AcpPhase::Ready || self.active_turn.is_some() {
            return Err(AcpCoreError::state());
        }
        let session_id = self
            .provider_session_id
            .clone()
            .ok_or_else(AcpCoreError::state)?;
        let option = self
            .capabilities
            .config_options
            .iter()
            .find(|option| option.id == config_id)
            .ok_or_else(AcpCoreError::state)?;
        if !option.values.iter().any(|value| value.id == value_id) {
            return Err(AcpCoreError::state());
        }
        if config_id == LEGACY_MODE_CONFIG_ID {
            self.request(
                AGENT_METHOD_NAMES.session_set_mode,
                SetSessionModeRequest::new(session_id, value_id.to_owned()),
                PendingRpcKind::SetMode {
                    value_id: value_id.to_owned(),
                },
            )
        } else {
            self.request(
                AGENT_METHOD_NAMES.session_set_config_option,
                SetSessionConfigOptionRequest::new(
                    session_id,
                    config_id.to_owned(),
                    SessionConfigOptionValue::value_id(value_id.to_owned()),
                ),
                PendingRpcKind::SetConfig {
                    config_id: config_id.to_owned(),
                    value_id: value_id.to_owned(),
                },
            )
        }
    }

    pub fn cancel_turn(&mut self) -> Result<Vec<Vec<u8>>, AcpCoreError> {
        if self.phase != AcpPhase::Ready || self.active_turn.is_none() {
            return Err(AcpCoreError::state());
        }
        if let Some(turn) = self.active_turn.as_mut() {
            turn.cancelled = true;
        }
        let session_id = self
            .provider_session_id
            .clone()
            .ok_or_else(AcpCoreError::state)?;
        let notification = Notification {
            method: Arc::from(AGENT_METHOD_NAMES.session_cancel),
            params: Some(CancelNotification::new(session_id)),
        };
        let mut frames = vec![serialize_message(JsonRpcMessage::wrap(notification))?];
        let permission_ids = self.pending_permissions.keys().cloned().collect::<Vec<_>>();
        for request_id in permission_ids {
            frames.push(self.respond_permission(&request_id, AcpPermissionDecision::Cancel)?);
        }
        Ok(frames)
    }

    pub fn respond_permission(
        &mut self,
        request_id: &RequestId,
        decision: AcpPermissionDecision,
    ) -> Result<Vec<u8>, AcpCoreError> {
        let permission = self
            .pending_permissions
            .get(request_id)
            .ok_or_else(AcpCoreError::state)?;
        let outcome = match decision {
            AcpPermissionDecision::Cancel => RequestPermissionOutcome::Cancelled,
            AcpPermissionDecision::Select(option_id) => {
                if !permission
                    .options
                    .iter()
                    .any(|option| option.id == option_id)
                {
                    return Err(AcpCoreError::state());
                }
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
            }
        };
        let response = Response::new(
            request_id.clone(),
            Ok(RequestPermissionResponse::new(outcome)),
        );
        let frame = serialize_message(JsonRpcMessage::wrap(response))?;
        self.pending_permissions.remove(request_id);
        Ok(frame)
    }

    pub fn close(&mut self) -> Result<Vec<Vec<u8>>, AcpCoreError> {
        let permission_ids = self.pending_permissions.keys().cloned().collect::<Vec<_>>();
        let mut frames = Vec::with_capacity(permission_ids.len());
        for request_id in permission_ids {
            frames.push(self.respond_permission(&request_id, AcpPermissionDecision::Cancel)?);
        }
        self.pending.clear();
        self.active_turn = None;
        self.phase = AcpPhase::Closed;
        Ok(frames)
    }

    pub fn handle_frame(
        &mut self,
        generation: u64,
        frame: &[u8],
    ) -> Result<Vec<AcpEvent>, AcpCoreError> {
        if generation != self.generation || self.phase == AcpPhase::Closed {
            return Err(AcpCoreError::protocol());
        }
        if frame.is_empty() || frame.len() > self.limits.frame_bytes {
            return Err(AcpCoreError::limit());
        }
        let value: Value = serde_json::from_slice(frame).map_err(|_| AcpCoreError::protocol())?;
        let envelope = classify_envelope(&value)?;
        match envelope {
            Envelope::Response { id, result, error } => self.handle_response(id, result, error),
            Envelope::Request { id, method, params } => self.handle_request(id, method, params),
            Envelope::Notification { method, params } => self.handle_notification(method, params),
        }
    }

    fn request<T: Serialize>(
        &mut self,
        method: &'static str,
        params: T,
        kind: PendingRpcKind,
    ) -> Result<Vec<u8>, AcpCoreError> {
        let id = self.next_id()?;
        let request = Request {
            id: id.clone(),
            method: Arc::from(method),
            params: Some(params),
        };
        let frame = serialize_message(JsonRpcMessage::wrap(request))?;
        if frame.len() > self.limits.frame_bytes {
            return Err(AcpCoreError::limit());
        }
        self.pending.insert(id, kind);
        Ok(frame)
    }

    fn next_id(&mut self) -> Result<RequestId, AcpCoreError> {
        if self.pending.len() >= self.limits.pending_requests {
            return Err(AcpCoreError::limit());
        }
        let id = RequestId::Number(self.next_request_id);
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(AcpCoreError::limit)?;
        Ok(id)
    }

    fn handle_response(
        &mut self,
        id: RequestId,
        result: Option<Value>,
        error: Option<Value>,
    ) -> Result<Vec<AcpEvent>, AcpCoreError> {
        let kind = self
            .pending
            .get(&id)
            .cloned()
            .ok_or_else(AcpCoreError::protocol)?;
        if let Some(error) = error {
            let error: RpcError = typed(error)?;
            self.pending.remove(&id);
            return Ok(self.handle_rpc_error(kind, error));
        }
        let result = result.ok_or_else(AcpCoreError::protocol)?;
        let events = match kind {
            PendingRpcKind::Initialize => {
                if self.phase != AcpPhase::Initializing {
                    return Err(AcpCoreError::protocol());
                }
                let response: InitializeResponse = typed(result)?;
                if response.protocol_version != ProtocolVersion::V1 {
                    self.phase = AcpPhase::Failed;
                    vec![
                        AcpEvent::PhaseChanged(AcpPhase::Failed),
                        AcpEvent::Failed {
                            category: "acp_version_unsupported",
                            message: "el proveedor no negoció ACP v1",
                        },
                    ]
                } else {
                    self.capabilities.load_session = response.agent_capabilities.load_session;
                    self.capabilities.image = response.agent_capabilities.prompt_capabilities.image;
                    self.capabilities.audio = response.agent_capabilities.prompt_capabilities.audio;
                    self.capabilities.embedded_context = response
                        .agent_capabilities
                        .prompt_capabilities
                        .embedded_context;
                    self.phase = AcpPhase::Initialized;
                    vec![AcpEvent::PhaseChanged(AcpPhase::Initialized)]
                }
            }
            PendingRpcKind::NewSession => {
                if self.phase != AcpPhase::CreatingSession {
                    return Err(AcpCoreError::protocol());
                }
                let response: NewSessionResponse = typed(result)?;
                let provider_session_id = response.session_id.to_string();
                if provider_session_id.is_empty() {
                    return Err(AcpCoreError::protocol());
                }
                self.apply_session_options(response.modes, response.config_options)?;
                self.provider_session_id = Some(provider_session_id.clone());
                self.provider_session_valid = true;
                self.phase = AcpPhase::Ready;
                vec![
                    AcpEvent::PhaseChanged(AcpPhase::Ready),
                    AcpEvent::ProviderSessionReady {
                        provider_session_id,
                        capabilities: self.capabilities.clone(),
                    },
                ]
            }
            PendingRpcKind::LoadSession => {
                if self.phase != AcpPhase::CreatingSession {
                    return Err(AcpCoreError::protocol());
                }
                let response: LoadSessionResponse = typed(result)?;
                self.apply_session_options(response.modes, response.config_options)?;
                let provider_session_id = self
                    .provider_session_id
                    .clone()
                    .ok_or_else(AcpCoreError::protocol)?;
                self.provider_session_valid = true;
                self.phase = AcpPhase::Ready;
                vec![
                    AcpEvent::PhaseChanged(AcpPhase::Ready),
                    AcpEvent::ProviderSessionReady {
                        provider_session_id,
                        capabilities: self.capabilities.clone(),
                    },
                ]
            }
            PendingRpcKind::Prompt => {
                if self.phase != AcpPhase::Ready {
                    return Err(AcpCoreError::protocol());
                }
                let response: PromptResponse = typed(result)?;
                let turn = self.active_turn.take().ok_or_else(AcpCoreError::protocol)?;
                if turn.prompt_request_id != id {
                    return Err(AcpCoreError::protocol());
                }
                let stop_reason = serde_json::to_value(response.stop_reason)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "completed".to_owned());
                let permission_ids = self
                    .pending_permissions
                    .iter()
                    .filter(|(_, permission)| permission.turn_id == turn.id)
                    .map(|(request_id, _)| request_id.clone())
                    .collect::<Vec<_>>();
                let mut events = permission_ids
                    .into_iter()
                    .map(|request_id| {
                        self.respond_permission(&request_id, AcpPermissionDecision::Cancel)
                            .map(AcpEvent::OutboundFrame)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                events.push(AcpEvent::TurnCompleted {
                    turn_id: turn.id,
                    stop_reason,
                });
                events
            }
            PendingRpcKind::SetMode { value_id } => {
                if self.phase != AcpPhase::Ready || self.active_turn.is_some() {
                    return Err(AcpCoreError::protocol());
                }
                let _: SetSessionModeResponse = typed(result)?;
                let modes = self
                    .session_modes
                    .as_mut()
                    .ok_or_else(AcpCoreError::protocol)?;
                if !modes
                    .available_modes
                    .iter()
                    .any(|mode| mode.id.to_string() == value_id)
                {
                    return Err(AcpCoreError::protocol());
                }
                modes.current_mode_id = value_id.into();
                self.refresh_public_config_options()?;
                vec![AcpEvent::Update(AcpUpdate {
                    turn_id: String::new(),
                    kind: AcpUpdateKind::ConfigurationChanged,
                    text: "Configuración actualizada".to_owned(),
                    tool_call_id: None,
                })]
            }
            PendingRpcKind::SetConfig {
                config_id,
                value_id,
            } => {
                if self.phase != AcpPhase::Ready || self.active_turn.is_some() {
                    return Err(AcpCoreError::protocol());
                }
                let response: SetSessionConfigOptionResponse = typed(result)?;
                self.apply_config_options(response.config_options)?;
                let updated = self
                    .capabilities
                    .config_options
                    .iter()
                    .any(|option| option.id == config_id && option.current_value == value_id);
                if !updated {
                    return Err(AcpCoreError::protocol());
                }
                vec![AcpEvent::Update(AcpUpdate {
                    turn_id: String::new(),
                    kind: AcpUpdateKind::ConfigurationChanged,
                    text: "Configuración actualizada".to_owned(),
                    tool_call_id: None,
                })]
            }
        };
        self.pending.remove(&id);
        Ok(events)
    }

    fn handle_rpc_error(&mut self, kind: PendingRpcKind, error: RpcError) -> Vec<AcpEvent> {
        let pre_session = !self.provider_session_valid;
        if error.code == ErrorCode::AuthRequired {
            self.active_turn = None;
            if pre_session {
                self.phase = AcpPhase::AuthRequired;
                return vec![
                    AcpEvent::PhaseChanged(AcpPhase::AuthRequired),
                    AcpEvent::AuthenticationRequired,
                ];
            }
            self.phase = AcpPhase::Failed;
            return vec![
                AcpEvent::PhaseChanged(AcpPhase::Failed),
                AcpEvent::Failed {
                    category: "acp_authentication_expired",
                    message: "La autenticación del agente caducó. Inicia sesión desde su CLI y reintenta; Tinto no recibe ni guarda credenciales. No se reenvió ni reprodujo el turno mediante PTY.",
                },
            ];
        }
        if kind == PendingRpcKind::LoadSession && pre_session {
            self.provider_session_id = None;
            self.phase = AcpPhase::Initialized;
            return vec![
                AcpEvent::PhaseChanged(AcpPhase::Initialized),
                AcpEvent::LoadSessionUnavailable,
            ];
        }
        self.phase = AcpPhase::Failed;
        self.active_turn = None;
        vec![
            AcpEvent::PhaseChanged(AcpPhase::Failed),
            AcpEvent::Failed {
                category: "acp_provider_error",
                message: "el proveedor devolvió un error ACP",
            },
        ]
    }

    fn handle_notification(
        &mut self,
        method: String,
        params: Value,
    ) -> Result<Vec<AcpEvent>, AcpCoreError> {
        if method != CLIENT_METHOD_NAMES.session_update {
            return Err(AcpCoreError::protocol());
        }
        let notification: SessionNotification = typed(params)?;
        self.require_current_session(&notification.session_id.to_string())?;
        if self.phase == AcpPhase::CreatingSession
            && self
                .pending
                .values()
                .any(|kind| kind == &PendingRpcKind::LoadSession)
        {
            self.account_load_replay(&notification.update)?;
            return Ok(Vec::new());
        }
        if self.phase != AcpPhase::Ready {
            return Err(AcpCoreError::protocol());
        }
        let update = self.normalize_update(notification.update)?;
        Ok(vec![AcpEvent::Update(update)])
    }

    fn handle_request(
        &mut self,
        id: RequestId,
        method: String,
        params: Value,
    ) -> Result<Vec<AcpEvent>, AcpCoreError> {
        if method == CLIENT_METHOD_NAMES.session_request_permission {
            return self.handle_permission_request(id, params);
        }
        let response: Response<Value> = Response::new(id, Err(RpcError::method_not_found()));
        Ok(vec![AcpEvent::OutboundFrame(serialize_message(
            JsonRpcMessage::wrap(response),
        )?)])
    }

    fn handle_permission_request(
        &mut self,
        id: RequestId,
        params: Value,
    ) -> Result<Vec<AcpEvent>, AcpCoreError> {
        if matches!(&id, RequestId::Null)
            || matches!(&id, RequestId::Str(value) if value.len() > 256)
        {
            return Err(AcpCoreError::protocol());
        }
        if self.phase != AcpPhase::Ready
            || self.pending_permissions.contains_key(&id)
            || self.seen_permission_requests.contains(&id)
            || self.pending_permissions.len() >= self.limits.pending_permissions
        {
            return Err(AcpCoreError::protocol());
        }
        if self.seen_permission_requests.len() >= self.limits.pending_requests {
            return Err(AcpCoreError::limit());
        }
        let request: RequestPermissionRequest = typed(params)?;
        self.require_current_session(&request.session_id.to_string())?;
        let turn_id = self
            .active_turn
            .as_ref()
            .map(|turn| turn.id.clone())
            .ok_or_else(AcpCoreError::protocol)?;
        if request.options.is_empty() {
            return Err(AcpCoreError::protocol());
        }
        let mut option_ids = HashSet::new();
        let option_text_bytes = request.options.iter().try_fold(0usize, |total, option| {
            if !option_ids.insert(option.option_id.to_string()) {
                return Err(AcpCoreError::protocol());
            }
            total
                .checked_add(option.name.len())
                .ok_or_else(AcpCoreError::limit)
        })?;
        let title = request
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| "Acción solicitada".to_owned());
        let tool_call_id = request.tool_call.tool_call_id.to_string();
        self.active_turn
            .as_mut()
            .ok_or_else(AcpCoreError::protocol)?
            .tool_call_ids
            .insert(tool_call_id.clone());
        self.account_text(title.len())?;
        self.account_text(option_text_bytes)?;
        let permission = AcpPermissionRequest {
            request_id: id.clone(),
            generation: self.generation,
            tinto_session_id: self.tinto_session_id.clone(),
            provider_session_id: request.session_id.to_string(),
            turn_id,
            tool_call_id,
            title,
            options: request
                .options
                .into_iter()
                .map(|option| AcpPermissionOption {
                    id: option.option_id.to_string(),
                    label: option.name,
                    kind: match option.kind {
                        PermissionOptionKind::AllowOnce => AcpPermissionKind::AllowOnce,
                        PermissionOptionKind::AllowAlways => AcpPermissionKind::AllowAlways,
                        PermissionOptionKind::RejectOnce => AcpPermissionKind::RejectOnce,
                        PermissionOptionKind::RejectAlways => AcpPermissionKind::RejectAlways,
                        _ => AcpPermissionKind::RejectOnce,
                    },
                })
                .collect(),
        };
        self.seen_permission_requests.insert(id.clone());
        self.pending_permissions.insert(id, permission.clone());
        Ok(vec![AcpEvent::PermissionRequested(permission)])
    }

    fn normalize_update(&mut self, update: SessionUpdate) -> Result<AcpUpdate, AcpCoreError> {
        let runtime_configuration = matches!(
            &update,
            SessionUpdate::CurrentModeUpdate(_) | SessionUpdate::ConfigOptionUpdate(_)
        );
        if !runtime_configuration {
            self.account_update()?;
        }
        let (kind, text, tool_call_id) = match update {
            SessionUpdate::UserMessageChunk(chunk) => (
                AcpUpdateKind::UserMessage,
                text_from_content(chunk.content),
                None,
            ),
            SessionUpdate::AgentMessageChunk(chunk) => (
                AcpUpdateKind::AgentMessage,
                text_from_content(chunk.content),
                None,
            ),
            SessionUpdate::AgentThoughtChunk(chunk) => (
                AcpUpdateKind::AgentThought,
                text_from_content(chunk.content),
                None,
            ),
            SessionUpdate::ToolCall(tool) => {
                let tool_call_id = tool.tool_call_id.to_string();
                let inserted = self
                    .active_turn
                    .as_mut()
                    .ok_or_else(AcpCoreError::protocol)?
                    .tool_call_ids
                    .insert(tool_call_id.clone());
                if !inserted {
                    return Err(AcpCoreError::protocol());
                }
                (AcpUpdateKind::ToolCall, tool.title, Some(tool_call_id))
            }
            SessionUpdate::ToolCallUpdate(tool) => {
                let tool_call_id = tool.tool_call_id.to_string();
                let known = self
                    .active_turn
                    .as_ref()
                    .is_some_and(|turn| turn.tool_call_ids.contains(&tool_call_id));
                if !known {
                    return Err(AcpCoreError::protocol());
                }
                (
                    AcpUpdateKind::ToolCallUpdate,
                    tool.fields
                        .title
                        .unwrap_or_else(|| "Herramienta actualizada".to_owned()),
                    Some(tool_call_id),
                )
            }
            SessionUpdate::CurrentModeUpdate(mode) => {
                let mode_id = mode.current_mode_id.to_string();
                let modes = self
                    .session_modes
                    .as_mut()
                    .ok_or_else(AcpCoreError::protocol)?;
                if !modes
                    .available_modes
                    .iter()
                    .any(|available| available.id.to_string() == mode_id)
                {
                    return Err(AcpCoreError::protocol());
                }
                modes.current_mode_id = mode.current_mode_id;
                self.refresh_public_config_options()?;
                (AcpUpdateKind::ModeChanged, mode_id, None)
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.apply_config_options(update.config_options)?;
                (
                    AcpUpdateKind::ConfigurationChanged,
                    "Configuración actualizada".to_owned(),
                    None,
                )
            }
            SessionUpdate::Plan(_) => {
                (AcpUpdateKind::Progress, "Plan actualizado".to_owned(), None)
            }
            SessionUpdate::AvailableCommandsUpdate(_) => (
                AcpUpdateKind::Progress,
                "Comandos disponibles actualizados".to_owned(),
                None,
            ),
            SessionUpdate::SessionInfoUpdate(_) => (
                AcpUpdateKind::Progress,
                "Información de sesión actualizada".to_owned(),
                None,
            ),
            SessionUpdate::UsageUpdate(_) => {
                (AcpUpdateKind::Progress, "Uso actualizado".to_owned(), None)
            }
            _ => (
                AcpUpdateKind::Progress,
                "Progreso actualizado".to_owned(),
                None,
            ),
        };
        if !runtime_configuration {
            self.account_text(text.len())?;
        }
        let turn_id = self
            .active_turn
            .as_ref()
            .map(|turn| turn.id.clone())
            .unwrap_or_default();
        Ok(AcpUpdate {
            turn_id,
            kind,
            text,
            tool_call_id,
        })
    }

    fn account_update(&mut self) -> Result<(), AcpCoreError> {
        let turn = self
            .active_turn
            .as_mut()
            .ok_or_else(AcpCoreError::protocol)?;
        if turn.updates >= self.limits.updates_per_turn {
            return Err(AcpCoreError::limit());
        }
        turn.updates += 1;
        Ok(())
    }

    fn account_text(&mut self, bytes: usize) -> Result<(), AcpCoreError> {
        let turn = self
            .active_turn
            .as_mut()
            .ok_or_else(AcpCoreError::protocol)?;
        turn.text_bytes = turn
            .text_bytes
            .checked_add(bytes)
            .ok_or_else(AcpCoreError::limit)?;
        if turn.text_bytes > self.limits.text_bytes_per_turn {
            return Err(AcpCoreError::limit());
        }
        Ok(())
    }

    fn account_load_replay(&mut self, update: &SessionUpdate) -> Result<(), AcpCoreError> {
        if self.load_replay_updates >= self.limits.updates_per_turn {
            return Err(AcpCoreError::limit());
        }
        self.load_replay_updates += 1;
        self.load_replay_text_bytes = self
            .load_replay_text_bytes
            .checked_add(session_update_text_bytes(update))
            .ok_or_else(AcpCoreError::limit)?;
        if self.load_replay_text_bytes > self.limits.text_bytes_per_turn {
            return Err(AcpCoreError::limit());
        }
        Ok(())
    }

    fn require_current_session(&self, session_id: &str) -> Result<(), AcpCoreError> {
        if self.provider_session_id.as_deref() == Some(session_id) {
            Ok(())
        } else {
            Err(AcpCoreError::protocol())
        }
    }

    fn apply_session_options(
        &mut self,
        modes: Option<SessionModeState>,
        options: Option<Vec<SessionConfigOption>>,
    ) -> Result<(), AcpCoreError> {
        self.session_modes = modes;
        self.session_config_options = options.unwrap_or_default();
        self.refresh_public_config_options()
    }

    fn apply_config_options(
        &mut self,
        options: Vec<SessionConfigOption>,
    ) -> Result<(), AcpCoreError> {
        self.session_config_options = options;
        self.refresh_public_config_options()
    }

    fn refresh_public_config_options(&mut self) -> Result<(), AcpCoreError> {
        let mut projected = Vec::new();
        if let Some(modes) = &self.session_modes {
            if modes.available_modes.is_empty()
                || modes.available_modes.len() > MAX_CONFIG_VALUES
                || !modes
                    .available_modes
                    .iter()
                    .any(|mode| mode.id.to_string() == modes.current_mode_id.to_string())
            {
                return Err(AcpCoreError::protocol());
            }
            let mut ids = HashSet::new();
            let values = modes
                .available_modes
                .iter()
                .map(|mode| {
                    let id = mode.id.to_string();
                    validate_config_text(&id)?;
                    validate_config_text(&mode.name)?;
                    if !ids.insert(id.clone()) {
                        return Err(AcpCoreError::protocol());
                    }
                    Ok(AgentSessionAcpConfigValue {
                        id,
                        label: mode.name.clone(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            projected.push(AgentSessionAcpConfigOption {
                id: LEGACY_MODE_CONFIG_ID.to_owned(),
                label: "Modo".to_owned(),
                category: AgentSessionAcpConfigCategory::Mode,
                current_value: modes.current_mode_id.to_string(),
                values,
            });
        }

        if self.session_config_options.len() > MAX_CONFIG_OPTIONS {
            return Err(AcpCoreError::limit());
        }
        for option in &self.session_config_options {
            let category = match option.category {
                Some(SessionConfigOptionCategory::Mode) => AgentSessionAcpConfigCategory::Mode,
                Some(
                    SessionConfigOptionCategory::Model | SessionConfigOptionCategory::ModelConfig,
                ) => AgentSessionAcpConfigCategory::Model,
                _ => continue,
            };
            let SessionConfigKind::Select(select) = &option.kind else {
                continue;
            };
            let id = option.id.to_string();
            let current_value = select.current_value.to_string();
            if id == LEGACY_MODE_CONFIG_ID {
                return Err(AcpCoreError::protocol());
            }
            validate_config_text(&id)?;
            validate_config_text(&option.name)?;
            validate_config_text(&current_value)?;
            let values = projected_config_values(&select.options)?;
            if values.is_empty() || !values.iter().any(|value| value.id == current_value) {
                return Err(AcpCoreError::protocol());
            }
            projected.push(AgentSessionAcpConfigOption {
                id,
                label: option.name.clone(),
                category,
                current_value,
                values,
            });
            if projected.len() > MAX_CONFIG_OPTIONS {
                return Err(AcpCoreError::limit());
            }
        }
        self.capabilities.modes = projected
            .iter()
            .any(|option| option.category == AgentSessionAcpConfigCategory::Mode);
        self.capabilities.models = projected
            .iter()
            .any(|option| option.category == AgentSessionAcpConfigCategory::Model);
        self.capabilities.config_options = projected;
        Ok(())
    }
}

fn projected_config_values(
    options: &SessionConfigSelectOptions,
) -> Result<Vec<AgentSessionAcpConfigValue>, AcpCoreError> {
    let values = match options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| (option.value.to_string(), option.name.clone()))
            .collect::<Vec<_>>(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| {
                group
                    .options
                    .iter()
                    .map(|option| (option.value.to_string(), option.name.clone()))
            })
            .collect::<Vec<_>>(),
        _ => return Err(AcpCoreError::protocol()),
    };
    if values.len() > MAX_CONFIG_VALUES {
        return Err(AcpCoreError::limit());
    }
    let mut ids = HashSet::new();
    values
        .into_iter()
        .map(|(id, label)| {
            validate_config_text(&id)?;
            validate_config_text(&label)?;
            if !ids.insert(id.clone()) {
                return Err(AcpCoreError::protocol());
            }
            Ok(AgentSessionAcpConfigValue { id, label })
        })
        .collect()
}

fn validate_config_text(value: &str) -> Result<(), AcpCoreError> {
    if value.is_empty() || value.len() > MAX_CONFIG_TEXT_BYTES {
        Err(AcpCoreError::limit())
    } else {
        Ok(())
    }
}

fn text_from_content(content: ContentBlock) -> String {
    match content {
        ContentBlock::Text(text) => text.text,
        ContentBlock::Image(_) => "[Imagen]".to_owned(),
        ContentBlock::Audio(_) => "[Audio]".to_owned(),
        ContentBlock::ResourceLink(_) | ContentBlock::Resource(_) => "[Contexto]".to_owned(),
        _ => "[Contenido]".to_owned(),
    }
}

fn session_update_text_bytes(update: &SessionUpdate) -> usize {
    match update {
        SessionUpdate::UserMessageChunk(chunk)
        | SessionUpdate::AgentMessageChunk(chunk)
        | SessionUpdate::AgentThoughtChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => text.text.len(),
            _ => 0,
        },
        SessionUpdate::ToolCall(tool) => tool.title.len(),
        SessionUpdate::ToolCallUpdate(tool) => tool.fields.title.as_deref().map_or(0, str::len),
        _ => 0,
    }
}

fn typed<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, AcpCoreError> {
    serde_json::from_value(value).map_err(|_| AcpCoreError::protocol())
}

fn serialize_message<T: Serialize>(message: T) -> Result<Vec<u8>, AcpCoreError> {
    serde_json::to_vec(&message).map_err(|_| AcpCoreError::protocol())
}

enum Envelope {
    Response {
        id: RequestId,
        result: Option<Value>,
        error: Option<Value>,
    },
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

fn classify_envelope(value: &Value) -> Result<Envelope, AcpCoreError> {
    let object = value.as_object().ok_or_else(AcpCoreError::protocol)?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(AcpCoreError::protocol());
    }
    let method = object.get("method");
    let result = object.get("result");
    let error = object.get("error");
    if let Some(method) = method {
        if result.is_some() || error.is_some() {
            return Err(AcpCoreError::protocol());
        }
        let method = method
            .as_str()
            .filter(|method| !method.is_empty())
            .ok_or_else(AcpCoreError::protocol)?
            .to_owned();
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        if let Some(id) = object.get("id") {
            Ok(Envelope::Request {
                id: typed(id.clone())?,
                method,
                params,
            })
        } else {
            Ok(Envelope::Notification { method, params })
        }
    } else {
        if object.get("id").is_none() || (result.is_some() == error.is_some()) {
            return Err(AcpCoreError::protocol());
        }
        Ok(Envelope::Response {
            id: typed(object["id"].clone())?,
            result: result.cloned(),
            error: error.cloned(),
        })
    }
}

pub fn read_bounded_ndjson_frame<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Ok(Some(frame))
            };
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(buffer.len(), |index| index);
        let trailing_cr = take > 0 && buffer[take - 1] == b'\r';
        let payload_take = take - usize::from(newline.is_some() && trailing_cr);
        if frame.len().saturating_add(payload_take) > max_bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidData, LIMIT_EXCEEDED));
        }
        frame.extend_from_slice(&buffer[..payload_take]);
        let consumed = take + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            if !trailing_cr && frame.last() == Some(&b'\r') {
                frame.pop();
            }
            return Ok(Some(frame));
        }
    }
}

#[derive(Debug, Clone)]
pub struct AcpStderrTail {
    line_bytes: usize,
    tail_bytes: usize,
    bytes: Vec<u8>,
}

impl AcpStderrTail {
    pub fn new(limits: AcpLimits) -> Self {
        Self {
            line_bytes: limits.stderr_line_bytes,
            tail_bytes: limits.stderr_tail_bytes,
            bytes: Vec::new(),
        }
    }

    pub fn push_line(&mut self, line: &[u8]) {
        let line = &line[..line.len().min(self.line_bytes)];
        self.bytes.extend_from_slice(line);
        self.bytes.push(b'\n');
        if self.bytes.len() > self.tail_bytes {
            let remove = self.bytes.len() - self.tail_bytes;
            self.bytes.drain(..remove);
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone)]
pub enum AcpLaunchIntent {
    NewSession,
    LoadSession {
        provider_session_id: String,
        fallback_context: AgentSessionContextSummary,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcpProvider {
    Kimi,
    OpenCode,
}

impl AcpProvider {
    fn id(self) -> &'static str {
        match self {
            Self::Kimi => "kimi",
            Self::OpenCode => "opencode",
        }
    }

    fn thread_name(self) -> &'static str {
        match self {
            Self::Kimi => "tinto-kimi-acp",
            Self::OpenCode => "tinto-opencode-acp",
        }
    }
}

struct SupervisorShared {
    pid: Option<u32>,
    exit_code: Option<i32>,
    provider_session_id: Option<String>,
    generation: u64,
    runtime: AgentSessionAcpRuntime,
    permissions: Vec<AgentSessionAcpPermission>,
}

impl SupervisorShared {
    fn connecting() -> Self {
        Self {
            pid: None,
            exit_code: None,
            provider_session_id: None,
            generation: 1,
            runtime: AgentSessionAcpRuntime {
                state: AgentSessionAcpState::ConnectingAcp,
                mode: None,
                detail: None,
                lost_capabilities: Vec::new(),
                retry_available: false,
                image_attachments: false,
                config_options: Vec::new(),
            },
            permissions: Vec::new(),
        }
    }
}

enum SupervisorControl {
    Prompt {
        turn_id: String,
        text: String,
        attachments: Vec<AgentTurnAttachment>,
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
    Resize {
        cols: u16,
        rows: u16,
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
    Retry {
        confirmed: bool,
        turn_idle: bool,
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
    Permission {
        permission_id: String,
        option_id: Option<String>,
        deny: bool,
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
    Config {
        config_id: String,
        value_id: String,
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
    Stop {
        reply: mpsc::Sender<Result<(), AgentConsoleError>>,
    },
}

pub struct AcpProcessSupervisor {
    control_tx: SyncSender<SupervisorControl>,
    event_rx: Receiver<AgentProcessEvent>,
    output_reader: Option<AcpChannelReader>,
    shared: Arc<Mutex<SupervisorShared>>,
    worker: Option<JoinHandle<()>>,
    line_buffer: Vec<u8>,
    next_turn_id: AtomicU64,
    resume_result: Option<Receiver<Result<AgentSessionResumeMode, AgentConsoleError>>>,
}

impl AcpProcessSupervisor {
    pub fn spawn(
        binary_path: PathBuf,
        working_dir: PathBuf,
        tinto_session_id: String,
        intent: AcpLaunchIntent,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_provider(
            AcpProvider::Kimi,
            binary_path,
            working_dir,
            tinto_session_id,
            intent,
        )
    }

    pub fn spawn_opencode(
        binary_path: PathBuf,
        working_dir: PathBuf,
        tinto_session_id: String,
        intent: AcpLaunchIntent,
    ) -> Result<Self, AgentConsoleError> {
        Self::spawn_provider(
            AcpProvider::OpenCode,
            binary_path,
            working_dir,
            tinto_session_id,
            intent,
        )
    }

    fn spawn_provider(
        provider: AcpProvider,
        binary_path: PathBuf,
        working_dir: PathBuf,
        tinto_session_id: String,
        intent: AcpLaunchIntent,
    ) -> Result<Self, AgentConsoleError> {
        let limits = AcpLimits::default();
        let (control_tx, control_rx) = mpsc::sync_channel(64);
        let (event_tx, event_rx) = mpsc::sync_channel(limits.event_queue);
        let (output_tx, output_rx) = mpsc::sync_channel(limits.event_queue);
        let (resume_tx, resume_result) = if matches!(&intent, AcpLaunchIntent::LoadSession { .. }) {
            let (tx, rx) = mpsc::channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let shared = Arc::new(Mutex::new(SupervisorShared::connecting()));
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name(provider.thread_name().to_owned())
            .spawn(move || {
                supervisor_worker(
                    provider,
                    binary_path,
                    working_dir,
                    tinto_session_id,
                    intent,
                    limits,
                    control_rx,
                    event_tx,
                    output_tx,
                    worker_shared,
                    resume_tx,
                );
            })
            .map_err(|_| {
                AgentConsoleError::new(
                    "acp_worker_spawn_failed",
                    "no se pudo iniciar el supervisor ACP",
                )
            })?;
        Ok(Self {
            control_tx,
            event_rx,
            output_reader: Some(AcpChannelReader::new(output_rx)),
            shared,
            worker: Some(worker),
            line_buffer: Vec::new(),
            next_turn_id: AtomicU64::new(1),
            resume_result,
        })
    }

    fn send_prompt(
        &self,
        text: String,
        attachments: Vec<AgentTurnAttachment>,
    ) -> Result<(), AgentConsoleError> {
        let turn_id = format!("turn-{}", self.next_turn_id.fetch_add(1, Ordering::SeqCst));
        let (reply_tx, reply_rx) = mpsc::channel();
        self.try_send_control(SupervisorControl::Prompt {
            turn_id,
            text,
            attachments,
            reply: reply_tx,
        })?;
        receive_control_reply(reply_rx)
    }

    fn write_buffered_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        let mut prompts = Vec::new();
        for byte in input {
            match *byte {
                b'\r' => {
                    let text = String::from_utf8_lossy(&self.line_buffer)
                        .trim_end_matches(['\r', '\n'])
                        .to_owned();
                    self.line_buffer.clear();
                    if !text.is_empty() {
                        prompts.push(text);
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
        for prompt in prompts {
            self.send_prompt(prompt, Vec::new())?;
        }
        Ok(())
    }

    fn try_send_control(&self, control: SupervisorControl) -> Result<(), AgentConsoleError> {
        try_send_supervisor_control(&self.control_tx, control)
    }

    fn respond_permission(
        &self,
        permission_id: &str,
        option_id: Option<&str>,
        deny: bool,
    ) -> Result<(), AgentConsoleError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.try_send_control(SupervisorControl::Permission {
            permission_id: permission_id.to_owned(),
            option_id: option_id.map(str::to_owned),
            deny,
            reply: reply_tx,
        })?;
        receive_control_reply(reply_rx)
    }

    fn set_config_option(&self, config_id: &str, value_id: &str) -> Result<(), AgentConsoleError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.try_send_control(SupervisorControl::Config {
            config_id: config_id.to_owned(),
            value_id: value_id.to_owned(),
            reply: reply_tx,
        })?;
        receive_control_reply(reply_rx)
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn try_send_supervisor_control(
    control_tx: &SyncSender<SupervisorControl>,
    control: SupervisorControl,
) -> Result<(), AgentConsoleError> {
    control_tx.try_send(control).map_err(|error| match error {
        TrySendError::Full(_) => {
            AgentConsoleError::new("acp_control_queue_full", "el supervisor ACP está ocupado")
        }
        TrySendError::Disconnected(_) => {
            AgentConsoleError::new("acp_supervisor_stopped", "el supervisor ACP ya terminó")
        }
    })
}

impl AgentProcess for AcpProcessSupervisor {
    fn pid(&self) -> Option<u32> {
        self.shared.lock().ok().and_then(|shared| shared.pid)
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        let exit_code = self.shared.lock().ok().and_then(|shared| shared.exit_code);
        if exit_code.is_some() {
            self.join_worker();
        }
        Ok(exit_code)
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        if self
            .shared
            .lock()
            .ok()
            .and_then(|shared| shared.exit_code)
            .is_none()
        {
            let (reply_tx, reply_rx) = mpsc::channel();
            self.try_send_control(SupervisorControl::Stop { reply: reply_tx })?;
            receive_control_reply_with_timeout(reply_rx, Duration::from_secs(5))?;
        }
        self.join_worker();
        Ok(())
    }

    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.write_buffered_input(input)
    }

    fn write_input_with_options(
        &mut self,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        reject_unnegotiated_runtime_options(options.as_ref())?;
        self.write_buffered_input(input)
    }

    fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        reject_unnegotiated_runtime_options(options.as_ref())?;
        let runtime = self
            .shared
            .lock()
            .map_err(|_| {
                AgentConsoleError::new("acp_state_unavailable", "estado ACP no disponible")
            })?
            .runtime
            .clone();
        if !attachments.is_empty() && !runtime.image_attachments {
            return Err(AgentConsoleError::new(
                "acp_attachment_not_negotiated",
                "el proveedor ACP no negoció adjuntos de imagen",
            ));
        }
        if attachments.iter().any(|attachment| !attachment.is_image) {
            return Err(AgentConsoleError::new(
                "acp_attachment_unsupported",
                "ACP sólo admite imágenes negociadas en esta sesión",
            ));
        }
        self.send_prompt(text.to_owned(), attachments.to_vec())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.try_send_control(SupervisorControl::Resize {
            cols,
            rows,
            reply: reply_tx,
        })?;
        receive_control_reply(reply_rx)
    }

    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.output_reader
            .take()
            .map(|reader| Box::new(reader) as Box<dyn Read + Send>)
    }

    fn take_resume_result(
        &mut self,
    ) -> Option<Receiver<Result<AgentSessionResumeMode, AgentConsoleError>>> {
        self.resume_result.take()
    }

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        self.event_rx.try_iter().collect()
    }

    fn provider_session_id(&self) -> Option<String> {
        self.shared
            .lock()
            .ok()
            .and_then(|shared| shared.provider_session_id.clone())
    }

    fn acp_runtime(&self) -> Option<AgentSessionAcpRuntime> {
        self.shared.lock().ok().map(|shared| shared.runtime.clone())
    }

    fn acp_permissions(&self) -> Vec<AgentSessionAcpPermission> {
        self.shared
            .lock()
            .map(|shared| shared.permissions.clone())
            .unwrap_or_default()
    }

    fn respond_acp_permission(
        &mut self,
        permission_id: &str,
        option_id: Option<&str>,
        deny: bool,
    ) -> Result<(), AgentConsoleError> {
        self.respond_permission(permission_id, option_id, deny)
    }

    fn set_acp_config_option(
        &mut self,
        config_id: &str,
        value_id: &str,
    ) -> Result<(), AgentConsoleError> {
        self.set_config_option(config_id, value_id)
    }

    fn retry_acp(&mut self, confirmed: bool, turn_idle: bool) -> Result<(), AgentConsoleError> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.try_send_control(SupervisorControl::Retry {
            confirmed,
            turn_idle,
            reply: reply_tx,
        })?;
        receive_control_reply(reply_rx)
    }
}

impl Drop for AcpProcessSupervisor {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

pub struct PtyCompatibilityProcess {
    inner: Box<dyn AgentProcess>,
    runtime: AgentSessionAcpRuntime,
}

impl PtyCompatibilityProcess {
    pub fn new(inner: Box<dyn AgentProcess>, detail: impl Into<String>) -> Self {
        let mut runtime = pty_compatibility_runtime(detail);
        runtime.retry_available = false;
        Self { inner, runtime }
    }
}

impl AgentProcess for PtyCompatibilityProcess {
    fn pid(&self) -> Option<u32> {
        self.inner.pid()
    }

    fn try_exit_code(&mut self) -> Result<Option<i32>, AgentConsoleError> {
        self.inner.try_exit_code()
    }

    fn kill(&mut self) -> Result<(), AgentConsoleError> {
        self.inner.kill()
    }

    fn write_input(&mut self, input: &[u8]) -> Result<(), AgentConsoleError> {
        self.inner.write_input(input)
    }

    fn write_input_with_options(
        &mut self,
        input: &[u8],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.inner.write_input_with_options(input, options)
    }

    fn write_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
        options: Option<AgentSessionRuntimeOptions>,
    ) -> Result<(), AgentConsoleError> {
        self.inner.write_turn(text, attachments, options)
    }

    fn steer_turn(
        &mut self,
        text: &str,
        attachments: &[AgentTurnAttachment],
    ) -> Result<(), AgentConsoleError> {
        self.inner.steer_turn(text, attachments)
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), AgentConsoleError> {
        self.inner.resize(cols, rows)
    }

    fn take_output_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.inner.take_output_reader()
    }

    fn drain_events(&mut self) -> Vec<AgentProcessEvent> {
        self.inner.drain_events()
    }

    fn acp_runtime(&self) -> Option<AgentSessionAcpRuntime> {
        Some(self.runtime.clone())
    }

    fn acp_permissions(&self) -> Vec<AgentSessionAcpPermission> {
        Vec::new()
    }

    fn retry_acp(&mut self, _confirmed: bool, _turn_idle: bool) -> Result<(), AgentConsoleError> {
        Err(AgentConsoleError::new(
            "acp_retry_requires_supervisor",
            "esta sesión PTY debe reiniciarse mediante el supervisor ACP",
        ))
    }
}

struct AcpChannelReader {
    receiver: Receiver<Vec<u8>>,
    current: Vec<u8>,
    offset: usize,
}

impl AcpChannelReader {
    fn new(receiver: Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            current: Vec::new(),
            offset: 0,
        }
    }
}

impl Read for AcpChannelReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.offset >= self.current.len() {
            self.current = self
                .receiver
                .recv()
                .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "ACP output closed"))?;
            self.offset = 0;
        }
        let available = &self.current[self.offset..];
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        self.offset += count;
        Ok(count)
    }
}

fn receive_control_reply(
    receiver: Receiver<Result<(), AgentConsoleError>>,
) -> Result<(), AgentConsoleError> {
    receive_control_reply_with_timeout(receiver, Duration::from_secs(2))
}

fn receive_control_reply_with_timeout(
    receiver: Receiver<Result<(), AgentConsoleError>>,
    timeout: Duration,
) -> Result<(), AgentConsoleError> {
    receiver.recv_timeout(timeout).map_err(|_| {
        AgentConsoleError::new(
            "acp_supervisor_timeout",
            "el supervisor ACP no respondió a tiempo",
        )
    })?
}

fn reject_unnegotiated_runtime_options(
    options: Option<&AgentSessionRuntimeOptions>,
) -> Result<(), AgentConsoleError> {
    if options.is_some_and(|options| {
        options.model.is_some() || options.reasoning_effort.is_some() || options.speed.is_some()
    }) {
        return Err(AgentConsoleError::new(
            "acp_option_not_negotiated",
            "el proveedor ACP no negoció estas opciones de ejecución",
        ));
    }
    Ok(())
}

fn pty_compatibility_runtime(detail: impl Into<String>) -> AgentSessionAcpRuntime {
    AgentSessionAcpRuntime {
        state: AgentSessionAcpState::PtyCompatibility,
        mode: Some(AgentSessionAcpMode::Pty),
        detail: Some(detail.into()),
        lost_capabilities: vec![
            "actualizaciones estructuradas".to_owned(),
            "permisos ACP".to_owned(),
            "reanudación nativa".to_owned(),
        ],
        retry_available: true,
        image_attachments: false,
        config_options: Vec::new(),
    }
}

enum TransportEvent {
    Frame(Vec<u8>),
    Eof,
    Failed,
}

struct StdinWrite {
    frame: Vec<u8>,
    reply: Option<mpsc::Sender<Result<(), AgentConsoleError>>>,
}

struct AcpChildRuntime {
    child: Child,
    stdin_tx: Option<SyncSender<StdinWrite>>,
    stdin_thread: Option<JoinHandle<()>>,
    transport_rx: Receiver<TransportEvent>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    overflowed: Arc<AtomicBool>,
    core: AcpConnectionCore,
    permission_ids: HashMap<String, RequestId>,
    limits: AcpLimits,
    reaped: bool,
    #[cfg(windows)]
    containment: Option<KillOnCloseJob>,
}

impl AcpChildRuntime {
    fn spawn(
        provider: AcpProvider,
        binary_path: &Path,
        working_dir: &Path,
        tinto_session_id: &str,
        generation: u64,
        limits: AcpLimits,
    ) -> Result<Self, AgentConsoleError> {
        let mut child = spawn_acp_child(provider, binary_path, working_dir)?;
        #[cfg(windows)]
        let containment = match KillOnCloseJob::attach(&child) {
            Ok(containment) => Some(containment),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AgentConsoleError::new(
                    "acp_containment_failed",
                    "no se pudo contener el árbol de procesos ACP",
                ));
            }
        };
        let stdio = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (stdin, stdout, stderr) = match stdio {
            (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
            _ => {
                #[cfg(windows)]
                if let Some(containment) = containment {
                    let _ = containment.terminate();
                }
                #[cfg(not(windows))]
                let _ = kill_process_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(AgentConsoleError::new(
                    "acp_stdio_unavailable",
                    "el proveedor no expuso los canales necesarios para ACP",
                ));
            }
        };
        let (transport_tx, transport_rx) = mpsc::sync_channel(limits.event_queue);
        let (stdin_tx, stdin_rx) = mpsc::sync_channel(limits.pending_requests);
        let overflowed = Arc::new(AtomicBool::new(false));
        let stdin_thread = Some(spawn_acp_stdin_writer(stdin, stdin_rx));
        let stdout_thread = Some(spawn_acp_stdout_pump(
            stdout,
            transport_tx,
            Arc::clone(&overflowed),
            limits,
        ));
        let stderr_thread = Some(spawn_acp_stderr_pump(stderr, limits));
        let mut runtime = Self {
            child,
            stdin_tx: Some(stdin_tx),
            stdin_thread,
            transport_rx,
            stdout_thread,
            stderr_thread,
            overflowed,
            core: AcpConnectionCore::new(provider.id(), tinto_session_id, generation, limits),
            permission_ids: HashMap::new(),
            limits,
            reaped: false,
            #[cfg(windows)]
            containment,
        };
        let initialize = match runtime.core.initialize().map_err(core_to_console_error) {
            Ok(initialize) => initialize,
            Err(error) => {
                if let Err(cleanup) = runtime.stop_and_reap(false) {
                    return Err(AgentConsoleError::new(
                        "acp_cleanup_failed",
                        cleanup.message,
                    ));
                }
                return Err(error);
            }
        };
        if let Err(error) = runtime.write_frame(&initialize) {
            if let Err(cleanup) = runtime.stop_and_reap(false) {
                return Err(AgentConsoleError::new(
                    "acp_cleanup_failed",
                    cleanup.message,
                ));
            }
            return Err(error);
        }
        Ok(runtime)
    }

    fn write_frame(&mut self, frame: &[u8]) -> Result<(), AgentConsoleError> {
        if frame.len() > self.limits.frame_bytes {
            return Err(AgentConsoleError::new(
                "acp_frame_too_large",
                "el mensaje saliente supera el límite ACP",
            ));
        }
        let stdin_tx = self.stdin_tx.as_ref().ok_or_else(|| {
            AgentConsoleError::new("acp_stdin_closed", "la entrada ACP ya está cerrada")
        })?;
        let (reply_tx, reply_rx) = mpsc::channel();
        stdin_tx
            .try_send(StdinWrite {
                frame: frame.to_vec(),
                reply: Some(reply_tx),
            })
            .map_err(|_| {
                AgentConsoleError::new(
                    "acp_write_queue_full",
                    "la cola de escritura ACP está llena",
                )
            })?;
        reply_rx.recv_timeout(Duration::from_secs(1)).map_err(|_| {
            AgentConsoleError::new(
                "acp_write_timeout",
                "el proveedor ACP no recibió el mensaje a tiempo",
            )
        })?
    }

    fn try_queue_frame(&self, frame: Vec<u8>) -> Result<(), AgentConsoleError> {
        if frame.len() > self.limits.frame_bytes {
            return Err(AgentConsoleError::new(
                "acp_frame_too_large",
                "el mensaje saliente supera el límite ACP",
            ));
        }
        self.stdin_tx
            .as_ref()
            .ok_or_else(|| {
                AgentConsoleError::new("acp_stdin_closed", "la entrada ACP ya está cerrada")
            })?
            .try_send(StdinWrite { frame, reply: None })
            .map_err(|_| {
                AgentConsoleError::new(
                    "acp_write_queue_full",
                    "la cola de escritura ACP está llena",
                )
            })
    }

    fn stop_and_reap(&mut self, graceful: bool) -> Result<i32, AgentConsoleError> {
        if self.reaped {
            return Ok(self
                .child
                .try_wait()
                .ok()
                .flatten()
                .and_then(|status| status.code())
                .unwrap_or_default());
        }
        if graceful && self.core.has_active_turn() {
            if let Ok(frames) = self.core.cancel_turn() {
                for frame in frames {
                    let _ = self.try_queue_frame(frame);
                }
                let deadline = Instant::now() + self.limits.cancel_grace;
                while Instant::now() < deadline && self.core.has_active_turn() {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    match self
                        .transport_rx
                        .recv_timeout(remaining.min(Duration::from_millis(50)))
                    {
                        Ok(TransportEvent::Frame(frame)) => {
                            let _ = self.core.handle_frame(self.core.generation(), &frame);
                        }
                        Ok(TransportEvent::Eof | TransportEvent::Failed)
                        | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            }
        }

        self.stdin_tx.take();
        let parent_exited = self
            .child
            .try_wait()
            .map_err(|_| {
                AgentConsoleError::new(
                    "process_status_failed",
                    "no se pudo consultar el proceso ACP",
                )
            })?
            .is_some();
        #[cfg(windows)]
        let tree_result = self
            .containment
            .take()
            .map(KillOnCloseJob::terminate)
            .transpose()
            .map_err(|_| {
                AgentConsoleError::new(
                    "process_tree_kill_failed",
                    "no se pudo terminar el árbol de procesos ACP",
                )
            });
        #[cfg(not(windows))]
        let tree_result = match kill_process_tree(self.child.id()) {
            Ok(()) => Ok(()),
            Err(_) if parent_exited => Ok(()),
            Err(error) => Err(error),
        };
        if !parent_exited {
            let _ = self.child.kill();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        let exit_code = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or_default(),
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(None) | Err(_) => {
                    return Err(AgentConsoleError::new(
                        "process_reap_timeout",
                        "el proceso ACP no terminó dentro del plazo",
                    ));
                }
            }
        };
        self.reaped = true;
        join_thread_bounded(self.stdin_thread.take(), "escritura ACP")?;
        join_thread_bounded(self.stdout_thread.take(), "salida ACP")?;
        join_thread_bounded(self.stderr_thread.take(), "diagnóstico ACP")?;
        tree_result?;
        Ok(exit_code)
    }
}

impl Drop for AcpChildRuntime {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.stop_and_reap(false);
        }
    }
}

fn join_thread_bounded(
    thread: Option<JoinHandle<()>>,
    label: &'static str,
) -> Result<(), AgentConsoleError> {
    let Some(thread) = thread else {
        return Ok(());
    };
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = thread.join();
        let _ = done_tx.send(());
    });
    done_rx
        .recv_timeout(Duration::from_millis(500))
        .map_err(|_| {
            AgentConsoleError::new(
                "process_reap_timeout",
                format!("el hilo de {label} no terminó dentro del plazo"),
            )
        })
}

enum ConnectOutcome {
    Ready {
        runtime: Box<AcpChildRuntime>,
        resume_mode: Option<AgentSessionResumeMode>,
    },
    AuthenticationRequired,
    Fallback(&'static str),
    CleanupFailed,
    Stopped,
}

enum AcpRunOutcome {
    Stopped,
    Failed,
}

enum PtyRunOutcome {
    Retry,
    Stopped,
    Exited(i32),
    Failed,
}

#[allow(clippy::too_many_arguments)]
fn supervisor_worker(
    provider: AcpProvider,
    binary_path: PathBuf,
    working_dir: PathBuf,
    tinto_session_id: String,
    initial_intent: AcpLaunchIntent,
    limits: AcpLimits,
    control_rx: Receiver<SupervisorControl>,
    event_tx: SyncSender<AgentProcessEvent>,
    output_tx: SyncSender<Vec<u8>>,
    shared: Arc<Mutex<SupervisorShared>>,
    mut resume_tx: Option<mpsc::Sender<Result<AgentSessionResumeMode, AgentConsoleError>>>,
) {
    let mut intent = initial_intent;
    loop {
        set_connecting_runtime(&shared);
        let _ = send_timeline_output(
            &output_tx,
            AgentSessionTimelineKind::Lifecycle,
            "Conectando el agente mediante ACP.",
        );
        let generation = shared.lock().map(|shared| shared.generation).unwrap_or(1);
        let connect = connect_acp(
            provider,
            &binary_path,
            &working_dir,
            &tinto_session_id,
            generation,
            intent.clone(),
            limits,
            &control_rx,
            &event_tx,
            &output_tx,
            &shared,
        );
        match connect {
            ConnectOutcome::Ready {
                runtime,
                resume_mode,
            } => {
                if let Some(mode) = resume_mode {
                    resolve_resume(&mut resume_tx, Ok(mode));
                }
                if matches!(
                    run_ready_acp(*runtime, &control_rx, &event_tx, &output_tx, &shared,),
                    AcpRunOutcome::Stopped
                ) {
                    return;
                }
                let already_failed = shared
                    .lock()
                    .is_ok_and(|state| state.runtime.state == AgentSessionAcpState::Failed);
                if !already_failed {
                    mark_terminal_failure(
                        &shared,
                        &event_tx,
                        "acp_connection_failed",
                        "La conexión ACP terminó después de crear la sesión. No se reenvió ni reprodujo el turno mediante PTY.",
                    );
                    let _ = send_timeline_output(
                        &output_tx,
                        AgentSessionTimelineKind::Lifecycle,
                        "La conexión ACP terminó. No se reprodujo el turno mediante PTY.",
                    );
                }
                return;
            }
            ConnectOutcome::AuthenticationRequired => {
                resolve_resume(
                    &mut resume_tx,
                    Err(AgentConsoleError::new(
                        "acp_authentication_required",
                        "inicia sesión desde la CLI del proveedor y vuelve a reanudar; Tinto no recibe ni guarda credenciales",
                    )),
                );
                if wait_for_auth_retry(&control_rx, &shared) {
                    increment_generation(&shared);
                    continue;
                }
                resolve_resume(
                    &mut resume_tx,
                    Err(AgentConsoleError::new(
                        "agent_resume_stopped",
                        "la reanudación se detuvo antes de completar la conexión",
                    )),
                );
                mark_stopped(&shared, 0);
                return;
            }
            ConnectOutcome::Fallback(detail) => {
                match run_pty_fallback(
                    provider,
                    &binary_path,
                    &working_dir,
                    detail,
                    &control_rx,
                    &output_tx,
                    &shared,
                    &mut resume_tx,
                ) {
                    PtyRunOutcome::Retry => {
                        intent = AcpLaunchIntent::NewSession;
                        increment_generation(&shared);
                        continue;
                    }
                    PtyRunOutcome::Stopped => {
                        mark_stopped(&shared, 0);
                        return;
                    }
                    PtyRunOutcome::Exited(code) => {
                        mark_stopped(&shared, code);
                        return;
                    }
                    PtyRunOutcome::Failed => {
                        resolve_resume(
                            &mut resume_tx,
                            Err(AgentConsoleError::new(
                                "agent_resume_failed",
                                "no se pudo iniciar la reanudación con contexto archivado",
                            )),
                        );
                        mark_terminal_failure(
                            &shared,
                            &event_tx,
                            "pty_fallback_failed",
                            "el agente no pudo iniciarse mediante ACP ni PTY",
                        );
                        return;
                    }
                }
            }
            ConnectOutcome::Stopped => {
                resolve_resume(
                    &mut resume_tx,
                    Err(AgentConsoleError::new(
                        "agent_resume_stopped",
                        "la reanudación se detuvo antes de completar la conexión",
                    )),
                );
                mark_stopped(&shared, 0);
                return;
            }
            ConnectOutcome::CleanupFailed => {
                resolve_resume(
                    &mut resume_tx,
                    Err(AgentConsoleError::new(
                        "process_tree_kill_failed",
                        "no se pudo confirmar la terminación del proceso de reanudación",
                    )),
                );
                mark_terminal_failure(
                    &shared,
                    &event_tx,
                    "process_tree_kill_failed",
                    "No se pudo confirmar la terminación del árbol de procesos ACP. No se inició PTY.",
                );
                return;
            }
        }
    }
}

fn resolve_resume(
    resume_tx: &mut Option<mpsc::Sender<Result<AgentSessionResumeMode, AgentConsoleError>>>,
    result: Result<AgentSessionResumeMode, AgentConsoleError>,
) {
    if let Some(resume_tx) = resume_tx.take() {
        let _ = resume_tx.send(result);
    }
}

fn cleanup_connect_runtime(
    runtime: &mut AcpChildRuntime,
    shared: &Arc<Mutex<SupervisorShared>>,
) -> Result<(), AgentConsoleError> {
    let result = runtime.stop_and_reap(false).map(|_| ());
    clear_pid(shared);
    result
}

#[allow(clippy::too_many_arguments)]
fn connect_acp(
    provider: AcpProvider,
    binary_path: &Path,
    working_dir: &Path,
    tinto_session_id: &str,
    generation: u64,
    intent: AcpLaunchIntent,
    limits: AcpLimits,
    control_rx: &Receiver<SupervisorControl>,
    event_tx: &SyncSender<AgentProcessEvent>,
    output_tx: &SyncSender<Vec<u8>>,
    shared: &Arc<Mutex<SupervisorShared>>,
) -> ConnectOutcome {
    let resume_requested = matches!(&intent, AcpLaunchIntent::LoadSession { .. });
    let mut resume_context_bridge = false;
    let mut runtime = match AcpChildRuntime::spawn(
        provider,
        binary_path,
        working_dir,
        tinto_session_id,
        generation,
        limits,
    ) {
        Ok(runtime) => runtime,
        Err(error)
            if matches!(
                error.category.as_str(),
                "acp_containment_failed" | "acp_cleanup_failed"
            ) =>
        {
            return ConnectOutcome::CleanupFailed
        }
        Err(_) => {
            return ConnectOutcome::Fallback("El proveedor no pudo iniciar ACP v1; se usa PTY.")
        }
    };
    if let Ok(mut state) = shared.lock() {
        state.pid = Some(runtime.child.id());
    }
    let deadline = Instant::now() + limits.handshake_timeout;
    let mut load_attempted = false;
    let mut session_request_sent = false;
    while Instant::now() < deadline {
        if runtime.overflowed.load(Ordering::Relaxed) {
            if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                return ConnectOutcome::CleanupFailed;
            }
            return ConnectOutcome::Fallback("El proveedor excedió la cola ACP; se usa PTY.");
        }
        match control_rx.try_recv() {
            Ok(SupervisorControl::Stop { reply }) => {
                let result = cleanup_connect_runtime(&mut runtime, shared);
                let failed = result.is_err();
                let _ = reply.send(result);
                return if failed {
                    ConnectOutcome::CleanupFailed
                } else {
                    ConnectOutcome::Stopped
                };
            }
            Ok(control) => reject_connecting_control(control),
            Err(TryRecvError::Disconnected) => {
                return if cleanup_connect_runtime(&mut runtime, shared).is_ok() {
                    ConnectOutcome::Stopped
                } else {
                    ConnectOutcome::CleanupFailed
                };
            }
            Err(TryRecvError::Empty) => {}
        }
        match runtime.transport_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(TransportEvent::Frame(frame)) => {
                let events = match runtime.core.handle_frame(generation, &frame) {
                    Ok(events) => events,
                    Err(_) => {
                        if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                            return ConnectOutcome::CleanupFailed;
                        }
                        return ConnectOutcome::Fallback(
                            "El proveedor no completó un handshake ACP válido; se usa PTY.",
                        );
                    }
                };
                let outcome =
                    match dispatch_acp_events(&mut runtime, events, event_tx, output_tx, shared) {
                        Ok(outcome) => outcome,
                        Err(_) => {
                            if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                                return ConnectOutcome::CleanupFailed;
                            }
                            return ConnectOutcome::Fallback(
                                "El proveedor no completó un handshake ACP válido; se usa PTY.",
                            );
                        }
                    };
                if outcome.authentication_required {
                    if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                        return ConnectOutcome::CleanupFailed;
                    }
                    set_auth_required_runtime(shared);
                    let _ = send_timeline_output(
                        output_tx,
                        AgentSessionTimelineKind::Lifecycle,
                        "El proveedor requiere autenticación desde su CLI. Tinto no recibe ni guarda credenciales.",
                    );
                    return ConnectOutcome::AuthenticationRequired;
                }
                if outcome.failed {
                    if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                        return ConnectOutcome::CleanupFailed;
                    }
                    return ConnectOutcome::Fallback("El proveedor rechazó ACP v1; se usa PTY.");
                }
                if outcome.ready {
                    let resume_mode = resume_requested.then_some(if resume_context_bridge {
                        AgentSessionResumeMode::ContextBridge
                    } else {
                        AgentSessionResumeMode::Native
                    });
                    return ConnectOutcome::Ready {
                        runtime: Box::new(runtime),
                        resume_mode,
                    };
                }
                if outcome.load_unavailable {
                    resume_context_bridge = true;
                    if let AcpLaunchIntent::LoadSession {
                        fallback_context, ..
                    } = &intent
                    {
                        let _ = try_send_process_event(
                            event_tx,
                            AgentProcessEvent::ResumeContextRequired {
                                summary: fallback_context.clone(),
                            },
                        );
                    }
                    session_request_sent = false;
                }
                if runtime.core.phase() == AcpPhase::Initialized && !session_request_sent {
                    let request = match &intent {
                        AcpLaunchIntent::LoadSession {
                            provider_session_id,
                            ..
                        } if runtime.core.capabilities().load_session && !load_attempted => {
                            load_attempted = true;
                            runtime.core.load_session(provider_session_id, working_dir)
                        }
                        AcpLaunchIntent::LoadSession {
                            fallback_context, ..
                        } => {
                            if !resume_context_bridge {
                                let _ = try_send_process_event(
                                    event_tx,
                                    AgentProcessEvent::ResumeContextRequired {
                                        summary: fallback_context.clone(),
                                    },
                                );
                            }
                            resume_context_bridge = true;
                            runtime.core.new_session(working_dir)
                        }
                        AcpLaunchIntent::NewSession => runtime.core.new_session(working_dir),
                    };
                    match request {
                        Ok(frame) if runtime.write_frame(&frame).is_ok() => {
                            session_request_sent = true;
                        }
                        _ => {
                            if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                                return ConnectOutcome::CleanupFailed;
                            }
                            return ConnectOutcome::Fallback(
                                "El proveedor no pudo crear la sesión ACP; se usa PTY.",
                            );
                        }
                    }
                }
            }
            Ok(TransportEvent::Eof | TransportEvent::Failed)
            | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if cleanup_connect_runtime(&mut runtime, shared).is_err() {
                    return ConnectOutcome::CleanupFailed;
                }
                return ConnectOutcome::Fallback(
                    "El proveedor cerró ACP durante el arranque; se usa PTY.",
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    if cleanup_connect_runtime(&mut runtime, shared).is_err() {
        return ConnectOutcome::CleanupFailed;
    }
    ConnectOutcome::Fallback("El proveedor no respondió al handshake ACP; se usa PTY.")
}

fn acp_prompt_content(
    core: &AcpConnectionCore,
    text: String,
    attachments: &[AgentTurnAttachment],
) -> Result<Vec<ContentBlock>, AgentConsoleError> {
    if !attachments.is_empty() && !core.capabilities().image {
        return Err(AgentConsoleError::new(
            "acp_attachment_not_negotiated",
            "el proveedor ACP no negoció adjuntos de imagen",
        ));
    }
    let mut content = vec![ContentBlock::from(text)];
    for attachment in attachments {
        if !attachment.is_image {
            return Err(AgentConsoleError::new(
                "acp_attachment_unsupported",
                "ACP sólo admite imágenes negociadas en esta sesión",
            ));
        }
        let bytes = std::fs::read(&attachment.path).map_err(|_| {
            AgentConsoleError::new(
                "acp_attachment_read_failed",
                "no se pudo leer la imagen adjunta",
            )
        })?;
        let mime_type = match attachment
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => {
                return Err(AgentConsoleError::new(
                    "acp_attachment_unsupported",
                    "el formato de imagen no está admitido por ACP",
                ))
            }
        };
        content.push(ContentBlock::Image(ImageContent::new(
            STANDARD.encode(bytes),
            mime_type,
        )));
    }
    Ok(content)
}

fn run_ready_acp(
    mut runtime: AcpChildRuntime,
    control_rx: &Receiver<SupervisorControl>,
    event_tx: &SyncSender<AgentProcessEvent>,
    output_tx: &SyncSender<Vec<u8>>,
    shared: &Arc<Mutex<SupervisorShared>>,
) -> AcpRunOutcome {
    loop {
        if expire_permissions(&mut runtime, shared, output_tx).is_err() {
            invalidate_pending_permissions(shared, "La conexión ACP terminó antes de responder.");
            cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
            return AcpRunOutcome::Failed;
        }
        if runtime.overflowed.load(Ordering::Relaxed) {
            cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
            return AcpRunOutcome::Failed;
        }
        match control_rx.try_recv() {
            Ok(SupervisorControl::Prompt {
                turn_id,
                text,
                attachments,
                reply,
            }) => {
                let result = acp_prompt_content(&runtime.core, text, &attachments)
                    .and_then(|content| {
                        runtime
                            .core
                            .prompt_content(turn_id, content)
                            .map_err(core_to_console_error)
                    })
                    .and_then(|frame| runtime.write_frame(&frame));
                let fatal = result.as_ref().is_err_and(is_fatal_acp_write_error);
                let _ = reply.send(result.clone());
                if fatal {
                    invalidate_pending_permissions(
                        shared,
                        "La conexión ACP terminó antes de responder.",
                    );
                    cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                    return AcpRunOutcome::Failed;
                }
            }
            Ok(SupervisorControl::Resize { reply, .. }) => {
                let _ = reply.send(Ok(()));
            }
            Ok(SupervisorControl::Retry { reply, .. }) => {
                let _ = reply.send(Err(AgentConsoleError::new(
                    "acp_retry_unavailable",
                    "la sesión ya está conectada mediante ACP",
                )));
            }
            Ok(SupervisorControl::Permission {
                permission_id,
                option_id,
                deny,
                reply,
            }) => {
                let result = respond_ready_permission(
                    &mut runtime,
                    shared,
                    output_tx,
                    &permission_id,
                    option_id.as_deref(),
                    deny,
                    None,
                );
                let fatal = result.as_ref().is_err_and(is_fatal_acp_write_error);
                let _ = reply.send(result.clone());
                if fatal {
                    invalidate_pending_permissions(
                        shared,
                        "La conexión ACP terminó antes de responder.",
                    );
                    cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                    return AcpRunOutcome::Failed;
                }
            }
            Ok(SupervisorControl::Config {
                config_id,
                value_id,
                reply,
            }) => {
                let result = runtime
                    .core
                    .set_config_option(&config_id, &value_id)
                    .map_err(core_to_console_error)
                    .and_then(|frame| runtime.write_frame(&frame));
                let fatal = result.as_ref().is_err_and(is_fatal_acp_write_error);
                let _ = reply.send(result.clone());
                if fatal {
                    invalidate_pending_permissions(
                        shared,
                        "La conexión ACP terminó antes de responder.",
                    );
                    cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                    return AcpRunOutcome::Failed;
                }
            }
            Ok(SupervisorControl::Stop { reply }) => {
                invalidate_pending_permissions(shared, "La sesión fue detenida.");
                match runtime.stop_and_reap(true) {
                    Ok(code) => {
                        clear_pid(shared);
                        mark_stopped(shared, code);
                        let _ = reply.send(Ok(()));
                        return AcpRunOutcome::Stopped;
                    }
                    Err(error) => {
                        clear_pid(shared);
                        mark_terminal_failure(shared, event_tx, &error.category, &error.message);
                        let _ = reply.send(Err(error));
                        return AcpRunOutcome::Failed;
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                return if cleanup_ready_runtime(&mut runtime, shared, event_tx, true) {
                    AcpRunOutcome::Stopped
                } else {
                    AcpRunOutcome::Failed
                };
            }
            Err(TryRecvError::Empty) => {}
        }

        match runtime.transport_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(TransportEvent::Frame(frame)) => {
                let events = match runtime.core.handle_frame(runtime.core.generation(), &frame) {
                    Ok(events) => events,
                    Err(_) => {
                        invalidate_pending_permissions(
                            shared,
                            "La conexión ACP terminó antes de responder.",
                        );
                        cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                        return AcpRunOutcome::Failed;
                    }
                };
                match dispatch_acp_events(&mut runtime, events, event_tx, output_tx, shared) {
                    Ok(outcome) if !outcome.failed && !outcome.authentication_required => {}
                    _ => {
                        invalidate_pending_permissions(
                            shared,
                            "La conexión ACP terminó antes de responder.",
                        );
                        cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                        return AcpRunOutcome::Failed;
                    }
                }
            }
            Ok(TransportEvent::Eof | TransportEvent::Failed)
            | Err(mpsc::RecvTimeoutError::Disconnected) => {
                invalidate_pending_permissions(
                    shared,
                    "La conexión ACP terminó antes de responder.",
                );
                cleanup_ready_runtime(&mut runtime, shared, event_tx, false);
                return AcpRunOutcome::Failed;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn cleanup_ready_runtime(
    runtime: &mut AcpChildRuntime,
    shared: &Arc<Mutex<SupervisorShared>>,
    event_tx: &SyncSender<AgentProcessEvent>,
    graceful: bool,
) -> bool {
    let result = runtime.stop_and_reap(graceful).map(|_| ());
    clear_pid(shared);
    if let Err(error) = result {
        mark_terminal_failure(shared, event_tx, &error.category, &error.message);
        false
    } else {
        true
    }
}

fn wait_for_auth_retry(
    control_rx: &Receiver<SupervisorControl>,
    shared: &Arc<Mutex<SupervisorShared>>,
) -> bool {
    loop {
        match control_rx.recv() {
            Ok(SupervisorControl::Retry { reply, .. }) => {
                set_connecting_runtime(shared);
                let _ = reply.send(Ok(()));
                return true;
            }
            Ok(SupervisorControl::Stop { reply }) => {
                let _ = reply.send(Ok(()));
                return false;
            }
            Ok(control) => reject_auth_control(control),
            Err(_) => return false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_pty_fallback(
    provider: AcpProvider,
    binary_path: &Path,
    working_dir: &Path,
    detail: &'static str,
    control_rx: &Receiver<SupervisorControl>,
    output_tx: &SyncSender<Vec<u8>>,
    shared: &Arc<Mutex<SupervisorShared>>,
    resume_tx: &mut Option<mpsc::Sender<Result<AgentSessionResumeMode, AgentConsoleError>>>,
) -> PtyRunOutcome {
    let spawn_result = match provider {
        AcpProvider::Kimi => {
            PtyHandle::spawn_with_env_allowlist(binary_path, working_dir, KIMI_ALLOWED_ENV)
        }
        AcpProvider::OpenCode => PtyHandle::spawn(binary_path, working_dir),
    };
    let mut pty = match spawn_result {
        Ok(pty) => pty,
        Err(_) => return PtyRunOutcome::Failed,
    };
    let pid = pty.pid();
    let reader = match pty.take_output_reader() {
        Some(reader) => reader,
        None => {
            let _ = pty.kill();
            return PtyRunOutcome::Failed;
        }
    };
    let overflowed = Arc::new(AtomicBool::new(false));
    let output_thread = spawn_pty_output_pump(reader, output_tx.clone(), Arc::clone(&overflowed));
    if let Ok(mut state) = shared.lock() {
        state.pid = pid;
        state.provider_session_id = None;
        state.runtime = pty_compatibility_runtime(detail);
    }
    resolve_resume(resume_tx, Ok(AgentSessionResumeMode::ContextBridge));
    let _ = send_timeline_output(output_tx, AgentSessionTimelineKind::Lifecycle, detail);
    loop {
        if overflowed.load(Ordering::Relaxed) {
            let _ = stop_pty_fallback(pty, output_thread);
            clear_pid(shared);
            return PtyRunOutcome::Failed;
        }
        match control_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(SupervisorControl::Prompt {
                text,
                attachments,
                reply,
                ..
            }) => {
                let result = pty.write_turn(&text, &attachments, None);
                let _ = reply.send(result);
            }
            Ok(SupervisorControl::Resize { cols, rows, reply }) => {
                let _ = reply.send(pty.resize(cols, rows));
            }
            Ok(SupervisorControl::Retry {
                confirmed,
                turn_idle,
                reply,
            }) => {
                if !confirmed {
                    let _ = reply.send(Err(AgentConsoleError::new(
                        "acp_retry_confirmation_required",
                        "confirma el cambio de PTY a ACP",
                    )));
                    continue;
                }
                if !turn_idle {
                    let _ = reply.send(Err(AgentConsoleError::new(
                        "acp_retry_turn_active",
                        "espera a que termine el turno PTY antes de reintentar ACP",
                    )));
                    continue;
                }
                let result = stop_pty_fallback(pty, output_thread);
                clear_pid(shared);
                if let Err(error) = result {
                    let _ = reply.send(Err(error));
                    return PtyRunOutcome::Failed;
                }
                set_connecting_runtime(shared);
                let _ = reply.send(Ok(()));
                return PtyRunOutcome::Retry;
            }
            Ok(SupervisorControl::Permission { reply, .. }) => {
                let _ = reply.send(Err(AgentConsoleError::new(
                    "acp_permission_unavailable",
                    "el modo PTY no tiene permisos ACP pendientes",
                )));
            }
            Ok(SupervisorControl::Config { reply, .. }) => {
                let _ = reply.send(Err(AgentConsoleError::new(
                    "acp_option_unavailable",
                    "el modo PTY no tiene opciones ACP negociadas",
                )));
            }
            Ok(SupervisorControl::Stop { reply }) => {
                let result = stop_pty_fallback(pty, output_thread);
                clear_pid(shared);
                let stopped = result.is_ok();
                let _ = reply.send(result);
                return if stopped {
                    PtyRunOutcome::Stopped
                } else {
                    PtyRunOutcome::Failed
                };
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let stopped = stop_pty_fallback(pty, output_thread).is_ok();
                clear_pid(shared);
                return if stopped {
                    PtyRunOutcome::Stopped
                } else {
                    PtyRunOutcome::Failed
                };
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        match pty.try_exit_code() {
            Ok(Some(code)) => {
                drop(pty);
                if join_thread_bounded(Some(output_thread), "salida PTY").is_err() {
                    clear_pid(shared);
                    return PtyRunOutcome::Failed;
                }
                clear_pid(shared);
                return PtyRunOutcome::Exited(code);
            }
            Ok(None) => {}
            Err(_) => {
                let _ = stop_pty_fallback(pty, output_thread);
                clear_pid(shared);
                return PtyRunOutcome::Failed;
            }
        }
    }
}

fn stop_pty_fallback(
    mut pty: PtyHandle,
    output_thread: JoinHandle<()>,
) -> Result<(), AgentConsoleError> {
    let kill_result = pty.kill();
    drop(pty);
    let join_result = join_thread_bounded(Some(output_thread), "salida PTY");
    kill_result.and(join_result)
}

#[derive(Default)]
struct DispatchOutcome {
    ready: bool,
    authentication_required: bool,
    load_unavailable: bool,
    failed: bool,
}

fn dispatch_acp_events(
    runtime: &mut AcpChildRuntime,
    events: Vec<AcpEvent>,
    event_tx: &SyncSender<AgentProcessEvent>,
    output_tx: &SyncSender<Vec<u8>>,
    shared: &Arc<Mutex<SupervisorShared>>,
) -> Result<DispatchOutcome, AgentConsoleError> {
    let mut outcome = DispatchOutcome::default();
    for event in events {
        match event {
            AcpEvent::ProviderSessionReady {
                provider_session_id,
                capabilities,
            } => {
                if let Ok(mut state) = shared.lock() {
                    state.provider_session_id = Some(provider_session_id);
                    state.runtime = AgentSessionAcpRuntime {
                        state: AgentSessionAcpState::AcpReady,
                        mode: Some(AgentSessionAcpMode::Acp),
                        detail: None,
                        lost_capabilities: Vec::new(),
                        retry_available: false,
                        image_attachments: capabilities.image,
                        config_options: capabilities.config_options,
                    };
                }
                send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::Lifecycle,
                    "El agente está conectado mediante ACP.",
                )?;
                outcome.ready = true;
            }
            AcpEvent::LoadSessionUnavailable => {
                send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::Lifecycle,
                    "La sesión ACP previa no pudo cargarse; se creó una nueva con el contexto archivado.",
                )?;
                outcome.load_unavailable = true;
            }
            AcpEvent::Update(update) => match update.kind {
                AcpUpdateKind::AgentMessage => send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::AgentMessage,
                    &update.text,
                )?,
                AcpUpdateKind::UserMessage => {}
                AcpUpdateKind::ModeChanged | AcpUpdateKind::ConfigurationChanged => {
                    if let Ok(mut state) = shared.lock() {
                        state.runtime.image_attachments = runtime.core.capabilities().image;
                        state.runtime.config_options =
                            runtime.core.capabilities().config_options.clone();
                    }
                    send_timeline_output(
                        output_tx,
                        AgentSessionTimelineKind::Activity,
                        &update.text,
                    )?;
                }
                _ => send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::Activity,
                    &update.text,
                )?,
            },
            AcpEvent::PermissionRequested(permission) => {
                let permission_id = public_permission_id(&permission);
                runtime
                    .permission_ids
                    .insert(permission_id.clone(), permission.request_id.clone());
                let contract = AgentSessionAcpPermission {
                    id: permission_id,
                    generation: permission.generation,
                    provider_session_id: permission.provider_session_id,
                    turn_id: permission.turn_id,
                    tool_call_id: permission.tool_call_id,
                    title: permission.title.clone(),
                    options: permission
                        .options
                        .into_iter()
                        .map(|option| AgentSessionAcpPermissionOption {
                            id: option.id,
                            label: option.label,
                            kind: match option.kind {
                                AcpPermissionKind::AllowOnce => {
                                    AgentSessionAcpPermissionKind::AllowOnce
                                }
                                AcpPermissionKind::AllowAlways => {
                                    AgentSessionAcpPermissionKind::AllowAlways
                                }
                                AcpPermissionKind::RejectOnce => {
                                    AgentSessionAcpPermissionKind::RejectOnce
                                }
                                AcpPermissionKind::RejectAlways => {
                                    AgentSessionAcpPermissionKind::RejectAlways
                                }
                            },
                        })
                        .collect(),
                    state: AgentSessionAcpPermissionState::Pending,
                    reason: None,
                    expires_at_ms: now_ms()
                        .saturating_add(runtime.limits.permission_timeout.as_millis() as u64),
                };
                push_shared_permission(shared, contract);
                send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::Activity,
                    PERMISSION_ACTIVITY_TEXT,
                )?;
            }
            AcpEvent::TurnCompleted { .. } => {
                invalidate_pending_permissions(
                    shared,
                    "El turno terminó antes de recibir una decisión.",
                );
                try_send_process_event(
                    event_tx,
                    AgentProcessEvent::TurnCompleted {
                        timestamp_ms: now_ms(),
                    },
                )?;
                send_timeline_output(
                    output_tx,
                    AgentSessionTimelineKind::Lifecycle,
                    "Turno ACP completado.",
                )?;
            }
            AcpEvent::OutboundFrame(frame) => runtime.write_frame(&frame)?,
            AcpEvent::AuthenticationRequired => outcome.authentication_required = true,
            AcpEvent::Failed { category, message } => {
                if runtime.core.provider_session_valid {
                    mark_terminal_failure(shared, event_tx, category, message);
                    send_timeline_output(output_tx, AgentSessionTimelineKind::Lifecycle, message)?;
                }
                outcome.failed = true;
            }
            AcpEvent::PhaseChanged(_) => {}
        }
    }
    Ok(outcome)
}

fn reject_connecting_control(control: SupervisorControl) {
    let error = || {
        Err(AgentConsoleError::new(
            "acp_connecting",
            "el agente todavía está conectando mediante ACP",
        ))
    };
    match control {
        SupervisorControl::Prompt { reply, .. }
        | SupervisorControl::Resize { reply, .. }
        | SupervisorControl::Retry { reply, .. }
        | SupervisorControl::Permission { reply, .. }
        | SupervisorControl::Config { reply, .. } => {
            let _ = reply.send(error());
        }
        SupervisorControl::Stop { reply } => {
            let _ = reply.send(Ok(()));
        }
    }
}

fn reject_auth_control(control: SupervisorControl) {
    let error = || {
        Err(AgentConsoleError::new(
            "acp_authentication_required",
            "inicia sesión desde la CLI del proveedor y vuelve a intentarlo",
        ))
    };
    match control {
        SupervisorControl::Prompt { reply, .. }
        | SupervisorControl::Resize { reply, .. }
        | SupervisorControl::Permission { reply, .. }
        | SupervisorControl::Config { reply, .. } => {
            let _ = reply.send(error());
        }
        SupervisorControl::Retry { reply, .. } | SupervisorControl::Stop { reply } => {
            let _ = reply.send(Ok(()));
        }
    }
}

fn request_id_key(request_id: &RequestId) -> String {
    match request_id {
        RequestId::Number(value) => format!("n:{value}"),
        RequestId::Str(value) => format!("s:{value}"),
        RequestId::Null => "null".to_owned(),
    }
}

fn public_permission_id(permission: &AcpPermissionRequest) -> String {
    format!(
        "{}:{}:{}",
        permission.tinto_session_id,
        permission.generation,
        request_id_key(&permission.request_id)
    )
}

fn push_shared_permission(
    shared: &Arc<Mutex<SupervisorShared>>,
    permission: AgentSessionAcpPermission,
) {
    if let Ok(mut state) = shared.lock() {
        if state.permissions.len() >= 32 {
            if let Some(index) = state
                .permissions
                .iter()
                .position(|permission| permission.state != AgentSessionAcpPermissionState::Pending)
            {
                state.permissions.remove(index);
            }
        }
        state.permissions.push(permission);
    }
}

fn respond_ready_permission(
    runtime: &mut AcpChildRuntime,
    shared: &Arc<Mutex<SupervisorShared>>,
    output_tx: &SyncSender<Vec<u8>>,
    permission_id: &str,
    option_id: Option<&str>,
    deny: bool,
    terminal_override: Option<AgentSessionAcpPermissionState>,
) -> Result<(), AgentConsoleError> {
    let permission = shared
        .lock()
        .ok()
        .and_then(|state| {
            state
                .permissions
                .iter()
                .find(|permission| permission.id == permission_id)
                .cloned()
        })
        .ok_or_else(|| {
            AgentConsoleError::new("acp_permission_not_found", "el permiso ACP ya no existe")
        })?;
    if permission.state != AgentSessionAcpPermissionState::Pending
        || permission.generation != runtime.core.generation()
        || runtime.core.provider_session_id() != Some(permission.provider_session_id.as_str())
    {
        return Err(AgentConsoleError::new(
            "acp_permission_stale",
            "el permiso ACP ya no está pendiente",
        ));
    }
    let request_id = runtime
        .permission_ids
        .get(permission_id)
        .cloned()
        .ok_or_else(|| {
            AgentConsoleError::new(
                "acp_permission_stale",
                "el permiso ACP ya no está pendiente",
            )
        })?;
    let (decision, terminal_state, reason) =
        resolve_permission_decision(&permission, option_id, deny, terminal_override)?;
    let frame = runtime
        .core
        .respond_permission(&request_id, decision)
        .map_err(core_to_console_error)?;
    runtime.write_frame(&frame)?;
    runtime.permission_ids.remove(permission_id);
    if let Ok(mut state) = shared.lock() {
        if let Some(permission) = state
            .permissions
            .iter_mut()
            .find(|permission| permission.id == permission_id)
        {
            permission.state = terminal_state;
            permission.reason = Some(reason.to_owned());
        }
    }
    send_timeline_output(output_tx, AgentSessionTimelineKind::Activity, reason)
}

fn resolve_permission_decision(
    permission: &AgentSessionAcpPermission,
    option_id: Option<&str>,
    deny: bool,
    terminal_override: Option<AgentSessionAcpPermissionState>,
) -> Result<
    (
        AcpPermissionDecision,
        AgentSessionAcpPermissionState,
        &'static str,
    ),
    AgentConsoleError,
> {
    if deny {
        if option_id.is_some() {
            return Err(AgentConsoleError::new(
                "acp_permission_decision_invalid",
                "una denegación local no puede seleccionar una opción del proveedor",
            ));
        }
        return Ok((
            AcpPermissionDecision::Cancel,
            AgentSessionAcpPermissionState::Denied,
            "Permiso denegado.",
        ));
    }
    Ok(match option_id {
        Some(option_id) => {
            let option = permission
                .options
                .iter()
                .find(|option| option.id == option_id)
                .ok_or_else(|| {
                    AgentConsoleError::new(
                        "acp_permission_option_invalid",
                        "la opción de permiso no fue ofrecida por el proveedor",
                    )
                })?;
            let terminal_state = match option.kind {
                AgentSessionAcpPermissionKind::AllowOnce
                | AgentSessionAcpPermissionKind::AllowAlways => {
                    AgentSessionAcpPermissionState::Allowed
                }
                AgentSessionAcpPermissionKind::RejectOnce
                | AgentSessionAcpPermissionKind::RejectAlways => {
                    AgentSessionAcpPermissionState::Denied
                }
            };
            (
                AcpPermissionDecision::Select(option_id.to_owned()),
                terminal_state,
                if terminal_state == AgentSessionAcpPermissionState::Allowed {
                    "Permiso concedido."
                } else {
                    "Permiso denegado."
                },
            )
        }
        None => (
            AcpPermissionDecision::Cancel,
            terminal_override.unwrap_or(AgentSessionAcpPermissionState::Cancelled),
            if terminal_override == Some(AgentSessionAcpPermissionState::Expired) {
                "El permiso caducó sin aprobación."
            } else {
                "Permiso cancelado."
            },
        ),
    })
}

fn expire_permissions(
    runtime: &mut AcpChildRuntime,
    shared: &Arc<Mutex<SupervisorShared>>,
    output_tx: &SyncSender<Vec<u8>>,
) -> Result<(), AgentConsoleError> {
    let now = now_ms();
    let expired = shared
        .lock()
        .map(|state| expired_permission_ids_at(&state.permissions, now))
        .unwrap_or_default();
    for permission_id in expired {
        respond_ready_permission(
            runtime,
            shared,
            output_tx,
            &permission_id,
            None,
            false,
            Some(AgentSessionAcpPermissionState::Expired),
        )?;
    }
    Ok(())
}

fn expired_permission_ids_at(
    permissions: &[AgentSessionAcpPermission],
    now_ms: u64,
) -> Vec<String> {
    permissions
        .iter()
        .filter(|permission| {
            permission.state == AgentSessionAcpPermissionState::Pending
                && permission.expires_at_ms <= now_ms
        })
        .map(|permission| permission.id.clone())
        .collect()
}

fn invalidate_pending_permissions(shared: &Arc<Mutex<SupervisorShared>>, reason: &str) {
    if let Ok(mut state) = shared.lock() {
        for permission in &mut state.permissions {
            if permission.state == AgentSessionAcpPermissionState::Pending {
                permission.state = AgentSessionAcpPermissionState::Invalidated;
                permission.reason = Some(reason.to_owned());
            }
        }
    }
}

fn set_connecting_runtime(shared: &Arc<Mutex<SupervisorShared>>) {
    if let Ok(mut state) = shared.lock() {
        state.pid = None;
        state.exit_code = None;
        state.provider_session_id = None;
        state.runtime = AgentSessionAcpRuntime {
            state: AgentSessionAcpState::ConnectingAcp,
            mode: None,
            detail: None,
            lost_capabilities: Vec::new(),
            retry_available: false,
            image_attachments: false,
            config_options: Vec::new(),
        };
    }
}

fn set_auth_required_runtime(shared: &Arc<Mutex<SupervisorShared>>) {
    if let Ok(mut state) = shared.lock() {
        state.runtime = AgentSessionAcpRuntime {
            state: AgentSessionAcpState::AuthenticationRequired,
            mode: None,
            detail: Some("Inicia sesión desde la CLI del proveedor y reintenta ACP. Tinto no recibe ni guarda credenciales.".to_owned()),
            lost_capabilities: Vec::new(),
            retry_available: true,
            image_attachments: false,
            config_options: Vec::new(),
        };
    }
}

fn clear_pid(shared: &Arc<Mutex<SupervisorShared>>) {
    if let Ok(mut state) = shared.lock() {
        state.pid = None;
    }
}

fn increment_generation(shared: &Arc<Mutex<SupervisorShared>>) {
    if let Ok(mut state) = shared.lock() {
        state.generation = state.generation.saturating_add(1);
    }
}

fn mark_stopped(shared: &Arc<Mutex<SupervisorShared>>, exit_code: i32) {
    if let Ok(mut state) = shared.lock() {
        state.pid = None;
        state.exit_code = Some(exit_code);
    }
}

fn mark_terminal_failure(
    shared: &Arc<Mutex<SupervisorShared>>,
    event_tx: &SyncSender<AgentProcessEvent>,
    category: &str,
    message: &str,
) {
    invalidate_pending_permissions(shared, "La conexión ACP terminó antes de responder.");
    if let Ok(mut state) = shared.lock() {
        state.pid = None;
        state.exit_code = Some(1);
        state.runtime = AgentSessionAcpRuntime {
            state: AgentSessionAcpState::Failed,
            mode: Some(AgentSessionAcpMode::Acp),
            detail: Some(message.to_owned()),
            lost_capabilities: Vec::new(),
            retry_available: false,
            image_attachments: false,
            config_options: Vec::new(),
        };
    }
    let _ = event_tx.try_send(AgentProcessEvent::Error {
        error: AgentConsoleError::new(category, message),
    });
}

fn try_send_output(
    output_tx: &SyncSender<Vec<u8>>,
    output: Vec<u8>,
) -> Result<(), AgentConsoleError> {
    output_tx.try_send(output).map_err(|_| {
        AgentConsoleError::new("acp_output_queue_full", "la cola de salida ACP está llena")
    })
}

fn try_send_process_event(
    event_tx: &SyncSender<AgentProcessEvent>,
    event: AgentProcessEvent,
) -> Result<(), AgentConsoleError> {
    event_tx.try_send(event).map_err(|_| {
        AgentConsoleError::new("acp_event_queue_full", "la cola de eventos ACP está llena")
    })
}

fn send_timeline_output(
    output_tx: &SyncSender<Vec<u8>>,
    kind: AgentSessionTimelineKind,
    text: &str,
) -> Result<(), AgentConsoleError> {
    let mut frame = TIMELINE_FRAME_PREFIX.to_vec();
    frame.extend(
        serde_json::to_vec(&serde_json::json!({
            "kind": kind,
            "text_base64": STANDARD.encode(text.as_bytes())
        }))
        .map_err(|_| AgentConsoleError::new("acp_output_failed", "salida ACP no válida"))?,
    );
    frame.push(b'\n');
    try_send_output(output_tx, frame)
}

fn core_to_console_error(error: AcpCoreError) -> AgentConsoleError {
    AgentConsoleError::new(error.category, error.message)
}

fn is_fatal_acp_write_error(error: &AgentConsoleError) -> bool {
    error.category.starts_with("acp_write")
        || matches!(
            error.category.as_str(),
            "acp_stdin_closed" | "acp_frame_too_large"
        )
}

pub(crate) fn build_kimi_acp_command(binary_path: &Path, working_dir: &Path) -> Command {
    let mut command = build_acp_command_base(binary_path, working_dir);
    command.arg("acp");
    command
}

pub(crate) fn build_opencode_acp_command(binary_path: &Path, working_dir: &Path) -> Command {
    let mut command = build_acp_command_base(binary_path, working_dir);
    command
        .arg("acp")
        .arg("--cwd")
        .arg(working_dir)
        .args(["--hostname", "127.0.0.1", "--port", "0", "--no-mdns"])
        .env("OPENCODE_SERVER_PASSWORD", ephemeral_server_password());
    command
}

fn build_acp_command_base(binary_path: &Path, working_dir: &Path) -> Command {
    let mut command = Command::new(binary_path);
    command
        .current_dir(working_dir)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in KIMI_ALLOWED_ENV {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("TERM", "dumb");
    command
}

fn ephemeral_server_password() -> String {
    (0..3)
        .map(|_| uuid::Uuid::new_v4().simple().to_string())
        .collect()
}

fn spawn_acp_child(
    provider: AcpProvider,
    binary_path: &Path,
    working_dir: &Path,
) -> Result<Child, AgentConsoleError> {
    let mut command = match provider {
        AcpProvider::Kimi => build_kimi_acp_command(binary_path, working_dir),
        AcpProvider::OpenCode => build_opencode_acp_command(binary_path, working_dir),
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    let command = hide_console(&mut command);
    command
        .spawn()
        .map_err(|_| AgentConsoleError::new("acp_spawn_failed", "no se pudo iniciar ACP"))
}

fn spawn_acp_stdin_writer(
    mut stdin: impl Write + Send + 'static,
    receiver: Receiver<StdinWrite>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(write) = receiver.recv() {
            let result = stdin
                .write_all(&write.frame)
                .and_then(|_| stdin.write_all(b"\n"))
                .and_then(|_| stdin.flush())
                .map_err(|_| {
                    AgentConsoleError::new(
                        "acp_write_failed",
                        "no se pudo escribir al proveedor ACP",
                    )
                });
            if let Some(reply) = write.reply {
                let _ = reply.send(result.clone());
            }
            if result.is_err() {
                break;
            }
        }
    })
}

fn spawn_acp_stdout_pump(
    stdout: impl Read + Send + 'static,
    sender: SyncSender<TransportEvent>,
    overflowed: Arc<AtomicBool>,
    limits: AcpLimits,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_ndjson_frame(&mut reader, limits.frame_bytes) {
                Ok(Some(frame)) => match sender.try_send(TransportEvent::Frame(frame)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        overflowed.store(true, Ordering::Relaxed);
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                },
                Ok(None) => {
                    let _ = sender.try_send(TransportEvent::Eof);
                    break;
                }
                Err(_) => {
                    if sender.try_send(TransportEvent::Failed).is_err() {
                        overflowed.store(true, Ordering::Relaxed);
                    }
                    break;
                }
            }
        }
    })
}

fn spawn_acp_stderr_pump(
    mut stderr: impl Read + Send + 'static,
    limits: AcpLimits,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut tail = AcpStderrTail::new(limits);
        let mut buffer = [0u8; 8 * 1024];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => tail.push_line(&buffer[..read]),
            }
        }
    })
}

fn spawn_pty_output_pump(
    mut reader: Box<dyn Read + Send>,
    sender: SyncSender<Vec<u8>>,
    overflowed: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8 * 1024];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => {
                    for (output, turn_done) in take_pty_output_frames(&mut pending, true) {
                        if sender
                            .try_send(raw_pty_output_frame_with_signal(&output, turn_done))
                            .is_err()
                        {
                            overflowed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    break;
                }
                Ok(read) => {
                    pending.extend_from_slice(&buffer[..read]);
                    let frames = take_pty_output_frames(&mut pending, false);
                    let mut disconnected = false;
                    for (output, turn_done) in frames {
                        match sender.try_send(raw_pty_output_frame_with_signal(&output, turn_done))
                        {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => {
                                overflowed.store(true, Ordering::Relaxed);
                                disconnected = true;
                                break;
                            }
                            Err(TrySendError::Disconnected(_)) => {
                                disconnected = true;
                                break;
                            }
                        }
                    }
                    if disconnected {
                        break;
                    }
                }
            }
        }
    })
}

#[cfg(test)]
fn raw_pty_output_frame(output: &[u8]) -> Vec<u8> {
    raw_pty_output_frame_with_signal(output, false)
}

fn raw_pty_output_frame_with_signal(output: &[u8], turn_done: bool) -> Vec<u8> {
    let mut frame = TIMELINE_FRAME_PREFIX.to_vec();
    frame.extend(
        serde_json::to_vec(&serde_json::json!({
            "kind": AgentSessionTimelineKind::AgentMessage,
            "raw_output_base64": STANDARD.encode(output),
            "turn_done": turn_done
        }))
        .unwrap_or_else(|_| b"{\"kind\":\"agent_message\",\"raw_output_base64\":\"\"}".to_vec()),
    );
    frame.push(b'\n');
    frame
}

fn take_pty_output_frames(pending: &mut Vec<u8>, flush: bool) -> Vec<(Vec<u8>, bool)> {
    let marker = TINTO_TURN_DONE_MARKER.as_bytes();
    let mut frames = Vec::new();
    while let Some(offset) = pending
        .windows(marker.len())
        .position(|window| window == marker)
    {
        let output = pending.drain(..offset).collect();
        pending.drain(..marker.len());
        frames.push((output, true));
    }
    let retained = if flush {
        0
    } else {
        (1..marker.len())
            .rev()
            .find(|length| pending.ends_with(&marker[..*length]))
            .unwrap_or(0)
    };
    let emit = pending.len().saturating_sub(retained);
    if emit > 0 {
        frames.push((pending.drain(..emit).collect(), false));
    }
    frames
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Barrier;

    fn absolute_test_path(leaf: &str) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!("C:\\{leaf}"))
        }
        #[cfg(unix)]
        {
            PathBuf::from(format!("/{leaf}"))
        }
    }

    fn initialized_core() -> AcpConnectionCore {
        let mut core = AcpConnectionCore::new("kimi", "tinto-session", 7, AcpLimits::default());
        core.initialize().unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true,"audio":false,"embeddedContext":true}}}}"#,
        )
        .unwrap();
        core
    }

    fn ready_core() -> AcpConnectionCore {
        let mut core = initialized_core();
        core.new_session(&absolute_test_path("repo")).unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"provider-session"}}"#,
        )
        .unwrap();
        core
    }

    fn configured_core() -> AcpConnectionCore {
        let mut core = initialized_core();
        core.new_session(&absolute_test_path("repo")).unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"provider-session","modes":{"currentModeId":"code","availableModes":[{"id":"code","name":"Code"},{"id":"plan","name":"Plan"}]},"configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"kimi-k2","options":[{"value":"kimi-k2","name":"Kimi K2"},{"value":"kimi-k1","name":"Kimi K1"}]}]}}"#,
        )
        .unwrap();
        core
    }

    #[test]
    fn default_limits_match_the_delivery_contract() {
        let limits = AcpLimits::default();
        assert_eq!(limits.handshake_timeout, Duration::from_secs(30));
        assert_eq!(limits.permission_timeout, Duration::from_secs(60));
        assert_eq!(limits.cancel_grace, Duration::from_secs(2));
        assert_eq!(limits.frame_bytes, 1024 * 1024);
        assert_eq!(limits.stderr_line_bytes, 64 * 1024);
        assert_eq!(limits.stderr_tail_bytes, 256 * 1024);
        assert_eq!(limits.event_queue, 256);
        assert_eq!(limits.pending_requests, 64);
        assert_eq!(limits.pending_permissions, 16);
        assert_eq!(limits.updates_per_turn, 512);
        assert_eq!(limits.text_bytes_per_turn, 8 * 1024 * 1024);
    }

    #[test]
    fn pending_request_permission_and_queue_limits_fail_at_the_boundary() {
        let limits = AcpLimits {
            pending_requests: 2,
            ..AcpLimits::default()
        };
        let mut core = AcpConnectionCore::new("kimi", "tinto", 1, limits);
        core.initialize().unwrap();
        let second = core.next_id().unwrap();
        core.pending.insert(second, PendingRpcKind::Prompt);
        assert_eq!(core.pending.len(), 2);
        assert_eq!(core.next_id().unwrap_err().message, LIMIT_EXCEEDED);

        let mut permissions = ready_core();
        permissions.limits.pending_permissions = 2;
        permissions.prompt_text("turn", "go").unwrap();
        for id in ["p1", "p2"] {
            let frame = format!(
                r#"{{"jsonrpc":"2.0","id":"{id}","method":"session/request_permission","params":{{"sessionId":"provider-session","toolCall":{{"toolCallId":"tool-{id}","title":"Run"}},"options":[{{"optionId":"allow-{id}","name":"Allow","kind":"allow_once"}}]}}}}"#
            );
            permissions.handle_frame(7, frame.as_bytes()).unwrap();
        }
        assert_eq!(permissions.pending_permission_count(), 2);
        let above = permissions
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"p3","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-p3","title":"Run"},"options":[{"optionId":"allow-p3","name":"Allow","kind":"allow_once"}]}}"#,
            )
            .unwrap_err();
        assert_eq!(above.message, INVALID_MESSAGE);

        let (output_tx, _output_rx) = mpsc::sync_channel(1);
        try_send_output(&output_tx, b"one".to_vec()).unwrap();
        assert_eq!(
            try_send_output(&output_tx, b"two".to_vec())
                .unwrap_err()
                .category,
            "acp_output_queue_full"
        );

        let (event_tx, _event_rx) = mpsc::sync_channel(1);
        try_send_process_event(
            &event_tx,
            AgentProcessEvent::FileActivity { timestamp_ms: 1 },
        )
        .unwrap();
        assert_eq!(
            try_send_process_event(
                &event_tx,
                AgentProcessEvent::FileActivity { timestamp_ms: 2 },
            )
            .unwrap_err()
            .category,
            "acp_event_queue_full"
        );

        let (control_tx, _control_rx) = mpsc::sync_channel(1);
        let control = || {
            let (reply, _reply_rx) = mpsc::channel();
            SupervisorControl::Resize {
                cols: 80,
                rows: 24,
                reply,
            }
        };
        try_send_supervisor_control(&control_tx, control()).unwrap();
        assert_eq!(
            try_send_supervisor_control(&control_tx, control())
                .unwrap_err()
                .category,
            "acp_control_queue_full"
        );
    }

    #[test]
    fn resolved_permission_request_ids_remain_bounded_tombstones() {
        let mut core = ready_core();
        core.limits.pending_requests = 2;
        core.prompt_text("turn", "go").unwrap();
        for id in ["p1", "p2"] {
            let frame = format!(
                r#"{{"jsonrpc":"2.0","id":"{id}","method":"session/request_permission","params":{{"sessionId":"provider-session","toolCall":{{"toolCallId":"tool-{id}","title":"Run"}},"options":[{{"optionId":"allow-{id}","name":"Allow","kind":"allow_once"}}]}}}}"#
            );
            core.handle_frame(7, frame.as_bytes()).unwrap();
            core.respond_permission(
                &RequestId::Str(id.into()),
                AcpPermissionDecision::Select(format!("allow-{id}")),
            )
            .unwrap();
        }
        assert_eq!(core.seen_permission_requests.len(), 2);
        let error = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"p3","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-p3","title":"Run"},"options":[{"optionId":"allow-p3","name":"Allow","kind":"allow_once"}]}}"#,
            )
            .unwrap_err();
        assert_eq!(error.message, LIMIT_EXCEEDED);
    }

    #[test]
    fn initialize_is_typed_as_acp_v1_without_filesystem_or_terminal_capabilities() {
        let mut core = AcpConnectionCore::new("kimi", "tinto-session", 7, AcpLimits::default());
        let frame = core.initialize().unwrap();
        let value: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");
        assert_eq!(value["method"], "initialize");
        assert_eq!(value["params"]["protocolVersion"], 1);
        assert_eq!(
            value["params"]["clientCapabilities"]["fs"]["readTextFile"],
            false
        );
        assert_eq!(
            value["params"]["clientCapabilities"]["fs"]["writeTextFile"],
            false
        );
        assert_eq!(value["params"]["clientCapabilities"]["terminal"], false);
    }

    #[test]
    fn negotiated_images_models_and_modes_are_projected_and_enforced() {
        let mut core = configured_core();
        assert!(core.capabilities().image);
        assert!(core.capabilities().models);
        assert!(core.capabilities().modes);
        assert_eq!(core.capabilities().config_options.len(), 2);
        assert_eq!(
            core.capabilities().config_options[0].category,
            AgentSessionAcpConfigCategory::Mode
        );
        assert_eq!(
            core.capabilities().config_options[1].category,
            AgentSessionAcpConfigCategory::Model
        );

        let frame = core.set_config_option("model", "kimi-k1").unwrap();
        let frame: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(
            frame["method"],
            AGENT_METHOD_NAMES.session_set_config_option
        );
        assert_eq!(frame["params"]["configId"], "model");
        assert_eq!(frame["params"]["value"], "kimi-k1");
        assert!(core.set_config_option("model", "unknown").is_err());
    }

    #[test]
    fn absent_optional_capabilities_remain_hidden_and_rejected() {
        let mut core = ready_core();
        assert!(core.capabilities().config_options.is_empty());
        assert!(!core.capabilities().models);
        assert!(!core.capabilities().modes);
        assert!(core.set_config_option("model", "anything").is_err());
    }

    #[test]
    fn negotiated_image_attachment_is_encoded_as_typed_content() {
        let mut core = ready_core();
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("tiny.png");
        std::fs::write(&image, b"png fixture").unwrap();
        let content = acp_prompt_content(
            &core,
            "inspect".to_owned(),
            &[AgentTurnAttachment {
                path: image,
                is_image: true,
            }],
        )
        .unwrap();
        let frame = core.prompt_content("turn", content).unwrap();
        let frame: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(frame["params"]["prompt"][1]["type"], "image");
        assert_eq!(frame["params"]["prompt"][1]["mimeType"], "image/png");
        assert_eq!(
            frame["params"]["prompt"][1]["data"],
            STANDARD.encode(b"png fixture")
        );
    }

    #[test]
    fn baseline_flow_normalizes_updates_permissions_and_completion() {
        let mut core = ready_core();
        assert_eq!(core.provider_session_id(), Some("provider-session"));
        let prompt = core.prompt_text("turn-1", "Haz el cambio").unwrap();
        let prompt: Value = serde_json::from_slice(&prompt).unwrap();
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(prompt["params"]["sessionId"], "provider-session");

        let updates = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Listo"}}}}"#,
            )
            .unwrap();
        assert_eq!(
            updates,
            vec![AcpEvent::Update(AcpUpdate {
                turn_id: "turn-1".into(),
                kind: AcpUpdateKind::AgentMessage,
                text: "Listo".into(),
                tool_call_id: None,
            })]
        );

        let permission = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"permission-1","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-1","title":"Modificar archivo"},"options":[{"optionId":"allow","name":"Permitir","kind":"allow_once"},{"optionId":"deny","name":"Denegar","kind":"reject_once"}]}}"#,
            )
            .unwrap();
        let AcpEvent::PermissionRequested(permission) = &permission[0] else {
            panic!("expected permission");
        };
        assert_eq!(permission.turn_id, "turn-1");
        assert_eq!(permission.tool_call_id, "tool-1");
        let response = core
            .respond_permission(
                &RequestId::Str("permission-1".into()),
                AcpPermissionDecision::Select("allow".into()),
            )
            .unwrap();
        let response: Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["id"], "permission-1");
        assert_eq!(response["result"]["outcome"]["outcome"], "selected");
        assert_eq!(response["result"]["outcome"]["optionId"], "allow");

        let completed = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#,
            )
            .unwrap();
        assert_eq!(
            completed,
            vec![AcpEvent::TurnCompleted {
                turn_id: "turn-1".into(),
                stop_reason: "end_turn".into(),
            }]
        );
    }

    #[test]
    fn stale_duplicate_unknown_and_mismatched_messages_are_rejected() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        assert!(core
            .handle_frame(
                6,
                br#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#
            )
            .is_err());
        assert!(core
            .handle_frame(7, br#"{"jsonrpc":"2.0","id":99,"result":{}}"#)
            .is_err());
        assert!(core.handle_frame(7, br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"wrong","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"x"}}}}"#).is_err());
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"$/progress","params":{"token":"x"}}"#,
            )
            .is_err());
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"unknown-tool","status":"completed"}}}"#,
            )
            .is_err());
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#,
        )
        .unwrap();
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#
            )
            .is_err());
    }

    #[test]
    fn malformed_and_oversized_frames_are_rejected_without_echoing_provider_data() {
        let limits = AcpLimits {
            frame_bytes: 32,
            ..AcpLimits::default()
        };
        let mut core = AcpConnectionCore::new("kimi", "tinto", 1, limits);
        let malformed = core.handle_frame(1, b"secret-not-json").unwrap_err();
        assert_eq!(malformed.message, INVALID_MESSAGE);
        assert!(!malformed.to_string().contains("secret"));
        let oversized = core.handle_frame(1, &[b'x'; 33]).unwrap_err();
        assert_eq!(oversized.message, LIMIT_EXCEEDED);
    }

    #[test]
    fn auth_error_is_sanitized_and_never_falls_back_after_a_valid_session() {
        let mut pre_session = initialized_core();
        pre_session
            .new_session(&absolute_test_path("repo"))
            .unwrap();
        let auth = pre_session
            .handle_frame(7, br#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"secret provider detail","data":{"token":"secret"}}}"#)
            .unwrap();
        assert_eq!(pre_session.phase(), AcpPhase::AuthRequired);
        assert_eq!(auth.last(), Some(&AcpEvent::AuthenticationRequired));

        let mut post_session = ready_core();
        post_session.prompt_text("turn", "go").unwrap();
        let failed = post_session
            .handle_frame(7, br#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,"message":"secret provider detail"}}"#)
            .unwrap();
        assert_eq!(post_session.phase(), AcpPhase::Failed);
        assert!(!format!("{failed:?}").contains("secret"));
    }

    #[test]
    fn load_replay_is_validated_but_not_emitted_before_the_session_is_ready() {
        let mut core = initialized_core();
        core.load_session("provider-session", &absolute_test_path("repo"))
            .unwrap();
        let events = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"historic"}}}}"#,
            )
            .unwrap();
        assert!(events.is_empty());
        let ready = core
            .handle_frame(7, br#"{"jsonrpc":"2.0","id":2,"result":{}}"#)
            .unwrap();
        assert!(ready.iter().any(|event| matches!(
            event,
            AcpEvent::ProviderSessionReady {
                provider_session_id,
                ..
            } if provider_session_id == "provider-session"
        )));
    }

    #[test]
    fn non_auth_load_failure_can_create_a_fresh_session_on_the_same_connection() {
        let mut core = initialized_core();
        core.load_session("old-session", &absolute_test_path("repo"))
            .unwrap();
        let events = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":2,"error":{"code":-32002,"message":"not found"}}"#,
            )
            .unwrap();
        assert_eq!(core.phase(), AcpPhase::Initialized);
        assert!(events.contains(&AcpEvent::LoadSessionUnavailable));
        let request = core.new_session(&absolute_test_path("repo")).unwrap();
        let request: Value = serde_json::from_slice(&request).unwrap();
        assert_eq!(request["method"], "session/new");
    }

    #[test]
    fn unsupported_reverse_requests_receive_method_not_found_without_side_effects() {
        let mut core = ready_core();
        let events = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":40,"method":"fs/read_text_file","params":{"sessionId":"provider-session","path":"C:\\secret"}}"#,
            )
            .unwrap();
        let AcpEvent::OutboundFrame(frame) = &events[0] else {
            panic!("expected outbound response");
        };
        let response: Value = serde_json::from_slice(frame).unwrap();
        assert_eq!(response["id"], 40);
        assert_eq!(response["error"]["code"], -32601);

        let git = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":41,"method":"fs/read_text_file","params":{"sessionId":"provider-session","path":"C:\\repo\\.git\\config"}}"#,
            )
            .unwrap();
        let AcpEvent::OutboundFrame(frame) = &git[0] else {
            panic!("expected outbound response");
        };
        let response: Value = serde_json::from_slice(frame).unwrap();
        assert_eq!(response["id"], 41);
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn pending_permission_remains_authoritative_until_an_explicit_terminal_decision() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":"p","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool","title":"Run"},"options":[{"optionId":"deny","name":"Deny","kind":"reject_once"}]}}"#,
        )
        .unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"completed"}}}"#,
        )
        .unwrap();
        assert_eq!(core.pending_permission_count(), 1);
        let completed = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}"#,
            )
            .unwrap();
        assert!(matches!(
            completed.first(),
            Some(AcpEvent::OutboundFrame(_))
        ));
        assert!(matches!(
            completed.last(),
            Some(AcpEvent::TurnCompleted { .. })
        ));
        assert_eq!(core.pending_permission_count(), 0);
        assert!(core
            .respond_permission(
                &RequestId::Str("p".into()),
                AcpPermissionDecision::Select("deny".into()),
            )
            .is_err());
    }

    #[test]
    fn simultaneous_permissions_keep_exact_bindings_and_reject_invalid_responses() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        let first = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"p1","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-1","title":"First"},"options":[{"optionId":"allow-1","name":"Allow first","kind":"allow_once"}]}}"#,
            )
            .unwrap();
        let second = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"p2","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-2","title":"Second"},"options":[{"optionId":"deny-2","name":"Deny second","kind":"reject_once"}]}}"#,
            )
            .unwrap();
        let AcpEvent::PermissionRequested(first) = &first[0] else {
            panic!("expected first permission");
        };
        let AcpEvent::PermissionRequested(second) = &second[0] else {
            panic!("expected second permission");
        };
        assert_eq!(first.tinto_session_id, "tinto-session");
        assert_eq!(first.provider_session_id, "provider-session");
        assert_eq!(first.turn_id, "turn");
        assert_eq!(first.tool_call_id, "tool-1");
        assert_eq!(second.tool_call_id, "tool-2");
        assert_eq!(core.pending_permission_count(), 2);

        assert!(core
            .respond_permission(
                &RequestId::Str("p1".into()),
                AcpPermissionDecision::Select("deny-2".into()),
            )
            .is_err());
        assert_eq!(core.pending_permission_count(), 2);
        core.respond_permission(
            &RequestId::Str("p1".into()),
            AcpPermissionDecision::Select("allow-1".into()),
        )
        .unwrap();
        assert_eq!(core.pending_permission_count(), 1);
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"p1","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool-3","title":"Repeated"},"options":[{"optionId":"allow-3","name":"Allow repeated","kind":"allow_once"}]}}"#,
            )
            .is_err());
        assert!(core
            .respond_permission(
                &RequestId::Str("p1".into()),
                AcpPermissionDecision::Select("allow-1".into()),
            )
            .is_err());
        assert_eq!(core.pending_permission_count(), 1);
    }

    #[test]
    fn public_permission_ids_are_bound_to_the_tinto_session() {
        let mut first = AcpPermissionRequest {
            request_id: RequestId::Str("permission".into()),
            generation: 7,
            tinto_session_id: "tinto-a".to_owned(),
            provider_session_id: "provider-session".to_owned(),
            turn_id: "turn".to_owned(),
            tool_call_id: "tool".to_owned(),
            title: "Run".to_owned(),
            options: Vec::new(),
        };
        let first_id = public_permission_id(&first);
        first.tinto_session_id = "tinto-b".to_owned();

        assert_ne!(first_id, public_permission_id(&first));
    }

    #[test]
    fn permission_response_has_exactly_one_winner_under_concurrency() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":"race","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool","title":"Run"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]}}"#,
        )
        .unwrap();
        let core = Arc::new(Mutex::new(core));
        let barrier = Arc::new(Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let core = Arc::clone(&core);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    core.lock().unwrap().respond_permission(
                        &RequestId::Str("race".into()),
                        AcpPermissionDecision::Select("allow".into()),
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert_eq!(core.lock().unwrap().pending_permission_count(), 0);
    }

    #[test]
    fn local_deny_is_distinct_from_cancel_when_the_provider_offers_only_allow() {
        let permission = AgentSessionAcpPermission {
            id: "permission".to_owned(),
            generation: 7,
            provider_session_id: "provider-session".to_owned(),
            turn_id: "turn".to_owned(),
            tool_call_id: "tool".to_owned(),
            title: "Run".to_owned(),
            options: vec![AgentSessionAcpPermissionOption {
                id: "allow".to_owned(),
                label: "Allow".to_owned(),
                kind: AgentSessionAcpPermissionKind::AllowOnce,
            }],
            state: AgentSessionAcpPermissionState::Pending,
            reason: None,
            expires_at_ms: u64::MAX,
        };

        let denied = resolve_permission_decision(&permission, None, true, None).unwrap();
        assert_eq!(denied.0, AcpPermissionDecision::Cancel);
        assert_eq!(denied.1, AgentSessionAcpPermissionState::Denied);
        assert_eq!(denied.2, "Permiso denegado.");

        let cancelled = resolve_permission_decision(&permission, None, false, None).unwrap();
        assert_eq!(cancelled.0, AcpPermissionDecision::Cancel);
        assert_eq!(cancelled.1, AgentSessionAcpPermissionState::Cancelled);
        assert_eq!(cancelled.2, "Permiso cancelado.");
        assert!(resolve_permission_decision(&permission, Some("allow"), true, None).is_err());
    }

    #[test]
    fn permission_requests_reject_wrong_sessions_and_duplicate_option_ids() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"wrong-session","method":"session/request_permission","params":{"sessionId":"other-session","toolCall":{"toolCallId":"tool","title":"Run"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]}}"#,
            )
            .is_err());
        assert!(core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","id":"duplicate-options","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool","title":"Run"},"options":[{"optionId":"same","name":"Allow","kind":"allow_once"},{"optionId":"same","name":"Deny","kind":"reject_once"}]}}"#,
            )
            .is_err());
        assert_eq!(core.pending_permission_count(), 0);
    }

    #[test]
    fn permission_expiry_uses_the_injected_deadline_and_ignores_terminal_cards() {
        let permission = |id: &str, state, expires_at_ms| AgentSessionAcpPermission {
            id: id.to_owned(),
            generation: 7,
            provider_session_id: "provider-session".to_owned(),
            turn_id: "turn".to_owned(),
            tool_call_id: "tool".to_owned(),
            title: "Run".to_owned(),
            options: Vec::new(),
            state,
            reason: None,
            expires_at_ms,
        };
        let permissions = vec![
            permission("future", AgentSessionAcpPermissionState::Pending, 101),
            permission("due", AgentSessionAcpPermissionState::Pending, 100),
            permission("terminal", AgentSessionAcpPermissionState::Denied, 1),
        ];
        assert!(expired_permission_ids_at(&permissions, 99).is_empty());
        assert_eq!(expired_permission_ids_at(&permissions, 100), vec!["due"]);
        assert_eq!(
            expired_permission_ids_at(&permissions, 101),
            vec!["future", "due"]
        );
    }

    #[test]
    fn cancel_emits_session_cancel_and_cancels_every_permission() {
        let mut core = ready_core();
        core.prompt_text("turn", "go").unwrap();
        core.handle_frame(
            7,
            br#"{"jsonrpc":"2.0","id":"p","method":"session/request_permission","params":{"sessionId":"provider-session","toolCall":{"toolCallId":"tool","title":"Run"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"}]}}"#,
        )
        .unwrap();
        let frames = core.cancel_turn().unwrap();
        assert_eq!(frames.len(), 2);
        let cancel: Value = serde_json::from_slice(&frames[0]).unwrap();
        assert_eq!(cancel["method"], "session/cancel");
        let permission: Value = serde_json::from_slice(&frames[1]).unwrap();
        assert_eq!(permission["result"]["outcome"]["outcome"], "cancelled");
        assert_eq!(core.pending_permission_count(), 0);
    }

    #[test]
    fn update_and_text_quotas_are_enforced() {
        let limits = AcpLimits {
            updates_per_turn: 1,
            text_bytes_per_turn: 4,
            ..AcpLimits::default()
        };
        let mut core = AcpConnectionCore::new("kimi", "tinto", 1, limits);
        core.initialize().unwrap();
        core.handle_frame(
            1,
            br#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#,
        )
        .unwrap();
        core.new_session(&absolute_test_path("repo")).unwrap();
        core.handle_frame(1, br#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"s"}}"#)
            .unwrap();
        core.prompt_text("t", "go").unwrap();
        core.handle_frame(1, br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"1234"}}}}"#).unwrap();
        let error = core.handle_frame(1, br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"x"}}}}"#).unwrap_err();
        assert_eq!(error.message, LIMIT_EXCEEDED);
    }

    #[test]
    fn text_quota_rejects_slow_accumulation_above_the_limit() {
        let mut core = ready_core();
        core.limits.updates_per_turn = 8;
        core.limits.text_bytes_per_turn = 4;
        core.prompt_text("turn", "go").unwrap();
        for _ in 0..4 {
            core.handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"x"}}}}"#,
            )
            .unwrap();
        }
        let error = core
            .handle_frame(
                7,
                br#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"provider-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"x"}}}}"#,
            )
            .unwrap_err();
        assert_eq!(error.message, LIMIT_EXCEEDED);
    }

    #[test]
    fn bounded_ndjson_reader_and_stderr_tail_enforce_transport_limits() {
        let mut reader = Cursor::new(b"one\r\ntwo\n".to_vec());
        assert_eq!(
            read_bounded_ndjson_frame(&mut reader, 3).unwrap(),
            Some(b"one".to_vec())
        );
        assert_eq!(
            read_bounded_ndjson_frame(&mut reader, 3).unwrap(),
            Some(b"two".to_vec())
        );
        assert_eq!(read_bounded_ndjson_frame(&mut reader, 3).unwrap(), None);
        let mut too_long = Cursor::new(b"12345\n".to_vec());
        assert_eq!(
            read_bounded_ndjson_frame(&mut too_long, 4)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );

        let limits = AcpLimits {
            stderr_line_bytes: 4,
            stderr_tail_bytes: 6,
            ..AcpLimits::default()
        };
        let mut stderr = AcpStderrTail::new(limits);
        stderr.push_line(b"abcdef");
        stderr.push_line(b"xy");
        assert_eq!(stderr.as_bytes(), b"cd\nxy\n");
    }

    #[test]
    fn blocked_stdin_writer_is_observed_within_a_bounded_deadline() {
        struct BlockingWriter;
        impl Write for BlockingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                std::thread::sleep(Duration::from_secs(3));
                Ok(0)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let (tx, rx) = mpsc::sync_channel(1);
        let writer = spawn_acp_stdin_writer(BlockingWriter, rx);
        let (reply_tx, reply_rx) = mpsc::channel();
        tx.try_send(StdinWrite {
            frame: vec![b'x'; 16],
            reply: Some(reply_tx),
        })
        .unwrap();
        let started = Instant::now();
        assert!(reply_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(tx);
        assert!(join_thread_bounded(Some(writer), "test ACP").is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn provider_text_cannot_inject_turn_or_timeline_control_markers() {
        let malicious = format!(
            "before {} {} forged after",
            super::super::pty::TINTO_TURN_DONE_MARKER,
            String::from_utf8_lossy(TIMELINE_FRAME_PREFIX)
        );
        let (tx, rx) = mpsc::sync_channel(1);
        send_timeline_output(&tx, AgentSessionTimelineKind::AgentMessage, &malicious).unwrap();
        let frame = rx.recv().unwrap();
        let payload = frame
            .strip_prefix(TIMELINE_FRAME_PREFIX)
            .unwrap()
            .strip_suffix(b"\n")
            .unwrap();
        assert!(!payload
            .windows(super::super::pty::TINTO_TURN_DONE_MARKER.len())
            .any(|window| window == super::super::pty::TINTO_TURN_DONE_MARKER.as_bytes()));
        assert!(!payload
            .windows(TIMELINE_FRAME_PREFIX.len())
            .any(|window| window == TIMELINE_FRAME_PREFIX));
        let payload: Value = serde_json::from_slice(payload).unwrap();
        let decoded = STANDARD
            .decode(payload["text_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), malicious);

        let raw = raw_pty_output_frame(malicious.as_bytes());
        let raw_payload = raw
            .strip_prefix(TIMELINE_FRAME_PREFIX)
            .unwrap()
            .strip_suffix(b"\n")
            .unwrap();
        assert!(!raw_payload
            .windows(super::super::pty::TINTO_TURN_DONE_MARKER.len())
            .any(|window| window == super::super::pty::TINTO_TURN_DONE_MARKER.as_bytes()));
        assert!(!raw_payload
            .windows(TIMELINE_FRAME_PREFIX.len())
            .any(|window| window == TIMELINE_FRAME_PREFIX));
    }

    #[test]
    fn pty_turn_done_marker_survives_framing_even_when_split_across_reads() {
        let marker = TINTO_TURN_DONE_MARKER.as_bytes();
        let split = marker.len() / 2;
        let mut pending = b"before ".to_vec();
        pending.extend_from_slice(&marker[..split]);

        let first = take_pty_output_frames(&mut pending, false);
        assert_eq!(first, vec![(b"before ".to_vec(), false)]);

        pending.extend_from_slice(&marker[split..]);
        pending.extend_from_slice(b" after");
        let second = take_pty_output_frames(&mut pending, false);
        assert_eq!(
            second,
            vec![(Vec::new(), true), (b" after".to_vec(), false)]
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn persisted_permission_activity_is_provider_independent() {
        let provider_title = "curl -H Authorization:Bearer-TINTO_SECRET_CANARY";
        assert!(!PERMISSION_ACTIVITY_TEXT.contains(provider_title));
        assert!(!PERMISSION_ACTIVITY_TEXT.contains("TINTO_SECRET_CANARY"));
    }

    #[test]
    fn kimi_acp_command_uses_stdio_and_an_explicit_non_secret_environment() {
        let repo = absolute_test_path("repo");
        let command = build_kimi_acp_command(Path::new("kimi"), &repo);
        assert_eq!(command.get_args().collect::<Vec<_>>(), vec!["acp"]);
        assert_eq!(command.get_current_dir(), Some(repo.as_path()));
        let names = command
            .get_envs()
            .map(|(name, _)| name.to_string_lossy().to_ascii_uppercase())
            .collect::<Vec<_>>();
        assert!(names.contains(&"TERM".to_owned()));
        assert!(!names.iter().any(|name| {
            name.contains("API_KEY") || name.contains("TOKEN") || name.contains("PASSWORD")
        }));
    }

    #[test]
    fn opencode_descriptor_forces_the_approved_loopback_controls_and_ephemeral_secret() {
        let repo = absolute_test_path("repo");
        let command = build_opencode_acp_command(Path::new("opencode"), &repo);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec![
                "acp",
                "--cwd",
                repo.to_string_lossy().as_ref(),
                "--hostname",
                "127.0.0.1",
                "--port",
                "0",
                "--no-mdns",
            ]
        );
        let password = command
            .get_envs()
            .find(|(name, _)| *name == "OPENCODE_SERVER_PASSWORD")
            .and_then(|(_, value)| value)
            .unwrap()
            .to_string_lossy();
        assert_eq!(password.len(), 96);
        assert!(!args.iter().any(|arg| arg.contains(password.as_ref())));
    }

    #[test]
    fn opencode_attempts_acp_when_launched_with_the_requested_ephemeral_port() {
        let directory = tempfile::tempdir().unwrap();
        let binary = malformed_acp_test_cli(directory.path());
        let mut supervisor = AcpProcessSupervisor::spawn_opencode(
            binary,
            directory.path().to_path_buf(),
            "tinto-opencode".to_owned(),
            AcpLaunchIntent::NewSession,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let runtime = loop {
            let runtime = supervisor.acp_runtime().unwrap();
            if runtime.state == AgentSessionAcpState::PtyCompatibility {
                break runtime;
            }
            assert!(Instant::now() < deadline, "OpenCode did not enter PTY mode");
            std::thread::sleep(Duration::from_millis(10));
        };

        assert!(runtime.retry_available);
        let detail = runtime.detail.unwrap();
        assert!(!detail.is_empty());
        assert!(!detail.contains("127.0.0.1:4096"));
        assert!(runtime
            .lost_capabilities
            .iter()
            .any(|capability| capability.contains("permisos")));
        supervisor.kill().unwrap();
    }

    #[test]
    fn pre_session_failure_retries_only_after_confirmation_and_an_idle_pty_turn() {
        let directory = tempfile::tempdir().unwrap();
        let binary = malformed_acp_test_cli(directory.path());
        let mut supervisor = AcpProcessSupervisor::spawn(
            binary,
            directory.path().to_path_buf(),
            "tinto-retry".to_owned(),
            AcpLaunchIntent::NewSession,
        )
        .unwrap();
        let mut output_reader = supervisor.take_output_reader().unwrap();
        assert!(supervisor.take_output_reader().is_none());
        let first_deadline = Instant::now() + Duration::from_secs(3);
        while supervisor.acp_runtime().unwrap().state != AgentSessionAcpState::PtyCompatibility {
            assert!(
                Instant::now() < first_deadline,
                "pre-session failure did not enter PTY"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let initial_generation = supervisor.shared.lock().unwrap().generation;
        let runtime = supervisor.acp_runtime().unwrap();
        assert!(runtime.retry_available);
        assert!(!runtime.lost_capabilities.is_empty());

        let unconfirmed = supervisor.retry_acp(false, true).unwrap_err();
        assert_eq!(unconfirmed.category, "acp_retry_confirmation_required");
        let active = supervisor.retry_acp(true, false).unwrap_err();
        assert_eq!(active.category, "acp_retry_turn_active");

        let (first_reply_tx, first_reply_rx) = mpsc::channel();
        let (second_reply_tx, second_reply_rx) = mpsc::channel();
        let state_guard = supervisor.shared.lock().unwrap();
        supervisor
            .try_send_control(SupervisorControl::Retry {
                confirmed: true,
                turn_idle: true,
                reply: first_reply_tx,
            })
            .unwrap();
        supervisor
            .try_send_control(SupervisorControl::Retry {
                confirmed: true,
                turn_idle: true,
                reply: second_reply_tx,
            })
            .unwrap();
        drop(state_guard);
        let retry_results = [
            receive_control_reply(first_reply_rx),
            receive_control_reply(second_reply_rx),
        ];
        assert_eq!(
            retry_results.iter().filter(|result| result.is_ok()).count(),
            1
        );
        assert_eq!(
            retry_results
                .iter()
                .filter(|result| result.is_err())
                .count(),
            1
        );

        let retried_deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let state = supervisor.shared.lock().unwrap();
            if state.generation == initial_generation + 1
                && state.runtime.state == AgentSessionAcpState::PtyCompatibility
            {
                break;
            }
            assert!(
                Instant::now() < retried_deadline,
                "confirmed idle retry did not create one fresh ACP generation"
            );
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
        supervisor.kill().unwrap();
        let mut output = Vec::new();
        assert_eq!(
            output_reader.read_to_end(&mut output).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        let connecting = STANDARD.encode("Conectando el agente mediante ACP.");
        assert!(
            output
                .windows(connecting.len())
                .filter(|window| *window == connecting.as_bytes())
                .count()
                >= 2
        );
    }

    #[test]
    fn blocked_handshake_returns_immediately_and_can_be_stopped() {
        let directory = tempfile::tempdir().unwrap();
        let binary = blocking_test_cli(directory.path());
        let started = Instant::now();
        let mut supervisor = AcpProcessSupervisor::spawn(
            binary,
            directory.path().to_path_buf(),
            "tinto-test".to_owned(),
            AcpLaunchIntent::NewSession,
        )
        .unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            supervisor.acp_runtime().unwrap().state,
            AgentSessionAcpState::ConnectingAcp
        );
        supervisor.kill().unwrap();
        assert!(supervisor.try_exit_code().unwrap().is_some());
    }

    #[test]
    fn auth_required_resume_returns_an_actionable_error_without_declaring_a_mode() {
        let directory = tempfile::tempdir().unwrap();
        let binary = auth_required_test_cli(directory.path());
        let mut supervisor = AcpProcessSupervisor::spawn(
            binary,
            directory.path().to_path_buf(),
            "tinto-resume".to_owned(),
            AcpLaunchIntent::LoadSession {
                provider_session_id: "provider-old".to_owned(),
                fallback_context: AgentSessionContextSummary {
                    text: "archived context".to_owned(),
                    created_at_ms: 1,
                    source_events: 1,
                    source_turns: 1,
                },
            },
        )
        .unwrap();
        let error = supervisor
            .take_resume_result()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap_err();

        assert_eq!(error.category, "acp_authentication_required");
        assert!(error.message.contains("CLI del proveedor"));
        assert!(error.message.contains("no recibe ni guarda credenciales"));
        assert_eq!(
            supervisor.acp_runtime().unwrap().state,
            AgentSessionAcpState::AuthenticationRequired
        );
        supervisor.kill().unwrap();
    }

    #[cfg(windows)]
    fn malformed_acp_test_cli(directory: &Path) -> PathBuf {
        let path = directory.join("fake-kimi-malformed.cmd");
        std::fs::write(
            &path,
            "@echo off\r\nif \"%1\"==\"acp\" (\r\n  echo not-json\r\n  exit /b 0\r\n)\r\nping -n 30 127.0.0.1 >nul\r\n",
        )
        .unwrap();
        path
    }

    #[cfg(unix)]
    fn malformed_acp_test_cli(directory: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("fake-kimi-malformed");
        std::fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"acp\" ]; then\n  printf '%s\\n' 'not-json'\n  exit 0\nfi\nsleep 30\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[test]
    fn unavailable_load_resolves_context_bridge_after_a_fresh_acp_session_is_ready() {
        let directory = tempfile::tempdir().unwrap();
        let fallback_marker = directory.path().join("pty-fallback-started");
        let binary = ready_then_disconnect_test_cli(directory.path(), &fallback_marker);
        let summary = AgentSessionContextSummary {
            text: "archived context".to_owned(),
            created_at_ms: 1,
            source_events: 2,
            source_turns: 1,
        };
        let mut supervisor = AcpProcessSupervisor::spawn(
            binary,
            directory.path().to_path_buf(),
            "tinto-resume".to_owned(),
            AcpLaunchIntent::LoadSession {
                provider_session_id: "provider-old".to_owned(),
                fallback_context: summary.clone(),
            },
        )
        .unwrap();

        let mode = supervisor
            .take_resume_result()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();

        assert_eq!(mode, AgentSessionResumeMode::ContextBridge);
        assert_eq!(
            supervisor.acp_runtime().unwrap().state,
            AgentSessionAcpState::AcpReady
        );
        assert_eq!(
            supervisor.provider_session_id().as_deref(),
            Some("fixture-session")
        );
        assert!(supervisor.drain_events().iter().any(|event| matches!(
            event,
            AgentProcessEvent::ResumeContextRequired { summary: event_summary }
                if event_summary == &summary
        )));
        assert!(!fallback_marker.exists());
        supervisor.kill().unwrap();
    }

    #[test]
    fn post_session_disconnect_fails_without_pty_fallback_or_turn_replay() {
        let directory = tempfile::tempdir().unwrap();
        let fallback_marker = directory.path().join("pty-fallback-started");
        let binary = ready_then_disconnect_test_cli(directory.path(), &fallback_marker);
        let mut supervisor = AcpProcessSupervisor::spawn(
            binary,
            directory.path().to_path_buf(),
            "tinto-post-session".to_owned(),
            AcpLaunchIntent::NewSession,
        )
        .unwrap();
        let ready_deadline = Instant::now() + Duration::from_secs(3);
        while supervisor.acp_runtime().unwrap().state != AgentSessionAcpState::AcpReady {
            assert!(Instant::now() < ready_deadline, "ACP did not become ready");
            std::thread::sleep(Duration::from_millis(10));
        }

        supervisor.write_turn("do not replay", &[], None).unwrap();
        let failed_deadline = Instant::now() + Duration::from_secs(3);
        let runtime = loop {
            let runtime = supervisor.acp_runtime().unwrap();
            if runtime.state == AgentSessionAcpState::Failed {
                break runtime;
            }
            assert!(
                Instant::now() < failed_deadline,
                "ACP disconnect did not fail the structured session"
            );
            std::thread::sleep(Duration::from_millis(10));
        };

        assert!(runtime
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("No se reenvió ni reprodujo")));
        assert!(!runtime.retry_available);
        assert!(!fallback_marker.exists());
        assert!(supervisor.try_exit_code().unwrap().is_some());
    }

    #[cfg(windows)]
    fn ready_then_disconnect_test_cli(directory: &Path, fallback_marker: &Path) -> PathBuf {
        let script = directory.join("fake-kimi-ready.ps1");
        std::fs::write(
            &script,
            concat!(
                "$null = [Console]::In.ReadLine()\n",
                "[Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":false,\"promptCapabilities\":{\"image\":false,\"audio\":false,\"embeddedContext\":false}}}}')\n",
                "[Console]::Out.Flush()\n",
                "$null = [Console]::In.ReadLine()\n",
                "[Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"sessionId\":\"fixture-session\"}}')\n",
                "[Console]::Out.Flush()\n",
                "$null = [Console]::In.ReadLine()\n",
            ),
        )
        .unwrap();
        let path = directory.join("fake-kimi-ready.cmd");
        std::fs::write(
            &path,
            format!(
                "@echo off\r\nif \"%1\"==\"acp\" (\r\n  powershell.exe -NoProfile -NonInteractive -File \"{}\"\r\n  exit /b %errorlevel%\r\n)\r\n> \"{}\" echo fallback\r\nping -n 30 127.0.0.1 >nul\r\n",
                script.display(),
                fallback_marker.display()
            ),
        )
        .unwrap();
        path
    }

    #[cfg(unix)]
    fn ready_then_disconnect_test_cli(directory: &Path, fallback_marker: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("fake-kimi-ready");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nif [ \"$1\" != \"acp\" ]; then\n  : > \"{}\"\n  sleep 30\n  exit 0\nfi\nIFS= read -r _\nprintf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{{\"protocolVersion\":1,\"agentCapabilities\":{{\"loadSession\":false,\"promptCapabilities\":{{\"image\":false,\"audio\":false,\"embeddedContext\":false}}}}}}}}'\nIFS= read -r _\nprintf '%s\\n' '{{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{{\"sessionId\":\"fixture-session\"}}}}'\nIFS= read -r _\n",
                fallback_marker.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(windows)]
    fn blocking_test_cli(directory: &Path) -> PathBuf {
        let path = directory.join("fake-kimi.cmd");
        std::fs::write(&path, "@echo off\r\nping -n 30 127.0.0.1 >nul\r\n").unwrap();
        path
    }

    #[cfg(windows)]
    fn auth_required_test_cli(directory: &Path) -> PathBuf {
        let script = directory.join("fake-kimi-auth.ps1");
        std::fs::write(
            &script,
            "[Console]::Out.WriteLine('{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32000,\"message\":\"auth required\"}}')\nStart-Sleep -Seconds 30\n",
        )
        .unwrap();
        let path = directory.join("fake-kimi-auth.cmd");
        std::fs::write(
            &path,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -NonInteractive -File \"{}\"\r\n",
                script.display()
            ),
        )
        .unwrap();
        path
    }

    #[cfg(unix)]
    fn blocking_test_cli(directory: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("fake-kimi");
        std::fs::write(&path, "#!/bin/sh\nsleep 30\n").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    fn auth_required_test_cli(directory: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("fake-kimi-auth");
        std::fs::write(
            &path,
            "#!/bin/sh\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32000,\"message\":\"auth required\"}}'\nsleep 30\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn assert_provider_fixture(fixture: &str, expected_version: &str, expects_auth_error: bool) {
        let entries = fixture
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(!entries.is_empty());
        assert_eq!(entries[0]["source"], "observed");
        assert_eq!(entries[0]["redacted"], true);
        assert!(entries[0]["message"]["result"].get("authMethods").is_none());
        assert_eq!(
            entries[0]["message"]["result"]["agentInfo"]["version"],
            expected_version
        );
        let initialize: InitializeResponse =
            typed(entries[0]["message"]["result"].clone()).unwrap();
        assert_eq!(initialize.protocol_version, ProtocolVersion::V1);

        let find_message = |predicate: &dyn Fn(&Value) -> bool| {
            entries
                .iter()
                .find(|entry| predicate(&entry["message"]))
                .map(|entry| entry["message"].clone())
                .unwrap()
        };
        let mut core = AcpConnectionCore::new("fixture", "tinto-fixture", 1, AcpLimits::default());
        core.initialize().unwrap();
        core.handle_frame(1, &serde_json::to_vec(&entries[0]["message"]).unwrap())
            .unwrap();
        assert_eq!(core.phase(), AcpPhase::Initialized);
        core.new_session(&absolute_test_path("fixture")).unwrap();
        let session_ready = find_message(&|message| {
            message["id"] == 2 && message["result"]["sessionId"] == "fixture-session"
        });
        core.handle_frame(1, &serde_json::to_vec(&session_ready).unwrap())
            .unwrap();
        assert_eq!(core.phase(), AcpPhase::Ready);

        let prompt = core.prompt_text("fixture-turn", "fixture prompt").unwrap();
        let prompt: Value = serde_json::from_slice(&prompt).unwrap();
        let fixture_prompt =
            find_message(&|message| message["method"] == AGENT_METHOD_NAMES.session_prompt);
        assert_eq!(prompt, fixture_prompt);

        let update =
            find_message(&|message| message["method"] == CLIENT_METHOD_NAMES.session_update);
        let updates = core
            .handle_frame(1, &serde_json::to_vec(&update).unwrap())
            .unwrap();
        assert!(matches!(
            updates.as_slice(),
            [AcpEvent::Update(AcpUpdate {
                kind: AcpUpdateKind::AgentMessage,
                ..
            })]
        ));

        let permission = find_message(&|message| {
            message["method"] == CLIENT_METHOD_NAMES.session_request_permission
        });
        let permission = core
            .handle_frame(1, &serde_json::to_vec(&permission).unwrap())
            .unwrap();
        assert!(matches!(
            permission.as_slice(),
            [AcpEvent::PermissionRequested(_)]
        ));

        let cancel =
            find_message(&|message| message["method"] == AGENT_METHOD_NAMES.session_cancel);
        let cancel_frames = core.cancel_turn().unwrap();
        let emitted_cancel: Value = serde_json::from_slice(&cancel_frames[0]).unwrap();
        assert_eq!(emitted_cancel, cancel);

        let completion =
            find_message(&|message| message["id"] == 3 && message.get("result").is_some());
        let completed = core
            .handle_frame(1, &serde_json::to_vec(&completion).unwrap())
            .unwrap();
        assert!(matches!(
            completed.last(),
            Some(AcpEvent::TurnCompleted { turn_id, .. }) if turn_id == "fixture-turn"
        ));

        let auth_message = entries
            .iter()
            .find(|entry| entry["source"] == "observed" && entry["message"].get("error").is_some());
        assert_eq!(auth_message.is_some(), expects_auth_error);
        if let Some(auth_message) = auth_message {
            let mut auth_core =
                AcpConnectionCore::new("fixture", "tinto-auth-fixture", 1, AcpLimits::default());
            auth_core.initialize().unwrap();
            auth_core
                .handle_frame(1, &serde_json::to_vec(&entries[0]["message"]).unwrap())
                .unwrap();
            auth_core
                .new_session(&absolute_test_path("fixture"))
                .unwrap();
            let events = auth_core
                .handle_frame(1, &serde_json::to_vec(&auth_message["message"]).unwrap())
                .unwrap();
            assert!(events.contains(&AcpEvent::AuthenticationRequired));
        }
    }

    #[test]
    fn official_schema_validates_the_kimi_and_opencode_provider_fixtures() {
        assert_provider_fixture(
            include_str!("test_fixtures/kimi-acp-v1.jsonl"),
            "0.27.0",
            true,
        );
        assert_provider_fixture(
            include_str!("test_fixtures/opencode-acp-v1.jsonl"),
            "1.18.3",
            false,
        );
    }
}
