---
date: 2026-06-15
topic: rdm-009-watched-files-ui
---

# RDM-009 — Watched files UI — Requirements

- **Roadmap item:** RDM-009 (`docs/roadmaps/2026-06-10-001-tinto-roadmap.md`)
- **Depends on (shipped):** RDM-004 (watcher), RDM-006 (state/event bus), RDM-007 (dockable UI)
- **Current baseline also includes:** RDM-008 (diff viewer), but RDM-009 does not functionally depend on opening diffs.
- **Production posture:** prototype, solo dev, no users
- **Language:** English (repo directive, Wave 4)

## Summary

RDM-009 adds the Plane 2 UI: per-repo watched-file events and an editor for the repo's `fs_watch` patterns. It makes sensitive ignored files visible when users opt into watching them, without pretending those files have git diffs.

## Problem frame

Tinto now shows Plane 1 git state and live diffs, but the second monitoring plane is still invisible. The backend can classify files as Plane 2 when a repo has `fs_watch` patterns, and it emits `tinto://fs-events` with event metadata. Users still cannot see those events or edit the watch patterns without modifying the TOML store by hand.

This item closes that gap. The value is not alerting yet; it is observability and opt-in control for files like `.env`, secrets, generated local config, or other gitignored files that an agent may touch.

## Key decisions

- **D-009-1 — Plane 2 is not a diff.** Watched-file entries show event kind, timestamp, size, and size delta. They do not open the diff viewer by default and they do not synthesize fake git state.
- **D-009-2 — Patterns are edited per repo.** The source of truth is each `RepoEntry.fs_watch` in the active workbench config. The UI edits one repo's list at a time and uses the existing backend `update_repo` command with `fs_watch`.
- **D-009-3 — Recent event memory is frontend-local.** `FsEventBatch` is an event stream, not persisted history. The UI keeps a bounded recent-event list per repo for the session and clears it when the repo leaves the active workbench.
- **D-009-4 — Safe pattern editing over clever validation.** The backend already validates watch patterns through `PathClassifier`. The UI prevents empty duplicates and surfaces backend errors, but does not invent a separate glob dialect.

## Requirements

**Watched-file events**

- R1. Each repo detail view exposes a Watched files section that lists recent Plane 2 events for that repo with path, event kind, timestamp, size, and size delta.
- R2. Plane 2 events update live from `tinto://fs-events` without requiring a git status recompute.
- R3. The event list is bounded per repo and newest-first so a long agent run cannot grow UI memory without bound.
- R4. Empty, degraded, and unavailable states are explicit: no patterns configured, no events yet, watcher degraded, and repo removed from workbench render distinct messages.

**Pattern management**

- R5. Users can add, remove, and edit `fs_watch` patterns for a repo from the UI without editing the TOML file.
- R6. Saving patterns calls the existing `update_repo` command for the active workbench and repo path, then reloads workbench config and active snapshot.
- R7. Pattern editing preserves the repo alias unless the user is editing alias in a future flow.
- R8. The editor rejects empty duplicate rows locally and surfaces backend validation errors without discarding the user's draft.
- R9. The UI makes the active patterns visible near the event list so users can understand why an ignored file is or is not appearing.

**Product boundaries**

- R10. Plane 2 entries do not trigger passive highlights, severity scoring, or aggregated metrics; those belong to RDM-011.
- R11. Plane 2 entries do not become commit history or timeline events; timeline/history is RDM-010.
- R12. The feature stays read-only with respect to watched files themselves: it edits only Tinto's watch configuration.

## Key flows

- F1. **Observe watched events**
  - **Trigger:** A repo has `fs_watch` patterns and the watcher emits a Plane 2 event.
  - **Steps:** The bus listener applies the batch to frontend state; the repo panel shows the new event at the top of the Watched files section; the repo activity indicator updates.
  - **Outcome:** The user sees that a sensitive ignored file changed, without opening a diff.

- F2. **Edit patterns**
  - **Trigger:** The user opens a repo panel and edits its watched patterns.
  - **Steps:** The editor allows row edits, add, and remove; Save calls `update_repo` with `fs_watch`; the UI reloads config/snapshot; the watcher remount flow observes the changed repo config.
  - **Outcome:** Future matching filesystem events appear as Plane 2 events.

## Acceptance examples

- AE1. **Covers R1-R3.** Given a repo with a pattern matching `.env`, when an `FsEventBatch` arrives with `.env` modified, then the repo panel shows `.env`, `modified`, a formatted timestamp, and size metadata newest-first.
- AE2. **Covers R4.** Given the watcher is degraded, when the repo panel renders, then the Watched files section shows the degraded reason instead of implying events are healthy.
- AE3. **Covers R5-R8.** Given a user adds `.env` and `secrets/*.json`, when Save succeeds, then `update_repo` receives the active workbench, repo path, preserved alias fields, and the new `fs_watch` list.
- AE4. **Covers R8.** Given a user enters duplicate or blank patterns, when they try to save, then the UI reports the local validation error and does not call `update_repo`.
- AE5. **Covers R8.** Given the backend rejects a pattern, when Save fails, then the editor keeps the draft visible and shows the backend error message.
- AE6. **Covers D-009-3.** Given more than the per-repo event limit arrives, when the store applies the batches, then only the newest bounded set remains for that repo.
- AE7. **Covers R10-R12.** Given a Plane 2 event appears, when the user interacts with it, then it does not open a diff panel or mutate the watched file.

## Scope boundaries

- In scope: watched-file event list, event state in the frontend store, per-repo pattern editor, existing backend `update_repo` wrapper in the TS client, tests, and styling.
- Deferred: passive signals/highlights and metrics (RDM-011), timeline integration (RDM-010), alias editing, pattern presets, global pattern templates, and persisted event history.
- Out of scope: changing the watcher contract, changing git classification semantics, reading Plane 2 file contents, or adding file write operations.

## Sources

- `docs/roadmaps/2026-06-10-001-tinto-roadmap.md` — RDM-009 scope and dependencies.
- `src/bus/contract.ts` — `FsEventBatch`, `FsEvent`, `RepoEntry.fs_watch`, and `WatchingState`.
- `src/bus/store.ts` — existing `applyFsEvents` activity-only behavior that RDM-009 extends with recent event memory.
- `src-tauri/src/workbench/commands.rs` and `src-tauri/src/lib.rs` — existing registered `update_repo(... fs_watch ...)` command.
- `src-tauri/src/watcher/mod.rs` — watcher rebuilds classification when `fs_watch` changes.
