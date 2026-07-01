---
title: Passive signals and lightweight metrics
status: completed
roadmap_item: RDM-011
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-15-rdm-011-passive-signals-requirements.md
origin_planning_input: docs/brainstorms/2026-06-15-rdm-011-passive-signals-requirements.md
origin_plan: docs/plans/2026-06-15-005-feat-passive-signals-plan.md
units: [U1, U2, U3, U4, U5]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Passive Signals and Lightweight Metrics

## Scope

Implement deterministic passive signals and lightweight metrics across Tinto's existing workbench surfaces. The backend computes bounded signal facts from current repo status, worktree diffs, and Plane 2 events; the frontend renders compact indicators in existing panels.

## Non-goals

- Notifications, filters/search, glance mode, or user-configurable signal rules.
- Durable metrics/event storage.
- Natural-language summaries, AI scoring, or advice.
- Any git write operation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: exact CSS, chip labels, severity names, deterministic rule thresholds, and small helper/component names.
- Agent must record as assumptions: payload caps, large-delete threshold, secret/config/test pattern set, and any performance tradeoff in the bus recompute path.
- Agent must escalate: non-additive contract changes, new commands/events, file-content reads outside existing diff/blob paths, notifications, persistence, configurable rules, git writes, branch/base strategy, Jira/PR workflow, or destructive operations.
- Safe fallback: if backend metrics prove too costly or require a non-additive contract change, stop and record the gap instead of silently narrowing the feature.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-006, RDM-007, RDM-008, RDM-009, and RDM-010 merged to `develop`.
- Blocks: RDM-012 can consume these signals for filtering/search/notifications later.

## Production Posture

- Posture: prototype.
- Evidence: Tinto remains a local desktop prototype with no live deployment.
- Confidence: high.
- Consequences for this package: local iteration is acceptable; read-only behavior and no-secret-leak copy remain non-negotiable.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Contract fields are required before backend/frontend work. |
| U2 | yes | Backend computation owns the canonical signal facts. |
| U3 | yes | Frontend types/selectors must consume the additive fields safely. |
| U4 | yes | Existing views are the user-facing value of the signals. |
| U5 | yes | Verification and state closeout are part of the review unit. |

Grouping rationale:

- Single RU. Contract, backend computation, TS store helpers, and UI rendering are tightly coupled around the same additive fields. Splitting by layer would make early PRs hard to verify and later PRs repeat the same review context. Broadness is managed through separate commits and focused tests.

## Implementation Units

- U1. Contract types and backend signal model.
- U2. Backend metrics and detection rules.
- U3. Frontend contract/store helpers.
- U4. UI presentation across existing views.
- U5. Verification and closeout.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Passive signals and metrics end to end | Rust bus contract/computation/tests, TS contract/store/helpers/tests, existing panel UI/tests, docs | develop | optional standalone Tarea | Broad but one coherent additive contract feature; likely >900 human lines because it touches backend + multiple UI surfaces |

## Reviewability Diagnosis

- Reviewer-experience check: yes. The PR can be reviewed in commits: contract, backend rules, frontend selectors, UI surfaces, docs.
- Granularity chosen because: one independently useful capability and one shared contract extension; layer-splitting would not produce independently useful review units.
- Open-stack plan: independent PR, depth 0 of max 3.
- Jira mapping: single-review-unit PR maps to one standalone Tarea if Jira is available.
- Downstream-fix trace: none.
- Failure-mode check: not a deep micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests

- Likely backend files: `src-tauri/src/bus/contract.rs`, `src-tauri/src/bus/mod.rs`, backend bus tests.
- Likely frontend files: `src/bus/contract.ts`, `src/bus/store.ts`, `src/bus/store.test.ts`, `src/bus/contract.test.ts`, `src/panels/RepoCard.tsx`, `src/panels/RepoPanel.tsx`, `src/panels/RepoTreePanel.tsx`, `src/panels/diff/DiffPanel.tsx`, `src/panels/WatchedFilesSection.tsx`, `src/panels/timeline/model.ts`, `src/App.css`, and related tests.
- Docs: this package, the origin brainstorm, the plan, `docs/contracts/bus-contract.md`, and `docs/orchestration/compound-master-state.md`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive `RepoDelta.metrics`, `RepoDelta.signals`, and `FsEvent.signals` fields in the bus contract.
- Consumer scan patterns: `rg "RepoDelta|FsEvent|signals|metrics|subscribed_diffs|statusSummary|getFsEvents" src src-tauri/src docs/contracts`.
- Consumers found: bus Rust contract/task, TS contract/store, Dashboard/RepoCard, RepoPanel, RepoTreePanel, DiffPanel, WatchedFilesSection, Timeline model/panel.
- Contract-drift tests searched: Rust serialization test, TS contract test, store tests, affected panel tests.
- Required consumer tests: Rust bus tests for signal computation; frontend store/helper tests; panel tests for signal/metric rendering; full Vitest and cargo test.
- Consumer tests run/skipped: focused Vitest passed 73/73 for bus contract/store and affected panels; full Vitest passed 132/132. Rust bus tests passed 28/28; full cargo tests passed 117/117.

## Verification Gate

- Required: `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`.
- Surface-aware evidence: contract serialization tests, backend rule tests, store selector tests, affected panel tests.
- Production posture evidence: prototype; local Linux build/smoke sufficient unless a new OS capability is introduced.
- Results (2026-06-15): focused Vitest 73/73; full `npm test` 132/132; `npm run lint`; `npm run format:check`; `npm run build`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test` 117/117; `cargo build`; `npm run tauri build` with deb/rpm/AppImage output. `rtk timeout 25s npm run tauri dev` launched Vite and the Tauri binary, then exited by timeout as expected; EGL/MESA warnings were environment-only.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Result: inline review pass. Main reviewed risk was unconditional `worktree_diff()` for metrics; existing git implementation handles unborn HEAD by diffing against no tree, so no behavior regression was found. The payload cap sorting was corrected so large-delete signals are not truncated behind lower-value path warnings.

## Security Gate

- Run after work-review loop: required if implementation reads raw file content outside existing git diff/blob mechanisms or introduces notification/persistence/configurable rules; otherwise adversarial review in the normal code-review loop is sufficient.
- Security Watch during work: enabled informally for secret-leak risk in copy/payloads.
- Security Watch notes: signal messages must never include raw matched line values.
- Security reviewer: inline/adversarial fallback; escalate to `krt-security-sentinel` if new high-risk surface appears.
- Security review result: pass inline. No git writes, no new Tauri capability, no new file-content reads outside existing git diff data, and signal messages are tested not to expose matched secret values.
- Required security verification: prove no git writes, no new Tauri capability, no secret value in signal messages/tests.

## Execution Result

- Added additive Rust/TS bus contract fields for `RepoMetrics`, `PassiveSignal`, signal kind, and severity.
- Added backend read-only rule computation for changed-file/line metrics, sensitive paths, possible secret markers, large deletions, config changes, test changes, and Plane 2 event signals.
- Added frontend store helpers for metrics, repo signals, path signals, deterministic ordering, and severity counts.
- Added reusable signal chips/metric pills and rendered them across repo cards, repo panels, repo tree, diff panels, watched-file rows, and timeline activity details.
- Updated `docs/contracts/bus-contract.md` with the additive fields.
- Kept the feature local/session-only; no notifications, filters, persistence, configurable rules, git writes, or new Tauri capabilities.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust tests/clippy, TS tests/typecheck/lint/build, bus contract drift, broad UI snapshots/queries.
- Preventive evidence: full local verification before Release Marshal handoff.
- If CI breaks: invoke `krt-ci-questor` with PR/check context.
- Escalation rule: keep release-follow-up blocker until cause and owner are recorded.

## Branch and PR Handoff Inputs

- Review unit: RU1 - Passive signals and lightweight metrics.
- Branch name: `feat/passive-signals`.
- Branch/docs rule: this review unit carries related RDM-011 planning artifacts on the same semantic branch.
- PR base: develop.
- Suggested commit grouping:
  - `feat(signals): extend bus contract with passive facts` - Rust/TS contract types, docs contract, serialization tests.
  - `feat(signals): compute passive metrics in the bus` - backend rule engine, metrics/signals computation, Rust tests.
  - `feat(ui): render passive signal indicators` - frontend selectors/components/panel rendering/tests/CSS.
  - `docs(orchestration): capture passive signals delivery` - brainstorm, plan, package, state.
- PR title: `Add passive signals and lightweight repo metrics`
- PR body bullets:
  - Adds passive signal facts and lightweight metrics to the workbench bus contract.
  - Highlights sensitive files, possible secrets, large deletions, config changes, and test changes across existing views.
  - Keeps signal copy deterministic and read-only without exposing matched secret values.
- Verification results location: this package and release thread.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea.
- Jira summary: "Senales pasivas y metricas ligeras"
- Jira description: "Anadir senales pasivas deterministicas y metricas ligeras en Tinto para destacar cambios sensibles, eliminaciones grandes, configuracion y tests sin resumir ni juzgar el contenido."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
