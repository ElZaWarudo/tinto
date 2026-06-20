---
title: ACI-001 Backend PTY Runtime + Agent Process Lifecycle
status: review-passed
roadmap_item: ACI-001
origin_roadmap: docs/roadmaps/2026-06-19-002-agent-console-integration.md
origin_plan: docs/plans/2026-06-19-001-feat-agent-console-backend-plan.md
units: [U1, U2, U3, U4, U5, U6, U7, U8, U9, U10]
unit_alignment: complete
review_units: [RU1, RU2, RU3]
base_branch: develop
pr_strategy: stacked
max_open_stack: 2
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# ACI-001 Backend PTY Runtime + Agent Process Lifecycle

## Scope
Implementar el backend Rust para gestión de sesiones de agentes de codificación con terminales PTY:
- Integración de `portable-pty` para terminales interactivas
- Módulo `agent_console` con registro de sesiones
- Comandos Tauri: `start_agent_session`, `stop_agent_session`, `list_agent_sessions`
- Extensión del contrato bus con tipos de sesión
- Validación de binarios (allowlist: claude, codex, opencode)
- Limpieza de procesos multiplataforma (Unix/Windows/WSL)

## Non-goals
- Streaming de output al frontend (ACI-002)
- UI de lanzamiento (ACI-003)
- Checkpoints y revert (ACI-004)
- Auto-splitting (ACI-005)
- Límites de recursos (ACI-006)

## Autonomy Contract
- Mode: guarded
- Agent may decide without asking: internal naming, test structure, error message wording, Cargo.toml dependency versions within semver
- Agent must record as assumptions: portable-pty API compatibility, which crate version for uuid/which, process kill semantics per platform
- Agent must escalate: changes to bus contract shape, new Tauri commands beyond the three specified, platform-specific behavior that differs from plan
- Safe fallback: continue with implementation that doesn't depend on blocked decisions; record blocker and next question
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies
- Requires: None (Wave 1)
- Blocks: ACI-002, ACI-003

## Production Posture
- Posture: prototype
- Evidence: User statement, docs/orchestration/compound-master-state.md
- Confidence: high
- Consequences for this package: No compatibility guarantees required. Can iterate freely.
- Breaking existing behavior allowed: yes because prototype and approved

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1: Añadir dependencia portable-pty | yes | Required for PTY support |
| U2: Definir tipos de contrato para sesiones | yes | Required for bus contract extension |
| U3: Implementar módulo agent_console::session | yes | Core session management |
| U4: Implementar validación de binarios | yes | Security requirement |
| U5: Implementar PTY wrapper con portable-pty | yes | PTY abstraction layer |
| U6: Implementar registro de sesiones | yes | Session lifecycle management |
| U7: Implementar comandos Tauri | yes | Frontend API surface |
| U8: Integrar registro en Tauri app | yes | App integration |
| U9: Implementar process tree kill multiplataforma | yes | Process cleanup requirement |
| U10: Tests de integración end-to-end | yes | Verification requirement |

Grouping rationale:
- All units belong to one cohesive backend feature with tight dependencies
- U1-U2 are foundational (dependencies and contract)
- U3-U6 are core implementation (session, validation, PTY, registry)
- U7-U8 are integration (commands and app wiring)
- U9-U10 are hardening (process cleanup and tests)
- Splitting into multiple packages would create artificial boundaries and increase review overhead

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1-U2: portable-pty dependency + bus contract types | Cargo.toml, bus/contract.rs, bus-contract.md | develop | new | ~150 lines, low risk, foundational |
| RU2 | U3-U6: session module, validation, PTY wrapper, registry | agent_console/* (new module), Cargo.toml (uuid, which) | develop (after RU1 merge) | new | ~600 lines, medium risk, core logic |
| RU3 | U7-U10: Tauri commands, app integration, process kill, tests | agent_console/commands.rs, lib.rs, tests | develop (after RU2 merge) | new | ~500 lines, medium risk, integration + platform-specific |

Rules:
- RU1 is small and foundational, can merge independently
- RU2 is the largest but cohesive (all session management logic)
- RU3 bundles integration + hardening because commands depend on registry, and tests validate the full stack
- Stacked PRs: RU1 → RU2 → RU3, max 2 open at a time
- At-cap action: wait for parent to merge into develop before opening next PR

## Execution Status

- RU1 status (2026-06-20): implementation complete and direct review passed on `feat/agent-console-contract`; local full-suite verification gap resolved; release handoff pending.
- RU1 changed surfaces: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/bus/contract.test.ts`, `docs/contracts/bus-contract.md`, orchestration state, one CI-hygiene fix outside RU1 (`src/qol/shortcuts.test.ts` unused import removal), and local full-suite gap fixes in `src-tauri/src/bus/commands.rs` and `src-tauri/src/watcher/mod.rs`.
- RU1 assumptions recorded:
  - `portable-pty = "0.9.0"` selected from current crates.io search; RU1 does not call the API yet, so API compatibility is deferred to RU2.
  - RU1 intentionally adds session metadata types only. Tauri lifecycle commands remain RU3, and PTY output/input streaming remains ACI-002.
- RU1 Impact Scan:
  - Changed contracts/dependencies: additive `AgentSessionStatus`, `AgentSessionError`, and `AgentSession` Rust/TypeScript contract types; new Rust dependency `portable-pty`.
  - Consumer scan patterns: `AgentSession`, `AgentSessionStatus`, `AgentSessionError`, `agent_session`, `agent_type`, `started_at_ms`, `exit_code`, `portable-pty`.
  - Consumers found: contract docs/tests and ACI planning artifacts only; no runtime consumers yet.
  - Contract-drift tests: `src-tauri/src/bus/contract.rs` serialization test and `src/bus/contract.test.ts` TypeScript shape test updated and passing.
- RU1 Verification:
  - PASS: `python C:\Users\Mayor\.agents\skills\krt-compound-master\scripts\check_work_package.py docs\work-packages\ACI-001-agent-console-backend\2026-06-19-001-agent-console-backend-work-package.md`.
  - PASS: `cargo check`.
  - PASS: `cargo fmt --check`.
  - PASS: `cargo test bus::contract` (3/3).
  - PASS: `cargo test agent_session_serializa_con_estado_snake_case`.
  - PASS after gap fix: `cargo test resolve_within_rechaza_traversal`.
  - PASS after gap fix: `cargo test ae8_repo_removido_estado_terminal`.
  - PASS after gap fix: `cargo test ae5_borrado_del_root_sintetiza_error`.
  - PASS after gap fix: `cargo test remount_tras_repo_removed_revive_el_repo`.
  - PASS after gap fix: `cargo test` (121/121).
  - PASS: `cargo clippy --all-targets -- -D warnings`.
  - PASS: `npm test -- contract.test.ts` (11/11).
  - PASS after the CI-hygiene fix: `npm test -- shortcuts.test.ts contract.test.ts` (32/32).
  - PASS: `npx prettier --check src/bus/contract.ts src/bus/contract.test.ts src/qol/shortcuts.test.ts`.
  - PASS: `npx eslint src/bus/contract.ts src/bus/contract.test.ts src/qol/shortcuts.test.ts`.
  - PASS after `npm install` and CI-hygiene fix: `npm run lint`, `npm run build`.
  - GAP: `npm run format:check` still fails globally on 85 pre-existing files; RU1 TypeScript files pass targeted Prettier.
- Local full-suite gap resolution:
  - `resolve_within_rechaza_traversal` depended on Unix `/etc/hostname`; fixed by creating a real temp external file so the traversal assertion is portable on Windows.
  - Windows `notify` did not emit the exact removed-root event when deleting a watched root; fixed by adding a lightweight router health tick that detects missing mounted roots and synthesizes the same `RepoRemoved` path used by event-based routing.
- RU1 Review/Security Watch:
  - Direct Compound Master diff review found no P0-P2 findings in the additive contract types, docs, tests, or dependency manifest.
  - Security Watch: dependency surface changed, but RU1 does not execute `portable-pty`, add commands, accept user input, spawn processes, or widen Tauri capabilities. Runtime execution, binary allowlist, env sanitization, and process-tree kill remain gated to RU2/RU3 security review.
  - `cargo tree -p portable-pty` reviewed for transitive dependency awareness; no code-level use exists in RU1.

## Reviewability Diagnosis
- Reviewer-experience check: yes, each PR is understandable and verifiable on its own
  - RU1: contract changes are isolated and testable
  - RU2: core logic is self-contained with unit tests
  - RU3: integration is testable end-to-end
- Granularity chosen because: RU2 would be too large if combined with RU3 (~1100 lines), and RU1 is small enough to merge quickly to unblock RU2
- Open-stack plan: chain depth 3, cap 2. At-cap action: wait for parent merge. RU1 merges first, then RU2 opens, then RU2 merges before RU3 opens.
- Jira mapping: each RU maps to one standalone Tarea (no parent needed, each RU is independently valuable)
- Downstream-fix trace: none expected (no overlapping surfaces)
- Failure-mode check: confirmed not a deep micro-PR stack (3 PRs for ~1250 lines is reasonable), not a deferred mega-consolidation PR

## Files and Tests
**Files:**
- `src-tauri/Cargo.toml` (add portable-pty, uuid, which dependencies)
- `src-tauri/src/bus/contract.rs` (add AgentSession types)
- `src-tauri/src/agent_console/mod.rs` (new module)
- `src-tauri/src/agent_console/session.rs` (session struct)
- `src-tauri/src/agent_console/validation.rs` (binary validation)
- `src-tauri/src/agent_console/pty.rs` (PTY wrapper)
- `src-tauri/src/agent_console/commands.rs` (Tauri commands)
- `src-tauri/src/lib.rs` (register commands, manage registry)
- `docs/contracts/bus-contract.md` (document new types)

**Tests:**
- Unit tests for session state transitions
- Unit tests for binary validation (allowlist, PATH lookup)
- Unit tests for PTY wrapper (mock portable-pty if needed)
- Unit tests for registry (start/stop/list)
- Unit tests for commands (invoke handlers)
- Integration tests for full lifecycle
- Platform-specific tests for process tree kill (Unix/Windows/WSL)

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures:
  - Bus contract: add AgentSession, AgentSessionStatus, AgentSessionError types
  - Tauri commands: add start_agent_session, stop_agent_session, list_agent_sessions
  - Cargo.toml: add portable-pty, uuid, which dependencies
- Consumer scan patterns:
  - `rg "bus/contract"` (frontend contract consumers)
  - `rg "invoke.*agent"` (frontend command consumers)
- Consumers found: none (new feature, no existing consumers)
- Contract-drift tests searched:
  - `src/bus/contract.test.ts` (frontend contract tests)
  - `src-tauri/src/bus/contract.rs` tests (backend contract tests)
- Required consumer tests: none (new feature, no existing consumers to update)
- Consumer tests run/skipped: N/A (no existing consumers)

## Verification Gate
- `cargo build` passes without errors
- `cargo test` passes all tests (unit + integration)
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- `cargo fmt --check` passes
- Manual verification: launch a test agent session (e.g., `echo` command) and verify it starts/stops correctly
- Surface-aware evidence:
  - Bus contract: run frontend contract tests to ensure no drift
  - Tauri commands: run integration tests that invoke commands
  - Process cleanup: verify no zombie processes after stop
- Production posture evidence: prototype, no compatibility requirements

## Review Gate
- Code review threshold: P0-P2 (default)
- Findings below threshold: log unless user marks blocking

## Security Gate
- Run after work-review loop: required because binary validation and process execution are security-sensitive
- Security Watch during work: enabled (monitor binary validation, process execution, environment sanitization)
- Security Watch notes:
  - Binary validation must use allowlist (no arbitrary command execution)
  - Environment must be sanitized (no secret leakage)
  - Process tree kill must not affect unrelated processes
- Security reviewer: krt-security-sentinel
- Security review result: pending
- Required security verification:
  - Test that allowlist rejects unknown binaries
  - Test that environment sanitization works
  - Test that process tree kill is scoped correctly

## CI Break-Prevention And Escalation
- CI risk surfaces:
  - Build: new dependency (`portable-pty`) may have platform-specific build requirements
  - Tests: new contract serialization tests must pass on Rust and TypeScript sides
  - Lint: new code must pass clippy
- Preventive evidence:
  - Local `cargo build` on target platform
  - Local `cargo test` on target platform
  - Local `cargo clippy` passes
- If CI breaks: invoke krt-ci-questor with PR/run/check context
- Escalation rule: record release-follow-up blocker until CI incident has cause, owner, and next action

## Branch and PR Handoff Inputs
- Review unit: RU1
- Branch name: feat/agent-console-contract
- Branch/docs rule: RU1 carries planning artifacts (roadmap, plan) on the same semantic branch
- PR base: develop
- Suggested commit grouping for RU1:
  - `feat(agent-console): add portable-pty dependency` - Cargo.toml - foundational dependency
  - `feat(bus-contract): add agent session types` - Rust/TS contract mirrors, tests, and docs
- PR title: feat(agent-console): add PTY runtime dependencies and bus contract
- PR body bullets:
  - Add portable-pty dependency for the agent console backend
  - Extend bus contract with AgentSession, AgentSessionStatus, AgentSessionError types
  - Mirror and test the new types in TypeScript and document them in bus-contract.md
- Verification results location: CI run link
- Production/deployment notes: none (prototype)
- Autonomous mutation request: none

## Jira Handoff Inputs
- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone Tarea per RU (no parent needed, each RU is independently valuable)
- PR-to-Jira mapping: each RU PR maps to one standalone Tarea
- Jira summary: Backend PTY runtime y contrato para sesiones de agentes
- Jira description: Implementar el backend Rust para gestión de sesiones de agentes de codificación con terminales PTY. Incluir integración de portable-pty, extensión del contrato bus con tipos de sesión, y validación de binarios.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking
