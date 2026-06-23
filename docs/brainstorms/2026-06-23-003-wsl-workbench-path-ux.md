---
title: Windows-only WSL workbench source and path UX
status: reviewed
roadmap_item: RDM-003
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
date: 2026-06-23
source_docs:
  - docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
  - docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md
  - docs/work-packages/RDM-002-windows-wsl-agent-bootstrap-protocol/2026-06-23-002-wsl-agent-bootstrap-protocol-work-package.md
  - src-tauri/src/workbench/mod.rs
  - src-tauri/src/workbench/commands.rs
  - src/bus/contract.ts
  - src/bus/client.ts
  - src/workbench/operations.ts
  - src/workbench/MenuBar.tsx
  - src/workbench/wslAbsence.test.ts
---

# Windows-only WSL Workbench Source and Path UX

## Context

RDM-001 added a persisted `RepoSource::Wsl` shape and hides unsupported WSL entries from the runtime projection. RDM-002 added the Ubuntu/dev-source agent bootstrap foundation but no public WSL command or frontend surface. RDM-003 is the first user-facing Windows-only configuration slice: a Windows user can add a WSL repo by choosing the supported distro (`Ubuntu`) and typing a Linux repo path.

RDM-003 must not promise monitoring yet. RDM-004 owns the read/watch/event path through `tinto-agent`. Until RDM-004 lands, configured WSL repos may be visible as configured WSL entries in the workbench UI, but they must not be routed through local git/filesystem commands.

## Decisions Carried In

- WSL baseline: WSL 2 only.
- First distro scope: one selected distro per WSL repo.
- First distro: Ubuntu.
- Agent availability model: dev-only build/run from source for RDM-002; RDM-003 does not install or launch the agent.
- Product posture: Windows-only complement/add-on. Linux builds/runs must not expose WSL UI, settings, commands, empty states, degraded notices, or behavior.
- Release timing: release remains deferred until the end of the active Compound Master run.

## Problem

The current add-repo flow opens a native directory picker and calls `add_repo(workbench, path, alias)`, which canonicalizes a local filesystem path and validates it with local git. That is correct for local repos but wrong for WSL repos:

- A WSL repo identity should be the Linux path plus distro, not a Windows `\\wsl$` translation.
- Local git validation must not run on a Linux path from the Windows host.
- The frontend currently has no way to display configured WSL entries because TypeScript `RepoEntry` omits `source` and `distro`.
- Existing absence tests assert no WSL runtime strings anywhere in frontend source; RDM-003 needs a stronger rule: WSL strings may exist only behind Windows-only gates and must render nothing on Linux.

## Goals

- Add an explicit Windows-only WSL add path for the active workbench.
- Persist WSL repos as `source = "wsl"`, `distro = "Ubuntu"`, and a normalized Linux path.
- Keep local `add_repo` behavior unchanged.
- Extend the frontend workbench config types additively with `source` and `distro`.
- Show configured WSL entries on Windows with clear local-vs-WSL labels.
- Hide all WSL controls and WSL configured entries on Linux/non-Windows runtime.
- Preserve RDM-001 guard behavior: WSL repos are not mounted into local bus read/watch/file/session command paths before RDM-004.

## Non-goals

- No WSL browse/list flow.
- No automatic distro discovery.
- No multi-distro picker beyond the selected Ubuntu value.
- No agent launch, health probe, read/watch/event forwarding, file tree, diff, media preview, Gitleaks, file operations, or agent-console routing for WSL repos.
- No `\\wsl$` path translation as repo identity.
- No Linux desktop WSL UI, commands, settings, empty state, or degraded notice.
- No arbitrary remote host, SSH, container, cloud, or VS Code integration.

## Functional Requirements

- FR1: On Windows only, the Repos menu exposes an "Add WSL repo..." action near the existing local add/autodetect actions.
- FR2: The add WSL flow accepts a Linux absolute path and the selected distro. For this package the only selectable distro is Ubuntu.
- FR3: The add WSL flow trims whitespace, rejects an empty path, rejects non-absolute Linux paths, rejects paths with Windows drive or UNC syntax, and normalizes repeated/trailing slashes without changing meaningful Linux path segments.
- FR4: Adding a WSL repo persists a `RepoEntry` with `source = "wsl"`, `distro = "Ubuntu"`, `path = "<linux path>"`, optional alias, and empty `fs_watch`.
- FR5: Duplicate detection treats WSL entries as duplicate only when source, distro, and Linux path match. A local repo and a WSL repo may share the same textual path without colliding.
- FR6: Removing a configured WSL repo removes only the matching WSL entry and does not remove a local repo with the same textual path.
- FR7: Local repo add/remove/update/reorder behavior remains unchanged.
- FR8: The workbench config returned to the frontend includes additive `source?: "local" | "wsl"` and `distro?: string | null` fields.
- FR9: On Windows, configured WSL entries can be shown in workbench/project configuration surfaces with a clear label such as `Ubuntu:/home/user/repo` or the alias plus a WSL source badge.
- FR10: Before RDM-004, configured WSL entries must not create live bus `RepoDelta` cards, file explorers, tree requests, diff requests, media requests, file operation actions, Gitleaks actions, or agent console launch actions.
- FR11: On non-Windows runtime, WSL entries remain absent from visible config/runtime UI and WSL add controls are not rendered.
- FR12: Any WSL-specific Tauri command introduced by this package is compiled and registered only on Windows.
- FR13: Frontend WSL wrapper calls are reachable only from Windows-gated UI paths; local add/autodetect flows continue to work without platform checks.

## Non-Functional Requirements

- NFR1: Additive persistence: existing `workbenches.toml` files without `source`/`distro` continue to load as local repos.
- NFR2: Compatibility: no existing bus contract fields are removed or renamed; TypeScript additions are optional for local entries.
- NFR3: Safety: WSL path validation is lexical and does not touch the Windows filesystem or traverse `\\wsl$`.
- NFR4: Reviewability: split backend persistence/command shape from frontend Windows-only UI if implementation becomes broad.
- NFR5: Testability: include Rust tests for WSL add/remove/duplicate/path validation and frontend tests for Windows rendering plus Linux absence.
- NFR6: Accessibility: the add WSL form uses labelled controls, keyboard submission/cancel, and clear validation text.
- NFR7: Release evidence: record that RDM-003 is configuration-only and still depends on RDM-004 for live monitoring.

## Acceptance Criteria

- AC1: `add_repo` still canonicalizes and validates local repos exactly as before.
- AC2: A Windows-only `add_wsl_repo` path can persist `Ubuntu` plus `/home/...` without local git validation.
- AC3: Invalid WSL paths such as `relative/path`, `C:\repo`, `\\wsl$\Ubuntu\repo`, and blank input are rejected with safe typed errors.
- AC4: `list_workbenches` exposes WSL source metadata on Windows but keeps non-Windows runtime UI absent.
- AC5: A hidden/persisted WSL entry cannot be mounted into local bus snapshots before RDM-004.
- AC6: Frontend tests prove the WSL add control renders when the host is Windows and does not render on Linux.
- AC7: Frontend tests prove configured WSL entries use an Ubuntu/Linux-path label on Windows.
- AC8: Existing frontend absence tests are updated from "no WSL text anywhere" to "no WSL UI renders on non-Windows and WSL command wrappers are isolated."
- AC9: Targeted Rust workbench/bus tests, targeted frontend workbench tests, `npx tsc --noEmit`, and relevant formatting checks pass.

## Impact Scan Expectations

- Backend contract surface: workbench config command payloads and possibly one Windows-only `add_wsl_repo` command.
- Frontend contract surface: `RepoEntry` optional `source`/`distro`, client wrapper for Windows-only command, workbench operations/menu/config UI.
- Runtime guard surface: `runtime_config`, active workbench reseed, bus unsupported entries, and absence tests.
- Consumer scan pattern: `rg "RepoEntry|RepoSource|runtime_config|active_runtime_repos_for|list_workbenches|add_repo|remove_repo|update_repo|wslAbsence|displayName|sortedRepoPaths|addRepoFlow|MenuBar|invoke_handler" src src-tauri docs/contracts`.

## Open Questions

None blocking for RDM-003. RDM-005 still owns the product/security decision for repo-writing commands and agent console sessions on WSL repos.

## Review Result

Requirements review passed after self-review. Findings are recorded at `docs/review-findings/2026-06-23-rdm-003-requirements-review.md`.
