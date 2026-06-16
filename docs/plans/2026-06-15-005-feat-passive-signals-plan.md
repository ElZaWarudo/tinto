---
title: "feat: Passive signals and lightweight metrics"
status: plan-ready
date: 2026-06-15
origin: docs/brainstorms/2026-06-15-rdm-011-passive-signals-requirements.md
roadmap_item: RDM-011
---

# feat: Passive Signals and Lightweight Metrics

## Goal

Add deterministic passive signals and lightweight metrics to the existing bus contract and render them across existing workbench surfaces. The implementation remains read-only, additive-first, and local-only.

## Key Technical Decisions

- **KTD1 - Additive bus fields only.** Extend `RepoDelta` and `FsEvent`; do not add new event names, commands, or required workflow steps.
- **KTD2 - Backend rule engine in the bus layer.** Add a small pure module/function set near the bus contract that classifies paths, diff lines, and Plane 2 size deltas into `PassiveSignal` values.
- **KTD3 - Bounded facts.** Keep a deterministic cap per repo/event and sort by severity then path/kind so payloads are stable.
- **KTD4 - No content leakage.** Signals may name a path and a rule kind, but never include the raw matched line or secret-like value.
- **KTD5 - Surface reuse.** Add small reusable frontend helpers/components for signal chips and metrics; do not add a new panel.
- **KTD6 - Contract tests at both edges.** Rust serialization and TS store/component tests anchor the shape.

## Implementation Units

### U1. Contract types and backend signal model

- **Files:** `docs/contracts/bus-contract.md`, `src-tauri/src/bus/contract.rs`, new or existing backend bus tests.
- **Approach:** Add `PassiveSignal`, `PassiveSignalKind`, `SignalSeverity`, and `RepoMetrics`. Add `metrics` and `signals` to `RepoDelta`; add `signals` to `FsEvent` with empty-list skip where appropriate.
- **Tests:** serialization shape test for signal/metrics fields; backward-additive expectations.

### U2. Backend metrics and detection rules

- **Files:** `src-tauri/src/bus/mod.rs` plus focused tests.
- **Approach:** Compute repo metrics from status + worktree diffs in `recalc_blocking`. Detect sensitive paths, config paths, test paths, large deletions, and simple added-line secret markers. Add Plane 2 signal detection in `RepoLiveState::fs_events`.
- **Tests:** pure rule tests for sensitive paths, secret-line redaction, large deletes, config/test paths, Plane 2 event signals, deterministic cap/sort.

### U3. Frontend contract/store helpers

- **Files:** `src/bus/contract.ts`, `src/bus/store.ts`, `src/bus/store.test.ts`, `src/bus/contract.test.ts`.
- **Approach:** Mirror the additive types. Add helper selectors for repo signals, signal counts, path signals, and metrics fallback.
- **Tests:** store/selectors handle missing fields, path filtering, and severity counts.

### U4. UI presentation across existing views

- **Files:** `src/panels/RepoCard.tsx`, `src/panels/RepoPanel.tsx`, `src/panels/RepoTreePanel.tsx`, `src/panels/diff/DiffPanel.tsx`, `src/panels/WatchedFilesSection.tsx`, `src/panels/timeline/model.ts`, `src/App.css`, and related tests.
- **Approach:** Add compact signal chips/metrics rows. Keep copy factual. Show path-level hints near changed paths and open diffs; show Plane 2 event signals on watched-file rows; optionally include signal labels in Timeline activity entries.
- **Tests:** component tests for card metrics, repo signal list, tree/path marker, diff signal banner, watched-file signal row, and timeline label.

### U5. Verification and closeout

- **Files:** work package and `docs/orchestration/compound-master-state.md`.
- **Approach:** Run surface-aware frontend and Rust verification, perform inline code review, update package/state, then hand off to Release Marshal.
- **Verification:** `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`.

## Review / Risk Notes

- The main risk is backend recompute cost if metrics require `worktree_diff` on every delta. Prototype posture allows it, but the implementation should keep the rule engine bounded and avoid extra file reads beyond existing git diff data.
- Secret detection must be conservative and must not leak matched values.
- UI changes are broad but shallow; one RU is preferable because all views consume the same contract fields.

## Out of Scope

- Notifications, filters/search, glance mode, and configurable rules.
- Durable metrics or event persistence.
- Natural-language summaries or AI scoring.
- Any git write operation.
