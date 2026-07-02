//! Tipos del contrato congelado backend↔frontend (ver
//! `docs/contracts/bus-contract.md`). Cambios additive-first: campos y
//! variantes nuevas sí; renames/removals no.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::git::{BranchInfo, CommitInfo, FileDiff, RepoStatus};

/// Nombres de los eventos `emit` del contrato.
pub const EVENT_WORKBENCH_DELTA: &str = "tinto://workbench-delta";
pub const EVENT_FS_EVENTS: &str = "tinto://fs-events";
pub const EVENT_WATCHING_STATE: &str = "tinto://watching-state";
pub const EVENT_AGENT_SESSIONS_CHANGED: &str = "tinto://agent-sessions-changed";
pub const EVENT_AGENT_SESSION_OUTPUT: &str = "tinto://agent-session-output";
pub const EVENT_AGENT_SESSION_CHANGE_LOG: &str = "tinto://agent-session-change-log";
pub const EVENT_AGENT_SESSION_TIMELINE: &str = "tinto://agent-session-timeline";

/// Estado lifecycle de una sesion de agente gestionada por el backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Starting,
    Running,
    Exited,
    Error,
    Completed,
    Failed,
    Reverted,
}

/// Error estructurado y seguro para comandos/lifecycle de sesiones de agente.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionError {
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionCheckpointType {
    GitRef,
    FsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionCheckpoint {
    pub checkpoint_type: AgentSessionCheckpointType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_hash: Option<String>,
    pub snapshot_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentSessionChangeKind {
    Created,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionChange {
    pub path: PathBuf,
    pub kind: AgentSessionChangeKind,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionChangeLog {
    pub session_id: String,
    pub changes: Vec<AgentSessionChange>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionTurnStatus {
    Waiting,
    Working,
    Settling,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionTurnCheckpoint {
    pub id: String,
    pub index: u32,
    pub started_at_ms: u64,
    pub ended_at_ms: u64,
    pub checkpoint: AgentSessionCheckpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_checkpoint: Option<AgentSessionCheckpoint>,
    pub changes: Vec<AgentSessionChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionLimits {
    pub max_sessions: usize,
    pub max_sessions_per_repo: usize,
    pub max_lifetime_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionRuntimeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionGoal {
    pub text: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionPersonality {
    pub name: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionPlanMode {
    pub enabled: bool,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionFeedback {
    pub kind: String,
    pub text: String,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionContextSummary {
    pub text: String,
    pub created_at_ms: u64,
    pub source_events: usize,
    pub source_turns: usize,
}

/// Metadata publica de una sesion de agente. La E/S PTY se anade en ACI-002.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSession {
    pub id: String,
    pub repo: PathBuf,
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
    pub status: AgentSessionStatus,
    pub pid: Option<u32>,
    pub started_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub error: Option<AgentSessionError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<AgentSessionCheckpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change_log: Vec<AgentSessionChange>,
    pub turn_status: AgentSessionTurnStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turn_checkpoints: Vec<AgentSessionTurnCheckpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeline: Vec<AgentSessionTimelineItem>,
    #[serde(default, skip_serializing_if = "AgentSessionRuntimeOptions::is_empty")]
    pub runtime_options: AgentSessionRuntimeOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<AgentSessionGoal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<AgentSessionPersonality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<AgentSessionPlanMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<AgentSessionFeedback>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_summary: Option<AgentSessionContextSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverted_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored_to_turn_index: Option<u32>,
    pub active_sessions: usize,
    pub age_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bytes_per_second: Option<u64>,
}

impl AgentSessionRuntimeOptions {
    pub fn is_empty(&self) -> bool {
        self.model.is_none() && self.reasoning_effort.is_none() && self.speed.is_none()
    }
}

/// Chunk binario del PTY de una sesion de agente, transportado en base64 para
/// preservar ANSI y bytes parciales.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionOutput {
    pub session_id: String,
    pub chunk_base64: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionTimelineKind {
    UserMessage,
    AgentMessage,
    CommandOutput,
    Lifecycle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionTimelineItem {
    pub session_id: String,
    pub id: String,
    pub kind: AgentSessionTimelineKind,
    pub text: String,
    pub timestamp_ms: u64,
}

/// Guarda de tamaño para contenido de archivos/blobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentJournalSessionSummary {
    pub id: String,
    pub repo: PathBuf,
    pub agent_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wsl_distro: Option<String>,
    pub status: AgentSessionStatus,
    pub started_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at_ms: Option<u64>,
    pub updated_at_ms: u64,
    pub event_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_kind: Option<AgentSessionTimelineKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentHostCommandStatus {
    Completed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReviewSummary {
    pub branch: String,
    pub changed_files: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_shortstat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staged_shortstat: Option<String>,
    pub files: Vec<String>,
    pub truncated_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentReviewFinding {
    pub severity: String,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentHostCommandResult {
    pub command: String,
    pub status: AgentHostCommandStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_summary: Option<AgentReviewSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_findings: Option<Vec<AgentReviewFinding>>,
}

pub const FILE_CONTENT_MAX_BYTES: usize = 1024 * 1024;
/// Guarda de tamaño para vistas multimedia embebidas (PDF/imágenes).
pub const MEDIA_CONTENT_MAX_BYTES: usize = 12 * 1024 * 1024;
/// Cap de entradas del árbol del repo.
pub const REPO_TREE_MAX_ENTRIES: usize = 20_000;
/// Cap del conjunto de suscripciones.
pub const MAX_SUBSCRIPTIONS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitleaksSetupStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitleaksInstallResult {
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
    pub method: Option<String>,
    pub message: String,
}

/// Clase del error de un repo (contrato para los estados de UI).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepoErrorClass {
    /// GitError de recálculo: se limpia solo en el siguiente recálculo OK.
    Transient,
    /// Error del watcher (repo removido, fallo de montaje/clasificador):
    /// persiste hasta remount (`retry_repo`, conmutación o snapshot).
    Terminal,
}

/// Error de un repo, serializado con mensaje seguro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoErrorState {
    pub class: RepoErrorClass,
    pub category: String,
    pub message: String,
}

/// Severidad factual de una señal pasiva. No implica juicio de calidad.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SignalSeverity {
    Info,
    Warning,
    Critical,
}

/// Tipo de señal pasiva detectada por reglas determinísticas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PassiveSignalKind {
    SensitivePath,
    PossibleSecret,
    LargeDelete,
    ConfigChange,
    TestChange,
}

/// Señal pasiva: hecho detectado, sin valores secretos ni resumen interpretativo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PassiveSignal {
    pub kind: PassiveSignalKind,
    pub severity: SignalSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub message: String,
}

/// Métricas livianas del estado actual del repo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RepoMetrics {
    pub changed_files: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

/// Hallazgo de secreto asociado a una línea concreta del archivo actual.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretFinding {
    pub path: PathBuf,
    pub line: u32,
    pub rule_id: String,
    pub description: String,
}

/// Delta de estado de un repo (evento `tinto://workbench-delta` y entrada
/// del snapshot).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoDelta {
    /// Identidad: path canónico del repo.
    pub repo: PathBuf,
    /// Monotónica por repo; el consumidor aplica solo si es más nueva.
    pub revision: u64,
    pub status: RepoStatus,
    pub branch: Option<BranchInfo>,
    pub head: Option<CommitInfo>,
    pub last_activity_ms: u64,
    pub error: Option<RepoErrorState>,
    pub metrics: RepoMetrics,
    pub gitleaks_configured: bool,
    #[serde(default)]
    pub agents_md_configured: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<PassiveSignal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_findings: Vec<SecretFinding>,
    /// Diffs de los objetivos suscritos de este repo; `None` sin suscripción.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribed_diffs: Option<Vec<FileDiff>>,
}

/// Tipo de evento FS del Plano 2 (taxonomía del watcher).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FsEventKind {
    Created,
    Modified,
    Removed,
}

/// Un evento del Plano 2 (archivo vigilado).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FsEvent {
    pub path: PathBuf,
    pub kind: FsEventKind,
    pub timestamp_ms: u64,
    /// Tamaño actual; `None` si el archivo ya no existe.
    pub size: Option<u64>,
    /// Delta vs el último tamaño conocido; `None` sin tamaño previo.
    pub size_delta: Option<i64>,
    /// Señales aplicables a este evento.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<PassiveSignal>,
}

/// Lote de eventos FS de un repo (evento `tinto://fs-events`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FsEventBatch {
    pub repo: PathBuf,
    pub events: Vec<FsEvent>,
}

/// Disponibilidad del watching (evento `tinto://watching-state`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchingState {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Objetivo de suscripción: un repo, opcionalmente un archivo.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubscriptionTarget {
    pub repo: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Entrada del árbol del repo (lista plana; el frontend arma el árbol).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub is_dir: bool,
}

/// Respuesta de `list_repo_tree`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTree {
    pub entries: Vec<TreeEntry>,
    pub truncated: bool,
}

/// Contenido de un archivo/blob con guardas (1 MiB, binario → base64).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileContent {
    pub encoding: ContentEncoding,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncoding {
    Utf8,
    Base64,
}

/// Snapshot completo del workbench activo (`get_workbench_snapshot`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkbenchSnapshot {
    pub watching: WatchingState,
    pub repos: Vec<RepoDelta>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ancla del contrato: el shape JSON de RepoDelta no cambia sin querer.
    #[test]
    fn repo_delta_serializa_con_el_shape_del_contrato() {
        let delta = RepoDelta {
            repo: "/r/a".into(),
            revision: 7,
            status: RepoStatus::default(),
            branch: None,
            head: None,
            last_activity_ms: 123,
            error: Some(RepoErrorState {
                class: RepoErrorClass::Terminal,
                category: "repo-removed".into(),
                message: "el root fue removido".into(),
            }),
            metrics: RepoMetrics {
                changed_files: 1,
                lines_added: 2,
                lines_removed: 3,
            },
            gitleaks_configured: false,
            agents_md_configured: false,
            signals: vec![PassiveSignal {
                kind: PassiveSignalKind::SensitivePath,
                severity: SignalSeverity::Warning,
                path: Some(".env".into()),
                message: "Sensitive filename changed".into(),
            }],
            secret_findings: vec![SecretFinding {
                path: "src/config.ts".into(),
                line: 12,
                rule_id: "generic-api-key".into(),
                description: "Possible secret".into(),
            }],
            subscribed_diffs: None,
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["repo"], "/r/a");
        assert_eq!(json["revision"], 7);
        assert_eq!(json["error"]["class"], "terminal");
        assert_eq!(json["metrics"]["changed_files"], 1);
        assert_eq!(json["gitleaks_configured"], false);
        assert_eq!(json["agents_md_configured"], false);
        assert_eq!(json["signals"][0]["kind"], "sensitive_path");
        assert_eq!(json["signals"][0]["severity"], "warning");
        assert_eq!(json["secret_findings"][0]["line"], 12);
        assert_eq!(json["secret_findings"][0]["rule_id"], "generic-api-key");
        assert!(json.get("subscribed_diffs").is_none(), "None se omite");
        for key in ["status", "branch", "head", "last_activity_ms"] {
            assert!(json.get(key).is_some(), "falta {key}");
        }
    }

    #[test]
    fn file_content_y_fs_event_serializan_estables() {
        let fc = FileContent {
            encoding: ContentEncoding::Base64,
            content: "QUJD".into(),
            truncated: true,
        };
        let json = serde_json::to_value(&fc).unwrap();
        assert_eq!(json["encoding"], "base64");
        assert_eq!(json["truncated"], true);

        let ev = FsEvent {
            path: ".env".into(),
            kind: FsEventKind::Modified,
            timestamp_ms: 1,
            size: Some(10),
            size_delta: Some(-2),
            signals: vec![PassiveSignal {
                kind: PassiveSignalKind::ConfigChange,
                severity: SignalSeverity::Warning,
                path: Some("ci.yml".into()),
                message: "Config file changed".into(),
            }],
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["kind"], "modified");
        assert_eq!(json["size_delta"], -2);
        assert_eq!(json["signals"][0]["kind"], "config_change");
    }

    #[test]
    fn agent_session_serializa_con_estado_snake_case() {
        let session = AgentSession {
            id: "sess-1".into(),
            repo: "/r/api".into(),
            agent_type: "codex".into(),
            wsl_distro: None,
            status: AgentSessionStatus::Running,
            pid: Some(42),
            started_at_ms: 1760000000000,
            ended_at_ms: None,
            exit_code: None,
            error: Some(AgentSessionError {
                category: "spawn_failed".into(),
                message: "no se pudo iniciar la sesion".into(),
            }),
            checkpoint: Some(AgentSessionCheckpoint {
                checkpoint_type: AgentSessionCheckpointType::GitRef,
                git_hash: Some("abc123".into()),
                snapshot_files: Vec::new(),
            }),
            change_log: vec![AgentSessionChange {
                path: "src/a.rs".into(),
                kind: AgentSessionChangeKind::Modified,
                timestamp_ms: 1760000000001,
            }],
            turn_status: AgentSessionTurnStatus::Working,
            turn_checkpoints: vec![AgentSessionTurnCheckpoint {
                id: "sess-1:turn-1".into(),
                index: 1,
                started_at_ms: 1760000000000,
                ended_at_ms: 1760000000100,
                checkpoint: AgentSessionCheckpoint {
                    checkpoint_type: AgentSessionCheckpointType::GitRef,
                    git_hash: Some("abc123".into()),
                    snapshot_files: Vec::new(),
                },
                restore_checkpoint: Some(AgentSessionCheckpoint {
                    checkpoint_type: AgentSessionCheckpointType::GitRef,
                    git_hash: Some("def456".into()),
                    snapshot_files: Vec::new(),
                }),
                changes: vec![AgentSessionChange {
                    path: "src/a.rs".into(),
                    kind: AgentSessionChangeKind::Modified,
                    timestamp_ms: 1760000000100,
                }],
            }],
            timeline: vec![AgentSessionTimelineItem {
                session_id: "sess-1".into(),
                id: "evt-1".into(),
                kind: AgentSessionTimelineKind::AgentMessage,
                text: "Listo".into(),
                timestamp_ms: 1760000000101,
            }],
            runtime_options: AgentSessionRuntimeOptions {
                model: Some("gpt-5.5".into()),
                reasoning_effort: Some("high".into()),
                speed: Some("standard".into()),
            },
            goal: Some(AgentSessionGoal {
                text: "Ship host commands".into(),
                updated_at_ms: 1760000000200,
            }),
            personality: Some(AgentSessionPersonality {
                name: "precise".into(),
                updated_at_ms: 1760000000250,
            }),
            plan_mode: Some(AgentSessionPlanMode {
                enabled: true,
                updated_at_ms: 1760000000255,
            }),
            feedback: vec![AgentSessionFeedback {
                kind: "feedback".into(),
                text: "The command palette should stay native.".into(),
                created_at_ms: 1760000000260,
            }],
            context_summary: Some(AgentSessionContextSummary {
                text: "Goal: Ship host commands".into(),
                created_at_ms: 1760000000300,
                source_events: 4,
                source_turns: 2,
            }),
            reverted_at_ms: None,
            restored_to_turn_index: None,
            active_sessions: 1,
            age_ms: 42,
            output_bytes_per_second: None,
        };

        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["id"], "sess-1");
        assert_eq!(json["repo"], "/r/api");
        assert_eq!(json["agent_type"], "codex");
        assert_eq!(json["status"], "running");
        assert_eq!(json["pid"], 42);
        assert_eq!(json["started_at_ms"], 1760000000000u64);
        assert!(json["exit_code"].is_null());
        assert_eq!(json["error"]["category"], "spawn_failed");
        assert_eq!(json["checkpoint"]["checkpoint_type"], "git_ref");
        assert_eq!(json["change_log"][0]["kind"], "modified");
        assert_eq!(json["turn_status"], "working");
        assert_eq!(json["turn_checkpoints"][0]["id"], "sess-1:turn-1");
        assert_eq!(
            json["turn_checkpoints"][0]["changes"][0]["kind"],
            "modified"
        );
        assert_eq!(json["timeline"][0]["kind"], "agent_message");
        assert_eq!(json["timeline"][0]["text"], "Listo");
        assert_eq!(json["runtime_options"]["model"], "gpt-5.5");
        assert_eq!(json["runtime_options"]["reasoning_effort"], "high");
        assert_eq!(json["runtime_options"]["speed"], "standard");
        assert_eq!(json["goal"]["text"], "Ship host commands");
        assert_eq!(json["goal"]["updated_at_ms"], 1760000000200u64);
        assert_eq!(json["personality"]["name"], "precise");
        assert_eq!(json["personality"]["updated_at_ms"], 1760000000250u64);
        assert_eq!(json["plan_mode"]["enabled"], true);
        assert_eq!(json["plan_mode"]["updated_at_ms"], 1760000000255u64);
        assert_eq!(json["feedback"][0]["kind"], "feedback");
        assert_eq!(
            json["feedback"][0]["text"],
            "The command palette should stay native."
        );
        assert_eq!(json["feedback"][0]["created_at_ms"], 1760000000260u64);
        assert_eq!(json["context_summary"]["text"], "Goal: Ship host commands");
        assert_eq!(json["context_summary"]["created_at_ms"], 1760000000300u64);
        assert_eq!(json["context_summary"]["source_events"], 4);
        assert_eq!(json["context_summary"]["source_turns"], 2);
        assert_eq!(json["active_sessions"], 1);
        assert_eq!(json["age_ms"], 42);
    }

    #[test]
    fn agent_session_output_serializa_chunk_base64() {
        let output = AgentSessionOutput {
            session_id: "sess-1".into(),
            chunk_base64: "SG9sYQ0K".into(),
            timestamp_ms: 1760000000001,
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["session_id"], "sess-1");
        assert_eq!(json["chunk_base64"], "SG9sYQ0K");
        assert_eq!(json["timestamp_ms"], 1760000000001u64);
    }

    #[test]
    fn agent_host_command_result_serializa_estado_snake_case() {
        let result = AgentHostCommandResult {
            command: "status".into(),
            status: AgentHostCommandStatus::Unavailable,
            message: "pending backend".into(),
            session_id: Some("sess-child".into()),
            repo: Some(PathBuf::from("/r/api-fork")),
            agent_type: Some("codex".into()),
            review_summary: Some(AgentReviewSummary {
                branch: "main".into(),
                changed_files: 1,
                working_shortstat: Some("1 file changed, 2 insertions(+)".into()),
                staged_shortstat: None,
                files: vec![" M src/App.tsx".into()],
                truncated_count: 0,
            }),
            review_findings: Some(vec![AgentReviewFinding {
                severity: "high".into(),
                title: "Conflict marker present".into(),
                detail: "src/App.tsx still contains a merge conflict marker.".into(),
                path: Some(PathBuf::from("src/App.tsx")),
                line: Some(12),
            }]),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["command"], "status");
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["message"], "pending backend");
        assert_eq!(json["session_id"], "sess-child");
        assert_eq!(json["repo"], "/r/api-fork");
        assert_eq!(json["agent_type"], "codex");
        assert_eq!(json["review_summary"]["branch"], "main");
        assert_eq!(json["review_summary"]["changed_files"], 1);
        assert_eq!(json["review_summary"]["files"][0], " M src/App.tsx");
        assert_eq!(json["review_findings"][0]["severity"], "high");
        assert_eq!(json["review_findings"][0]["path"], "src/App.tsx");
        assert_eq!(json["review_findings"][0]["line"], 12);
    }

    #[test]
    fn agent_session_timeline_serializa_evento_nativo() {
        let item = AgentSessionTimelineItem {
            session_id: "sess-1".into(),
            id: "sess-1:1".into(),
            kind: AgentSessionTimelineKind::AgentMessage,
            text: "Hecho".into(),
            timestamp_ms: 1760000000001,
        };

        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["session_id"], "sess-1");
        assert_eq!(json["kind"], "agent_message");
        assert_eq!(json["text"], "Hecho");
    }
}
