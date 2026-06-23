---
title: WSL Agent Console parity
status: review-passed
roadmap_item: RDM-009
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-009-wsl-agent-console.md
origin_planning_input: docs/brainstorms/2026-06-23-009-wsl-agent-console.md
origin_plan: docs/plans/2026-06-23-009-wsl-agent-console-plan.md
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

# WSL Agent Console parity

## Scope
Enable Agent Console lifecycle support for Ubuntu WSL repos in the active workbench while preserving existing Windows/local repo behavior. The package covers backend source-aware start, WSL PTY launch, repo-aware availability checks, UI honesty for revert availability, docs, and tests.

## Non-goals
Multi-distro selection beyond Ubuntu, networked remote agents, shell profile management, and release packaging are excluded. Releases remain deferred to the final batched Release Marshal.

## Autonomy Contract
- Mode: guarded
- Agent may decide without asking: internal names, equivalent test commands, WSL command builder shape that preserves static script plus argv-passed data, and compatible UI wording for disabled revert.
- Agent must record as assumptions: Ubuntu-only behavior, skipped real WSL smoke if the host lacks WSL/Ubuntu, and any explicit parity gap left behind.
- Agent must escalate: public command removal, destructive repo behavior beyond existing revert semantics, branch/base/release changes, paid credentials, or broad scope outside Agent Console WSL parity.
- Safe fallback: implement source-aware launch and explicit no-fake-checkpoint behavior, then record any remote checkpoint hardening as a named remaining package.
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies
- Requires: RDM-001 source-aware repo identity, RDM-002/RDM-006 Ubuntu agent availability model, and RDM-004 WSL runtime repo projection.
- Blocks: final WSL manual smoke and release batch.

## Production Posture
- Posture: prototype
- Evidence: current Compound Master state marks this initiative as prototype and user is actively shaping WSL complement behavior.
- Confidence: high
- Consequences for this package: preserve compatibility and fail closed, but no migration or production rollout protocol is required.
- Breaking existing behavior allowed: no

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Source-aware start is required for WSL sessions. |
| U2 | yes | PTY launch is the core backend capability. |
| U3 | yes | Availability must match repo source to avoid false Windows-only blocks. |
| U4 | yes | Revert/checkpoint honesty is part of serious session safety. |
| U5 | yes | Verification and review are required before final release queueing. |

Grouping rationale:
- One integrated review unit is justified because the public session lifecycle is only usable when backend start, PTY IO, availability, terminal affordances, contract docs, and tests land together. Splitting would create a launch path that could still be blocked by UI availability or expose misleading revert controls.

## Implementation Units
- U1: make `start_agent_session` resolve repo identity and branch local versus WSL session start.
- U2: extend the PTY factory with WSL command construction and process launch.
- U3: add repo-aware agent availability command and frontend wrapper usage.
- U4: represent WSL sessions without fake local checkpoints unless remote checkpoint is implemented in this package; disable/reject revert honestly.
- U5: update docs/state and run targeted verification.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL Agent Console parity | `src-tauri/src/agent_console/*`, `src-tauri/src/lib.rs`, `src/bus/*`, repo card/terminal tests, contract docs, orchestration docs | develop | Optional new standalone Tarea | Medium risk external process launch; docs kept with code for reviewer context. |

## Files and Tests
- Expected files: `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/agent_console/pty.rs`, `src-tauri/src/agent_console/session.rs`, `src-tauri/src/agent_console/validation.rs`, `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/bus/contract.test.ts`, `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/terminal/TerminalPanel.tsx`, `src/panels/terminal/TerminalPanel.test.tsx`, `docs/contracts/bus-contract.md`.
- Expected tests: `cargo test --lib agent_console`, `cargo test --lib invoke_handler`, `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/terminal/TerminalPanel.test.tsx`, `npx tsc --noEmit`, `git diff --check`.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri command for repo-aware agent availability; existing session commands remain.
- Consumer scan patterns: `agent_binary_available|agent_binary_available_for_repo|start_agent_session|revert_session|checkpoint|RepoSource::Wsl|spawn_wsl_agent`.
- Consumers found: `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/agent_console/pty.rs`, `src-tauri/src/agent_console/session.rs`, `src-tauri/src/agent_console/validation.rs`, `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/bus/contract.test.ts`, `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/DashboardPanel.test.tsx`, `src/panels/terminal/TerminalPanel.tsx`, `src/panels/terminal/TerminalPanel.test.tsx`, and `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: frontend invoke-name tests and Rust invoke handler test.
- Required consumer tests: contract wrapper tests, repo card availability tests, terminal revert affordance tests, agent console backend tests.
- Consumer tests run/skipped: `cargo test --lib agent_console` passed 41/41; `cargo test --lib invoke_handler` passed 1/1; `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/DashboardPanel.test.tsx src/panels/terminal/TerminalPanel.test.tsx` passed 47/47; `npx tsc --noEmit` passed; `git diff --check` passed with CRLF warnings only.

## Verification Gate
- `cargo test --lib agent_console`
- `cargo test --lib invoke_handler`
- `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/terminal/TerminalPanel.test.tsx`
- `npx tsc --noEmit`
- `git diff --check`
- Surface-aware evidence: backend command routing, WSL command construction, frontend availability/revert behavior, and contract docs.
- Production posture evidence: compatibility-preserving, additive command only, no existing command removal.

## Review Gate
- Code review threshold: P0-P2
- Findings below threshold: log unless user marks blocking

## Security Gate
- Run after work-review loop: required because the change launches external processes and handles repo mutation/revert safety.
- Security Watch during work: enabled for process invocation, argv quoting, path containment, and no-fake-checkpoint behavior.
- Security Watch notes: WSL command construction uses a static `sh -lc` script and passes repo path plus agent id as argv; agent ids are allowlisted before availability checks and launch; local sessions keep existing checkpoint/revert behavior; WSL sessions expose `checkpoint: null` and Terminal disables Revert rather than advertising an unsupported rollback.
- Security reviewer: inline fallback if canonical reviewer unavailable.
- Security review result: pass with residual advisory to implement remote WSL checkpoint/revert before claiming full rollback parity.
- Required security verification: tests for argv construction and safe missing-binary errors; inspection of command construction and environment handling.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust compile/tests, Tauri invoke registration, frontend contract wrappers, TypeScript.
- Preventive evidence: local targeted tests and typecheck before release handoff.
- If CI breaks: invoke `krt-ci-questor` with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 WSL Agent Console parity
- Branch name: feat/wsl-agent-console-parity
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch; no separate docs-only branch.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `feat(agent-console): run sessions from WSL repos in Ubuntu` - backend session routing, WSL PTY launch, availability command, frontend affordances, tests, and contract docs.
  - `docs(orchestration): add WSL Agent Console delivery artifacts [skip ci]` - brainstorm, plan, package, review findings, and state updates.
- PR title: Run Agent Console sessions from WSL repos
- PR body bullets:
  - Route Agent Console launch and availability checks by repo source so WSL repos execute inside Ubuntu.
  - Preserve local session behavior and keep WSL revert/checkpoint behavior explicit.
  - Add backend/frontend coverage for WSL launch routing and terminal controls.
- Verification results location: this work package and `docs/review-findings/2026-06-23-rdm-009-code-security-review.md`
- Production/deployment notes: final release must include manual Windows/Ubuntu smoke.
- Autonomous mutation request: none

## Jira Handoff Inputs
- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone Tarea unless a broader WSL complement parent already exists.
- Jira summary: Ejecutar Agent Console en repositorios WSL
- Jira description: Permitir que las sesiones de Agent Console iniciadas desde repositorios WSL se ejecuten dentro de Ubuntu, manteniendo la compatibilidad de repos locales y controles honestos de revert/checkpoint.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.

## Implementation Results
- Status: review-passed locally; queued for final release batch.
- Backend: `start_agent_session` now resolves repo identity and routes local repos to the existing host PTY/checkpoint path while WSL repos launch inside Ubuntu through `wsl.exe`.
- PTY: WSL launch command changes directory to the Linux repo and executes the allowlisted agent with argv-passed values; Codex keeps `--no-alt-screen`.
- Frontend: repo cards call `agent_binary_available_for_repo(repo, agentType)` so mixed Windows/WSL workbenches check the correct environment. Dashboard and RepoCard tests cover the repo-aware call.
- Revert honesty: WSL sessions currently have `checkpoint: null`; Terminal disables Revert and backend returns `checkpoint_unsupported` if a no-checkpoint session is reverted directly. This is intentionally safer than a fake local checkpoint, but remote WSL checkpoint/revert remains a named follow-up before claiming complete Agent Console rollback parity.
- Verification: `cargo test --lib agent_console` 41/41, `cargo test --lib invoke_handler` 1/1, affected Vitest suite 47/47, `npx tsc --noEmit`, and `git diff --check` passed.
