---
title: File overview ruler parity requirements
status: reviewed
date: 2026-06-23
roadmap_item: RUL-001
artifact_kind: requirements-brainstorm
planning_input: true
source_docs:
  - docs/orchestration/compound-master-state.md
  - docs/orchestration/2026-06-16-compound-master-summary.md
  - src/panels/file/FileOverviewRuler.tsx
  - src/panels/file/FileView.tsx
  - src/panels/diff/DiffView.tsx
  - src/panels/diff/FullFileView.tsx
---

# File overview ruler parity requirements

## Problem and Goal

The current file overview ruler is an alert-only rail. It disappears when no markers exist, cannot follow scroll position, and does not behave like the whole-file navigation surface users expect from editors such as VS Code.

Goal: make the right-side overview a persistent whole-file minimap/follow surface that works in full-file and diff views, keeps secret markers visible, supports future hunk/search markers, and remains consistent with the existing Tinto IDE UI.

## Scope In

- Always render the overview surface for text full-file and diff views, even when there are no markers.
- Show a whole-file miniature surface with line-scaled content, marker stripes, and a viewport indicator.
- Keep the overview sticky within the scrollable file body.
- Support click-to-jump on the track and on markers.
- Support keyboard navigation on the track.
- Keep active marker highlighting and clear it when the user scrolls past the marker.
- Add a `source` discriminator to markers for alert, hunk, and search categories.
- Add hunk markers for added/removed/changed diff regions.
- Prepare the marker model for search results without adding a search UI.
- Cover behavior with focused unit tests and a dev/demo fixture suitable for visual review.

## Scope Out

- No global file search feature.
- No backend changes.
- No changes to git diff generation.
- No media/PDF overview.
- No production route that opens the demo fixture from the Tauri app.

## Functional Requirements

- FR1: The overview shall render for text surfaces when total line count is known, even with zero markers.
- FR2: The overview shall expose a scroll-position viewport indicator synced to the visible file body.
- FR3: Clicking the overview track shall scroll the file body to the corresponding line.
- FR4: Clicking a marker shall scroll the file body to that marker line and mark it active.
- FR5: Keyboard focus on the overview shall support ArrowUp, ArrowDown, Home, and End navigation.
- FR6: The marker model shall distinguish alert, hunk, and search sources.
- FR7: Full-file rows with alert markers shall keep their existing non-overlapping inline label behavior when present.
- FR8: Diff views shall create hunk markers from changed lines and show them in the overview.
- FR9: Search markers shall be representable by the component API, but no search workflow shall be introduced in this package.

## Non-Functional Requirements

- NFR1: Existing text/diff layout must not shift or resize when markers appear.
- NFR2: The overview must remain usable for small files and bounded for very large files.
- NFR3: The component must match the existing compact IDE styling.
- NFR4: The surface must remain accessible through labels, focus, and keyboard operation.
- NFR5: No backend, bus contract, auth, persistence, or filesystem authority changes are allowed.

## Acceptance Criteria

- AC1: Unit tests prove the overview renders when marker count is zero.
- AC2: Unit tests prove track click, marker click, and keyboard navigation call the expected jump behavior.
- AC3: Unit tests prove marker test ids disambiguate stacked markers by line and index.
- AC4: Unit tests prove hunk markers are derived from diff lines.
- AC5: Unit tests prove search marker source is accepted by the component API without a search UI.
- AC6: Typecheck passes for affected frontend files.
- AC7: Visual review can open a dev-only demo fixture and confirm minimap, viewport indicator, sticky behavior, and marker interaction.

## Assumptions

- RUL-001 is frontend-only.
- The existing `FileOverviewRuler.tsx` on `develop` is the source of truth, despite older state text claiming RU1 had already been re-applied.
- Dev-only demo files are acceptable if they are not reachable from the Tauri production app.

## Validation Status

Requirements status: reviewed through Compound Master fallback review.
