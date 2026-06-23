---
title: Windows WSL agent bootstrap and minimal stdio protocol requirements
status: reviewed
date: 2026-06-23
roadmap_item: RDM-002
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
artifact_kind: requirements-brainstorm
planning_input: true
source_docs:
  - docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
  - docs/review-findings/2026-06-23-wsl-complement-roadmap-review.md
  - docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md
  - docs/contracts/bus-contract.md
  - src-tauri/src/lib.rs
  - src-tauri/src/agent_console/commands.rs
  - src-tauri/src/agent_console/pty.rs
  - src-tauri/src/agent_console/validation.rs
  - src-tauri/src/workbench/mod.rs
---

# Windows WSL agent bootstrap and minimal stdio protocol requirements

## Problem and Goal

Tinto now has the RDM-001 foundation for distinguishing local repos from future Windows WSL repos without exposing WSL behavior on Linux. The next problem is that the Windows app cannot safely treat a WSL repo as a Windows filesystem path. Git, watch, path, and process operations for a WSL repo should run inside the selected Linux distro.

Goal: define the smallest Windows-only `tinto-agent` bootstrap and bounded stdio protocol that lets the Windows host start a Linux-side child process inside WSL, verify compatibility, exchange simple request/response messages, and report safe health errors without adding WSL repo browsing, full read/watch parity, auto-install/update, or Linux desktop WSL surfaces.

## Vocabulary

- Windows host launcher: Windows-only Rust code that invokes `wsl.exe` with a selected distro and a Linux-side agent command.
- Linux-side agent: a `tinto-agent` process running inside WSL, owned by the Windows Tinto app process lifetime.
- Stdio protocol: a bounded request/response channel over the child process stdin/stdout, using line-delimited JSON or an equivalently simple framing selected in the plan.
- Handshake: the first protocol exchange that proves the agent version, protocol version, and basic health.
- Health error: a safe structured error category for missing WSL, missing distro, missing agent binary, protocol mismatch, spawn failure, timeout, malformed response, and child exit.

## Stakeholders and Users

- Primary user: a Windows Tinto user who wants Tinto to operate on repos stored inside WSL.
- Existing users: Windows and Linux Tinto users monitoring local repos; their current local behavior must not regress.
- Maintainer/reviewer: implementers reviewing a high-risk process boundary and future protocol surface.

## Scope In

- Define the first `tinto-agent` binary target or crate shape needed for a Linux-side child process.
- Define shared host/agent DTOs for the first protocol messages.
- Add a Windows-only host launcher that builds the `wsl.exe -d <distro> -- <agent command>` invocation under compile/runtime gates.
- Use WSL 2 only for the first supported baseline.
- Use one selected distro per WSL repo for the first release; the initial manual smoke target is Ubuntu.
- Add a bounded startup/handshake flow with protocol version, agent version, and safe health categories.
- Add process lifetime cleanup for the spawned WSL agent child.
- Add mocked launcher/protocol tests that can run in this workspace without a real Windows host or WSL distro.
- Keep Linux desktop builds free of WSL command registration, frontend wrappers, UI, settings, empty states, degraded notices, and runtime behavior.
- Record a manual Windows/WSL smoke checklist for verification that cannot run in this workspace.

## Scope Out

- No WSL repo picker, distro selector UI, or Linux path entry UI.
- No WSL browse/list flow.
- No full JSON-RPC framework.
- No broad capability negotiation beyond the minimal version/health handshake.
- No auto-install/update model for `tinto-agent`; the first implementation uses a dev-only build/run-from-source command inside Ubuntu.
- No core WSL read/watch data path.
- No WSL routing for media preview, secret findings, Gitleaks, file operations, or agent console sessions.
- No Linux desktop WSL feature surface.
- No SSH, cloud, container, or arbitrary remote host support.

## Constraints

- The complement is Windows-only. Linux desktop builds must behave as though the WSL complement does not exist.
- First-release support is scoped to WSL 2 only.
- First-release distro scope is one selected distro per WSL repo, with Ubuntu as the initial validation target.
- First implementation availability model is dev-only build/run from source inside Ubuntu.
- RDM-002 depends on RDM-001 and must reuse the source-aware repo boundary instead of adding parallel repo identity rules.
- The current workspace can cover unit tests, mocked launch behavior, and compile gates, but cannot prove real Windows/WSL execution.
- Existing `agent_console` PTY behavior is a separate local-agent feature and must not be confused with the WSL `tinto-agent` protocol.
- Safe command errors must not leak secrets, host env details, or sensitive path content beyond what is needed to identify the selected distro and high-level failure category.
- Jira remains optional and currently degraded as `jira-env-not-configured`.

## Functional Requirements

- FR1: On Windows only, the system shall define a launcher path for starting a Linux-side `tinto-agent` inside a selected WSL distro.
- FR2: The launcher shall construct the WSL command without shell interpolation.
- FR3: The launcher shall support a selected distro name and a Linux-side agent executable path or command selected by the later install/dev model.
- FR4: The launcher shall enforce startup timeout and return a safe structured error if the child does not complete the handshake in time.
- FR5: The agent shall respond to a minimal handshake request with protocol version, agent version, and health status.
- FR6: The host shall reject incompatible protocol versions before later repo operations can use the agent.
- FR7: The stdio protocol shall use bounded message sizes and malformed-message handling.
- FR8: The host shall distinguish missing WSL, missing distro, missing agent, spawn failure, timeout, protocol mismatch, malformed response, and unexpected child exit as safe categories.
- FR9: The host shall clean up the child process when the owning agent session/connection is stopped or when the app shuts down.
- FR10: Linux desktop builds shall not register WSL launch commands, export frontend wrappers, render WSL UI, or emit WSL empty/degraded state.
- FR11: Tests shall cover command construction and protocol/health behavior through mocked process launch and mocked IO.
- FR12: The artifact set shall include a manual Windows/WSL smoke checklist for real `wsl.exe` launch and handshake validation.

## Non-Functional Requirements

- NFR1: Compatibility: local repo behavior and local agent-console PTY behavior must remain unchanged.
- NFR2: Security: launcher code must avoid shell expansion and must keep environment/path details out of user-facing errors unless explicitly safe.
- NFR3: Reliability: startup and protocol waits must be bounded so the UI cannot hang on a missing or unhealthy WSL distro.
- NFR4: Reviewability: protocol DTOs, process launch, and tests should be separable from future read/watch behavior.
- NFR5: Portability: non-Windows builds must compile without WSL launch dependencies or inert WSL surfaces.

## Business Rules

- BR1: WSL agent launch is meaningful only from a Windows host.
- BR2: A WSL repo operation must not fall back to Windows filesystem access when agent bootstrap fails.
- BR3: The Linux desktop app must not show WSL as unavailable; it must not show WSL at all.
- BR4: A protocol mismatch is a hard failure for WSL repo operations until the agent and host are compatible.
- BR5: First implementation may be mocked outside Windows, but release handoff must record the Windows/WSL manual smoke gap or evidence.
- BR6: The first release supports one selected distro per WSL repo rather than cross-distro fan-out.
- BR7: Ubuntu is the expected manual smoke distro for the first Windows/WSL validation pass.

## Acceptance Criteria

- AC1: Non-Windows command registration tests prove no WSL launch command or `tinto-agent` invoke surface is registered.
- AC2: Unit tests prove the Windows launcher command shape uses `wsl.exe`, `-d`, the selected distro, `--`, and the agent command without shell interpolation.
- AC3: Unit tests prove the handshake accepts a compatible version and rejects an incompatible version.
- AC4: Unit tests prove malformed, oversized, timed-out, and prematurely closed stdio responses map to safe structured error categories.
- AC5: Unit tests prove missing WSL/distro/agent/spawn failure paths are distinguishable through mocked launch errors.
- AC6: Unit tests or inspection prove local repo and local agent-console command behavior is unchanged.
- AC7: A manual smoke checklist describes how to validate real launch on Windows with WSL installed.
- AC8: No WSL repo picker, WSL read/watch path, WSL file mutation path, WSL Gitleaks/secret scan path, WSL media preview path, or WSL agent-console session routing is implemented by this slice.

## Assumptions

- RDM-002 can introduce internal DTOs and launcher/protocol modules without exposing a public frontend WSL workflow yet.
- The first useful protocol operation is handshake/health only; read/watch requests belong to RDM-004.
- The repo remains in prototype posture, but process-boundary errors still need safe categories and bounded waits.

## Open Questions

- OQ1: Resolved on 2026-06-23. First supported baseline is WSL 2 only.
- OQ2: Resolved on 2026-06-23. First release supports one selected distro per WSL repo.
- OQ3: Resolved on 2026-06-23. For the first RDM-002 implementation, `tinto-agent` is made available inside Ubuntu through dev-only build/run from source.
- OQ4: Resolved on 2026-06-23. The expected manual smoke distro is Ubuntu.

## Validation Status

Requirements status: reviewed and ready for planning.

Planning input path: `docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md`.

OQ1, OQ2, OQ3, and OQ4 are resolved by user decision on 2026-06-23: WSL 2 only, one selected distro per WSL repo, dev-only build/run from source for `tinto-agent`, and Ubuntu as the initial manual smoke distro. Requirements review findings are recorded in `docs/review-findings/2026-06-23-rdm-002-requirements-review.md`.
