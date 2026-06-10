---
title: Workbench manager con persistencia TOML y autodetección
status: ready
roadmap_item: RDM-005
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-11-rdm-005-workbench-manager-requirements.md
origin_planning_input: docs/brainstorms/2026-06-11-rdm-005-workbench-manager-requirements.md
origin_plan: docs/plans/2026-06-11-003-feat-workbench-manager-plan.md
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

# Workbench manager con persistencia TOML y autodetección

## Scope

Módulo `workbench`: modelo serde + store TOML atómico en config dir inyectable (`dirs` en producción), CRUD de workbenches/repos con validación vía Git2Engine, autodetección BFS acotada de repos git, y comandos Tauri delgados con errores serializables. Tests con config dir temporal. Incluye docs de planificación + estado de compound master actualizado (regla de cierre por unidad).

## Non-goals

UI (RDM-007/009), watcher/eventos (RDM-004/006), SQLite, migraciones de config.

## Autonomy Contract

- Mode: guarded (auto-hasta-release-plan; merge pre-autorizado por usuario si el PR incluye estado actualizado).
- Decide: forma interna de structs/comandos, nombres de campos TOML dentro del esquema del diseño, helpers de test.
- Record: semántica de rename verificada por test, lista final de dirs excluidos del scan.
- Escalate: cambios al esquema TOML visible del diseño §8, mutaciones externas fuera del release plan, scope externo.
- Ledger: none. Mutation classes: none.

## Dependencies

- Requires: RDM-001 (✔), RDM-002 (✔ — validación con Git2Engine).
- Blocks: RDM-006, RDM-007.

## Production Posture

prototype | greenfield | high | breaking allowed: yes.

## Plan Unit Alignment

| Plan unit | Included | Reason |
|---|---|---|
| U1 | yes | Modelo+store; núcleo. |
| U2 | yes | CRUD+autodetección; mismo módulo. |
| U3 | yes | Comandos; wiring en lib.rs. |

Grouping rationale: un módulo cohesivo con dependencia secuencial; un RU. Docs+estado en commit aparte (regla de cierre por unidad).

## Implementation Units

- U1. Modelo + store TOML atómico (R1–R5; AE1–AE2).
- U2. CRUD + set_active + autodetect BFS (R6–R9; AE3–AE4).
- U3. Comandos Tauri + State<Mutex> + WorkbenchError Serialize (R10–R11).

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Módulo workbench + comandos + tests + docs | runtime Rust (workbench/, lib.rs, Cargo.toml), generated (Cargo.lock), docs | develop | optional → omitted | ~450–650 líneas humanas; riesgo bajo-medio (persistencia a disco) |

## Files and Tests

- `src-tauri/src/workbench/{mod.rs,autodetect.rs,commands.rs}`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` (dirs, toml), `src-tauri/Cargo.lock`.
- Tests inline con tempdir como config dir y repos git reales para validación/autodetección.
- Docs: brainstorm/plan/package RDM-005 + compound-master-state actualizado.

## Impact Scan

- Changed contracts: nacen los comandos invoke de workbench (primer consumidor: RDM-007). El smoke ping/tick se conserva intacto.
- Consumers found: None aún. Drift tests: None. Consumer tests: n/a.

## Verification Gate

Estado 2026-06-11: **PASS**

- [x] `cargo test` 53/53 (17 del workbench: AE1–AE4, activo colgante, duplicados, reorden total y parcial, backup `.corrupt`, worktrees y profundidad en autodetect).
- [x] `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` limpios.
- [x] `npm test` 3/3 + `npm run lint` sin regresión.
- [x] Round-trip TOML con paths Windows reales (tempdir) y doble escritura (rename reemplaza destino).

## Review Gate

- Threshold P0-P2; debajo → log.
- **Resultado 2026-06-11:** review dual (correctness+adversarial), 9 hallazgos. Aplicados (4×P1+2×P2): tmp por-PID (colisión multi-instancia), backup `.corrupt` antes del primer persist degradado, mutex envenenado → `StoreLocked` tipado (sin panic en el event loop), contrato `update_repo` con `alias`+`clear_alias` (el doble-Option no sobrevive JSON), comentario de profundidad, test de reorden parcial. Descartado con announce: "panic" de PathBuf no-UTF8 (serde devuelve Err tipado, no panic). Residual: case-sensitivity de paths Windows en duplicados (canonicalización → futuro), recuperación multi-instancia depende de rename atómico del FS. **PASS.**

## Security Gate

- Not required como gate separado, con una nota: la config persiste paths del usuario (no secretos); los comandos no exponen lectura arbitraria de FS más allá de listar dirs candidatos de la raíz que el usuario elige. Watch: disabled.

## CI Break-Prevention And Escalation

- Surfaces: build/tests Rust. Sin CI (D2); evidencia local; gap Linux explícito.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Workbench manager
- Branch name: `feat/workbench-config`
- PR base: `develop`
- Suggested commit grouping:
  - `feat(workbench): modelo y persistencia TOML atómica de workbenches` — `workbench/mod.rs`, `lib.rs`, `Cargo.toml` — modelo+store.
  - `feat(workbench): autodetección de repos y comandos para el frontend` — `workbench/autodetect.rs`, `workbench/commands.rs`, `lib.rs` — operaciones expuestas.
  - `chore(generated): lockfile actualizado por dirs y toml` — `Cargo.lock`.
  - `docs(orquestación): artefactos del workbench manager y estado` — `docs/**`.
- PR title: `feat: workbenches persistentes con autodetección de repos`
- PR body bullets:
  - Workbenches nombrados con repos, alias y patrones de vigilancia por repo, persistidos en TOML en el directorio de configuración del SO con escritura atómica.
  - Operaciones completas: crear/renombrar/eliminar workbenches, gestionar repos con validación git real, conmutar el workbench activo y autodetectar repos bajo una carpeta raíz.
  - Comandos expuestos al frontend con errores tipados serializables; configuración corrupta se reporta sin sobrescribir el archivo.
  - Tests sobre disco real: round-trip, config corrupta intacta, autodetección con repos anidados y worktrees.
- Production/deployment notes: none. Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional | fallback: "Jira omitted: checker ok:false (env-loaded-without-project-secret-file)".
- Issue type: Tarea standalone.
- Jira summary: Gestión persistente de espacios de trabajo con autodetección de repositorios
- Jira description: Implementar los espacios de trabajo del monitor: conjuntos nombrados de repositorios con alias y patrones de vigilancia, guardados en la configuración del sistema operativo con escritura segura, más la detección automática de repositorios bajo una carpeta elegida y las operaciones que la interfaz necesitará para crearlos, editarlos y conmutarlos.
