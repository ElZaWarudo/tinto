---
title: ACI-003 Repo Agent Launcher UI
status: delivered
roadmap_item: ACI-003
origin_roadmap: docs/roadmaps/2026-06-19-002-agent-console-integration.md
origin_brainstorm: docs/brainstorms/2026-06-20-002-agent-launcher-ui-requirements.md
origin_plan: docs/plans/2026-06-20-002-feat-agent-launcher-ui-plan.md
units: [U1, U2]
unit_alignment: complete
review_units: [RU1, RU2]
base_branch: develop
pr_strategy: stacked
max_open_stack: 2
jira_policy: optional
production_posture: prototype
autonomy: guarded
allowed_mutation_classes: []
---

# ACI-003 Repo Agent Launcher UI

## Scope

- Add `agent_binary_available(agent_type)` over the existing agent allowlist.
- Mirror the command in the frontend bus client.
- Add compact repo-card controls for selecting and launching an agent.
- On successful launch, open/focus `PANEL_AGENT_TERMINAL` for the returned session id.

## Non-goals

- Checkpoints, revert, resource limits, persistent session history, global preferences, or custom command arguments.
- Multi-agent auto-splitting decisions; ACI-005 owns layout policy.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1 binary availability backend/TS contract | agent_console commands, lib registration, bus client/tests, contract docs | develop | new | Low/medium: additive command over process-binary allowlist |
| RU2 | U2 repo card launcher and terminal opener wiring | RepoCard/Dashboard/App/actions/CSS/tests | develop after RU1 | new | Medium: card interaction and launch side effects |

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri command `agent_binary_available`, frontend wrapper, repo-card launch controls, workspace action wiring.
- Consumer scan patterns: `agentBinaryAvailable`, `agent_binary_available`, `startAgentSession`, `openAgentTerminalPanel`, `PANEL_AGENT_TERMINAL`.
- Required consumer tests: contract wrapper test, command tests, RepoCard launch tests, Dashboard launch wiring tests, App action/registration tests.

## Gates

- Verification: `cargo fmt --check`, `cargo test agent_console --lib`, targeted Vitest, `npm run lint`, `npm run build`.
- Review: direct Compound Master fallback review, P0-P2 threshold.
- Security: verify availability uses allowlist/PATH lookup only, unsupported ids fail closed, missing binaries do not spawn, launch still relies on active-workbench repo allowlist.

## Branch and PR Handoff Inputs

- Review unit: RU1 binary availability contract.
- Branch name: `feat/agent-launcher-ui`
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop
- Suggested commit grouping:
  - `feat(agent-console): expose agent binary availability`
  - `feat(repo-card): launch agent sessions from repo cards`
  - `docs(orchestration): add agent launcher package state`
- PR title: Add repo agent launcher
- PR body bullets:
  - Add binary availability validation for supported agents.
  - Add repo-card launch controls and terminal opener wiring.
  - Document ACI-003 package state and verification.
- Verification results location: this work package Execution Status section and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: prototype-local commands and UI only.
- Autonomous mutation request: none

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Jira summary: Anadir launcher de agentes por repo
- Jira description: Permitir seleccionar un agente soportado desde una repo card, validar su binario local y abrir la terminal de sesion al lanzarlo.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.

## Execution Status

- Status: complete locally on `feat/agent-launcher-ui`, pending release marshal.
- Changed surfaces: `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/lib.rs`, `docs/contracts/bus-contract.md`, `src/bus/client.ts`, `src/bus/contract.test.ts`, `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/DashboardPanel.tsx`, `src/panels/DashboardPanel.test.tsx`, `src/workspace/actions.tsx`, `src/App.tsx`, `src/App.css`, plus existing action fixture tests.
- RU1 notes: added `agent_binary_available(agent_type)` using the existing allowlist/PATH resolver. Known missing binaries return `false`; unsupported ids return `unsupported_agent`. Registered the command and mirrored it with `agentBinaryAvailable`.
- RU2 notes: added repo-card select/launch controls, availability checks, disabled missing-binary state, launch error state, event propagation guards, dashboard launch wiring to `startAgentSession`, and terminal opening through `openAgentTerminal`.
- Direct review result: no P0-P2 findings. One React lint issue around synchronous effect state reset was fixed by moving the reset to the select-change handler. Test mocks were typed with explicit rest signatures to satisfy TS build.
- Security result: passed direct watch. Availability cannot bypass the backend allowlist; missing binaries do not spawn; actual launch still goes through `start_agent_session`, which validates active-workbench repo membership before creating a PTY.
- Verification: `cargo fmt --check` passed; `cargo test agent_console --lib` passed 23/23; `cargo clippy --all-targets -- -D warnings` passed; `npx vitest run src/bus/contract.test.ts` passed 15/15; `npx vitest run src/panels/RepoCard.test.tsx src/panels/DashboardPanel.test.tsx src/App.test.tsx src/workbench/workbench.test.tsx src/bus/contract.test.ts` passed 46/46; `npm run build` passed with existing Vite chunk-size warning; `npm run lint` passed.
- Release marshal status: pending local semantic commits, fast-forward merge to `develop`, and push.
