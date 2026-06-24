---
date: 2026-06-22
topic: rul-overview-ruler-parity
roadmap_item: RUL-001
origin_roadmap: docs/roadmaps/2026-06-22-003-post-closeout-ux.md
---

# RUL-001 File Overview Ruler Parity — Requirements

## Summary

Turn the right-side file overview ruler (currently alert-only) into a true whole-file navigation/follow surface synced to the full document, with the look and feel of the Visual Studio Code overview ruler. Add an always-visible track, a scroll-synced caret indicator, click-to-jump anywhere on the track, configurable width and density via CSS custom properties, and a marker `source` discriminator that supports the upcoming hunk-marker (RU2) and search-marker (RU3, deferred) additions. The work happens inside the existing read-only file/diff surface and must not change the bus contract, the diff subscription model, or the dock layout.

The review path for this iteration is a **dev-only browser fixture** (`http://127.0.0.1:1420/demo.html`) that mounts the rail against a mock 80-line file with 12 secret-pattern lines. The Tauri app is not on the review path because the user could not inspect the rail in the Tauri webview.

---

## Problem Frame

The current ruler (`src/panels/file/FileOverviewRuler.tsx`) renders only when alerts exist, shows a summary chip, and offers clickable alert chips that jump to a line. There is no track, no scroll-synced caret, no full-file click navigation, and no marker for diff hunks. Long files in `DiffView` and `FullFileView` are hard to navigate beyond `Ctrl+F`-style browser search, and the rail does not behave like the global file-follow surface that the design promises.

The 2026-06-22 commit `233bd41` introduced the alert-marker foundation; this brainstorm covers the remaining behavior.

---

## Requirements

**Always-visible track** — R1. Always visible in `DiffView` and `FullFileView` (and the dev fixture) when the body has at least one line. Hidden during loading/empty/binary/oversized guards. R2. Thin vertical column on the right of the body; no layout shift on marker count. R3. Configurable width and density via CSS custom properties.

**Scroll-synced caret indicator** — R4. Follows the visible top-line within one animation frame. R5. Hides when the body has no scroll overflow. R6. Uses `requestAnimationFrame` coalescing to stay smooth on rapid scroll.

**Click-to-jump navigation** — R7. Click anywhere on the track jumps the body so the corresponding line is centered (or as close to center as the body allows). R8. Clicking a marker jumps to that line and visually distinguishes the active marker. R9. Uses the existing `data-line` / `data-new-line` selectors.

**Markers** — R10. Existing alert markers continue to work. R11. Hunk markers (RU2) are derived from `FileDiff.hunks` and pass through the same `markers` prop. R12. Two markers on the same line stack visually; each remains independently clickable. R13. `FileOverviewMarker` gains an optional `source?: "alert" | "hunk" | "search"` field (default `"alert"`). R14. Search markers (RU3) are out of scope, reserved for a future review unit.

**Accessibility and keyboard** — R15. Track and markers are keyboard-reachable (Tab). R16. Caret is `aria-hidden="true"`. R17. Track exposes `role="slider"` with `aria-valuemin/max/now/valuetext`. R18. ArrowUp/Down move ±1 line, Home/End jump to the ends.

**Out of scope** — N1. New file-content search feature. N2. Bus contract, diff subscription, dock layout, or dockview persistence changes. N3. Tauri commands or backend changes (RUL-001 is frontend-only). N4. Rail in `FileView` itself.

---

## Key Decisions

- **Scroll-sync via a single ref + passive scroll listener + `ResizeObserver` + `requestAnimationFrame`.**
- **Reuse the existing `data-line` / `data-new-line` DOM contract.** No new DOM attributes.
- **Optional `bodyRef` on the rail.** The rail stays presentation-only; the view supplies the ref. Falls back to `document` for the click-to-jump query when the ref is null.
- **Marker `source` discriminator on `FileOverviewMarker`** (additive, optional, default `"alert"`).
- **CSS custom properties for width and density:** `--file-overview-ruler-width` and `--file-overview-ruler-density`.
- **Click-to-jump line math:** `line = round(ratio * (totalLines - 1)) + 1`, clamped to `[1, totalLines]`. Disabled when `totalLines <= visibleRows` (no scroll possible).
- **Dev fixture for review.** The fixture bypasses the Tauri webview entirely and mounts the rail against a mock file. It is reached only via `/demo.html`, which the Tauri app never opens, so the fixture is dev-only and never bundled.

---

## Actors

- A1. Developer using Tinto to read or review a file.
- A2. `FileView` (route + view-mode toggle) — hosts `DiffView` and `FullFileView`; passes through the overview markers.
- A3. `DiffView` — renders the diff body, computes hunk markers in RU2, hosts the ruler.
- A4. `FullFileView` — renders the full file body and hosts the ruler.
- A5. `useOverviewScrollSync` hook — reports the first visible top line to the rail.
- A6. Dev fixture (`src/demo/main.tsx` + `demo.html`) — the user-visible review surface for this iteration.

---

## Open Decisions

- O1. **Search-result markers:** blocked on a search feature; deferred to RU3.
- O2. **Marker `source` discriminator:** chosen over a sibling `infoMarkers` prop because it preserves the single `markers` prop surface.

---

## Verification Criteria

- V1. Targeted test: `npm test -- FileOverviewRuler.test.tsx` — always-visible track, scroll-synced caret, click-to-jump, marker stacking, accessibility attributes.
- V2. Targeted test: `npm test -- FileView.test.tsx DiffView.test.tsx FullFileView.test.tsx` — rail presence per view kind; existing marker behavior preserved.
- V3. Full test: `npm test`.
- V4. Lint/format/build: `npm run lint`, `npm run format:check`, `npm run build`.
- V5. **User review via dev fixture** at `http://127.0.0.1:1420/demo.html` (see work package "What to test" for the full checklist).
- V6. No backend changes; no `cargo` commands expected. RUL-001 is frontend-only.
