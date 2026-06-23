---
title: File overview ruler parity
status: review-passed
roadmap_item: RUL-001
origin_roadmap: docs/orchestration/compound-master-state.md
origin_brainstorm: docs/brainstorms/2026-06-23-003-file-overview-ruler-parity.md
origin_planning_input: docs/brainstorms/2026-06-23-003-file-overview-ruler-parity.md
origin_plan: docs/plans/2026-06-23-003-file-overview-ruler-parity-plan.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1, RU2, RU3]
base_branch: develop
pr_strategy: local-fast-forward
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# File overview ruler parity

## Scope

Deliver a VS Code-style whole-file overview/minimap surface for text full-file and diff views, then add hunk marker semantics and future search-marker compatibility.

## Non-goals

- No backend changes.
- No bus contract changes.
- No search UI or search backend.
- No media/PDF overview.
- No production route for the dev demo fixture.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: helper names, CSS class names matching local conventions, equivalent focused tests, and exact dev fixture sample content.
- Agent must record as assumptions: any visual approximation of VS Code behavior and any skipped browser verification.
- Agent must escalate: product-visible search workflow, backend changes, public contract changes, branch/base strategy changes, or non-dev demo routing.
- Safe fallback: continue frontend-only implementation and tests that do not depend on search product decisions.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: current `develop` and existing file/diff view surfaces.
- Blocks: residual backlog item "File overview ruler parity with Visual Studio Code".

## Production Posture

- Posture: prototype.
- Evidence: `docs/orchestration/compound-master-state.md` records prototype posture and local desktop iteration flow.
- Confidence: high.
- Consequences for this package: compatibility with existing file/diff views is required, but no production migration plan is needed.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Whole-file minimap/follow behavior is the required foundation. |
| U2 | yes | Diff hunk markers are the next expected marker category and use existing diff data. |
| U3 | yes | Search marker compatibility is an API/rendering preparation only, not a search product feature. |

Grouping rationale:
- RU1 is isolated because it rewrites the core component and visual behavior.
- RU2 is isolated because it connects diff semantics to marker generation.
- RU3 is small and may be grouped with RU2 if implementation remains limited to marker typing/rendering tests.

## Implementation Units

- U1 - Whole-file Overview Foundation.
- U2 - Diff Hunk Marker Semantics.
- U3 - Search Marker Compatibility.

## Review Unit Progress

| Review unit | Status | Notes |
|---|---|---|
| RU1 | review-passed | Persistent minimap foundation implemented with scroll sync, click/keyboard navigation, zero-marker rendering, bounded mini-row rendering, stable marker ids, and dev-only demo at `/demo.html`. |
| RU2 | review-passed | Diff hunk markers are derived from existing structured diff lines and rendered through the overview marker source model. |
| RU3 | review-passed | Search marker source is supported in the marker API/rendering/tests without adding search UI or backend behavior. |

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Persistent minimap foundation, scroll sync, track/marker navigation, keyboard support, dev demo | `src/panels/file/FileOverviewRuler.tsx`, `src/panels/file/useOverviewScrollSync.ts`, `src/panels/file/FileOverviewRuler.test.tsx`, `src/panels/file/useOverviewScrollSync.test.tsx`, `src/panels/diff/FullFileView.tsx`, `src/panels/diff/DiffView.tsx`, `src/panels/file/FileView.tsx`, `src/App.css`, optional `demo.html` and `src/demo/*` | `develop` | optional Tarea | Medium frontend risk; core interaction and CSS changes. |
| RU2 | Hunk marker derivation and styling | `src/panels/file/FileView.tsx`, `src/panels/diff/DiffView.tsx`, `src/panels/diff/FullFileView.tsx`, related tests, `src/App.css` | RU1 integrated | optional Tarea | Low/medium; uses existing diff data and marker API. |
| RU3 | Search marker source compatibility without search UI | `src/panels/file/FileOverviewRuler.tsx`, tests, docs/state | RU2 integrated | optional Tarea | Low; API/rendering compatibility only. |

## Files and Tests

Expected files:
- `src/panels/file/FileOverviewRuler.tsx`
- `src/panels/file/useOverviewScrollSync.ts`
- `src/panels/file/FileOverviewRuler.test.tsx`
- `src/panels/file/useOverviewScrollSync.test.tsx`
- `src/panels/file/FileView.tsx`
- `src/panels/diff/DiffView.tsx`
- `src/panels/diff/FullFileView.tsx`
- `src/panels/diff/DiffView.test.tsx`
- `src/panels/diff/FullFileView.test.tsx`
- `src/panels/file/FileView.test.tsx`
- `src/App.css`
- optional dev-only `demo.html` and `src/demo/*`

Expected tests:
- Overview renders without markers.
- Overview renders stacked markers with stable test ids.
- Track click and marker click jump to expected file rows.
- Keyboard navigation jumps to expected lines.
- Scroll sync reports top line and viewport position.
- Diff hunk markers appear on changed lines.
- Search marker source renders through the component API.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: frontend component API only.
- Consumer scan patterns: `rg "FileOverviewRuler|FileOverviewMarker|overviewMarkers|data-new-line|data-line" src`.
- Consumers found: expected file and diff view surfaces.
- Contract-drift tests searched: frontend component tests only; no backend/bus contract changes.
- Required consumer tests: affected file/diff tests plus typecheck.
- Consumer tests run/skipped: complete. Focused file/diff overview tests, typecheck, formatter check, and diff whitespace check passed. Browser screenshot automation was skipped because no Browser control tool was exposed and Playwright/Puppeteer are not installed; Vite served `/demo.html` with HTTP 200.

## Verification Gate

- `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx`
- `npm run test -- src/panels/file/FileView.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx`
- `npx tsc --noEmit`
- Visual smoke of the dev fixture when possible.
- Surface-aware evidence: changed marker model, scroll sync, diff/full-file consumers, and CSS each need targeted test or visual evidence.
- Production posture evidence: frontend-only prototype change; no migration or backend regression evidence required.

Verification results:
- `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx src/panels/file/FileView.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx`: 28 passed.
- `npx tsc --noEmit`: passed.
- `npx prettier --check <changed files>`: passed.
- `git diff --check`: passed.
- `http://127.0.0.1:1422/demo.html`: HTTP 200.
- Visual screenshot verification: skipped because no Browser control tool was exposed and `playwright`/`puppeteer` are missing.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Code review result: passed. Findings path: `docs/review-findings/2026-06-23-rul-001-code-review.md`.

## Security Gate

- Run after work-review loop: not required by default.
- Security Watch during work: light, because the package must not add backend, filesystem, auth, secret handling, or external process authority.
- Security Watch notes: dev fixture must remain dev-only and unreachable from production Tauri app.
- Security reviewer: fallback inline.
- Security review result: passed inline. No backend/Tauri command, auth, filesystem, secret-handling, external process, or production demo routing changes were made.
- Required security verification: completed by changed-file inspection and targeted tests/typecheck.

## CI Break-Prevention And Escalation

- CI risk surfaces: TypeScript compile, Vitest, CSS/layout regressions.
- Preventive evidence: targeted tests and typecheck.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: release follow-up is blocked until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1/RU2/RU3 - Persistent minimap foundation, hunk markers, and search marker compatibility.
- Branch name: `feat/file-overview-minimap`
- Branch/docs rule: the executable review unit carries related planning artifacts with implementation; current project preference is direct local commit/push to `develop`, no PR, batched at the end of the active Compound Master run.
- PR base: `develop` if the user requests PR flow; otherwise final local no-PR release targets `develop` over `origin/develop`.
- Suggested commit grouping for this release unit:
  - `feat(files): add persistent file overview minimap` - component, hook, CSS, consumers, and tests.
  - `docs(orchestration): add file overview ruler parity artifacts [skip ci]` - requirements, plan, package, findings, and state.
- PR title: `Add persistent file overview minimap`
- PR body bullets:
  - Add a persistent whole-file overview surface for text and diff views.
  - Sync the viewport indicator and support track, marker, and keyboard navigation.
  - Keep the work frontend-only with focused tests and dev-only visual smoke.
- Verification results location: this package and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea unless multiple RUL review units are tracked under one parent.
- Jira summary: `Mejorar el minimapa de archivos`
- Jira description: `Convertir la regla lateral de archivos en una superficie persistente de navegacion y seguimiento, compatible con marcadores de secretos, hunks y busqueda futura.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
