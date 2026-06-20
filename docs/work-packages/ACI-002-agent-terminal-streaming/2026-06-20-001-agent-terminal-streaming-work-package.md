---
title: ACI-002 PTY Stream Bridge + Dockable Terminal Panel
status: package-review-passed
roadmap_item: ACI-002
origin_roadmap: docs/roadmaps/2026-06-19-002-agent-console-integration.md
origin_brainstorm: docs/brainstorms/2026-06-20-001-agent-terminal-streaming-requirements.md
origin_planning_input: docs/brainstorms/2026-06-20-001-agent-terminal-streaming-requirements.md
origin_plan: docs/plans/2026-06-20-001-feat-agent-terminal-streaming-plan.md
units: [U1, U2, U3, U4]
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

# ACI-002 PTY Stream Bridge + Dockable Terminal Panel

## Scope

Implement live PTY streaming and a dockable terminal attachment surface for sessions created by ACI-001:

- Backend output event and input/resize commands.
- TypeScript bus wrappers and contract docs.
- xterm-based terminal panel.
- Stable dockview panel registration and open/focus helper.

## Non-goals

- Repo-card launch UI and binary availability validation UX.
- Checkpoints, revert, audit trail, or change attribution.
- Multi-agent auto-split layout decisions.
- Resource limits, lifetime caps, persistent session history, or telemetry.
- Historical PTY output replay for output produced before panel attachment.

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: internal method names, test mocks, event payload helper names, debounce implementation, and equivalent verification commands.
- Agent must record as assumptions: xterm package names, jsdom mock boundaries, no historical stream replay, and any local build blocker caused by the running app binary.
- Agent must escalate: public contract removal, persistent session history, checkpoint/revert behavior, launch UI behavior, or branch/base strategy changes.
- Safe fallback: continue with backend and contract work when frontend package installation or visual testing is blocked; record the gap.
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies

- Requires: ACI-001 delivered on `develop` at `eca0e40`.
- Blocks: ACI-003, ACI-005, and part of ACI-006.

## Production Posture

- Posture: prototype
- Evidence: Agent Console Integration roadmap and existing Compound Master state.
- Confidence: high
- Consequences for this package: additive contract changes and dependency additions are acceptable when verified locally.
- Breaking existing behavior allowed: no intentional breakage; additive panel/command behavior only.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1: Backend stream contract and registry I/O | yes | Required for live output, input, and resize behavior |
| U2: TypeScript contract and client stream wrappers | yes | Required consumer mirror for backend commands/events |
| U3: Terminal panel surface | yes | Required user-visible terminal attachment |
| U4: Dock workspace integration | yes | Required to make terminal panels openable and persistent |

Grouping rationale:
- U1 and U2 define the contract bridge and should land before UI work.
- U3 adds the visual terminal surface with dependency risk isolated from backend concurrency.
- U4 connects terminal panels to dockview and can be reviewed after the panel itself exists.

## Implementation Units

- U1. Backend stream contract and registry I/O.
- U2. TypeScript contract and client stream wrappers.
- U3. Terminal panel surface.
- U4. Dock workspace integration.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1-U2: backend output/input/resize contract plus frontend bus mirror | `agent_console/*`, bus contract Rust/TS, client wrappers, contract docs/tests | develop | new | Medium risk: backend concurrency and public IPC/event contract |
| RU2 | U3: xterm terminal panel | package deps, terminal panel, CSS, panel tests | develop after RU1 | new | Medium risk: UI dependency and effect cleanup |
| RU3 | U4: dock panel registration/open helper/persistence compatibility | workspace panel registry/open helper/App wiring/tests | develop after RU2 | new | Low/medium risk: dockview behavior and layout compatibility |

Rules:
- Execute one RU at a time.
- Keep related contract documentation with RU1.
- Keep xterm dependency changes with RU2 so dependency review is focused.
- Keep launch UI out of this package; ACI-003 owns it.

## Files and Tests

- Backend: `src-tauri/src/agent_console/*`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`.
- Frontend contract: `src/bus/contract.ts`, `src/bus/client.ts`, `src/bus/contract.test.ts`.
- Terminal UI: `src/panels/terminal/*`, `src/App.css`.
- Workspace integration: `src/workspace/panels.ts`, `src/workspace/openAgentTerminal.ts`, `src/App.tsx`.
- Verification: `cargo test agent_console`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `npm test -- contract.test.ts`, targeted terminal/workspace tests, `npm test`, `npm run lint`, `npm run build`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri commands, additive Tauri event, `AgentSessionOutput` contract, frontend bus wrappers, terminal panel registry.
- Consumer scan patterns: `AgentSessionOutput`, `EVENT_AGENT_SESSION_OUTPUT`, `write_agent_session_input`, `resize_agent_session`, `PANEL_AGENT_TERMINAL`, `TerminalPanel`, `openAgentTerminal`.
- Consumers found: pending implementation; expected consumers are `src/bus/client.ts`, `src/bus/contract.test.ts`, `src/panels/terminal/TerminalPanel.tsx`, `src/workspace/panels.ts`, and `src/App.tsx`.
- Contract-drift tests searched: `src/bus/contract.test.ts` and `src-tauri/src/bus/contract.rs` serialization tests.
- Required consumer tests: contract wrapper tests, terminal panel tests, workspace open helper tests.
- Consumer tests run/skipped: RU1 ran `src/bus/contract.test.ts`; RU2 ran `src/panels/terminal/TerminalPanel.test.tsx` plus contract regression; RU3 ran `src/workspace/openAgentTerminal.test.ts`, `src/App.test.tsx`, and the terminal panel regression.

## Verification Gate

- RU1: `cargo fmt --check`, `cargo test agent_console`, `cargo test bus::contract`, `cargo clippy --all-targets -- -D warnings`, `npm test -- contract.test.ts`.
- RU2: targeted terminal panel tests, `npm test -- TerminalPanel.test.tsx contract.test.ts`, `npm run lint`.
- RU3: workspace open helper tests, full `npm test`, `npm run build`, and Rust smoke if backend unchanged.
- Surface-aware evidence: record backend IPC/event contract, frontend typed wrappers, terminal effect cleanup, dockview persistence compatibility, and docs/orchestration updates.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate

- Run after work-review loop: required for RU1 because it exposes interactive process I/O commands and output events.
- Security Watch during work: enabled for all RUs because this roadmap executes local processes and forwards input/output.
- Security Watch notes: validate repo/session allowlists, avoid secret-bearing env or logs in errors, keep output events scoped by session id, and avoid spawning extra shells.
- Security reviewer: inline fallback acceptable if `krt-security-sentinel` is not invoked.
- Security review result: RU1 direct security watch passed on 2026-06-20. `start_agent_session` still fail-closes through the active-workbench repo allowlist; `write_agent_session_input` and `resize_agent_session` only target existing running session ids; invalid base64 returns `invalid_input`; invalid dimensions return `invalid_terminal_size`; missing/stopped sessions are rejected with structured errors. Output events carry live PTY bytes scoped by session id and do not log command input or read errors. RU2 dependency watch notes: `@xterm/xterm@6.0.0` and `@xterm/addon-fit@0.11.0` are MIT packages; `npm audit` reports remaining transitive vulnerabilities in `vite -> esbuild@0.27.7` (low) and `jsdom -> undici@7.27.2` (high), not introduced by xterm directly. No remote network surface is added by the terminal panel itself.
- Required security verification: command paths reject missing sessions and invalid dimensions; no command reconstructs shell input.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust build/test/clippy, frontend typecheck/build/lint/test, dependency install, event/command contract drift.
- Preventive evidence: record local command outcomes per RU; if `cargo build` binary replacement is blocked by an open app, record `cargo check`, `cargo test`, and `cargo build --lib`.
- If CI breaks: invoke `krt-ci-questor` or perform direct evidence-first triage with workflow/job context.
- Escalation rule: do not bypass failing CI without explicit user approval.

## Branch and PR Handoff Inputs

- Review unit: RU1 backend stream contract and bus mirror.
- Branch name: `feat/agent-terminal-streaming`
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `feat(agent-console): stream PTY output and input events` - Rust backend contract, registry I/O, commands, tests.
  - `feat(bus): mirror agent terminal stream contract` - TypeScript contract/client/docs/tests.
  - `docs(orchestration): add terminal streaming package state` - brainstorm, plan, work package, Compound Master state.
- PR title: Add agent terminal streaming contract
- PR body bullets:
  - Add live PTY output events and input/resize commands for agent sessions.
  - Mirror the terminal stream contract in the frontend bus client.
  - Document the terminal streaming contract and review-unit state.
- Verification results location: this work package Execution Status section and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: prototype-local additive commands/events only.
- Autonomous mutation request: none

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: create/reuse subtasks only if a real parent with sibling ACI work exists; otherwise use a standalone task.
- Jira summary: Anadir streaming de terminal para sesiones de agente
- Jira description: Habilitar salida, entrada y resize de PTY para que las sesiones de agente puedan mostrarse en un panel de terminal dentro de Tinto.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.

## Execution Status

- RU1 implementation status: released to `develop` and pushed at `3ed1075`.
- RU1 changed surfaces: `src-tauri/src/agent_console/*`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`, `src/bus/contract.ts`, `src/bus/client.ts`, `src/bus/contract.test.ts`, and `docs/contracts/bus-contract.md`.
- RU1 implementation notes: added `tinto://agent-session-output`, `AgentSessionOutput`, a PTY output reader handoff, live output emission, `write_agent_session_input`, `resize_agent_session`, frontend wrappers, and contract tests. No xterm dependency, terminal panel, dock registration, launch UI, replay buffer, checkpoint, or revert behavior was added in RU1.
- RU1 direct review result: no P0-P2 findings. One local TypeScript mock typing issue found by `npm run build` was fixed before closeout.
- RU1 verification: `cargo fmt --check` passed; `cargo clippy --all-targets -- -D warnings` passed; `cargo test agent_console --lib` passed 22/22; `cargo test bus::contract --lib` passed 4/4; `npx vitest run src/bus/contract.test.ts` passed 15/15; `npm run build` passed with only the existing Vite chunk-size warning.
- RU1 release marshal status: complete. Commits: `ade10c3`, `28e9849`, `3ed1075`; fast-forward merged and pushed to `origin/develop`.
- RU2 implementation status: complete locally on `feat/agent-terminal-panel`.
- RU2 changed surfaces: `package.json`, `package-lock.json`, `src/panels/terminal/TerminalPanel.tsx`, `src/panels/terminal/TerminalPanel.test.tsx`, `src/App.css`, and `src/bus/contract.test.ts`.
- RU2 implementation notes: added the xterm-backed terminal attachment surface, fit-addon resize publication, session-filtered output rendering, input forwarding, cleanup of terminal/listener resources, and compact IDE styling. No dock panel registration/open helper or launch UI was added in RU2.
- RU2 direct review result: no P0-P2 findings. A TypeScript/ESLint mock varargs issue was fixed before closeout.
- RU2 verification: `npx vitest run src/panels/terminal/TerminalPanel.test.tsx src/bus/contract.test.ts` passed 19/19; `npm run build` passed with only the existing Vite chunk-size warning; `npm run lint` passed.
- RU2 release marshal status: complete. Commits: `e4f3361`, `de67219`; fast-forward merged and pushed to `origin/develop`.
- RU3 implementation status: complete locally on `feat/agent-terminal-dock`.
- RU3 changed surfaces: `src/workspace/panels.ts`, `src/workspace/openAgentTerminal.ts`, `src/workspace/openAgentTerminal.test.ts`, `src/App.tsx`, `src/App.test.tsx`, and this orchestration state.
- RU3 implementation notes: registered `TerminalPanel` under `PANEL_AGENT_TERMINAL`, added stable panel ids derived from session ids, added a deduplicating open/focus helper, and verified the app component registry includes the terminal panel. No launch UI, checkpoint, revert, or session-history behavior was added.
- RU3 direct review result: no P0-P2 findings. Build warning remains for large chunks; xterm registration increases the main desktop bundle, accepted for this prototype package.
- RU3 verification: `npx vitest run src/workspace/openAgentTerminal.test.ts src/App.test.tsx src/panels/terminal/TerminalPanel.test.tsx` passed 10/10; `npm run build` passed with Vite chunk-size warning; `npm run lint` passed.
- RU3 release marshal status: pending local semantic commits, fast-forward merge to `develop`, and push.
- ACI-002 package status after RU3 release: complete.
