---
title: Workbench management completion code review
status: passed
date: 2026-06-26
roadmap_item: RDM-014
review_unit: RU1
review_type: inline-code-review
threshold: P0-P2
---

# Workbench management completion code review

## Scope Reviewed

- Commits inspected: `3da7192`, `27a9501`, `d6a6e20`, `3f68601`.
- Runtime surfaces: MRU helper, Workbench menu, manage modal, operations wrappers, bus client wrappers, CSS, focused tests.
- Backend surfaces checked for compatibility: existing `rename_workbench`, `delete_workbench`, and `WorkbenchConfig` behavior.

## Findings

| Severity | Finding | Status |
|---|---|---|
| P2 | `ManageWorkbenchesDialog` rendered the inline rename `<input>` inside the row toggle `<button>`. Interactive controls nested inside a button are invalid HTML and can cause focus/key/click behavior to diverge across browsers/webviews. | Fixed by rendering the rename state as a non-button `.manage-workbench__toggle` container while keeping the normal collapsed/expanded state as a real button. |
| P2 | The Workbench menu used `window.prompt` for new workbench creation, and the manage modal lacked enough active-workbench context for a high-risk management surface. | Fixed by removing the duplicate create menu item, keeping creation inside the manage modal, adding an active-workbench summary, returning to Dashboard after successful creation, and adding focused regression tests. |

## Positive Checks

- MRU helper is best-effort and does not replace backend `WorkbenchConfig` as source of truth.
- Rename/delete wrappers preserve backend command ownership and keep frontend behavior thin.
- Active delete promotion is tested separately from non-active and only-workbench deletes.
- Delete UX explicitly confirms that repos on disk are not removed.
- Partial config handling is consistent with the existing MenuBar defensive pattern.
- Creation now happens only in the same managed surface as review/rename/delete, avoiding browser prompt UI, keeping active-workbench context visible, closing management after success, and opening Dashboard.
- The menu integration test covers a post-create config refresh where the new workbench and prior workbenches remain switchable.

## Security Watch

- No auth, tenant, network, secrets, process launch, or public API surface changed.
- Delete is destructive only for the workbench config entry. Existing backend `delete_workbench` clears `active` if needed and persists the config; it does not touch repo paths on disk.
- No separate Security Sentinel gate required for prototype posture.

## Verification

- `python3 /home/teb/.agents/skills/krt-compound-master/scripts/check_work_package.py docs/work-packages/RDM-014-workbench-management-completion/2026-06-26-014-workbench-management-completion-work-package.md`: passed.
- `npm test -- src/workbench/recentWorkbenches.test.ts src/workbench/workbench.test.tsx src/workbench/ManageWorkbenchesDialog.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts`: 130 passed.
- Focused `npx prettier --check ...`: passed after mechanical formatting.
- `npx tsc --noEmit`: passed.
- `npm run build`: passed with existing Vite large-chunk warning.

## Result

Review passed. No remaining P0-P2 findings.
