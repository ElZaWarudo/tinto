---
title: Windows-only WSL workbench source and path UX
status: review-passed
roadmap_item: RDM-003
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-003-wsl-workbench-path-ux.md
origin_plan: docs/plans/2026-06-23-003-wsl-workbench-path-ux-plan.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1, RU2, RU3]
base_branch: develop
pr_strategy: local-final-batch
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Windows-only WSL Workbench Source and Path UX

## Scope

Deliver the Windows-only configuration UX for adding an Ubuntu WSL repo to a workbench by Linux path. This package persists and displays WSL repo source metadata but does not monitor, read, diff, watch, mutate, or open agent sessions for WSL repos yet.

Confirmed product decisions:
- WSL baseline: WSL 2 only.
- Initial distro: Ubuntu.
- One selected distro per WSL repo.
- RDM-003 is configuration-only; RDM-004 owns live WSL monitoring.
- Releases are deferred until the end of the active Compound Master run.

## Non-goals

- No WSL browse/list flow.
- No distro discovery or multi-distro support beyond fixed Ubuntu.
- No agent launch, health probe, read/watch/event forwarding, file tree, diff, media preview, Gitleaks, file operations, or agent-console routing for WSL repos.
- No `\\wsl$` path translation as repo identity.
- No Linux desktop WSL UI, commands, settings, empty states, degraded notices, or behavior.
- No SSH, cloud, container, arbitrary remote host, or VS Code integration.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: helper/module names, exact typed error names for invalid WSL paths, component file names, and equivalent focused test names.
- Agent must record as assumptions: platform-gating strategy, any configured-but-not-monitored WSL UI copy, and any skipped Windows manual smoke.
- Agent must escalate: adding live WSL monitoring, launching `tinto-agent`, enabling WSL file operations/Gitleaks/media/agent-console behavior, adding non-Ubuntu distro support, changing release timing, or weakening Linux absence.
- Safe fallback: keep changes at artifact or tests-only level and ask before expanding scope.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-001 delivered to `origin/develop`.
- Requires locally: RDM-002 implemented/review-passed in the current dirty worktree.
- Blocks/enables: RDM-004 and RDM-005.

## Production Posture

- Posture: prototype.
- Evidence: `docs/orchestration/compound-master-state.md` records prototype posture and local desktop iteration flow.
- Consequences: manual Windows smoke may be recorded as a final release gap, but Linux absence and local regression tests are required before review-passed.
- Breaking existing behavior allowed: no.

## Implementation Units

- U1 - Backend WSL workbench persistence and Windows-only command.
- U2 - Frontend contract, platform gate, and client wrapper.
- U3 - Windows-only add WSL UX, configured-entry labels, and absence/regression verification.

## Review Unit Progress

| Review unit | Status | Notes |
|---|---|---|
| RU1 | review-passed | Backend persistence, validation, duplicate/removal semantics, and Windows-only command registration implemented and verified. |
| RU2 | review-passed | Frontend contract additions, platform helper, client wrapper, absence test strategy, and bus contract docs implemented and verified. |
| RU3 | review-passed | Windows-only UI, configured labels, impact scan, security review, and closeout evidence complete. |

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Backend WSL workbench persistence and Windows-only command registration | `src-tauri/src/workbench/mod.rs`, `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs`, focused Rust tests | `develop` with queued RDM-002 local changes | optional Tarea | Medium/high; persistence and invoke registration must preserve local behavior and Linux absence. |
| RU2 | Frontend contract additions, platform gate, and client wrapper | `src/bus/contract.ts`, `src/bus/client.ts`, `src/workbench/platform.ts`, `src/workbench/wslAbsence.test.ts`, focused TS tests | RU1 integrated | optional Tarea | Medium; WSL strings become intentional but must remain gated. |
| RU3 | Windows-only add WSL UX, configured WSL labels, and final verification | `src/workbench/operations.ts`, `src/workbench/MenuBar.tsx`, optional `src/workbench/AddWslRepoDialog.tsx`, workbench tests, state/package docs | RU2 integrated | optional Tarea | Medium/high; visible UX must not create live bus actions before RDM-004. |

## Files and Tests

Expected files:
- `src-tauri/src/workbench/mod.rs`
- `src-tauri/src/workbench/commands.rs`
- `src-tauri/src/lib.rs`
- `src/bus/contract.ts`
- `src/bus/client.ts`
- `src/workbench/operations.ts`
- `src/workbench/MenuBar.tsx`
- optional `src/workbench/platform.ts`
- optional `src/workbench/AddWslRepoDialog.tsx`
- `src/workbench/wslAbsence.test.ts`
- `src/workbench/workbench.test.tsx`
- `src/workbench/operations.test.ts`
- `docs/orchestration/compound-master-state.md`

Expected tests:
- WSL path validation accepts `/home/user/repo` and rejects blank, relative, Windows drive, and UNC-like paths.
- WSL add persists source/distro/path/alias/fs_watch without local git validation.
- Duplicate detection uses source+distro+path for WSL repos.
- Local and WSL same-text paths can coexist.
- WSL remove removes only the matching WSL entry.
- Local add/remove/update/reorder behavior remains unchanged.
- `add_wsl_repo` is absent from non-Windows invoke registration.
- Frontend menu/form render WSL controls only when the platform helper reports Windows.
- Configured WSL labels render on Windows and do not render on Linux.
- WSL client wrapper is isolated from local add/autodetect flows.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: workbench config payload becomes additive with optional frontend `source`/`distro`; backend may add Windows-only `add_wsl_repo`.
- Consumer scan pattern: `rg "RepoEntry|RepoSource|runtime_config|active_runtime_repos_for|list_workbenches|add_repo|remove_repo|update_repo|wslAbsence|displayName|sortedRepoPaths|addRepoFlow|MenuBar|invoke_handler|add_wsl_repo" src src-tauri docs/contracts`.
- Expected consumers: workbench store/commands, lib invoke handler, bus unsupported-source resolver, frontend bus contract/client/store, workbench menu/operations/tests, dashboard/project labels if touched.
- Required consumer tests: targeted Rust workbench/invoke/bus tests and targeted frontend workbench/operations/absence tests.
- Consumer tests run/skipped: RU1 backend impact scan complete; RU2 frontend contract/wrapper/gate tests complete; RU3 visible UI and absence tests complete.

## Verification Gate

- `cargo test --lib workbench`
- `cargo test --lib invoke_handler`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/workbench/workbench.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts`
- `npx tsc --noEmit`
- Relevant Prettier checks for changed TypeScript/TSX files
- Rust formatting for changed Rust files, with explicit note if existing `secret_scan.rs` drift prevents global `cargo fmt --check`
- `git diff --check`
- Optional final-release Windows smoke: add Ubuntu + `/home/...` WSL repo and verify it appears configured but not monitored

RU1 verification:
- `cargo test --lib workbench`: 33 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `rustfmt --edition 2021 --check --config skip_children=true src\workbench\mod.rs src\workbench\commands.rs src\lib.rs`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.

RU2 verification:
- `npm test -- src/bus/contract.test.ts src/workbench/wslAbsence.test.ts src/workbench/operations.test.ts`: 91 passed.
- `npx tsc --noEmit`: passed.
- `npx prettier --check src\bus\contract.ts src\bus\client.ts src\bus\contract.test.ts src\workbench\platform.ts src\workbench\wslAbsence.test.ts`: passed.

RU3 verification:
- `npm test -- src/workbench/workbench.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts src/bus/contract.test.ts`: 109 passed.
- `npx tsc --noEmit`: passed.
- `npx prettier --check src\workbench\AddWslRepoDialog.tsx src\workbench\MenuBar.tsx src\workbench\operations.ts src\workbench\workbench.test.tsx src\workbench\operations.test.ts src\workbench\wslAbsence.test.ts src\bus\client.ts src\bus\contract.ts src\bus\contract.test.ts`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Artifact review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-003-work-package-review.md`. Mechanical checker passed with an accepted warning that orchestration docs and runtime files are mixed but split into RU1/RU2/RU3.
- RU1 code review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-003-ru1-code-review.md`.
- RU2 code review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-003-ru2-code-review.md`.
- RU3 code review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-003-ru3-code-review.md`.
- Package code review result: passed.

## Security Gate

- Run after work-review loop: required, because this package accepts user-provided Linux paths and introduces a Windows-only command.
- Security Watch during work: enabled.
- Security Watch notes: no Windows filesystem canonicalization for WSL identity, no `\\wsl$` traversal, no agent launch, no local git/file/bus command route, safe typed errors, no non-Windows WSL command/UI.
- Security review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-003-security-review.md`.
- Security verification: backend/frontend WSL path validation is lexical; WSL commands are Windows-only; WSL entries remain unsupported by bus live routes before RDM-004; non-Windows UI stays hidden; no agent launch, shell execution, `\\wsl$` traversal, secret handling, or local filesystem canonicalization was added.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust compile/invoke cfg, frontend platform tests, TypeScript contract additions, existing absence test strategy, and formatting.
- Preventive evidence: targeted Rust tests, targeted frontend tests, typecheck, formatting checks, and `git diff --check`.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: release follow-up is blocked until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1/RU2/RU3 batched only after all active work packages complete, unless the user changes release timing.
- Branch name: `feat/wsl-workbench-path-ux`
- Branch/docs rule: related planning artifacts ship with the final batched release; no intermediate release for this package.
- PR base: `develop` if the user requests PR flow; otherwise final local no-PR release targets `develop` over `origin/develop`.
- Suggested commit grouping for this review unit:
  - `feat(wsl): persist Windows WSL workbench repos` - backend WSL repo add/remove validation and Windows-only command.
  - `feat(wsl): add Windows WSL repo configuration UI` - frontend contract, platform gate, add dialog, configured labels, tests.
  - `docs(orchestration): add WSL workbench path UX artifacts [skip ci]` - requirements, plan, package, findings, and state.
- PR title: `Add Windows WSL repo configuration UX`
- PR body bullets:
  - Add Windows-only Ubuntu WSL repo configuration by Linux path.
  - Preserve local repo add/remove behavior and keep WSL repos out of live monitoring until RDM-004.
  - Keep WSL UI and commands absent outside Windows.
- Verification results location: this package and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: RDM-003 is configuration-only; live WSL monitoring requires RDM-004.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea unless later WSL packages are grouped under a parent.
- Jira summary: `Agregar configuracion de repos WSL en Windows`
- Jira description: `Permitir configurar repos WSL de Ubuntu por path Linux en Windows, conservando repos locales y sin exponer superficies WSL en Linux.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
