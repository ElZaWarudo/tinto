---
title: Console dock tabs
status: review-passed
roadmap_item: RDM-013
origin_roadmap: docs/roadmaps/2026-06-22-003-post-closeout-ux.md
origin_brainstorm: docs/brainstorms/2026-06-25-013-console-dock.md
origin_planning_input: docs/brainstorms/2026-06-25-013-console-dock.md
origin_plan: docs/plans/2026-06-25-013-console-dock-plan.md
units: [U1, U2]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Console dock tabs

## Scope

Group agent console sessions under a single top-level Consoles workspace tab and open individual terminal sessions as nested Dockview tabs inside it.

## Non-goals

- External OS-window popouts.
- Backend session, PTY, WSL, or command-contract changes.
- New agent-launch choices or persistence outside existing Dockview layout persistence.

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: internal helper names, equivalent focused tests, CSS class names matching existing conventions, and small test fixture updates.
- Agent must record as assumptions: any interpretation of "sacar tabs" that stays within current Dockview behavior.
- Agent must escalate: true OS-window detach, backend contract changes, branch/base strategy, Jira/PR workflow, or scope outside console tab grouping.
- Safe fallback: implement the nested in-app dock and report true OS-window detach as deferred if requested later.
- Autonomous ledger: none
- Allowed external mutation classes: none

## Dependencies

- Requires: existing ACI terminal panel/session store and project file-dock pattern.
- Blocks: none.

## Production Posture

- Posture: prototype
- Evidence: current orchestration state describes Tinto as prototype-local desktop work.
- Confidence: high
- Consequences for this package: frontend layout changes may be iterative, but existing terminal behavior must remain compatible.
- Breaking existing behavior allowed: no, unless explicitly approved.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Core level-1 console container and nested dock behavior. |
| U2 | yes | Required polish and tests for the user-visible frontend change. |

Grouping rationale:
- U1 and U2 touch the same frontend workspace surfaces and are easier to review as one cohesive console layout change.

## Implementation Units

- U1: Console dock registry and level-1 container.
- U2: Layout and registration polish.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Console tabs grouped in a nested Consoles dock | `src/workspace/*`, `src/panels/terminal/*`, `src/App.tsx`, `src/App.css`, targeted tests, orchestration docs | develop | optional Tarea | Low/medium frontend layout risk; no backend contract changes. |

## Files and Tests

- Expected files: `src/workspace/panels.ts`, `src/workspace/openAgentTerminal.ts`, `src/workspace/consoleDock.ts`, `src/workspace/consoleDock.test.ts`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/App.tsx`, `src/App.css`, `src/App.test.tsx`, `src/workspace/openAgentTerminal.test.ts`.
- Expected tests: `npm test -- src/workspace/openAgentTerminal.test.ts src/workspace/consoleDock.test.ts src/App.test.tsx src/panels/terminal/TerminalPanel.test.tsx`, `npx tsc --noEmit`, `npm run build`, `git diff --check`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: frontend workspace helper behavior only; no public Tauri or backend contract changes expected.
- Consumer scan patterns: `PANEL_AGENT_TERMINAL`, `openAgentTerminalPanel`, `agentTerminalPanelId`, `sessionIdFromAgentTerminalPanelId`, `DockWorkspace`, `TerminalPanel`.
- Consumers found: `src/App.tsx`, `src/App.test.tsx`, `src/workspace/openAgentTerminal.ts`, `src/workspace/openAgentTerminal.test.ts`, `src/workspace/panels.ts`, `src/workspace/consoleDock.ts`, `src/workspace/consoleDock.test.ts`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/panels/terminal/TerminalPanel.tsx`, and `src/panels/terminal/TerminalPanel.test.tsx`.
- Contract-drift tests searched: frontend registration/helper/terminal tests.
- Required consumer tests: console dock helper tests, app registration tests, terminal panel regression tests.
- Consumer tests run/skipped: targeted Vitest passed 30/30; typecheck/build passed; browser automation skipped because the in-app browser target `iab` was unavailable.

## Verification Gate

- `npm test -- src/workspace/openAgentTerminal.test.ts src/workspace/consoleDock.test.ts src/App.test.tsx src/panels/terminal/TerminalPanel.test.tsx`
- `npx tsc --noEmit`
- `npm run build`
- `git diff --check`
- Surface-aware evidence: workspace/tab behavior by helper tests; terminal lifecycle by existing terminal panel tests; build/typecheck by repo scripts.
- Production posture evidence: prototype frontend-only change; no migration or deployment behavior.

## Review Gate

- Code review threshold: P0-P2
- Findings below threshold: log unless user marks blocking.

## Security Gate

- Run after work-review loop: not required because no auth, secrets, PII, public API, deployment, dependency, or backend execution surface changes are planned.
- Security Watch during work: disabled; verify no backend/session command behavior changes.
- Security Watch notes: none.
- Security reviewer: inline fallback if diff unexpectedly touches backend/security-sensitive surfaces.
- Security review result: passed by inline fallback; diff is frontend-only for the review unit.
- Required security verification: inspect diff for frontend-only scope.

## CI Break-Prevention And Escalation

- CI risk surfaces: TypeScript, Vitest, Vite build, formatting/diff whitespace.
- Preventive evidence: targeted tests, typecheck, build, and `git diff --check`.
- If CI breaks: invoke krt-ci-questor with run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1 Console tabs grouped in a nested Consoles dock.
- Branch name: feat/console-dock-tabs
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch; do not ship a separate docs-planning branch unless explicitly requested.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `feat(console): group terminal tabs in a console dock` - workspace helpers, console container, app registration, CSS, and tests.
  - `docs(orchestration): record console dock delivery` - brainstorm, plan, package, state, and review notes.
- PR title: Group terminal tabs in a console dock
- PR body bullets:
  - Add a Consoles top-level workspace tab for agent sessions.
  - Open terminal sessions as draggable/splittable tabs inside the console dock.
  - Preserve existing terminal lifecycle, input, output, resize, and revert behavior.
- Verification results location: this work package Execution Status and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: prototype-local frontend layout change only.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Jira summary: Agrupar terminales de agente en una pestana de consolas
- Jira description: Anadir una pestana de nivel superior para consolas y abrir cada sesion de agente como una tab interna arrastrable, sin cambiar el backend de sesiones.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.

## Execution Status

- Status: RU1 implementation complete and review-passed.
- Changed surfaces: `src/workspace/panels.ts`, `src/workspace/openAgentTerminal.ts`, `src/workspace/consoleDock.ts`, `src/workspace/consoleDock.test.ts`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/App.tsx`, `src/App.css`, `src/App.test.tsx`, and `src/workspace/openAgentTerminal.test.ts`.
- Implementation notes: `openAgentTerminalPanel` now opens/focuses a level-1 `PANEL_AGENT_CONSOLES` panel and delegates session tabs to `consoleDock`. `ConsoleDockPanel` hosts a nested Dockview with `TerminalPanel` as the nested component. `consoleDock` queues opens before mount, deduplicates session ids, focuses existing terminal tabs, and persists/restores the nested layout under `tinto:console-dock`.
- Verification: work-package checker passed; targeted Vitest 30/30 passed; Prettier focused check passed; `npx tsc --noEmit` passed; `npm run build` passed with the existing chunk-size warning; `git diff --check` passed with CRLF warnings only.
- Review result: passed; findings path `docs/review-findings/2026-06-25-rdm-013-code-review.md`.
- Security result: passed by inline fallback; no backend/session command, WSL routing, auth, secrets, PII, public API, deployment, or dependency surface changed by this review unit.
- Visual/server smoke: Vite dev server is running at `http://127.0.0.1:1420`; `Invoke-WebRequest` returned HTTP 200. Automated in-app browser inspection skipped because browser target `iab` was unavailable.
