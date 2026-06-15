---
title: Timeline and commit history panel
status: implemented-verified-awaiting-release
roadmap_item: RDM-010
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-15-rdm-010-timeline-history-requirements.md
origin_planning_input: docs/brainstorms/2026-06-15-rdm-010-timeline-history-requirements.md
origin_plan: docs/plans/2026-06-15-004-feat-timeline-history-plan.md
units: [U1, U2, U3, U4, U5]
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

# Timeline and Commit History Panel

## Scope

Implement the first Timeline / history surface for Tinto: a dockable workbench-level panel with cross-repo activity, recent commit history, commit diff inspection, and frontend orphan detection for dirty quiet repos.

## Non-goals

- SQLite or durable historical event storage.
- Filters/search/date ranges, native notifications, or glance mode.
- Passive signals, severity, secret detection, metrics, or line-count analytics.
- Natural-language summaries.
- Any git write operation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: component names, CSS details, feed row copy, initial commit-log limit, initial orphan threshold constant, and whether blob support is implemented now or deferred.
- Agent must record as assumptions: chosen orphan threshold, commit-log limit, any backend command argument-shape discovery, and any UI state copy that differs from the requirements.
- Agent must escalate: new backend command/contract fields, durable persistence, git writes, notification behavior, passive signal scoring, filters/search scope, branch/base strategy, Jira/PR workflow, or destructive operations.
- Safe fallback: if existing history commands do not support the UI shape, stop and record the contract gap instead of modifying the backend implicitly.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-002, RDM-006, RDM-007, and RDM-008 merged to `develop`.
- Blocks: RDM-012 benefits from the feed surface for future filtering/search.

## Production Posture

- Posture: prototype.
- Evidence: Tinto remains a greenfield desktop app with local verification and no live deployment.
- Confidence: high.
- Consequences for this package: frontend iteration is acceptable, but read-only behavior remains non-negotiable.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Feed model is required before rendering Timeline. |
| U2 | yes | Commit navigation needs TS wrappers over existing backend commands. |
| U3 | yes | The panel is the user-visible capability. |
| U4 | yes | Registration/opening makes the panel reachable and restorable. |
| U5 | yes | Verification and state closeout are part of the review unit. |

Grouping rationale:

- Single RU. The feed model, client wrappers, panel rendering, and workspace registration form one coherent user-visible capability. Splitting would create a foundation PR with limited standalone value and extra review overhead.

## Implementation Units

- U1. Add timeline model selectors.
- U2. Add commit history client support.
- U3. Build TimelinePanel.
- U4. Register and open the Timeline panel.
- U5. Verify and update orchestration state.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Full Timeline/history panel | frontend model/client/panel/workspace wiring, tests, CSS, docs | develop | optional standalone Tarea | Medium integrated UI slice; backend should remain unchanged |

## Reviewability Diagnosis

- Reviewer-experience check: yes. A reviewer can read the model/client first, then the panel, then workspace wiring and tests.
- Granularity chosen because: one independently useful UI capability with tight integration across the shell and history commands.
- Open-stack plan: independent PR, depth 0 of max 3.
- Jira mapping: single-review-unit PR maps to one standalone Tarea if Jira is available.
- Downstream-fix trace: none.
- Failure-mode check: not a micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests

- Likely frontend files: `src/panels/timeline/model.ts`, `src/panels/timeline/model.test.ts`, `src/panels/timeline/TimelinePanel.tsx`, `src/panels/timeline/TimelinePanel.test.tsx`, `src/bus/client.ts`, `src/bus/contract.test.ts`, `src/workspace/panels.ts`, `src/workspace/openTimeline.ts`, `src/workspace/openTimeline.test.ts`, `src/workspace/actions.tsx`, `src/workbench/TopBar.tsx`, `src/App.tsx`, `src/App.test.tsx`, `src/App.css`.
- Docs: this package, the origin brainstorm, the plan, and `docs/orchestration/compound-master-state.md`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: TS wrappers over existing `get_commit_diff` and possibly `get_blob`; no Rust contract change planned.
- Consumer scan patterns: `rg "get_commit_diff|get_commit_log|get_blob|PANEL_DIFF|PANEL_TREE|DiffView" src src-tauri/src docs/contracts`.
- Consumers found: `src/bus/client.ts`, `src/panels/RepoPanel.tsx`, `src/panels/diff/DiffPanel.tsx`, `src/panels/diff/DiffView.tsx`, `src/workspace/panels.ts`, backend `bus/commands.rs`.
- Contract-drift tests searched: client wrapper invoke-shape tests and App panel registration tests.
- Required consumer tests: timeline model tests, TimelinePanel tests, client wrapper tests, App/TopBar/openTimeline tests, full Vitest.
- Consumer tests run/skipped: focused Vitest passed 42/42 for timeline model/panel, client wrappers, App/TopBar/openTimeline, and affected workspace-action consumers.

## Verification Gate

- Required: `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`.
- Surface-aware evidence: model tests for ordering/orphans; client tests for command wrappers; panel tests for commit selection/diff/error/degraded states; workspace tests for opener and registration.
- Production posture evidence: prototype; Linux build/smoke sufficient for this branch.
- Results (2026-06-15): focused Vitest 42/42; full `npm test` 126/126; `npm run lint`; `npm run format:check`; `npm run build`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test` 114/114; `cargo build`; `npm run tauri build` with deb/rpm/AppImage output. `rtk timeout 25s npm run tauri dev` launched Vite and the Tauri binary, then exited by timeout as expected; EGL warnings were environment-only.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Result: inline review pass after fixing one P2 stale-state issue. Timeline now filters commits and selected commit detail against the active repo set so a workbench switch plus failed log reload cannot leave stale commits from a previous workbench visible.

## Security Gate

- Run after work-review loop: not required unless execution adds file-content display from historical blobs, new backend commands, persistence, notifications, or capability changes.
- Security Watch during work: disabled initially; read-only local UI over already-allowlisted commands.
- Security Watch notes: commit diffs can display repository content already available through the existing allowlist. No watched-file content or secrets-specific scanning is added.
- Security reviewer: fallback inline/adversarial unless new high-risk surface appears.
- Security review result: not required; no Rust contract/capability change, no git writes, and no new persistence.
- Required security verification: confirm no git writes and no new Tauri capability.

## Execution Result

- Added pure timeline model selectors for activity, Plane 2 events, degraded/error entries, and orphan candidates with deterministic threshold tests.
- Added TS wrappers for existing `get_commit_diff` and `get_blob` commands with invoke-shape tests.
- Added a dockable Timeline panel that combines live activity with recent commit history and renders selected commit file diffs through the existing `DiffView`.
- Added workspace opener/action, TopBar entry point, App registration, CSS, and regression tests.
- Kept the feature frontend/session-only; no SQLite, filters, notifications, passive signals, natural-language summaries, Tauri capability changes, or git writes.

## CI Break-Prevention And Escalation

- CI risk surfaces: frontend tests/typecheck/lint/build; Rust compile if a command shape mismatch forces backend edits.
- Preventive evidence: full local gate before Release Marshal handoff.
- If CI breaks: invoke `krt-ci-questor` with PR/check context.
- Escalation rule: keep release-follow-up blocker until cause and owner are recorded.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Timeline and commit history panel.
- Branch name: `feat/timeline-history`.
- Branch/docs rule: this review unit carries related RDM-010 planning artifacts on the same branch.
- PR base: develop.
- Suggested commit grouping:
  - `feat(timeline): add activity feed model` — model/selectors/tests.
  - `feat(history): expose commit diff client wrappers` — TS wrappers and contract tests.
  - `feat(ui): add timeline history panel` — panel component, diff rendering, CSS/tests.
  - `feat(app): register timeline panel entry point` — workspace/action/TopBar/App wiring/tests.
  - `docs(orchestration): capture timeline delivery` — docs and state.
- PR title: `Timeline history panel for workbench activity`
- PR body bullets:
  - Adds a dockable timeline panel for cross-repo workbench activity.
  - Adds commit history navigation with commit diff inspection through existing read-only commands.
  - Flags dirty repos that have stayed quiet past the orphan threshold.
- Verification results location: Verification Gate of this package and release thread.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea.
- Jira summary: "Panel de timeline e historial de commits"
- Jira description: "Añadir en Tinto una vista timeline del workbench para revisar actividad reciente entre repositorios, navegar commits con sus diffs y detectar repos sucios que llevan tiempo sin commit."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
