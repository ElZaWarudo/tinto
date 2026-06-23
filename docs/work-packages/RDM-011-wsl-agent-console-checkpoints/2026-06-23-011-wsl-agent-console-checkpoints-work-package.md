---
title: WSL Agent Console remote checkpoints
status: review-passed
roadmap_item: RDM-011
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md
origin_planning_input: docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md
origin_plan: docs/plans/2026-06-23-011-wsl-agent-console-checkpoints-plan.md
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

# WSL Agent Console remote checkpoints

## Scope
Add remote checkpoint, change-log, and revert support for WSL Agent Console sessions while preserving the existing local session behavior and public command names.

## Non-goals
Long-lived remote session daemon, non-Ubuntu distros, UI redesign, and release shipping are excluded.

## Autonomy Contract
- Mode: guarded
- Agent may decide without asking: internal enum names, protocol variant names, equivalent test commands, and minor DTO placement.
- Agent must record as assumptions: Ubuntu-only launch path and skipped real WSL smoke if environment is unavailable.
- Agent must escalate: destructive behavior beyond existing explicit revert, public command removal, branch/base/release changes, or credentials.
- Safe fallback: fail WSL session start if checkpoint cannot be created.
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies
- Requires: RDM-009 WSL Agent Console launch and RDM-006 packaged-first agent launcher.
- Blocks: final Agent Console WSL smoke and release batch.

## Production Posture
- Posture: prototype
- Evidence: active local desktop development state.
- Confidence: high
- Consequences for this package: preserve compatibility and keep destructive revert consent-gated.
- Breaking existing behavior allowed: no

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Protocol operations are needed for remote checkpoint lifecycle. |
| U2 | yes | Runtime handlers run the checkpoint logic in Ubuntu. |
| U3 | yes | Registry must scan/revert through the correct backend. |
| U4 | yes | Start must create checkpoint before spawning the agent. |
| U5 | yes | Verification and review are required. |

Grouping rationale:
- One RU is justified because the checkpoint lifecycle is only safe if create, scan, revert, registry state, and docs land together.

## Implementation Units
- U1: add checkpoint protocol DTOs.
- U2: implement agent-side create/scan/revert.
- U3: add local/WSL checkpoint backend handling in session records.
- U4: create remote checkpoint before WSL PTY spawn.
- U5: verify and review.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL Agent Console checkpoints | `src-tauri/src/agent_console/*`, `src-tauri/src/wsl_agent/*`, docs/tests | develop | Optional standalone Tarea | High risk destructive revert, but consent-gated and confined to existing repo checkpoint behavior. |

## Files and Tests
- Expected files: `src-tauri/src/agent_console/checkpoint.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/agent_console/session.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `docs/contracts/bus-contract.md`.
- Expected tests: `cargo test --lib wsl_agent`, `cargo test --lib agent_console`, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, `git diff --check`.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal WSL protocol only; public Tauri session commands unchanged.
- Consumer scan patterns: `AgentCheckpoint|CheckpointRecord|revert_session|scan_change_log|start_wsl_session`.
- Consumers found: `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/agent_console/session.rs`, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src/panels/terminal/TerminalPanel.tsx`, and `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: WSL protocol/runtime tests and Agent Console tests.
- Required consumer tests: remote checkpoint runtime roundtrip and Agent Console regression suite.
- Consumer tests run/skipped: `cargo test --lib wsl_agent` passed 26/26 and `cargo test --lib agent_console` passed 41/41. Real Windows/Ubuntu WSL smoke is deferred to the final packaging/smoke package.

## Verification Gate
- `cargo test --lib wsl_agent`
- `cargo test --lib agent_console`
- `cargo build --bin tinto-agent`
- `npx tsc --noEmit`
- `git diff --check`

Verification results:
- `cargo test --lib wsl_agent` passed 26/26.
- `cargo test --lib agent_console` passed 41/41.
- `cargo build --bin tinto-agent` passed with pre-existing warnings only (`GITLEAKS_GO_PACKAGE`, `PollDetected.repo`).
- `npx tsc --noEmit` passed.
- `git diff --check` passed with CRLF warnings only.
- Work package checker passed.

## Review Gate
- Code review threshold: P0-P2

## Security Gate
- Run after work-review loop: required because revert is destructive.
- Security Watch during work: enabled for allowlist, repo confinement, consent preservation, and no host-path rollback.
- Security Watch notes: WSL checkpoint create/scan/revert is repo-allowlisted; scan/revert requests use the checkpoint's Linux repo path, not a host-translated path; destructive revert still requires `revert_session` consent; local checkpoint behavior is unchanged.
- Security reviewer: inline fallback if canonical reviewer unavailable.
- Security review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-011-code-security-review.md`.
- Required security verification: runtime test proving remote revert restores/deletes expected files and rejects outside allowlist.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust compile/tests, protocol compatibility, destructive test fixtures.
- Preventive evidence: targeted tests and diff hygiene.
- If CI breaks: invoke `krt-ci-questor` with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 WSL Agent Console checkpoints
- Branch name: feat/wsl-agent-console-checkpoints
- Branch/docs rule: keep docs with implementation.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `feat(agent-console): checkpoint and revert WSL sessions` - protocol, runtime, registry, tests, docs.
  - `docs(orchestration): add WSL Agent Console checkpoint artifacts [skip ci]`
- PR title: Checkpoint and revert Agent Console sessions in WSL
- PR body bullets:
  - Create Agent Console checkpoints inside Ubuntu before WSL session launch.
  - Scan change logs and revert WSL sessions through `tinto-agent`.
  - Preserve local session behavior and explicit user consent.
- Verification results location: this package and `docs/review-findings/2026-06-23-rdm-011-code-security-review.md`
- Production/deployment notes: final smoke should create, modify, delete, stop, and revert a WSL session.
- Autonomous mutation request: none

## Jira Handoff Inputs
- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone Tarea unless a broader WSL complement parent already exists.
- Jira summary: Revertir sesiones de Agent Console en WSL
- Jira description: Crear checkpoints y ejecutar revert de sesiones de Agent Console dentro de Ubuntu para repositorios WSL, manteniendo consentimiento explícito y comportamiento local existente.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.
