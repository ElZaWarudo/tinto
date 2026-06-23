---
title: WSL packaging, recovery, and verification
status: review-passed
roadmap_item: RDM-006
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-006-wsl-packaging-recovery-verification.md
origin_planning_input: docs/brainstorms/2026-06-23-006-wsl-packaging-recovery-verification.md
origin_plan: docs/plans/2026-06-23-006-wsl-packaging-recovery-verification-plan.md
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

# WSL Packaging, Recovery, And Verification

## Scope

Replace the implicit dev-source-only WSL agent launch with a packaged-first Linux agent bootstrap path, while preserving an explicit development fallback.

## Non-goals

- No full auto-updater.
- No multi-distro support beyond Ubuntu.
- No WSL Gitleaks, media preview, Agent Console, or fine-grained `fs-events`.
- No release, push, PR, merge, or Jira mutation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide internal helper names, test fixtures, and app-relative packaged lookup paths.
- Agent must record as assumptions any verification that requires a real Windows/Ubuntu host.
- Agent must escalate a different install/update model, external mutation, or release timing change.
- Safe fallback: keep the current dev-source flow behind an explicit dev fallback.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-002, RDM-004, RDM-005.
- Blocks: final WSL release handoff.

## Production Posture

- Posture: prototype.
- Evidence: active orchestration state records prototype posture and local desktop delivery.
- Confidence: high.
- Consequences for this package: preserve compatibility and produce clear diagnostics, but accept manual Windows smoke as the final environment proof.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Packaged-first discovery/install is the core hardening. |
| U2 | yes | Existing WSL calls must use the new launcher. |
| U3 | yes | Diagnostics and tests are part of release hardening. |
| U4 | yes | Docs/state/smoke evidence keep release handoff honest. |

Grouping rationale:
- The units share the same launcher boundary and are easier to review as one integrated hardening slice.

## Implementation Units

- U1: Add packaged agent discovery/install helpers.
- U2: Switch WSL bus/file operation requests to the packaged-first request helper.
- U3: Extend launcher tests.
- U4: Update docs, smoke checklist, review findings, and state.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Packaged-first WSL agent bootstrap and recovery docs | `src-tauri/src/wsl_agent/launcher.rs`, WSL request call sites, docs/tests | `develop` with queued WSL work | optional Tarea | Medium risk; packaging/process boundary, no external mutation. |

## Files and Tests

- Runtime/CI: `src-tauri/src/wsl_agent/launcher.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/bus/mod.rs`, `.github/workflows/ci.yml`.
- Docs: `docs/contracts/bus-contract.md`, `docs/manual-smoke/`, this work package, review findings, state.
- Tests: `cargo test --lib wsl_agent`, `cargo test --lib bus -- --test-threads=1`, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, work package checker, `git diff --check`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal launcher helper behavior and WSL agent availability contract docs.
- Consumer scan patterns: `rg "request_ubuntu_dev_source|request_ubuntu_agent|tinto-agent|TINTO_WSL_AGENT" src-tauri docs`.
- Consumers found: WSL bus recalc, WSL read command routing, WSL file operation routing, contract docs, manual smoke docs.
- Contract-drift tests searched: launcher/unit tests and WSL absence tests.
- Required consumer tests: WSL agent tests and bus tests.
- Consumer tests run/skipped: Rust WSL agent tests, bus tests, tinto-agent build, TypeScript check, work package checker, and `git diff --check` passed. Real Windows/Ubuntu packaged-agent smoke remains pending for final release.

## Verification Gate

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `cargo build --bin tinto-agent`
- `npx tsc --noEmit`
- `python C:\Users\Mayor\.agents\skills\krt-compound-master\scripts\check_work_package.py docs\work-packages\RDM-006-wsl-packaging-recovery-verification\2026-06-23-006-wsl-packaging-recovery-verification-work-package.md`
- `git diff --check`

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate

- Run after work-review loop: required because process launch and binary installation paths change.
- Security Watch during work: enabled.
- Security Watch notes: avoid shell interpolation for request execution; constrain WSL install destination; keep errors safe.
- Security reviewer: inline fallback if dedicated security tool is not invoked.
- Security review result: passed by inline fallback on 2026-06-23.
- Required security verification: launcher tests and manual inspection of command construction.

## Implementation Summary

- Added packaged-first WSL agent discovery with `TINTO_WSL_AGENT_LINUX_BIN` plus app-relative candidate paths.
- Added a versioned WSL install path at `$HOME/.local/share/tinto/agents/<version>/tinto-agent`, streamed from the host-side Linux artifact over stdin.
- Switched WSL request call sites from dev-source-only launch to packaged-first `request_ubuntu_agent`.
- Kept source checkout launch only behind explicit `TINTO_WSL_AGENT_ALLOW_DEV_SOURCE=1` for development.
- Added a CI artifact step that builds `tinto-agent` on Ubuntu and uploads it as `tinto-agent-linux-x86_64`.
- Updated the Windows/Ubuntu smoke checklist to validate packaged install, handshake, protocol mismatch, local+WSL repo coexistence, and WSL file operations.

## Review Result

- Status: `review-passed`.
- Findings path: `docs/review-findings/2026-06-23-rdm-006-code-security-review.md`.
- No P0-P2 findings remain.
- Honest note: final release still needs installer/resource wiring to consume the CI `tinto-agent-linux-x86_64` artifact and a Windows/Ubuntu packaged-agent smoke run.

## Verification Results

- `cargo test --lib wsl_agent`: passed, 21 tests.
- `cargo test --lib bus -- --test-threads=1`: passed, 42 tests.
- `cargo build --bin tinto-agent`: passed.
- `npx tsc --noEmit`: passed.
- Work package checker: passed.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust compile/tests, Windows-only launcher cfg, docs.
- Preventive evidence: local Rust tests/build and docs checker; real Windows/Ubuntu smoke remains manual.
- If CI breaks: use direct evidence-first triage; no CI polling in Compound Master.
- Escalation rule: final release remains blocked until manual smoke is completed or explicitly waived.

## Branch and PR Handoff Inputs

- Review unit: RU1 Packaged-first WSL agent bootstrap.
- Branch name: `feat/wsl-agent-packaged-bootstrap`
- Branch/docs rule: keep related planning docs with the implementation branch for the final batch.
- PR base: `develop`.
- Suggested commit grouping for this review unit:
  - `feat(wsl): prefer packaged agent bootstrap` - launcher/request call sites/tests - replaces dev-only launch with a serious runtime path.
  - `docs(wsl): add packaging recovery smoke evidence` - contract/manual smoke/work package/state - records operator expectations and final release gates.
- PR title: `Prefer packaged WSL agent bootstrap`
- PR body bullets:
  - Discovers and installs a packaged Linux `tinto-agent` into Ubuntu WSL before request execution.
  - Keeps dev-source launch as an explicit development fallback.
  - Adds safe diagnostics and tests for missing agent artifacts, install command shape, and no-shell argv handling.
- Verification results location: this work package and review findings.
- Production/deployment notes: final release requires a Linux `tinto-agent` artifact in the packaged resource location and Windows/Ubuntu manual smoke.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone `Tarea` unless a real multi-child parent exists.
- Jira summary: `Preparar arranque empaquetado del agente WSL`
- Jira description: `Endurecer el arranque del agente Ubuntu WSL para que Tinto use un binario Linux empaquetado e instalado en la distro, manteniendo el modo desde código solo como fallback de desarrollo.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.
