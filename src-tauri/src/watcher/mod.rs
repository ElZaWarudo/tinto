//! Watcher de FS del workbench activo (diseño §7): observa los working
//! dirs con `notify`, normaliza y clasifica cada evento con el
//! `PathClassifier` (módulo `paths`), agrupa ráfagas con debounce por repo
//! y limita la frecuencia de lotes (throttling). Entrega mensajes
//! `Batch | RepoError` por un canal tokio que el bus de estado (RDM-006)
//! consumirá.
//!
//! Capas:
//! - `normalize`: evento crudo de notify → taxonomía `Created|Modified|Removed`.
//! - `debounce`: buffer + deadline + throttle por repo (testeable con reloj
//!   pausado de tokio; sin conocer el clasificador).
//! - `FsWatcher` (este archivo): montaje/remontaje por workbench,
//!   clasificación, rebuild coalescido del clasificador, errores por repo
//!   y apagado limpio.
//!
//! El wiring como managed state de Tauri y la llamada desde la conmutación
//! de workbench pertenecen a RDM-006; aquí los tests construyen y ejercen
//! el watcher directamente.

pub mod debounce;
pub mod normalize;

use std::path::PathBuf;

use thiserror::Error;

use crate::paths::{Classification, ClassifierError};
pub use normalize::EventType;

/// Evento ya clasificado que viaja dentro de un lote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedEvent {
    pub path: PathBuf,
    pub classification: Classification,
    pub kind: EventType,
    /// Epoch ms del momento en que el watcher recibió el evento.
    pub timestamp_ms: u64,
}

/// Mensaje del canal del watcher: un solo mecanismo de entrega para lotes
/// y errores por repo (sin store de estado vivo aparte — eso es del bus).
#[derive(Debug)]
pub enum WatcherMessage {
    /// Lote de eventos clasificados de un repo, emitido al cerrar su
    /// ventana de debounce o al vencer su throttle.
    Batch {
        repo: PathBuf,
        events: Vec<ClassifiedEvent>,
    },
    /// Fallo aislado de un repo; los demás repos siguen observándose.
    RepoError { repo: PathBuf, error: WatcherError },
    /// El backend del SO perdió eventos (p. ej. overflow de la cola de
    /// inotify bajo una ráfaga masiva). El consumidor debe recalcular el
    /// estado completo del repo: hubo cambios que nunca llegarán como
    /// eventos.
    RescanNeeded { repo: PathBuf },
}

/// Errores por repo del watcher (vía R3 del origin).
#[derive(Debug, Error)]
pub enum WatcherError {
    /// `watcher.watch()` falló al montar (path inexistente, permisos,
    /// límite de watches del SO).
    #[error("no se pudo montar el watch del repo {repo}: {message}")]
    MountFailed { repo: PathBuf, message: String },
    /// No se pudo construir el `PathClassifier` del repo.
    #[error("no se pudo construir el clasificador del repo {repo}")]
    ClassifierInit {
        repo: PathBuf,
        #[source]
        source: ClassifierError,
    },
    /// El root del repo desapareció del disco; notify remueve el watch en
    /// silencio, así que el watcher sintetiza este error al detectarlo.
    #[error("el root del repo {repo} fue removido del disco")]
    RepoRemoved { repo: PathBuf },
    /// El backend de notify no pudo inicializarse (p. ej. límite de
    /// instancias de inotify agotado en la máquina). Error ambiental, no
    /// de programación: el llamador decide degradar sin watching.
    #[error("no se pudo inicializar el backend de watching: {message}")]
    BackendInit { message: String },
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
