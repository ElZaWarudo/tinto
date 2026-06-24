---
title: "RUL-001 RU1 demo fixture review findings"
date: 2026-06-23
review_unit: RUL-001 RU1
surface: http://127.0.0.1:1420/demo.html
status: review-passed
reviewer: compound-master-inline
---

# RUL-001 RU1 Demo Fixture Review Findings

## Summary

Browser review of `http://127.0.0.1:1420/demo.html` initially found blocking RU1 behavior gaps in the overview ruler fixture. The issues were fixed and re-verified on 2026-06-23. A follow-up user review clarified that the expected surface is a VS Code-style minimap, not a thin marker rail. The fixture now loads and renders an always-visible, scroll-pinned, adaptively sized minimap-width track with mini-code content, 16 expected critical markers, a viewport overlay, focusable slider semantics, accurate scroll-sync, click-to-jump, marker activation, keyboard navigation, scroll-past active clear, and a clean console.

## Findings

### P1 - Marker `data-line` attributes contaminate scroll-sync and jump targets - resolved

- Evidence: at `scrollTop = 0`, the first visible code line is 1, but the fixture reports `topLine: 7` and `aria-valuenow="7"`.
- Evidence: after Home, visible code lines are 1-42, but the slider still reports line 7.
- Evidence: after End, visible code lines are 39-80, but the slider reports line 76.
- Cause: `useOverviewScrollSync` queries `body.querySelectorAll("[data-line], [data-new-line]")`, while `FileOverviewRuler` also renders marker buttons with `data-line`. Because the ruler is inside the same scroll body and appears before the file content, marker buttons can be selected as line elements.
- Code refs: `src/panels/file/useOverviewScrollSync.ts:24`, `src/panels/file/FileOverviewRuler.tsx:140`.
- Impact: R4, R17, and the fixture `topLine` review path are not trustworthy. The caret and slider value follow marker positions rather than the first visible file line.
- Resolution: marker buttons now use `data-marker-line`, `useOverviewScrollSync` ignores `.file-overview-ruler` descendants, and `FileOverviewRuler` resolves jump targets through a helper that excludes rail descendants.
- Re-test evidence: at `scrollTop = 0`, visible code starts at line 1 and the fixture reports `topLine: 1` / `aria-valuenow="1"`. After End, visible code lines are 39-80 and the slider reports line 39. After Home, visible code lines are 1-42 and the slider reports line 1.

### P1 - Marker clicks can scroll to the marker button instead of the file line - resolved

- Evidence: clicking marker line 57 set the active marker but left visible code lines at 9-51, so line 57 was not visible or centered.
- Cause: `jumpToLine()` runs `root.querySelector([data-line="57"])`; inside the fixture body, the matching marker button precedes the real `.full-file__line[data-line="57"]`.
- Code refs: `src/panels/file/FileOverviewRuler.tsx:41`, `src/panels/file/FileOverviewRuler.tsx:140`, `src/demo/main.tsx:155`.
- Impact: R7, R8, and keyboard navigation are unreliable for marker lines because jumps can target rail controls rather than file content.
- Resolution: marker buttons no longer expose `data-line`; jump target lookup excludes `.file-overview-ruler` descendants.
- Re-test evidence: clicking marker line 57 scrolls to visible code lines 36-78, activates marker 57, and highlights code line 57.

### P2 - The fixture side panel does not reflect rail-driven active line - resolved

- Evidence: after clicking the rail, the side panel stayed `activeLine: -`; after clicking marker line 57, the panel still stayed `activeLine: -`.
- Cause: `FileOverviewRuler` owns its own `activeLine` state, while the fixture side panel reads `DemoFileView`'s separate `activeLine` state. Rail jumps do not notify the fixture state.
- Code refs: `src/panels/file/FileOverviewRuler.tsx:31`, `src/demo/main.tsx:117`, `src/demo/main.tsx:187`.
- Impact: the checklist item "activeLine updates when you click a marker or the track" fails, and the fixture cannot be used as stated for user verification.
- Resolution: `FileOverviewRuler` accepts an optional `onActiveLineChange` callback; the fixture passes `setActiveLine` so rail-driven jumps update the side panel.
- Re-test evidence: clicking the rail sets `activeLine: 40`; clicking marker 57 sets `activeLine: 57`; scrolling past marker 7 clears the active marker and side-panel value.

### P3 - Console contains a favicon 404 - resolved

- Evidence: Chrome console shows `Failed to load resource: the server responded with a status of 404 (Not Found)` for `http://127.0.0.1:1420/favicon.ico`.
- Code ref: `demo.html:3`.
- Impact: the checklist says "No console errors"; this is not behavior-breaking, but it makes the checklist fail literally.
- Resolution: `demo.html` now declares an embedded empty favicon.
- Re-test evidence: console contains only Vite debug messages and the React DevTools informational message; no errors.

## Passing Checks

- `demo.html` served HTTP 200 from Vite on port 1420.
- Ruler rendered with `file-overview-ruler--has-track`.
- Track is focusable with `role="slider"`, `aria-valuemin="1"`, `aria-valuemax="80"`, and caret `aria-hidden="true"`.
- The required marker lines are present: 7, 13, 14, 23, 24, 25, 26, 38, 39, 46, 47, 57, 58, 59, 73, 76.
- Targeted tests passed before the fix: `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx` (9/9).
- Targeted tests passed after the fix: `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx` (11/11).
- Affected suite passed after the fix: `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx src/panels/file/FileView.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx` (33/33).
- TypeScript passed after the fix: `npx tsc --noEmit`.
- Prettier passed after the fix for changed files.
- Minimap visual rework verification: browser inspection confirmed a 122 px overview surface, 80 mini-code lines, 16 required marker lines, viewport overlay, no console errors, marker line 57 click centers code line 57, and Home returns to `topLine: 1`.
- Sticky minimap verification: after scrolling the fixture body from `scrollTop=0` to `scrollTop=650`, the minimap stayed fixed at `top=153` / `bottom=882` while `topLine` updated from 1 to 37.
- Adaptive-size verification: the 80-line fixture renders a 320 px minimap track (`80 * 4px`) inside the sticky 729 px available area, so shorter files no longer stretch to full height. Tests cover 2-line files collapsing to the 96 px minimum and 5000-line files rendering only 600 sampled minimap rows.
- Scroll-past-end verification: open file and diff surfaces add bottom scroll space equal to the visible file body height minus one line. In the fixture, scrolling to the end places line 80 at the top of the file body (`offset=0`) with 723 px of scroll space below it.
- Viewport-indicator smoothness: the minimap viewport indicator is now driven by continuous `scrollTop / maxScroll` progress instead of discrete `topLine`, and updates with `transform: translate3d(...)` plus fixed pixel height. The transition was removed so the indicator tracks scroll frames directly instead of lagging behind.
- Marker semantics: the minimap includes a compact legend badge for marker meanings, currently `posibles secretos` plus future-ready hunk/search grouping. Tests cover accessible labels and hover/title summaries.
- Inline marker labels: full-file and diff rows with overview markers show a right-side non-overlapping label such as `Possible secret` inside the reserved gutter before the minimap.

## Fix Applied

- Marker metadata now uses `data-marker-line` instead of `data-line`.
- Scroll-sync filters out `.file-overview-ruler` descendants.
- Jump target lookup filters out `.file-overview-ruler` descendants.
- The fixture side panel subscribes to rail active-line changes.
- The overview surface now renders mini-code content and a viewport overlay, matching the requested VS Code minimap direction.
- The overview surface is `position: sticky` inside the file scroll container and sizes itself from the visible body height, so it remains visible while the file content scrolls.
- The minimap track uses adaptive height: minimum 96 px, 4 px per line until it reaches the visible container height, and sampled rendering capped at 600 rows for huge files.
- Full-file and diff content surfaces use measured file-body height for bottom scroll padding, allowing the final line to be moved to the top of the visible panel without adding fake line numbers.
- The minimap viewport indicator uses compositor-friendly transform animation for smoother scroll feedback.
- The minimap summary badge groups marker types so red stripes have an explicit meaning instead of being unexplained decoration.
- Marked rows render an inline label in the reserved right gutter, with title text carrying line context and `pointer-events: none` so the label does not block code interaction.
- Regression tests cover embedded rail/content selector collisions and accidental rail `data-line` descendants.
- `demo.html` suppresses the favicon 404.
