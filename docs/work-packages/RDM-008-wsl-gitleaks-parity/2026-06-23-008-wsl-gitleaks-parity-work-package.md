---
title: WSL Gitleaks parity
status: review-passed
roadmap_item: RDM-008
origin_roadmap: docs/orchestration/compound-master-state.md
origin_brainstorm: docs/brainstorms/2026-06-23-008-wsl-gitleaks-parity.md
origin_planning_input: docs/brainstorms/2026-06-23-008-wsl-gitleaks-parity.md
origin_plan: docs/plans/2026-06-23-008-wsl-gitleaks-parity-plan.md
units: [U1, U2, U3, U4]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: local-final-batch
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# WSL Gitleaks Parity

## Scope

Add source-aware Gitleaks commands so WSL repos use Gitleaks and `.gitleaks.toml` inside Ubuntu through `tinto-agent`, while existing global host Addons commands remain backward compatible.

## Non-goals

- No Gitleaks UI redesign.
- No automatic Gitleaks install.
- No Agent Console or fine-grained `fs-events`.
- No release, push, PR, merge, or Jira mutation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide additive command names and protocol helper names.
- Agent must escalate UI behavior changes beyond additive wrappers.
- Agent must record any Windows/Ubuntu smoke gap.
- Safe fallback: keep host global commands unchanged and fail WSL repo commands closed.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-004, RDM-006.
- Blocks: final WSL release parity.

## Production Posture

- Posture: prototype.
- Evidence: active orchestration state.
- Confidence: high.
- Consequences for this package: preserve public existing commands and add repo-aware commands.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | DTO reuse is needed for protocol results. |
| U2 | yes | WSL support lives in the agent. |
| U3 | yes | Frontend needs additive repo-aware wrappers. |
| U4 | yes | Docs/state/review track release readiness. |

Grouping rationale:
- One cohesive Gitleaks capability slice with one public additive surface and one agent protocol change.

## Implementation Units

- U1: Reuse setup/install DTOs in protocol.
- U2: Add agent status/install/config handlers.
- U3: Add host commands/wrappers and route config creation.
- U4: Update docs/review/state.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Source-aware Gitleaks for WSL repos | `src-tauri/src/bus/commands.rs`, `src-tauri/src/wsl_agent/*`, `src/bus/*`, docs/tests | `develop` with queued WSL work | optional Tarea | Medium risk; security tooling and repo writes for `.gitleaks.toml`. |

## Files and Tests

- Runtime/frontend: `src-tauri/src/bus/commands.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src/bus/client.ts`, `src/bus/contract.ts`, `src/bus/contract.test.ts`.
- Docs: `docs/contracts/bus-contract.md`, this work package, review findings, state.
- Tests: WSL agent, bus, frontend contract/repo tests, TypeScript, checker, diff check.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri commands and internal agent protocol variants.
- Consumer scan patterns: `rg "gitleaks|Gitleaks|create_repo_gitleaks_config|get_gitleaks_setup_status|install_gitleaks" src src-tauri docs`.
- Consumers found: AddonsManager, GitleaksConfigNotice, RepoCard, RepoPanel, bus client/contract tests, backend bus commands, WSL runtime/protocol.
- Contract-drift tests searched: frontend contract tests and Rust protocol/runtime tests.
- Required consumer tests: frontend contract and repo notice tests; WSL agent/bus tests.
- Consumer tests run/skipped: WSL agent tests, bus tests, invoke-handler test, frontend contract/repo tests, TypeScript, tinto-agent build, work package checker, and `git diff --check` passed. Real Windows/Ubuntu Gitleaks smoke remains pending for final release.

## Verification Gate

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`
- `npx tsc --noEmit`
- Work package checker
- `git diff --check`

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate

- Run after work-review loop: required because this touches secret scanning, installer commands, and repo config writes.
- Security Watch during work: enabled.
- Security Watch notes: no secret values in messages, active-workbench allowlist before repo write, no shell interpolation with repo paths, preserve host global command compatibility.
- Security reviewer: inline fallback.
- Security review result: passed by inline fallback on 2026-06-23.
- Required security verification: tests plus direct inspection.

## Implementation Summary

- Moved Gitleaks setup/install DTOs into the Rust bus contract so they can be shared by Tauri commands and the WSL agent protocol.
- Added repo-aware Tauri commands: `get_repo_gitleaks_setup_status` and `install_repo_gitleaks`.
- Kept existing global `get_gitleaks_setup_status` and `install_gitleaks` host-scoped for backward compatibility.
- Routed `create_repo_gitleaks_config` by repo source: local repos write on the host, WSL repos write through `tinto-agent`.
- Added WSL agent protocol/runtime support for Gitleaks status, install, and config creation.
- Added frontend client wrappers and contract tests for the repo-aware commands.

## Review Result

- Status: `review-passed`.
- Findings path: `docs/review-findings/2026-06-23-rdm-008-code-security-review.md`.
- No P0-P2 findings remain.
- Honest note: final release smoke still needs a real Windows/Ubuntu check for Gitleaks setup/config and secret findings in a WSL repo.

## Verification Results

- `cargo test --lib wsl_agent`: passed, 23 tests.
- `cargo test --lib bus -- --test-threads=1`: passed, 42 tests.
- `cargo test --lib invoke_handler`: passed, 1 test.
- `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`: passed, 47 tests.
- `cargo build --bin tinto-agent`: passed.
- `npx tsc --noEmit`: passed.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust protocol/runtime tests, frontend contract tests, TypeScript.
- Preventive evidence: local verification.
- If CI breaks: direct evidence-first triage.
- Escalation rule: final release remains blocked until Windows/Ubuntu smoke covers WSL Gitleaks status/config or the gap is explicitly waived.

## Branch and PR Handoff Inputs

- Review unit: RU1 Source-aware Gitleaks for WSL repos.
- Branch name: `feat/wsl-gitleaks-parity`
- Branch/docs rule: keep docs with implementation for final batch.
- PR base: `develop`.
- Suggested commit grouping for this review unit:
  - `feat(security): route Gitleaks for WSL repos` - backend/protocol/frontend wrappers/tests - enables repo-aware Gitleaks status/install/config.
  - `docs(wsl): record Gitleaks parity` - package/state/review docs - records verification and smoke gap.
- PR title: `Route Gitleaks for WSL repos`
- PR body bullets:
  - Adds repo-aware Gitleaks setup/install/config commands that route WSL repos through the Ubuntu agent.
  - Keeps existing global host Addons commands backward compatible.
  - Preserves active-workbench allowlisting and safe error categories for WSL repo config writes.
- Verification results location: this work package and review findings.
- Production/deployment notes: Windows/Ubuntu Gitleaks smoke remains final-release evidence.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone `Tarea` unless a real multi-child parent exists.
- Jira summary: `Soportar Gitleaks en repos WSL`
- Jira description: `Hacer que el estado, instalacion y configuracion de Gitleaks puedan ejecutarse en el entorno correcto para repos Ubuntu WSL, sin romper los comandos globales existentes del host.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.
