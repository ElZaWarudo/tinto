---
date: 2026-06-15
topic: rdm-010-timeline-history
---

# Timeline and History Requirements

## Summary

Tinto should add a dockable timeline surface that shows chronological workbench activity across repos, lets the user inspect commit history and commit diffs without a terminal, and flags dirty repos that have stayed uncommitted for a while. The first version is read-only and session-oriented: it uses in-memory state plus existing git history commands, not SQLite.

---

## Problem Frame

Dashboard cards show the current state, repo panels show per-repo detail, and diff panels show the current working-tree change. They do not answer the time-based questions a supervisor asks while agents are working: what moved recently, which repo produced a commit, what changed in that commit, and which dirty repo has been sitting unresolved.

The design source calls this "Timeline / historial" and explicitly keeps historical persistence as a later SQLite step. RDM-010 should therefore close the initial temporal view without creating a durable event database.

---

## Key Decisions

- **Dockable timeline panel.** Timeline should live as a first-class dockview panel, not as a modal or a hidden subsection, so it can sit beside Dashboard, Tree, Repo, and Diff panels.
- **Session feed, not historical database.** Activity events are derived from the current bus state, Plane 2 events, and on-demand commit log reads; no event persistence or SQLite is introduced.
- **Commit inspection uses the frozen contract.** Commit navigation uses existing `get_commit_log`, `get_commit_diff`, and `get_blob` commands; new backend behavior is out of scope unless execution proves a contract gap.
- **Orphan detection is a frontend heuristic.** A repo with dirty status and no activity for a threshold becomes "orphaned"; the threshold is a UI constant for this package, not a persisted user preference.
- **Facts only.** The timeline may group, sort, and label facts, but it must not summarize changes in natural language or judge whether a change is good.

---

## Actors

- A1. Supervisor user: watches agent work across a workbench and chooses what to inspect.
- A2. Local repos in the active workbench: provide current status, activity timestamps, and commit history.
- A3. Tinto frontend: accumulates session activity and renders chronological, read-only navigation.
- A4. Tinto backend: serves already-allowlisted git history and blob reads.

---

## Key Flows

- F1. Cross-repo activity scan
  - **Trigger:** The user opens the Timeline panel while a workbench is active.
  - **Actors:** A1, A2, A3.
  - **Steps:** Timeline reads current repo state, recent activity timestamps, and recent Plane 2 events; it renders newest-first entries grouped by repo and kind.
  - **Outcome:** The user sees what changed recently without opening each repo panel.

- F2. Commit diff navigation
  - **Trigger:** The user selects a commit entry.
  - **Actors:** A1, A3, A4.
  - **Steps:** Timeline loads commit diffs, shows touched files, and renders a selected file diff with the existing diff renderer.
  - **Outcome:** The user can inspect a commit diff without leaving Tinto or using a terminal.

- F3. Orphan detection
  - **Trigger:** A repo remains dirty past the quiet threshold.
  - **Actors:** A1, A2, A3.
  - **Steps:** Timeline computes dirty repos from status and activity timestamps; entries older than the threshold are marked as orphaned.
  - **Outcome:** The user can spot working trees that may need attention.

---

## Requirements

**Timeline feed**

- R1. Timeline must render a newest-first workbench feed spanning all repos in the active workbench.
- R2. Timeline entries must distinguish current working-tree activity, Plane 2 file events, commits, degraded/error states, and orphaned dirty repos.
- R3. Timeline must show repo display names while preserving canonical paths as opaque identities.
- R4. Timeline must remain useful when the watcher is degraded by rendering on-demand state and visible degraded messaging.

**Commit history**

- R5. Timeline must load recent commits per repo using paginated commit log reads.
- R6. Selecting a commit must load its diff and show touched files.
- R7. Selecting a changed file inside a commit must render that file's commit diff with the existing structured diff renderer.
- R8. Commit history navigation must be read-only and must not create git operations.

**Orphans and boundaries**

- R9. Timeline must flag dirty repos that have stayed quiet longer than the configured orphan threshold.
- R10. Orphan detection must be deterministic and testable with injected or controlled time.
- R11. Timeline must not persist historical events beyond the current frontend session.
- R12. Timeline must not include passive signal severity, filters/search, notifications, or natural-language summaries.

---

## Acceptance Examples

- AE1. Given two repos with different `last_activity_ms` values, when Timeline opens, then the newer repo activity appears first.
- AE2. Given recent Plane 2 events for a repo, when Timeline renders, then those events appear as file-event entries and do not open diffs.
- AE3. Given a commit with two file diffs, when the user selects the commit and then a file, then the file diff renders through the structured diff view.
- AE4. Given `get_commit_diff` fails, when the user selects a commit, then Timeline shows a retryable error state without crashing the workspace.
- AE5. Given a dirty repo whose last activity is older than the orphan threshold, when Timeline renders, then it shows an orphan entry.
- AE6. Given a dirty repo whose last activity is newer than the orphan threshold, when Timeline renders, then it does not show as orphaned.
- AE7. Given no repos in the active workbench, when Timeline opens, then it shows an empty state with no errors.
- AE8. Given watcher degraded state, when Timeline opens, then it labels the feed as on-demand/degraded instead of implying live completeness.

---

## Success Criteria

- The user can answer "what moved recently?" from one panel.
- The user can inspect commit diffs without a terminal.
- Dirty quiet repos are visible as orphan candidates.
- The implementation stays within the frozen bus contract unless a real gap is proven.

---

## Scope Boundaries

- No SQLite or durable historical timeline storage.
- No filters/search by repo, extension, or date range; that belongs to RDM-012.
- No notifications or glance mode; that belongs to RDM-012.
- No passive severity signals, secret detection, metrics, or line-count analytics; that belongs to RDM-011.
- No git writes, staging, commits, branch operations, revert, or approval workflow.
- No natural-language summaries of commits or diffs.

---

## Dependencies / Assumptions

- Requires RDM-002, RDM-006, and RDM-007. RDM-008 is available and should be reused for structured diff rendering.
- Assumption: the initial orphan threshold can be a frontend constant, proposed as 30 minutes, because no user preference system exists yet.
- Assumption: commit feed depth can start with a small per-repo limit to protect UI cost; pagination or "load more" can be included if cheap.
- Assumption: frontend-only session accumulation is acceptable for v1 because the roadmap explicitly defers SQLite persistence.

---

## Sources / Research

- `tinto-design.md`: Timeline / historial lists cross-repo feed, commit diff navigation, and orphan detection.
- `docs/contracts/bus-contract.md`: maps Timeline needs to frontend accumulation, `get_commit_log`, `get_commit_diff`, and current state timestamps.
- `src/bus/client.ts`: currently exposes `get_commit_log` but not TS wrappers for `get_commit_diff` or `get_blob`.
- `src/panels/diff/DiffView.tsx`: existing structured diff renderer to reuse for commit diffs.
- `src/workspace/panels.ts`: stable panel registry pattern for adding a `timeline` panel.
