---
title: Windows WSL agent bootstrap and minimal stdio protocol plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-002
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_requirements: docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md
planning_status: planned
delivery_approach: hybrid
---

# Windows WSL agent bootstrap and minimal stdio protocol plan

## Planning Source

- Requirements packet: `docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md`.
- Requirements review: `docs/review-findings/2026-06-23-rdm-002-requirements-review.md`.
- Roadmap item: `docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md`.
- Prior dependency: `docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md`.

## Scope Summary

Build the smallest Windows-only host/agent bootstrap seam for future WSL repo operations. The first supported baseline is WSL 2 only. The first-release distro model is one selected distro per WSL repo, with Ubuntu as the initial manual smoke target.

This plan does not add WSL repo picker UI, read/watch routing, file operations, Gitleaks routing, media preview routing, or agent-console routing. Linux desktop builds must keep the WSL complement absent.

## Delivery Approach

Hybrid plan:

- Phase 1 defines internal protocol and agent shape with tests that can run in this workspace.
- Phase 2 defines the Windows-only launcher seam and health mapping using mocked process launch.
- Phase 3 wires only the gated host command/registration boundary needed for handshake diagnostics.
- Phase 4 records Windows/Ubuntu WSL manual smoke steps and CI break-prevention evidence.

The approach is hybrid because RDM-002 has a hard process-boundary milestone, but implementation must remain incremental and testable without a real Windows/WSL host.

## Confirmed Decisions

- WSL support baseline: WSL 2 only.
- First-release distro scope: one selected distro per WSL repo.
- Initial manual smoke distro: Ubuntu.
- `tinto-agent` availability model: dev-only build/run from source inside Ubuntu.
- Release timing: defer releases until the end of the active Compound Master run.

## Resolved Availability Model

OD1 was resolved on 2026-06-23: the first implementation uses dev-only build/run from source. The launcher may use an injected command fixture in tests and the manual smoke checklist should validate a dev command inside Ubuntu, such as running the repository's `tinto-agent` binary target from source.

## Plan Units

### U1 - Protocol DTOs and Agent Binary Skeleton

Outcome:
- Add an internal host/agent protocol module with handshake DTOs, protocol version constants, health status, and safe error categories.
- Add the smallest Linux-side `tinto-agent` binary target or crate shape needed to respond to handshake input.

Expected surfaces:
- `src-tauri/Cargo.toml`
- `src-tauri/src/wsl_agent/` or equivalent host module
- `src-tauri/src/bin/tinto-agent.rs` or equivalent agent binary entrypoint
- Rust unit tests for serialization, version compatibility, malformed input, and oversize input.

Acceptance mapping:
- FR5, FR6, FR7, FR8
- AC3, AC4
- NFR2, NFR3, NFR4

### U2 - Windows-Only Launcher Seam

Outcome:
- Add Windows-gated launcher construction for `wsl.exe -d Ubuntu -- <dev-source agent command>` without shell interpolation.
- Keep the selected distro parameterized for later RDM-003 persisted repo source data, while the first smoke target remains Ubuntu.
- Model missing WSL, missing distro, missing agent, spawn failure, timeout, malformed response, protocol mismatch, and child exit as safe categories.

Expected surfaces:
- `src-tauri/src/wsl_agent/launcher.rs` or equivalent
- mocked process-launch abstraction/tests
- no public frontend wrappers yet unless needed only for a gated diagnostic command

Acceptance mapping:
- FR1, FR2, FR3, FR4, FR8, FR9
- AC2, AC5
- BR1, BR2, BR4, BR6, BR7

Implementation detail:
- The concrete command is dev-only build/run from source. Tests should keep it injected as an argument vector so later packaging work can swap the availability model without rewriting the launcher seam.

### U3 - Host Registration Boundary and Linux Absence Tests

Outcome:
- Preserve Linux absence: non-Windows builds must not expose WSL launch commands, frontend wrappers, UI, settings, empty states, degraded notices, or runtime behavior.
- Add only Windows-gated registration for any host diagnostic/handshake command selected by implementation.
- Preserve existing local repo and local agent-console behavior.

Expected surfaces:
- `src-tauri/src/lib.rs`
- `src/workbench/wslAbsence.test.ts`
- possibly `src/bus/contract.test.ts` if command surface documentation changes
- docs/contracts only if a public command is introduced

Acceptance mapping:
- FR10, FR11
- AC1, AC6, AC8
- NFR1, NFR5

### U4 - Verification and Manual Windows/Ubuntu Smoke Checklist

Outcome:
- Record local mocked verification commands.
- Record manual Windows/Ubuntu WSL smoke steps for real `wsl.exe` launch and handshake validation.
- Record CI gaps and release evidence expectations for the final batched release.

Expected surfaces:
- work package verification section
- state update
- optional docs under `docs/contracts/` only if the public command contract changes

Acceptance mapping:
- FR12
- AC7
- BR5

## Sequencing

1. Implement U1 first because protocol DTOs and agent skeleton are independent of process launch.
2. Implement U2 using mocked launcher tests before any real Windows smoke.
3. Implement U3 with U2 or immediately after it so registration and absence boundaries are reviewed with the launcher.
4. Complete U4 before final release handoff.

## Review Unit Strategy

- RU1: Protocol DTOs and agent skeleton.
- RU2: Windows-only launcher seam and safe health categories.
- RU3: Registration/absence boundary plus manual Windows/Ubuntu smoke checklist.

RU1 can be reviewed independently. RU2 depends on RU1. RU3 depends on the final command registration shape from RU2.

## Verification Gate

- `cargo test --lib wsl_agent`
- targeted `cargo test --lib` tests for command registration/absence
- targeted frontend absence test if WSL-facing source scan changes
- `cargo fmt --check`
- `npx tsc --noEmit` only if frontend/contract TypeScript changes
- manual Windows/Ubuntu WSL smoke checklist before final release handoff, or explicit CI-only/manual gap recorded if unavailable

## Risks and Mitigations

- Risk: real Windows/WSL behavior cannot be proven from this workspace.
  - Mitigation: keep process launch behind an abstraction, cover command construction and IO behavior with mocked tests, and record manual Ubuntu smoke steps.
- Risk: WSL feature leaks into Linux desktop builds.
  - Mitigation: compile/runtime gates and absence tests must stay in the verification gate.
- Risk: the dev-only availability model is not a production install/update story.
  - Mitigation: keep packaging and updater behavior out of RDM-002; RDM-006 owns packaging hardening.
- Risk: stdio protocol grows into a framework too early.
  - Mitigation: keep protocol to handshake/health only in RDM-002; read/watch belongs to RDM-004.

## Planning Review Result

Status: plan-review-passed via Compound Master fallback review.

Review notes:
- The plan traces to the reviewed requirements and roadmap.
- U1/U2/U3 preserve focused review units and dependency order.
- The plan uses the user-selected dev-only build/run-from-source availability model and defers packaging to RDM-006.
- Linux absence and local behavior compatibility are included in verification.

## Next Step

Create and execute the RDM-002 work package in RU1/RU2/RU3 order. Release remains deferred until the end of the active Compound Master run.
