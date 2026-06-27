use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent_console::checkpoint::CheckpointRecord;
use crate::bus::contract::{
    AgentSessionChange, FileContent, GitleaksInstallResult, GitleaksSetupStatus, RepoDelta,
    RepoTree, SubscriptionTarget,
};
use crate::file_ops::commands::{CopyResult, DeleteResult};
use crate::git::{CommitInfo, FileDiff};

pub const PROTOCOL_VERSION: u16 = 1;
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_MESSAGE_BYTES: usize = 20 * 1024 * 1024;
pub const HANDSHAKE_MESSAGE: &str = "handshake";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeRequest {
    #[serde(rename = "type")]
    pub message_type: String,
    pub protocol_version: u16,
    pub client_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    #[serde(rename = "type")]
    pub message_type: String,
    pub protocol_version: u16,
    pub agent_version: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprint {
    pub path: PathBuf,
    pub size: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoFileFingerprintSnapshot {
    pub repo: PathBuf,
    pub files: Vec<FileFingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoFsWatchConfig {
    pub repo: PathBuf,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WslDirectoryEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WslDirectoryListing {
    pub path: String,
    pub is_git_repo: bool,
    pub entries: Vec<WslDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRequest {
    Handshake {
        protocol_version: u16,
        client_version: String,
    },
    RepoSnapshot {
        protocol_version: u16,
        repos: Vec<PathBuf>,
        subscriptions: Vec<SubscriptionTarget>,
    },
    RepoSnapshotWithFsEvents {
        protocol_version: u16,
        repos: Vec<PathBuf>,
        subscriptions: Vec<SubscriptionTarget>,
        fs_watch: Vec<RepoFsWatchConfig>,
    },
    ListDirectory {
        protocol_version: u16,
        path: Option<PathBuf>,
    },
    WorktreeDiff {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
    },
    CommitDiff {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        commit_id: String,
    },
    CommitLog {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        offset: usize,
        limit: usize,
    },
    Blob {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        commit_id: String,
        path: PathBuf,
    },
    FileContent {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        path: PathBuf,
    },
    MediaContent {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        path: PathBuf,
    },
    RepoTree {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
    },
    GitleaksSetupStatus {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
    },
    InstallGitleaks {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
    },
    CreateGitleaksConfig {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
    },
    AgentBinaryAvailable {
        protocol_version: u16,
        agent_type: String,
    },
    AgentCheckpointCreate {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        session_id: String,
        created_at_ms: u64,
    },
    AgentCheckpointScan {
        protocol_version: u16,
        allowed_repos: Vec<PathBuf>,
        checkpoint: CheckpointRecord,
        timestamp_ms: u64,
    },
    AgentCheckpointRevert {
        protocol_version: u16,
        allowed_repos: Vec<PathBuf>,
        checkpoint: CheckpointRecord,
    },
    AgentCheckpointRevertFile {
        protocol_version: u16,
        allowed_repos: Vec<PathBuf>,
        checkpoint: CheckpointRecord,
        path: PathBuf,
    },
    CopyToRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        dest_dir: PathBuf,
        sources: Vec<PathBuf>,
        overwrite: bool,
    },
    CopyWithinRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        overwrite: bool,
    },
    MoveWithinRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
        overwrite: bool,
    },
    ExportFromRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
    },
    DeleteFromRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        sources: Vec<PathBuf>,
    },
    RestoreDeletedFromRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        token: String,
    },
    RedoDeletedFromRepo {
        protocol_version: u16,
        repo: PathBuf,
        allowed_repos: Vec<PathBuf>,
        token: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentResponse {
    Handshake {
        protocol_version: u16,
        agent_version: String,
        status: AgentStatus,
    },
    RepoSnapshot {
        repos: Vec<RepoDelta>,
    },
    RepoSnapshotWithFsEvents {
        repos: Vec<RepoDelta>,
        fingerprints: Vec<RepoFileFingerprintSnapshot>,
    },
    DirectoryListing {
        listing: WslDirectoryListing,
    },
    WorktreeDiff {
        diffs: Vec<FileDiff>,
    },
    CommitDiff {
        diffs: Vec<FileDiff>,
    },
    CommitLog {
        commits: Vec<CommitInfo>,
    },
    Blob {
        content: FileContent,
    },
    FileContent {
        content: FileContent,
    },
    MediaContent {
        content: FileContent,
    },
    RepoTree {
        tree: RepoTree,
    },
    GitleaksSetupStatus {
        status: GitleaksSetupStatus,
    },
    GitleaksInstallResult {
        result: GitleaksInstallResult,
    },
    AgentBinaryAvailable {
        available: bool,
    },
    AgentCheckpoint {
        checkpoint: CheckpointRecord,
    },
    AgentChangeLog {
        changes: Vec<AgentSessionChange>,
    },
    CopyResult {
        result: CopyResult,
    },
    DeleteResult {
        result: DeleteResult,
    },
    Unit,
    Error {
        category: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    pub category: AgentErrorCategory,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorCategory {
    MissingWsl,
    MissingDistro,
    MissingAgent,
    SpawnFailed,
    Timeout,
    ProtocolMismatch,
    MalformedResponse,
    OversizedResponse,
    ChildExit,
}

impl AgentErrorCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentErrorCategory::MissingWsl => "missing_wsl",
            AgentErrorCategory::MissingDistro => "missing_distro",
            AgentErrorCategory::MissingAgent => "missing_agent",
            AgentErrorCategory::SpawnFailed => "spawn_failed",
            AgentErrorCategory::Timeout => "timeout",
            AgentErrorCategory::ProtocolMismatch => "protocol_mismatch",
            AgentErrorCategory::MalformedResponse => "malformed_response",
            AgentErrorCategory::OversizedResponse => "oversized_response",
            AgentErrorCategory::ChildExit => "child_exit",
        }
    }
}

impl AgentError {
    pub fn new(category: AgentErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }

    pub fn safe_category(&self) -> &'static str {
        self.category.as_str()
    }
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.safe_category(), self.message)
    }
}

impl std::error::Error for AgentError {}

impl HandshakeRequest {
    pub fn current(client_version: impl Into<String>) -> Self {
        Self {
            message_type: HANDSHAKE_MESSAGE.into(),
            protocol_version: PROTOCOL_VERSION,
            client_version: client_version.into(),
        }
    }
}

impl HandshakeResponse {
    pub fn current() -> Self {
        Self {
            message_type: HANDSHAKE_MESSAGE.into(),
            protocol_version: PROTOCOL_VERSION,
            agent_version: AGENT_VERSION.into(),
            status: AgentStatus::Ok,
        }
    }
}

pub fn encode_request(request: &HandshakeRequest) -> Result<String, AgentError> {
    encode_line(request)
}

pub fn encode_response(response: &HandshakeResponse) -> Result<String, AgentError> {
    encode_line(response)
}

pub fn encode_agent_request(request: &AgentRequest) -> Result<String, AgentError> {
    encode_line(request)
}

pub fn encode_agent_response(response: &AgentResponse) -> Result<String, AgentError> {
    encode_line(response)
}

fn encode_line<T: Serialize>(value: &T) -> Result<String, AgentError> {
    let mut line = serde_json::to_string(value).map_err(|_| {
        AgentError::new(
            AgentErrorCategory::MalformedResponse,
            "no se pudo serializar el mensaje del agente",
        )
    })?;
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(AgentError::new(
            AgentErrorCategory::OversizedResponse,
            "el mensaje del agente supera el limite permitido",
        ));
    }
    line.push('\n');
    Ok(line)
}

pub fn parse_request_line(line: &str) -> Result<HandshakeRequest, AgentError> {
    let request: HandshakeRequest = parse_line(line)?;
    validate_message_type(&request.message_type)?;
    validate_protocol(request.protocol_version)?;
    Ok(request)
}

pub fn parse_response_line(line: &str) -> Result<HandshakeResponse, AgentError> {
    let response: HandshakeResponse = parse_line(line)?;
    validate_message_type(&response.message_type)?;
    validate_protocol(response.protocol_version)?;
    Ok(response)
}

pub fn parse_agent_request_line(line: &str) -> Result<AgentRequest, AgentError> {
    let request: AgentRequest = parse_line(line)?;
    validate_protocol(request.protocol_version())?;
    Ok(request)
}

pub fn parse_agent_response_line(line: &str) -> Result<AgentResponse, AgentError> {
    let response: AgentResponse = parse_line(line)?;
    if let AgentResponse::Handshake {
        protocol_version, ..
    } = &response
    {
        validate_protocol(*protocol_version)?;
    }
    Ok(response)
}

pub fn respond_to_handshake_line(line: &str) -> Result<String, AgentError> {
    parse_request_line(line)?;
    encode_response(&HandshakeResponse::current())
}

impl AgentRequest {
    pub fn handshake(client_version: impl Into<String>) -> Self {
        Self::Handshake {
            protocol_version: PROTOCOL_VERSION,
            client_version: client_version.into(),
        }
    }

    pub fn protocol_version(&self) -> u16 {
        match self {
            Self::Handshake {
                protocol_version, ..
            }
            | Self::RepoSnapshot {
                protocol_version, ..
            }
            | Self::RepoSnapshotWithFsEvents {
                protocol_version, ..
            }
            | Self::ListDirectory {
                protocol_version, ..
            }
            | Self::WorktreeDiff {
                protocol_version, ..
            }
            | Self::CommitDiff {
                protocol_version, ..
            }
            | Self::CommitLog {
                protocol_version, ..
            }
            | Self::Blob {
                protocol_version, ..
            }
            | Self::FileContent {
                protocol_version, ..
            }
            | Self::MediaContent {
                protocol_version, ..
            }
            | Self::RepoTree {
                protocol_version, ..
            }
            | Self::GitleaksSetupStatus {
                protocol_version, ..
            }
            | Self::InstallGitleaks {
                protocol_version, ..
            }
            | Self::CreateGitleaksConfig {
                protocol_version, ..
            }
            | Self::AgentBinaryAvailable {
                protocol_version, ..
            }
            | Self::AgentCheckpointCreate {
                protocol_version, ..
            }
            | Self::AgentCheckpointScan {
                protocol_version, ..
            }
            | Self::AgentCheckpointRevert {
                protocol_version, ..
            }
            | Self::AgentCheckpointRevertFile {
                protocol_version, ..
            }
            | Self::CopyToRepo {
                protocol_version, ..
            }
            | Self::CopyWithinRepo {
                protocol_version, ..
            }
            | Self::MoveWithinRepo {
                protocol_version, ..
            }
            | Self::ExportFromRepo {
                protocol_version, ..
            }
            | Self::DeleteFromRepo {
                protocol_version, ..
            }
            | Self::RestoreDeletedFromRepo {
                protocol_version, ..
            }
            | Self::RedoDeletedFromRepo {
                protocol_version, ..
            } => *protocol_version,
        }
    }
}

impl AgentResponse {
    pub fn current_handshake() -> Self {
        Self::Handshake {
            protocol_version: PROTOCOL_VERSION,
            agent_version: AGENT_VERSION.into(),
            status: AgentStatus::Ok,
        }
    }

    pub fn error(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            category: category.into(),
            message: message.into(),
        }
    }
}

fn parse_line<T>(line: &str) -> Result<T, AgentError>
where
    T: for<'de> Deserialize<'de>,
{
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(AgentError::new(
            AgentErrorCategory::OversizedResponse,
            "el mensaje del agente supera el limite permitido",
        ));
    }
    serde_json::from_str(line.trim_end_matches(['\r', '\n']))
        .map_err(|_| AgentError::new(AgentErrorCategory::MalformedResponse, "mensaje invalido"))
}

fn validate_message_type(message_type: &str) -> Result<(), AgentError> {
    if message_type == HANDSHAKE_MESSAGE {
        Ok(())
    } else {
        Err(AgentError::new(
            AgentErrorCategory::MalformedResponse,
            "tipo de mensaje no soportado",
        ))
    }
}

fn validate_protocol(protocol_version: u16) -> Result<(), AgentError> {
    if protocol_version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(AgentError::new(
            AgentErrorCategory::ProtocolMismatch,
            "version de protocolo incompatible",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_roundtrip_compatible() {
        let request = HandshakeRequest::current("host-test");
        let line = encode_request(&request).expect("encode request");
        let response_line = respond_to_handshake_line(&line).expect("response");
        let response = parse_response_line(&response_line).expect("parse response");

        assert_eq!(response.message_type, HANDSHAKE_MESSAGE);
        assert_eq!(response.protocol_version, PROTOCOL_VERSION);
        assert_eq!(response.agent_version, AGENT_VERSION);
        assert_eq!(response.status, AgentStatus::Ok);
    }

    #[test]
    fn incompatible_protocol_is_rejected() {
        let line = r#"{"type":"handshake","protocol_version":999,"client_version":"x"}"#;
        let error = parse_request_line(line).expect_err("incompatible");

        assert_eq!(error.category, AgentErrorCategory::ProtocolMismatch);
        assert_eq!(error.safe_category(), "protocol_mismatch");
    }

    #[test]
    fn malformed_and_wrong_type_are_safe_errors() {
        let malformed = parse_request_line("{ nope").expect_err("malformed");
        assert_eq!(malformed.category, AgentErrorCategory::MalformedResponse);

        let wrong =
            parse_request_line(r#"{"type":"read_file","protocol_version":1,"client_version":"x"}"#)
                .expect_err("wrong type");
        assert_eq!(wrong.category, AgentErrorCategory::MalformedResponse);
    }

    #[test]
    fn generic_request_response_roundtrip() {
        let request = AgentRequest::WorktreeDiff {
            protocol_version: PROTOCOL_VERSION,
            repo: "/home/me/repo".into(),
            allowed_repos: vec!["/home/me/repo".into()],
        };
        let line = encode_agent_request(&request).expect("encode");
        let parsed = parse_agent_request_line(&line).expect("parse request");

        assert_eq!(parsed, request);

        let response = AgentResponse::WorktreeDiff { diffs: Vec::new() };
        let line = encode_agent_response(&response).expect("encode response");
        let parsed = parse_agent_response_line(&line).expect("parse response");

        assert_eq!(parsed, response);
    }

    #[test]
    fn oversized_message_is_rejected_before_json_parse() {
        let line = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let error = parse_response_line(&line).expect_err("oversized");

        assert_eq!(error.category, AgentErrorCategory::OversizedResponse);
    }
}
