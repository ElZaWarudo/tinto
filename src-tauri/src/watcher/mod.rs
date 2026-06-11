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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::paths::{Classification, ClassifierError, PathClassifier};
use crate::workbench::RepoEntry;
use debounce::{DebounceInput, EmittedBatch};
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

/// Entrada de la task router: eventos crudos de notify + control de repos.
#[derive(Debug)]
enum RouterInput {
    Raw(notify::Result<notify::Event>),
    AddRepo {
        root: PathBuf,
        fs_watch: Vec<String>,
        classifier: Box<PathClassifier>,
    },
    RemoveRepo {
        root: PathBuf,
    },
    ReportError {
        repo: PathBuf,
        error: WatcherError,
    },
}

struct MountedRepo {
    fs_watch: Vec<String>,
    classifier: Box<PathClassifier>,
}

/// Watcher de FS del workbench activo. Construirlo requiere contexto de
/// runtime tokio (`tokio::spawn`); en la app, RDM-006 lo crea dentro de
/// `tauri::async_runtime`. `new()` devuelve el handle y el receiver único
/// del canal de mensajes.
pub struct FsWatcher {
    watcher: RecommendedWatcher,
    router_tx: mpsc::UnboundedSender<RouterInput>,
    /// Roots montados con su `fs_watch`, para diff en `watch_workbench`.
    mounted: HashMap<PathBuf, Vec<String>>,
    /// Roots cuyo borrado en caliente detectó el router (`RepoRemoved`):
    /// el diff de `watch_workbench` los trata como NO montados para que el
    /// remount explícito funcione (el kernel ya removió sus watches).
    dead_roots: Arc<Mutex<HashSet<PathBuf>>>,
    router_handle: Option<JoinHandle<()>>,
}

impl FsWatcher {
    /// Crea el watcher: canal público, task router (clasificación +
    /// rebuild + reenvío) y task de debounce. Devuelve `BackendInit` si el
    /// backend de notify no puede inicializarse (error ambiental). Panics
    /// solo fuera de un runtime tokio (contrato del plan).
    #[allow(clippy::type_complexity)]
    pub fn new() -> Result<(Self, mpsc::UnboundedReceiver<WatcherMessage>), WatcherError> {
        let (public_tx, public_rx) = mpsc::unbounded_channel::<WatcherMessage>();
        let (router_tx, router_rx) = mpsc::unbounded_channel::<RouterInput>();
        let (debounce_tx, debounce_rx) = mpsc::unbounded_channel::<DebounceInput>();
        let (batch_tx, batch_rx) = mpsc::unbounded_channel::<EmittedBatch>();
        let dead_roots = Arc::new(Mutex::new(HashSet::new()));

        let raw_tx = router_tx.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            // Callback en el thread de notify: solo empuja al puente.
            let _ = raw_tx.send(RouterInput::Raw(res));
        })
        .map_err(|e| WatcherError::BackendInit {
            message: e.to_string(),
        })?;

        let debounce_handle = tokio::spawn(debounce::run(debounce_rx, batch_tx));
        let router_handle = tokio::spawn(router(
            router_rx,
            debounce_tx,
            batch_rx,
            public_tx,
            debounce_handle,
            Arc::clone(&dead_roots),
        ));

        Ok((
            Self {
                watcher,
                router_tx,
                mounted: HashMap::new(),
                dead_roots,
                router_handle: Some(router_handle),
            },
            public_rx,
        ))
    }

    /// Monta los repos del workbench dado y desmonta los que ya no están
    /// (diff por root). Fallos por repo → `RepoError` por el canal, sin
    /// abortar los demás (R3). Si un root ya montado cambia su `fs_watch`,
    /// se reconstruye su clasificador. Los roots cuyo borrado detectó el
    /// router se remontan completos si reaparecen.
    ///
    /// Los paths se canonicalizan al montar (el routing compara prefijos
    /// contra los paths que emite el backend del SO, que pueden venir con
    /// symlinks resueltos); la identidad del repo en los mensajes es el
    /// path canónico.
    pub fn watch_workbench(&mut self, repos: &[RepoEntry]) {
        let new_roots: HashMap<PathBuf, Vec<String>> = repos
            .iter()
            .map(|r| {
                // Si el path no existe, canonicalize falla y el watch()
                // posterior reporta MountFailed con el path original.
                let root = r.path.canonicalize().unwrap_or_else(|_| r.path.clone());
                (root, r.fs_watch.clone())
            })
            .collect();

        // Desmontar los que salen.
        let removed: Vec<PathBuf> = self
            .mounted
            .keys()
            .filter(|root| !new_roots.contains_key(*root))
            .cloned()
            .collect();
        for root in &removed {
            self.unmount(root);
        }

        // notify comparte registros de watch por path: desmontar un root
        // anidado/contenedor destruye los watches del subtree compartido.
        // Re-asegurar el watch de los roots retenidos que se solapan con
        // alguno removido.
        if !removed.is_empty() {
            let overlapping: Vec<PathBuf> = self
                .mounted
                .keys()
                .filter(|kept| {
                    removed
                        .iter()
                        .any(|gone| kept.starts_with(gone) || gone.starts_with(kept))
                })
                .cloned()
                .collect();
            for root in overlapping {
                let _ = self.watcher.watch(&root, RecursiveMode::Recursive);
            }
        }

        // Montar nuevos / remontar muertos / refrescar clasificadores.
        for (root, fs_watch) in new_roots {
            let was_removed_live = self
                .dead_roots
                .lock()
                .map(|mut dead| dead.remove(&root))
                .unwrap_or(false);

            match self.mounted.get(&root) {
                Some(current) if !was_removed_live && *current == fs_watch => continue,
                Some(_) if !was_removed_live => {
                    // Mismo root, watchlist distinta: solo rebuild.
                    self.send_classifier(&root, &fs_watch);
                    self.mounted.insert(root, fs_watch);
                }
                _ => {
                    // Montaje fresco (o remount tras RepoRemoved). El
                    // clasificador viaja ANTES de registrar el watch: el
                    // canal del router es FIFO, así el primer evento crudo
                    // ya encuentra su clasificador (sin ventana de pérdida
                    // durante el walk de construcción).
                    if !self.send_classifier(&root, &fs_watch) {
                        continue; // ya reportado por el canal
                    }
                    if let Err(e) = self.watcher.watch(&root, RecursiveMode::Recursive) {
                        // Un fallo parcial (p. ej. límite de inotify a
                        // mitad del walk) deja watches huérfanos que
                        // consumen presupuesto del kernel: liberarlos.
                        let _ = self.watcher.unwatch(&root);
                        let _ = self
                            .router_tx
                            .send(RouterInput::RemoveRepo { root: root.clone() });
                        let _ = self.router_tx.send(RouterInput::ReportError {
                            repo: root.clone(),
                            error: WatcherError::MountFailed {
                                repo: root.clone(),
                                message: e.to_string(),
                            },
                        });
                        self.mounted.remove(&root);
                        continue;
                    }
                    self.mounted.insert(root, fs_watch);
                }
            }
        }
    }

    /// Construye y envía el clasificador del repo al router. `false` si la
    /// construcción falló (ya reportado por el canal).
    fn send_classifier(&mut self, root: &PathBuf, fs_watch: &[String]) -> bool {
        match PathClassifier::new(root, fs_watch) {
            Ok(classifier) => {
                let _ = self.router_tx.send(RouterInput::AddRepo {
                    root: root.clone(),
                    fs_watch: fs_watch.to_vec(),
                    classifier: Box::new(classifier),
                });
                true
            }
            Err(source) => {
                let _ = self.router_tx.send(RouterInput::ReportError {
                    repo: root.clone(),
                    error: WatcherError::ClassifierInit {
                        repo: root.clone(),
                        source,
                    },
                });
                false
            }
        }
    }

    fn unmount(&mut self, root: &PathBuf) {
        // WatchNotFound es esperable (p. ej. root borrado: inotify ya
        // removió el watch); no es un segundo error.
        let _ = self.watcher.unwatch(root);
        self.mounted.remove(root);
        if let Ok(mut dead) = self.dead_roots.lock() {
            dead.remove(root);
        }
        let _ = self
            .router_tx
            .send(RouterInput::RemoveRepo { root: root.clone() });
    }

    /// Apaga el watcher: detiene notify, cierra el puente y espera a que
    /// router y debounce terminen (garantía verificable de R4). Los lotes
    /// en ventana abierta se ENTREGAN antes de cerrar el canal (flush
    /// final), no se descartan: por eso no se desmonta repo por repo (eso
    /// olvidaría sus buffers pendientes).
    pub async fn shutdown(mut self) {
        let router_handle = self.router_handle.take();
        // Drop cierra notify (sus threads) y el puente router_tx; el
        // router al ver el canal cerrado cierra el debounce, que hace su
        // flush final, y el router lo reenvía antes de terminar.
        drop(self); // Drop: aborta solo si quedara handle; aquí ya fue tomado.
        if let Some(handle) = router_handle {
            let _ = handle.await;
        }
    }
}

impl Drop for FsWatcher {
    fn drop(&mut self) {
        // Best-effort: shutdown() es la vía con garantías; Drop solo evita
        // tasks colgadas si el handle se descarta sin apagar.
        if let Some(handle) = self.router_handle.take() {
            handle.abort();
        }
    }
}

/// Task router: clasifica eventos crudos, detecta borrado del root y
/// `.gitignore`, alimenta el debounce, ejecuta el rebuild coalescido al
/// recibir cada lote y reenvía los lotes no vacíos al canal público.
async fn router(
    mut input: mpsc::UnboundedReceiver<RouterInput>,
    debounce_tx: mpsc::UnboundedSender<DebounceInput>,
    mut batches: mpsc::UnboundedReceiver<EmittedBatch>,
    public_tx: mpsc::UnboundedSender<WatcherMessage>,
    debounce_handle: JoinHandle<()>,
    dead_roots: Arc<Mutex<HashSet<PathBuf>>>,
) {
    let mut repos: HashMap<PathBuf, MountedRepo> = HashMap::new();

    loop {
        tokio::select! {
            maybe = input.recv() => {
                match maybe {
                    Some(RouterInput::Raw(Ok(event))) => {
                        route_event(&event, &mut repos, &debounce_tx, &public_tx, &dead_roots);
                    }
                    Some(RouterInput::Raw(Err(_))) => {
                        // Errores globales del backend de notify sin repo
                        // identificable: sin destino por ahora (los fallos
                        // por repo viajan por las otras vías).
                    }
                    Some(RouterInput::AddRepo { root, fs_watch, classifier }) => {
                        repos.insert(root, MountedRepo { fs_watch, classifier });
                    }
                    Some(RouterInput::RemoveRepo { root }) => {
                        repos.remove(&root);
                        let _ = debounce_tx.send(DebounceInput::ForgetRepo { repo: root });
                    }
                    Some(RouterInput::ReportError { repo, error }) => {
                        let _ = public_tx.send(WatcherMessage::RepoError { repo, error });
                    }
                    // Puente cerrado (shutdown): cerrar el debounce y
                    // drenar sus lotes finales.
                    None => break,
                }
            }
            Some(batch) = batches.recv() => {
                forward_batch(batch, &mut repos, &debounce_tx, &public_tx);
            }
        }
    }

    drop(debounce_tx);
    while let Some(batch) = batches.recv().await {
        // Flush final del shutdown: ya no hay debounce al que reintentar
        // un rebuild; solo reenviar los lotes pendientes.
        if !batch.events.is_empty() && repos.contains_key(&batch.repo) {
            let _ = public_tx.send(WatcherMessage::Batch {
                repo: batch.repo,
                events: batch.events,
            });
        }
    }
    let _ = debounce_handle.await;
}

/// Enruta un evento crudo: normaliza, detecta root borrado / `.gitignore`,
/// clasifica y alimenta el debounce.
fn route_event(
    event: &notify::Event,
    repos: &mut HashMap<PathBuf, MountedRepo>,
    debounce_tx: &mpsc::UnboundedSender<DebounceInput>,
    public_tx: &mpsc::UnboundedSender<WatcherMessage>,
    dead_roots: &Arc<Mutex<HashSet<PathBuf>>>,
) {
    // El kernel perdió eventos (p. ej. IN_Q_OVERFLOW bajo ráfaga masiva):
    // notify lo señala con el flag Rescan, sin paths. Avisar a TODOS los
    // repos montados que su estado puede estar desactualizado (R9: lo que
    // no se puede diferir, al menos se señala).
    if event.need_rescan() {
        for root in repos.keys() {
            let _ = public_tx.send(WatcherMessage::RescanNeeded { repo: root.clone() });
        }
        return;
    }

    let timestamp_ms = now_ms();
    for normalized in normalize::normalize(event) {
        // Repo dueño del path: el root montado más profundo que lo prefija.
        let Some(root) = repos
            .keys()
            .filter(|root| normalized.path.starts_with(root))
            .max_by_key(|root| root.as_os_str().len())
            .cloned()
        else {
            continue; // Evento fuera de los repos montados.
        };

        // Borrado del root: notify removió el watch en silencio; el
        // watcher sintetiza el error y desmonta su estado (R3/AE5). El
        // root queda marcado para que watch_workbench pueda remontarlo.
        if normalized.path == root && normalized.kind == EventType::Removed {
            repos.remove(&root);
            if let Ok(mut dead) = dead_roots.lock() {
                dead.insert(root.clone());
            }
            let _ = debounce_tx.send(DebounceInput::ForgetRepo { repo: root.clone() });
            let _ = public_tx.send(WatcherMessage::RepoError {
                repo: root.clone(),
                error: WatcherError::RepoRemoved { repo: root },
            });
            continue;
        }

        let repo = repos.get(&root).expect("recién resuelto");
        let classification = repo
            .classifier
            .classify(&normalized.path, normalized.is_dir);

        // `.gitignore` relevante tocado: rebuild coalescido (R6). Solo si
        // git lo consultaría (Plane1) — un `.gitignore` dentro de un dir
        // ignorado o de `.git/` no cambia las reglas y dispararía walks
        // inútiles (p. ej. node_modules trae muchos).
        if classification == Classification::Plane1
            && normalized
                .path
                .file_name()
                .is_some_and(|n| n == ".gitignore")
        {
            let _ = debounce_tx.send(DebounceInput::GitignoreTouched { repo: root.clone() });
        }

        match classification {
            Classification::GitInternal | Classification::Ignored | Classification::OutsideRepo => {
                continue
            }
            Classification::GitMeta | Classification::Plane1 | Classification::Plane2 => {
                let _ = debounce_tx.send(DebounceInput::Event {
                    repo: root,
                    event: ClassifiedEvent {
                        path: normalized.path,
                        classification,
                        kind: normalized.kind,
                        timestamp_ms,
                    },
                });
            }
        }
    }
}

/// Ejecuta el rebuild coalescido si el lote lo trae y reenvía el lote no
/// vacío al canal público. Los lotes de repos ya desmontados/removidos no
/// se reenvían (el consumidor ya recibió su `RepoError`/desmonte).
fn forward_batch(
    batch: EmittedBatch,
    repos: &mut HashMap<PathBuf, MountedRepo>,
    debounce_tx: &mpsc::UnboundedSender<DebounceInput>,
    public_tx: &mpsc::UnboundedSender<WatcherMessage>,
) {
    let Some(mounted) = repos.get_mut(&batch.repo) else {
        return; // Lote rezagado de un repo desmontado: descartar.
    };
    if batch.rebuild_pending {
        match PathClassifier::new(&batch.repo, &mounted.fs_watch) {
            Ok(classifier) => *mounted.classifier = classifier,
            Err(source) => {
                // Fallo transitorio (p. ej. root reemplazado atómicamente
                // a mitad de ventana): reportar y RE-ARMAR el rebuild para
                // la próxima ventana — sin retry, el clasificador viejo
                // quedaría silenciosamente autoritativo para siempre.
                let _ = public_tx.send(WatcherMessage::RepoError {
                    repo: batch.repo.clone(),
                    error: WatcherError::ClassifierInit {
                        repo: batch.repo.clone(),
                        source,
                    },
                });
                let _ = debounce_tx.send(DebounceInput::GitignoreTouched {
                    repo: batch.repo.clone(),
                });
            }
        }
    }
    if !batch.events.is_empty() {
        let _ = public_tx.send(WatcherMessage::Batch {
            repo: batch.repo,
            events: batch.events,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    /// Repo de fixture: dir con `.gitignore`, `.git/HEAD` y `src/`.
    fn fixture_repo(gitignore: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join(".gitignore"), gitignore).unwrap();
        dir
    }

    fn entry(dir: &TempDir, fs_watch: &[&str]) -> RepoEntry {
        RepoEntry {
            path: dir.path().canonicalize().unwrap(),
            alias: None,
            fs_watch: fs_watch.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Espera mensajes hasta que `pred` se cumpla o venza el timeout.
    async fn wait_for(
        rx: &mut mpsc::UnboundedReceiver<WatcherMessage>,
        mut pred: impl FnMut(&WatcherMessage) -> bool,
    ) -> Option<WatcherMessage> {
        timeout(Duration::from_secs(10), async {
            loop {
                let msg = rx.recv().await?;
                if pred(&msg) {
                    return Some(msg);
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    fn batch_has(msg: &WatcherMessage, classification: Classification, suffix: &str) -> bool {
        matches!(msg, WatcherMessage::Batch { events, .. } if events
            .iter()
            .any(|e| e.classification == classification && e.path.ends_with(suffix)))
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae2_clasifica_y_descarta_por_bucket() {
        // Plano 2 = gitignoreado Y en watchlist: .env debe estar en ambos.
        let repo = fixture_repo("target/\n.env\n");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[entry(&repo, &[".env"])]);
        tokio::time::sleep(Duration::from_millis(200)).await;

        let epoch_before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        fs::write(repo.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::create_dir_all(repo.path().join("target")).unwrap();
        fs::write(repo.path().join("target/x.o"), "obj").unwrap();
        fs::write(repo.path().join(".env"), "SECRET=1").unwrap();
        fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/dev\n").unwrap();

        let mut seen_plane1 = false;
        let mut seen_plane2 = false;
        let mut seen_gitmeta = false;
        let mut seen_ignored = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline
            && !(seen_plane1 && seen_plane2 && seen_gitmeta)
        {
            let Ok(Some(msg)) = timeout(Duration::from_secs(3), rx.recv()).await else {
                break;
            };
            if let WatcherMessage::Batch { events, .. } = &msg {
                for e in events {
                    match e.classification {
                        Classification::Plane1 if e.path.ends_with("src/main.rs") => {
                            seen_plane1 = true;
                        }
                        Classification::Plane2 if e.path.ends_with(".env") => {
                            seen_plane2 = true;
                        }
                        Classification::GitMeta => seen_gitmeta = true,
                        Classification::Ignored => seen_ignored = true,
                        _ => {}
                    }
                    assert!(
                        !e.path.ends_with("target/x.o"),
                        "lo ignorado no debe llegar en lotes"
                    );
                    assert!(
                        e.timestamp_ms >= epoch_before,
                        "timestamp_ms debe ser plausible (campo del contrato R5)"
                    );
                }
            }
        }
        assert!(seen_plane1, "falta Plane1 de src/main.rs");
        assert!(seen_plane2, "falta Plane2 de .env");
        assert!(seen_gitmeta, "falta GitMeta de .git/HEAD");
        assert!(!seen_ignored, "Ignored nunca viaja");

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae4_remount_conmuta_los_watches() {
        let a = fixture_repo("");
        let b = fixture_repo("");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();

        watcher.watch_workbench(&[entry(&a, &[])]);
        tokio::time::sleep(Duration::from_millis(200)).await;
        fs::write(a.path().join("src/a.rs"), "a").unwrap();
        assert!(
            wait_for(&mut rx, |m| batch_has(
                m,
                Classification::Plane1,
                "src/a.rs"
            ))
            .await
            .is_some(),
            "A montado debe emitir"
        );

        // Conmutar a B.
        watcher.watch_workbench(&[entry(&b, &[])]);
        tokio::time::sleep(Duration::from_millis(200)).await;
        while rx.try_recv().is_ok() {} // drenar restos de A

        fs::write(a.path().join("src/a2.rs"), "a2").unwrap();
        fs::write(b.path().join("src/b.rs"), "b").unwrap();

        // Aserción por contenido (no depende del drenado): debe llegar el
        // lote de B con su archivo, y ningún lote debe traer el de A.
        let msg = wait_for(&mut rx, |m| {
            batch_has(m, Classification::Plane1, "src/b.rs")
        })
        .await
        .expect("B debe emitir su lote");
        if let WatcherMessage::Batch { repo, events } = &msg {
            assert_eq!(repo, &b.path().canonicalize().unwrap());
            assert!(events.iter().all(|e| !e.path.ends_with("a2.rs")));
        }
        // Nada posterior debe traer eventos de A desmontado.
        while let Ok(m) = rx.try_recv() {
            if let WatcherMessage::Batch { events, .. } = &m {
                assert!(events.iter().all(|e| !e.path.ends_with("a2.rs")));
            }
        }

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae5_errores_aislados_por_repo() {
        // Montaje de path inexistente → MountFailed/ClassifierInit.
        let vivo = fixture_repo("");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        // Path fantasma derivado de un TempDir propio: nonexistencia
        // garantizada sin depender del namespace global de /tmp.
        let phantom_base = TempDir::new().unwrap();
        let phantom_path = phantom_base.path().join("no-existe");
        let fantasma = RepoEntry {
            path: phantom_path.clone(),
            alias: None,
            fs_watch: vec![],
        };
        watcher.watch_workbench(&[entry(&vivo, &[]), fantasma]);

        let err = wait_for(&mut rx, |m| matches!(m, WatcherMessage::RepoError { .. }))
            .await
            .expect("error de montaje del fantasma");
        if let WatcherMessage::RepoError { repo, .. } = &err {
            assert!(repo.ends_with("no-existe"));
        }

        // El repo vivo sigue operando.
        tokio::time::sleep(Duration::from_millis(200)).await;
        fs::write(vivo.path().join("src/ok.rs"), "ok").unwrap();
        assert!(
            wait_for(&mut rx, |m| batch_has(
                m,
                Classification::Plane1,
                "src/ok.rs"
            ))
            .await
            .is_some(),
            "el repo vivo no debe verse afectado"
        );

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae5_borrado_del_root_sintetiza_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".gitignore"), "").unwrap();
        let canonical = root.canonicalize().unwrap();

        let otro = fixture_repo("");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[
            RepoEntry {
                path: canonical.clone(),
                alias: None,
                fs_watch: vec![],
            },
            entry(&otro, &[]),
        ]);
        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::remove_dir_all(&root).unwrap();

        let err = wait_for(&mut rx, |m| {
            matches!(
                m,
                WatcherMessage::RepoError {
                    error: WatcherError::RepoRemoved { .. },
                    ..
                }
            )
        })
        .await
        .expect("RepoRemoved sintetizado");
        if let WatcherMessage::RepoError { repo, .. } = &err {
            assert_eq!(repo, &canonical);
        }

        // El otro repo sigue vivo.
        fs::write(otro.path().join("src/vivo.rs"), "v").unwrap();
        assert!(wait_for(&mut rx, |m| batch_has(
            m,
            Classification::Plane1,
            "src/vivo.rs"
        ))
        .await
        .is_some());

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae6_rebuild_ante_gitignore() {
        let repo = fixture_repo("");
        fs::create_dir_all(repo.path().join("logs")).unwrap();
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[entry(&repo, &[])]);
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Sin regla: logs/ es Plane1.
        fs::write(repo.path().join("logs/antes.log"), "x").unwrap();
        assert!(wait_for(&mut rx, |m| batch_has(
            m,
            Classification::Plane1,
            "logs/antes.log"
        ))
        .await
        .is_some());

        // Regla nueva → rebuild (el lote del .gitignore confirma la ventana).
        fs::write(repo.path().join(".gitignore"), "logs/\n").unwrap();
        assert!(
            wait_for(&mut rx, |m| batch_has(
                m,
                Classification::Plane1,
                ".gitignore"
            ))
            .await
            .is_some(),
            "el cambio de .gitignore viaja como Plane1 y ejecuta el rebuild"
        );

        // Tras el rebuild, logs/ queda ignorado: no debe llegar.
        fs::write(repo.path().join("logs/despues.log"), "y").unwrap();
        fs::write(repo.path().join("src/control.rs"), "c").unwrap();
        let msg = wait_for(&mut rx, |m| {
            batch_has(m, Classification::Plane1, "src/control.rs")
        })
        .await
        .expect("el evento de control debe llegar");
        if let WatcherMessage::Batch { events, .. } = &msg {
            assert!(
                events.iter().all(|e| !e.path.ends_with("despues.log")),
                "logs/ ya está ignorado tras el rebuild"
            );
        }

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_entrega_los_lotes_en_ventana_abierta() {
        let repo = fixture_repo("");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[entry(&repo, &[])]);
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Escribir y apagar ANTES de que cierre la ventana de debounce:
        // el flush final debe entregar el lote, no descartarlo.
        fs::write(repo.path().join("src/pendiente.rs"), "p").unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await; // evento llega al puente
        watcher.shutdown().await;

        let mut saw_pending = false;
        while let Some(msg) = rx.recv().await {
            if batch_has(&msg, Classification::Plane1, "src/pendiente.rs") {
                saw_pending = true;
            }
        }
        assert!(
            saw_pending,
            "el flush de shutdown debe entregar la ventana abierta"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cambio_de_fs_watch_reconstruye_el_clasificador_sin_remontar() {
        // .env gitignoreado; sin watchlist primero → Ignored (no llega).
        let repo = fixture_repo(".env\n");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[entry(&repo, &[])]);
        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::write(repo.path().join(".env"), "v1").unwrap();
        fs::write(repo.path().join("src/control1.rs"), "c1").unwrap();
        let msg = wait_for(&mut rx, |m| {
            batch_has(m, Classification::Plane1, "src/control1.rs")
        })
        .await
        .expect("control1 debe llegar");
        if let WatcherMessage::Batch { events, .. } = &msg {
            assert!(events.iter().all(|e| !e.path.ends_with(".env")));
        }

        // Mismo root, watchlist nueva: rebuild sin re-watch.
        watcher.watch_workbench(&[entry(&repo, &[".env"])]);
        tokio::time::sleep(Duration::from_millis(200)).await;
        fs::write(repo.path().join(".env"), "v2").unwrap();
        assert!(
            wait_for(&mut rx, |m| batch_has(m, Classification::Plane2, ".env"))
                .await
                .is_some(),
            "tras el cambio de fs_watch, .env debe llegar como Plane2"
        );

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn remount_tras_repo_removed_revive_el_repo() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".gitignore"), "").unwrap();
        let canonical = root.canonicalize().unwrap();
        let entry = RepoEntry {
            path: canonical.clone(),
            alias: None,
            fs_watch: vec![],
        };

        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(std::slice::from_ref(&entry));
        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::remove_dir_all(&root).unwrap();
        wait_for(&mut rx, |m| {
            matches!(
                m,
                WatcherMessage::RepoError {
                    error: WatcherError::RepoRemoved { .. },
                    ..
                }
            )
        })
        .await
        .expect("RepoRemoved");

        // El repo reaparece (mismo path) y el remount explícito lo revive.
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".gitignore"), "").unwrap();
        watcher.watch_workbench(std::slice::from_ref(&entry));
        tokio::time::sleep(Duration::from_millis(200)).await;

        fs::write(root.join("src/revivido.rs"), "r").unwrap();
        assert!(
            wait_for(&mut rx, |m| batch_has(
                m,
                Classification::Plane1,
                "src/revivido.rs"
            ))
            .await
            .is_some(),
            "tras recrear el root, watch_workbench debe remontarlo de verdad"
        );

        watcher.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rescan_del_kernel_avisa_a_todos_los_repos() {
        // Unit-level: route_event con un evento sintético con flag Rescan
        // (overflow de la cola del kernel) debe emitir RescanNeeded por
        // cada repo montado.
        let repo_dir = fixture_repo("");
        let root = repo_dir.path().canonicalize().unwrap();
        let classifier = PathClassifier::new(&root, &[]).unwrap();
        let mut repos = HashMap::new();
        repos.insert(
            root.clone(),
            MountedRepo {
                fs_watch: vec![],
                classifier: Box::new(classifier),
            },
        );
        let (debounce_tx, _debounce_rx) = mpsc::unbounded_channel();
        let (public_tx, mut public_rx) = mpsc::unbounded_channel();
        let dead = Arc::new(Mutex::new(HashSet::new()));

        let overflow =
            notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
        route_event(&overflow, &mut repos, &debounce_tx, &public_tx, &dead);

        match public_rx.try_recv() {
            Ok(WatcherMessage::RescanNeeded { repo }) => assert_eq!(repo, root),
            other => panic!("esperaba RescanNeeded, llegó {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_cierra_el_canal_sin_mensajes_pendientes() {
        let repo = fixture_repo("");
        let (mut watcher, mut rx) = FsWatcher::new().unwrap();
        watcher.watch_workbench(&[entry(&repo, &[])]);
        tokio::time::sleep(Duration::from_millis(100)).await;

        watcher.shutdown().await;

        // Drenar lo emitido antes del cierre; el canal debe terminar en None.
        let closed = timeout(Duration::from_secs(5), async {
            while rx.recv().await.is_some() {}
        })
        .await
        .is_ok();
        assert!(closed, "tras shutdown el canal debe cerrarse");
    }
}
