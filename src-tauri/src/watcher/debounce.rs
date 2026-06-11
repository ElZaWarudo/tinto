//! Debounce + throttling por repo (plan U2). Lógica de agrupación pura
//! respecto al FS: consume eventos clasificados etiquetados por repo y
//! emite lotes (`EmittedBatch`) hacia la capa dueña de los clasificadores
//! (`FsWatcher`), que ejecuta el rebuild coalescido y reenvía al canal
//! público.
//!
//! Por repo se mantiene: buffer de eventos, deadline de emisión, marca de
//! última emisión y el flag "rebuild pendiente" (viaja con el lote). El
//! deadline combina tres reglas:
//!
//! - **Calma (debounce):** el lote cierra tras `DEBOUNCE_WINDOW` sin
//!   eventos nuevos.
//! - **Techo de ventana:** bajo actividad continua el lote sale a más
//!   tardar `THROTTLE_INTERVAL` después de abrirse la ventana (R9: nada
//!   se pierde, se difiere).
//! - **Piso de throttle:** ningún lote sale antes de
//!   `última emisión + THROTTLE_INTERVAL` (máximo un lote por intervalo).
//!
//! `deadline = max(piso, min(calma, techo))`.
//!
//! Una sola task con `select!` sobre el deadline mínimo (no una task por
//! repo): la actividad de un repo no toca los deadlines de otro. Testeable
//! determinísticamente con `#[tokio::test(start_paused = true)]`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

use super::ClassifiedEvent;

/// Ventana de calma que cierra un lote (~200–400 ms según el diseño §7).
pub const DEBOUNCE_WINDOW: Duration = Duration::from_millis(300);
/// Frecuencia máxima de lotes por repo bajo actividad continua.
pub const THROTTLE_INTERVAL: Duration = Duration::from_secs(1);

/// Entrada de la task de debounce.
#[derive(Debug)]
pub enum DebounceInput {
    /// Evento clasificado de un repo.
    Event {
        repo: PathBuf,
        event: ClassifiedEvent,
    },
    /// El `.gitignore` del repo cambió: marca rebuild coalescido.
    GitignoreTouched { repo: PathBuf },
    /// Descarta el estado pendiente de un repo desmontado.
    ForgetRepo { repo: PathBuf },
}

/// Lote emitido hacia el `FsWatcher`, con la señal de rebuild coalescida
/// de la ventana. Puede llevar `events` vacío si solo hay rebuild.
#[derive(Debug)]
pub struct EmittedBatch {
    pub repo: PathBuf,
    pub events: Vec<ClassifiedEvent>,
    pub rebuild_pending: bool,
}

#[derive(Debug, Default)]
struct RepoState {
    buffer: Vec<ClassifiedEvent>,
    /// Apertura de la ventana actual (primer evento desde el último emit).
    window_start: Option<Instant>,
    deadline: Option<Instant>,
    last_emit: Option<Instant>,
    rebuild_pending: bool,
}

impl RepoState {
    /// Re-arma el deadline al recibir actividad en `now`.
    fn arm(&mut self, now: Instant) {
        let window_start = *self.window_start.get_or_insert(now);
        let quiet = now + DEBOUNCE_WINDOW;
        let ceiling = window_start + THROTTLE_INTERVAL;
        let mut deadline = quiet.min(ceiling);
        if let Some(last) = self.last_emit {
            deadline = deadline.max(last + THROTTLE_INTERVAL);
        }
        self.deadline = Some(deadline);
    }

    fn take_batch(&mut self, repo: &std::path::Path, now: Instant) -> EmittedBatch {
        let batch = EmittedBatch {
            repo: repo.to_path_buf(),
            events: std::mem::take(&mut self.buffer),
            rebuild_pending: std::mem::take(&mut self.rebuild_pending),
        };
        self.window_start = None;
        self.deadline = None;
        self.last_emit = Some(now);
        batch
    }
}

/// Corre el loop de debounce: consume `DebounceInput` y emite
/// `EmittedBatch` por `out`. Termina (con flush final) cuando `input` se
/// cierra.
pub async fn run(
    mut input: mpsc::UnboundedReceiver<DebounceInput>,
    out: mpsc::UnboundedSender<EmittedBatch>,
) {
    let mut repos: HashMap<PathBuf, RepoState> = HashMap::new();

    loop {
        let next_deadline = repos.values().filter_map(|s| s.deadline).min();

        tokio::select! {
            maybe = input.recv() => {
                let now = Instant::now();
                match maybe {
                    Some(DebounceInput::Event { repo, event }) => {
                        let state = repos.entry(repo).or_default();
                        state.buffer.push(event);
                        state.arm(now);
                    }
                    Some(DebounceInput::GitignoreTouched { repo }) => {
                        // La señal también abre/renueva ventana: el rebuild
                        // se entrega aunque no haya eventos clasificables.
                        let state = repos.entry(repo).or_default();
                        state.rebuild_pending = true;
                        state.arm(now);
                    }
                    Some(DebounceInput::ForgetRepo { repo }) => {
                        repos.remove(&repo);
                    }
                    // Canal de entrada cerrado: flush final y salir.
                    None => {
                        for (repo, mut state) in repos.drain() {
                            if !state.buffer.is_empty() || state.rebuild_pending {
                                let batch = state.take_batch(&repo, now);
                                let _ = out.send(batch);
                            }
                        }
                        return;
                    }
                }
            }
            _ = tokio::time::sleep_until(
                next_deadline.unwrap_or_else(|| Instant::now() + DEBOUNCE_WINDOW)
            ), if next_deadline.is_some() => {
                let now = Instant::now();
                let due: Vec<PathBuf> = repos
                    .iter()
                    .filter(|(_, s)| s.deadline.is_some_and(|d| d <= now))
                    .map(|(repo, _)| repo.clone())
                    .collect();
                for repo in due {
                    if let Some(state) = repos.get_mut(&repo) {
                        if state.buffer.is_empty() && !state.rebuild_pending {
                            state.deadline = None;
                            continue;
                        }
                        let batch = state.take_batch(&repo, now);
                        let _ = out.send(batch);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Classification;
    use crate::watcher::EventType;
    use tokio::time::{advance, Duration};

    fn ev(path: &str) -> ClassifiedEvent {
        ClassifiedEvent {
            path: path.into(),
            classification: Classification::Plane1,
            kind: EventType::Modified,
            timestamp_ms: 0,
        }
    }

    fn spawn_loop() -> (
        mpsc::UnboundedSender<DebounceInput>,
        mpsc::UnboundedReceiver<EmittedBatch>,
        tokio::task::JoinHandle<()>,
    ) {
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(run(in_rx, out_tx));
        (in_tx, out_rx, handle)
    }

    /// Cede el control para que la task de debounce procese lo encolado
    /// antes de mover el reloj.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rafaga_dentro_de_la_ventana_produce_un_solo_lote() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        for i in 0..20 {
            tx.send(DebounceInput::Event {
                repo: repo.to_path_buf(),
                event: ev(&format!("/r/a/f{i}.txt")),
            })
            .unwrap();
            settle().await;
            advance(Duration::from_millis(4)).await;
        }

        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;

        let batch = rx.try_recv().expect("debe haber exactamente un lote");
        assert_eq!(batch.events.len(), 20);
        assert!(rx.try_recv().is_err(), "no debe haber un segundo lote");
    }

    #[tokio::test(start_paused = true)]
    async fn actividad_continua_emite_a_ritmo_del_throttle() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        // 3 s de actividad continua: un evento cada 100 ms (< ventana de
        // calma), así que el debounce nunca cierra por calma.
        let mut batches = 0;
        for i in 0..30 {
            tx.send(DebounceInput::Event {
                repo: repo.to_path_buf(),
                event: ev(&format!("/r/a/f{i}.txt")),
            })
            .unwrap();
            settle().await;
            advance(Duration::from_millis(100)).await;
            settle().await;
            while rx.try_recv().is_ok() {
                batches += 1;
            }
        }
        // Cierre final por calma.
        advance(THROTTLE_INTERVAL + DEBOUNCE_WINDOW).await;
        settle().await;
        while rx.try_recv().is_ok() {
            batches += 1;
        }

        // ~3 s de actividad con throttle de 1 s → ~3-4 lotes (tolerancia),
        // nunca ~30.
        assert!(
            (2..=5).contains(&batches),
            "esperaba lotes acotados por throttle, hubo {batches}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn ningun_evento_se_pierde_bajo_throttle() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        let total = 30;
        for i in 0..total {
            tx.send(DebounceInput::Event {
                repo: repo.to_path_buf(),
                event: ev(&format!("/r/a/f{i}.txt")),
            })
            .unwrap();
            settle().await;
            advance(Duration::from_millis(100)).await;
            settle().await;
        }
        advance(THROTTLE_INTERVAL + DEBOUNCE_WINDOW).await;
        settle().await;

        let mut received = 0;
        while let Ok(batch) = rx.try_recv() {
            received += batch.events.len();
        }
        assert_eq!(
            received, total,
            "todos los eventos deben llegar en algún lote"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn repos_independientes_no_se_bloquean() {
        let (tx, mut rx, _handle) = spawn_loop();
        let a = PathBuf::from("/r/a");
        let b = PathBuf::from("/r/b");

        // A con actividad continua; B con un evento único.
        for i in 0..5 {
            tx.send(DebounceInput::Event {
                repo: a.clone(),
                event: ev(&format!("/r/a/f{i}.txt")),
            })
            .unwrap();
            settle().await;
            advance(Duration::from_millis(100)).await;
        }
        tx.send(DebounceInput::Event {
            repo: b.clone(),
            event: ev("/r/b/solo.txt"),
        })
        .unwrap();
        settle().await;

        // La calma de B vence aunque A siga activa.
        for _ in 0..4 {
            advance(Duration::from_millis(100)).await;
            settle().await;
            tx.send(DebounceInput::Event {
                repo: a.clone(),
                event: ev("/r/a/mas.txt"),
            })
            .unwrap();
            settle().await;
        }

        advance(DEBOUNCE_WINDOW).await;
        settle().await;

        let mut saw_b = false;
        while let Ok(batch) = rx.try_recv() {
            if batch.repo == b {
                saw_b = true;
                assert_eq!(batch.events.len(), 1);
            }
        }
        assert!(saw_b, "el lote de B no debe esperar al throttle de A");
    }

    #[tokio::test(start_paused = true)]
    async fn rebuild_viaja_con_el_lote_y_se_coalesce() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        for _ in 0..3 {
            tx.send(DebounceInput::GitignoreTouched { repo: repo.clone() })
                .unwrap();
        }
        tx.send(DebounceInput::Event {
            repo: repo.to_path_buf(),
            event: ev("/r/a/x.txt"),
        })
        .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;

        let batch = rx.try_recv().expect("un lote");
        assert!(batch.rebuild_pending, "el rebuild debe viajar con el lote");
        assert!(rx.try_recv().is_err(), "tres señales → un solo rebuild");
    }

    #[tokio::test(start_paused = true)]
    async fn rebuild_sin_eventos_tambien_se_entrega() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        tx.send(DebounceInput::GitignoreTouched { repo: repo.clone() })
            .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;

        let batch = rx.try_recv().expect("lote solo-rebuild");
        assert!(batch.events.is_empty());
        assert!(batch.rebuild_pending);
    }

    #[tokio::test(start_paused = true)]
    async fn piso_de_throttle_retiene_el_lote_tras_una_emision() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        // Primera ventana: emite por calma.
        tx.send(DebounceInput::Event {
            repo: repo.clone(),
            event: ev("/r/a/uno.txt"),
        })
        .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;
        assert!(rx.try_recv().is_ok(), "primer lote por calma");

        // Segundo evento inmediato: la calma vence pero el piso de
        // throttle (última emisión + intervalo) retiene el lote.
        tx.send(DebounceInput::Event {
            repo: repo.clone(),
            event: ev("/r/a/dos.txt"),
        })
        .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;
        assert!(
            rx.try_recv().is_err(),
            "el lote NO sale aún: el piso de throttle lo retiene (R9)"
        );

        advance(THROTTLE_INTERVAL).await;
        settle().await;
        let batch = rx.try_recv().expect("el lote sale al vencer el piso");
        assert_eq!(batch.events.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn evento_en_el_borde_de_la_ventana_cae_al_lote_siguiente() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        tx.send(DebounceInput::Event {
            repo: repo.clone(),
            event: ev("/r/a/primero.txt"),
        })
        .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW).await; // exactamente el borde (d <= now)
        settle().await;
        let primero = rx.try_recv().expect("lote del borde");
        assert_eq!(primero.events.len(), 1);

        // Evento que llega justo tras el cierre: va al lote siguiente.
        tx.send(DebounceInput::Event {
            repo: repo.clone(),
            event: ev("/r/a/segundo.txt"),
        })
        .unwrap();
        settle().await;
        advance(THROTTLE_INTERVAL + DEBOUNCE_WINDOW).await;
        settle().await;
        let segundo = rx.try_recv().expect("lote siguiente");
        assert_eq!(segundo.events.len(), 1);
        assert_eq!(segundo.events[0].path, PathBuf::from("/r/a/segundo.txt"));
    }

    #[tokio::test(start_paused = true)]
    async fn forget_repo_descarta_pendientes() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        tx.send(DebounceInput::Event {
            repo: repo.to_path_buf(),
            event: ev("/r/a/x.txt"),
        })
        .unwrap();
        settle().await;
        tx.send(DebounceInput::ForgetRepo { repo: repo.clone() })
            .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW * 4).await;
        settle().await;

        assert!(rx.try_recv().is_err(), "el repo olvidado no emite");
    }

    #[tokio::test(start_paused = true)]
    async fn cierre_del_canal_hace_flush_final() {
        let (tx, mut rx, handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        tx.send(DebounceInput::Event {
            repo: repo.to_path_buf(),
            event: ev("/r/a/x.txt"),
        })
        .unwrap();
        drop(tx);
        handle.await.unwrap();

        let batch = rx.try_recv().expect("flush final");
        assert_eq!(batch.events.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn lotes_del_mismo_repo_llegan_en_orden() {
        let (tx, mut rx, _handle) = spawn_loop();
        let repo = PathBuf::from("/r/a");

        tx.send(DebounceInput::Event {
            repo: repo.to_path_buf(),
            event: ev("/r/a/primero.txt"),
        })
        .unwrap();
        settle().await;
        advance(DEBOUNCE_WINDOW + Duration::from_millis(10)).await;
        settle().await;

        advance(THROTTLE_INTERVAL).await;
        settle().await;

        tx.send(DebounceInput::Event {
            repo: repo.to_path_buf(),
            event: ev("/r/a/segundo.txt"),
        })
        .unwrap();
        settle().await;
        advance(THROTTLE_INTERVAL + DEBOUNCE_WINDOW).await;
        settle().await;

        let primero = rx.try_recv().expect("primer lote");
        let segundo = rx.try_recv().expect("segundo lote");
        assert_eq!(primero.events[0].path, PathBuf::from("/r/a/primero.txt"));
        assert_eq!(segundo.events[0].path, PathBuf::from("/r/a/segundo.txt"));
    }
}
