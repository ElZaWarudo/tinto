---
title: File overview ruler parity plan
status: reviewed
date: 2026-06-23
origin_requirements: docs/brainstorms/2026-06-23-003-file-overview-ruler-parity.md
artifact_kind: delivery-plan
---

# File overview ruler parity plan

## Planning Source

- Requirements: `docs/brainstorms/2026-06-23-003-file-overview-ruler-parity.md`.
- Current code evidence: `src/panels/file/FileOverviewRuler.tsx` still implements an alert-only rail that returns `null` with zero markers.
- State discrepancy: `docs/orchestration/compound-master-state.md` previously described RU1 as re-applied, but the current worktree contradicts that. The plan uses the current worktree as authority.

## Delivery Approach

Hybrid incremental plan. RU1 restores the whole-file overview foundation. RU2 adds hunk marker semantics. RU3 prepares search marker compatibility without adding a search product workflow.

## Units

### U1 - Whole-file Overview Foundation

- Replace the alert-only rail with a persistent overview surface.
- Add scroll sync via a focused hook.
- Add click-to-jump, marker activation, keyboard navigation, adaptive track sizing, bounded mini-line rendering, and zero-marker rendering.
- Add focused component/hook tests and a dev-only visual fixture.

### U2 - Diff Hunk Marker Semantics

- Derive overview markers from changed diff lines in inline and side-by-side views.
- Preserve existing alert markers and ensure stacked marker rendering is stable.
- Add hunk marker styling and tests.

### U3 - Search Marker Compatibility

- Extend marker typing, legend, and rendering to accept search markers.
- Do not add a search UI, search backend, or search state.
- Add API-level tests proving search markers are supported as future input.

## Dependencies

- U2 depends on U1 because hunk markers need the persistent overview surface.
- U3 depends on U1 marker-source support and can land with U2 if the diff is still reviewable.

## Verification

- `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx`
- `npm run test -- src/panels/file/FileView.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx`
- `npx tsc --noEmit`
- Visual smoke of `demo.html` if the dev server is available.

## Risks and Mitigations

- Scroll math can become flaky in jsdom. Mitigation: isolate pure line/position behavior and keep DOM scroll tests focused.
- Minimap rendering can become expensive for huge files. Mitigation: render sampled mini-lines with a cap.
- Overview markers can interfere with file-row lookup. Mitigation: marker elements use marker-specific data attributes and line queries ignore overview descendants.

## Planning Status

Planned and ready for work-package derivation.
