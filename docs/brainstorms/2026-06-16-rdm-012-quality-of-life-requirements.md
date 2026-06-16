---
date: 2026-06-16
topic: rdm-012-quality-of-life
---

# Quality of Life Requirements

## Summary

Tinto should add the final quality-of-life layer on top of the completed monitoring surfaces: native notifications for relevant events, repo/extension/time filtering and search, and a compact glance mode that summarizes the current workbench state. The feature must stay read-only, local-only, and factual.

## Problem Frame

Tinto now exposes raw repo state, diffs, watched-file events, timeline entries, and passive signals. The remaining friction is day-to-day use: a supervisor needs to reduce noise, quickly find a repo/file/event, and notice high-attention changes without keeping the full dashboard in view.

The design source calls out native OS notifications, filters/search by repo/extension/time range, and a glance mode. RDM-012 should add those surfaces without adding new monitoring scope or interpretation.

## Key Decisions

- **Quality-of-life over existing facts.** Filters/search and glance mode consume existing bus state, fs events, timeline entries, and passive signals. They do not create new backend monitoring semantics.
- **Native notifications are signal-gated.** Notifications should fire only for high-attention facts: critical passive signals, warning watched-file signals, terminal repo errors, and degraded watching transitions. They must not notify on every status delta.
- **Privacy-first notification copy.** Native notification titles/bodies may include workbench/repo display names and signal labels, but not full paths, raw diff lines, matched secret values, or file contents.
- **Session controls first.** Notification enablement, filters, and glance mode can start as frontend session UI. Persisting them through `ui-state` is allowed if cheap, but not required for v1.
- **Glance mode is in-app compact mode.** Implement an in-app compact summary strip/panel rather than OS tray integration. Tray/menu integration can be a future native-shell enhancement.
- **Facts only.** No natural-language summaries or recommendations.

## Actors

- A1. Supervisor user: keeps Tinto open while agents work and wants less noise.
- A2. Tinto frontend: filters and presents existing facts.
- A3. Tinto backend/Tauri shell: provides notification capability if enabled.
- A4. Local OS notification system: displays concise native notifications.

## Key Flows

- F1. Filtered monitoring
  - **Trigger:** The user enters search text or selects repo/extension/time filters.
  - **Steps:** Dashboard, tree, timeline, watched events, and diff entry points filter visible rows/cards using existing state and event timestamps.
  - **Outcome:** The user can focus on one repo, extension, or recent time window without changing the active workbench.

- F2. Relevant native notification
  - **Trigger:** A new repo delta or watched-file batch introduces a high-attention signal or degraded state.
  - **Steps:** Frontend deduplicates by repo/revision/event id, checks notification enablement/permission, and sends a native notification with redacted factual copy.
  - **Outcome:** The user notices relevant changes even when Tinto is not foregrounded.

- F3. Glance mode
  - **Trigger:** The user toggles glance mode.
  - **Steps:** The app shows a compact summary with repo count, dirty repo count, critical/warning signal counts, watcher state, and latest activity.
  - **Outcome:** The user can monitor the workbench from a smaller UI footprint.

## Requirements

**Filters and search**

- R1. Provide a global quality-of-life control surface reachable from the top bar.
- R2. Search must match repo display names, canonical repo paths, changed file paths, passive signal labels, and watched-file paths.
- R3. Repo filter must narrow to one repo or all repos.
- R4. Extension filter must support common file extensions and free-text extension entry.
- R5. Time filter must support at least all, 15 minutes, 1 hour, and today for timestamped event/timeline surfaces.
- R6. Filters must not mutate workbench state or subscriptions.
- R7. Empty filtered results must render explicit "no matches" states.

**Notifications**

- R8. Notifications must be opt-in from the UI.
- R9. Notification copy must be redacted: no full paths, raw content, or secret-like matched values.
- R10. Notifications must deduplicate repeated state for the same repo revision/event.
- R11. Notifications must cover critical passive signals, warning watched-file signals, terminal repo errors, and degraded watching transitions.
- R12. If native notification permission or plugin support is unavailable, the UI must degrade visibly and keep filtering/glance usable.

**Glance mode**

- R13. Glance mode must show compact totals: repos, dirty repos, critical/warning signals, watcher state, and latest activity age.
- R14. Glance mode must be toggleable without changing the dock layout.
- R15. Glance mode must still let the user reopen the normal workspace view.

**Boundaries**

- R16. No new monitoring sources, no git writes, no remote network behavior, no AI summaries, and no OS tray/menu integration in v1.
- R17. No durable metrics database or event persistence.

## Acceptance Examples

- AE1. Given three repos, when the user filters to one repo, only that repo's card/tree/timeline facts remain visible.
- AE2. Given changed files with `.ts` and `.rs` extensions, when the extension filter is `.ts`, only `.ts` matching facts remain visible.
- AE3. Given timeline and watched events older than the selected time window, they are hidden while current repo cards remain visible when they match the repo/search filters.
- AE4. Given a critical possible-secret signal arrives and notifications are enabled, Tinto sends one redacted native notification for that repo revision.
- AE5. Given the same repo revision is emitted again, no duplicate notification fires.
- AE6. Given notification permission is denied or unsupported, the UI shows notifications unavailable and does not crash.
- AE7. Given glance mode is enabled, the compact summary shows dirty repo and signal counts and hides the heavy dock workspace.
- AE8. Given filters produce no matches, each affected surface shows a no-match state rather than looking broken.

## Success Criteria

- The user can reduce dashboard/timeline/watch noise by repo/search/extension/time.
- Native notifications surface relevant facts without leaking secrets or full paths.
- Glance mode provides a compact monitor state for day-to-day use.
- Existing read-only guarantees remain intact.

## Scope Boundaries

- No OS tray icon or menu in this package.
- No configurable rule editor for signal relevance.
- No persistent historical search database.
- No remote sync, git operations, or agent control.
- No natural-language explanations.

## Dependencies / Assumptions

- Requires RDM-007 through RDM-011 merged to `develop`.
- Assumption: native notifications can use Tauri's notification plugin. If package installation or Linux environment support blocks it, the implementation must keep the UI-visible unavailable state and record the blocker.
- Assumption: session-level filter/glance state is acceptable for v1 because `ui-state` already persists dock layout but not arbitrary preferences.
- Assumption: in-app glance mode satisfies the roadmap's "ventana compacta / item de barra" prototype intent without OS tray integration.

## Sources / Research

- `tinto-design.md`: quality-of-life bullets: native notifications, filters/search, glance mode.
- `docs/roadmaps/2026-06-10-001-tinto-roadmap.md`: RDM-012 depends on RDM-007 and RDM-011.
- `docs/contracts/bus-contract.md`: existing repo state, fs events, passive signals, metrics, and timeline facts.
- `src/workbench/TopBar.tsx`: top-level control surface.
- `src/bus/store.ts`: central frontend state and selectors.
- `src-tauri/src/ui_state.rs`: opaque UI-state persistence if preferences need later persistence.
