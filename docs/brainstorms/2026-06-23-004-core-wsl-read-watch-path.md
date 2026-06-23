---
title: Core WSL read/watch path and event forwarding
status: reviewed
date: 2026-06-23
roadmap_item: RDM-004
source_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
validation_status: validated
---

# Core WSL Read/Watch Path And Event Forwarding

## Problem And Goal

Tinto must let a Windows user track local Windows repositories and Ubuntu WSL repositories in the same active workbench. Local repositories must keep using the existing in-process git and filesystem watcher path. WSL repositories must be handled through the Linux-side `tinto-agent` so Linux paths are not translated through Windows filesystem semantics.

Success means a workbench snapshot can contain both repo types, the current repo viewer can read WSL repo state/files/diffs through the existing frontend contract, and WSL activity reaches the same Tauri event names that the frontend already consumes.

## Actors And Stakeholders

- Primary user: Windows desktop user with one or more local repos plus one or more Ubuntu WSL repos.
- Internal maintainer: Tinto developer preserving the frozen bus contract and Linux absence rule.
- Reviewer/security owner: ensures WSL support does not accidentally widen local filesystem authority or enable repo mutations.

## Scope In

- Windows-only runtime support for configured Ubuntu WSL repos in the active workbench.
- Simultaneous active tracking of local Windows repos and WSL repos.
- WSL backend selection from persisted `RepoEntry.source == "wsl"`.
- Minimal request/response protocol for WSL repo status, branch/head, worktree diff, commit diff/log/blob, file content, and repo tree.
- WSL snapshot and delta emission using existing `RepoDelta` and `tinto://workbench-delta`.
- WSL filesystem activity forwarding using existing `FsEventBatch` and `tinto://fs-events`.
- WSL subscribed diff refresh using existing `SubscriptionTarget.subscribed_diffs`.
- Per-repo WSL agent/distro/path failures mapped into `RepoErrorState`.
- Agent-side active-workbench allowlist for configured WSL repo paths.
- Tests that local and WSL repos can coexist without sending WSL paths to local canonicalization or the local watcher.

## Scope Out

- File mutations, delete/move/copy/export, `.gitleaks.toml` creation, Gitleaks scans, and managed Gitleaks install/status for WSL repos.
- Media preview reads for WSL repos.
- Agent Console PTY sessions for WSL repos.
- Arbitrary distro selection beyond the already approved initial Ubuntu path.
- `\\wsl$` path translation or Windows-side direct traversal of Linux repo files.
- Any WSL UI, commands, settings, empty states, degraded notices, or behavior on non-Windows desktop builds.
- Packaging, auto-install, auto-update, or recovery UX for the agent.

## Constraints And Business Rules

- Linux desktop behavior remains local-only and unchanged.
- WSL support is additive to the existing frontend contract; no second frontend data model.
- Local watcher degradation remains represented by global `WatchingState`; WSL failures are per-repo errors.
- Local repos are still canonicalized and watched by the local backend; WSL repos are treated as opaque Linux paths and are never canonicalized by Windows.
- The host must launch WSL commands with argument vectors only, not shell interpolation.
- The agent must reject requests for repo paths outside its current allowlist.
- WSL file reads keep the same size, binary, `.git`, and traversal guard intent as local reads, enforced inside Linux.

## Functional Requirements

- FR1: When the active Windows workbench contains both local and WSL repos, Tinto shall include both in `get_workbench_snapshot`.
- FR2: When a WSL repo is snapshotted, Tinto shall populate `RepoDelta` with status, branch, head, metrics, revision, activity time, gitleaks configuration state, and optional subscribed diffs using the existing JSON shape.
- FR3: When a local repo is snapshotted, existing behavior shall remain unchanged.
- FR4: When a frontend read command targets a WSL repo, Tinto shall route the request through `tinto-agent` instead of local git/filesystem code.
- FR5: When a frontend read command targets a local repo, Tinto shall keep using the local code path.
- FR6: When a WSL repo emits file activity, Tinto shall emit `tinto://fs-events` for that repo using existing event shape.
- FR7: When WSL repo activity affects repo status or subscribed files, Tinto shall emit `tinto://workbench-delta` for that repo using existing event shape.
- FR8: When WSL/distro/agent/path handling fails, Tinto shall keep other repos active and attach a safe `RepoErrorState` to only the affected WSL repo.
- FR9: When subscriptions are set for a WSL repo, Tinto shall refresh subscribed diffs through the WSL agent and not drop local repo subscriptions.
- FR10: When a WSL repo is retried, Tinto shall attempt a WSL refresh for that repo without remounting it through the local watcher.

## Non-Functional Requirements

- Security: no WSL repo path may be resolved through Windows filesystem APIs after it is identified as WSL.
- Compatibility: frontend contract names and event names remain unchanged.
- Isolation: WSL agent requests must be bounded by message size, request type, and repo allowlist.
- Resilience: WSL backend failure must degrade per repo, not globally.
- Testability: Linux CI can cover mocked host/agent routing, local regression behavior, and protocol/guard behavior; final Windows/Ubuntu smoke remains a release gate.

## Acceptance Criteria

- Given a workbench with one local repo and one WSL repo, when the bus starts on Windows, then the local repo is watched locally and the WSL repo is represented in snapshot without local canonicalization.
- Given a WSL repo path, when `get_worktree_diff`, `get_blob`, `get_file_content`, `get_commit_log`, `get_commit_diff`, or `list_repo_tree` is invoked, then the request is routed to the WSL backend and returns the existing DTO shape.
- Given a local repo path, when the same commands are invoked, then the current local path behavior and tests still pass.
- Given a WSL agent failure, when the snapshot or retry path runs, then only that WSL repo reports a safe per-repo error.
- Given both local and WSL subscriptions, when subscriptions are updated, then each repo type refreshes through its own backend and the frontend receives standard `RepoDelta` payloads.
- Given a non-Windows build/test configuration, when WSL runtime code is scanned, then WSL UI/commands/runtime entry points remain absent or `cfg(target_os = "windows")` gated.

## Assumptions And Open Questions

- Assumption: the first implementation may use the approved dev-source agent launch model from RDM-002.
- Assumption: polling-based WSL watch inside the agent is acceptable for the first monitoring path if it preserves the event contract and is bounded.
- Assumption: WSL media preview remains deferred even though local media preview exists.
- Open question deferred to RDM-005: whether repo mutations, Gitleaks, and Agent Console should stay disabled or later route through the agent.

## Validation Notes

The scope matches the latest user request to track WSL and Windows projects at the same time. No new user question is required before planning because distro, Windows-only posture, and dev-source agent model were already decided for the current initiative.
