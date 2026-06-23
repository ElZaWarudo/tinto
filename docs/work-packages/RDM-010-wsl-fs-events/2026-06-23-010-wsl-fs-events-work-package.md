---
title: WSL filesystem events
status: review-passed
roadmap_item: RDM-010
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-010-wsl-fs-events.md
origin_planning_input: docs/brainstorms/2026-06-23-010-wsl-fs-events.md
origin_plan: docs/plans/2026-06-23-010-wsl-fs-events-plan.md
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

# WSL filesystem events

## Scope
Emit existing `tinto://fs-events` batches for Ubuntu WSL repos by comparing agent-side file fingerprints during the current WSL polling cycle.

## Non-goals
Long-lived WSL inotify streaming, multi-distro support, UI redesign, and release packaging are excluded. Releases remain deferred to the final batch.

## Autonomy Contract
- Mode: guarded
- Agent may decide without asking: fingerprint field names, deterministic sort order, event cap, and equivalent targeted test commands.
- Agent must record as assumptions: polling remains the transport boundary, initial snapshot primes without events, and any real WSL smoke skipped due environment.
- Agent must escalate: public event contract removal, destructive filesystem operations, auth/tenant/data contract changes, branch/base/release changes, or scope outside WSL event parity.
- Safe fallback: keep existing WSL polling deltas and record any blocker with exact failing test or environment.
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies
- Requires: RDM-004 WSL snapshot/read routing and RDM-006 packaged-first agent launcher.
- Blocks: final Windows/Ubuntu mixed-workbench manual smoke and release batch.

## Production Posture
- Posture: prototype
- Evidence: current Compound Master state and active local desktop development posture.
- Confidence: high
- Consequences for this package: preserve existing local event contract and fail closed for WSL agent errors.
- Breaking existing behavior allowed: no

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Protocol DTO is required to move fingerprints across the one-shot agent boundary. |
| U2 | yes | Agent-side scan owns Linux path semantics and `.git` exclusion. |
| U3 | yes | Bus diffing and event emission produce the public contract behavior. |
| U4 | yes | Message guard/docs must match bounded WSL payloads. |
| U5 | yes | Verification and review are required before release queueing. |

Grouping rationale:
- A single RU is justified because protocol, runtime scan, bus state, docs, and tests must land together for a meaningful event contract.

## Implementation Units
- U1: add `FileFingerprint`, `FsEventSnapshot` request, and `FsEventSnapshot` response to the WSL protocol.
- U2: implement Linux fingerprint walking in `wsl_agent::runtime`.
- U3: diff WSL fingerprints in `RepoLiveState` and emit `FsEventBatch` from WSL poll results.
- U4: update message guard and contract docs.
- U5: verify and review.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL fs-events | `src-tauri/src/wsl_agent/*`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/contract.rs` if needed, docs/tests | develop | Optional standalone Tarea | Medium risk remote filesystem enumeration; no file contents read. |

## Files and Tests
- Expected files: `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src-tauri/src/bus/mod.rs`, `docs/contracts/bus-contract.md`, orchestration docs/review findings.
- Expected tests: `cargo test --lib wsl_agent`, `cargo test --lib bus -- --test-threads=1`, `npx tsc --noEmit`, `git diff --check`.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal WSL agent protocol; existing public `FsEventBatch` unchanged.
- Consumer scan patterns: `FsEventSnapshot|FileFingerprint|EVENT_FS_EVENTS|wsl_poll|last_known_sizes|fsEventsByRepo`.
- Consumers found: `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src-tauri/src/bus/mod.rs`, `docs/contracts/bus-contract.md`, and `src/bus/store.ts` as the unchanged frontend consumer of public `FsEventBatch`.
- Contract-drift tests searched: protocol serialization tests and bus fs-event tests.
- Required consumer tests: WSL agent runtime test and bus event diff test.
- Consumer tests run/skipped: `cargo test --lib wsl_agent` passed 25/25; `cargo test --lib bus -- --test-threads=1` passed 43/43; `cargo build --bin tinto-agent` passed; `npx tsc --noEmit` passed; `git diff --check` passed with CRLF warnings only.

## Verification Gate
- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npx tsc --noEmit`
- `git diff --check`
- Surface-aware evidence: protocol roundtrip, agent scan, bus event emission, public contract docs.
- Production posture evidence: additive internal agent protocol, public event shape unchanged.

## Review Gate
- Code review threshold: P0-P2
- Findings below threshold: log unless user marks blocking

## Security Gate
- Run after work-review loop: required because the agent enumerates filesystem paths in WSL repos.
- Security Watch during work: enabled for allowlist, `.git` exclusion, symlink behavior, payload bounds, and no content reads.
- Security Watch notes: agent-side fingerprint scan is active-workbench scoped by the host request, does not read file contents, follows no symlinks, excludes `.git`, respects ignore walking, caps entries at the repo-tree guard, and returns only relative path/size/mtime metadata.
- Security reviewer: inline fallback if canonical reviewer unavailable.
- Security review result: passed.
- Required security verification: tests for allowlist rejection and `.git`/navigation exclusion.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust compile/tests, protocol compatibility, bus timing tests.
- Preventive evidence: local targeted tests and diff hygiene before release handoff.
- If CI breaks: invoke `krt-ci-questor` with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 WSL fs-events
- Branch name: feat/wsl-file-events
- Branch/docs rule: keep related docs with implementation.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `feat(wsl): emit filesystem events from WSL repos` - protocol, runtime scan, bus diffing, tests, and contract docs.
  - `docs(orchestration): add WSL fs-events delivery artifacts [skip ci]` - brainstorm, plan, package, review findings, and state updates.
- PR title: Emit filesystem events from WSL repos
- PR body bullets:
  - Add agent-side file fingerprint snapshots for WSL repos.
  - Emit existing `tinto://fs-events` batches for WSL created/modified/removed files.
  - Keep local watcher behavior and public event shape unchanged.
- Verification results location: this work package and `docs/review-findings/2026-06-23-rdm-010-code-security-review.md`
- Production/deployment notes: final release should smoke a WSL repo file create/modify/delete and confirm event-driven activity.
- Autonomous mutation request: none

## Jira Handoff Inputs
- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone Tarea unless a broader WSL complement parent already exists.
- Jira summary: Emitir eventos de archivos para repositorios WSL
- Jira description: Añadir eventos de archivos para repositorios WSL comparando fingerprints del agente Linux y emitiendo el contrato existente `tinto://fs-events`, sin cambiar el watcher local.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.

## Implementation Results
- Status: review-passed locally; queued for final release batch.
- Protocol/runtime: added `RepoSnapshotWithFsEvents`, `FileFingerprint`, and `RepoFileFingerprintSnapshot`; raised `tinto-agent` message guard to 20 MiB for bounded media/fingerprint payloads.
- Agent scan: Linux-side fingerprint walker returns relative file paths, size, and mtime, skipping `.git`, not following symlinks, and capping entries.
- Bus: WSL polling now requests deltas plus fingerprints in one agent execution, primes without initial flood, emits existing `EVENT_FS_EVENTS` batches for created/modified/removed paths, and then applies the normal `RepoDelta`.
- Public contract: unchanged event shape; docs clarify that WSL events are agent-side fingerprint batches during the polling cycle, not long-lived inotify streaming.
- Verification: `cargo test --lib wsl_agent` 25/25, `cargo test --lib bus -- --test-threads=1` 43/43, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, and `git diff --check` passed.
