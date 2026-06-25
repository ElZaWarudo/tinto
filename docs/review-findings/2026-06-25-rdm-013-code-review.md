---
title: RDM-013 Console Dock Code Review
status: passed
date: 2026-06-25
review_unit: RU1
threshold: P0-P2
---

# RDM-013 Console Dock Code Review

## Scope

- Reviewed frontend workspace changes for grouping agent console sessions under a level-1 Consoles tab with a nested Dockview.
- Reviewed tests for app registration, top-level opener behavior, nested console dock queueing, duplicate focus, layout restore, and existing terminal lifecycle regression coverage.
- Confirmed no backend, Tauri command, PTY, WSL routing, auth, persistence schema, or public contract code was changed by this review unit.

## Findings

- P0: none.
- P1: none.
- P2: none.

## Notes

- The first implementation pass would have lost nested console split layout across reload because top-level Dockview no longer owns terminal panels directly. That was fixed before final review by adding `localStorage` persistence for the nested console dock layout, matching the project file-dock pattern.
- Browser automation was unavailable because the in-app browser target `iab` was not exposed in this runtime. Vite was started successfully at `http://127.0.0.1:1420` for manual inspection.

## Verification

- `npm test -- src/workspace/openAgentTerminal.test.ts src/workspace/consoleDock.test.ts src/App.test.tsx src/panels/terminal/TerminalPanel.test.tsx`: 30 passed.
- `npx prettier --check src/App.tsx src/App.test.tsx src/App.css src/workspace/panels.ts src/workspace/openAgentTerminal.ts src/workspace/openAgentTerminal.test.ts src/workspace/consoleDock.ts src/workspace/consoleDock.test.ts src/panels/terminal/ConsoleDockPanel.tsx docs/brainstorms/2026-06-25-013-console-dock.md docs/plans/2026-06-25-013-console-dock-plan.md docs/work-packages/RDM-013-console-dock/2026-06-25-013-console-dock-work-package.md`: passed.
- `npx tsc --noEmit`: passed.
- `npm run build`: passed with the existing Vite chunk-size warning.
- `git diff --check`: passed with Windows CRLF conversion warnings only.
