---
title: Dashboard UI — dockable workspace, repo cards, workbench management
status: ready
roadmap_item: RDM-007
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
depends_on: [RDM-005, RDM-006]
contract: docs/contracts/bus-contract.md
production_posture: prototype
date: 2026-06-15
reviewed: 2026-06-15 (coherence, feasibility, scope-guardian, design-lens)
---

# Dashboard UI — dockable workspace, repo cards, workbench management

## Outcome

The first Tinto frontend surface. A VS Code–style **dockable workspace** that consumes the frozen
backend↔frontend bus contract and turns the active workbench's live state into at-a-glance supervision:
a card per repo, an optional per-repo detail tab, and a repo-level tree — all in panels the user can
split, drag, tab-group, and rearrange, with the layout persisted across restarts. Includes first-run
setup and ongoing workbench curation so the app is usable end to end without touching the config file.

## Problem & Context

The backend is complete (git engine, classifier, workbenches, watcher, state/event bus) and the
event/command contract is **frozen** at `docs/contracts/bus-contract.md`. Nothing yet renders it: the
frontend is only the ping/tick smoke (`src/App.tsx`, React 19 + Vite 7 + TS, Tauri 2). RDM-007 is the
first visible value — it converts the bus stream into supervision of repos being edited by code agents.

The user (solo developer, building their own tool; prototype, no other users) wants a workspace they can
arrange like VS Code: rearrange tabs, split the screen, dock sub-panels wherever they want. This raises
RDM-007 from "a dashboard" to "a dockable shell whose first panels are the dashboard + tree." Building
the shell now (rather than a fixed layout reworked later) was an explicit user decision, so RDM-008+ can
add diff/file/timeline panels into the same workspace without a layout rewrite.

## Scope

### In scope (RDM-007)

- **R1 — Dockable workspace shell.** Panels can be split (horizontal/vertical), dragged, grouped into
  tab strips, and rearranged. This is the app's foundational shell; later items (008–010) register new
  panel types into it without reworking the layout. **Empty-workspace guard:** the user cannot end up in
  a frame with no panels and no way back — closing the last panel is prevented, or a top-bar action
  reopens the Dashboard.
- **R2 — Layout persistence (global).** The workspace arrangement (open panels + splits + tab groups)
  persists across app restarts. It is **global**, not per-workbench: switching workbench keeps the same
  layout and swaps the data shown. (Per-workbench layouts are explicitly deferred.)
- **R3 — Dashboard panel.** A card grid of all repos in the active workbench. Each card shows: current
  branch, modified/staged/untracked counts, ahead/behind vs upstream, last commit (summary), an activity
  indicator, and an error badge when applicable.
  - **Compact vs expanded:** default **compact** (branch, M/S/U counts, activity indicator, error badge);
    **expanded** adds ahead/behind and last-commit summary. Per-card toggle (chevron); the compact/expanded
    state is not persisted (resets on launch — layout persistence covers panel arrangement, not card state).
  - **Reading hierarchy** (priority, not visual design): (1) activity indicator + repo name; (2) error
    badge if present; (3) M/S/U counts; (4) branch; (5) ahead/behind + last commit. The job is "which
    repo is moving / broken right now," so activity and errors lead.
  - **Git edge states** (the contract models these — specify, don't crash): `branch.unborn` → show
    "no commits yet", suppress ahead/behind; `branch.detached` → show short SHA + "(detached)"; no
    upstream (`ahead`/`behind` null) → show branch, suppress/"no upstream"; `head` null → no last commit.
- **R4 — Repo panel (per-project tabs), differentiated from the card.** A repo opened into its own panel
  (shown as a tab) shows what the card does **not**: a short **commit log** (via `get_commit_log`, last N)
  and the **full status file list** (not just counts), plus error detail with retry. (If, in planning,
  the Repo panel cannot offer content beyond the expanded card, cut it — it must not be "a bigger card.")
  - **Open trigger + dedup:** opening a repo (double-click a card, or click a repo node in the tree)
    opens its Repo panel; if one is already open for that repo, **focus it** rather than opening a second
    (dedup by canonical path).
- **R5 — Repo tree panel (repo-level in 007).** A tree listing the active workbench's repos as nodes,
  each showing status/activity; clicking a repo node focuses/opens its Repo panel (R4). **File-level
  expansion is deferred to RDM-008** (where files become clickable and open diffs) — so 007 does not use
  `list_repo_tree`.
- **R6 — Live activity indicator.** Defined as `last_activity_ms` within a short recent window (~5s,
  tunable). Visual: a small dot / narrow accent on the card and repo tab that fades out over ~2s; each
  card's indicator is independent (no global coordination) so a busy workbench stays scannable rather
  than a wall of flashing.
- **R7 — Workbench switcher.** Lives in a **fixed top bar** outside the dockable area (always visible).
  Selecting a workbench calls `set_active_workbench`, re-reads `get_workbench_snapshot`, and continues
  consuming the delta stream for the new workbench. (007 does **not** call `set_subscriptions` — see A7.)
  The workbench list + active marker come from `list_workbenches` (`WorkbenchConfig.active`).
- **R8 — First-run / empty state.** With no workbench configured, show an empty state that creates a
  workbench inline (name), then adds repos. The flow must be resumable (creating a workbench with zero
  repos is a valid intermediate state, handled by R9b).
- **R9 — Workbench curation (Core).** Add repos to the active workbench via folder pick **or**
  root-with-autodetection (`autodetect_repos_under`), and remove a repo from the active workbench.
  - **Remove flow:** one-line confirmation ("Remove <repo> from this workbench? Files are not deleted.");
    on confirm, the card disappears and any open Repo panel for that repo closes. No undo (prototype).
  - **R9b — Zero-repos-in-workbench state:** a workbench with no repos shows, in the Dashboard panel and
    tree, a "No repos in this workbench" message with an inline "Add repo" action (distinct from the
    no-workbench first-run state of R8).
- **R10 — Live data binding.** Load initial state via `get_workbench_snapshot`, then apply the
  `tinto://workbench-delta` / `tinto://fs-events` / `tinto://watching-state` stream. Apply a delta/snapshot
  for a repo only if its `revision` is greater than the one already shown (contract rule). **Loading
  state:** between launch and snapshot resolution, render card skeletons (placeholder count from the
  persisted layout's last-known repos, else a single generic skeleton); the switcher is active during
  load (it reads local config, not the snapshot).
  - **Display-name join:** deltas/snapshot are keyed by canonical repo **path** and carry no name/alias;
    repo display names + aliases + active-workbench come from `list_workbenches` (`Workbench.repos[].alias`,
    `WorkbenchConfig.active`). The UI joins the two by canonical path (opaque identity per the contract).
- **R11 — Degraded & error states.** Workbench-level "watching unavailable" banner when
  `tinto://watching-state` reports `available:false` (on-demand data still loads). Per-card/per-repo error
  states for `workbench-delta.error` (transient vs terminal) with a retry affordance wired to `retry_repo`.
- **R12 — Keyboard baseline (dev-tool floor).** Arrow keys move focus between cards; Enter/Space opens the
  focused card's Repo panel; Escape closes the focused panel. Panel focus follows the dock library's
  tab-key behavior. ARIA labeling beyond this is deferred post-prototype.

### Out of scope (later items)

- Diff viewer, full-file view, live diff, file→diff / card→diff drill-through — **RDM-008**.
- **File-level tree expansion** (`list_repo_tree`, flat→tree assembly, 20k-cap truncation, changed-file
  highlighting) — **RDM-008** (where files are clickable).
- Plane-2 (watched files) list UI and `fs_watch` editing — **RDM-009**.
- Cross-repo timeline / history feed — **RDM-010**.
- Passive signals (highlights, metrics) — **RDM-011**.
- Rename workbench, delete workbench, full manage-workbenches surface — later (Core curation only in 007).
- Per-workbench layouts (007 persists one global layout).
- `set_subscriptions` usage (subscriptions drive live diffs — RDM-008).

## Acceptance Criteria

- **AE1.** From a clean state (no config), the empty state creates a workbench and adds at least one repo
  (folder pick and root-with-autodetection both work); the dashboard then shows a card per repo.
- **AE2.** Editing a tracked file in a monitored repo updates that repo's card (counts/status) live,
  without manual refresh, and does not disturb other repos' cards.
- **AE3.** A new commit in a repo updates its card's branch/last-commit and ahead/behind live.
- **AE4.** Panels can be split, dragged, and tab-grouped; after quitting and relaunching, the previous
  arrangement is restored (global persistence). Closing the last panel cannot strand the user (R1 guard).
- **AE5.** Switching workbench keeps the panel layout and swaps every panel's data to the new workbench's
  repos.
- **AE6.** Opening a repo (double-click card / click tree node) creates a Repo panel/tab with that repo's
  commit log + full status list; opening the same repo again focuses the existing panel (no duplicate);
  two repos can be shown side by side via a split.
- **AE7.** The repo tree shows repos as nodes with status; clicking a node opens/focuses its Repo panel.
  (No file-level nodes in 007.)
- **AE8.** When the watcher backend is unavailable (`watching-state available:false`), a degraded banner
  is visible and on-demand data still loads (snapshot/commands).
- **AE9.** A repo in error (e.g., removed from disk → terminal) shows an error state with a working retry
  that clears it once the repo is back.
- **AE10.** The live activity indicator marks the repo that just changed and fades afterward; with many
  repos active at once the grid stays scannable (independent per-card indicators, no full-card flashing).
- **AE11.** Git edge states render without crashing: unborn HEAD, detached HEAD, and no-upstream repos
  show their specified labels.
- **AE12.** Loading: on launch the dashboard shows skeletons until the snapshot resolves; the empty
  (no-workbench) and zero-repos states each show their own message + create/add action.

## Dependencies & Assumptions

- **Depends on:** RDM-006 (frozen bus contract + events/commands), RDM-005 (workbench CRUD commands).
  Both merged on `develop`.
- **A1 — Dock engine = biggest technical risk; validate first.** The shell needs a mature React docking
  library with serializable layout (split/drag/tab-group + save/restore round-trip). `dockview` is the
  leading candidate (native split/drag/tab-group, JSON `toJSON`/`fromJSON`, dynamic panel registration by
  type, React 19 compatible). Planning must make the **library choice + a layout serialize/restore +
  Tauri-webview smoke the FIRST step**, gating panel-content work — so a webview/serialization
  incompatibility surfaces before the data layer is built on top.
- **A2 — React live-update perf (D3).** Acceptable at dashboard scale (handful–dozens of repos) with
  memoized per-card components and revision-gated updates; the heavy live-update concern is RDM-008's live
  diff, not this item. Watch, don't block.
- **A3 — Tauri listener lifecycle.** React 19 StrictMode double-invokes effects in dev → naive `listen()`
  double-subscribes. Reuse the existing `App.tsx` cleanup pattern (`unlisten.then(fn => fn())` + an active
  guard) for all of delta/fs-events/watching-state listeners.
- **A4 — Plugins + capabilities not present yet (planning directive).** Today `package.json` has only
  `@tauri-apps/api`; `Cargo.toml` has no dialog/fs/store plugins; `capabilities/default.json` grants only
  `core:default`. 007 needs: (a) a folder dialog — Tauri `plugin-dialog` `open({ directory: true })`;
  (b) UI-layout persistence — preferably a small custom Tauri command writing `ui-state.json` to the
  config dir via the existing `dirs` dep (avoids adding `plugin-store`/`plugin-fs` + their capability
  surface). The plan must add the corresponding `capabilities/default.json` entries or the calls fail at
  runtime, not build time.
- **A5 — Aesthetic baseline.** No design system yet; default to a dark-first, dense, developer-tool look
  consistent with the VS Code reference. Concrete styling approach is a planning decision.
- **A6 — Display identity.** Repo names/aliases + active workbench come from `list_workbenches`, joined to
  path-keyed deltas by canonical path (see R10). `list_repo_tree` is NOT used in 007 (deferred to 008).
- **A7 — No subscriptions in 007.** `set_subscriptions` drives live diffs (008); 007 only listens to the
  workbench delta stream and uses `get_commit_log` for the Repo panel. Confirmed against the contract.

## Outstanding Questions (for planning, non-blocking)

- Final docking library pick + the serialize/restore + Tauri-webview validation result (A1).
- Default first-run layout (e.g., tree left + Dashboard right) and the always-available panel set.
- `get_commit_log` page size N for the Repo panel; whether expanded card should reuse it (default: no —
  keep the card light, the log lives in the Repo panel).
- Exact UI-state file shape/location for layout persistence (A4).

## Success Criteria

The user can launch Tinto, set up a workbench from scratch, arrange a workspace they like (split/dock,
restored next launch), and watch their repos' status update live as code agents edit them — with clear
loading / degraded / error / edge states — all without editing a config file or seeing a diff (which
arrives in RDM-008).
