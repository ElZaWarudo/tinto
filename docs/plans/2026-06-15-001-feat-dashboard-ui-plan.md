---
title: "feat: Dashboard UI — dockable workspace, repo cards, workbench management"
type: feat
roadmap_item: RDM-007
origin: docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md
contract: docs/contracts/bus-contract.md
depends_on: [RDM-005, RDM-006]
production_posture: prototype
date: 2026-06-15
---

# feat: Dashboard UI — dockable workspace, repo cards, workbench management

## Summary

Build the first Tinto frontend: a VS Code–style dockable workspace (split/drag/tab-group panels with a
persisted global layout) that consumes the frozen bus contract and surfaces the active workbench's repos
live. Panels are a Dashboard (card grid), per-repo detail tabs, and a repo-level tree; plus a top-bar
workbench switcher and Core workbench curation (create/switch/add/remove). The dock engine is the primary
technical risk, so its validation is the first gate (U2) before any panel content is built.

Ships as one review unit (RDM-007), but the units are sequenced so the foundation (deps → dock shell →
data layer) is proven before the panels (dashboard → repo panel → tree → workbench mgmt → integration).

## Problem Frame

The backend and the backend↔frontend contract (`docs/contracts/bus-contract.md`) are complete and frozen;
nothing renders them yet (`src/App.tsx` is only the ping/tick smoke). RDM-007 turns the bus stream into
at-a-glance supervision of repos edited by code agents. The user wants a rearrangeable, splittable
workspace (VS Code–like), built now as the app's shell so RDM-008+ can add diff/file/timeline panels
without a layout rewrite. See origin: `docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md`.

## Requirements Traceability

Honors origin R1–R12 and AE1–AE12. Map (R → unit):

- R1 dockable shell, R2 global persistence, R12 keyboard floor → **U2**, **U8**
- R3 dashboard cards (compact/expanded, hierarchy, git edge states), R6 activity, R11 errors/degraded, R10 loading → **U4** (data: **U3**)
- R4 repo panel (commit log + status list, dedup open) → **U5**
- R5 repo-node tree → **U6**
- R7 switcher, R8 first-run, R9 curation (+R9b zero-repos) → **U7** (deps: **U1**)
- R10 live data binding + name join → **U3**

Out of scope (origin): diffs/file view (RDM-008), file-level tree expansion + `list_repo_tree` (RDM-008),
`set_subscriptions` (RDM-008), Plane-2 UI (RDM-009), timeline (RDM-010), signals (RDM-011), rename/delete
workbench, per-workbench layouts.

## Key Technical Decisions

- **KTD1 — Dock engine: `dockview` (dockview-react).** Native split/drag/tab-group, JSON layout
  serialize/restore (`toJSON`/`fromJSON`), dynamic panel registration by component type, React 19
  compatible, framework-agnostic core. Alternatives rejected: `react-mosaic` (weak tab-group/dynamic
  typed panels), `rc-dock` (heavier, less maintained), `FlexLayout-react` (rigid drag). Validated as a
  gate in U2 before building on it.
- **KTD2 — Layout persistence via a custom Rust command, not a plugin.** A small `get_ui_state` /
  `set_ui_state` command pair writes `ui-state.json` to the OS config dir using the existing `dirs` dep
  (same pattern as the workbench TOML store). Avoids adding `plugin-store`/`plugin-fs` and their
  capability surface. Custom app commands need no capability grant (only plugin/core commands do).
- **KTD3 — Folder picking via `@tauri-apps/plugin-dialog`.** `open({ directory: true })`. Requires adding
  the JS plugin, the Rust `tauri-plugin-dialog`, and the **`dialog:allow-open`** permission in
  `src-tauri/capabilities/default.json` — `dialog:default` does NOT enable the file/folder picker, so the
  open call would fail at runtime, not build time.
- **KTD4 — Frontend state: a lightweight revision-gated store, no heavy framework.** A small store
  (React context + `useSyncExternalStore`, or Zustand if it earns its keep) holds per-repo state keyed by
  canonical path. Deltas apply only if `revision >` the known one (contract rule). Per-card components are
  memoized so a single repo's delta re-renders only its card. No Redux/RTK (overkill for a prototype).
- **KTD5 — Display-name join + canonical-path alignment.** Snapshot/deltas are keyed by the backend's
  **canonical** path and carry no names. Repo aliases + the active workbench come from `list_workbenches`
  (`WorkbenchConfig.active`, `Workbench.repos[].alias`). **Gotcha:** `add_repo` currently stores the
  user-entered path verbatim, so `list_workbenches` paths may NOT be canonical and a naive string join
  against canonical delta paths silently fails (symlinks, `..`, trailing slash, Windows case). Fix at the
  source: U1 canonicalizes the path in the workbench store's add path so config paths match delta paths
  (a small RDM-005 correctness amendment — flagged as a touch to delivered code; surface at release). The
  frontend then joins by exact canonical path; fall back to the path basename when no alias exists.
- **KTD8 — Timestamp units.** `last_activity_ms` is `u64` epoch **milliseconds**; `CommitInfo.timestamp`
  (in `head`) is `i64` epoch **seconds**. The TS contract mirror and the card must treat them distinctly
  (×1000 on `head.timestamp` for a JS `Date`). Mixing them is a silent "commit 50 years ago" bug.
- **KTD9 — invoke strings stay snake_case.** Client wrappers are camelCase JS functions, but the
  `invoke("...")` string must be the exact registered snake_case name (`autodetect_repos_under`,
  `set_active_workbench`, `get_commit_log`, `retry_repo`, etc.).
- **KTD6 — StrictMode-safe listeners.** All `listen()` calls use the existing `src/App.tsx` cleanup
  pattern (`unlisten.then(fn => fn())` + an `active` guard) to survive React 19 dev double-invoke.
- **KTD7 — No subscriptions, no tree command in 007.** `set_subscriptions` and `list_repo_tree` are
  deferred to RDM-008. The Repo panel uses `get_commit_log`.

## High-Level Technical Design

Layered, bottom-up; each layer is verifiable before the next:

```
┌──────────────────────────────────────────────────────────────┐
│ U8 Integration: panel registry, default layout, App shell,    │
│    keyboard floor                                              │
├───────────────┬───────────────┬───────────────┬──────────────┤
│ U4 Dashboard  │ U5 Repo panel │ U6 Repo tree  │ U7 Workbench │
│    panel      │ (commit log + │ (repo nodes)  │ mgmt + first │
│ (cards)       │  status list) │               │ run + switcher│
├───────────────┴───────────────┴───────────────┴──────────────┤
│ U3 Bus client: TS contract types · StrictMode listeners ·     │
│    revision-gated store · snapshot load · list_workbenches join│
├──────────────────────────────────┬───────────────────────────┤
│ U2 Dock shell (dockview) +        │  GATE: validate in webview │
│    layout persistence + guard     │  before panels             │
├──────────────────────────────────┴───────────────────────────┤
│ U1 Deps + Tauri wiring (plugin-dialog, capabilities,          │
│    ui-state Rust command)                                      │
└──────────────────────────────────────────────────────────────┘
```

Data flow (live): backend `emit` → Tauri event → U3 listener → revision-gate → store → memoized panel
re-render. Workbench switch: U7 calls `set_active_workbench` → re-`get_workbench_snapshot` → store reset →
panels swap data; layout (U2) untouched.

## Output Structure

New frontend layout (paths under `src/`):

```
src/
  bus/
    contract.ts          # TS mirror of bus-contract.md types/events/commands
    client.ts            # invoke wrappers + event listeners (StrictMode-safe)
    store.ts             # revision-gated per-repo store + workbench state
    store.test.ts
  workspace/
    DockWorkspace.tsx     # dockview host + panel registry + layout persistence
    panels.ts             # panel-type registry/ids
    layout.ts             # serialize/restore + default layout + guard
    layout.test.ts
  panels/
    DashboardPanel.tsx
    RepoCard.tsx
    RepoCard.test.tsx
    RepoPanel.tsx
    RepoPanel.test.tsx
    RepoTreePanel.tsx
    RepoTreePanel.test.tsx
  workbench/
    TopBar.tsx            # switcher + global actions
    WorkbenchControls.tsx # create / add repos / remove
    firstRun.tsx          # empty state
    workbench.test.tsx
  App.tsx                 # shell composition (replaces smoke)
src-tauri/src/
  ui_state.rs            # get_ui_state / set_ui_state (ui-state.json via dirs)
```

(Final structure may shift slightly during implementation; per-unit Files lists are authoritative.)

---

## Implementation Units

### U1. Dependencies and Tauri wiring

**Goal:** Add the frontend deps and the Tauri plumbing the rest depends on; nothing visual.
**Requirements:** Enables R2, R9 (folder pick + persistence).
**Dependencies:** none.
**Files:**
- `package.json` (+ `dockview-react`, `@tauri-apps/plugin-dialog`)
- `src-tauri/Cargo.toml` (+ `tauri-plugin-dialog`)
- `src-tauri/src/lib.rs` (register the dialog plugin + `ui_state` commands)
- `src-tauri/src/ui_state.rs` (new: `get_ui_state` / `set_ui_state`)
- `src-tauri/src/workbench/mod.rs` (canonicalize the stored repo path on add — KTD5 alignment)
- `src-tauri/capabilities/default.json` (+ `dialog:allow-open`)
**Approach:** `ui_state.rs` mirrors the workbench store pattern: read/write `ui-state.json` in
`dirs::config_dir()/tinto/`; `get_ui_state` returns `Option<String>` (the serialized dockview layout +
any UI prefs) and tolerates missing/corrupt file (returns `None`, never panics); `set_ui_state(json)`
writes atomically (tmp + rename, like `WorkbenchStore`). Register `tauri_plugin_dialog::init()` and add
`get_ui_state`/`set_ui_state` to `generate_handler!`. Add `dialog:allow-open` to capabilities (NOT just
`dialog:default` — see KTD3). **KTD5 amendment:** in the workbench store's add-repo path, canonicalize the
path before storing (fall back to the raw path if canonicalize fails, matching the bus's
`canonicalize().unwrap_or(path)` convention) so `list_workbenches` paths match the canonical paths in
snapshot/deltas. This touches delivered RDM-005 code — minimal and correctness-only; surface at release.
**Patterns to follow:** `src-tauri/src/workbench/mod.rs` (atomic TOML write, corrupt-tolerant load; the
bus's `set_workbench` canonicalization for the convention); existing `generate_handler!` in
`src-tauri/src/lib.rs`.
**Test scenarios (cargo):**
- (`ui_state`) `get_ui_state` on a missing file returns `None` (no error).
- (`ui_state`) `set_ui_state` then `get_ui_state` round-trips the exact string.
- (`ui_state`) a corrupt/unreadable `ui-state.json` yields `None` without panicking.
- (`ui_state`) `set_ui_state` overwrite replaces prior content atomically (no partial-write artifact).
- (`workbench`) adding a repo by a non-canonical path (e.g. with a trailing `/.` or `..`) stores the
  canonical form, so it equals the path the bus/snapshot reports.
**Verification:** `cargo test` green (existing 106 + new); `cargo clippy -- -D warnings` clean;
`cargo fmt --check`; `npm install` clean; app still builds (`cargo build`). Existing workbench tests stay
green after the canonicalization change.

### U2. Dock workspace shell + layout persistence (GATE)

**Goal:** A working dockview workspace — split/drag/tab-group, panels registered by type, layout
serialized to and restored from the U1 command, with the empty-workspace guard. Proves the dock engine in
the Tauri webview before any panel content exists.
**Requirements:** R1, R2; supports R12.
**Dependencies:** U1.
**Files:**
- `src/workspace/DockWorkspace.tsx`, `src/workspace/panels.ts`, `src/workspace/layout.ts`,
  `src/workspace/layout.test.ts`
- `src/App.tsx` (mount the workspace with placeholder panels for the gate)
**Approach:** Host `dockview-react`'s `DockviewReact` with a panel-component registry keyed by panel-type
id (`panels.ts`). `layout.ts` owns: serialize current layout (`api.toJSON()`), restore
(`api.fromJSON()`), a sane **default layout**, and the **empty-workspace guard** (prevent closing the last
panel, or expose a "reset layout"/"reopen Dashboard" action). Persistence: on layout change (debounced)
call `set_ui_state`; on mount, `get_ui_state` → restore or fall back to default. **Write-failure is
non-fatal** (a rejected `set_ui_state` is logged and swallowed — prototype). **Flush on quit:** flush any
pending debounced layout write before the window closes (Tauri close-requested / `beforeunload`) so the
last arrangement reliably persists for AE4. For the gate, register two placeholder panels and confirm
split/drag/tab-group + persist/restore work in `tauri dev`.
**Patterns to follow:** dockview-react docs for `onReady`/`api.toJSON`/`fromJSON`; KTD6 listener pattern
not needed here (no events yet).
**Test scenarios (Vitest, `layout.test.ts`):**
- Default layout builder returns a valid serializable layout with the expected panel ids.
- Serialize→deserialize round-trip of a layout object is stable (deep-equal).
- Restore with `null`/missing persisted state falls back to the default layout.
- Restore with a corrupt/old layout string falls back to default without throwing.
- Guard: attempting to remove the last panel is prevented (or reset reopens the Dashboard).
- `Covers AE4.` quitting/relaunch path is exercised at integration (U8); here assert the
  serialize/restore primitives the persistence relies on.
**Verification:** `npm test` green; manual `tauri dev` smoke (split a panel, drag a tab, relaunch →
arrangement restored) — this is the gate before U4+.

### U3. Bus client: contract types, listeners, revision-gated store

**Goal:** The data backbone — typed contract, StrictMode-safe event listeners, a revision-gated per-repo
store, initial snapshot load, and the `list_workbenches` name join. No UI.
**Requirements:** R10 (+ name join); foundation for R3/R4/R5/R6/R11.
**Dependencies:** U1.
**Files:**
- `src/bus/contract.ts` (TS mirror of `docs/contracts/bus-contract.md`: `RepoDelta`, `RepoStatus`,
  `BranchInfo`, `CommitInfo`, `FsEventBatch`, `WatchingState`, `WorkbenchSnapshot`, `RepoErrorState`,
  event-name constants). Note timestamp units (KTD8): `last_activity_ms` = ms; `CommitInfo.timestamp` = s.
- `src/bus/client.ts` (invoke wrappers — invoke strings are snake_case per KTD9: `getWorkbenchSnapshot`
  → `get_workbench_snapshot` (no args), `listWorkbenches`, `setActiveWorkbench(name)`,
  `createWorkbench(name)`, `addRepo(workbench, path, alias?)`, `removeRepo(workbench, path)`,
  `autodetectReposUnder(root)` → `autodetect_repos_under`, `getCommitLog(repo, offset, limit)`,
  `retryRepo(repo)`; `listen` wrappers for `tinto://workbench-delta` / `tinto://fs-events` /
  `tinto://watching-state`). **Mutations require the workbench NAME** — there is no implicit active
  workbench for `add_repo`/`remove_repo`; the caller (U7) threads the active name from
  `WorkbenchConfig.active`.
- `src/bus/store.ts`, `src/bus/store.test.ts`
**Approach:** `contract.ts` is hand-derived from the frozen doc (it is the source of truth). `store.ts`
holds `Map<canonicalPath, RepoLiveState>` + `watching` + `workbenches`/`active`; `applyDelta` ignores a
delta whose `revision <=` the stored one; `loadSnapshot` seeds state; a separate `joinNames` merges
`list_workbenches` aliases by canonical path. Expose via `useSyncExternalStore` (or Zustand) so panels
subscribe granularly. `client.ts` listeners use KTD6 cleanup.
**Patterns to follow:** `src/App.tsx` listener cleanup (KTD6); contract field names exactly as in
`bus-contract.md`.
**Test scenarios (Vitest, `store.test.ts`):**
- `applyDelta` with higher revision updates the repo; with equal/lower revision is a no-op (`Covers` the
  contract revision rule).
- `loadSnapshot` populates repos + watching; subsequent higher-revision deltas update; stale deltas drop.
- A delta for an unknown repo path is handled per policy (ignored — repo not in active workbench).
- Name join: a repo with an alias in `list_workbenches` shows the alias; without one falls back to a path
  basename; join is by canonical path.
- `watching-state available:false` sets the degraded flag; `true` clears it.
- Error states: a delta with `error` (transient/terminal) is stored and exposed; cleared when a later OK
  delta arrives.
**Verification:** `npm test` green; types compile (`tsc`/build); no listener leaks (cleanup invoked).

### U4. Dashboard panel (card grid)

**Goal:** The card grid panel: one card per repo with compact/expanded views, activity indicator,
reading hierarchy, git edge states, loading skeletons, and per-card error/degraded states + retry.
**Requirements:** R3, R6, R10 (loading), R11.
**Dependencies:** U2, U3.
**Files:** `src/panels/DashboardPanel.tsx`, `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`
**Approach:** `DashboardPanel` reads the store and renders a responsive grid of memoized `RepoCard`s.
Card compact = branch, M/S/U counts, activity dot, error badge; expanded (per-card chevron, not persisted)
adds ahead/behind + last-commit summary. Reading hierarchy per origin R3. **Timestamp units (KTD8):**
`head.timestamp` is **seconds** → ×1000 for a JS `Date`; `last_activity_ms` is already ms — do not mix. Git edge states: unborn → "no
commits yet" (suppress ahead/behind); detached → short SHA + "(detached)"; null upstream → suppress/"no
upstream"; null head → no last commit. Activity: dot/accent when `last_activity_ms` within ~5s, fades
~2s, independent per card. Loading: skeleton cards until snapshot resolves (count from persisted layout
or generic). Degraded banner when `watching.available === false`. Per-card error state with a retry
button → `retryRepo`.
**Patterns to follow:** U3 store selectors; memoization (`React.memo` keyed by repo path + revision).
**Test scenarios (Vitest + testing-library, `RepoCard.test.tsx`):**
- Compact card shows branch, counts, no last-commit; chevron expands to show ahead/behind + last commit.
- `Covers AE2.` a status change for one repo updates that card and not others (store-driven).
- `Covers AE3.` a new commit updates branch/last-commit/ahead-behind.
- `Covers AE11.` unborn / detached / no-upstream / null-head each render their specified label without
  crashing.
- `Covers AE10.` activity indicator appears on a fresh delta and clears after the window; multiple active
  cards each show independent indicators (no global flash).
- `Covers AE9.` a terminal-error repo shows the error state; clicking retry calls `retryRepo`.
- `Covers AE8.` degraded banner renders when `watching.available === false`.
- `Covers AE12.` loading skeletons render before the snapshot resolves.
**Verification:** `npm test` green; visual smoke in `tauri dev` against a real workbench.

### U5. Repo panel (per-project tab)

**Goal:** The per-repo detail panel, differentiated from the card: a short commit log + the full status
file list + error detail/retry; opened/dedup'd by canonical path.
**Requirements:** R4.
**Dependencies:** U2, U3.
**Files:** `src/panels/RepoPanel.tsx`, `src/panels/RepoPanel.test.tsx`
**Approach:** On open, register a Repo panel keyed by canonical path; if one exists for that path, focus
it instead of duplicating (dedup). Body: commit log via `getCommitLog(repo, 0, N)` (N pinned, e.g. 30) +
the full status lists (modified/staged/untracked file names, not just counts) from the store + error
detail with retry. No diffs (008). Opening is triggered from the card (double-click) and the tree (U6).
**Patterns to follow:** dockview `api.addPanel` / `getPanel` for dedup-by-id; U3 `getCommitLog`.
**Test scenarios (Vitest + testing-library):**
- `Covers AE6.` opening a repo renders its commit log (mocked `get_commit_log`) + full status list;
  opening the same repo again focuses the existing panel (no second panel).
- Status list reflects store updates live (a new untracked file appears).
- Commit-log empty (unborn repo) renders an empty-but-not-broken state.
- Error detail + retry calls `retryRepo`.
**Verification:** `npm test` green; two repo panels can be split side by side in `tauri dev`.

### U6. Repo-node tree panel

**Goal:** A tree of the active workbench's repos as nodes (status + activity); clicking a node opens/
focuses its Repo panel. No file-level expansion (deferred to RDM-008).
**Requirements:** R5.
**Dependencies:** U2, U3, U5.
**Files:** `src/panels/RepoTreePanel.tsx`, `src/panels/RepoTreePanel.test.tsx`
**Approach:** Flat list of repo nodes from the store (name via KTD5 join), each showing a compact status
(e.g., `3M 1S 2U` or "clean") + activity dot. Click → open/focus Repo panel (reuse U5 dedup). No
`list_repo_tree` call. Zero-repos handled by U7's empty state.
**Test scenarios (Vitest + testing-library):**
- `Covers AE7.` renders a node per repo with status; clicking a node opens/focuses its Repo panel.
- Node status updates live on a delta.
- No file-level nodes are rendered.
**Verification:** `npm test` green; clicking a tree node opens the repo in `tauri dev`.

### U7. Workbench management: switcher, first-run, curation

**Goal:** Top-bar workbench switcher + first-run empty state + Core curation (create, add repos via
dialog/autodetect, remove with confirmation) + zero-repos state.
**Requirements:** R7, R8, R9 (+ R9b).
**Dependencies:** U1, U3.
**Files:** `src/workbench/TopBar.tsx`, `src/workbench/WorkbenchControls.tsx`, `src/workbench/firstRun.tsx`,
`src/workbench/workbench.test.tsx`
**Approach:** Top bar (fixed, outside the dock area): workbench switcher (from `list_workbenches`, marks
`active`), global actions (add repo, reset layout). Switch → `setActiveWorkbench(name)` →
re-`getWorkbenchSnapshot` → store reset; layout untouched (R2). First-run (no workbench): create-workbench
form (name) → then add repos. **All curation targets the active workbench by NAME** (resolved from
`WorkbenchConfig.active`) — there is no implicit active workbench in the backend mutations. Add repos:
`@tauri-apps/plugin-dialog` `open({directory:true})` → either add the folder as a repo
(`addRepo(activeName, path)`) or autodetect under a root (`autodetectReposUnder(root)` then add each).
Remove repo: one-line confirm → `removeRepo(activeName, path)` → close that repo's open Repo panel. Does
NOT call `update_repo` (alias editing deferred). Zero-repos-in-workbench (R9b): Dashboard + tree show "No repos" +
inline Add action (distinct from first-run).
**Patterns to follow:** U3 client wrappers; existing RDM-005 command shapes in
`src-tauri/src/workbench/commands.rs`.
**Test scenarios (Vitest + testing-library, dialog + commands mocked):**
- `Covers AE1.` first-run create workbench → add a repo (folder pick) → card appears; autodetect path
  also adds repos.
- `Covers AE5.` switching workbench calls `setActiveWorkbench`, reloads snapshot, swaps data (layout
  unchanged — assert layout store untouched).
- `Covers AE12.` (zero-repos) a workbench with no repos shows the "No repos" state with an Add action,
  distinct from the no-workbench first-run.
- Remove repo shows a confirmation; on confirm calls `removeRepo` and closes that repo's panel.
- Switcher reflects `WorkbenchConfig.active`.
**Verification:** `npm test` green; full first-run → add → switch → remove flow works in `tauri dev`.

### U8. Integration: panel registry, default layout, keyboard, app shell

**Goal:** Compose everything: register all real panel types, wire the default layout, keyboard floor,
replace the smoke `App.tsx`, and end-to-end verify.
**Requirements:** R1, R2, R12; integrates all.
**Dependencies:** U2, U4, U5, U6, U7.
**Files:** `src/App.tsx`, `src/workspace/panels.ts` (register real panels), `src/workspace/layout.ts`
(default = tree left + Dashboard right), `src/App.test.tsx` (replace smoke test)
**Approach:** Register Dashboard / Repo / RepoTree panel components in the dockview registry. Default
first-run layout: repo tree left, Dashboard right; top bar above the dock area. Keyboard floor (R12):
arrow keys move card focus, Enter/Space opens the focused card's Repo panel, Escape closes the focused
panel. **Execution note:** roving-tabindex card focus *inside* a dockview panel while Escape closes the
*dockview* panel is the likeliest overrun here — dockview owns its focus/DOM and may intercept keys;
time-box it and keep it minimal (degrade to mouse-first if dockview fights the key handling). Remove the
ping/tick smoke from `App.tsx` (keep the bridge proven elsewhere). Update `App.test.tsx`
to assert the shell mounts with the top bar + default panels.
**Test scenarios (Vitest + testing-library):**
- App mounts: top bar present, Dashboard + tree panels present by default (no persisted layout).
- `Covers AE4.` with a persisted layout, the saved arrangement is restored on mount (mock `get_ui_state`).
- Keyboard: arrow moves focus, Enter opens a Repo panel, Escape closes a panel.
- Empty-workspace guard holds (can't strand the user).
**Verification:** full suite green (`npm test`, `npm run lint`, `cargo test`, `cargo clippy -- -D warnings`,
`cargo fmt --check`); `tauri dev` end-to-end smoke covering AE1–AE12; `tauri build` succeeds.

---

## Test & Verification Strategy

- Frontend: Vitest + testing-library (jsdom); mock `@tauri-apps/api` `invoke`/`listen` and
  `plugin-dialog` (the existing `App.test.tsx` shows the mockIPC/`vi.mock` approach).
- Backend: `cargo test` for the new `ui_state` command; existing 106 tests must stay green.
- Gates per unit as listed; final U8 runs the whole suite + `tauri dev` AE walk-through + `tauri build`.
- Acceptance: AE1–AE12 are each covered by a named test scenario above (see `Covers AEn` tags).

## Risks & Mitigations

- **Dock engine incompatibility (highest).** Mitigated by U2 as an explicit gate: validate dockview's
  split/drag/tab-group + serialize/restore in the Tauri webview before building panels. If dockview fails
  in the webview, fall back (react-mosaic / FlexLayout) is a U2-local decision, not a rebuild of U3+.
- **React 19 StrictMode double-subscribe** → KTD6 cleanup pattern, asserted in U3 tests.
- **Capability/permission gaps** (dialog) → U1 adds the capability entry; without it the dialog fails at
  runtime — covered by the U7 `tauri dev` smoke.
- **Live re-render storms** → memoized per-card components + revision-gated store (KTD4); watch at U4
  smoke. Heavy live-update perf is RDM-008's concern (live diff), not here.
- **Layout persistence corruption** → `get_ui_state` returns `None`/falls back to default on corrupt
  input (U1 + U2 tests).

## Deferred to Follow-Up Work

- File-level tree expansion + `list_repo_tree` wiring, diff/file panels, card→diff drill-through — RDM-008.
- `set_subscriptions`-driven live diffs — RDM-008.
- Rename/delete workbench, per-workbench layouts, richer keyboard/ARIA — later polish.

## Sources & Research

- Origin requirements: `docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md` (reviewed:
  coherence, feasibility, scope-guardian, design-lens).
- Frozen contract: `docs/contracts/bus-contract.md`.
- Backend commands: `src-tauri/src/workbench/commands.rs`, `src-tauri/src/bus/commands.rs`,
  `src-tauri/src/lib.rs`.
- Listener pattern: `src/App.tsx`. Atomic store pattern: `src-tauri/src/workbench/mod.rs`.
- Dock engine: dockview-react (KTD1) — selected as the feasibility-recommended candidate.
