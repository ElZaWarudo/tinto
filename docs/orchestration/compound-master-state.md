# Compound Master — Estado en vivo

> Entrypoint compacto de resume. Detalle histórico largo se archiva en `archive/compound-master-state/`.

## Iniciativa

- **Proyecto:** Tinto — app de escritorio (Tauri 2) de monitoreo read-only de repos git editados por agentes de código.
- **Fuente de diseño:** `tinto-design.md` (diseño a nivel arquitectura, no spec de implementación).
- **Repo:** `/home/teb/personal-proyects/tinto` (**checkout Linux/WSL desde 2026-06-11**; corridas previas en `C:\Users\Mayor\Documents\Caribbean\tinto`, Windows). Remote: https://github.com/ElZaWarudo/tinto. Integración: `develop` (gitflow, D1).
- **Toolchain Linux verificado (2026-06-11):** Rust 1.93.1, node 24.13, npm 11.6, `gh` autenticado (ElZaWarudo). Plugin compound-engineering 3.12.0 **reinstalado** en este host (user scope, marketplace EveryInc); los SKILL.md de ce-* se leen del cache `~/.claude/plugins/cache/.../3.12.0/skills/`.

## Argumentos resueltos

| Arg | Valor | Notas |
|---|---|---|
| mode | full | artefactos + arrancar ejecución del primer review unit |
| production | prototype | greenfield, sin sistema live |
| jira-policy | optional | sin contexto Jira detectado → handoff degradado sin Jira |
| pr-granularity | auto (review-unit) | |
| parallel | false | |
| delegation | auto → autonomy:guarded | |
| worktree-policy | avoid | |
| autonomy | guarded | sin ledger → solo autonomía local, sin mutación externa |
| review-threshold | P0-P2 | |

## Decisiones de usuario (preflight)

- **Alcance:** mode:full — generar artefactos y arrancar ejecución.
- **Git:** inicializado en el folder tinto (rama `main`).
- **Frontend:** **React** (el diseño lo marcaba opcional; usuario lo fija explícitamente).

## Roles resueltos

| Rol | Skill | Runtime |
|---|---|---|
| roadmap_generator | krt-roadmap-cartographer | nativo |
| brainstorm | ce-brainstorm | plugin compound-engineering 3.12.0* |
| plan | ce-plan | plugin compound-engineering 3.12.0* |
| document_review | ce-doc-review | plugin compound-engineering 3.12.0* |
| work | ce-work | plugin compound-engineering 3.12.0* |
| code_review | ce-code-review | plugin compound-engineering 3.12.0* |
| security_review | krt-security-sentinel | nativo |
| project_pr | krt-release-marshal | nativo |

\* Plugin instalado a mitad de sesión (user scope, marketplace EveryInc/compound-engineering-plugin). En esta sesión los SKILL.md se ejecutan leyéndolos desde `C:\Users\Mayor\.claude\plugins\cache\compound-engineering-plugin\compound-engineering\3.12.0\skills\`; tras reiniciar sesión serán invocables nativamente.

## Pipeline — progreso

- [x] 1. Preflight (roles, repo, branch, jira, producción, delegación, contexto)
- [x] 2. Roadmap generado: `docs/roadmaps/2026-06-10-001-tinto-roadmap.md` (12 items RDM-001..012, 8 waves; decisiones D1 gitflow, D2 CI, D3 React registradas)
- [x] 3a. Review del roadmap (ce-doc-review headless: coherence, feasibility, design-lens, scope-guardian — 4 reviewers paralelos). 2 safe_auto aplicados (drift de Blocks/enables en RDM-001/005). 3 decisiones de usuario aplicadas: editor fs_watch → RDM-009; first-run onboarding → RDM-007; secrets → patrones simples en RDM-011. Boundary RDM-011↔008/009 aclarado. Resto ruteado a "Deferred / Open Questions" del roadmap.
- [x] 3b-RDM-001. Brainstorm capturado y revisado: `docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md` (TypeScript; npm ignore-scripts; GitHub remote ahora + CI diferido → D2 resuelta; AE3: Windows duro, Linux best-effort hasta CI; R1 sin pin de React 18 → versión del template).
- [x] 4-RDM-001. Plan escrito y revisado: `docs/plans/2026-06-10-001-feat-esqueleto-tauri-react-plan.md` (U1 scaffold, U2 tooling, U3 puente humo vía tauri::async_runtime + mockIPC, U4 entrega-solo-handoff). Work package derivado, checker OK (warning docs-mixing justificado) y review "PACKAGE SOUND": `docs/work-packages/RDM-001-esqueleto-tauri/2026-06-10-001-esqueleto-tauri-work-package.md` — RU1 única, rama `feat/tauri-react-skeleton`, base `main`.
  - Items RDM-002..012: brainstorm/plan/package pendientes; se generan al acercarse su wave (los de UI dependen del contrato de eventos que congela RDM-006).
- [ ] 5. Ejecutar RU1 de RDM-001 — **EN CURSO** (inline, rama `feat/tauri-react-skeleton`)
  - Delegación resuelta: inline (matriz: trabajo acoplado mismo-archivo + CLI pesado). Usuario aprobó ejecución y aportó dirección de UI (tabs por proyecto + árbol izquierda) → registrada en roadmap Open Questions RDM-007/008.
  - **Acciones de toolchain (asunciones ejecutadas, reversibles):** rustup 1.29/Rust 1.96 MSVC instalado vía winget; Windows 11 SDK 10.0.26100.8249 instalado (elevado, UAC aceptado por usuario) — VS2019 BuildTools MSVC 14.29 y WebView2 ya existían. Sanity link test OK.
  - U1 ✔: template react-ts scaffoldeado (React 19.1, Vite 7, TS 5.8, Tauri 2) vía dir temporal → raíz; identidad renombrada (tinto / dev.tinto.app / Tinto); `.npmrc ignore-scripts=true` ANTES de install; **asunción verificada: install limpio sin lifecycle scripts** (esbuild/rollup/tauri via optionalDependencies); tokio time + cargo check OK.
  - U2 ✔: ESLint flat + Prettier + Vitest jsdom + testing-library; scripts lint/format/test; format aplicado; lint verde; `.prettierignore`.
  - U3 ✔ (código+tests): ping/tick con tauri::async_runtime + Emitter; App.tsx con instrumentación; vitest 3/3 (AE1, AE2, error path — mockIPC + vi.mock del módulo event); cargo test 2/2; fmt+clippy -D warnings limpios.
  - Humo visual `tauri dev` ✔ (ventana "Tinto" PID 5532, ping/tick visibles).
- [ ] 6. Code review (rol ce-code-review, 4 personas: correctness/testing/maintainability/adversarial) — **fixes aplicados**:
  - CSP `default-src 'self'` + ipc (era `null`) [correctness P1 + adversarial].
  - Removido `tauri-plugin-opener` no usado (Cargo.toml, lib.rs, package.json) y su grant `opener:default` en capabilities [maintainability + adversarial].
  - Test AE2: assertion fortalecida a patrón de hora real; eliminado `clearMocks()` a mitad de test [testing P1/P2].
  - README de Tinto (reemplaza template) y `<title>Tinto</title>` [maintainability P2/P3].
  - Descartados con announce: core:default NO incluye fs/shell en Tauri 2 (overclaim del adversarial); StrictMode double-listen limpia cada unlisten propio; tick task muere con el proceso. Registrados como residual notes.
  - Re-verificación post-fixes ✔: vitest 3/3, lint, fmt, clippy, cargo test 2/2.
  - Re-humo dev con CSP ✔ (ventana up). `tauri build` exit 0: exe 8.6MB + MSI + NSIS; humo del binario release ✔ (ventana "Tinto" con CSP de producción). **Items 5 y 6 del pipeline COMPLETOS; review unit RU1 implementation-complete + review PASS + verificación PASS.** Paquete → `implemented-verified-awaiting-release`.
- [x] 7. Handoff a krt-release-marshal **COMPLETO** (2026-06-10):
  - Plan de release aprobado por usuario (incluida decisión de tamaño/alcance: aprobar PR grande — masa = lockfiles generados en commit propio; autoría humana ~600 líneas código + ~890 docs).
  - Bootstrap: `main` con root commit `4b7438d` (init vacío); rama `feat/tauri-react-skeleton` con 4 commits: `e9c46e5` feat(app) 600+, `8f16675` chore(tooling) 66+, `26003e1` chore(generated) 9198+, `44f69a7` docs(orquestación) 890+. Guard env-ignore de gitflow-knight OK (creó `.krt/env/.gitignore`, commiteado en tooling).
  - Rebase: innecesario (historia lineal nueva).
  - Jira: **omitted** — checker `ok: false` diagnóstico `env-loaded-without-project-secret-file` (falta `.krt/env/jira-scribe.env` en el checkout). Sin backlink ni transición.
  - Repo GitHub privado creado: https://github.com/ElZaWarudo/tinto; push de `main` y `feat/tauri-react-skeleton`.
  - **PR #1: https://github.com/ElZaWarudo/tinto/pull/1** (`feat/tauri-react-skeleton → main`, ready). Cuerpo validado con format/check_pr_body (5 bullets, sin IDs internos).
  - Reviewers: omitidos con nota — repo nuevo sin colaboradores.
  - **Merge: NO intentado** — requiere gate visible en GitHub + autorización explícita del usuario para ese merge exacto.
  - R12 ✔ (remote origin privado con main pusheada). RDM-001 entregado a revisión.

## Checkpoint de merge (2026-06-11)

- **PR #1 MERGED** con autorización explícita del usuario ("completa el merge"); gate visible: MERGEABLE, sin checks ni reviews requeridos (main sin protección, repo sin colaboradores). Merge commit `62e5653`; rama feature borrada local y remota.
- **D1 RESUELTA:** `develop` creada desde main y pusheada. Las features siguientes basan en `develop`; PRs `feat/* → develop`.
- Docs post-PR (este estado + summary 2026-06-10) en working tree; viajan con la rama de RDM-002.

## Wave 2 — EN CURSO

- RDM-001 ✅ ENTREGADO Y MERGEADO (PR #1).
- RDM-002 ✅ ENTREGADO Y MERGEADO (**PR #2**, merge `b9465e9`, autorización explícita en el plan de release aprobado; 4 commits semánticos; review 3 personas → 7 tests añadidos + 1 bug real atrapado: log() unborn devolvía Internal; 24/24 tests). Artefactos: brainstorm/plan/package 2026-06-11 + gates actualizados.
- **Gates wave 2 (decisión usuario 2026-06-11): auto hasta release plan** — solo detengo en el plan de release por PR y en decisiones de producto reales.
- RDM-003 ✅ ENTREGADO Y MERGEADO (**PR #3**, merge `2011005`; merge autorizado bajo la regla standing). Módulo `paths` (clasificador con semántica git real, poda BFS, cero I/O en classify).
- **Acuerdos de flujo (usuario 2026-06-11):** (a) actualizar y commitear los archivos de compound master al cierre de cada unidad; (b) **merge pre-autorizado para los PRs del programa siempre que el PR incluya el estado de compound master actualizado** (el release plan debe afirmar esa condición).
- RDM-005 ✅ ENTREGADO Y MERGEADO (**PR #4**, merge `a199d0e`, bajo regla standing — estado incluido en `9e6f670`). Módulo `workbench`: store TOML atómico (tmp por-PID, backup `.corrupt`), CRUD + autodetección BFS-4 con worktrees, 9 comandos Tauri. 53/53 tests; review dual con 4 P1 corregidos.

## ✅ WAVE 2 COMPLETA (2026-06-11)

PRs #2 (git engine), #3 (clasificador), #4 (workbenches) mergeados a `develop`. Backend listo para integración: 53 tests, clippy limpio.

## Pausa de corrida (decisión usuario 2026-06-11: "aprobar y pausar después")

- ~~Rama `feat/fs-watcher` ya creada~~ — esa rama se creó **solo en el checkout Windows** y nunca se pusheó; **recreada en el checkout Linux** desde `develop` (2026-06-11, esta corrida).
- Después de RDM-004: wave 4 RDM-006 (bus — congelar contrato de eventos; resolver wrapping spawn_blocking del GitEngine; recordar dirección de UI del usuario para RDM-007/008: tabs por proyecto + árbol izquierda + gestión de archivos abiertos).

## Resume Linux (2026-06-11) — incidente de reconciliación y decisiones

- **Incidente (resuelto):** la corrida resumida en este host arrancó con el checkout en `main` (stale, 14 commits detrás) y sin inspeccionar el contenido de `origin/develop`; regeneró brainstorm/plan/work-package de **RDM-002**, que ya estaba entregado (PR #2). El review de feasibility del work package lo detectó (conf 100). **Corrección:** trabajo local duplicado descartado (decisión usuario), checkout resincronizado a `develop`. *Lección operativa: el preflight de resume debe inspeccionar la base de integración remota (`git log origin/main..origin/develop`), no solo verificar que exista.*
- **Decisión de usuario — fetch opt-in → BACKLOG (2026-06-11):** durante la regeneración el usuario eligió (3 confirmaciones) añadir un `fetch` opt-in al git engine. develop entregó RDM-002 deliberadamente sin red (`git2 0.20` sin features ssh/https, "Tinto nunca hace fetch/push"). El usuario decidió preservar el diseño del fetch como **item de backlog/follow-up** (enmienda futura al git engine), no ejecutarlo ahora ni descartarlo. Diseño preservado en `docs/backlog/2026-06-11-fetch-opt-in-backlog.md` (decisiones + requisitos de seguridad del review: host fail-closed sin CertificatePassthrough, scoping de credenciales al host confirmado, sanitización de errores, known_hosts manual, staleness honesta).

## Blockers

- Ninguno. Jira sigue omitida (falta `.krt/env/jira-scribe.env`; crearlo la habilitaría).

## Próxima acción

Wave 3 — **RDM-004 (watcher notify + debounce 200–400ms + throttling por repo)**: brainstorm → review → plan → review → package → ejecución, consumiendo `PathClassifier` (RDM-003) y la config de workbenches (RDM-005). Rama `feat/fs-watcher` (este checkout). Preguntas abiertas del roadmap para el brainstorm: confirmar que debounce (004) y coalescing (006) no dupliquen trabajo; scope del watcher (workbench activo vs todos, configurable). Gates: auto hasta release plan (regla standing wave 2, salvo decisión de producto real).
