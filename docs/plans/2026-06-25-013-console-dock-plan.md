---
title: "feat: Group console tabs in a nested dock"
date: 2026-06-25
origin: docs/brainstorms/2026-06-25-013-console-dock.md
status: accepted
---

# feat: Group console tabs in a nested dock

## Summary

Move agent terminal panels from the top-level Dockview into a dedicated Consoles level-1 tab. The Consoles tab owns a nested Dockview, mirroring the project file-dock pattern, so session tabs can be split/dragged without crowding Dashboard or project tabs.

## Units

### U1. Console dock registry and level-1 container

- **Goal:** Add a `PANEL_AGENT_CONSOLES` level-1 panel and a `consoleDock` registry that can queue terminal opens until the nested dock is ready.
- **Files:** `src/workspace/panels.ts`, `src/workspace/consoleDock.ts`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/App.tsx`.
- **Approach:** Keep `PANEL_AGENT_TERMINAL` as the nested terminal component. Add/open `PANEL_AGENT_CONSOLES` on top-level workspace; register its nested Dockview with the registry; open/focus session panels inside that nested API.
- **Tests:** Unit-test stable ids, queuing, duplicate focus, and top-level panel creation.

### U2. Layout and registration polish

- **Goal:** Keep the Consoles surface visually consistent and preserve existing terminal behavior.
- **Files:** `src/App.css`, `src/App.test.tsx`, `src/workspace/openAgentTerminal.test.ts`.
- **Approach:** Add minimal CSS for the Consoles container. Update app registration tests so both the container and nested terminal component remain registered.
- **Tests:** Targeted Vitest for app registration and console dock helpers; existing terminal tests remain the lifecycle/input guard.

## Verification

- `npm test -- src/workspace/openAgentTerminal.test.ts src/workspace/consoleDock.test.ts src/App.test.tsx src/panels/terminal/TerminalPanel.test.tsx`
- `npx tsc --noEmit`
- `npm run build`
- `git diff --check`

## Non-Goals

- No Tauri command, backend PTY, WSL routing, or persisted UI schema change beyond Dockview's existing saved layout content.
