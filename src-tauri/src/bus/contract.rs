//! Tipos del contrato congelado backend↔frontend (ver
//! `docs/contracts/bus-contract.md`). Cambios additive-first: campos y
//! variantes nuevas sí; renames/removals no.

use std::path::PathBuf;

use serde::Serialize;

use crate::git::{BranchInfo, CommitInfo, FileDiff, RepoStatus};

/// Nombres de los eventos `emit` del contrato.
pub const EVENT_WORKBENCH_DELTA: &str = "tinto://workbench-delta";
pub const EVENT_FS_EVENTS: &str = "tinto://fs-events";
pub const EVENT_WATCHING_STATE: &str = "tinto://watching-state";
pub const EVENT_AGENT_SESSION_OUTPUT: &str = "tinto://agent-session-output";

/// Estado lifecycle de una sesion de agente gestionada por el backend.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Starting,
    Running,
    Exited,
    Error,
}

/// Error estructurado y seguro para comandos/lifecycle de sesiones de agente.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentSessionError {
    pub category: String,
    pub message: String,
}

/// Metadata publica de una sesion de agente. La E/S PTY se anade en ACI-002.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentSession {
    pub id: String,
    pub repo: PathBuf,
    pub agent_type: String,
    pub status: AgentSessionStatus,
    pub pid: Option<u32>,
    pub started_at_ms: u64,
    pub exit_code: Option<i32>,
    pub error: Option<AgentSessionError>,
}

/// Chunk binario del PTY de una sesion de agente, transportado en base64 para
/// preservar ANSI y bytes parciales.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentSessionOutput {
    pub session_id: String,
    pub chunk_base64: String,
    pub timestamp_ms: u64,
}

/// Guarda de tamaño para contenido de archivos/blobs.
pub const FILE_CONTENT_MAX_BYTES: usize = 1024 * 1024;
/// Guarda de tamaño para vistas multimedia embebidas (PDF/imágenes).
pub const MEDIA_CONTENT_MAX_BYTES: usize = 12 * 1024 * 1024;
/// Cap de entradas del árbol del repo.
pub const REPO_TREE_MAX_ENTRIES: usize = 20_000;
/// Cap del conjunto de suscripciones.
pub const MAX_SUBSCRIPTIONS: usize = 8;

/// Clase del error de un repo (contrato para los estados de UI).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepoErrorClass {
    /// GitError de recálculo: se limpia solo en el siguiente recálculo OK.
    Transient,
    /// Error del watcher (repo removido, fallo de montaje/clasificador):
    /// persiste hasta remount (`retry_repo`, conmutación o snapshot).
    Terminal,
}

/// Error de un repo, serializado con mensaje seguro.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoErrorState {
    pub class: RepoErrorClass,
    pub category: String,
    pub message: String,
}

/// Severidad factual de una señal pasiva. No implica juicio de calidad.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SignalSeverity {
    Info,
    Warning,
    Critical,
}

/// Tipo de señal pasiva detectada por reglas determinísticas.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PassiveSignalKind {
    SensitivePath,
    PossibleSecret,
    LargeDelete,
    ConfigChange,
    TestChange,
}

/// Señal pasiva: hecho detectado, sin valores secretos ni resumen interpretativo.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PassiveSignal {
    pub kind: PassiveSignalKind,
    pub severity: SignalSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub message: String,
}

/// Métricas livianas del estado actual del repo.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct RepoMetrics {
    pub changed_files: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
}

/// Delta de estado de un repo (evento `tinto://workbench-delta` y entrada
/// del snapshot).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<PassiveSignal>,
    /// Diffs de los objetivos suscritos de este repo; `None` sin suscripción.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribed_diffs: Option<Vec<FileDiff>>,
}

/// Tipo de evento FS del Plano 2 (taxonomía del watcher).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FsEventKind {
    Created,
    Modified,
    Removed,
}

/// Un evento del Plano 2 (archivo vigilado).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FsEventBatch {
    pub repo: PathBuf,
    pub events: Vec<FsEvent>,
}

/// Disponibilidad del watching (evento `tinto://watching-state`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WatchingState {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Objetivo de suscripción: un repo, opcionalmente un archivo.
#[derive(Debug, Clone, serde::Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct SubscriptionTarget {
    pub repo: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Entrada del árbol del repo (lista plana; el frontend arma el árbol).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub is_dir: bool,
}

/// Respuesta de `list_repo_tree`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoTree {
    pub entries: Vec<TreeEntry>,
    pub truncated: bool,
}

/// Contenido de un archivo/blob con guardas (1 MiB, binario → base64).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileContent {
    pub encoding: ContentEncoding,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ContentEncoding {
    Utf8,
    Base64,
}

/// Snapshot completo del workbench activo (`get_workbench_snapshot`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
            signals: vec![PassiveSignal {
                kind: PassiveSignalKind::SensitivePath,
                severity: SignalSeverity::Warning,
                path: Some(".env".into()),
                message: "Sensitive filename changed".into(),
            }],
            subscribed_diffs: None,
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["repo"], "/r/a");
        assert_eq!(json["revision"], 7);
        assert_eq!(json["error"]["class"], "terminal");
        assert_eq!(json["metrics"]["changed_files"], 1);
        assert_eq!(json["signals"][0]["kind"], "sensitive_path");
        assert_eq!(json["signals"][0]["severity"], "warning");
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
            status: AgentSessionStatus::Running,
            pid: Some(42),
            started_at_ms: 1760000000000,
            exit_code: None,
            error: Some(AgentSessionError {
                category: "spawn_failed".into(),
                message: "no se pudo iniciar la sesion".into(),
            }),
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
}
