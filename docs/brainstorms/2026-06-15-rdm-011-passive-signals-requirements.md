---
date: 2026-06-15
topic: rdm-011-passive-signals
---

# Passive Signals Requirements

## Summary

Tinto should turn raw repo facts into lightweight passive signals: visual hints for changes worth looking at and small metrics that summarize the current workbench state. The signals must stay factual and deterministic. They should not explain intent, approve changes, score agent quality, or produce natural-language summaries.

## Problem Frame

The dashboard, diff viewer, watched-file list, and timeline now expose the raw facts of a workbench: status lists, diffs, Plane 2 filesystem events, and commit history. A supervisor still has to scan every surface manually to notice high-attention facts such as sensitive filenames, large deletions, config changes, or test changes.

The design source calls for "passive signals" and "metricas livianas" while also setting a hard product principle: no interpretation. RDM-011 should therefore add deterministic markers and counts, not advice.

## Key Decisions

- **Additive contract extension.** Signals arrive as new fields on existing bus payloads. Existing event names, commands, and required fields are not renamed or removed.
- **Backend-owned detection.** The backend computes repo metrics and signal facts from status, diffs, and Plane 2 events so all frontend views consume one consistent truth.
- **Frontend-owned presentation.** Existing views render badges, small metric rows, and path-level hints without creating a new primary panel.
- **Simple deterministic rules.** Secret-related detection is limited to filename/path patterns and conservative added-line patterns such as PEM/private-key/API-token names. No entropy analysis, remote reputation, AI scoring, or hidden content upload.
- **Facts only.** Signal copy says what matched, not whether the change is good, bad, safe, or ready.

## Actors

- A1. Supervisor user: wants to notice high-attention facts quickly while watching local agents.
- A2. Backend bus: computes metrics and signal facts from already-read local git/watch data.
- A3. Frontend panels: render the facts in Dashboard, Repo, Tree, Diff, Watched Files, and Timeline surfaces.
- A4. Local repo content: remains local and read-only.

## Key Flows

- F1. Repo-level signal scan
  - **Trigger:** A repo delta is emitted or loaded from the snapshot.
  - **Steps:** Backend computes changed-file count, line additions/removals, and a bounded list of signals from status paths and worktree diff hunks.
  - **Outcome:** Cards and repo panels show high-attention facts without the user opening every diff.

- F2. Path-level diff signal
  - **Trigger:** User opens a changed file in a Diff panel.
  - **Steps:** Frontend filters repo signals by the open path and renders matching badges near the diff controls.
  - **Outcome:** A sensitive/config/test/large-delete signal stays visible while inspecting the file.

- F3. Plane 2 watched-file signal
  - **Trigger:** A watched file event is emitted.
  - **Steps:** Backend attaches deterministic signals to the event when its path matches sensitive/config patterns or when size delta crosses the large-change threshold.
  - **Outcome:** The watched-file list shows why a gitignored event deserves attention.

## Requirements

**Contract and computation**

- R1. Extend `RepoDelta` with `metrics` and `signals` additive fields.
- R2. Extend `FsEvent` with additive `signals` so Plane 2 events can be highlighted without a second event stream.
- R3. Metrics must include changed-file count and line additions/removals for the current working-tree diff.
- R4. Signals must be bounded per repo/event to keep payloads lightweight and deterministic.
- R5. Detection must cover sensitive paths, possible secret content patterns in added diff lines, large deletions, config changes, and test changes.
- R6. Signal messages must not include secret values or raw added-line content.
- R7. Backend must keep all operations read-only.

**Frontend presentation**

- R8. Dashboard cards must show compact metrics and the highest-severity signal count.
- R9. Repo panels must show a passive-signals section with deterministic signal rows.
- R10. Tree and Diff surfaces must show path-level signal hints for affected files.
- R11. Watched-file rows must show Plane 2 event signals.
- R12. Timeline may surface signal labels when a current activity entry has repo-level signals, but must not create a separate historical signal database.

**Boundaries**

- R13. No notifications, filters/search, glance mode, or user-configurable rules in this item.
- R14. No natural-language summaries, AI explanations, or quality judgment.
- R15. No durable metrics database; metrics are current-state/session facts.

## Acceptance Examples

- AE1. Given a modified `.env` file, when a repo delta arrives, then the repo card and repo panel show a sensitive-file signal without displaying file contents.
- AE2. Given an added line containing a private-key marker, when diff metrics are computed, then the repo signal reports a possible-secret match for the path and does not include the matched line.
- AE3. Given a diff with a large number of removed lines, when the repo delta arrives, then a large-delete signal appears with line-count metrics.
- AE4. Given a changed `package.json`, `Cargo.toml`, or workflow file, then a config-change signal appears.
- AE5. Given a changed test file, then a test-change signal appears at info severity.
- AE6. Given a Plane 2 event for `.env.local`, then the watched-file row shows the sensitive-file signal.
- AE7. Given no signal-worthy changes, views still show metrics without warning badges.
- AE8. Given more matches than the payload cap, the backend truncates deterministically and includes the highest-severity facts first.

## Success Criteria

- The user can see which repos deserve attention without opening every panel.
- Signals are consistent across Dashboard, Repo, Tree, Diff, Watch, and Timeline surfaces.
- No secret values or raw content are leaked in signal messages.
- The contract remains additive and read-only.

## Scope Boundaries

- No OS notifications or glance mode.
- No filters/search UI.
- No configurable signal rules.
- No persistence or SQLite.
- No AI/natural-language summaries.
- No git write operations.

## Dependencies / Assumptions

- Requires RDM-006, RDM-008, and the UI surfaces from RDM-007/RDM-009/RDM-010.
- Assumption: computing the working-tree diff inside the existing bounded bus recompute path is acceptable for prototype metrics. If performance becomes a problem, RDM-011 may cache or scope metrics later without changing the public contract.
- Assumption: a small fixed cap per repo is enough for v1 because the UI needs attention markers, not exhaustive auditing.

## Sources / Research

- `tinto-design.md`: passive signals and lightweight metrics.
- `docs/contracts/bus-contract.md`: signals were reserved as additive fields in RDM-011.
- `src-tauri/src/bus/contract.rs`: current `RepoDelta` and `FsEvent` payloads.
- `src-tauri/src/bus/mod.rs`: central recompute path for status/diffs and Plane 2 event construction.
- `src/bus/contract.ts` and `src/bus/store.ts`: frontend mirrors and revision-gated state.
- Existing panels: `RepoCard`, `RepoPanel`, `RepoTreePanel`, `DiffPanel`, `WatchedFilesSection`, `TimelinePanel`.
