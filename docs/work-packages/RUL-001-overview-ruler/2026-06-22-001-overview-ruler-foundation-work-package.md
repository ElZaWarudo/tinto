---
title: RUL-001 File Overview Ruler VS Code Parity
status: review-passed
roadmap_item: RUL-001
origin_roadmap: docs/roadmaps/2026-06-22-003-post-closeout-ux.md
origin_brainstorm: docs/brainstorms/2026-06-22-001-rul-overview-ruler-requirements.md
origin_planning_input: docs/brainstorms/2026-06-22-001-rul-overview-ruler-requirements.md
origin_plan: docs/plans/2026-06-22-001-feat-overview-ruler-foundation-plan.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1, RU2, RU3]
base_branch: develop
pr_strategy: none
max_open_stack: 1
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# RUL-001 File Overview Ruler VS Code Parity

## Supersession Note

This 2026-06-22 artifact is historical. The current RUL-001 source of truth is `docs/work-packages/RUL-001-file-overview-ruler-parity/2026-06-23-003-file-overview-ruler-parity-work-package.md`, which records RU1/RU2/RU3 as implemented, verified, and review-passed. Compound Master state also records the 2026-06-23 package as the current work package and release-ready unit. This file is kept for traceability only and must not be used as an active `in-review` blocker.

## Scope

Convert the right-side file overview ruler from an alert-only chip rail into a true whole-file navigation/follow surface synced to the full document, matching the Visual Studio Code overview-ruler experience:

- Always-visible track in `DiffView` and `FullFileView` (hidden only during loading/empty/binary/oversized guards).
- Scroll-synced caret indicator following the visible top-line.
- Click-to-jump on the full track.
- Configurable width and density via CSS custom properties.
- Diff-hunk markers as a distinct marker type alongside alerts.
- Active-marker highlight and a11y (keyboard reachability, `aria-hidden` caret).
- **Dev-only browser fixture for review** at `http://127.0.0.1:1420/demo.html` (Tauri app is not on the review path for this iteration).

## Non-goals

- Adding a new file-content search feature or any search command/event/payload.
- Search-result markers in the rail (RU3 placeholder only; blocked on the search feature).
- Changing the bus contract, the diff subscription lifecycle, the dock layout, or the dockview persistence model.
- Adding new Tauri commands, Rust files, or backend behavior. RUL-001 is frontend-only.
- Moving the rail into `FileView` itself (markdown/media surfaces).

## Autonomy Contract

- Mode: `guarded`
- Agent may decide without asking: internal method names, test mocks, debounce/throttle implementation, CSS variable names, file structure inside `src/panels/file/` and `src/demo/`, and equivalent verification commands.
- Agent must record as assumptions: jsdom scroll behavior, mock boundaries for `ResizeObserver`, click coordinate math on the track, any local build blocker caused by the running app binary, and the rationale for the dev fixture as the review path.
- Agent must escalate: any new Tauri command or backend change, any bus contract or persistence schema change, branch/base strategy changes, or scope expansion into search/markdown/media surfaces.
- Safe fallback: continue with frontend work and unit tests when visual smoke testing is blocked; record the gap.
- Autonomous ledger: `none`
- Allowed external mutation classes: `[]`

## Dependencies

- Requires: the existing `FileOverviewRuler`, `FileView`, `DiffView`, and `FullFileView` shipped on `develop` (alert-marker foundation at `233bd41`).
- Blocks: RUL-001-RU3 (search markers) when/if a search feature lands.
- External blockers: none.

## Production Posture

- Posture: `prototype`
- Evidence: post-closeout UX iteration in `docs/orchestration/compound-master-state.md`.
- Confidence: high
- Consequences for this package: additive frontend changes are acceptable when verified locally. No breaking changes to existing contract or layout.
- Breaking existing behavior allowed: no intentional breakage; additive rail behavior only.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1: Overview ruler foundation + dev fixture | yes (RU1) | Required for VS Code parity baseline; dev fixture is the user review path |
| U2: Diff-hunk markers in the rail | yes (RU2) | Required for the second visible parity feature |
| U3: Search-result markers (deferred) | placeholder only (RU3) | Blocked on a separate file-content search feature |

## Implementation Units

- U1. Overview ruler foundation (track, caret, click, configurability, a11y) + dev fixture for browser review.
- U2. Diff-hunk markers in the rail.
- U3. Search-result markers (deferred placeholder, blocked).

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1: foundation track, scroll-synced caret, click-to-jump, configurability, a11y, marker `source` field, dev fixture | `FileOverviewRuler.tsx`, new `useOverviewScrollSync.ts`, new `FileOverviewRuler.test.tsx`, new `useOverviewScrollSync.test.tsx`, `DiffView.tsx`, `FullFileView.tsx`, `FileView.tsx`, `App.css`, new `demo.html` + `src/demo/main.tsx` + `src/demo/demo.css` | develop | none (jira-env-not-configured) | Medium risk: new hook + DOM contract extension; large-ish diff but localized |
| RU2 | U2: hunk markers derivation in `DiffView` + hunk style | `DiffView.tsx`, `DiffView.test.tsx`, `FileOverviewRuler.test.tsx`, `App.css` | develop after RU1 | none (jira-env-not-configured) | Low/medium risk: additive marker type |
| RU3 | U2: search markers (deferred placeholder, blocked) | none in this iteration | n/a | n/a | Blocked on a search feature |

Rules:
- Execute one RU at a time. RU1 first; RU2 after RU1 is verified and merged.
- Keep related CSS changes inside the same RU as the React changes that use them.
- Keep tests inside the same RU as the production code they cover.
- Do not start RU2 until RU1 has passed user review in the dev fixture and is merged on `develop`.

## Reviewability Diagnosis

- RU1 is independently reviewable: it touches one well-known component (`FileOverviewRuler`), adds a new hook (`useOverviewScrollSync`), updates the two view components only to forward scroll/click props, and adds a self-contained dev fixture. The contract change (one optional `source` field on `FileOverviewMarker`, one new `bodyRef` prop, one `topLine` prop) is small and the test surface is focused on the rail itself.
- RU2 is independently reviewable: it adds a small derivation in `DiffView` and a new visual style for the `source: "hunk"` markers.
- RU3 is a placeholder and does not ship in this iteration.
- Open-stack plan: with only one ready review unit active on the `develop` branch at any time, the open-stack cap is comfortably respected. `pr_strategy: none`.
- Independent mergeability: RU1 can ship and be reviewed without RU2. With only RU1 merged, the rail still works correctly for alerts; hunk markers are simply absent until RU2 lands. RU2 is strictly additive on top of RU1.
- Granularity rationale: coarsening RU1 and RU2 into a single PR would mix rail scaffolding with a new marker type and make the review harder. Splitting keeps each PR focused on one concern.

## Files and Tests

- Frontend:
  - `src/panels/file/FileOverviewRuler.tsx` (rewrite)
  - `src/panels/file/useOverviewScrollSync.ts` (new)
  - `src/panels/file/FileOverviewRuler.test.tsx` (new)
  - `src/panels/file/useOverviewScrollSync.test.tsx` (new)
  - `src/panels/diff/DiffView.tsx`
  - `src/panels/diff/DiffView.test.tsx` (extend with hunk-marker cases in RU2)
  - `src/panels/diff/FullFileView.tsx`
  - `src/panels/diff/FullFileView.test.tsx` (regression: rail still hides during loading/binary/oversized)
  - `src/panels/file/FileView.test.tsx` (regression: rail placement)
  - `src/App.css`
- Dev fixture (dev-only, never bundled in the Tauri app):
  - `demo.html` (Vite multi-page entry)
  - `src/demo/main.tsx` (mounts the rail against a mock 80-line file with 12 secret-pattern lines)
  - `src/demo/demo.css`
- Backend: none.
- Orchestration: `docs/orchestration/compound-master-state.md`, this work package, the roadmap, the brainstorm, and the plan.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: none. RUL-001 is frontend-only and does not touch the bus contract.
- Consumer scan patterns: `FileOverviewRuler`, `FileOverviewMarker`, `data-line`, `data-new-line`, `.file-view__body`, `.diff-view`, `.full-file`.
- Consumers found: `src/panels/file/FileView.tsx`, `src/panels/file/FileOverviewRuler.tsx`, `src/panels/diff/DiffView.tsx`, `src/panels/diff/FullFileView.tsx`, `src/panels/file/FileView.test.tsx`, `src/panels/diff/DiffView.test.tsx`, `src/panels/diff/FullFileView.test.tsx`, `src/App.css`, and the new `src/demo/*`.
- Contract-drift tests searched: `src/panels/diff/DiffView.test.tsx`, `src/panels/diff/FullFileView.test.tsx`, `src/panels/file/FileView.test.tsx`. No bus contract changes.
- Required consumer tests: rail behavior tests in `FileOverviewRuler.test.tsx`; regression on rail placement in the three view tests.
- Consumer tests run/skipped: RU1 ran the targeted rail + hook + view tests; all 241 frontend tests pass.

## Verification Gate

- RU1:
  - `npm test -- FileOverviewRuler.test.tsx useOverviewScrollSync.test.tsx FileView.test.tsx DiffView.test.tsx FullFileView.test.tsx` (last run: 33/33)
  - `npm test` (full suite, last run: 241/241)
  - `npx tsc --noEmit` (clean)
  - `npx eslint <changed files>` (clean on RU1 changes; pre-existing issues in `ProjectExplorer.tsx` / `TerminalPanel.tsx` / `AddonsManager.tsx` are out of scope)
  - `npx prettier --check <changed files>` (clean)
  - `npm run build` (clean; only the pre-existing Vite chunk-size warning)
  - **User review via the dev fixture** at `http://127.0.0.1:1420/demo.html` — see "What to test (user review checklist)" below
- RU2: `npm test -- FileOverviewRuler.test.tsx DiffView.test.tsx`; full `npm test`; `npm run lint`; `npm run build`.
- Surface-aware evidence: rail placement, scroll-sync behavior, click-to-jump math, marker stacking, active-marker highlight, a11y attributes, dev fixture served at HTTP 200.
- No `cargo` commands expected. No backend changes.

## What to test (user review checklist)

Open `http://127.0.0.1:1420/demo.html` in Chrome/Firefox with DevTools (F12) open. The fixture mounts a mock 80-line file (`src/handler.ts`) with 12 secret-pattern lines (api_key, password, AWS keys, JWT, etc.).

1. **Track always visible.** The right-side rail is rendered even with zero markers on the same line range. The 2 px right-edge gradient should be visible on a dark background.
2. **Caret follows scroll.** Scroll the body with the mouse wheel or scrollbar. The white caret inside the rail moves in lockstep with the visible top line. The `topLine` value in the side panel updates on every scroll.
3. **Click on the rail (empty region).** Clicking anywhere on the track (between markers) scrolls the body so the corresponding line is centered, and the `activeLine` value in the side panel updates.
4. **Click on a red alert chip.** Scrolls the body to that line and applies the `--active` outline to the chip. The `activeLine` value updates.
5. **Click on a line in the body.** Activates that line and updates `activeLine` in the side panel.
6. **Keyboard nav on the track.** Tab into the track (focus ring visible), then ArrowUp/ArrowDown move ±1 line, Home jumps to line 1, End jumps to line 80.
7. **Active-marker scroll-past clear.** Click a marker (e.g. line 7), then scroll down past it. The `--active` outline clears.
8. **12 critical markers** should be visible at lines 7, 13, 14, 23, 24, 25, 26, 38, 39, 46, 47, 57, 58, 59, 73, 76.
9. **No console errors** in DevTools Console. Warnings from the `react-hooks/set-state-in-effect` rule on the scroll-past useEffect are expected and documented in `FileOverviewRuler.tsx`.
10. **DevTools probes (optional, paste in Console):**
    ```js
    document.querySelector('[data-testid="demo-body"]').scrollHeight
    document.querySelectorAll('[data-line]').length
    document.querySelector('[data-testid="file-overview-ruler-caret"]')?.style.top
    ```

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Internal review mode: direct Compound Master fallback review (no subagent spawning in this runtime, consistent with the precedent set in `docs/orchestration/compound-master-state.md`).

## Security Gate

- RUL-001 is frontend-only. No Tauri command, no backend IPC, no new event payload, no new persistence, no new auth/tenant/ownership surface. Focused Security Sentinel is not required for this slice; a direct Security Watch note is recorded below.

## CI Break-Prevention And Escalation

- CI risk surfaces: frontend typecheck/build/lint/test only. No Rust/Cargo surface in this package.
- Preventive evidence: record local command outcomes per RU; if visual smoke testing is blocked, record the gap and continue.
- If CI breaks: invoke `krt-ci-questor` or perform direct evidence-first triage with workflow/job context.
- Escalation rule: do not bypass failing CI without explicit user approval.

## Branch and PR Handoff Inputs

- Review unit: RU1 overview ruler foundation.
- Branch name: `feat/overview-ruler-foundation` (work locally on `develop` per standing user preference; the branch is used only if the user requests a PR; default path is direct local fast-forward merge into `develop` and push, no PR).
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop
- Suggested commit grouping for RU1:
  - `feat(file-view): add scroll-synced caret and click-to-jump to file overview ruler` - `FileOverviewRuler.tsx`, `useOverviewScrollSync.ts`, `DiffView.tsx`, `FullFileView.tsx`, `FileView.tsx`, `App.css`.
  - `test(file-view): cover always-visible track, scroll sync, click, and marker source` - `FileOverviewRuler.test.tsx`, `useOverviewScrollSync.test.tsx`, test id format updates in the three view tests.
  - `chore(dev): add RUL-001 rail browser fixture` - `demo.html`, `src/demo/main.tsx`, `src/demo/demo.css`.
  - `docs(orchestration): add RUL-001 overview ruler work package state [skip ci]` - roadmap, brainstorm, plan, work package, Compound Master state.
- PR title: Add file overview ruler foundation (VS Code parity)
- PR body bullets:
  - Add an always-visible track with configurable width and density to the file overview ruler.
  - Add a scroll-synced caret indicator that follows the visible top-line and hides when the body has no scroll overflow.
  - Add click-to-jump on the full track and an active-marker highlight.
  - Extend `FileOverviewMarker` with an optional `source` field so future marker types (hunks, search) can be distinguished from alerts.
  - Add a dev-only browser fixture (`/demo.html`) for reviewing the rail in a normal Chrome/Firefox DevTools session.
  - Document the ruler foundation in the orchestration artifacts and the state file.
- Verification results location: this work package Execution Status section and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: prototype-local frontend-only changes; the dev fixture never reaches the Tauri app.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Jira summary: Anadir rail de navegacion tipo VS Code al file overview
- Jira description: Convertir el file overview ruler en una superficie de navegacion/seguimiento de todo el archivo: track siempre visible, caret sincronizado al scroll, click-to-jump, ancho configurable, y soporte para marcadores de hunks de diff.
- Optional-policy fallback: Jira omitted (jira-env-not-configured). Recorded in the release closeout and the state file.

## Execution Status

- Historical RU1 implementation status: re-applied locally on `develop` after the previous revert. This status was superseded by the 2026-06-23 RUL-001 parity package, where RU1/RU2/RU3 are implemented, verified, and review-passed.
- RU1 changed surfaces (re-applied):
  - `src/panels/file/FileOverviewRuler.tsx` (rewrite: always-visible track, scroll-synced caret, click-to-jump, active marker highlight with scroll-past clear, `source` discriminator on `FileOverviewMarker`, a11y `role=slider` + aria-valuemin/max/now/valuetext, keyboard nav, `aria-hidden` on caret).
  - `src/panels/file/useOverviewScrollSync.ts` (new: passive scroll listener + `ResizeObserver` + `requestAnimationFrame` coalescing; first line whose bottom is strictly below the viewport top).
  - `src/panels/file/FileOverviewRuler.test.tsx` (new, 7 tests: always-visible track, scroll-synced caret, click-to-jump on track and on markers, marker stacking, a11y, keyboard nav, `data-new-line` jump target, empty-file fast path).
  - `src/panels/file/useOverviewScrollSync.test.tsx` (new, 2 tests: rAF coalescing and scroll-driven `topLine` update).
  - `src/panels/diff/DiffView.tsx` (forwards optional `bodyRef`; uses the hook to compute `topLine`).
  - `src/panels/diff/FullFileView.tsx` (forwards optional `bodyRef`; uses the hook with `lines.length`; computes `lines` via `useMemo`).
  - `src/panels/file/FileView.tsx` (owns `bodyRef` on `.file-view__body`; forwards to 3 `FullFileView` calls and 1 `DiffView` call).
  - `src/panels/diff/FullFileView.test.tsx`, `src/panels/diff/DiffView.test.tsx`, `src/panels/file/FileView.test.tsx` (test id format updated to `overview-marker-{line}-{index}` to disambiguate stacked markers).
  - `src/App.css` (new `--has-track` class makes the track clickable, new `--empty` class for the empty state, caret indicator style, hunk-marker style placeholder, active-marker outline + glow).
  - `demo.html` (Vite multi-page entry), `src/demo/main.tsx` (mock fixture), `src/demo/demo.css` (fixture styles) — dev-only, never bundled in the Tauri app.
- RU1 implementation notes:
  - The rail stays inside `DiffView` and `FullFileView`; `FileView` does not host a rail (markdown/media surfaces keep the current empty space). This matches the existing placement and keeps RU1 small.
  - The `FileOverviewMarker` type gained an optional `source?: "alert" | "hunk" | "search"` field with default `"alert"`. Alert markers keep their `severity: "critical"` visual treatment; the hunk style is reserved for RU2.
  - Click-to-jump on the full track uses `offsetY / trackHeight` ratio and is a no-op when `totalLines <= 0` or when the rect has zero height.
  - Keyboard nav on the track uses ArrowUp/Down for ±1 line, Home/End for the ends; `tabIndex=0` makes the track keyboard-reachable.
  - The hook's "first visible" semantic is tightened to `rect.bottom > bodyRect.top` (strict) so the caret is on the line the user actually sees at the top, not a line whose bottom edge just touches the viewport top.
  - No backend changes; no `cargo` commands expected. RUL-001 is frontend-only.
  - The dev fixture is the user-visible review surface for this iteration. It bypasses the Tauri webview entirely and mounts the rail against a mock file, so the user can review in a normal Chrome/Firefox DevTools session.
- RU1 direct review result: no P0-P2 findings. Direct Compound Master fallback review used (no subagent spawning in this runtime, per `state.md:64-67`). Security Watch note: frontend-only slice; no new IPC, no new event payload, no new persistence, no new auth/tenant, no new external integration; rail is presentation-only; dev fixture never reaches the Tauri app; focused Security Sentinel not required.
- RU1 verification:
  - `npx vitest run src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx src/panels/file/FileView.test.tsx` passed 33/33.
  - `npx vitest run` (full suite) passed 241/241.
  - `npx tsc --noEmit` clean.
  - `npx eslint <changed files>` clean (no new issues; pre-existing issues in `ProjectExplorer.tsx` / `TerminalPanel.tsx` / `AddonsManager.tsx` are out of scope for this RU).
  - `npx prettier --check <changed files>` clean.
  - `npm run build` clean (only the pre-existing Vite chunk-size warning).
  - Vite dev server serves `demo.html` (HTTP 200) and `src/demo/main.tsx` (transformed).
- Historical RU1 user review status: superseded. The current 2026-06-23 RUL-001 package records the dev fixture review and verification evidence.
- Historical RU2 implementation status: superseded. The current 2026-06-23 RUL-001 package records diff hunk markers as implemented and review-passed.
- Historical RU3 implementation status: superseded. The current 2026-06-23 RUL-001 package records search marker source compatibility as implemented and review-passed without adding search UI or backend behavior.
- RUL-001 package status: superseded by the current 2026-06-23 RUL-001 parity package; no active `in-review` blocker remains in this historical artifact.
