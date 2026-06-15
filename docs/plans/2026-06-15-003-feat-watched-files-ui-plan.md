---
title: Watched files UI
type: feat
date: 2026-06-15
origin: docs/brainstorms/2026-06-15-rdm-009-watched-files-ui-requirements.md
---

# Watched files UI — Implementation Plan

## Summary

Build the Plane 2 UI for watched files: recent filesystem events per repo, visible `fs_watch` patterns, and a per-repo pattern editor backed by the existing registered `update_repo` command. The backend command and watcher behavior already exist; the plan is frontend state, client wrapper, panel UI, tests, and styling.

## Problem frame

RDM-004 and RDM-006 made Plane 2 observable in the backend event stream, but the current frontend only uses filesystem events to bump activity. Users cannot see what watched ignored files changed, and they cannot edit `fs_watch` without touching the workbench TOML.

## Requirements

**Watched-file event display**

- R1. Repo detail surfaces recent Plane 2 events with path, kind, timestamp, size, and size delta.
- R2. Events update live from `FsEventBatch` and are bounded per repo.
- R3. Empty, degraded, no-patterns, and missing-repo states render distinct copy.

**Pattern editing**

- R4. Users can add, edit, remove, cancel, and save per-repo `fs_watch` patterns.
- R5. Saving uses the existing `update_repo` command and preserves alias state.
- R6. Local validation blocks blank duplicate patterns; backend failures keep the draft visible.

**Boundaries**

- R7. Plane 2 events do not open diffs, synthesize git state, or become passive alerts.
- R8. No backend Rust command or contract change is planned; `update_repo` is already registered in `src-tauri/src/lib.rs`.

## Key technical decisions

- **KTD1 — Extend store state for recent events.** `BusStore.applyFsEvents` currently updates activity only. Add a bounded `fsEventsByRepo` map so panels can render recent Plane 2 events without persisting history.
- **KTD2 — Use existing backend `update_repo`.** `src-tauri/src/workbench/commands.rs` already accepts `fs_watch: Option<Vec<String>>`; add only the TS client wrapper and call it from the UI.
- **KTD3 — Reload after save.** After a successful save, call `reloadActiveWorkbench()` to refresh config and snapshot. The bus/watch remount path already rebuilds classifiers when `fs_watch` changes.
- **KTD4 — Editor draft is local to the panel.** The draft initializes from `busStore.config`, survives failed saves, and resets on cancel or successful reload.
- **KTD5 — Plane 2 remains non-diff.** Event rows are informational and inert for RDM-009. RDM-011 may add highlighting and scoring later.

## Existing patterns to follow

- Store mutation and selector style: `src/bus/store.ts`, `src/bus/store.test.ts`.
- Client wrapper style: `src/bus/client.ts`.
- Workbench mutation/reload flow: `src/workbench/operations.ts`.
- Repo panel layout and state copy: `src/panels/RepoPanel.tsx`, `src/panels/RepoPanel.test.tsx`.
- App-wide styling tokens: `src/App.css`.

## Implementation units

### U1. Add watched-event state to the bus store

- **Goal:** Preserve a bounded recent Plane 2 event list per repo while retaining the existing activity update behavior.
- **Files:** `src/bus/store.ts`, `src/bus/store.test.ts`.
- **Approach:** Add `fsEventsByRepo: Record<string, FsEvent[]>`, prepend new events newest-first, cap each repo at a small constant, drop entries when repos leave membership, and clear on reset.
- **Test scenarios:** apply a batch and see newest-first events; cap is enforced; unknown repo is ignored; repo leaving membership drops events; existing activity behavior remains.
- **Verification:** `npm test -- --run src/bus/store.test.ts`.

### U2. Add client and operation support for `fs_watch`

- **Goal:** Expose the backend `update_repo` command and provide a safe per-repo save flow.
- **Files:** `src/bus/client.ts`, `src/workbench/operations.ts`, `src/workbench/operations.test.ts` if needed.
- **Approach:** Add `updateRepo(workbench, path, { alias?, clearAlias?, fsWatch? })`; add `updateRepoFsWatch(active, repo, patterns)` that preserves alias by omitting alias fields and reloads the active workbench after success.
- **Test scenarios:** wrapper invokes `update_repo` with `{ workbench, path, fsWatch }`; save flow reloads after success; failed save rejects so the UI can keep the draft.
- **Verification:** targeted Vitest for wrapper/operation or component-level invocation coverage if operations stay thin.

### U3. Build the watched-files panel section

- **Goal:** Show patterns and recent Plane 2 events inside the repo detail panel.
- **Files:** `src/panels/WatchedFilesSection.tsx`, `src/panels/WatchedFilesSection.test.tsx`, `src/panels/RepoPanel.tsx`, `src/App.css`.
- **Approach:** Extract a section component that receives repo path, active workbench name/config, watching state, events, and save callback; render event rows, pattern chips/list, and state copy.
- **Test scenarios:** no patterns; patterns but no events; watcher degraded; events render path/kind/time/size; missing config renders a safe loading message.
- **Verification:** component Vitest and full `npm test`.

### U4. Add the pattern editor

- **Goal:** Allow add/edit/remove/cancel/save of `fs_watch` rows without losing drafts on failure.
- **Files:** `src/panels/WatchedFilesSection.tsx`, `src/panels/WatchedFilesSection.test.tsx`, `src/App.css`.
- **Approach:** Use local React state for draft patterns, inline row inputs, Add pattern, Remove, Save, and Cancel. Normalize by trimming rows and preserving order of unique rows.
- **Test scenarios:** add/remove/edit; blank and duplicate validation blocks save; successful save calls the supplied callback and exits dirty state; backend error remains visible with draft intact.
- **Verification:** component Vitest and lint.

### U5. Integrate with RepoPanel and workspace data

- **Goal:** Wire the section into `RepoPanel` using store selectors and existing action patterns.
- **Files:** `src/panels/RepoPanel.tsx`, `src/panels/RepoPanel.test.tsx`, `src/bus/store.ts`.
- **Approach:** Find the repo's `RepoEntry` in the active workbench config, pass `fs_watch`, events, and `watching` into the section, and call the save operation with the active workbench name.
- **Test scenarios:** RepoPanel displays watched section for a repo; save uses active workbench and repo path; repo missing still shows the existing missing-repo message.
- **Verification:** targeted RepoPanel tests plus full Vitest.

### U6. Full verification and release readiness

- **Goal:** Confirm RDM-009 remains frontend-only except TS wrapper usage and does not regress RDM-008.
- **Files:** `docs/orchestration/compound-master-state.md`, work package status.
- **Approach:** Run the frontend and Rust gates; run a Tauri boot smoke if the UI wiring changes boot behavior.
- **Verification:** `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`.

## Acceptance examples

- AE1. A `.env` modified event renders in the repo's watched-files section with metadata and does not open a diff.
- AE2. A repo with configured patterns and no recent events shows the patterns plus "No watched file events yet."
- AE3. Saving `.env` and `secrets/*.json` calls `update_repo` with the active workbench and repo path.
- AE4. Blank or duplicate pattern rows show validation and do not call the backend.
- AE5. Backend save failure leaves the draft visible and shows an error.
- AE6. More than the per-repo event cap keeps only newest events.

## Risks and mitigations

- **Command wrapper risk:** the Rust command is registered, but the TS wrapper must use the exact snake_case command name and argument shape.
- **Event memory growth:** U1 caps per-repo events and clears stale repos.
- **Pattern dialect mismatch:** UI avoids custom glob validation beyond blank/duplicate checks and lets backend validation own semantics.
- **Watcher remount timing:** Save reloads config/snapshot; existing watcher tests prove `fs_watch` changes rebuild classifiers.

## Sources

- `docs/brainstorms/2026-06-15-rdm-009-watched-files-ui-requirements.md`
- `src/bus/contract.ts`
- `src/bus/store.ts`
- `src-tauri/src/workbench/commands.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/watcher/mod.rs`
