---
title: Quality-of-life filters, notifications, and glance mode
status: completed
roadmap_item: RDM-012
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-16-rdm-012-quality-of-life-requirements.md
origin_planning_input: docs/brainstorms/2026-06-16-rdm-012-quality-of-life-requirements.md
origin_plan: docs/plans/2026-06-16-001-feat-quality-of-life-plan.md
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

# Quality-of-Life Filters, Notifications, and Glance Mode

## Scope

Implement Tinto's final quality-of-life layer: global repo/search/extension/time filters, opt-in native notifications for relevant passive/degraded events, and an in-app compact glance mode.

## Non-goals

- OS tray icon/menu or separate native compact window.
- Persistent historical search database.
- Configurable notification/rule editor.
- Git writes, remote network behavior, or agent control.
- Natural-language summaries.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: exact UI copy, CSS, filter control layout, default time-window labels, notification dedupe keys, and compact glance presentation.
- Agent must record as assumptions: notification plugin/version used, permission fallback behavior, redaction policy, and any degraded notification support.
- Agent must escalate: non-additive backend contract changes, OS tray/window management, persistent preference schema beyond existing `ui-state`, new remote/network behavior, git writes, public capability broadening beyond notifications, branch/base strategy, Jira/PR workflow, or destructive operations.
- Safe fallback: if native notifications cannot be installed or verified, keep the UI-visible unavailable path and proceed with filters/glance only if the work package records the gap before release.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-007, RDM-008, RDM-009, RDM-010, and RDM-011 merged to `develop`.
- Blocks: roadmap completion.

## Production Posture

- Posture: prototype.
- Evidence: Tinto remains a local desktop prototype with no live deployment.
- Confidence: high.
- Consequences for this package: local UI iteration is acceptable; notification privacy and read-only behavior are not relaxed.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Filter state/helpers are the foundation for search/filter behavior. |
| U2 | yes | Filters only produce value when applied to existing views. |
| U3 | yes | Native notifications are one of the roadmap outcomes. |
| U4 | yes | Glance mode is one of the roadmap outcomes. |
| U5 | yes | Verification and closeout are part of the review unit. |

Grouping rationale:

- Single RU. This is a final UX layer over existing state; filters, notifications, and glance all consume the same store facts and top-bar controls. Splitting notification setup into a standalone PR would add review overhead but little independent value unless plugin installation blocks, in which case the package records a degraded notification result.

## Implementation Units

- U1. QoL state and pure filters.
- U2. Top-bar controls and filtered surfaces.
- U3. Native notification adapter and watcher.
- U4. Glance mode.
- U5. Verification and closeout.

Implementation result:

- U1 implemented and verified: `src/qol/state.ts`, `src/qol/filters.ts`, and `src/qol/filters.test.ts`.
- U2 implemented and verified: TopBar controls plus dashboard, repo tree, repo panel, watched-file, and timeline filtering/no-match states.
- U3 implemented and verified: `@tauri-apps/plugin-notification` / `tauri-plugin-notification` with `notification:default`, redacted notification adapter, permission fallback, watcher, and redaction tests.
- U4 implemented and verified: in-app glance mode, compact summary styling, and glance tests.
- U5 complete: package/state updated with verification, review, security, and release handoff facts.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Quality-of-life layer end to end | frontend QoL helpers/state/components/tests, optional notification plugin/capability, docs | develop | optional standalone Tarea | Broad final UX slice; native notifications add privacy/capability risk, mitigated by redaction tests and adapter boundary |

## Reviewability Diagnosis

- Reviewer-experience check: yes. A reviewer can read commits as filter helpers, surface integration, notification adapter, glance mode, and docs.
- Granularity chosen because: one independently useful final UX capability over the same state source; no stacked PR needed.
- Open-stack plan: independent PR, depth 0 of max 3.
- Jira mapping: single-review-unit PR maps to one standalone Tarea if Jira is available.
- Downstream-fix trace: none.
- Failure-mode check: not a deep micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests

- Likely frontend files: `src/qol/*`, `src/App.tsx`, `src/workbench/TopBar.tsx`, `src/panels/DashboardPanel.tsx`, `src/panels/RepoTreePanel.tsx`, `src/panels/RepoPanel.tsx`, `src/panels/WatchedFilesSection.tsx`, `src/panels/timeline/TimelinePanel.tsx`, `src/App.css`, and related tests.
- Possible native dependency/config files: `package.json`, lockfile, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`.
- Docs: this package, origin brainstorm, plan, and `docs/orchestration/compound-master-state.md`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: no bus contract change planned; possible Tauri notification capability/dependency.
- Consumer scan patterns: `rg "useBusState|RepoDelta|FsEvent|PassiveSignal|TimelinePanel|DashboardPanel|TopBar|set_ui_state|get_ui_state|notification" src src-tauri docs`.
- Consumers found: existing bus consumers in `src/bus/*`, `src/panels/*`, `src/workbench/TopBar.tsx`, `src/workspace/*`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, and prior package docs. No bus payload/schema consumers required migration.
- Contract-drift tests searched: affected component tests, notification adapter tests, capability/package review, `src/bus/contract.test.ts`, Rust contract tests, and Tauri capability/dependency inspection.
- Required consumer tests: pure filters, dashboard/tree/timeline/watch filtering, notifications permission/dedupe/redaction, glance mode summary, existing panel tests, frontend build, Rust build/tests/clippy, and Tauri bundle.
- Consumer tests run/skipped: `npm test` 140/140, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, `cargo build`, `npm run tauri build`, and `rtk timeout 25s npm run tauri dev` smoke all passed or reached expected timeout after launch.

## Verification Gate

- Required: `npm test` **140/140**, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` **117/117**, `cargo build`, `npm run tauri build`, `rtk timeout 25s npm run tauri dev`.
- Surface-aware evidence: pure filter tests; notification permission/redaction tests; glance summary tests; existing dashboard/tree/repo/watch/timeline panel tests; new timeline time-filter regression; `notification:default` capability inspection; package/Cargo lockfile changes; Tauri build bundles deb/rpm/AppImage.
- Production posture evidence: prototype; local Linux build/smoke sufficient unless native notification plugin introduces platform-specific blockers.

Verification results:

- PASS: `npm test` (25 files, 140 tests).
- PASS: `npm run lint`.
- PASS: `npm run format:check`.
- PASS: `npm run build` (`tsc && vite build`).
- PASS: `cargo fmt --check`.
- PASS: `cargo clippy --all-targets -- -D warnings`.
- PASS: `cargo test` (117 tests).
- PASS: `cargo build`.
- PASS: `npm run tauri build` (release binary plus deb/rpm/AppImage).
- PASS with expected timeout: `rtk timeout 25s npm run tauri dev` launched Vite and `target/debug/tinto`; it ended by timeout, with environment-only EGL/MESA warnings.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

Review result: PASS inline.

- Finding fixed: timeline time-window filtering initially covered activity/fs entries but not commit entries. `TimelinePanel` now applies `matchesTimeWindow` to commits and has a regression test.
- No remaining P0-P2 findings after re-verification.

## Security Gate

- Run after work-review loop: required inline/adversarial because native notifications can expose sensitive context.
- Security Watch during work: enabled for notification copy, permission behavior, and capability scope.
- Security Watch notes: notifications must not include full paths, raw file content, raw diff lines, or matched secret values.
- Security reviewer: inline/adversarial; escalate to `krt-security-sentinel` only if capability scope broadens beyond notifications/dialog or if notification copy needs sensitive details.
- Security review result: PASS inline/adversarial.
- Required security verification: redaction tests prove notification titles/bodies do not include repo full paths, watched-file paths, or matched secret-like values; no git writes added; no remote runtime behavior added; Tauri capability broadening is limited to `notification:default` alongside the existing dialog grant.

## CI Break-Prevention And Escalation

- CI risk surfaces: frontend tests/typecheck/lint/build, Rust dependency resolution/compile for notification plugin, Tauri capability schema, package/Cargo lockfiles, Tauri bundle.
- Preventive evidence: full local verification complete; first Rust dependency download required network escalation, then Cargo test/build/clippy and Tauri build passed.
- If CI breaks: invoke `krt-ci-questor` with PR/check context.
- Escalation rule: keep release-follow-up blocker until cause and owner are recorded.

Release result:

- Completed by local fast-forward merge into `develop` and `git push origin develop` on 2026-06-16, per user direction to skip GitHub PR merges for this project flow.
- Jira omitted: `jira-env-not-configured`.
- PR omitted intentionally by user request.

## Branch and PR Handoff Inputs

- Review unit: RU1 - Quality-of-life layer.
- Branch name: `feat/quality-of-life`.
- Branch/docs rule: this review unit carries related RDM-012 planning artifacts on the same semantic branch.
- PR base: develop.
- Suggested commit grouping:
  - `feat(qol): add filter state and matching helpers` - QoL state/filter helpers/tests.
  - `feat(qol): apply filters across monitoring views` - TopBar controls, dashboard/tree/timeline/watch integrations/tests.
  - `feat(qol): add redacted native notifications` - notification adapter/plugin/capability/tests.
  - `feat(qol): add glance mode summary` - glance mode component/App wiring/tests/CSS.
  - `docs(orchestration): capture quality-of-life delivery` - docs and state.
- PR title: `Add quality-of-life filters, notifications, and glance mode`
- PR body bullets:
  - Adds global repo, extension, time, and search filters across monitoring views.
  - Adds opt-in redacted notifications for high-attention local events.
  - Adds a compact glance mode for workbench status.
- Verification results location: this package and release thread.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea.
- Jira summary: "Filtros, notificaciones y modo vistazo"
- Jira description: "Anadir en Tinto filtros globales, notificaciones locales redactadas y un modo compacto de vistazo para supervisar el workbench con menos ruido."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
