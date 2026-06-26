---
title: Workbench selection and management completion plan
status: plan-review-passed
date: 2026-06-26
roadmap_item: RDM-014
origin: docs/brainstorms/2026-06-26-014-workbench-management-completion.md
production_posture: prototype
---

# Workbench selection and management completion plan

## Summary

Add the missing frontend layer for full workbench management using the already-delivered backend workbench commands. The capability has four implementation units: MRU ordering, Workbench menu entrypoints, operations wrappers for rename/delete/create/activate, and the manage-workbenches modal.

## Requirements Traceability

- R1, R2 -> U2 Workbench menu.
- R3, R6, R7 -> U3 operations wrappers.
- R4, R5, R8 -> U4 manage modal.
- R9 -> U1 MRU helper.
- R10 -> U5 focused tests and verification.

## Key Technical Decisions

- KTD1: MRU lives in `localStorage` under a versioned key and never becomes source of truth.
- KTD2: The Workbench menu replaces the compact select because create/manage actions need a command surface, not just selection.
- KTD3: Destructive delete is confirmed in the UI and still delegates real validation to the backend command.
- KTD4: Active deletion promotion is frontend-owned for now: after delete, select the first remaining workbench when the removed name was active.
- KTD5: Partial config tolerance belongs in frontend consumers because live config can be transiently incomplete during first-run/recovery races.

## Implementation Units

### U1. MRU ordering helper

Files:
- `src/workbench/recentWorkbenches.ts`
- `src/workbench/recentWorkbenches.test.ts`

Approach:
- Add safe read/write around `localStorage`.
- Implement mark, forget, read, and sort helpers.
- Ignore malformed persisted payloads and non-string entries.

Verification:
- Unit tests for empty state, dedupe, malformed payloads, forget, and stable unknown ordering.

### U2. Workbench menu entrypoints

Files:
- `src/workbench/MenuBar.tsx`
- `src/workbench/workbench.test.tsx`

Approach:
- Add a `Workbench` menu to the existing menu bar.
- List configured workbenches by MRU order and mark active item.
- Keep create and manage actions in the same menu.
- Preserve existing Repos, Proyectos, Ver, Complementos, and Ayuda menus.

Verification:
- Component tests for recent ordering, active mark, switching, empty menu, and opening the manage modal.

### U3. Workbench operation wrappers

Files:
- `src/bus/client.ts`
- `src/workbench/operations.ts`
- `src/workbench/operations.test.ts`

Approach:
- Expose `renameWorkbench` and `deleteWorkbench` wrappers around existing invoke commands.
- Add `renameWorkbenchFlow`, `deleteWorkbenchFlow`, and `pickNextActiveAfterRemove`.
- Preserve existing switch/create/reload patterns.
- Reset bus before reload after delete because active workbench membership may have changed.

Verification:
- Unit tests for rename trimming/MRU update/no-op behavior, active delete promotion, non-active delete, only-workbench delete, and existing remove-repo partial-config tolerance.

### U4. Manage workbenches modal

Files:
- `src/workbench/ManageWorkbenchesDialog.tsx`
- `src/workbench/ManageWorkbenchesDialog.test.tsx`
- `src/App.css`

Approach:
- Render a modal using existing app modal styling.
- Show ordered workbench rows with active badge, repo count, repo list, activate, rename, delete.
- Use inline rename input outside of button nesting so focus and keyboard behavior stay valid.
- After successful creation, close the management modal and return to the Dashboard while keeping the refreshed workbench switcher available.
- Add safe confirmation fallback to `window.confirm`.
- Tolerate missing `workbenches`.

Verification:
- Component tests for ordering, expansion, repo labels/subtitles, activate, rename, delete confirm/cancel, create, post-create Dashboard focus, Escape/backdrop close, click containment, and partial config.

### U5. Review and verification

Files:
- `docs/review-findings/2026-06-26-rdm-014-code-review.md`
- `docs/work-packages/RDM-014-workbench-management-completion/2026-06-26-014-workbench-management-completion-work-package.md`
- `docs/orchestration/compound-master-state.md`

Approach:
- Run work-package checker.
- Run focused Vitest suites, typecheck/build, and Prettier check for changed frontend files.
- Record review finding for nested interactive control and fix it before amend.

## Review Notes

Plan review passed inline. The plan is bounded by existing backend commands and avoids inventing new persistence or product behavior. The one notable risk is destructive delete UX, handled by confirmation, backend validation, focused tests, and explicit non-goal that repos on disk are never deleted.
