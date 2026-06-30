---
title: Codex app-server runtime
status: review-passed
roadmap_item: RDM-016
origin_roadmap: docs/roadmaps/2026-06-30-007-codex-app-server-runtime-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md
origin_planning_input: docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md
origin_plan: docs/plans/2026-06-30-016-codex-app-server-runtime-plan.md
units: [U1, U2, U3, U4, U5]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Codex App Server Runtime

## Scope
Implement a Codex app-server-backed agent runtime for Tinto sessions. The runtime should start Codex turns, stream output, consume app-server turn/change notifications, and close Agent Lens checkpoints through structured Codex lifecycle events.

## Implementation Evidence
- RU1 implementation completed and reviewed locally on 2026-06-30.
- Backend:
  - Added `src-tauri/src/agent_console/app_server.rs` with a Codex app-server stdio process adapter.
  - Local Codex sessions prefer app-server through `PortablePtyFactory`; app-server launch failure falls back to the existing PTY runtime.
  - The adapter initializes app-server, starts an ephemeral thread with repo `cwd`, subscribes to `fs/watch`, converts line input into `turn/start`, streams assistant/command deltas as session output, and maps `turn/completed` / `fs/changed` / diff/file-change notifications into structured process events.
  - `AgentProcess` now supports drainable structured events so Codex checkpoint closure no longer relies on an injected terminal marker. The marker remains only for PTY/fallback agents.
- Contract/docs:
  - Updated `docs/contracts/bus-contract.md` to document app-server-preferred local Codex sessions and PTY fallback.
- Local app-server smoke:
  - Node smoke against installed Codex app-server proved `thread/start` and `fs/watch` responses for a temp repo.
- Focused verification passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1` (55/55)
  - `cargo test --manifest-path src-tauri/Cargo.toml agent_console::app_server -- --test-threads=1` (5/5)
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1` (47/47)
  - `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run` (44/44)
  - `npm run build` passed with the existing chunk-size warning
  - `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed
  - `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` passed
  - `git diff --check` passed with existing CRLF warnings only

## Non-goals
- Removing the existing PTY runtime.
- Implementing OpenCode or Claude native adapters.
- Committing, PR generation, or Jira mutation from turn data.
- Full chat redesign outside the current Agents surface.
- Generated Codex app-server schema artifacts committed to the repo.

## Autonomy Contract
- Mode: guarded.
- Agent may decide without asking: internal Rust module names, tolerant parser helper shape, exact test fixture messages, and equivalent focused verification commands.
- Agent must record as assumptions: Codex app-server is experimental, Windows local repos are first-class for this package, and WSL can remain fallback unless proven safe.
- Agent must escalate: removing PTY fallback, changing destructive revert semantics, changing public branch/PR/Jira workflow, introducing external credentials, or broad UI redesign.
- Safe fallback: keep Codex app-server as an optional runtime path and preserve terminal-backed Codex sessions if app-server launch fails.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies
- Requires: RDM-015 Agent turn checkpoints.
- Blocks: future native OpenCode/Claude adapters and richer IADE turns view.

## Production Posture
- Posture: prototype.
- Evidence: existing Compound Master state identifies Tinto as prototype and current work is local desktop behavior.
- Confidence: medium.
- Consequences for this package: preserve existing user-visible agent sessions and keep app-server behavior additive/fallback-capable.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Runtime boundary is required before app-server can be replaceable. |
| U2 | yes | Codex app-server transport is the core capability. |
| U3 | yes | Turn/checkpoint integration is the main product reason for app-server. |
| U4 | yes | User needs chat with Codex, not only background event capture. |
| U5 | yes | Contract and runtime changes need focused tests/docs. |

Grouping rationale:
- The units are tightly coupled for review: chat start, app-server transport, event mapping, and checkpoint closure must be verified together to prove the Codex runtime works. Splitting would create a stack where early PRs cannot demonstrate user value independently.

## Implementation Units
- U1. Runtime boundary and session model.
- U2. Codex app-server transport.
- U3. Event mapping and checkpoints.
- U4. Frontend chat/input wiring.
- U5. Tests and documentation.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Codex app-server chat and checkpoint runtime | backend runtime/process/event mapping, additive contracts, existing Agents UI wiring, tests/docs | develop | optional new Tarea | High integration risk; keep schema parsing narrow and preserve PTY fallback. |

## Reviewability Diagnosis
- Reviewer-experience check: yes. One PR can be understood as a single capability slice: Codex sessions use app-server events for chat and turn checkpoints while other agents keep fallback behavior.
- Granularity chosen because: the surfaces are coupled by runtime lifecycle and cannot be independently verified without test-only scaffolding.
- Open-stack plan: independent PR; no stack.
- Jira mapping: standalone `Tarea` if Jira is used.
- Downstream-fix trace: none.
- Failure-mode check: this is not a deep micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests
- Expected files: `src-tauri/src/agent_console/*`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`, `src/bus/contract.ts`, `src/bus/client.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `docs/contracts/bus-contract.md`.
- Expected tests: Rust `agent_console` tests for app-server parsing/event mapping; bus contract tests; terminal/session store tests where frontend contract changes are visible.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive session runtime metadata and/or app-server chat input command if required.
- Consumer scan patterns: `AgentSession`, `start_agent_session`, `write_agent_session_input`, `turn_checkpoints`, `TerminalPanel`, `codex`.
- Consumers found: `src-tauri/src/agent_console/*`, `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/panels/DashboardPanel.tsx`, `src/bus/contract.test.ts`, `src/panels/terminal/TerminalPanel.test.tsx`, `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: `src/bus/contract.test.ts`, Rust `agent_console` tests, terminal panel tests.
- Required consumer tests: backend agent_console tests, TS contract tests, and terminal panel tests.
- Consumer tests run/skipped: `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1` passed 55/55; `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1` passed 47/47; `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run` passed 44/44.

## Verification Gate
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run`
- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `git diff --check`
- Surface-aware evidence: include a local app-server smoke or unit-equivalent evidence for `fs/watch`/turn notification parsing.
- Production posture evidence: prototype posture does not relax compatibility for existing PTY sessions.

## Review Gate
- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate
- Run after work-review loop: not automatically required unless implementation introduces network listeners, credential handling, destructive behavior beyond existing revert, or public API exposure.
- Security Watch during work: enabled for app-server process launch, local transport, and fallback behavior.
- Security Watch notes: prefer stdio transport; do not expose unauthenticated WebSocket listeners; avoid printing tokens or app-server auth details.
- Security reviewer: inline fallback unless high-risk surfaces appear.
- Security review result: inline review passed; no P0-P2 findings remain.
- Required security verification: app-server launch uses stdio only; unknown JSON-RPC notifications are ignored; no WebSocket listener or token-bearing transport is introduced.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust process management, async/thread handling, frontend type contracts, terminal UI tests.
- Preventive evidence: focused Rust/frontend tests plus build before release handoff.
- If CI breaks: invoke krt-ci-questor with run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 Codex app-server chat and checkpoint runtime.
- Branch name: feat/codex-app-server-runtime.
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop.
- Suggested commit grouping for this review unit:
  - `feat(agent-console): add codex app-server runtime` - backend runtime, event parsing, checkpoint integration, tests.
  - `feat(agents): route codex chat through structured runtime` - frontend contract/UI wiring and tests.
  - `docs(agent-console): document codex app-server runtime` - contract and orchestration artifacts.
- PR title: Use Codex app-server for agent sessions
- PR body bullets:
  - Run Codex sessions through app-server lifecycle and change events.
  - Close Agent Lens turn checkpoints from structured Codex turn completion.
  - Preserve terminal-backed fallback behavior for other agents.
- Verification results location: update this work package and Compound Master state after execution.
- Production/deployment notes: none beyond preserving fallback behavior.
- Autonomous mutation request: none.

## Jira Handoff Inputs
- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone `Tarea`.
- PR-to-Jira mapping: single-review-unit PR maps to one standalone `Tarea`.
- Jira summary: Usar Codex app-server para sesiones de agente
- Jira description: Integrar Codex mediante app-server para capturar chat, eventos de turno y cambios estructurados, manteniendo el runtime de terminal como fallback para otros agentes.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" or the actual reason in state/release closeout and continue.
