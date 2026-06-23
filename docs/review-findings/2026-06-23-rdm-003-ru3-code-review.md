---
title: RDM-003 RU3 code review
status: passed
roadmap_item: RDM-003
review_unit: RU3
work_package: docs/work-packages/RDM-003-windows-wsl-workbench-path-ux/2026-06-23-003-wsl-workbench-path-ux-work-package.md
review_date: 2026-06-23
review_type: code
threshold: P0-P2
---

# RDM-003 RU3 Code Review

## Result

Passed. No remaining P0-P2 findings.

## Reviewed Scope

- `src/workbench/AddWslRepoDialog.tsx` adds the Windows-only add WSL repo dialog for Ubuntu and Linux path input.
- `src/workbench/MenuBar.tsx` gates "Add WSL repo..." and configured WSL labels behind `isWindowsHost()`.
- `src/workbench/operations.ts` validates/normalizes WSL Linux paths before calling the isolated client wrapper.
- `src/workbench/workbench.test.tsx` covers Windows-only menu/dialog behavior and configured WSL labels that do not create project entries.
- `src/workbench/operations.test.ts` covers WSL path normalization and backend wrapper invocation.

## Findings

None at P0-P2.

## Impact Scan

Changed visible surfaces:
- Repos menu gains a Windows-only "Add WSL repo..." action.
- Windows-only dialog accepts fixed `Ubuntu`, Linux path, and alias.
- Windows-only configured WSL labels can appear in the Repos menu.

Unchanged live surfaces:
- Project menu still derives from live bus snapshot, so configured WSL repos do not open project tabs before RDM-004.
- Dashboard/repo cards/file explorers/diff/media/file operations/Gitleaks/agent console are not wired to WSL config entries.
- Non-Windows runtime hides WSL controls through the platform gate.

## Verification

- `npm test -- src/workbench/workbench.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts src/bus/contract.test.ts`: 109 passed.
- `npx tsc --noEmit`: passed.
- `npx prettier --check src\workbench\AddWslRepoDialog.tsx src\workbench\MenuBar.tsx src\workbench\operations.ts src\workbench\workbench.test.tsx src\workbench\operations.test.ts src\workbench\wslAbsence.test.ts src\bus\client.ts src\bus\contract.ts src\bus\contract.test.ts`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.

## Residual Risk

Manual Windows smoke is still useful before final release: set Windows host, add `Ubuntu + /home/...`, confirm the entry appears as configured and remains absent from live project lists until RDM-004.
