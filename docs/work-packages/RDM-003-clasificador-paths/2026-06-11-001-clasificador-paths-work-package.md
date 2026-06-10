---
title: Clasificador de paths en tres buckets
status: ready
roadmap_item: RDM-003
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-11-rdm-003-clasificador-paths-requirements.md
origin_planning_input: docs/brainstorms/2026-06-11-rdm-003-clasificador-paths-requirements.md
origin_plan: docs/plans/2026-06-11-002-feat-clasificador-paths-plan.md
units: [U1, U2]
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

# Clasificador de paths en tres buckets

## Scope

Módulo `paths` puro: `PathClassifier` (repo_root + watchlist) → `Classification` {GitInternal, GitMeta, Plane1, Plane2, Ignored, OutsideRepo} con `ignore` + `globset`, cero I/O en classify (is_dir lo pasa el caller). Tests de tabla AE1–AE4 + edges. Incluye docs de planificación del item.

## Non-goals

Watcher (RDM-004), estado git/status (RDM-002/006), persistencia de watchlist (RDM-005), UI.

## Autonomy Contract

- Mode: guarded (auto-hasta-release-plan, wave 2).
- Decide: forma interna de structs, helpers de test, mensajes de error.
- Record: cualquier desviación del comportamiento del crate `ignore` descubierta por AE4 (spike), versiones de deps.
- Escalate: si `ignore` no soporta el scoping anidado (re-plan), mutaciones externas, scope fuera del paquete.
- Fallback: reportar blocker exacto.
- Ledger: none. Mutation classes: none.

## Dependencies

- Requires: RDM-001 (✔). Soft: comparte repo con RDM-002 (✔ mergeado).
- Blocks: RDM-004, RDM-006.

## Production Posture

prototype | greenfield | high | velocidad permitida | breaking allowed: yes.

## Plan Unit Alignment

| Plan unit | Included | Reason |
|---|---|---|
| U1 | yes | Tipos+builder+reglas .git. |
| U2 | yes | Reglas ignore+watchlist; mismo archivo. |

Grouping rationale: un módulo puro pequeño con dependencia secuencial; un RU integrado. Docs en commit aparte.

## Implementation Units

- U1. Tipos, builder, `.git`/HEAD/index, OutsideRepo — R1/R2/R8/R9, AE1.
- U2. gitignore anidado + watchlist globset — R3–R7, AE2–AE4.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Módulo paths + tests + docs | runtime Rust (src-tauri/src/paths/, lib.rs, Cargo.toml), generated (Cargo.lock), docs | develop | optional → omitted | ~250–400 líneas humanas; riesgo bajo (lógica pura) |

## Files and Tests

- `src-tauri/src/paths/mod.rs` (tipos+lógica+tests inline), `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` (ignore, globset), `src-tauri/Cargo.lock` (generado).
- Docs: brainstorm/plan/package de RDM-003.

## Impact Scan

- Changed contracts: nace `Classification` (interno; consumidores futuros RDM-004/006). Consumers found: None. Drift tests: None. Consumer tests: n/a.

## Verification Gate

Estado 2026-06-11: **PASS**

- [x] `cargo test` 36/36 (12 tests del módulo paths: AE1–AE4, negaciones en dir no-ignorado vs dentro de dir ignorado, borrados bajo patrón de directorio, watchlist inválida nombrada, OutsideRepo, abs/rel equivalentes).
- [x] `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` limpios.
- [x] `npm test` 3/3 + `npm run lint` sin regresión.
- Nota: AE4 funcionó como spike y ATRAPÓ que `GitignoreBuilder` escopa a su root (no por archivo añadido) → rediseño a un matcher por directorio con poda BFS.

## Review Gate

- Threshold P0-P2; debajo → log.
- **Resultado 2026-06-11:** review dual (correctness+adversarial). Aplicados: poda del walk con matchers incrementales (replica semántica git: negaciones dentro de dirs ignorados no des-ignoran; y evita recorrer node_modules al construir), normalización única de separadores antes de todos los matchers, 3 tests nuevos. Confirmado por test existente: el crate evalúa ancestros como dirs (borrados con is_dir=false bajo `target/` siguen ignorados). Residual notes: .git-como-archivo (worktrees) → GitInternal aceptable v1; symlink escapes asumen higiene de notify. **PASS.**

## Security Gate

- Not required: lógica pura sin superficies sensibles. Watch: disabled.

## CI Break-Prevention And Escalation

- Surfaces: build/tests Rust. Sin CI (D2): evidencia local; gap Linux explícito.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Clasificador de paths
- Branch name: `feat/path-classification`
- PR base: `develop`
- Suggested commit grouping:
  - `feat(paths): clasificador de eventos en buckets git/plano1/plano2` — `src-tauri/src/paths/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` — capacidad integrada con tests.
  - `chore(generated): lockfile actualizado por ignore y globset` — `src-tauri/Cargo.lock`.
  - `docs(orquestación): artefactos del clasificador de paths` — `docs/**`.
- PR title: `feat: clasificador de paths para enrutar eventos del watcher`
- PR body bullets:
  - Clasificación de cada path de evento FS en: interno de git, señal de metadata (HEAD/index), plano 1 (trackeable), plano 2 (gitignoreado vigilado) o descarte.
  - Respeta .gitignore anidados vía el crate ignore y patrones opt-in de vigilancia vía globset; cero I/O por evento.
  - Tests de tabla cubriendo los buckets, anidamiento, watchlist vacía, paths fuera del repo y borrados.
- Production/deployment notes: none. Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional | fallback: "Jira omitted: checker ok:false (env-loaded-without-project-secret-file)".
- Issue type: Tarea standalone.
- Jira summary: Clasificador de rutas para enrutar eventos del monitor de archivos
- Jira description: Implementar la lógica que decide, para cada cambio detectado en disco, si pertenece al plano de archivos versionados, al plano de archivos vigilados explícitamente, a señales internas de git o si debe descartarse, respetando las reglas de exclusión del repositorio. Es el filtro que evita procesar carpetas pesadas y habilita el watcher y el bus de eventos.
