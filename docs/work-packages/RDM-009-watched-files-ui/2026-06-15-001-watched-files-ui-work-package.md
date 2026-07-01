---
title: Watched files UI
status: completed
roadmap_item: RDM-009
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-15-rdm-009-watched-files-ui-requirements.md
origin_planning_input: docs/brainstorms/2026-06-15-rdm-009-watched-files-ui-requirements.md
origin_plan: docs/plans/2026-06-15-003-feat-watched-files-ui-plan.md
units: [U1, U2, U3, U4, U5, U6]
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

# Watched files UI

## Scope

Implement the RDM-009 Plane 2 UI in one integrated review unit: recent watched-file events in the repo detail panel, visible `fs_watch` patterns, a per-repo pattern editor, TS client support for the existing registered `update_repo`, and tests/styling. The planned implementation is frontend-only.

## Non-goals

- Passive highlights, severity scoring, metrics, or alerting (RDM-011).
- Timeline/history integration (RDM-010).
- Alias editing, global templates, pattern presets, or persisted Plane 2 event history.
- Reading, diffing, or mutating watched files.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: component names, per-repo event cap value, timestamp formatting, local validation copy, test fixture shapes, and whether the thin save helper lives in `operations.ts` or beside the component.
- Agent must record as assumptions: the chosen event cap, any backend command registration discovery, and any UI state copy that differs from the requirements.
- Agent must escalate: new backend contract fields, watcher semantics changes, reading Plane 2 file content, public behavior outside RDM-009, branch/base strategy, Jira/PR workflow, or destructive operations.
- Safe fallback: if the TS wrapper reveals an argument-shape mismatch, stop and align the wrapper to the existing Rust command without changing the Rust contract.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-004, RDM-006, RDM-007 merged to `develop`; current branch also includes RDM-008 because it is already merged.
- Blocks: RDM-011.

## Production Posture

- Posture: prototype.
- Evidence: same Tinto greenfield program; no live users.
- Confidence: high.
- Consequences for this package: speed and UI iteration are acceptable; keep read-only product posture.
- Breaking existing behavior allowed: no removal; additive UI and frontend state only.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Store state is required before the section can render events. |
| U2 | yes | Pattern editing needs the TS wrapper/save flow. |
| U3 | yes | The section is the user-visible event surface. |
| U4 | yes | Pattern editing is half of the roadmap outcome. |
| U5 | yes | RepoPanel integration binds state, config, and save flow. |
| U6 | yes | Full verification and artifact status close the RU. |

Grouping rationale:

- Single RU. Store, client wrapper, watched section, editor, and RepoPanel integration are tightly coupled around one visible capability and share the same review surface. Splitting would produce a foundation PR with no standalone user value. The work is expected to be medium-sized and test-heavy; commit grouping can still separate store/client/UI/docs for review.

## Implementation Units

- U1. Add bounded recent watched-file events to the bus store.
- U2. Add TS wrapper and operation support for `update_repo(... fs_watch ...)`.
- U3. Build the watched-files section for event display and states.
- U4. Add local pattern editor behavior.
- U5. Integrate the section into `RepoPanel`.
- U6. Run full verification and update release handoff state.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Full watched-files UI and pattern editor | frontend store/client/operations, repo panel UI, tests, CSS, docs | develop | optional standalone Tarea | Medium integrated UI slice; single PR is more reviewable than foundation-only stack |

## Reviewability Diagnosis

- Reviewer-experience check: yes. A reviewer can read the diff as store/client first, then component/editor, then RepoPanel integration and tests.
- Granularity chosen because: one cohesive Plane 2 UI capability, not because of Jira shape.
- Open-stack plan: independent PR, depth 0 of max 3.
- Jira mapping: single-review-unit PR maps to one standalone Tarea if Jira is available.
- Downstream-fix trace: none.
- Failure-mode check: not a micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests

- Likely frontend files: `src/bus/store.ts`, `src/bus/store.test.ts`, `src/bus/client.ts`, `src/workbench/operations.ts`, `src/panels/WatchedFilesSection.tsx`, `src/panels/WatchedFilesSection.test.tsx`, `src/panels/RepoPanel.tsx`, `src/panels/RepoPanel.test.tsx`, `src/App.css`.
- Docs: this package, the origin brainstorm, the plan, and `docs/orchestration/compound-master-state.md`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: frontend TS wrapper for existing registered `update_repo`; no new backend contract planned.
- Consumer scan patterns: `rg "update_repo|fs_watch|FsEventBatch|applyFsEvents" src src-tauri/src`.
- Consumers found: `src/bus/client.ts`, `src/bus/store.ts`, `src/workbench/operations.ts`, `src/panels/RepoPanel.tsx`, backend `workbench/commands.rs`.
- Contract-drift tests searched: client wrapper tests or component tests mocking `invoke`; Rust compile if command registration changes.
- Required consumer tests: store tests, watched section tests, RepoPanel tests, full Vitest.
- Consumer tests run/skipped: `npm test -- --run src/bus/store.test.ts src/bus/contract.test.ts src/panels/WatchedFilesSection.test.tsx src/panels/RepoPanel.test.tsx` passed 37/37.

## Verification Gate

- Required: `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`.
- Surface-aware evidence: event memory via store tests; pattern save via client/operation or component tests; UI states via watched section tests; RepoPanel integration via existing panel tests.
- Production posture evidence: prototype; Linux build/smoke sufficient for this branch.
- Results (2026-06-15): focused Vitest 37/37; full `npm test` 115/115; `npm run lint`; `npm run format:check`; `npm run build`; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test` 114/114; `cargo build`; `npm run tauri build` with deb/rpm/AppImage output. `npm run tauri dev` smoke first hit sandbox `EPERM` on `127.0.0.1:1420`, then passed startup under elevated `rtk timeout 25s npm run tauri dev`; process ended by timeout as expected after Vite and the Tauri binary launched. EGL warnings were environment-only.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Result: inline review pass after verification. One real issue was found and fixed during the test loop: the pattern editor could rehydrate from stale props immediately after a successful save. The component now keeps the optimistic saved rows and `RepoPanel` remounts the section when the configured `fs_watch` signature changes.

## Security Gate

- Run after work-review loop: not required unless execution adds file-content reads, secrets handling, new capabilities, or backend command changes.
- Security Watch during work: dependency/config watch only; feature edits Tinto config, not watched files.
- Security Watch notes: Plane 2 paths can reference sensitive filenames, but the UI displays only path and metadata already emitted by the backend.
- Security reviewer: fallback inline/adversarial unless new high-risk surface appears.
- Security review result: not required; inline adversarial check found no new backend capability, no watched-file content reads, and no new Tauri permission.
- Required security verification: ensure no watched file content is read or displayed.

## Execution Result

- Store: retains bounded recent Plane 2 file events per repo, newest-first, clears events for repos that leave membership, and exposes `getFsEvents`.
- Client/operations: adds a typed `update_repo` wrapper path for `fs_watch` edits and reloads the active workbench after save.
- UI: adds `WatchedFilesSection` to `RepoPanel`, showing configured patterns, watcher degraded state, recent event metadata, and a local pattern editor with blank/duplicate validation and clear-all support.
- Scope guard: the UI remains informational and inert; it does not open diffs, display watched-file contents, add passive signals, or add timeline behavior.

## CI Break-Prevention And Escalation

- CI risk surfaces: frontend tests/typecheck/lint/build; Rust compile if command registration changes.
- Preventive evidence: full local gate before Release Marshal handoff.
- If CI breaks: invoke `krt-ci-questor` with PR/check context.
- Escalation rule: keep release-follow-up blocker until cause and owner are recorded.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Watched files UI and pattern editor.
- Branch name: `feat/watched-files-ui`.
- Branch/docs rule: this review unit carries related RDM-009 planning artifacts on the same branch.
- PR base: develop.
- Suggested commit grouping:
  - `feat(bus): retain recent watched-file events` — store state/selectors/tests.
  - `feat(workbench): expose watched-file pattern updates` — client wrapper and save flow.
  - `feat(ui): add watched files section` — section component, editor, RepoPanel integration, CSS/tests.
  - `docs(orchestration): capture watched files UI delivery` — docs and state.
- PR title: `Watched files UI for Plane 2 events`
- PR body bullets:
  - Adds a per-repo watched-files section with recent Plane 2 event metadata.
  - Adds a pattern editor for repo `fs_watch` entries backed by the existing workbench update command.
  - Keeps Plane 2 informational and separate from git diffs, passive signals, and timeline history.
- Verification results location: Verification Gate of this package and release thread.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea.
- Jira summary: "Interfaz de archivos vigilados para eventos del Plano 2"
- Jira description: "Añadir la sección de archivos vigilados por repositorio en Tinto, mostrando eventos recientes del Plano 2 y permitiendo editar los patrones fs_watch desde la interfaz sin modificar el TOML manualmente."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
