---
title: "feat: Add repo agent launcher"
date: 2026-06-20
origin: docs/brainstorms/2026-06-20-002-agent-launcher-ui-requirements.md
status: accepted
---

# feat: Add repo agent launcher

## Summary

Add a repo-card launcher for ACI-003. The backend first exposes a read-only binary availability command over the existing allowlist. The frontend then mirrors that command, adds compact controls to `RepoCard`, starts sessions through `startAgentSession`, and opens the ACI-002 terminal panel through the stable dock helper.

## Units

### U1. Binary availability contract

- **Goal:** Let the UI ask whether a supported agent binary is available before launch.
- **Files:** `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/bus/contract.test.ts`, `docs/contracts/bus-contract.md`.
- **Approach:** Add `agent_binary_available(agent_type) -> bool`, returning `false` for known missing binaries and structured errors for unsupported ids. Mirror with a typed TS wrapper.
- **Tests:** Rust command/validation tests and TS invoke-shape test.

### U2. Repo card launch controls

- **Goal:** Start a selected agent session from a repo card and open the terminal panel.
- **Files:** `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/DashboardPanel.tsx`, `src/workspace/actions.tsx`, `src/App.tsx`, `src/App.css`.
- **Approach:** Add a compact select + launch button inside the repo card. Availability is checked when selection changes. Dashboard wires launch to `startAgentSession` and `openAgentTerminalPanel`.
- **Tests:** Missing binary disables launch; valid launch calls once and opens terminal; controls do not bubble to card open.

## Verification

- RU1: `cargo fmt --check`, `cargo test agent_console --lib`, `npx vitest run src/bus/contract.test.ts`.
- RU2: `npx vitest run src/panels/RepoCard.test.tsx src/panels/DashboardPanel.test.tsx src/App.test.tsx`, `npm run lint`, `npm run build`.

## Non-Goals

- No checkpoint/revert behavior.
- No persistent session store.
- No multi-agent auto-split policy.
- No custom command argument editing.
