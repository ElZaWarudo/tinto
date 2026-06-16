---
title: Compound Master State - Tinto
status: active
date: 2026-06-16
initiative: tinto
mode: full
production_posture: prototype
state_format: compact
last_compacted: 2026-06-16
archive_snapshot: docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md
final_summary: docs/orchestration/2026-06-16-compound-master-summary.md
---

# Compound Master State - Tinto

## Resume Snapshot

- Current phase/status: original roadmap (RDM-001 through RDM-012) complete and closed. A post-closeout, user-driven UX initiative is in progress: the workbench was redesigned into a project-centric, VS Code-style IDE. First tranche shipped to `develop`; a follow-up UI tranche is implemented, locally verified, internally reviewed, and ready for release handoff.
- Active package: post-closeout Workbench IDE Overhaul (no RDM id; iterative UX work directed turn-by-turn by the user, not a roadmap-planned package).
- Branch/base: `develop` is the integration branch at `2a701e3`, in sync with `origin/develop`. The working tree holds the uncommitted follow-up tranche plus `brand/wordmark.png`, which is unreferenced by the diff and excluded from this review unit unless explicitly assigned.
- Open PR/Jira: none. Deliveries use local fast-forward merge into `develop` plus push, no PR, by standing user preference. Jira omitted because this checkout reports `jira-env-not-configured`.
- Blockers: none.
- Required user decisions: release flow approval for the follow-up tranche commit/push plan; whether `brand/wordmark.png` belongs to a later branding task or should remain untracked.
- Next action: hand the completed follow-up tranche to `krt-release-marshal` for commit planning and the project-preferred no-PR local integration/push path.

## Source Documents

- Design source: `tinto-design.md`.
- Roadmap: `docs/roadmaps/2026-06-10-001-tinto-roadmap.md`.
- Final closeout summary: `docs/orchestration/2026-06-16-compound-master-summary.md`.
- Full archived pre-compaction state: `docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md`.

## Completed Delivery

- RDM-001: Tauri 2 + React skeleton.
- RDM-002: read-only Git engine.
- RDM-003: Git/path classifier.
- RDM-004: filesystem watcher.
- RDM-005: workbench manager and config persistence.
- RDM-006: state/event bus and frozen backend/frontend contract.
- RDM-007: dockable dashboard/workbench UI.
- RDM-008: diff viewer and live diff.
- RDM-009: watched-files UI and pattern editor.
- RDM-010: timeline/history panel.
- RDM-011: passive signals and lightweight metrics.
- RDM-012: filters/search, redacted native notifications, and glance mode.

## Post-Closeout Initiative — Workbench IDE Overhaul

User-directed UX initiative after roadmap closeout. Goal: reshape the dockable workbench into a project-centric, VS Code-style IDE. Iterated turn-by-turn; not a roadmap-planned RDM package and not gated through brainstorm/plan/work-package artifacts.

Shipped tranche — merged to `develop` at `2a701e3` (local fast-forward, no PR), 5 atomic commits:

- `fix(watcher): tolerate permission-denied subdirectories when mounting` — a permission-denied subtree (e.g. a mongo data dir under a worktree) no longer degrades the whole repo watch; accessible watches are kept.
- `fix(workbench): live-reflect repo mutations and open the added repo's tab` — `add_repo`/`remove_repo`/`update_repo` reseed the live bus so changes appear without restart; `add_repo` returns the canonical path so the new repo's tab opens.
- `feat(workspace): project-centric tabbed workspace with menu bar and per-project explorer` — VS Code-style menu bar (replaces the top bar), level-1 dockview tabs for Dashboard/Timeline/projects, per-project file explorer, project-tab change indicator, folder change indicators, dashboard filters.
- `feat(files): file viewer with diff/normal/markdown views and syntax highlighting` — unified FileView (diff for changed files, normal highlighted view otherwise, rendered Markdown for `.md`), Shiki highlighting in the full-file view, `react-markdown` + `remark-gfm` added.
- `feat(qol): per-file text zoom with Ctrl +/-/0` — scales only the open file's content via a `--file-zoom` CSS variable, persisted to `localStorage`.

Follow-up tranche — implemented, verified, and internally reviewed but UNCOMMITTED in the working tree (pending release handoff):

- Level-1 project tabs enlarged (dockview height override) and the change-indicator dot given a fixed slot so the title no longer shifts.
- Level-2 file tabs migrated to a NESTED dockview per project (`fileDock` registry) so files can be dragged into splits and two files can sit on screen at once; VS Code preview/pin re-implemented on top (single reused italic preview panel; double-click pins). Open-file layout (files + splits) persists across sessions in `localStorage` per repo.
- Single-click/double-click pattern replaced double-click abuse: single click previews, double click pins.
- Dashboard redesigned as a bento grid: cards lost the confusing expand toggle, show key health at a glance, fix text overflow, and feature attention-worthy repos with wider tiles.
- Internal review gate (2026-06-16): local Compound Master code review of the nested dock, persistence, click/double-click behavior, dashboard card simplification, and tests found no P0-P2 findings.
- Impact Scan (2026-06-16): no backend, Tauri command, auth, persistence schema, API payload, generated binding, fixture contract, or CI workflow contract changed. Changed surfaces are frontend UI state and browser `localStorage` keys (`tinto:filedock:<repo>`), covered by `src/workspace/fileDock.test.ts`.
- Excluded from this review unit: untracked `brand/wordmark.png`; no references to `wordmark` or `brand/` exist in the source tree.

## Canonical Artifact Roots

- Brainstorms: `docs/brainstorms/`.
- Plans: `docs/plans/`.
- Work packages: `docs/work-packages/`.
- Contract docs: `docs/contracts/`.
- Backlog: `docs/backlog/`.

## Release State

- PR-based deliveries: PR #1, #2, #3, #4, #6, #7, #8, #9, #10, #11, and #12.
- RDM-012 delivery: local fast-forward merge into `develop` and push to `origin/develop`, intentionally without PR by user request.
- Post-closeout state archive: `5631c0e` (`docs(orchestration): compact compound master state`).
- CI workflow delivery: `e0e91ba` (`ci(github): add validation workflow`), merged locally to `develop` and pushed to `origin/develop` without PR.
- CI runtime maintenance: `8ebbdd3` (`ci(github): opt actions into node 24 runtime`), merged locally to `develop` and pushed to `origin/develop` without PR.
- Prior orchestration reconciliation: `2f894d8` (`docs(orchestration): finalize ci maintenance state [skip ci]`), pushed directly to `origin/develop`.
- Workbench IDE Overhaul shipped tranche: `2a701e3` (`feat(qol): per-file text zoom…`), the tip of a 5-commit `feat/project-workspace-ide` set, fast-forward merged into `develop` and pushed to `origin/develop` without PR. Follow-up tranche (nested file dock, persistence, bento dashboard, level-1 tab polish) remains uncommitted pending the next ship.

## Verification Baseline

- Roadmap-closeout frontend gate: `npm test` 140/140, `npm run lint`, `npm run format:check`, `npm run build`.
- Roadmap-closeout Rust/Tauri gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, `cargo build`, `npm run tauri build`, and `rtk timeout 25s npm run tauri dev`.
- Workbench IDE Overhaul shipped tranche (`develop` `2a701e3`): `npm test` 148/148, `npm run lint`, `npm run build` green; `cargo test` 118/118, `cargo clippy -- -D warnings`, `cargo fmt --check` green.
- Workbench IDE Overhaul working tree (shipped + uncommitted follow-up): `npm test` 149/149, `npm run lint`, `npm run format:check`, `npm run build` green on 2026-06-16; backend unchanged from the shipped tranche (`cargo test` 118/118). CI was not re-run for the uncommitted follow-up because it is not yet pushed.
- GitHub Actions CI workflow exists at `.github/workflows/ci.yml`.
- Local pre-push CI workflow validation: `npm run format:check`, `npm run lint`, `npm test` 140/140, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, and `npm run tauri build`.
- GitHub Actions run `27601639210`: passed. Frontend passed in 58s, Rust passed in 8m44s, and Tauri bundle passed in 10m46s.
- GitHub Actions run `27602696319`: passed after the Node 24 runtime opt-in. Frontend passed in 1m0s, Rust passed in 2m2s, and Tauri bundle passed in 4m27s.
- CI warning cleanup: `.github/workflows/ci.yml` sets `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true`. GitHub now reports that Node 20-targeting actions are forced to run on Node 24; the remaining annotation is informational until the upstream actions target Node 24 natively.

## Post-Closeout Maintenance

- Branch cleanup completed for merged local branches: `docs/compact-compound-state`, `docs/compound-master-closeout`, `feat/diff-viewer`, `feat/passive-signals`, `feat/quality-of-life`, `feat/timeline-history`, and `feat/watched-files-ui`.
- Remote cleanup completed for merged feature branches: `origin/feat/diff-viewer`, `origin/feat/passive-signals`, `origin/feat/timeline-history`, and `origin/feat/watched-files-ui`.
- Remote `origin/checkpoint/state-event-bus` and local `checkpoint/state-event-bus` were retained because the remote branch was not listed as merged into `develop`.
- Final docs-only reconciliation used `[skip ci]`; no third CI run was expected or required.
- Delivery workflow preference updated by user: avoid GitHub PR merges for this project unless explicitly requested; use local integration into `develop` and push.

## Residual Backlog

- Opt-in Git fetch: `docs/backlog/2026-06-11-fetch-opt-in-backlog.md`.
- Phantom-repo generation token after workbench switch.
- TypeScript/Rust contract code generation.
- Keyboard arrow navigation/Escape polish.
- Diff viewer hardening/polish: manual-reload cancellation race, full-file/diff revision skew hardening, `useDiffData` extraction, S/M/U mark consolidation, and workbench-switch diff-panel orphan handling.

## Archive Status

- State archive status: compacted.
- Compact state path: `docs/orchestration/compound-master-state.md`.
- Archive snapshot: `docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md`.
- Notes: archive contains full historical run detail; this compact state is only the resume entrypoint.
