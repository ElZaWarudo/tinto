---
title: Compound Master State - Tinto
status: active
date: 2026-06-19
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

- Current phase/status: Agent Console Integration initiative in artifact generation phase. Roadmap complete and reviewed. ACI-001 plan and work package ready for execution.
- Active package: ACI-001 Backend PTY Runtime + Agent Process Lifecycle (docs/work-packages/ACI-001-agent-console-backend/2026-06-19-001-agent-console-backend-work-package.md).
- Branch/base: current branch `develop` over `origin/develop`. Working tree has uncommitted keyboard shortcuts feature (shortcuts.ts, KeyboardShortcuts.tsx, tests) that should be committed before starting ACI-001.
- Open PR/Jira: none. Deliveries use local fast-forward merge into `develop` plus push, no PR, by standing user preference. Jira omitted because this checkout reports `jira-env-not-configured`.
- Blockers: none.
- Required user decisions: none for artifact generation. Execution will require user approval to proceed.
- Next action: commit current keyboard shortcuts work, then execute ACI-001 work package (mode:execute package:docs/work-packages/ACI-001-agent-console-backend/2026-06-19-001-agent-console-backend-work-package.md review-unit:RU1).

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

Follow-up tranche — implemented, verified, internally reviewed, and release-approved:

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

## Current Release-Ready Work — PDF/Image Viewer

Post-closeout UX iteration requested on 2026-06-19: add visual previews for PDFs and common image formats inside the existing file tab surface.

- Intended implementation shape: extend the existing read-only file viewer, not a separate app route. Keep diff/text/Markdown behavior unchanged.
- Implementation status: complete. Added `get_media_content` as an additive Tauri bus command, a 12 MiB media read guard, backend media-extension validation, frontend PDF/image detection, `MediaView`, CSP allowances for data/blob media rendering, and contract docs/tests.
- Scope guard: media previews skip the live-diff subscription path, so opening PDFs/images does not consume the diff subscription cap or show diff-paused UI.
- Impact Scan (2026-06-19): changed IPC command contract, frontend client wrapper, Tauri command registration, CSP media directives, and file-view UI. Consumers found via `rg get_media_content|MEDIA_CONTENT_MAX_BYTES|mediaKind|iframe|svg`: `src/bus/client.ts`, `src/panels/file/FileView.tsx`, `src/panels/file/MediaView.tsx`, `src/panels/file/mediaTypes.ts`, `src/bus/contract.test.ts`, `src/panels/file/FileView.test.tsx`, `src/panels/file/MediaView.test.tsx`, and `docs/contracts/bus-contract.md`. Required consumer tests added/updated.
- Security Watch (2026-06-19): media command preserves active-workbench repo allowlist, canonical containment, `.git` exclusion, regular-file-only reads, bounded allocation, and rejects non-media extensions with `unsupported-media`. CSP is widened only for `img-src`, `frame-src`, and `object-src` data/blob media. No auth, tenant, secrets, persistence, external integration, or destructive behavior changed; focused Security Sentinel not required for this prototype-local read-only preview.
- Internal review gate (2026-06-19): direct Compound Master fallback review used because subagent spawning requires explicit user authorization in this runtime. Review found and fixed one P2 stale-content bug where `MediaView` could keep showing the prior file while a new media path loaded. No remaining P0-P2 findings after fix.
- Verification (2026-06-19): targeted `npm test -- FileView.test.tsx MediaView.test.tsx contract.test.ts` passed 29/29 before the stale-path fix; targeted `npm test -- MediaView.test.tsx FileView.test.tsx` passed 20/20 after the fix; full `npm test` passed 158/158; `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` passed 121/121; `git diff --check` clean.
- Visual/server smoke (2026-06-19): Vite dev server is running at `http://127.0.0.1:1420`; `curl -I http://127.0.0.1:1420` returned HTTP 200. Earlier Chrome DevTools MCP screenshot verification remained blocked by browser-target tooling (`Protocol error (Target.setDiscoverTargets): Target closed`), and the available DevTools session currently points at an unrelated app with no navigation tool exposed. Treat this as a tooling/environment blocker for screenshot verification, not evidence that the app failed to serve. Re-attempt visual verification later with a working Chrome DevTools MCP session, Tauri manual smoke, or packaged app run.
- Release handoff readiness: ready for `krt-release-marshal` with Jira policy optional/no-Jira fallback. Current branch/base: `develop` over `origin/develop`. Suggested semantic commit grouping: `feat(files): preview PDFs and images in file tabs` covering bus contract/command, frontend media view, CSP, docs, and tests. Suggested release title: `Preview PDFs and images in file tabs`. Suggested release bullets: add read-only PDF/image previews in existing file tabs; keep text, Markdown, and diff behavior unchanged; bound media reads and reject unsupported media extensions.
- Release Marshal preflight and local release (2026-06-19): starting branch `develop`, selected base `origin/develop`, origin remote `https://github.com/ElZaWarudo/tinto.git`, no existing PR for `develop`, Jira readiness `jira-env-not-configured`, scope guard `human_lines=193`, `generated_lines=0`, `orchestration_doc_lines=0`, `untracked_files_count=3` with no blocking scope warning. User approved local no-PR merge; local `develop` was fast-forwarded; push remains intentionally out of scope.

## Release State

- PR-based deliveries: PR #1, #2, #3, #4, #6, #7, #8, #9, #10, #11, and #12.
- RDM-012 delivery: local fast-forward merge into `develop` and push to `origin/develop`, intentionally without PR by user request.
- Post-closeout state archive: `5631c0e` (`docs(orchestration): compact compound master state`).
- CI workflow delivery: `e0e91ba` (`ci(github): add validation workflow`), merged locally to `develop` and pushed to `origin/develop` without PR.
- CI runtime maintenance: `8ebbdd3` (`ci(github): opt actions into node 24 runtime`), merged locally to `develop` and pushed to `origin/develop` without PR.
- Prior orchestration reconciliation: `2f894d8` (`docs(orchestration): finalize ci maintenance state [skip ci]`), pushed directly to `origin/develop`.
- Workbench IDE Overhaul shipped tranche: `2a701e3` (`feat(qol): per-file text zoom…`), the tip of a 5-commit `feat/project-workspace-ide` set, fast-forward merged into `develop` and pushed to `origin/develop` without PR.
- Workbench IDE Overhaul follow-up release: `8230397` (`feat(workspace): add nested file dock and bento dashboard`) plus orchestration docs, fast-forward merged into `develop` and pushed to `origin/develop` without PR.

## Verification Baseline

- Roadmap-closeout frontend gate: `npm test` 140/140, `npm run lint`, `npm run format:check`, `npm run build`.
- Roadmap-closeout Rust/Tauri gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, `cargo build`, `npm run tauri build`, and `rtk timeout 25s npm run tauri dev`.
- Workbench IDE Overhaul shipped tranche (`develop` `2a701e3`): `npm test` 148/148, `npm run lint`, `npm run build` green; `cargo test` 118/118, `cargo clippy -- -D warnings`, `cargo fmt --check` green.
- Workbench IDE Overhaul follow-up release: `npm test` 149/149, `npm run lint`, `npm run format:check`, `npm run build` green on 2026-06-16; backend unchanged from the shipped tranche (`cargo test` 118/118). CI was not re-run before push; GitHub Actions will validate the pushed `develop` tip.
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
