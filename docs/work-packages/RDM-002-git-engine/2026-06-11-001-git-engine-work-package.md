---
title: Capa de git read-only — trait GitEngine + impl git2-rs
status: ready
roadmap_item: RDM-002
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-11-rdm-002-git-engine-requirements.md
origin_planning_input: docs/brainstorms/2026-06-11-rdm-002-git-engine-requirements.md
origin_plan: docs/plans/2026-06-11-001-feat-git-engine-plan.md
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

# Capa de git read-only — trait GitEngine + impl git2-rs

## Scope

Módulo `git` en el backend Rust: tipos de dominio serializables, `GitError`, trait `GitEngine` (sync, read-only, Send+Sync) y `Git2Engine` (git2-rs vendored), con tests contra repos git temporales reales. Incluye los artefactos de planificación de este item y los docs post-PR pendientes (estado/summary de la corrida anterior) en commits de docs.

## Non-goals

- Escrituras git (design §9). Watcher/clasificador (RDM-003/004). Wrapping async, eventos, contrato frontend (RDM-006). Render de diffs (RDM-008). Impl CLI del escape hatch. Baseline de performance. Cambios de frontend.

## Autonomy Contract

- Mode: guarded (auto-hasta-release-plan autorizado por usuario 2026-06-11 para wave 2)
- Agent may decide: nombres internos, forma exacta de structs/hunks, helpers de fixtures, mapeo fino de errores git2.
- Agent must record: si RepositoryNotFound/NotARepository no son distinguibles (colapsar variante + ajustar AE4), versiones de deps añadidas, cualquier feature extra de git2 necesaria.
- Agent must escalate: cualquier operación de escritura git que parezca necesaria, cambios de contrato fuera del módulo, mutaciones externas, scope fuera del paquete.
- Safe fallback: continuar con unidades no bloqueadas; reportar blocker exacto.
- Autonomous ledger: none. Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-001 mergeado (✔, PR #1; base `develop` existe).
- Blocks: RDM-006, RDM-008, RDM-010 (y soft RDM-005 autodetección).

## Production Posture

- Posture: prototype | Evidence: greenfield, sin usuarios | Confidence: high
- Consequences: velocidad permitida; sin compat retro. Breaking allowed: yes (no hay consumidores aún).

## Plan Unit Alignment

| Plan unit | Included | Reason |
|---|---|---|
| U1 | yes | Contrato (tipos + trait); núcleo del item. |
| U2 | yes | Impl estado/historial; comparte fixtures con U3. |
| U3 | yes | Impl diffs/blobs; mismo archivo que U2. |

Grouping rationale:
- U1–U3 viven en el mismo módulo nuevo (`src-tauri/src/git/`), con dependencia secuencial fuerte y archivos compartidos (`git2_engine.rs`, fixtures) → un review unit integrado. Sin valor en PRs apilados.
- Docs de planificación + estado/summary post-PR de la corrida anterior van en commit docs separado (regla branch/docs).

## Implementation Units

- U1. Tipos + GitError + trait GitEngine (rustdoc con escape hatch y nota de paths) — R1–R8 formas, R11.
- U2. Git2Engine: open/status/branch_info/head_commit/log + fixtures — R1–R4, R7; AE1/AE2/AE4; edges unborn y discriminación de errores.
- U3. Git2Engine: worktree_diff/commit_diff/blob_at — R5, R6, R8; AE3; edges binario y untracked-excluido.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Módulo git completo + tests + docs | runtime Rust (src-tauri/src/git/, lib.rs, Cargo.toml), generated (Cargo.lock), docs (docs/**) | develop | optional: sin contexto Jira → omitted | ~500–700 líneas humanas (módulo+tests); Cargo.lock generado separado; riesgo bajo-medio (dep nativa vendored) |

## Files and Tests

- Código: `src-tauri/src/git/mod.rs`, `src-tauri/src/git/git2_engine.rs`, `src-tauri/src/git/test_fixtures.rs`, `src-tauri/src/lib.rs` (mod decl), `src-tauri/Cargo.toml` (git2 vendored, thiserror; tempfile dev-dep).
- Tests: inline `#[cfg(test)]` en el módulo git (fixtures reales con tempfile + git2).
- Docs (commit aparte): `docs/brainstorms/2026-06-11-*`, `docs/plans/2026-06-11-*`, `docs/work-packages/RDM-002-git-engine/`, `docs/orchestration/*` (estado + summary pendientes).

## Impact Scan

- Changed contracts: nace el contrato de tipos del módulo git (interno, sin consumidores aún — RDM-006 será el primero).
- Consumer scan patterns: None (no hay consumidores; verificado por construcción — módulo nuevo).
- Contract-drift tests searched: None previos.
- Required consumer tests / run: None — n/a.

## Verification Gate

Estado 2026-06-11 (ejecución RU1): **PASS**

- [x] `cargo test` 24/24 en verde (módulo git completo: AE1–AE4, unborn en todos los métodos, staged+re-modificado, borrado, untracked recursivo, binarios, multi-archivo, merge first-parent, paginación y límites, blob anidado) — fixtures reales con timestamps determinísticos.
- [x] `cargo fmt --check` y `cargo clippy --all-targets -- -D warnings` limpios.
- [x] `npm test` 3/3 y `npm run lint` sin regresión (frontend intacto).
- [x] libgit2 vendored compiló limpio con cc + MSVC (sin cmake, confirmando el ajuste del plan); git2 0.20 default-features=false.
- Production posture: prototype; gap CI-only Linux registrado (D2).

## Review Gate

- Threshold P0-P2; debajo → log.
- **Resultado 2026-06-11:** 3 personas (correctness, testing, adversarial; ~20 hallazgos). Aplicados: 7 tests nuevos (dual-list staged+modified — claim del rustdoc sin test; borrado; unborn-todos-los-métodos que ATRAPÓ un bug real: log() en unborn devolvía Internal en vez de UnbornHead, corregido; offset>historial; blob anidado; multi-archivo hunk-association; merge first-parent) + timestamps determinísticos en fixtures (anti-flake CI). Descartados con announce: TOCTOU en open() (quitar exists() rompería la distinción de errores tipados), openssl-en-Linux (default-features=false ya excluye https/ssh), mojibake/fd-pooling/memoria-unbounded/cursor-pagination → diferidos a RDM-006/008 como residual notes. **PASS.**

## Security Gate

- Run after work-review loop: not required — sin auth/secrets/PII/API pública. Superficie de dependencia: git2/libgit2 vendored (crate mainstream, build local); registrar versión.
- Security Watch during work: disabled; worker reporta si algo inesperado aparece.

## CI Break-Prevention And Escalation

- CI risk surfaces: build Rust (dep nativa vendored), tests. Sin CI aún (D2); evidencia local + gap Linux explícito.
- If CI breaks: krt-ci-questor cuando exista CI.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Capa de git read-only
- Branch name: `feat/git-read-engine`
- Branch/docs rule: lleva docs de planificación + estado/summary post-PR previos en commit docs.
- PR base: `develop`
- Suggested commit grouping:
  - `feat(git): trait GitEngine y tipos de dominio del plano git` — `src-tauri/src/git/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` — contrato de la capa.
  - `feat(git): Git2Engine con status, historial, diffs y blobs` — `src-tauri/src/git/git2_engine.rs`, `src-tauri/src/git/test_fixtures.rs` — implementación + tests.
  - `chore(generated): lockfile actualizado por git2/thiserror/tempfile` — `src-tauri/Cargo.lock`.
  - `docs(orquestación): artefactos de la capa git y cierre de la entrega anterior` — `docs/**`.
- PR title: `feat: capa de lectura de git detrás del trait GitEngine`
- PR body bullets:
  - Trait GitEngine read-only: status, branch y ahead/behind, último commit, log paginado, blobs y diffs estructurados (working tree y por commit).
  - Implementación Git2Engine con git2-rs (libgit2 vendored), errores tipados sin panics y detección de binarios.
  - Tests contra repos git temporales reales: happy paths, repos unborn, paths no-git, binarios y untracked excluido del diff.
  - Documentado el escape hatch a CLI git para repos grandes (criterio pendiente de baseline).
- Verification results location: Verification Gate de este paquete + hilo de ejecución.
- Production/deployment notes: none (prototype local).
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional | fallback: "Jira omitted: checker ok:false (env-loaded-without-project-secret-file)".
- Suggested issue type: Tarea (standalone; sin parent con hijo único).
- Jira summary: Capa de lectura de git con trait y soporte de status, historial y diffs
- Jira description: Implementar la capa de acceso de solo lectura a repositorios git detrás de una interfaz intercambiable: estado del working tree, rama y divergencia con el remoto, historial paginado, contenido de archivos por commit y diffs estructurados, con manejo de errores tipado y pruebas sobre repositorios reales temporales. Habilita el bus de eventos, el visor de diffs y el timeline.
