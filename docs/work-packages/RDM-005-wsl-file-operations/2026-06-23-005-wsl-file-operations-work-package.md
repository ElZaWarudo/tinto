---
title: WSL file operations parity
status: review-passed
roadmap_item: RDM-005
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-005-wsl-file-operations.md
origin_planning_input: docs/brainstorms/2026-06-23-005-wsl-file-operations.md
origin_plan: docs/plans/2026-06-23-005-wsl-file-operations-plan.md
units: [U1, U2, U3, U4]
unit_alignment: partial
review_units: [RU1]
base_branch: develop
pr_strategy: local-final-batch
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# WSL File Operations Parity

## Scope

Route existing Tauri file-operation commands through `tinto-agent` when the active repo is a configured Ubuntu WSL repo, while keeping local repos on the existing local filesystem path.

## Non-goals

- No WSL Gitleaks support.
- No WSL media preview support.
- No WSL Agent Console support.
- No shell interpolation or `\\wsl$`.

## Autonomy Contract

- Mode: guarded.
- Agent may decide helper names and internal DTO shapes that preserve the public command contract.
- Agent must escalate any product change to overwrite, undo, delete, or cross-repo semantics.
- Safe fallback: keep local behavior unchanged and fail WSL closed with safe errors.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL file operation routing and agent handlers | `src-tauri/src/file_ops/*`, `src-tauri/src/wsl_agent/*`, docs/tests | `develop` plus queued WSL work | optional Tarea | High risk because it enables WSL repo mutations; requires security review. |

## Impact Scan

- Changed command routing: `copy_to_repo`, `copy_within_repo`, `move_within_repo`, `export_from_repo`, `delete_from_repo`, `restore_deleted_from_repo`, `redo_deleted_from_repo`.
- Consumer scan: frontend wrappers already call the same commands, so no public command rename is expected.
- Required tests: Rust file operation/agent tests, frontend contract tests if DTOs change.

## Implementation Summary

- Added WSL agent protocol/runtime handlers for copy into repo, copy within repo, move, export, delete, restore, and redo.
- Routed existing Tauri file operation commands by `RepoSource`, preserving local repo behavior and public frontend command/DTO shapes.
- Reused active-workbench allowlist resolution, repo containment, `.git` rejection, existing conflict reporting, and delete backup manifest semantics.
- Translated Windows host paths to `/mnt/<drive>/...` for WSL drag/drop paste sources and export destinations without shell interpolation.

## Files and Tests

- Runtime files: `src-tauri/src/file_ops/mod.rs`, `src-tauri/src/file_ops/commands.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`, `src-tauri/src/wsl_agent/launcher.rs`.
- Contract/docs: `docs/contracts/bus-contract.md`, this work package, orchestration state, and direct review findings.
- Verification: `cargo test --lib file_ops`, `cargo test --lib wsl_agent`, `cargo test --lib bus -- --test-threads=1`, `cargo build --bin tinto-agent`, `npm test -- src/bus/contract.test.ts src/panels/tree/ProjectExplorer.test.tsx`, `npx tsc --noEmit`, targeted Rust formatting, work package checker, and `git diff --check`.

## Verification Gate

- `cargo test --lib file_ops`
- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/tree/ProjectExplorer.test.tsx`
- `npx tsc --noEmit`
- targeted Rust format check
- `git diff --check`

## Security Gate

- Required because WSL repo mutation support is enabled.
- Must verify no shell, no `\\wsl$`, source allowlist, path containment, `.git` rejection, and safe error categories.

Result: passed by direct security fallback on 2026-06-23. Findings path: `docs/review-findings/2026-06-23-rdm-005-code-security-review.md`.

## Review Result

- Status: `review-passed`.
- No P0-P2 findings remain after direct code/security review.
- Honest note: this package covers file operations only. WSL Gitleaks, media previews, Agent Console sessions, packaging/recovery, and fine-grained WSL `fs-events` remain deferred to later work.

## Verification Results

- `cargo test --lib file_ops`: passed.
- `cargo test --lib wsl_agent`: passed, 17 tests.
- `cargo test --lib bus -- --test-threads=1`: passed, 42 tests.
- `cargo build --bin tinto-agent`: passed.
- `npm test -- src/bus/contract.test.ts src/panels/tree/ProjectExplorer.test.tsx`: passed, 35 tests.
- `npx tsc --noEmit`: passed.

## Branch and PR Handoff Inputs

- Review unit: RU1.
- Base branch: `develop`.
- Release strategy: local final batch, no PR unless the user changes the standing preference.
- Suggested commit: `feat(files): route WSL file operations through agent`.
- PR body bullets:
  - Routes drag/drop, paste, move, delete, restore, redo, and export file operations through `tinto-agent` when the repo is Ubuntu WSL.
  - Keeps local repo file operations on the existing local filesystem path and preserves the frontend command contract.
  - Reuses repo allowlists, containment, `.git` rejection, conflict DTOs, and delete backup manifests across local and WSL repos.
  - Leaves WSL Gitleaks, media preview, Agent Console, packaging/recovery, and fine-grained `fs-events` as explicit deferred work.

## Jira Handoff Inputs

- Jira policy: optional.
- Jira summary: `Mantener operaciones de archivos en repos WSL`
- Jira description: `Permitir arrastrar, pegar, mover, borrar, restaurar y exportar archivos en repos Ubuntu WSL desde Tinto, preservando las mismas guardas de paths y conflictos que en repos locales.`
