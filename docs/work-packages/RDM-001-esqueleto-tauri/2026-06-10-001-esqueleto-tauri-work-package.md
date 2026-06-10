---
title: Esqueleto Tauri 2 + React + tooling base
status: implemented-verified-awaiting-release
roadmap_item: RDM-001
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md
origin_planning_input: docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md
origin_plan: docs/plans/2026-06-10-001-feat-esqueleto-tauri-react-plan.md
units: [U1, U2, U3, U4]
unit_alignment: complete
review_units: [RU1]
base_branch: main
pr_strategy: independent
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Esqueleto Tauri 2 + React + tooling base

## Scope

Implementar el esqueleto de Tinto en la raíz del repo según el plan origen: scaffold Tauri 2 + React/TS (Vite), tooling de calidad local (ESLint/Prettier, rustfmt/clippy, Vitest, cargo test), endurecimiento npm (`ignore-scripts=true`), y puente webview↔Rust de humo (comando `ping` + evento `tick` vía `tauri::async_runtime`). Incluye los artefactos de planificación de esta iniciativa en la misma rama (regla branch/docs).

## Non-goals

- Lógica de git, watcher, clasificador, workbenches, UI de producto (RDM-002..012).
- Workflow de CI (D2 resuelta: diferido a item posterior).
- Operaciones git de escritura sobre repos observados (no-goal global §9 del diseño).
- Creación del repo GitHub/push: la ejecuta la fase de release, no el worker.

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: nombres internos, estructura de módulos dentro del layout estándar, versiones exactas que traiga el template estable, configuración equivalente de lint/test, contenido del payload de humo.
- Agent must record as assumptions: versión de React/Vite scaffoldeada, cualquier excepción a `ignore-scripts` detectada, ajustes de clippy con `#[allow]` justificado.
- Agent must escalate: cambios de comportamiento de producto, alteración del layout estándar de Tauri, necesidad de postinstall scripts no documentada, cualquier mutación externa (remote, push, PR, Jira), scope fuera de este paquete.
- Safe fallback: continuar con unidades no bloqueadas (p. ej. tooling antes que bridge), reportar el blocker exacto.
- Autonomous ledger: none
- Allowed external mutation classes: none (autonomy:guarded sin ledger → solo autonomía local).

## Dependencies

- Requires: None (Wave 1; primer paquete del programa).
- Blocks: RDM-002, RDM-003, RDM-005 (y transitivos).

## Production Posture

- Posture: prototype
- Evidence: repo greenfield sin usuarios ni deploys; diseño lo define como app local nueva.
- Confidence: high
- Consequences for this package: velocidad permitida; sin requisitos de compatibilidad retro.
- Breaking existing behavior allowed: yes (no hay comportamiento previo).

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Scaffold base; núcleo del paquete. |
| U2 | yes | Tooling sobre los mismos archivos del scaffold. |
| U3 | yes | Puente de humo; toca `lib.rs`/`App.tsx` creados en U1. |
| U4 | partial | Solo como inputs de handoff: la creación de remote/push es mutación externa de la fase release (krt-release-marshal). |

Grouping rationale:
- U1–U3 comparten archivos núcleo (`package.json`, `src-tauri/src/lib.rs`, `src/App.tsx`) y dependencia secuencial fuerte; separarlos produciría PRs apilados ruidosos sin valor independiente (criterio de combinación del template). Un solo review unit integrado.
- Los archivos generados (p. ej. `package-lock.json`, íconos del template) se agrupan en commit separado dentro del mismo RU.
- Los artefactos de planificación (`docs/roadmaps/`, `docs/brainstorms/`, `docs/plans/`, `docs/work-packages/`, `docs/orchestration/`) viajan en esta misma rama por la regla "first executable review unit carries related planning artifacts"; commit separado de docs.

## Implementation Units

- U1. Scaffold Tauri 2 + React-TS en la raíz (R1–R4): create-tauri-app react-ts vía dir temporal → raíz; `.npmrc` ignore-scripts antes de `npm install`; tokio (`time`) + `tauri::async_runtime` como patrón async.
- U2. Tooling de calidad local (R7–R11): ESLint flat + Prettier; Vitest jsdom + testing-library con `setupTests.ts`; clippy/fmt limpios; `.gitignore`.
- U3. Puente de humo invoke+emit (R5–R6, AE1–AE2): comando `ping` con payload `{message, timestamp_ms}`; task en `setup` que emite `tick` ~1s; frontend muestra ambos; tests con `@tauri-apps/api/mocks` (`mockIPC`).
- U4. Preparación de entrega (R12, AE3): solo inputs de handoff (abajo); ejecución en fase release.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Esqueleto completo + tooling + puente de humo + docs de planificación | runtime (src/, src-tauri/), config (package.json, vite, eslint, npmrc, gitignore), tests (App.test.tsx, lib.rs), generated (package-lock.json, íconos template), docs (docs/**) | main | optional: sin contexto Jira detectado → "Jira omitted" salvo que aparezca config | ~300–500 líneas autoría humana; generated y docs en commits separados (sí separado); riesgo bajo |

## Files and Tests

- Código: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `.npmrc`, `.gitignore`, `eslint.config.js`, `.prettierrc`, `src/main.tsx`, `src/App.tsx`, `src/App.test.tsx`, `src/setupTests.ts`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`.
- Tests: `src/App.test.tsx` (Vitest, AE1/AE2 con mocks), tests unitarios de payload en `src-tauri/src/lib.rs` (`cargo test`).
- Docs (mismo RU, commit aparte): `docs/roadmaps/`, `docs/brainstorms/`, `docs/plans/`, `docs/work-packages/`, `docs/orchestration/`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: None — repo greenfield; el contrato `ping`/`tick` nace aquí y aún no tiene consumidores externos.
- Consumer scan patterns: None (no hay consumidores; RDM-006 generalizará el contrato).
- Consumers found: None.
- Contract-drift tests searched: None — no existen suites previas.
- Required consumer tests: None.
- Consumer tests run/skipped: skipped — no aplica en repo vacío; primer paquete.

## Verification Gate

Estado 2026-06-10 (ejecución RU1):

- [x] `npm install` sin ejecutar lifecycle scripts y sin errores — 132 paquetes, 0 vulnerabilidades; binarios nativos esbuild/rollup/tauri-cli llegaron vía optionalDependencies (asunción del plan verificada, sin excepciones a `ignore-scripts`).
- [x] `npm run lint` verde (ESLint flat + ts-eslint + react-hooks). `npm run format` aplicado.
- [x] `npm test` verde — 3/3 Vitest (AE1 ping, AE2 tick con handler real disparado, error path).
- [x] `cargo fmt --check` limpio; `cargo clippy -- -D warnings` limpio (sin `#[allow]` necesarios); `cargo test` 2/2 (payload ping estructura + serialización).
- [x] `npm run tauri dev` abre ventana "Tinto" en Windows (proceso `tinto` PID 5532 verificado; ping/tick visibles en pantalla del usuario) — AE1/AE2.
- [x] `npm run tauri build` — exit 0; binario `src-tauri/target/release/tinto.exe` (8.6 MB) + bundles MSI (`Tinto_0.1.0_x64_en-US.msi`) y NSIS (`Tinto_0.1.0_x64-setup.exe`). Binario release lanzado como humo: ventana "Tinto" abre con CSP de producción activo (AE3 tramo Windows ✔).
- [ ] AE3 tramo Linux: pendiente documentado hasta que exista CI (relajación intencional, D2).

**Resultado del gate: PASS** (re-verificado tras fixes de review; todas las superficies con evidencia).
- Surface-aware evidence: runtime Rust → cargo test + clippy; runtime React → vitest + lint; config npm → install log sin scripts; bridge → humo visual dev.
- Production posture evidence: prototype — sin checks de compatibilidad retro; AE3-Linux documentado como pendiente hasta CI (relajación intencional registrada).

## Review Gate

- Code review threshold: P0-P2 (default).
- Findings below threshold: log unless user marks blocking.
- **Resultado 2026-06-10:** 4 personas (correctness, testing, maintainability, adversarial). Aplicados: CSP `default-src 'self'` + ipc (era null, P1); plugin opener removido con su grant de capability (P2); assertion AE2 fortalecida y clearMocks mid-test eliminado (P1/P2); README Tinto + title (P2/P3). Descartados con announce (overclaims): core:default no incluye fs/shell en Tauri 2; StrictMode double-listen limpia su propio unlisten; tick task muere con el proceso. Residual notes en estado. Re-verificación post-fixes en verde. **PASS.**

## Security Gate

- Run after work-review loop: not required — sin auth, secrets, PII, API pública ni deployment; superficie de dependencia cubierta por `ignore-scripts=true` (mitigación supply-chain ya en scope).
- Security Watch during work: disabled — paquete de scaffolding sin superficie de alto riesgo; el worker reporta si aparece algo inesperado (p. ej. necesidad de habilitar scripts).
- Security Watch notes: None.
- Security reviewer: n/a (gate no requerido; krt-security-sentinel disponible si el worker eleva algo).
- Security review result: not required (confirmado tras review: sin superficies de alto riesgo; endurecimientos aplicados de paso: CSP activo, capability mínima, ignore-scripts verificado en log de install).
- Required security verification: confirmar en log de install que no corrieron lifecycle scripts.

## CI Break-Prevention And Escalation

- CI risk surfaces: build (vite + cargo), lint, tests, formato. No hay CI aún (D2): el riesgo es para el CI futuro.
- Preventive evidence: escalera local completa del Verification Gate; gap CI-only explícito = verificación Linux (registrada como pendiente en plan/roadmap).
- If CI breaks: n/a hasta que exista CI; cuando exista, invocar krt-ci-questor con contexto de PR/run.
- Escalation rule: registrar blocker de release-follow-up si el binario Windows no se genera o si `ignore-scripts` exige excepciones no documentadas.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Esqueleto Tauri 2 + React + tooling base
- Branch name: `feat/tauri-react-skeleton`
- Branch/docs rule: esta rama lleva también los artefactos de planificación (commit separado); no se crea rama `docs/*-planning`.
- PR base: `main` (primer PR del repo; tras merge se crea `develop` según D1 del roadmap — decisión a confirmar en release).
- Suggested commit grouping for this review unit:
  - `feat(app): scaffold Tauri 2 + React TS con puente de humo ping/tick` — `src/`, `src-tauri/`, `index.html`, `vite.config.ts`, `tsconfig.json`, `package.json` — una capacidad integrada: app esqueleto con bridge probado.
  - `chore(tooling): lint, format, tests y endurecimiento npm` — `eslint.config.js`, `.prettierrc`, `.npmrc`, `.gitignore`, `src/setupTests.ts`, `src/App.test.tsx` — superficie de tooling/config.
  - `chore(generated): lockfile e íconos del template` — `package-lock.json`, `src-tauri/icons/` — artefactos generados separados de la lógica.
  - `docs(orchestration): roadmap, brainstorm, plan y work package de la iniciativa` — `docs/**` — contexto de planificación de la capability.
- PR title: `feat: esqueleto Tauri 2 + React con puente webview↔Rust probado`
- PR body bullets:
  - App de escritorio Tauri 2 con React + TypeScript (Vite) que compila y abre ventana en Windows.
  - Puente webview↔Rust validado en ambas direcciones: comando `ping` y evento `tick` emitido desde `tauri::async_runtime`.
  - Tooling local: ESLint/Prettier, rustfmt/clippy sin warnings, Vitest (jsdom + mocks oficiales de Tauri) y cargo test.
  - npm endurecido con `ignore-scripts=true`; instalación verificada sin lifecycle scripts.
  - Incluye artefactos de planificación de la iniciativa (roadmap, requisitos, plan, paquete) en commits de docs separados.
- Verification results location: sección Verification Gate de este paquete (estado actualizado al cierre del trabajo) + salida de comandos en el hilo de ejecución.
- Production/deployment notes: none (prototype local).
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: tarea única standalone (un solo RU; no crear parent con hijo único). Si aparecen siblings del mismo roadmap item más adelante, evaluar parent real entonces.
- Jira summary: Esqueleto de aplicación de escritorio con puente frontend-backend probado
- Jira description: Crear la base de la app de monitoreo: shell de escritorio con frontend web tipado y backend nativo, puente de comandos/eventos verificado en ambas direcciones, y tooling local de calidad (lint, formato, tests) con instalación de dependencias endurecida. Deja el repositorio listo para construir la capa de lectura de git, el watcher y el dashboard.
- Optional-policy fallback: sin keys/URLs Jira ni `krt-jira-scribe` configurado en este entorno → registrar "Jira omitted: sin contexto/config Jira en preflight" y continuar.
