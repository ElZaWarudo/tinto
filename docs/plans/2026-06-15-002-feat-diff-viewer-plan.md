# RDM-008 — Diff viewer + Live diff — Implementation Plan

- **Date:** 2026-06-15
- **Origin:** `docs/brainstorms/2026-06-15-rdm-008-diff-viewer-requirements.md` (review-passed)
- **Roadmap item:** RDM-008
- **Depends on (shipped):** RDM-006 (frozen contract) · RDM-007 (dock shell, panels, bus store/client, `WorkspaceActions`)
- **Posture:** prototype, solo dev. English. Ships as **one review unit** (consistent with RDM-007; U2 store-slice correctness is the internal de-risking gate).

## Problem frame & scope

Build the diff viewer: a custom renderer over the backend's structured hunks
(inline + side-by-side, Shiki highlighting), a **full-file** view, **live** updates via
subscriptions, file-level **tree expansion**, and **drill-through** from cards/panels/tree.

**No backend changes.** The frozen contract already serves every need (brainstorm
"Contract mapping"); `cargo` surface is untouched (stays at 114 tests). All work is in
`src/` + one dependency (`shiki`). The one non-trivial design point is the **diff store
slice** (U2) that retains transient `subscribed_diffs` — that is the correctness core.

In scope: R1–R13 / AE1–AE13 of the brainstorm. Out of scope: RDM-009/010/011 surfaces,
writes, intra-line diff, virtualization.

## Key technical decisions

- **KTD1 — No backend / no contract change.** Verified against `src-tauri/src/bus/*` and
  `docs/contracts/bus-contract.md`. If any gap appears during execution, STOP and
  escalate (the contract is frozen; additive-only).
- **KTD2 — Diff store slice: retain on `null`, replace-set on non-null.** `subscribed_diffs`
  is `None` on every non-fresh-recalc emit path (backend `mod.rs` `delta()` default), and
  a fresh recompute emits a **non-null array that is authoritative** for that repo's
  subscribed targets (it contains one `FileDiff` per subscribed file that currently has a
  change; a file that went **clean is omitted**, not sent as null/empty). So the store keeps
  `diffs: Record<repo, Record<path, FileDiff>>` and:
  - **`subscribed_diffs == null`** → **retain** `diffs[repo]` unchanged (a status-only /
    transient-error delta must not blank an open diff — brainstorm R5a, AE5).
  - **`subscribed_diffs` is a (possibly empty) array** → **replace** `diffs[repo]` with the
    map built from it. This makes a reverted file's diff drop out (clean-clear by omission)
    rather than persisting stale. Setting `diffs[repo]` (even to `{}`) also marks "a diff
    computation has occurred for this repo", which the panel uses to leave its loading state
    (target absent after a computation ⇒ clean / no-renderable, not loading).
  This dual rule is the load-bearing invariant; AE5 tests the retain side and a new test
  covers the clean-clear side.
- **KTD3 — Single reconciled subscription set, frontend-bounded to 8 global.** A reconciler
  module derives the desired target set from open diff panels, bounds it global-MRU to 8,
  and pushes a coalesced/idempotent `set_subscriptions` (diff against last-pushed). Never
  rely on the backend's non-deterministic `truncate`; never N per-panel calls (brainstorm
  R6/R6a/R12).
- **KTD4 — Custom renderer from `DiffHunk`/`DiffLine`.** One data source feeds inline and
  side-by-side. Concrete TS types replace `hunks: unknown[]`; `DiffLineKind` is the
  PascalCase union `"Added"|"Removed"|"Context"` (round-trip test). (brainstorm D-008-1/5)
- **KTD5 — Shiki lazy + size-capped.** `shiki/core` with `createHighlighterCore` + lazy
  grammar/theme imports; highlighting layers on after structure renders; over a size cap
  (and binary) → plain monospace. No virtualization. (brainstorm D-008-2/R3/R13)
- **KTD6 — Panel id encodes target.** `diff:${repo}:${path}` → layout persistence + dedup,
  reusing the `repoPanelId`/`openRepoPanel` patterns (`src/workspace/panels.ts`,
  `openRepo.ts`). Restore funnels through one batched subscription push. (R11)
- **KTD7 — Initial load mandatory, tracked/untracked-branched.** Tracked → one-shot
  `get_worktree_diff` + subscribe; untracked → subscribe-triggered recompute (one-shot is
  empty for untracked). One-shot fills only until the first live delta. (R7)
- **KTD8 — Drill-through gesture = double-click + Enter/Space**, matching `RepoCard`. New
  `openDiff(repo, path)` on `WorkspaceActions`. (R9)

## Existing patterns to follow

- Panel id + dedup: `src/workspace/panels.ts`, `src/workspace/openRepo.ts`.
- Actions context: `src/workspace/actions.tsx` (`WorkspaceActions`).
- Store shape + revision rule + immutability-preserving sets: `src/bus/store.ts`.
- Client wrapper style (single-word args, snake_case invoke): `src/bus/client.ts`.
- Panel component + states (muted empty/error, loading): `src/panels/RepoPanel.tsx`.
- Card interaction (double-click + key): `src/panels/RepoCard.tsx`.
- Layout restore/persist + flush-on-quit: `src/workspace/layout.ts`, `DockWorkspace.tsx`.
- StrictMode-safe listeners: `src/bus/connection.ts`.
- Theme vars (`--added`/`--warn`/`--danger`): `src/App.css`.

## Implementation units

### U1 — Concrete diff/tree/content types + client wrappers  *(foundation)*

**Files**
- `src/bus/contract.ts` — replace `FileDiff.hunks: unknown[]` with concrete types:
  `DiffLineKind = "Added" | "Removed" | "Context"`; `DiffLine { kind, content,
  old_lineno: number|null, new_lineno: number|null }`; `DiffHunk { old_start, new_start,
  lines: DiffLine[] }`; `FileDiff.hunks: DiffHunk[]`. Add `SubscriptionTarget { repo,
  path?: string|null }`, `TreeEntry { path, is_dir }`, `RepoTree { entries: TreeEntry[],
  truncated }`, `FileContent { encoding: "utf8"|"base64", content, truncated }`.
- `src/bus/client.ts` — add `getWorktreeDiff(repo)`, `getFileContent(repo, path)`,
  `listRepoTree(repo)`, `setSubscriptions(targets)` (invoke `set_subscriptions`,
  `{ targets }`).
- `src/bus/contract.test.ts` *(new)* — round-trip the PascalCase enum + a sample hunk.

**Tests:** PascalCase kind serialization pinned; a representative `FileDiff` typechecks &
round-trips; client wrappers issue the right invoke name + arg shape (mock `invoke`).

**Verify:** tsc + the new tests green. **Depends on:** none.

### U2 — Diff store slice + subscription reconciler  *(internal correctness gate)*

**Files**
- `src/bus/store.ts` — add `diffs: Record<string, Record<string, FileDiff>>` to
  `BusState`; in `applyDelta` apply KTD2's dual rule: on `subscribed_diffs == null` leave
  `diffs[repo]` untouched; on a non-null array, **set** `diffs[repo]` to the map built from
  it (clean-clear by omission; an empty array sets `{}`). Drop a repo's diffs when it leaves
  membership (`loadSnapshot`) and on `reset`. Add `dropDiff(repo, path)` for panel close, a
  `getDiff(repo, path)` selector, and a `hasComputedDiffs(repo)` selector
  (`diffs[repo] !== undefined`) so a panel can distinguish "loading" from "clean". Preserve
  reference stability for untouched repos (existing pattern).
- `src/workspace/subscriptions.ts` *(new)* — `SubscriptionReconciler`: holds the set of
  open `(repo, path)` targets with open-order; `desiredSet()` = most-recently-opened ≤ 8
  (global); `reconcile()` diffs desired vs last-pushed and calls `setSubscriptions` only
  on change (coalesced via microtask/debounce); exposes `isLive(repo, path)` for the
  panel to choose live vs live-paused; `add`/`remove`/`restore(targets[])` (batched).
- `src/bus/store.test.ts` — extend.
- `src/workspace/subscriptions.test.ts` *(new)*.

**Tests (the de-risking gate):**
- AE5 retain: a delta with `subscribed_diffs:[A]` fills the slice; a *subsequent*
  status-only delta (`subscribed_diffs:null`, higher revision) for the same repo does
  **not** clear A.
- Clean-clear by omission: with A and B both in the slice, a non-null
  `subscribed_diffs:[A]` (B reverted/omitted) leaves `diffs[repo] = {A}` — B is gone; an
  empty array sets `{}`; `hasComputedDiffs(repo)` is true after either.
- Revision rule still gates status while diffs apply.
- `dropDiff` removes a target; repo leaving membership drops its diffs.
- Reconciler: 9 targets across 2 repos → exactly 8 pushed (MRU), 1 live-paused; opening
  an already-present target is idempotent (no push); rapid add→remove→add coalesces to
  one push; `restore([...>8])` pushes once, bounded.

**Verify:** vitest green. **Depends on:** U1.

### U3 — Diff renderer (inline + side-by-side + highlighting)

**Files**
- `src/panels/diff/DiffView.tsx` *(new)* — pure presentational: given a `FileDiff` + mode
  (`"inline"|"side-by-side"`) + optional highlighter, render hunks with per-side line
  numbers; added/removed/context styled via theme vars; binary → placeholder; over-cap →
  plain + "large file" notice.
- `src/panels/diff/highlight.ts` *(new)* — lazy Shiki: `getHighlighter()` memoized,
  `languageFromPath(path)`, `highlightLines(code, lang)` returning tokenized spans;
  size-cap guard returns null (→ plain). Theme aligned to `themeVisualStudio`.
- `src/panels/diff/diff.css` *(new)* or append to `App.css`.
- `package.json` — add `shiki`.
- `src/panels/diff/DiffView.test.tsx` *(new)*.

**Tests:** inline renders removed-then-added with correct old/new line numbers;
side-by-side aligns old left / new right; context lines carry both numbers; binary →
placeholder; over-cap → plain + notice. (Shiki mocked/bypassed; highlighting verified at
the decision boundary — `languageFromPath` + the cap guard, not Shiki internals.)

**Verify:** vitest + tsc; bundle sanity (lazy import present). **Depends on:** U1.

### U4 — Diff panel (lifecycle, initial load, live, states)

**Files**
- `src/workspace/panels.ts` — add `PANEL_DIFF = "diff"`, `diffPanelId(repo, path)`,
  `targetFromDiffPanelId(id)`.
- `src/workspace/openDiff.ts` *(new)* — `openDiffPanel(api, repo, path, title)`:
  dedup-by-id + focus (mirror of `openRepo.ts`).
- `src/workspace/actions.tsx` — add `openDiff(repo, path)` to `WorkspaceActions` + noop.
- `src/panels/diff/DiffPanel.tsx` *(new)* — the panel component: on mount, register the
  target with the reconciler (live vs paused) and run the R7 branched initial load
  (tracked: one-shot `get_worktree_diff` pick + subscribe; untracked: wait for live);
  read the live `FileDiff` from the store slice (`useBusState` selector). **Loading vs
  clean:** show loading until either a diff for the target appears or `hasComputedDiffs(repo)`
  becomes true with the target absent (⇒ clean / no-renderable — covers untracked
  binary/oversized where the backend synth returns nothing). Header controls
  = `Hunks · Full file` segmented + inline/side-by-side toggle (disabled in Full file);
  Full-file view (sub-component or inline) via `get_file_content` with R4 degrade rules;
  render all R10 states incl. live-paused (manual reload) and renamed (encoded path not
  in live targets after a delta); on unmount, `dropDiff` + reconciler `remove`.
- `src/panels/diff/FullFileView.tsx` *(new, optional split)*.
- `src/panels/diff/DiffPanel.test.tsx` *(new)*.

**Tests:** mount → reconciler gets the target and `set_subscriptions` reflects it; tracked
initial load paints from the one-shot then live delta supersedes; untracked paints from
the live recompute; a fresh `subscribed_diffs` for the repo **without** the target
(idle/None-synth untracked or reverted file) leaves loading → clean/no-renderable (not a
hang); status-only delta does not blank (integration of R5a at the panel
level); each state renders (loading/clean/error+retry/binary/truncated/repo-not-allowed/
live-paused/renamed); Hunks↔Full-file preserves target and disables the layout toggle;
unmount drops the slice + target. (dockview mocked as in RDM-007 tests.)

**Verify:** vitest + tsc/eslint. **Depends on:** U2, U3.

### U5 — File-tree expansion + drill-through

**Files**
- `src/panels/tree/fileTree.ts` *(new)* — pure: build a nested folder tree from flat
  `TreeEntry[]`; mark changed leaves from a `RepoStatus`; stable sort (dirs first).
- `src/panels/RepoTreePanel.tsx` — expand a repo node to its files: on expand, call
  `listRepoTree(repo)` (loading state), build the tree, collapsed-by-default folders,
  truncated notice; mark changed files (live from `status` via `useBusState`); file node
  double-click / Enter/Space → `openDiff(repo, path)`.
- `src/panels/RepoCard.tsx` — make changed-file entries (expanded view) activatable →
  `openDiff`. `src/panels/RepoPanel.tsx` — make the status-list entries activatable →
  `openDiff` (currently inert `<li>`s).
- `src/panels/tree/fileTree.test.ts` *(new)*; extend `RepoTreePanel.test.tsx`,
  `RepoCard.test.tsx`, `RepoPanel.test.tsx`.

**Tests:** `fileTree` builds correct nesting + change marks + ordering; expansion shows
loading → entries; truncated notice; markers update on a later status delta; activating a
file (tree / card / repo panel) calls `openDiff` with the right `(repo, path)`; dedup
(second activation focuses, not duplicates — asserted via the openDiff/panel mock).

**Verify:** vitest + tsc/eslint. **Depends on:** U1 (listRepoTree), U4 (openDiff).

### U6 — Integration, restore, verification

**Files**
- `src/App.tsx` — register `PANEL_DIFF` in the components map; provide `openDiff` in
  `WorkspaceActions`; instantiate the `SubscriptionReconciler`; bind dockview
  `onDidAddPanel`/`onDidRemovePanel` (filtered to diff panels) to reconciler add/remove.
  **Restore batching rides the microtask coalescer (KTD3/R6a):** `api.fromJSON()` fires N
  `onDidAddPanel` events during restore; the reconciler accumulates them and flushes a
  single `set_subscriptions` on the next microtask, cap applied — no new `onRestore` seam on
  `DockWorkspace` is needed (the existing async restore emits the add events the coalescer
  absorbs).
- `src/App.test.tsx` — extend: drill-through opens a diff panel; restore with diff panels
  issues one batched `set_subscriptions`; >8 restore bounds to 8 + live-paused.
- `src/App.css` — finalize diff styling.

**Tests:** integration of open→subscribe→live→close→unsubscribe; restore batching + cap;
cross-repo cap (5+4 → 8). **Verify (full gate):** `cargo test` (unchanged, 114),
`npx vitest run`, `tsc --noEmit`, `eslint`, `cargo clippy -D warnings`, `cargo fmt
--check`, `tauri dev` boot smoke (diff renders, live update visible, no panic),
`tauri build` exit 0. **Depends on:** U1–U5.

## Sequencing & dependencies

```
U1 ──▶ U2 ──┐
   ├──▶ U3 ──┴─▶ U4 ──▶ U5 ──▶ U6
   └────────────────────▶ U5   (U5 also uses U1: listRepoTree + TreeEntry)
```

U1 first (types/wrappers). U2 (store slice + reconciler) is the correctness gate — its
tests must pass before U4 builds on it. U3 (renderer) only needs U1, so it can proceed
alongside U2. U4 needs U2+U3. U5 needs U4 (openDiff) + U1. U6 integrates + verifies.

## Risks & mitigations

- **Transient-diff blanking (R5a)** — highest risk; pinned by U2's gate test (AE5) before
  anything renders.
- **Global cap mechanics** — reconciler owns the bound; cross-repo + restore covered by
  tests (AE11/AE12).
- **Shiki bundle/async** — lazy `shiki/core`; structure-first render; cap guard. If the
  bundle balloons, fall back to a smaller highlighter — does not change the contract or
  the renderer's data path.
- **jsdom can't lay out dockview / Shiki async** — same approach as RDM-007: mock dockview,
  test wiring + pure logic; the live visual is the `tauri dev` smoke gate.
- **Contract gap surfacing** — KTD1 says STOP + escalate; do not amend the frozen contract
  inline.

## Verification criteria (unit acceptance → AE mapping)

- U1 → D-008-5 concrete-types round-trip test (PascalCase `kind`); underpins AE3/AE13.
- U2 → AE5 (no-blank), AE6/AE11/AE12 (reconciler bound/coalesce/restore).
- U3 → AE1, AE2, AE3, AE13 (render modes, highlight fallback, large file).
- U4 → AE4, AE5 (panel-level), AE7, AE10 (states), R7 initial load.
- U5 → AE8 (tree), AE9 (drill-through dedup).
- U6 → AE11/AE12 integration + full build/smoke.

Done when: all AE1–AE13 covered by tests, full verification gate green, `tauri dev`
shows a live diff updating as a file changes and a side-by-side toggle, `tauri build`
produces bundles.
