---
title: Watcher de FS con debounce y throttling por repo
status: shipped-merged
roadmap_item: RDM-004
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-11-rdm-004-watcher-requirements.md
origin_planning_input: docs/brainstorms/2026-06-11-rdm-004-watcher-requirements.md
origin_plan: docs/plans/2026-06-11-004-feat-fs-watcher-plan.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Watcher de FS con debounce y throttling por repo

## Scope

Implementar el módulo `watcher` del backend según el plan origen: normalización de eventos de notify (U1), debounce + throttling por repo con timers de tokio testeables con reloj pausado (U2), y el `FsWatcher` real con montaje del workbench activo, clasificación vía `PathClassifier`, rebuild coalescido del classifier, errores por repo por el canal, remount por API y shutdown limpio (U3). Incluye los artefactos de planificación de RDM-004 en la misma rama (regla branch/docs).

## Non-goals

- Recálculo de git, contrato `invoke`/`emit`, coalescing de deltas y wiring del watcher como managed state (RDM-006).
- UI (RDM-007+), editor de `fs_watch` (RDM-009).
- Watching de workbenches no activos (diferido por decisión de usuario 2026-06-11).
- Persistencia de eventos (memoria solamente).

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: nombres internos, organización de tests, valores iniciales de debounce/throttle dentro de ~200–400 ms / ≥debounce, detalles de implementación que sigan los KTDs del plan.
- Agent must record as assumptions: versión de `notify` resuelta, cualquier divergencia del backend de notify observada en tests, ajustes de tolerancia en tests de integración.
- Agent must escalate: cambios de comportamiento de producto, ampliación de scope (p. ej. watching de todos los workbenches), cualquier mutación externa (push, PR, Jira), modificación de los módulos entregados (`paths`, `workbench`, `git`) más allá de `lib.rs` (`pub mod watcher;`).
- Safe fallback: U1 y U2 no dependen de FS real; ante un blocker en U3 (comportamiento de notify), reportar el blocker exacto con el test que lo evidencia.
- Autonomous ledger: none
- Allowed external mutation classes: none (autonomy:guarded sin ledger → solo autonomía local).

## Dependencies

- Requires: RDM-001 ✅, RDM-003 (`PathClassifier`) ✅, RDM-005 (`WorkbenchStore`/RepoEntry) ✅ — todo en `develop`.
- Blocks: RDM-006 (bus), RDM-009 (Plano 2 UI).

## Production Posture

- Posture: prototype
- Evidence: greenfield sin usuarios; el consumidor del canal (RDM-006) no existe aún.
- Confidence: high
- Consequences for this package: velocidad permitida; el contrato del canal es interno y se congela en RDM-006.
- Breaking existing behavior allowed: yes (módulo nuevo sin consumidores).

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Tipos + normalización; base de U2/U3. |
| U2 | yes | Debounce/throttle; mismo módulo. |
| U3 | yes | Watcher real; integra U1+U2 sobre los mismos archivos. |

Grouping rationale:
- U1–U3 forman un subsistema acoplado (mismo módulo `watcher/`, dependencia secuencial fuerte); separarlos daría PRs apilados sin valor independiente — mismo criterio que RDM-003/005 (un módulo = un PR). Review unit única.
- Artefactos de planificación de RDM-004 (brainstorm, plan, package, estado de orquestación, backlog del fetch, anotación del roadmap) viajan en esta rama en commits de docs separados.
- `Cargo.lock` (generado por `notify` + features tokio) en commit aparte.

## Implementation Units

- U1. Tipos del canal (`WatcherMessage::Batch|RepoError`, `ClassifiedEvent`, `WatcherError`) + normalización de kinds de notify (renames → Removed+Created; `Any`→Modified; `Access` descartado; `is_dir` del kind tipado o `false`).
- U2. Debounce (~300 ms, reinicia por evento) + throttle (≥debounce) por repo, una task con `select!` sobre el deadline mínimo; flag rebuild-pendiente viaja con el lote; tests con `#[tokio::test(start_paused = true)]`.
- U3. `FsWatcher`: `new()` (contexto tokio), `watch_workbench` (diff de repos, classifier por repo, errores de montaje → `RepoError`), clasificación y descartes, síntesis de `RepoError` ante borrado del root (tolerando `WatchNotFound`), rebuild coalescido, `shutdown(self)` async + `Drop` best-effort; tests de integración FS con `tempfile`.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Módulo watcher completo (U1–U3) + docs de planificación | runtime (`src-tauri/src/watcher/**`, `src-tauri/src/lib.rs`), deps (`src-tauri/Cargo.toml` + `Cargo.lock`), tests (inline), docs (`docs/**`) | develop | optional: sin contexto Jira → "Jira omitted" | ~500–700 líneas Rust autoría humana (incl. tests); lockfile y docs en commits aparte; riesgo medio (concurrencia/timing — mitigado por reloj pausado en U2 y aserciones tolerantes en U3) |

## Files and Tests

- Código: `src-tauri/src/lib.rs` (`pub mod watcher;`), `src-tauri/src/watcher/mod.rs`, `src-tauri/src/watcher/normalize.rs`, `src-tauri/src/watcher/debounce.rs`, `src-tauri/Cargo.toml` (`notify` pineado, tokio features `time,sync,macros,rt` + dev `macros,rt,test-util`).
- Tests: inline por unidad — normalize (tabla de kinds), debounce/throttle (reloj pausado, conteo de lotes con tolerancia), integración FS (`tempfile`): AE2 clasificación, AE4 remount, AE5 errores, AE6 rebuild, shutdown.
- Docs (commits aparte): `docs/brainstorms/2026-06-11-rdm-004-*`, `docs/plans/2026-06-11-004-*`, `docs/work-packages/RDM-004-watcher/`, `docs/orchestration/compound-master-state.md`, `docs/backlog/2026-06-11-fetch-opt-in-backlog.md`, anotación en `docs/roadmaps/2026-06-10-001-tinto-roadmap.md`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: contrato nuevo interno (`WatcherMessage`/`ClassifiedEvent`); sin consumidores aún (RDM-006 lo consumirá). `lib.rs` solo gana `pub mod watcher;`. `Cargo.toml` gana `notify` y features de tokio (aditivo; `tokio` ya unificaba versión con Tauri).
- Consumer scan patterns: `rg "watcher::|WatcherMessage"` en `src/`, `src-tauri/src/` → ninguno.
- Consumers found: None.
- Contract-drift tests searched: suites existentes (`git`, `paths`, `workbench`, vitest) no dependen del watcher; no hay snapshots/allowlists afectados.
- Required consumer tests: None nuevos; la suite completa (`cargo test`, `npm test`) debe seguir verde (features de tokio son aditivas).
- Consumer tests run/skipped: se corren como parte del Verification Gate (suite completa).

## Verification Gate

Estado 2026-06-11 (ejecución RU1, Linux — re-verificado tras fixes de review):

- [x] `cargo fmt --check` limpio
- [x] `cargo clippy --all-targets -- -D warnings` limpio (2 lints reales corregidos en ejecución: `replace_box`, `ptr_arg`)
- [x] `cargo test` **81/81** (53 previos + 7 normalize + 11 debounce con reloj pausado en 0.00s + 10 mod/integración FS en ~2.6s)
- [x] `npm install` limpio en este checkout (0 vulnerabilidades, sin lifecycle scripts); `npm test` 3/3; `npm run lint` verde
- [x] `cargo build` en Linux con `notify` 8
- Surface-aware evidence: runtime Rust → cargo test + clippy; deps → build con `notify`; concurrencia/timing → tests deterministas U2 + integración tolerante U3.
- Production posture evidence: prototype — sin compatibilidad retro; Windows best-effort hasta CI (D2), gap CI-only registrado.

## Review Gate

- Code review threshold: P0-P2 (default).
- Findings below threshold: log unless user marks blocking.
- **Resultado 2026-06-11:** 4 personas (correctness, testing, maintainability, adversarial). **Fixes aplicados (P1/P2):** (1) remount tras `RepoRemoved` funcionaba como no-op por estado `mounted` divergente → set compartido `dead_roots` y remount real (3 personas, P1); (2) unwatch de roots solapados/anidados envenenaba el subtree compartido de notify → re-assert del watch de roots retenidos solapados (P1); (3) overflow de cola del kernel (`Flag::Rescan`) se tragaba → nueva variante `WatcherMessage::RescanNeeded` por repo montado (P1); (4) eventos perdidos entre `watch()` y `AddRepo` → clasificador viaja ANTES de registrar el watch (FIFO) (P2); (5) paths sin canonicalizar rompían el routing por prefijos → canonicalización en el borde de `watch_workbench` (P2); (6) leak de watches parciales ante fallo de montaje (ENOSPC) → unwatch best-effort + rollback (P2); (7) `GitignoreTouched` disparado por `.gitignore` en dirs ignorados → solo si clasifica `Plane1` (P2); (8) `new()` panicaba ante fallo ambiental de notify → `Result` con `WatcherError::BackendInit` (P2); (9) `shutdown()` descartaba lotes en ventana abierta → flush final real (no se desmonta repo por repo) (P3↑); (10) lote rezagado de repo desmontado se reenviaba → filtro de membresía en `forward_batch` (P3); (11) rebuild fallido dejaba clasificador viejo sin retry → re-arma `GitignoreTouched` (P3); (12) re-export muerto `NormalizedEvent` removido + imports consolidados (safe_auto). **Tests añadidos por review:** piso de throttle (negativo "el lote NO sale aún"), borde de ventana, shutdown con pendientes, cambio de `fs_watch` (rebuild sin remount), remount tras `RepoRemoved`, rescan sintético, timestamp plausible, ae4 por contenido, phantom path aislado. **Descartado con announce:** rebuild inline bloqueante (riesgo aceptado en plan; `spawn_blocking` como salida si pesa), rename de `watch_workbench` (advisory), Serialize de `WatcherError` (contrato de RDM-006). Re-verificación post-fixes: 81/81, clippy, fmt, build. **PASS.**

## Security Gate

- Run after work-review loop: not required — sin auth, secrets, red, PII ni API pública. Superficie: lectura de paths locales ya monitoreados y metadata de eventos.
- Security Watch during work: disabled — sin superficie de alto riesgo; el worker eleva si aparece algo inesperado (p. ej. symlinks fuera del root del repo en el watch recursivo).
- Security Watch notes: nota preventiva — los paths de eventos provienen del FS observado (no confiable en teoría) pero solo se clasifican y reenvían, sin abrirse ni ejecutarse; el clasificador ya trata `OutsideRepo`.
- Security reviewer: n/a (gate no requerido; krt-security-sentinel disponible si el worker eleva).
- Security review result: not required.
- Required security verification: ninguna específica.

## CI Break-Prevention And Escalation

- CI risk surfaces: build (notify + features tokio), clippy, tests (timing). No hay CI aún (D2).
- Preventive evidence: escalera local completa; el riesgo clásico de CI (tests de timing flaky) se mitiga por diseño (reloj pausado en U2; tolerancia en U3). Gap CI-only: backend Windows de notify.
- If CI breaks: invocar krt-ci-questor con contexto de PR/run/check.
- Escalation rule: registrar blocker de release-follow-up si los tests de integración FS resultan flaky en local (señal de que lo serán más en CI).

## Branch and PR Handoff Inputs

- Review unit: RU1 — Watcher de FS con debounce y throttling por repo
- Branch name: `feat/fs-watcher` (ya creada desde `develop`; lleva el commit de reconciliación de docs de esta corrida)
- Branch/docs rule: esta rama lleva los artefactos de planificación de RDM-004 + docs de reconciliación (commits separados); no se crea rama `docs/*-planning`.
- PR base: `develop`
- Suggested commit grouping:
  - `feat(watcher): normalización de eventos y tipos del canal` — `watcher/normalize.rs`, tipos en `watcher/mod.rs`, `lib.rs`, `src-tauri/Cargo.toml` (notify + features tokio) — el contrato de eventos.
  - `feat(watcher): debounce y throttling por repo con timers testeables` — `watcher/debounce.rs` — la lógica de agrupación.
  - `feat(watcher): FsWatcher con montaje por workbench, clasificación y ciclo de vida` — `watcher/mod.rs` + tests de integración — la capacidad integrada.
  - `chore(generated): actualiza Cargo.lock` — `src-tauri/Cargo.lock`.
  - `docs(orquestación): brainstorm, plan y paquete del watcher` — `docs/**`.
  - (ya en la rama: `docs(orquestación): resume Linux, reconciliación...` — commit f7ea5d5.)
- PR title: `feat: watcher de archivos con debounce y throttling por repo`
- PR body bullets:
  - Watcher `notify` sobre los repos del workbench activo, con eventos normalizados (renames descompuestos, kinds desconocidos a "modificado") y clasificados con el clasificador de paths existente.
  - Debounce por repo (~300 ms) que agrupa ráfagas de agentes en un lote, y throttling que acota la frecuencia bajo actividad continua; repos independientes entre sí.
  - Canal interno con lotes de eventos clasificados y errores por repo (fallo de montaje y remoción en caliente detectada), listo para que el bus de estado lo consuma.
  - Remontaje por API al conmutar workbench y apagado limpio verificado; tests deterministas con reloj pausado de tokio + integración con FS real.
  - Incluye artefactos de planificación y la reconciliación de orquestación de la corrida en commits de docs separados.
- Verification results location: sección Verification Gate de este paquete + salida de comandos en el hilo de ejecución.
- Production/deployment notes: none (prototype local).
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: tarea única standalone (un solo RU).
- Jira summary: Observador de archivos con agrupación de ráfagas por repositorio
- Jira description: Construir el observador de archivos que vigila los repositorios del espacio de trabajo activo, clasifica cada cambio (versionado, vigilado u omitido), agrupa las ráfagas de escritura de los agentes en lotes y limita la frecuencia por repositorio para no saturar la interfaz. Entrega los lotes por un canal interno que el bus de estado consumirá, reporta errores por repositorio sin detener el resto y se desmonta limpiamente al conmutar de espacio de trabajo.
- Optional-policy fallback: sin keys/URLs Jira ni `krt-jira-scribe` configurado → registrar "Jira omitted: sin contexto/config Jira en preflight" y continuar.
