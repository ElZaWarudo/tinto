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

/// Guarda de tamaño para contenido de archivos/blobs.
pub const FILE_CONTENT_MAX_BYTES: usize = 1024 * 1024;
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
    pub path: PathBuf,
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
            subscribed_diffs: None,
        };
        let json = serde_json::to_value(&delta).unwrap();
        assert_eq!(json["repo"], "/r/a");
        assert_eq!(json["revision"], 7);
        assert_eq!(json["error"]["class"], "terminal");
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
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["kind"], "modified");
        assert_eq!(json["size_delta"], -2);
    }
}
