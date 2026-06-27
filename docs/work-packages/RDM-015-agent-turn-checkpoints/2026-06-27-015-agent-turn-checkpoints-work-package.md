---
title: Agent turn checkpoints
status: review-passed
roadmap_item: RDM-015
origin_roadmap: docs/roadmaps/2026-06-27-006-agent-turn-checkpoints-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-27-015-agent-turn-checkpoints.md
origin_planning_input: docs/brainstorms/2026-06-27-015-agent-turn-checkpoints.md
origin_plan: docs/plans/2026-06-27-015-agent-turn-checkpoints-plan.md
units: [U1, U2, U3, U4, U5]
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

# Agent turn checkpoints

## Scope
Implement Agent Lens turn checkpoints for Agent Console sessions. The package adds conservative turn detection, changed-turn checkpoint records, per-file revert from a checkpoint, and a UI that separates these session-scoped changes from global Git state.

## Implementation Evidence
- RU1 implemented on 2026-06-27.
- Backend: session-owned turn detector, changed-turn checkpoint records, local per-file checkpoint revert, WSL protocol/runtime per-file checkpoint revert, and PTY output activity tracking.
- Frontend: Agent Lens strip in TerminalPanel, turn checkpoint/file list, polling refresh while the panel is open, and per-file revert command wrapper.
- Contract: additive `turn_status`, `turn_checkpoints`, and `revert_session_turn_file` documentation/types.
- Focused verification passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1`
  - `cargo test --manifest-path src-tauri/Cargo.toml wsl_agent -- --test-threads=1`
  - `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run`
  - `npx prettier --check src/bus/contract.ts src/bus/client.ts src/agent/sessionStore.ts src/panels/terminal/TerminalPanel.tsx src/panels/terminal/TerminalPanel.test.tsx src/bus/contract.test.ts src/App.css`
- Broad verification passed:
  - `npm test -- --run` (404/404)
  - `npm run lint` (0 errors; existing warnings outside RDM-015 files)
  - `npm run build` (existing chunk-size warning)
  - `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1` (229/229)
  - `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
  - `npm run format:check`
  - `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
  - `git diff --check` (CRLF warnings only)
- Review artifacts:
  - Code review: `docs/review-findings/2026-06-27-rdm-015-code-review.md`
  - Security review: `docs/review-findings/2026-06-27-rdm-015-security-review.md`

## Non-goals
- Agent-emitted semantic events.
- Empty checkpoints for turns without file changes.
- Perfect authorship attribution.
- Commit-from-turn or PR generation.
- Whole-turn revert.

## Autonomy Contract
- Mode: guarded.
- Agent may decide without asking: internal naming, exact quiet-threshold constants, equivalent deterministic test helpers, and small fixture updates.
- Agent must record as assumptions: quiet-threshold choices, any WSL parity limitation discovered during implementation, and any skipped manual smoke.
- Agent must escalate: changing existing session-level revert semantics, removing public contract fields, destructive behavior beyond per-file rollback, branch/base strategy, Jira/PR workflow, or scope outside this package.
- Safe fallback: implement review-only Agent Lens and block per-file revert if containment or WSL parity cannot be proven.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies
- Requires: none beyond current Agent Console/checkpoint capabilities on `develop`.
- Blocks: future commit-from-turn and richer session audit workflows.

## Production Posture
- Posture: prototype.
- Evidence: existing Compound Master state identifies Tinto as prototype and standing delivery has favored direct `develop` pushes.
- Confidence: medium.
- Consequences for this package: strong tests are still required because revert is destructive.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Required foundation for turn state. |
| U2 | yes | Required to persist changed-turn checkpoint boundaries. |
| U3 | yes | User explicitly selected per-file revert as the rollback model. |
| U4 | yes | Product value depends on making Git changes and Agent Lens understandable. |
| U5 | yes | Contract and destructive behavior require tests and docs. |

Grouping rationale:
- The units are tightly coupled: the UI needs the contract shape, per-file revert needs checkpoint records, and tests must verify the integrated behavior. A single review unit is acceptable unless implementation exceeds size guardrails.

## Implementation Units
- U1. Session turn model and detector.
- U2. Turn checkpoint storage and scan.
- U3. Per-file revert command.
- U4. Frontend Agent Lens presentation.
- U5. Tests and contract documentation.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Agent turn checkpoints end to end | backend session/checkpoint/protocol, frontend terminal/session store, contracts/docs/tests | develop | optional new Tarea | High risk because per-file revert is destructive; keep changes additive and test containment. |

## Files and Tests
- Expected files: `src-tauri/src/agent_console/session.rs`, `src-tauri/src/agent_console/checkpoint.rs`, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `docs/contracts/bus-contract.md`.
- Expected tests: `src/panels/terminal/TerminalPanel.test.tsx`, `src/agent/sessionStore.test.ts` if added, `src/bus/contract.test.ts`, and focused Rust tests under `agent_console`, `wsl_agent`, and `bus`.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive AgentSession/turn checkpoint contract and per-file revert command/protocol.
- Consumer scan patterns: `AgentSession`, `AgentSessionChangeLog`, `revert_session`, `checkpoint`, `TerminalPanel`, `agent-session-output`.
- Consumers found: `src-tauri/src/agent_console/*`, `src-tauri/src/wsl_agent/*`, `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: `src/bus/contract.test.ts`, Rust contract serialization tests in `src-tauri/src/bus/contract.rs`, terminal panel tests.
- Required consumer tests: frontend contract/session/terminal tests and Rust agent_console/wsl_agent tests.
- Consumer tests run/skipped: full frontend test suite, terminal panel tests, bus contract tests, full Rust suite, focused agent_console, focused wsl_agent, and Rust bus contract tests passed. `src/agent/sessionStore.test.ts` was not added because normalization is covered through TerminalPanel and bus contract consumer tests.

## Verification Gate
- `npm test -- src/panels/terminal/TerminalPanel.test.tsx src/agent/sessionStore.test.ts`
- `npm test -- src/bus/contract.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_console`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib wsl_agent`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus`
- `npm run lint`
- `npm run build`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`
- Surface-aware evidence: include at least one manual or automated smoke showing a changed turn checkpoint and a per-file revert path.
- Production posture evidence: prototype posture does not relax destructive-revert containment.

## Review Gate
- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Code review result: passed; see `docs/review-findings/2026-06-27-rdm-015-code-review.md`.

## Security Gate
- Run after work-review loop: required because per-file revert is destructive.
- Security Watch during work: enabled for checkpoint containment and WSL allowlist behavior.
- Security Watch notes: per-file revert must remain repo-allowlisted, reject `.git`, reject path traversal, and avoid applying a stale checkpoint to the wrong repo.
- Security reviewer: krt-security-sentinel or inline fallback if unavailable.
- Security review result: passed; see `docs/review-findings/2026-06-27-rdm-015-security-review.md`.
- Required security verification: local and WSL tests proving per-file revert cannot escape the active repo and handles created/modified/removed files correctly.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust contract serialization, Tauri command registration, WSL protocol/runtime, frontend type contract, terminal tests.
- Preventive evidence: run focused frontend and Rust suites before release handoff; run broader lint/build/clippy gates if implementation changes shared contracts.
- If CI breaks: invoke krt-ci-questor with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 Agent turn checkpoints end to end.
- Branch name: feat/agent-turn-checkpoints.
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop.
- Suggested commit grouping for this review unit:
  - `feat(agent-console): capture changed agent turns as checkpoints` - backend session/checkpoint/protocol and tests.
  - `feat(terminal): show agent turn checkpoints separately from git state` - terminal UI, session store, contract mirror, frontend tests.
  - `docs(agent-console): document turn checkpoint contract` - contract docs and orchestration artifacts.
- PR title: Add agent turn checkpoints
- PR body bullets:
  - Capture changed Agent Console turns as reviewable checkpoints using conservative output/filesystem quiet detection.
  - Show Agent Lens changes separately from global Git state.
  - Add per-file revert from a selected turn checkpoint.
- Verification results location: update this work package and Compound Master state after execution.
- Production/deployment notes: destructive per-file revert requires containment evidence before release.
- Autonomous mutation request: none.

## Jira Handoff Inputs
- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone `Tarea` unless additional sibling review units are split during implementation.
- Jira summary: Anadir checkpoints por turno al Agent Lens
- Jira description: Agrupar los cambios producidos durante una sesion de agente en turnos detectados por quietud de consola y filesystem, diferenciarlos del estado Git global y permitir revert por archivo desde cada checkpoint.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" or the actual reason in state/release closeout and continue.
