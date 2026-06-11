---
title: "feat: Watcher de FS con debounce y throttling por repo (RDM-004)"
type: feat
date: 2026-06-11
origin: docs/brainstorms/2026-06-11-rdm-004-watcher-requirements.md
---

# feat: Watcher de FS con debounce y throttling por repo (RDM-004)

## Summary

Añadir al backend el módulo `watcher`: observa con `notify` los working dirs de los repos del workbench activo, normaliza y clasifica cada evento con el `PathClassifier` (RDM-003), agrupa ráfagas con debounce por repo (~200–400 ms), limita la frecuencia de lotes por repo (throttling) y entrega mensajes `(lote | error de repo)` por un canal tokio que RDM-006 consumirá. Expone un API explícito de montaje/remontaje por workbench; el wiring al comando de conmutación es de RDM-006. Sin recálculo de git, sin frontend.

## Requirements Trace

- R1, R2, R4 (montaje recursivo, remount por API, shutdown limpio) → U3
- R5 (clasificación, normalización de kinds) → U1 (tipos/normalización) + U3 (wiring clasificador); R7 (is_dir sin stat) → U1
- R6 (rebuild de classifier ante `.gitignore`, coalescido) → U3
- R8, R9, R10 (debounce, throttle, independencia entre repos) → U2
- R3, R11 (errores por el canal, suscripción y orden) → U1 (tipos) + U2 (entrega) + U3 (síntesis de errores)
- R12, AE1–AE6 → tests de U2 (timing determinista) y U3 (integración FS real)
- Origin: ver `docs/brainstorms/2026-06-11-rdm-004-watcher-requirements.md`

## Key Technical Decisions

- **Debounce/throttle propios sobre timers de tokio** (resuelve la Outstanding Question con su criterio obligatorio). Los `notify-debouncer-*` corren threads propios con reloj de pared, incompatibles con `tokio::time::pause` → tests flaky. La lógica de agrupación se implementa como una task async por watcher sobre `tokio::time`, con el reloj controlable en tests (`#[tokio::test(start_paused = true)]`). **Features de tokio:** producción `["time", "sync", "macros", "rt"]` (`select!` requiere `macros`, `spawn` requiere `rt`); dev-dependency tokio con `["macros", "rt", "test-util"]` (`start_paused` vive detrás de `test-util`). Nota de test: bajo `start_paused` tokio auto-avanza el reloj con las tasks idle — las aserciones "el lote NO salió aún" controlan el avance con `tokio::time::advance`, no con el orden de polls.
- **Puente notify → tokio.** El callback de notify corre en su thread propio (sync): empuja eventos crudos a un `tokio::sync::mpsc::UnboundedSender` (o `std::sync::mpsc` + task puente); la task async del watcher consume, normaliza, clasifica y debounce-a. El hot path por evento no hace `stat` ni I/O (R7): `is_dir` se deriva del `EventKind` cuando está tipado, `false` en su ausencia.
- **Normalización de kinds (contrato para RDM-006):** rename → borrado(from) + creado(to), cada mitad clasificada por su propio path; `Any`/desconocido → modificado; `Access` descartado antes de clasificar. Tres tipos finales: `Created | Modified | Removed`.
- **Mensaje del canal:** `WatcherMessage::Batch { repo: PathBuf, events: Vec<ClassifiedEvent> } | RepoError { repo: PathBuf, error: WatcherError }`. `ClassifiedEvent { path, classification, kind, timestamp }`. Un solo canal mpsc: el watcher posee el `Sender`; `subscribe()`/constructor entrega el `Receiver` único (mpsc, no broadcast — RDM-006 es el único consumidor previsto; broadcast se reevalúa allí).
- **Estado por repo — reparto entre capas:** `debounce.rs` (U2) mantiene por repo: buffer de eventos pendientes, deadline de debounce, marca de último emit (throttle) y un flag **"rebuild pendiente"** — sin conocer el `PathClassifier`. Al emitir el lote, la señal de rebuild viaja con el lote emitido; `mod.rs` (U3), dueño de los `PathClassifier` por repo, ejecuta la reconstrucción una vez por ventana (R6). La actividad de un repo no toca los deadlines de otro (R10).
- **Ciclo de vida:** `FsWatcher` se **diseña** para vivir como managed state de Tauri (patrón del `WorkbenchStore`); su registro en `setup` y la llamada desde la conmutación los cablea **RDM-006** (ver Deferred) — en este item los tests lo construyen y ejercen directamente. La task de debounce se spawnea con **`tokio::spawn`**: `FsWatcher::new()` requiere contexto de runtime tokio (en tests, `#[tokio::test]`; en RDM-006, dentro de `tauri::async_runtime::spawn`/`block_on`, no en el cuerpo síncrono de `setup` — precedente: el tick de `lib.rs`). API: `watch_workbench(&[RepoEntry])` (desmonta lo anterior, monta lo nuevo — diff de paths) y **`shutdown(self)` async** que desmonta watches, cierra el canal y **espera el `JoinHandle`** de la task (garantía verificable de R4); `Drop` es best-effort: `abort()` del handle + drop del watcher de notify (su Drop síncrono cierra los threads), sin esperar. Errores de montaje por repo → `RepoError` por el canal sin abortar el resto (R3). Remoción en caliente: notify no emite error → el watcher sintetiza `RepoError` al ver `Removed` del root del repo.
- **Arranque sin workbench activo:** el watcher arranca vacío (sin watches) y espera `watch_workbench`; no falla ni espera señal externa. Resuelve la Outstanding Question con la opción más simple y sin acoplar a RDM-006.

## Output Structure

```text
src-tauri/src/
├── lib.rs                  # + `pub mod watcher;` (patrón de paths/workbench: pub evita dead_code sin consumidor; managed state en setup cuando RDM-006 lo cablee)
└── watcher/
    ├── mod.rs              # FsWatcher (API pública), WatcherMessage, ClassifiedEvent, WatcherError
    ├── normalize.rs        # EventKind de notify → Created|Modified|Removed + is_dir + regla de renames
    └── debounce.rs         # estado por repo: buffer + deadline + throttle (lógica pura/async testeable)
```

## Implementation Units

### U1. Tipos del canal y normalización de eventos de notify

- **Goal:** Congelar los tipos que viajan por el canal y la regla de normalización de kinds, sin watcher real.
- **Requirements:** R5 (normalización), R7 (is_dir), R11 (forma del mensaje); parte de R3 (variante de error).
- **Dependencies:** None (sobre `paths` y `workbench` ya entregados).
- **Files:** `src-tauri/src/watcher/mod.rs` (tipos), `src-tauri/src/watcher/normalize.rs` (+ tests inline), `src-tauri/src/lib.rs` (`pub mod watcher;`), `src-tauri/Cargo.toml` (`notify` pineado a su major actual, p. ej. `"8"`; tokio producción `["time","sync","macros","rt"]`; dev-dep tokio `["macros","rt","test-util"]`).
- **Approach:** `WatcherMessage`, `ClassifiedEvent`, `WatcherError` (thiserror, siguiendo el estilo de `WorkbenchError`/`ClassifierError`). `normalize.rs`: función pura `notify::Event → Vec<(PathBuf, EventType, is_dir)>` que aplica la regla de renames (from→Removed, to→Created), mapea `Any`→Modified, descarta `Access`, y deriva `is_dir` del kind tipado o `false`.
- **Patterns to follow:** enums de error de `paths/mod.rs` y `workbench/mod.rs`; docs de módulo en español como los módulos entregados.
- **Test scenarios:**
  - Happy path: Create/Modify/Remove tipados mapean a Created/Modified/Removed con su is_dir.
  - Rename `Both` y par `From`/`To` → Removed(from) + Created(to).
  - `Access` → vector vacío; `Any` → Modified con is_dir=false.
- **Verification:** `cargo test` de normalize en verde; `cargo clippy -- -D warnings` limpio.

### U2. Debounce + throttling por repo (lógica async testeable con reloj pausado)

- **Goal:** La agrupación de ráfagas y el límite de frecuencia por repo, deterministas bajo `tokio::time::pause`.
- **Requirements:** R8, R9, R10; entrega por canal en orden (R11); parte de R12 (tests deterministas de debounce/throttle); AE1, AE3.
- **Dependencies:** U1.
- **Files:** `src-tauri/src/watcher/debounce.rs` (+ tests inline).
- **Approach:** Task async que consume eventos clasificados etiquetados por repo y mantiene, por repo: buffer, deadline de debounce (reinicia con cada evento, ventana ~300 ms constante ajustable), última emisión (throttle ≥ debounce: bajo actividad continua emite el buffer al vencer el intervalo aunque no haya calma) y el flag "rebuild pendiente" (lo setea U3 al ver `.gitignore`; viaja con el lote emitido). Al emitir: `WatcherMessage::Batch` por el `Sender`. Repos independientes: deadlines por entrada de un `HashMap`, un solo `select!` sobre el próximo deadline global (`sleep_until` del mínimo, con rama condicionada `if next.is_some()` para el caso sin deadlines pendientes) para no spawnear una task por repo.
- **Execution note:** test-first con `#[tokio::test(start_paused = true)]` — el reloj pausado es el contrato de testabilidad que motivó la decisión de no usar notify-debouncer.
- **Test scenarios:**
  - Covers AE1. Ráfaga de N eventos dentro de la ventana → un lote con los N eventos.
  - Covers AE3. Actividad continua sin calma → lotes a ritmo del throttle (conteo de lotes con tolerancia, no timing exacto); evento único en otro repo → su lote sale por su propio deadline sin esperar el throttle del primero.
  - Edge: evento que llega justo al vencer la ventana entra al lote siguiente, no se pierde (R9 "ningún evento se pierde").
  - Orden: lotes del mismo repo llegan en orden de emisión (R11).
- **Verification:** `cargo test` con reloj pausado, sin sleeps reales; clippy limpio.

### U3. FsWatcher: notify + clasificación + ciclo de vida

- **Goal:** El watcher real: montaje recursivo del workbench activo, clasificación, rebuild de classifier, errores por repo, remount y shutdown.
- **Requirements:** R1, R2, R3, R4, R5 (wiring), R6; AE2, AE4, AE5, AE6; R12.
- **Dependencies:** U1, U2.
- **Files:** `src-tauri/src/watcher/mod.rs` (FsWatcher + integración), tests de integración FS en el mismo módulo (`#[cfg(test)]`, fixtures con `tempfile` ya disponible).
- **Approach:** `FsWatcher::new()` crea el canal y la task de debounce (requiere contexto tokio — ver KTD); `watch_workbench(&[RepoEntry])` hace diff de repos (unwatch removidos, watch añadidos con `RecursiveMode::Recursive`), construye el `PathClassifier` por repo (con su `fs_watch`); fallo de `watch()` o de classifier → `RepoError` sin abortar los demás (R3). El callback de notify empuja al puente; la task clasifica (descarta `GitInternal`/`Ignored`/`OutsideRepo`), detecta `Removed` del root → sintetiza `RepoError` y desmonta ese repo, **tolerando `notify::ErrorKind::WatchNotFound` en el unwatch** (inotify ya removió el watch — no sintetizar un segundo `RepoError`); evento sobre `.gitignore` → marca rebuild coalescido en la capa de debounce; U3 ejecuta la reconstrucción al recibir la señal con el lote (R6). `shutdown(self)` async desmonta todo y espera la task (R4); `Drop` best-effort (abort).
- **Patterns to follow:** `tempfile` para fixtures (patrón de `git/test_fixtures.rs`); construcción de classifier como en `paths`.
- **Test scenarios:** (integración con FS real; aserciones por contenido de lotes con timeouts generosos, no timing exacto)
  - Covers AE2. Tocar trackeable + ignorado + watchlisted + `.git/HEAD` → lote con Plane1/Plane2/GitMeta, sin Ignored.
  - Covers AE4. `watch_workbench` con set B tras set A → eventos de A dejan de llegar, los de B llegan.
  - Covers AE5. Borrar el root de un repo → `RepoError` sintetizado; el otro repo sigue emitiendo. Path inexistente al montar → `RepoError` de montaje.
  - Covers AE6. Añadir patrón a `.gitignore` → eventos posteriores bajo ese patrón ya no aparecen.
  - Shutdown: tras `shutdown()`, no llegan más mensajes y la task terminó (R4, R12).
- **Verification:** `cargo test` completo en verde (unit + integración), `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo build` en Linux.

## Scope Boundaries

- Igual que el origin: sin recálculo de git (RDM-006 + git engine), sin emisión al frontend ni contrato `invoke`/`emit` (RDM-006), sin UI (RDM-007+), sin editor de `fs_watch` (RDM-009), sin watching de workbenches no activos (diferido), sin wiring del remount al comando `set_active_workbench` (integración → RDM-006), sin persistencia de eventos (memoria solamente).

### Deferred to Follow-Up Work

- Wiring de `FsWatcher` como managed state + llamada a `watch_workbench` desde la conmutación → RDM-006.
- Configurabilidad "todos los workbenches" → cuando haya necesidad real.
- Hook de rebuild del classifier ante edición runtime de `fs_watch` (RDM-009 vía RDM-005) → RDM-009.

## Open Questions

- Ninguna bloqueante. Resoluciones de planificación: debounce propio sobre tokio (KTD, criterio de testabilidad); canal mpsc único con mensaje `Batch | RepoError` (KTD); arranque vacío sin workbench activo (KTD); valores iniciales debounce 300 ms / throttle 1 s como constantes ajustables (tuning en ejecución si los tests de integración lo piden).

## Risks & Dependencies

- **Flakiness de tests de integración FS:** la latencia de entrega de inotify es variable. Mitigación: timing determinista en U2 (reloj pausado); en U3 aserciones por contenido con timeouts generosos y conteo con tolerancia, nunca timing exacto.
- **Límite `max_user_watches` de inotify en repos enormes:** riesgo documentado (origin); no se resuelve aquí. Si un `watch()` falla por límite, cae en la vía R3 (`RepoError`) sin tumbar el resto.
- **Ráfagas `GitInternal` durante operaciones git (checkout/gc):** se descartan pero atraviesan el hot path crudo. Aceptado sin medir (prototype); si pesa, optimizar el descarte de `.git/` antes de clasificar.
- **Backend Windows (`ReadDirectoryChangesW`, kinds `Any`):** cubierto por la normalización conservadora; verificación real best-effort hasta CI (D2).
- **Rebuild del classifier es I/O bloqueante en la task async:** `PathClassifier::new` hace un walk BFS con `std::fs`; corre inline en la task compartida del watcher y puede retrasar lotes de otros repos mientras dura. Aceptado a escala prototype; mover a `spawn_blocking` si los tests de integración muestran estancamiento entre repos.
- **Recuperación tras `RepoError` sintetizado** (repo borrado y restaurado, ¿se remonta solo?): semántica no definida aquí; se congela en RDM-006 con el contrato de consumo.

## Verification Strategy

Escalera local: `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` (U1 normalize + U2 reloj pausado + U3 integración FS) → `cargo build`. Sin humo visual (subsistema interno sin UI); la verificación visual llega con RDM-006/007.
