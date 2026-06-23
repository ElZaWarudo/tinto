---
title: Windows WSL agent bootstrap and minimal stdio protocol
status: review-passed
roadmap_item: RDM-002
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md
origin_planning_input: docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md
origin_plan: docs/plans/2026-06-23-002-wsl-agent-bootstrap-protocol-plan.md
units: [U1, U2, U3, U4]
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

# Windows WSL agent bootstrap and minimal stdio protocol

## Scope

Deliver the first Windows-only WSL `tinto-agent` bootstrap foundation: internal handshake DTOs, a Linux-side agent skeleton, a Windows-gated launcher seam, safe health categories, Linux absence tests, and a Windows/Ubuntu WSL manual smoke checklist.

Confirmed product decisions:
- First supported WSL baseline: WSL 2 only.
- First-release distro scope: one selected distro per WSL repo.
- Initial manual smoke distro: Ubuntu.
- Releases are deferred until the end of the active Compound Master run.

## Non-goals

- No WSL repo picker, distro selector UI, Linux path entry UI, or WSL browse/list flow.
- No core WSL read/watch path.
- No WSL routing for media preview, secret findings, Gitleaks, file operations, or agent console sessions.
- No full JSON-RPC framework.
- No auto-install/update model; the first implementation uses dev-only build/run from source inside Ubuntu.
- No Linux desktop WSL feature surface.
- No SSH, cloud, container, or arbitrary remote host support.

## Availability Model

OD1 was resolved on 2026-06-23: `tinto-agent` is made available inside Ubuntu through dev-only build/run from source for the first implementation.

Implementation consequence:
- The Windows launcher must accept an argument-vector style dev command for the agent.
- Tests should inject the command vector rather than hard-coding packaging behavior.
- RDM-006 owns any later app-managed copy, updater, or production packaging behavior.

Safe scope:
- Implement protocol, launcher seam, and smoke checklist for the dev-only availability model.
- Do not implement app-managed install/update.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: internal module names, DTO field names that preserve requirements, fixture command names for mocked tests, and equivalent focused test names.
- Agent must record as assumptions: any mocked launcher behavior, any skipped Windows/Ubuntu smoke, and any compatibility interpretation for non-Windows absence gates.
- Agent must escalate: `tinto-agent` availability model, public command contract changes, WSL UI/product behavior, repo mutation policy, agent-console routing policy, destructive behavior, branch/base strategy, or release mutation before final batch.
- Safe fallback: continue artifact validation and local read-only inspection; otherwise ask the OD1 question.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-001 delivered to `origin/develop`; current local dirty worktree includes queued RUL-001 changes with release deferred.
- Blocks: RDM-003, RDM-004, RDM-006.

## Production Posture

- Posture: prototype.
- Evidence: `docs/orchestration/compound-master-state.md` records prototype posture and local desktop iteration flow.
- Confidence: high.
- Consequences for this package: compatibility and safe process-boundary behavior still matter, but no production migration or rollout plan is required.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Protocol DTOs and agent skeleton are the foundation for the bootstrap handshake. |
| U2 | yes | Windows-only launcher seam is the core RDM-002 outcome; OD1 is resolved as dev-only build/run from source. |
| U3 | yes | Registration and Linux absence gates protect existing desktop behavior. |
| U4 | yes | Verification and Windows/Ubuntu smoke evidence are required before final release. |

Grouping rationale:
- RDM-002 is one cohesive package because DTOs, launcher behavior, command registration, and smoke evidence are tightly coupled by the handshake boundary.
- Review units split the high-risk surfaces: protocol, launcher/process, and registration/verification.
- Runtime implementation uses the resolved OD1 dev-only source-run model and remains queued for final batched release.

## Implementation Units

- U1 - Protocol DTOs and Agent Binary Skeleton.
- U2 - Windows-Only Launcher Seam.
- U3 - Host Registration Boundary and Linux Absence Tests.
- U4 - Verification and Manual Windows/Ubuntu Smoke Checklist.

## Review Unit Progress

| Review unit | Status | Notes |
|---|---|---|
| RU1 | review-passed | Protocol DTOs and Linux-side agent skeleton implemented and verified. |
| RU2 | review-passed | Windows-gated launcher seam implemented with mocked transport tests and no shell interpolation. |
| RU3 | review-passed | No public WSL command/UI registered; Linux absence and local guard regressions verified. |

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Protocol DTOs, protocol version constants, safe health/error categories, and Linux-side agent handshake skeleton | `src-tauri/Cargo.toml`, `src-tauri/src/wsl_agent/*`, `src-tauri/src/bin/tinto-agent.rs`, focused Rust tests | `develop` | optional Tarea | Medium; new internal protocol and binary target, mocked tests only. |
| RU2 | Windows-only launcher seam, no-shell command construction, startup timeout, mocked process IO, and child cleanup | `src-tauri/src/wsl_agent/launcher.rs`, `src-tauri/src/wsl_agent/*`, focused Rust tests | RU1 integrated | optional Tarea | High; process boundary and OD1-dependent command shape. |
| RU3 | Windows-gated host command registration if needed, Linux absence tests, local behavior regression checks, and Windows/Ubuntu smoke checklist | `src-tauri/src/lib.rs`, `src/workbench/wslAbsence.test.ts`, possibly `docs/contracts/bus-contract.md`, state/package docs | RU2 integrated | optional Tarea | Medium/high; must prove non-Windows absence and avoid public surface drift. |

## Files and Tests

Expected files:
- `src-tauri/Cargo.toml`
- `src-tauri/src/wsl_agent/mod.rs`
- `src-tauri/src/wsl_agent/protocol.rs`
- `src-tauri/src/wsl_agent/launcher.rs`
- `src-tauri/src/bin/tinto-agent.rs`
- `src-tauri/src/lib.rs`
- `src/workbench/wslAbsence.test.ts`
- optional `docs/contracts/bus-contract.md` only if a public Tauri command is introduced
- `docs/orchestration/compound-master-state.md`

Expected tests:
- DTO serialization/deserialization round trip.
- Compatible handshake accepted.
- Incompatible protocol rejected.
- Malformed, oversized, timed-out, and prematurely closed responses map to safe categories.
- Launcher command construction uses `wsl.exe`, `-d`, selected distro, `--`, and an injected agent command without shell interpolation.
- Missing WSL/distro/agent/spawn failure paths map to distinct safe categories through mocked launch errors.
- Non-Windows builds do not register WSL launch commands or frontend wrappers.
- Local repo and local agent-console behavior remains unchanged.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal Rust helper/DTO surface only.
- Public Tauri command contract: unchanged; no WSL invoke command was registered.
- Frontend bindings/UI/settings/empty states: unchanged; no WSL-facing UI was added.
- Internal changed surfaces: `wsl_agent::protocol`, `wsl_agent::launcher`, `tinto-agent` binary, and `bus::resolve_repo_for_command` unsupported-source guard behavior.
- Consumer scan pattern used: `rg "wsl_agent|tinto_agent|tinto-agent|invoke_handler|RepoSource|UnsupportedRepoSource|agent_console|CommandError|unsupported_entry_matches_request|normalize_path_lexically" src-tauri/src src docs/contracts`.
- Consumers found: `src-tauri/src/lib.rs`, `src-tauri/src/bin/tinto-agent.rs`, `src-tauri/src/wsl_agent/*`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/agent_console/commands.rs`, `src/workbench/wslAbsence.test.ts`, and existing workbench `RepoSource` tests.
- Contract-drift tests searched: Rust protocol/launcher tests, invoke registration absence tests, frontend absence scan, bus resolver tests, and agent-console compatibility tests.
- Consumer tests run: targeted Rust and frontend tests listed in the Verification Gate.

## Verification Gate

- `cargo test --lib wsl_agent`: 9 passed.
- `cargo build --bin tinto-agent`: passed.
- Local binary smoke with compatible handshake: returned `{"type":"handshake","protocol_version":1,"agent_version":"0.1.0","status":"ok"}`.
- Local binary smoke with incompatible protocol: returned safe `protocol_mismatch`.
- `cargo test --lib unsupported -- --test-threads=1`: 8 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `cargo test --lib agent_console`: 36 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npx tsc --noEmit`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.
- `rustfmt --edition 2021 --check src\wsl_agent\mod.rs src\wsl_agent\protocol.rs src\wsl_agent\launcher.rs src\bin\tinto-agent.rs`: passed.
- `rustfmt --edition 2021 --check --config skip_children=true src\bus\mod.rs`: passed.
- Work package checker: passed with the accepted warning that the package mixes orchestration docs and runtime files, justified by RU1/RU2/RU3 split.
- Manual Windows/Ubuntu WSL smoke checklist recorded before final release, with smoke execution deferred to the final batch.

Surface-aware evidence:
- Protocol DTOs: serialization, compatibility, malformed, wrong-type, and oversized-message tests.
- Launcher command: mocked command construction tests for `wsl.exe -d Ubuntu -- cargo run ...`.
- Safe health categories: mocked launcher/protocol error tests and local incompatible-protocol binary smoke.
- Linux absence: invoke-handler registration and frontend absence tests.
- Local compatibility: targeted existing bus and agent-console tests.
- Unsupported-source guard: navigation-alias regression test prevents unsupported WSL entries from canonicalizing into local repo command paths.

Production posture evidence:
- Prototype posture permits mocked non-Windows verification, but the final release handoff must record manual Windows/Ubuntu WSL evidence or the exact manual gap.
- Manual checklist is recorded at `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`.
- Global `cargo fmt --check` is not currently a clean signal because of pre-existing unrelated formatting drift in `src-tauri/src/bus/secret_scan.rs`; touched Rust files were checked directly, and `bus/mod.rs` was checked with `skip_children=true` to avoid traversing that unrelated child module.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Artifact review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-002-work-package-review.md`. OD1 was resolved after artifact review with dev-only build/run from source, so execution may proceed without changing package scope.
- Code review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-002-code-review.md`.
- Fixes made during code review: Windows-gated the real `StdCommandTransport` implementation and fixed a bus resolver navigation-alias bypass for unsupported WSL entries.

## Security Gate

- Run after work-review loop: required, because this package introduces a host process launch boundary and child process IO.
- Security Watch during work: enabled.
- Security Watch notes: verify no shell interpolation, bounded IO, safe error categories, child cleanup, no env/secret leakage, no Linux desktop WSL surface, and no fallback to Windows filesystem access for WSL repos.
- Security reviewer: `krt-security-sentinel` preferred; fallback direct security review if unavailable.
- Security review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-002-security-review.md`.
- Required security verification: command construction uses an argument vector with no shell interpolation; protocol messages are bounded to 64 KiB; timeout kills and waits for the child; stderr is not surfaced; error reporting uses safe categories; no public WSL invoke command or frontend UI was added.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust compile, new binary target, command registration tests, async timeout tests, frontend absence scan if touched.
- Preventive evidence: targeted Rust tests, binary build, touched-file `rustfmt --check`, frontend absence test, `npx tsc --noEmit`, `git diff --check`, and manual smoke gap recorded.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: release follow-up is blocked until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1/RU2/RU3 batched only after all active work packages complete, unless the user changes release timing.
- Branch name: `feat/wsl-agent-bootstrap`
- Branch/docs rule: related planning artifacts ship with the final batched release; no intermediate release for this package.
- PR base: `develop` if the user requests PR flow; otherwise final local no-PR release targets `develop` over `origin/develop`.
- Suggested commit grouping for this review unit:
  - `feat(wsl): add agent handshake protocol` - protocol DTOs, agent skeleton, tests.
  - `feat(wsl): add Windows launcher seam` - launcher command construction, mocked IO, safe errors, tests.
  - `test(wsl): preserve non-Windows absence` - command registration/source-scan/local compatibility tests.
  - `docs(orchestration): add WSL agent bootstrap artifacts [skip ci]` - requirements, plan, package, findings, and state.
- PR title: `Add Windows WSL agent bootstrap`
- PR body bullets:
  - Add the Windows-only WSL agent handshake foundation.
  - Launch the selected Ubuntu distro through a bounded no-shell process seam.
  - Preserve local repo behavior and keep WSL surfaces absent outside Windows.
- Verification results location: this package and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: manual Windows/Ubuntu WSL smoke evidence or explicit gap required before final release.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea unless later WSL packages are grouped under a parent.
- Jira summary: `Agregar arranque del agente WSL en Windows`
- Jira description: `Crear la base de arranque y protocolo minimo del agente Linux para repos WSL en Windows, manteniendo Ubuntu como objetivo inicial y sin exponer superficies WSL en Linux.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
