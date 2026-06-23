---
title: Windows-only WSL workbench source and path UX plan
status: plan-review-passed
roadmap_item: RDM-003
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_requirements: docs/brainstorms/2026-06-23-003-wsl-workbench-path-ux.md
date: 2026-06-23
delivery_approach: split-review-units
release_timing: final-batch
---

# Windows-only WSL Workbench Source and Path UX Plan

## Goal

Let Windows users configure an Ubuntu WSL repo in a workbench using a Linux path while preserving local repo behavior and keeping WSL entirely absent on Linux/non-Windows runtime. This package remains configuration-only; RDM-004 owns live WSL read/watch behavior.

## Scope

Included:
- Backend WSL repo persistence helper and validation.
- Windows-only `add_wsl_repo` command and invoke registration.
- Additive TypeScript workbench config fields.
- Windows-only frontend add WSL form/menu entry.
- Configured WSL entry display labels on Windows.
- Linux/non-Windows absence tests.

Excluded:
- WSL browse/list.
- Distro discovery beyond fixed Ubuntu.
- Agent launch or health probe.
- Bus read/watch/event forwarding.
- File tree/diff/media/Gitleaks/file operations/agent console behavior for WSL repos.
- Production packaging or installer changes.

## Implementation Units

### U1 - Backend WSL Workbench Persistence

Files:
- `src-tauri/src/workbench/mod.rs`
- `src-tauri/src/workbench/commands.rs`
- `src-tauri/src/lib.rs`

Tasks:
- Add lexical WSL Linux path validation helper.
- Add `RepoEntry::wsl(distro, path, alias)` or equivalent constructor.
- Add `WorkbenchStore::add_wsl_repo` and WSL-aware duplicate detection using source + distro + path.
- Add WSL-aware removal matching that does not remove local entries sharing the same textual path.
- Keep `add_repo`, local validation, local canonicalization, and local duplicate behavior unchanged.
- Keep WSL entries out of local bus runtime mounting until RDM-004 unless a Windows config-only projection is introduced separately from bus repo deltas.
- Add a Windows-only command wrapper for `add_wsl_repo`.
- Register `add_wsl_repo` only under `#[cfg(target_os = "windows")]`.

Tests:
- Local add/remove regression.
- WSL add persists source/distro/path/alias/fs_watch.
- WSL duplicate source+distro+path rejected.
- Local and WSL entries with the same textual path do not collide.
- WSL remove removes only the matching WSL entry.
- Invalid WSL paths rejected safely.
- Non-Windows invoke handler does not register `add_wsl_repo`.

### U2 - Frontend Contract and Windows Gate

Files:
- `src/bus/contract.ts`
- `src/bus/client.ts`
- `src/workbench/platform.ts` or equivalent small helper
- `src/workbench/wslAbsence.test.ts`

Tasks:
- Add optional `source?: "local" | "wsl"` and `distro?: string | null` to frontend `RepoEntry`.
- Add client wrapper for `add_wsl_repo`, isolated from local add flows.
- Add a platform gate helper that can be mocked in tests.
- Update absence tests from source-string bans to runtime/rendered behavior checks and wrapper-isolation checks.

Tests:
- Type-level fixture or component tests cover local entries without `source`/`distro`.
- Platform helper returns Windows/non-Windows behavior under mocks.
- Non-Windows tests prove no WSL add control or configured WSL entry renders.

### U3 - Windows-Only Add WSL UX and Labels

Files:
- `src/workbench/operations.ts`
- `src/workbench/MenuBar.tsx`
- optional `src/workbench/AddWslRepoDialog.tsx`
- `src/workbench/workbench.test.tsx`
- `src/workbench/operations.test.ts`
- `src/panels/DashboardPanel.tsx` only if configured entries need a visible non-live section

Tasks:
- Add an "Add WSL repo..." action visible only on Windows.
- Build a small labelled dialog/form for Ubuntu and Linux absolute path.
- Validate input before invoking the backend.
- On success, reload workbench config/snapshot and close the dialog.
- Display configured WSL entries on Windows with a source badge/label such as `Ubuntu:/home/user/repo`.
- Avoid opening project tabs, file explorers, or bus-backed repo cards for WSL entries until RDM-004.
- Keep local add/autodetect UX unchanged.

Tests:
- Menu shows Add WSL repo on Windows only.
- Form validates blank, relative, Windows drive, and UNC-like input.
- Valid Ubuntu/Linux path invokes `add_wsl_repo`.
- Configured WSL entries display with a WSL label on Windows.
- Local add repo test remains unchanged.

## Review Units

- RU1: Backend WSL workbench persistence and Windows-only command registration.
- RU2: Frontend contract/platform gate and Windows-only add/display UX.
- RU3: Cross-surface absence/regression verification and package closeout.

## Impact Scan

Run before review:

```powershell
rg "RepoEntry|RepoSource|runtime_config|active_runtime_repos_for|list_workbenches|add_repo|remove_repo|update_repo|wslAbsence|displayName|sortedRepoPaths|addRepoFlow|MenuBar|invoke_handler|add_wsl_repo" src src-tauri docs/contracts
```

Expected consumers:
- `src-tauri/src/workbench/mod.rs`
- `src-tauri/src/workbench/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/bus/mod.rs`
- `src/bus/contract.ts`
- `src/bus/client.ts`
- `src/bus/store.ts`
- `src/workbench/operations.ts`
- `src/workbench/MenuBar.tsx`
- `src/workbench/wslAbsence.test.ts`
- dashboard/project menu tests

## Verification

Required before marking review-passed:
- `cargo test --lib workbench`
- `cargo test --lib invoke_handler`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/workbench/workbench.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts`
- `npx tsc --noEmit`
- Relevant Prettier checks for changed TS/TSX files
- Rust formatting for changed Rust files, noting the existing `secret_scan.rs` global fmt drift if it still blocks full `cargo fmt --check`
- `git diff --check`

Optional/manual:
- Windows manual smoke: add Ubuntu + `/home/...` WSL repo and confirm it appears as configured but not yet monitored.

## Security Watch

Required. This package accepts user-provided Linux paths and introduces a Windows-only command.

Checks:
- No Windows filesystem canonicalization or `\\wsl$` traversal for WSL identity.
- No agent launch or command execution in RDM-003.
- No local git/file/bus command route for WSL entries before RDM-004.
- Safe typed errors for invalid WSL paths and duplicate entries.
- No Linux/non-Windows command registration or rendered WSL controls.

## Release Notes For Final Batch

RDM-003 should be released together with RUL-001/RDM-002 only after the active package set is complete. Release notes must say RDM-003 adds Windows-only WSL repo configuration UX, not full WSL monitoring.

## Plan Review Result

Passed. Findings are recorded at `docs/review-findings/2026-06-23-rdm-003-plan-review.md`.
