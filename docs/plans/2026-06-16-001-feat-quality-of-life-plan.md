---
title: "feat: Quality-of-life filters, notifications, and glance mode"
status: plan-ready
date: 2026-06-16
origin: docs/brainstorms/2026-06-16-rdm-012-quality-of-life-requirements.md
roadmap_item: RDM-012
---

# feat: Quality-of-Life Filters, Notifications, and Glance Mode

## Goal

Add the final user-facing quality-of-life layer over Tinto's existing monitoring state: global filters/search, opt-in native notifications for relevant facts, and an in-app glance mode.

## Key Technical Decisions

- **KTD1 - One frontend filter state.** Add a small QoL store/hook that tracks search, repo, extension, time window, notifications enabled/unavailable, and glance mode.
- **KTD2 - Pure filter helpers.** Implement filtering as pure helpers over `BusState`, `RepoDelta`, `FsEvent`, and timeline entries. Do not mutate backend workbench or subscription state.
- **KTD3 - Notification adapter boundary.** Wrap native notifications behind a small adapter so tests can mock permission/send behavior and the UI can degrade when unsupported.
- **KTD4 - Redacted notification copy.** Native notification messages use repo display name plus signal/error kind only; no full paths, file contents, or matched values.
- **KTD5 - Glance is in-app.** Toggle a compact app view/strip that summarizes state; do not add tray/menu/native window management in this package.
- **KTD6 - No backend contract change.** The existing bus contract has enough facts. The only backend/native surface is the Tauri notification plugin/capability if installation succeeds.

## Implementation Units

### U1. QoL state and pure filters

- **Files:** `src/qol/filters.ts`, `src/qol/state.ts`, tests.
- **Approach:** Define `QualityFilters`, `TimeWindow`, default state, matching helpers, extension extraction, and filtered repo/event/timeline helpers.
- **Tests:** search, repo, extension, and time-window filters; no-match behavior; path normalization.

### U2. Top-bar controls and filtered surfaces

- **Files:** `src/workbench/TopBar.tsx`, `src/panels/DashboardPanel.tsx`, `src/panels/RepoTreePanel.tsx`, `src/panels/RepoPanel.tsx`, `src/panels/timeline/TimelinePanel.tsx`, `src/panels/WatchedFilesSection.tsx`, CSS, tests.
- **Approach:** Add a compact filter/search control row. Apply filters to dashboard cards, tree repos/files, watched-file events, and timeline activity/commit entries. Render no-match states.
- **Tests:** controls update state; dashboard/tree/timeline/watch surfaces filter correctly.

### U3. Native notification adapter and watcher

- **Files:** `src/qol/notifications.ts`, `src/qol/notifications.test.ts`, `src/App.tsx`, package/Cargo/capability files if plugin is available.
- **Approach:** Add opt-in notification toggle. Request permission when enabling. Watch bus state for relevant facts and dedupe by stable keys. Send redacted notifications for critical passive signals, warning watched-file signals, terminal repo errors, and degraded watching.
- **Tests:** permission denied, unavailable adapter, dedupe, redaction, relevant event categories.

### U4. Glance mode

- **Files:** `src/qol/GlanceMode.tsx`, `src/App.tsx`, CSS, tests.
- **Approach:** Add compact summary mode with repo count, dirty repo count, critical/warning signal counts, watcher state, and latest activity age. Toggle from top bar; no dock layout mutation.
- **Tests:** glance summary counts, toggle back to workspace, degraded watcher state.

### U5. Verification and closeout

- **Files:** work package and `docs/orchestration/compound-master-state.md`.
- **Verification:** `npm test`, `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `npm run tauri build`, and `rtk timeout 25s npm run tauri dev` smoke.

## Review / Risk Notes

- Native notification plugin setup is the highest-risk surface. If dependency installation or runtime permission support blocks it, keep the notification UI in an unavailable/degraded state and record the gap.
- Notifications can leak sensitive context if copy is careless; tests must assert redaction.
- Filters touch several views but are shallow and should be tested through pure helpers plus focused component tests.

## Out of Scope

- OS tray icon/menu or separate compact native window.
- Persistent historical search database.
- Configurable notification/rule editor.
- Git writes, remote network behavior, or agent control.
- Natural-language summaries.
