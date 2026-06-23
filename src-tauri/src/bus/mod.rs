//! State/Event bus (diseño §6/§7): el punto de integración. Posee el
//! `FsWatcher`, consume sus mensajes, recalcula git solo del repo afectado
//! (vía `spawn_blocking` con concurrencia acotada), mantiene el estado en
//! vivo del workbench activo y emite los eventos del contrato congelado
//! (`docs/contracts/bus-contract.md`) hacia el frontend.
//!
//! Arquitectura: una task propietaria (`run_bus`) + un `BusHandle` clonable
//! (canal de comandos creado síncronamente en `setup`, sin carrera con los
//! `invoke` tempranos). Las lecturas pesadas bajo demanda viven en
//! `commands.rs` y NO pasan por la task. La emisión real se inyecta como
//! `DeltaSink` para testear sin `AppHandle`.

pub mod commands;
pub mod contract;
mod secret_scan;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{mpsc, oneshot, Semaphore};

use crate::git::{DiffLineKind, FileDiff, Git2Engine, GitEngine, GitError};
use crate::paths::Classification;
use crate::watcher::{ClassifiedEvent, EventType, FsWatcher, WatcherError, WatcherMessage};
use crate::workbench::{RepoEntry, RepoSource};
use contract::{
    FsEvent, FsEventBatch, FsEventKind, PassiveSignal, PassiveSignalKind, RepoDelta,
    RepoErrorClass, RepoErrorState, RepoMetrics, SecretFinding, SignalSeverity, SubscriptionTarget,
    WatchingState, WorkbenchSnapshot, EVENT_FS_EVENTS, EVENT_WATCHING_STATE, EVENT_WORKBENCH_DELTA,
    MAX_SUBSCRIPTIONS,
};

/// Concurrencia máxima de recálculos git en vuelo (acota el broadcast de
/// `RescanNeeded` sin matar el principio liviano).
const RECALC_CONCURRENCY: usize = 2;
const MAX_SIGNALS_PER_REPO: usize = 12;
const LARGE_DELETE_LINES: usize = 100;

/// Emisión inyectada (en la app: `AppHandle::emit`; en tests: un canal).
/// No llamarlo `Emitter`: colisiona con `tauri::Emitter`.
pub type DeltaSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

fn emit<T: Serialize>(sink: &DeltaSink, event: &str, payload: &T) {
    if let Ok(value) = serde_json::to_value(payload) {
        sink(event, value);
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ===========================================================================
// U2 — Estado en vivo puro
// ===========================================================================

/// Qué recalcular ante un lote del watcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RecalcScope {
    /// Solo `status` (cambios Plane1 ordinarios).
    StatusOnly,
    /// status + branch + head (GitMeta, `.gitignore`, RescanNeeded, snapshot).
    Everything,
}

/// Resultado de un recálculo git (producido en `spawn_blocking`).
#[derive(Debug)]
pub(crate) struct RecalcOutcome {
    pub status: crate::git::RepoStatus,
    /// `None` cuando el scope fue StatusOnly (se conservan los previos).
    pub branch: Option<crate::git::BranchInfo>,
    pub head: Option<Option<crate::git::CommitInfo>>,
    pub subscribed_diffs: Option<Vec<crate::git::FileDiff>>,
    pub metrics: RepoMetrics,
    pub gitleaks_configured: bool,
    pub signals: Vec<PassiveSignal>,
    pub secret_findings: Vec<SecretFinding>,
}

/// Estado en vivo de un repo.
#[derive(Debug, Default)]
pub(crate) struct RepoLiveState {
    status: crate::git::RepoStatus,
    branch: Option<crate::git::BranchInfo>,
    head: Option<crate::git::CommitInfo>,
    last_activity_ms: u64,
    error: Option<RepoErrorState>,
    revision: u64,
    metrics: RepoMetrics,
    gitleaks_configured: bool,
    signals: Vec<PassiveSignal>,
    secret_findings: Vec<SecretFinding>,
    /// Último tamaño conocido por path vigilado (delta de tamaño, Plano 2).
    last_known_sizes: HashMap<PathBuf, u64>,
}

impl RepoLiveState {
    fn delta(&self, repo: &Path) -> RepoDelta {
        RepoDelta {
            repo: repo.to_path_buf(),
            revision: self.revision,
            status: self.status.clone(),
            branch: self.branch.clone(),
            head: self.head.clone(),
            last_activity_ms: self.last_activity_ms,
            error: self.error.clone(),
            metrics: self.metrics.clone(),
            gitleaks_configured: self.gitleaks_configured,
            signals: self.signals.clone(),
            secret_findings: self.secret_findings.clone(),
            subscribed_diffs: None,
        }
    }

    /// Aplica un recálculo OK: sube la revisión y limpia errores transitorios.
    pub(crate) fn apply_recalc(&mut self, outcome: &RecalcOutcome) {
        self.status = outcome.status.clone();
        if let Some(branch) = &outcome.branch {
            self.branch = Some(branch.clone());
        }
        if let Some(head) = &outcome.head {
            self.head = head.clone();
        }
        // Un recálculo OK demuestra que el repo es legible: limpia cualquier
        // error, incluido el terminal (un repo-removed recreado revive vía
        // retry/remount, que dispara este recálculo). El estado de watching
        // degradado se reporta aparte por `tinto://watching-state`.
        self.error = None;
        self.metrics = outcome.metrics.clone();
        self.gitleaks_configured = outcome.gitleaks_configured;
        self.signals = outcome.signals.clone();
        self.secret_findings = outcome.secret_findings.clone();
        self.revision += 1;
    }

    /// Aplica un error (sube revisión para que el consumidor lo aplique).
    pub(crate) fn apply_error(&mut self, error: RepoErrorState) {
        self.error = Some(error);
        self.revision += 1;
    }

    /// Construye los FsEvents del Plano 2 con tamaño/delta (stat best-effort).
    pub(crate) fn fs_events(&mut self, repo: &Path, events: &[ClassifiedEvent]) -> Vec<FsEvent> {
        events
            .iter()
            .filter(|e| e.classification == Classification::Plane2)
            .map(|e| {
                // El watcher entrega paths absolutos; el contrato exige el path
                // relativo al repo (los demás campos del contrato también son
                // relativos). `abs` es solo para `stat`; `rel` es la identidad
                // emitida y la clave de tamaños conocidos.
                let (abs, rel): (PathBuf, PathBuf) = if e.path.is_absolute() {
                    let rel = e
                        .path
                        .strip_prefix(repo)
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|_| e.path.clone());
                    (e.path.clone(), rel)
                } else {
                    (repo.join(&e.path), e.path.clone())
                };
                let size = std::fs::metadata(&abs).ok().map(|m| m.len());
                let prev = self.last_known_sizes.get(&rel).copied();
                let size_delta = match (size, prev) {
                    (Some(new), Some(old)) => Some(new as i64 - old as i64),
                    _ => None,
                };
                match size {
                    Some(s) => {
                        self.last_known_sizes.insert(rel.clone(), s);
                    }
                    None => {
                        self.last_known_sizes.remove(&rel);
                    }
                }
                let signals = signals_for_path(&rel, SignalSurface::Plane2);
                FsEvent {
                    path: rel,
                    kind: match e.kind {
                        EventType::Created => FsEventKind::Created,
                        EventType::Modified => FsEventKind::Modified,
                        EventType::Removed => FsEventKind::Removed,
                    },
                    timestamp_ms: e.timestamp_ms,
                    size,
                    size_delta,
                    signals,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
enum SignalSurface {
    Repo,
    Plane2,
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn lower_path(path: &Path) -> String {
    path_text(path).to_ascii_lowercase()
}

fn base_name_lower(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_sensitive_path(path: &Path) -> bool {
    let lower = lower_path(path);
    let base = base_name_lower(path);
    base == ".env"
        || base.starts_with(".env.")
        || base.ends_with(".pem")
        || base.ends_with(".key")
        || base.ends_with(".p12")
        || base.ends_with(".pfx")
        || base == "id_rsa"
        || lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("token")
}

fn is_config_path(path: &Path) -> bool {
    let lower = lower_path(path);
    matches!(
        base_name_lower(path).as_str(),
        "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "cargo.toml"
            | "cargo.lock"
            | "dockerfile"
            | "docker-compose.yml"
            | "docker-compose.yaml"
            | "tsconfig.json"
            | "vite.config.ts"
            | "eslint.config.js"
    ) || lower.starts_with(".github/workflows/")
        || lower.ends_with(".config.js")
        || lower.ends_with(".config.ts")
        || lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
}

fn is_test_path(path: &Path) -> bool {
    let lower = lower_path(path);
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("__tests__")
        || lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.tsx")
        || lower.ends_with("_test.rs")
}

fn signal(
    kind: PassiveSignalKind,
    severity: SignalSeverity,
    path: Option<PathBuf>,
    message: impl Into<String>,
) -> PassiveSignal {
    PassiveSignal {
        kind,
        severity,
        path,
        message: message.into(),
    }
}

fn push_signal(
    signals: &mut Vec<PassiveSignal>,
    seen: &mut HashSet<(PassiveSignalKind, Option<PathBuf>)>,
    signal: PassiveSignal,
) {
    let key = (signal.kind, signal.path.clone());
    if seen.insert(key) {
        signals.push(signal);
    }
}

fn signals_for_path(path: &Path, surface: SignalSurface) -> Vec<PassiveSignal> {
    let mut signals = Vec::new();
    let mut seen = HashSet::new();
    if is_sensitive_path(path) {
        push_signal(
            &mut signals,
            &mut seen,
            signal(
                PassiveSignalKind::SensitivePath,
                SignalSeverity::Warning,
                Some(path.to_path_buf()),
                match surface {
                    SignalSurface::Repo => "Sensitive filename changed",
                    SignalSurface::Plane2 => "Sensitive watched file changed",
                },
            ),
        );
    }
    if is_config_path(path) {
        push_signal(
            &mut signals,
            &mut seen,
            signal(
                PassiveSignalKind::ConfigChange,
                SignalSeverity::Warning,
                Some(path.to_path_buf()),
                "Configuration file changed",
            ),
        );
    }
    if is_test_path(path) {
        push_signal(
            &mut signals,
            &mut seen,
            signal(
                PassiveSignalKind::TestChange,
                SignalSeverity::Info,
                Some(path.to_path_buf()),
                "Test file changed",
            ),
        );
    }
    signals
}

fn kind_rank(kind: PassiveSignalKind) -> u8 {
    match kind {
        PassiveSignalKind::PossibleSecret => 0,
        PassiveSignalKind::LargeDelete => 1,
        PassiveSignalKind::SensitivePath => 2,
        PassiveSignalKind::ConfigChange => 3,
        PassiveSignalKind::TestChange => 4,
    }
}

fn signal_rank(signal: &PassiveSignal) -> (u8, u8, String) {
    let severity = match signal.severity {
        SignalSeverity::Critical => 0,
        SignalSeverity::Warning => 1,
        SignalSeverity::Info => 2,
    };
    (
        severity,
        kind_rank(signal.kind),
        signal
            .path
            .as_deref()
            .map(path_text)
            .unwrap_or_else(|| "~".into()),
    )
}

fn bounded_sorted(mut signals: Vec<PassiveSignal>) -> Vec<PassiveSignal> {
    signals.sort_by_key(signal_rank);
    signals.truncate(MAX_SIGNALS_PER_REPO);
    signals
}

fn metrics_and_signals(
    status: &crate::git::RepoStatus,
    diffs: &[FileDiff],
    secret_findings: &[SecretFinding],
) -> (RepoMetrics, Vec<PassiveSignal>) {
    let mut changed_paths = BTreeSet::new();
    for path in status
        .modified
        .iter()
        .chain(status.staged.iter())
        .chain(status.untracked.iter())
    {
        changed_paths.insert(path.clone());
    }
    for diff in diffs {
        changed_paths.insert(diff.path.clone());
        if let Some(old) = &diff.old_path {
            changed_paths.insert(old.clone());
        }
    }

    let mut signals = Vec::new();
    let mut seen = HashSet::new();
    for path in &changed_paths {
        for s in signals_for_path(path, SignalSurface::Repo) {
            push_signal(&mut signals, &mut seen, s);
        }
    }

    let mut lines_added = 0;
    let mut lines_removed = 0;
    for diff in diffs {
        let mut removed_for_file = 0;
        for line in diff.hunks.iter().flat_map(|h| h.lines.iter()) {
            match line.kind {
                DiffLineKind::Added => lines_added += 1,
                DiffLineKind::Removed => {
                    lines_removed += 1;
                    removed_for_file += 1;
                }
                DiffLineKind::Context => {}
            }
        }
        if removed_for_file >= LARGE_DELETE_LINES {
            push_signal(
                &mut signals,
                &mut seen,
                signal(
                    PassiveSignalKind::LargeDelete,
                    SignalSeverity::Warning,
                    Some(diff.path.clone()),
                    format!("{removed_for_file} removed lines"),
                ),
            );
        }
    }

    for finding in secret_findings {
        push_signal(
            &mut signals,
            &mut seen,
            signal(
                PassiveSignalKind::PossibleSecret,
                SignalSeverity::Critical,
                Some(finding.path.clone()),
                "Possible secret detected",
            ),
        );
    }

    (
        RepoMetrics {
            changed_files: changed_paths.len(),
            lines_added,
            lines_removed,
        },
        bounded_sorted(signals),
    )
}

/// Decide el scope de recálculo git de un lote (None = solo Plano 2).
pub(crate) fn recalc_scope(events: &[ClassifiedEvent]) -> Option<RecalcScope> {
    let mut scope: Option<RecalcScope> = None;
    for e in events {
        match e.classification {
            Classification::GitMeta => return Some(RecalcScope::Everything),
            Classification::Plane1 => {
                if e.path.file_name().is_some_and(|n| n == ".gitignore") {
                    return Some(RecalcScope::Everything);
                }
                scope = Some(scope.unwrap_or(RecalcScope::StatusOnly));
            }
            _ => {}
        }
    }
    scope
}

fn watcher_error_state(error: &WatcherError) -> RepoErrorState {
    let category = match error {
        WatcherError::MountFailed { .. } => "mount-failed",
        WatcherError::ClassifierInit { .. } => "classifier-init",
        WatcherError::RepoRemoved { .. } => "repo-removed",
        WatcherError::BackendInit { .. } => "backend-init",
    };
    RepoErrorState {
        class: RepoErrorClass::Terminal,
        category: category.into(),
        message: error.to_string(),
    }
}

fn git_error_state(error: &GitError) -> RepoErrorState {
    RepoErrorState {
        class: RepoErrorClass::Transient,
        category: error.category().into(),
        message: error.to_string(),
    }
}

// ===========================================================================
// U3 — Task del bus
// ===========================================================================

/// Comandos hacia la task del bus.
#[derive(Debug)]
pub enum BusCommand {
    SetWorkbench(Vec<RepoEntry>),
    GetSnapshot(oneshot::Sender<WorkbenchSnapshot>),
    Subscribe(Vec<SubscriptionTarget>, oneshot::Sender<()>),
    RetryRepo(PathBuf, oneshot::Sender<()>),
    ResolveRepo(PathBuf, oneshot::Sender<Result<PathBuf, RepoResolveError>>),
    /// ¿El path canónico pertenece al workbench activo? Allowlist que acota
    /// las lecturas bajo demanda al conjunto de repos montado.
    IsKnownRepo(PathBuf, oneshot::Sender<bool>),
    Shutdown(oneshot::Sender<()>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoResolveError {
    UnsupportedRepoSource { source: RepoSource },
    RepositoryNotFound,
    RepoNotAllowed,
    BusUnavailable,
}

/// Handle clonable hacia el bus; vive como managed state de Tauri.
#[derive(Clone)]
pub struct BusHandle {
    tx: mpsc::UnboundedSender<BusCommand>,
}

impl BusHandle {
    /// Crea el par handle/receiver. El receiver se entrega a `run_bus`.
    pub fn new_pair() -> (Self, mpsc::UnboundedReceiver<BusCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn set_workbench(&self, repos: Vec<RepoEntry>) {
        let _ = self.tx.send(BusCommand::SetWorkbench(repos));
    }

    pub async fn snapshot(&self) -> Option<WorkbenchSnapshot> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(BusCommand::GetSnapshot(reply)).ok()?;
        rx.await.ok()
    }

    pub async fn subscribe(&self, targets: Vec<SubscriptionTarget>) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(BusCommand::Subscribe(targets, reply)).is_err() {
            return false;
        }
        rx.await.is_ok()
    }

    pub async fn retry_repo(&self, repo: PathBuf) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(BusCommand::RetryRepo(repo, reply)).is_err() {
            return false;
        }
        rx.await.is_ok()
    }

    pub async fn resolve_repo(&self, repo: PathBuf) -> Result<PathBuf, RepoResolveError> {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(BusCommand::ResolveRepo(repo, reply)).is_err() {
            return Err(RepoResolveError::BusUnavailable);
        }
        rx.await.unwrap_or(Err(RepoResolveError::BusUnavailable))
    }

    /// `true` si `repo` (path canónico) está en el workbench activo. Si el bus
    /// no responde, devuelve `false` (fail-closed).
    pub async fn is_known(&self, repo: PathBuf) -> bool {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(BusCommand::IsKnownRepo(repo, reply)).is_err() {
            return false;
        }
        rx.await.unwrap_or(false)
    }

    pub async fn shutdown(&self) {
        let (ack, rx) = oneshot::channel();
        if self.tx.send(BusCommand::Shutdown(ack)).is_ok() {
            let _ = rx.await;
        }
    }
}

/// Resultado de un recálculo en vuelo, de vuelta a la task.
struct RecalcResult {
    repo: PathBuf,
    outcome: Result<RecalcOutcome, GitError>,
}

/// Corre la task del bus. `initial` es el workbench activo al arrancar
/// (vacío si no hay). Si el backend de watching no puede inicializarse, el
/// bus arranca **degradado** (sin deltas en vivo; lecturas bajo demanda
/// siguen funcionando) y lo señala por `tinto://watching-state`.
pub async fn run_bus(
    commands: mpsc::UnboundedReceiver<BusCommand>,
    sink: DeltaSink,
    initial: Vec<RepoEntry>,
) {
    run_bus_inner(commands, sink, initial, FsWatcher::new()).await
}

/// Núcleo de la task con el resultado de inicialización del watcher
/// inyectado, para poder testear el arranque degradado (AE11) sin un backend
/// de notify real.
type WatcherInit = Result<(FsWatcher, mpsc::UnboundedReceiver<WatcherMessage>), WatcherError>;

async fn run_bus_inner(
    mut commands: mpsc::UnboundedReceiver<BusCommand>,
    sink: DeltaSink,
    initial: Vec<RepoEntry>,
    watcher_init: WatcherInit,
) {
    // Watcher propio (o degradado).
    let (mut watcher, mut watcher_rx) = match watcher_init {
        Ok((w, rx)) => {
            emit(
                &sink,
                EVENT_WATCHING_STATE,
                &WatchingState {
                    available: true,
                    reason: None,
                },
            );
            (Some(w), Some(rx))
        }
        Err(e) => {
            emit(
                &sink,
                EVENT_WATCHING_STATE,
                &WatchingState {
                    available: false,
                    reason: Some(e.to_string()),
                },
            );
            (None, None)
        }
    };

    let mut states: HashMap<PathBuf, RepoLiveState> = HashMap::new();
    let mut current_entries: Vec<RepoEntry> = Vec::new();
    let mut unsupported_entries: Vec<RepoEntry> = Vec::new();
    let mut subscriptions: Vec<SubscriptionTarget> = Vec::new();
    // Revisión durable por path canónico: sobrevive al desmontaje para que un
    // repo re-añadido continúe su contador (el contrato exige revisión
    // monotónica por repo; reiniciar a 0 haría que el frontend descarte el
    // nuevo estado).
    let mut revisions: HashMap<PathBuf, u64> = HashMap::new();

    let semaphore = Arc::new(Semaphore::new(RECALC_CONCURRENCY));
    let (results_tx, mut results_rx) = mpsc::unbounded_channel::<RecalcResult>();
    let mut inflight: HashSet<PathBuf> = HashSet::new();
    let mut pending: HashMap<PathBuf, RecalcScope> = HashMap::new();

    // Montaje inicial.
    set_workbench(
        initial,
        &mut watcher,
        &mut states,
        &mut current_entries,
        &mut unsupported_entries,
        &mut revisions,
        &subscriptions,
        &semaphore,
        &results_tx,
        &mut inflight,
        &mut pending,
    );

    loop {
        tokio::select! {
            maybe = commands.recv() => {
                match maybe {
                    Some(BusCommand::SetWorkbench(repos)) => {
                        set_workbench(repos, &mut watcher, &mut states, &mut current_entries,
                            &mut unsupported_entries, &mut revisions, &subscriptions, &semaphore, &results_tx,
                            &mut inflight, &mut pending);
                    }
                    Some(BusCommand::GetSnapshot(reply)) => {
                        // Remount de repos en error terminal: re-vigila Y dispara
                        // un recálculo completo, para que un repo recreado se
                        // limpie (re-vigilar solo no bastaba — el delta sano
                        // llega luego, no en este snapshot).
                        let terminal: Vec<PathBuf> = states
                            .iter()
                            .filter(|(_, s)| matches!(s.error, Some(RepoErrorState {
                                class: RepoErrorClass::Terminal, .. })))
                            .map(|(repo, _)| repo.clone())
                            .collect();
                        if !terminal.is_empty() {
                            if let Some(w) = watcher.as_mut() {
                                w.watch_workbench(&current_entries);
                            }
                            for repo in terminal {
                                trigger_recalc(repo, RecalcScope::Everything, &subscriptions,
                                    &semaphore, &results_tx, &mut inflight, &mut pending);
                            }
                        }
                        let snapshot = WorkbenchSnapshot {
                            watching: WatchingState {
                                available: watcher.is_some(),
                                reason: None,
                            },
                            repos: states.iter().map(|(repo, s)| s.delta(repo)).collect(),
                        };
                        let _ = reply.send(snapshot);
                    }
                    Some(BusCommand::Subscribe(mut targets, reply)) => {
                        targets.truncate(MAX_SUBSCRIPTIONS);
                        let targets: Vec<SubscriptionTarget> = targets
                            .into_iter()
                            .filter_map(|mut target| {
                                let repo = resolve_repo_for_command(
                                    target.repo,
                                    &current_entries,
                                    &unsupported_entries,
                                )
                                .ok()?;
                                target.repo = repo;
                                Some(target)
                            })
                            .collect();
                        let affected: HashSet<PathBuf> =
                            targets.iter().map(|t| t.repo.clone()).collect();
                        subscriptions = targets;
                        for repo in affected {
                            if states.contains_key(&repo) {
                                trigger_recalc(repo, RecalcScope::Everything, &subscriptions,
                                    &semaphore, &results_tx, &mut inflight, &mut pending);
                            }
                        }
                        let _ = reply.send(());
                    }
                    Some(BusCommand::RetryRepo(repo, reply)) => {
                        if let Ok(repo) =
                            resolve_repo_for_command(repo, &current_entries, &unsupported_entries)
                        {
                            if let Some(w) = watcher.as_mut() {
                                w.watch_workbench(&current_entries);
                            }
                            if states.contains_key(&repo) {
                                trigger_recalc(repo, RecalcScope::Everything, &subscriptions,
                                    &semaphore, &results_tx, &mut inflight, &mut pending);
                            }
                        }
                        let _ = reply.send(());
                    }
                    Some(BusCommand::ResolveRepo(repo, reply)) => {
                        let resolved = resolve_repo_for_command(
                            repo,
                            &current_entries,
                            &unsupported_entries,
                        );
                        let _ = reply.send(resolved);
                    }
                    Some(BusCommand::IsKnownRepo(repo, reply)) => {
                        let known = resolve_repo_for_command(
                            repo,
                            &current_entries,
                            &unsupported_entries,
                        )
                        .is_ok();
                        let _ = reply.send(known);
                    }
                    Some(BusCommand::Shutdown(ack)) => {
                        if let Some(w) = watcher.take() {
                            w.shutdown().await;
                        }
                        let _ = ack.send(());
                        return;
                    }
                    None => {
                        if let Some(w) = watcher.take() {
                            w.shutdown().await;
                        }
                        return;
                    }
                }
            }
            msg = async { watcher_rx.as_mut().expect("guard").recv().await },
                if watcher_rx.is_some() => {
                match msg {
                    Some(WatcherMessage::Batch { repo, events }) => {
                        // Solo repos montados: un lote tardío de un repo ya
                        // desmontado no debe recrear su estado (delta zombi).
                        let Some(state) = states.get_mut(&repo) else { continue };
                        state.last_activity_ms = now_ms();
                        let fs_events = state.fs_events(&repo, &events);
                        if !fs_events.is_empty() {
                            emit(&sink, EVENT_FS_EVENTS, &FsEventBatch {
                                repo: repo.clone(),
                                events: fs_events,
                            });
                        }
                        if let Some(scope) = recalc_scope(&events) {
                            trigger_recalc(repo, scope, &subscriptions,
                                &semaphore, &results_tx, &mut inflight, &mut pending);
                        }
                    }
                    Some(WatcherMessage::RepoError { repo, error }) => {
                        let Some(state) = states.get_mut(&repo) else { continue };
                        state.apply_error(watcher_error_state(&error));
                        let delta = state.delta(&repo);
                        emit(&sink, EVENT_WORKBENCH_DELTA, &delta);
                    }
                    Some(WatcherMessage::RescanNeeded { repo }) => {
                        if states.contains_key(&repo) {
                            trigger_recalc(repo, RecalcScope::Everything, &subscriptions,
                                &semaphore, &results_tx, &mut inflight, &mut pending);
                        }
                    }
                    // El watcher murió (shutdown externo improbable): degradar.
                    None => {
                        watcher_rx = None;
                        emit(&sink, EVENT_WATCHING_STATE, &WatchingState {
                            available: false,
                            reason: Some("el canal del watcher se cerró".into()),
                        });
                    }
                }
            }
            Some(result) = results_rx.recv() => {
                inflight.remove(&result.repo);
                // Si el repo se desmontó mientras el recálculo estaba en vuelo,
                // descartar el resultado: no recrear estado ni emitir delta zombi.
                let Some(state) = states.get_mut(&result.repo) else {
                    pending.remove(&result.repo);
                    continue;
                };
                match result.outcome {
                    Ok(outcome) => {
                        state.apply_recalc(&outcome);
                        let mut delta = state.delta(&result.repo);
                        delta.subscribed_diffs = outcome.subscribed_diffs;
                        emit(&sink, EVENT_WORKBENCH_DELTA, &delta);
                    }
                    Err(e) => {
                        state.apply_error(git_error_state(&e));
                        let delta = state.delta(&result.repo);
                        emit(&sink, EVENT_WORKBENCH_DELTA, &delta);
                    }
                }
                // Coalescing: si llegaron disparadores durante el vuelo, un
                // recálculo más (no una cola).
                if let Some(scope) = pending.remove(&result.repo) {
                    trigger_recalc(result.repo, scope, &subscriptions,
                        &semaphore, &results_tx, &mut inflight, &mut pending);
                }
            }
        }
    }
}

/// Remonta el workbench: canonicaliza, reconstruye estados y dispara el
/// snapshot completo (recálculo Everything por repo).
#[allow(clippy::too_many_arguments)]
fn set_workbench(
    repos: Vec<RepoEntry>,
    watcher: &mut Option<FsWatcher>,
    states: &mut HashMap<PathBuf, RepoLiveState>,
    current_entries: &mut Vec<RepoEntry>,
    unsupported_entries: &mut Vec<RepoEntry>,
    revisions: &mut HashMap<PathBuf, u64>,
    subscriptions: &[SubscriptionTarget],
    semaphore: &Arc<Semaphore>,
    results_tx: &mpsc::UnboundedSender<RecalcResult>,
    inflight: &mut HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, RecalcScope>,
) {
    let (supported, unsupported): (Vec<RepoEntry>, Vec<RepoEntry>) = repos
        .into_iter()
        .partition(|repo| repo.is_runtime_supported());
    *unsupported_entries = unsupported;

    let canonical: Vec<RepoEntry> = supported
        .into_iter()
        .map(|mut r| {
            r.path = r.path.canonicalize().unwrap_or(r.path);
            r
        })
        .collect();

    if let Some(w) = watcher.as_mut() {
        w.watch_workbench(&canonical);
    }

    let keep: HashSet<PathBuf> = canonical.iter().map(|r| r.path.clone()).collect();
    // Persistir la revisión de los repos que se desmontan, para reanudarla si
    // vuelven (revisión monotónica por repo del contrato).
    for (repo, state) in states.iter() {
        if !keep.contains(repo) {
            revisions.insert(repo.clone(), state.revision);
        }
    }
    states.retain(|repo, _| keep.contains(repo));
    pending.retain(|repo, _| keep.contains(repo));
    for entry in &canonical {
        // Repo re-añadido: continuar su contador desde la última revisión
        // conocida en vez de reiniciar a 0.
        states
            .entry(entry.path.clone())
            .or_insert_with(|| RepoLiveState {
                revision: revisions.get(&entry.path).copied().unwrap_or(0),
                ..Default::default()
            });
        trigger_recalc(
            entry.path.clone(),
            RecalcScope::Everything,
            subscriptions,
            semaphore,
            results_tx,
            inflight,
            pending,
        );
    }
    *current_entries = canonical;
}

fn resolve_repo_for_command(
    repo: PathBuf,
    current_entries: &[RepoEntry],
    unsupported_entries: &[RepoEntry],
) -> Result<PathBuf, RepoResolveError> {
    if current_entries.iter().any(|entry| entry.path == repo) {
        return Ok(repo);
    }

    if let Some(entry) = unsupported_entries.iter().find(|entry| entry.path == repo) {
        return Err(RepoResolveError::UnsupportedRepoSource {
            source: entry.source,
        });
    }

    let canon = repo
        .canonicalize()
        .map_err(|_| RepoResolveError::RepositoryNotFound)?;
    if current_entries.iter().any(|entry| entry.path == canon) {
        Ok(canon)
    } else {
        Err(RepoResolveError::RepoNotAllowed)
    }
}

/// Dispara un recálculo de un repo (o lo deja pendiente si hay uno en
/// vuelo). El trabajo git corre en `spawn_blocking` con permiso del
/// semáforo; los diffs de objetivos suscritos se computan dentro del mismo
/// closure con el snapshot de suscripciones capturado al spawnear.
fn trigger_recalc(
    repo: PathBuf,
    scope: RecalcScope,
    subscriptions: &[SubscriptionTarget],
    semaphore: &Arc<Semaphore>,
    results_tx: &mpsc::UnboundedSender<RecalcResult>,
    inflight: &mut HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, RecalcScope>,
) {
    if inflight.contains(&repo) {
        // Coalesce: conservar el scope más amplio.
        pending
            .entry(repo)
            .and_modify(|s| *s = (*s).max(scope))
            .or_insert(scope);
        return;
    }
    inflight.insert(repo.clone());

    let subs: Vec<SubscriptionTarget> = subscriptions
        .iter()
        .filter(|t| t.repo == repo)
        .cloned()
        .collect();
    let sem = Arc::clone(semaphore);
    let tx = results_tx.clone();

    tokio::spawn(async move {
        let _permit = sem.acquire_owned().await;
        let repo_for_task = repo.clone();
        let outcome =
            tokio::task::spawn_blocking(move || recalc_blocking(&repo_for_task, scope, &subs))
                .await
                .unwrap_or_else(|_| Err(GitError::NotFound("recalc task panicked".into())));
        let _ = tx.send(RecalcResult { repo, outcome });
    });
}

/// El recálculo bloqueante: abre el engine, lee según scope y computa los
/// diffs suscritos (incluido el sintetizado para untracked).
fn recalc_blocking(
    repo: &Path,
    scope: RecalcScope,
    subs: &[SubscriptionTarget],
) -> Result<RecalcOutcome, GitError> {
    let engine = Git2Engine::open(repo)?;
    let status = engine.status()?;
    let (branch, head) = match scope {
        RecalcScope::StatusOnly => (None, None),
        RecalcScope::Everything => {
            let branch = engine.branch_info()?;
            let head = match engine.head_commit() {
                Ok(c) => Some(c),
                Err(GitError::UnbornHead) => None,
                Err(e) => return Err(e),
            };
            (Some(branch), Some(head))
        }
    };

    // RDM-011 metrics/signals are computed from the current worktree diff.
    // This is intentionally read-only and runs inside the existing bounded
    // spawn_blocking recompute path.
    let worktree_diffs = engine.worktree_diff()?;
    let secret_findings = secret_scan::detect_secret_findings(repo, &status, &worktree_diffs);
    let gitleaks_configured = secret_scan::has_repo_gitleaks_config(repo);
    let (metrics, signals) = metrics_and_signals(&status, &worktree_diffs, &secret_findings);

    let subscribed_diffs = if subs.is_empty() {
        None
    } else {
        let mut diffs = worktree_diffs.clone();
        // Objetivo con archivo: filtrar al archivo; repo completo: todo.
        let file_targets: Vec<&PathBuf> = subs.iter().filter_map(|t| t.path.as_ref()).collect();
        let whole_repo = subs.iter().any(|t| t.path.is_none());
        if !whole_repo {
            diffs.retain(|d| file_targets.iter().any(|p| **p == d.path));
        }
        // Untracked suscrito: diff sintetizado todo-añadido.
        for target in &file_targets {
            let target: &Path = target.as_path();
            if status.untracked.iter().any(|u| u.as_path() == target)
                && !diffs.iter().any(|d| d.path.as_path() == target)
            {
                if let Some(diff) = commands::synthesize_untracked_diff(repo, target) {
                    diffs.push(diff);
                }
            }
        }
        Some(diffs)
    };

    Ok(RecalcOutcome {
        status,
        branch,
        head,
        subscribed_diffs,
        metrics,
        gitleaks_configured,
        signals,
        secret_findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_fixtures::TempRepo;
    use crate::watcher::EventType;
    use std::time::Duration;
    use tokio::time::timeout;

    // ---- U2: lógica pura ----

    fn classified(path: &str, classification: Classification, kind: EventType) -> ClassifiedEvent {
        ClassifiedEvent {
            path: path.into(),
            classification,
            kind,
            timestamp_ms: 1,
        }
    }

    #[test]
    fn recalc_scope_por_contenido_de_lote() {
        // GitMeta ⇒ Everything.
        assert_eq!(
            recalc_scope(&[classified(
                ".git/HEAD",
                Classification::GitMeta,
                EventType::Modified
            )]),
            Some(RecalcScope::Everything)
        );
        // Plane1 ordinario ⇒ StatusOnly.
        assert_eq!(
            recalc_scope(&[classified(
                "src/a.rs",
                Classification::Plane1,
                EventType::Modified
            )]),
            Some(RecalcScope::StatusOnly)
        );
        // .gitignore Plane1 ⇒ Everything.
        assert_eq!(
            recalc_scope(&[classified(
                ".gitignore",
                Classification::Plane1,
                EventType::Modified
            )]),
            Some(RecalcScope::Everything)
        );
        // Solo Plane2 ⇒ sin recálculo git.
        assert_eq!(
            recalc_scope(&[classified(
                ".env",
                Classification::Plane2,
                EventType::Modified
            )]),
            None
        );
    }

    #[test]
    fn revision_monotonica_y_error_transitorio_se_limpia() {
        let mut state = RepoLiveState::default();
        let outcome = RecalcOutcome {
            status: crate::git::RepoStatus::default(),
            branch: None,
            head: None,
            subscribed_diffs: None,
            metrics: RepoMetrics::default(),
            gitleaks_configured: false,
            signals: Vec::new(),
            secret_findings: Vec::new(),
        };
        state.apply_recalc(&outcome);
        assert_eq!(state.revision, 1);

        // Error transitorio.
        state.apply_error(RepoErrorState {
            class: RepoErrorClass::Transient,
            category: "internal".into(),
            message: "x".into(),
        });
        assert_eq!(state.revision, 2);
        assert!(state.error.is_some());

        // Recálculo OK limpia el transitorio.
        state.apply_recalc(&outcome);
        assert_eq!(state.revision, 3);
        assert!(state.error.is_none());
    }

    #[test]
    fn recalculo_ok_limpia_error_terminal() {
        // Un recálculo OK (que solo ocurre tras un remount/retry exitoso)
        // limpia el error terminal: el repo recreado revive (AE8).
        let mut state = RepoLiveState::default();
        state.apply_error(RepoErrorState {
            class: RepoErrorClass::Terminal,
            category: "repo-removed".into(),
            message: "x".into(),
        });
        let outcome = RecalcOutcome {
            status: crate::git::RepoStatus::default(),
            branch: None,
            head: None,
            subscribed_diffs: None,
            metrics: RepoMetrics::default(),
            gitleaks_configured: false,
            signals: Vec::new(),
            secret_findings: Vec::new(),
        };
        state.apply_recalc(&outcome);
        assert!(
            state.error.is_none(),
            "un recálculo OK demuestra repo legible → limpia el terminal"
        );
    }

    #[test]
    fn fs_events_calcula_tamano_y_delta() {
        let repo = TempRepo::with_initial_commit();
        repo.write(".env", "ABCDE"); // 5 bytes
        let mut state = RepoLiveState::default();

        let first = state.fs_events(
            repo.path(),
            &[classified(
                ".env",
                Classification::Plane2,
                EventType::Created,
            )],
        );
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].size, Some(5));
        assert_eq!(first[0].size_delta, None, "sin tamaño previo");
        assert!(
            first[0]
                .signals
                .iter()
                .any(|s| s.kind == PassiveSignalKind::SensitivePath),
            ".env emite señal sensible en Plane 2"
        );

        repo.write(".env", "ABCDEFG"); // 7 bytes
        let second = state.fs_events(
            repo.path(),
            &[classified(
                ".env",
                Classification::Plane2,
                EventType::Modified,
            )],
        );
        assert_eq!(second[0].size, Some(7));
        assert_eq!(second[0].size_delta, Some(2));

        // Un evento Plane1 no produce FsEvent.
        let none = state.fs_events(
            repo.path(),
            &[classified(
                "src/a.rs",
                Classification::Plane1,
                EventType::Modified,
            )],
        );
        assert!(none.is_empty());
    }

    #[test]
    fn signals_detectan_paths_sensibles_config_y_tests() {
        let status = crate::git::RepoStatus {
            modified: vec![
                ".env.local".into(),
                "package.json".into(),
                "src/app.test.ts".into(),
            ],
            staged: Vec::new(),
            untracked: Vec::new(),
        };
        let (_metrics, signals) = metrics_and_signals(&status, &[], &[]);
        assert!(signals
            .iter()
            .any(|s| s.kind == PassiveSignalKind::SensitivePath));
        assert!(signals
            .iter()
            .any(|s| s.kind == PassiveSignalKind::ConfigChange));
        assert!(signals
            .iter()
            .any(|s| s.kind == PassiveSignalKind::TestChange));
    }

    #[test]
    fn metrics_y_secret_marker_no_exponen_valor() {
        let diff = FileDiff {
            path: "src/config.rs".into(),
            old_path: None,
            is_binary: false,
            hunks: vec![crate::git::DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: vec![
                    crate::git::DiffLine {
                        kind: DiffLineKind::Added,
                        content: "api_key = \"super-secret-value\"".into(),
                        old_lineno: None,
                        new_lineno: Some(1),
                    },
                    crate::git::DiffLine {
                        kind: DiffLineKind::Removed,
                        content: "old".into(),
                        old_lineno: Some(1),
                        new_lineno: None,
                    },
                ],
            }],
        };
        let findings = vec![SecretFinding {
            path: "src/config.rs".into(),
            line: 1,
            rule_id: "generic-api-key".into(),
            description: "Possible secret".into(),
        }];
        let (metrics, signals) =
            metrics_and_signals(&crate::git::RepoStatus::default(), &[diff], &findings);
        assert_eq!(metrics.changed_files, 1);
        assert_eq!(metrics.lines_added, 1);
        assert_eq!(metrics.lines_removed, 1);
        let secret = signals
            .iter()
            .find(|s| s.kind == PassiveSignalKind::PossibleSecret)
            .expect("secret signal");
        assert_eq!(secret.severity, SignalSeverity::Critical);
        assert!(
            !secret.message.contains("super-secret-value"),
            "signal message must not leak matched value"
        );
    }

    #[test]
    fn large_delete_y_cap_son_deterministicos() {
        let removed_lines = (0..LARGE_DELETE_LINES)
            .map(|i| crate::git::DiffLine {
                kind: DiffLineKind::Removed,
                content: format!("line {i}"),
                old_lineno: Some(i as u32 + 1),
                new_lineno: None,
            })
            .collect();
        let diff = FileDiff {
            path: "src/big.ts".into(),
            old_path: None,
            is_binary: false,
            hunks: vec![crate::git::DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: removed_lines,
            }],
        };
        let status = crate::git::RepoStatus {
            modified: (0..20).map(|i| format!("secret-{i}.txt").into()).collect(),
            staged: Vec::new(),
            untracked: Vec::new(),
        };
        let (_metrics, signals) = metrics_and_signals(&status, &[diff], &[]);
        assert!(signals.len() <= MAX_SIGNALS_PER_REPO);
        assert!(signals
            .iter()
            .any(|s| s.kind == PassiveSignalKind::LargeDelete));
    }

    // ---- U3: integración (bus + watcher real sobre fixtures) ----

    type Events = mpsc::UnboundedReceiver<(String, serde_json::Value)>;

    fn entry(repo: &TempRepo) -> RepoEntry {
        RepoEntry::local(repo.path().to_path_buf(), None, Vec::new())
    }

    fn wsl_entry(path: &str) -> RepoEntry {
        RepoEntry {
            source: RepoSource::Wsl,
            path: PathBuf::from(path),
            distro: Some("Ubuntu".into()),
            alias: Some("WSL".into()),
            fs_watch: Vec::new(),
        }
    }

    fn canonical(repo: &TempRepo) -> PathBuf {
        repo.path().canonicalize().unwrap()
    }

    fn spawn_bus(initial: Vec<RepoEntry>) -> (BusHandle, Events) {
        let (handle, rx) = BusHandle::new_pair();
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let sink: DeltaSink = Arc::new(move |e: &str, p: serde_json::Value| {
            let _ = ev_tx.send((e.to_string(), p));
        });
        tokio::spawn(run_bus(rx, sink, initial));
        (handle, ev_rx)
    }

    async fn wait_event(
        rx: &mut Events,
        mut pred: impl FnMut(&str, &serde_json::Value) -> bool,
    ) -> Option<(String, serde_json::Value)> {
        timeout(Duration::from_secs(10), async {
            loop {
                let (e, p) = rx.recv().await?;
                if pred(&e, &p) {
                    return Some((e, p));
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    fn is_delta_for(e: &str, p: &serde_json::Value, repo: &Path) -> bool {
        e == EVENT_WORKBENCH_DELTA && p["repo"] == repo.to_string_lossy().as_ref()
    }

    /// Arranca el bus en modo degradado (init del watcher inyectado como Err),
    /// para cubrir AE11 sin un backend de notify real.
    fn spawn_bus_degraded(initial: Vec<RepoEntry>) -> (BusHandle, Events) {
        let (handle, rx) = BusHandle::new_pair();
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let sink: DeltaSink = Arc::new(move |e: &str, p: serde_json::Value| {
            let _ = ev_tx.send((e.to_string(), p));
        });
        let init: WatcherInit = Err(WatcherError::BackendInit {
            message: "inotify agotado (test)".into(),
        });
        tokio::spawn(run_bus_inner(rx, sink, initial, init));
        (handle, ev_rx)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae11_arranque_degradado_senala_estado_y_responde() {
        let a = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus_degraded(vec![entry(&a)]);
        let ca = canonical(&a);

        // Señala watching no disponible con razón.
        let (_e, p) = wait_event(&mut rx, |e, _| e == EVENT_WATCHING_STATE)
            .await
            .expect("watching-state");
        assert_eq!(p["available"], false);
        assert!(p["reason"].is_string());

        // Aun degradado, el snapshot responde y la allowlist funciona.
        let snap = handle.snapshot().await.expect("snapshot en degradado");
        assert!(!snap.watching.available);
        assert!(snap.repos.iter().any(|d| d.repo == ca));
        assert!(handle.is_known(ca.clone()).await, "allowlist sin watcher");
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn allowlist_solo_acepta_repos_montados() {
        let a = TempRepo::with_initial_commit();
        let b = TempRepo::with_initial_commit();
        let (handle, _rx) = spawn_bus(vec![entry(&a)]);
        assert!(handle.is_known(canonical(&a)).await, "A montado");
        assert!(
            !handle.is_known(canonical(&b)).await,
            "B no está en el workbench"
        );
        // Un path fuera de todo workbench (raíz) tampoco.
        assert!(!handle.is_known(PathBuf::from("/")).await);
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wsl_source_is_not_mounted_and_resolves_as_unsupported() {
        let a = TempRepo::with_initial_commit();
        let wsl_path = PathBuf::from("/home/me/proyecto");
        let (handle, mut rx) = spawn_bus_degraded(vec![entry(&a), wsl_entry("/home/me/proyecto")]);
        let ca = canonical(&a);

        wait_event(&mut rx, |e, _| e == EVENT_WATCHING_STATE).await;
        let snap = handle.snapshot().await.expect("snapshot");

        assert!(snap.repos.iter().any(|repo| repo.repo == ca));
        assert!(
            !snap.repos.iter().any(|repo| repo.repo == wsl_path),
            "future WSL sources must not enter the runtime snapshot"
        );
        assert!(!handle.is_known(wsl_path.clone()).await);
        assert!(matches!(
            handle.resolve_repo(wsl_path).await,
            Err(RepoResolveError::UnsupportedRepoSource {
                source: RepoSource::Wsl
            })
        ));
        assert!(handle.retry_repo(PathBuf::from("/home/me/proyecto")).await);
        assert!(
            handle
                .subscribe(vec![SubscriptionTarget {
                    repo: PathBuf::from("/home/me/proyecto"),
                    path: Some("base.txt".into()),
                }])
                .await
        );
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn same_path_wsl_source_does_not_block_mounted_local_repo() {
        let a = TempRepo::with_initial_commit();
        let ca = canonical(&a);
        let (handle, mut rx) =
            spawn_bus_degraded(vec![entry(&a), wsl_entry(ca.to_string_lossy().as_ref())]);

        wait_event(&mut rx, |e, _| e == EVENT_WATCHING_STATE).await;
        assert_eq!(handle.resolve_repo(ca.clone()).await, Ok(ca.clone()));
        assert!(handle.is_known(ca).await);
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unsupported_wsl_subscription_does_not_canonicalize_into_local_repo() {
        let a = TempRepo::with_initial_commit();
        let ca = canonical(&a);
        let unsupported_alias = ca.join(".");
        let (handle, mut rx) = spawn_bus_degraded(vec![
            entry(&a),
            wsl_entry(unsupported_alias.to_string_lossy().as_ref()),
        ]);

        wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca)).await;
        while rx.try_recv().is_ok() {}
        a.write("base.txt", "changed\n");

        assert!(
            handle
                .subscribe(vec![SubscriptionTarget {
                    repo: unsupported_alias,
                    path: Some("base.txt".into()),
                }])
                .await
        );

        let leaked = timeout(Duration::from_secs(1), async {
            loop {
                let (e, p) = rx.recv().await?;
                if is_delta_for(&e, &p, &ca) {
                    return Some(());
                }
            }
        })
        .await
        .ok()
        .flatten();
        assert!(
            leaked.is_none(),
            "unsupported WSL subscription must not resolve into a local repo"
        );
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unsupported_wsl_retry_does_not_canonicalize_into_local_repo() {
        let a = TempRepo::with_initial_commit();
        let ca = canonical(&a);
        let unsupported_alias = ca.join(".");
        let (handle, mut rx) = spawn_bus_degraded(vec![
            entry(&a),
            wsl_entry(unsupported_alias.to_string_lossy().as_ref()),
        ]);

        wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca)).await;
        while rx.try_recv().is_ok() {}
        a.write("retry.txt", "changed\n");

        assert!(handle.retry_repo(unsupported_alias).await);

        let leaked = timeout(Duration::from_secs(1), async {
            loop {
                let (e, p) = rx.recv().await?;
                if is_delta_for(&e, &p, &ca) {
                    return Some(());
                }
            }
        })
        .await
        .ok()
        .flatten();
        assert!(
            leaked.is_none(),
            "unsupported WSL retry must not resolve into a local repo"
        );
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn revision_continua_tras_re_add() {
        let a = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a)]);
        let ca = canonical(&a);
        let (_e, p) = wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca))
            .await
            .expect("snapshot inicial");
        let rev0 = p["revision"].as_u64().expect("revision");

        // Desmontar y re-montar el mismo repo.
        handle.set_workbench(vec![]);
        handle.set_workbench(vec![entry(&a)]);

        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &ca) && p["revision"].as_u64().is_some_and(|r| r > rev0)
        })
        .await
        .expect("delta tras re-add con revisión continuada");
        assert!(
            p["revision"].as_u64().unwrap() > rev0,
            "la revisión no debe reiniciarse a 0 al re-añadir"
        );
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae7_set_workbench_purga_repo_dropeado() {
        let a = TempRepo::with_initial_commit();
        let b = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a)]);
        let (ca, cb) = (canonical(&a), canonical(&b));
        wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca)).await;
        while rx.try_recv().is_ok() {}

        // Conmutar a un workbench con solo B.
        handle.set_workbench(vec![entry(&b)]);
        assert!(
            wait_event(&mut rx, |e, p| is_delta_for(e, p, &cb))
                .await
                .is_some(),
            "monta el nuevo repo B"
        );
        while rx.try_recv().is_ok() {}

        // Tocar A (ya dropeado) no debe producir delta zombi. Ventana corta:
        // es una aserción negativa.
        a.write("zombi.txt", "x");
        let leaked = timeout(Duration::from_secs(2), async {
            loop {
                let (e, p) = rx.recv().await?;
                if is_delta_for(&e, &p, &ca) {
                    return Some(());
                }
            }
        })
        .await
        .ok()
        .flatten();
        assert!(leaked.is_none(), "A fue dropeado: sin deltas zombi");
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae1_ae7_arranque_emite_delta_por_repo() {
        let a = TempRepo::with_initial_commit();
        let b = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a), entry(&b)]);
        let (ca, cb) = (canonical(&a), canonical(&b));

        // Los snapshots iniciales de A y B llegan en orden no determinista
        // (dos `spawn_blocking` concurrentes); se aceptan en cualquier orden.
        let (mut got_a, mut got_b) = (false, false);
        while !(got_a && got_b) {
            let found = wait_event(&mut rx, |e, p| {
                is_delta_for(e, p, &ca) || is_delta_for(e, p, &cb)
            })
            .await;
            match found {
                Some((_, p)) if p["repo"] == ca.to_string_lossy().as_ref() => got_a = true,
                Some((_, p)) if p["repo"] == cb.to_string_lossy().as_ref() => got_b = true,
                _ => panic!("falta snapshot inicial (A={got_a}, B={got_b})"),
            }
        }
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae1_escritura_produce_delta_solo_del_repo_tocado() {
        let a = TempRepo::with_initial_commit();
        let b = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a), entry(&b)]);
        let ca = canonical(&a);

        // Drenar snapshots iniciales.
        wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca)).await;
        while rx.try_recv().is_ok() {}

        a.write("nuevo.txt", "contenido");
        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &ca)
                && p["status"]["untracked"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == "nuevo.txt"))
        })
        .await
        .expect("delta de A con el untracked");
        assert_eq!(p["repo"], ca.to_string_lossy().as_ref());
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae3_suscripcion_incluye_diff_solo_cuando_suscrito() {
        let a = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a)]);
        let ca = canonical(&a);

        // Snapshot inicial: sin suscripción ⇒ sin subscribed_diffs.
        let (_e, p) = wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca))
            .await
            .expect("snapshot");
        assert!(p.get("subscribed_diffs").is_none() || p["subscribed_diffs"].is_null());
        while rx.try_recv().is_ok() {}

        // Modificar base.txt y suscribirse al archivo.
        a.write("base.txt", "linea 1 cambiada\nlinea 2\nlinea 3\n");
        assert!(
            handle
                .subscribe(vec![SubscriptionTarget {
                    repo: ca.clone(),
                    path: Some("base.txt".into()),
                }])
                .await
        );

        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &ca)
                && p["subscribed_diffs"]
                    .as_array()
                    .is_some_and(|d| !d.is_empty())
        })
        .await
        .expect("delta con diff del objetivo suscrito");
        let diffs = p["subscribed_diffs"].as_array().unwrap();
        assert!(diffs.iter().any(|d| d["path"] == "base.txt"));
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae10_untracked_suscrito_diff_sintetizado() {
        let a = TempRepo::with_initial_commit();
        let (handle, mut rx) = spawn_bus(vec![entry(&a)]);
        let ca = canonical(&a);
        wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca)).await;
        while rx.try_recv().is_ok() {}

        a.write("nuevo.txt", "alfa\nbeta\n");
        assert!(
            handle
                .subscribe(vec![SubscriptionTarget {
                    repo: ca.clone(),
                    path: Some("nuevo.txt".into()),
                }])
                .await
        );

        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &ca)
                && p["subscribed_diffs"]
                    .as_array()
                    .is_some_and(|d| d.iter().any(|x| x["path"] == "nuevo.txt"))
        })
        .await
        .expect("diff sintetizado del untracked");
        let diff = p["subscribed_diffs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["path"] == "nuevo.txt")
            .unwrap();
        assert_eq!(diff["is_binary"], false);
        assert!(diff["hunks"].as_array().is_some_and(|h| !h.is_empty()));
        handle.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ae8_repo_removido_estado_terminal() {
        let a = TempRepo::with_initial_commit();
        let temp = tempfile::tempdir().unwrap();
        let b_path = temp.path().join("repo_b");
        {
            let init = git2::Repository::init(&b_path).unwrap();
            std::fs::write(b_path.join("f.txt"), "x").unwrap();
            let mut idx = init.index().unwrap();
            idx.add_path(Path::new("f.txt")).unwrap();
            idx.write().unwrap();
        }
        let cb = b_path.canonicalize().unwrap();
        let entry_b = RepoEntry::local(cb.clone(), None, Vec::new());
        let (handle, mut rx) = spawn_bus(vec![entry(&a), entry_b]);
        wait_event(&mut rx, |e, p| is_delta_for(e, p, &cb)).await;

        std::fs::remove_dir_all(&b_path).unwrap();
        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &cb) && p["error"]["class"] == "terminal"
        })
        .await
        .expect("estado de error terminal de B");
        assert_eq!(p["error"]["category"], "repo-removed");

        // A sigue vivo.
        let ca = canonical(&a);
        a.write("vivo.txt", "y");
        assert!(
            wait_event(&mut rx, |e, p| is_delta_for(e, p, &ca))
                .await
                .is_some(),
            "A sigue produciendo deltas"
        );

        // Revivir B: recrearlo en disco y pedir retry → delta sano sin error.
        {
            let init = git2::Repository::init(&b_path).unwrap();
            std::fs::write(b_path.join("f.txt"), "x").unwrap();
            let mut idx = init.index().unwrap();
            idx.add_path(Path::new("f.txt")).unwrap();
            idx.write().unwrap();
        }
        assert!(handle.retry_repo(cb.clone()).await);
        let (_e, p) = wait_event(&mut rx, |e, p| {
            is_delta_for(e, p, &cb) && (p["error"].is_null() || p.get("error").is_none())
        })
        .await
        .expect("B revive tras retry (AE8)");
        assert!(
            p["error"].is_null() || p.get("error").is_none(),
            "el error terminal se limpia al revivir"
        );
        handle.shutdown().await;
    }
}
