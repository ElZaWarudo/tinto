# RDM-008 — Diff viewer + Live diff — Requirements

- **Date:** 2026-06-15
- **Roadmap item:** RDM-008 (`docs/roadmaps/2026-06-10-001-tinto-roadmap.md`)
- **Depends on (shipped):** RDM-006 (frozen bus contract) · RDM-007 (dockable shell, panel registry, bus store/client, `WorkspaceActions`)
- **Production posture:** prototype, solo dev, no users
- **Language:** English (repo directive, Wave 4)
- **Review:** document-review gate passed (coherence, feasibility, scope-guardian, design-lens, adversarial). All P0/P1 folded in; see "Review hardening" at the end.

## Problem & value

Tinto's core promise is *seeing the change while it happens*. Waves 1–7 built the
backend (git engine, classifier, workbenches, watcher, state/event bus) and the
first frontend (dockable workspace with live repo cards, repo panels, a
repo-node tree). What's missing is the payload: an actual **diff viewer**. RDM-008
delivers the comfortable diff surface — structured hunks rendered as a VS Code–style
review, **live-updating while an agent writes**, with a **full-file view** that shows
changes in context rather than only the isolated hunk.

The backend already serves everything this needs (the frozen contract's dry-run
table maps each RDM-008 need to an existing command/event — no contract change). The
work is concentrated in the **frontend**, including one non-trivial store change for
live diffs (see R5/R5a).

## User-stated UX direction

> "I see it like a VS Code line reviewer — you can see the previous line and then
> the next line; it'd be interesting to see it stacked under the same line, or as a
> left/right comparator."

This pins the two render modes: **inline** (removed line followed by the added line,
stacked) and **side-by-side** (old on the left, new on the right). Default to inline.

## Locked decisions (from this brainstorm)

- **D-008-1 — Custom renderer.** Render diffs from the backend's already-structured
  `DiffHunk`/`DiffLine` data (per-side line numbers, `Added`/`Removed`/`Context`
  kinds). No diff-parsing library: the backend gives structure, not unified-diff
  text, so a custom component is the natural fit and keeps deps lean.
- **D-008-2 — Shiki for syntax highlighting.** TextMate grammars (the engine VS Code
  uses) to match the existing `themeVisualStudio` aesthetic. Lazy-loaded; bounded by
  file size to contain the large-file performance risk (plain monospace fallback
  above the cap and for binary/oversized files). Highlighting layers on *after* the
  diff structure renders — never on the critical path to first paint.
- **D-008-3 — Both view modes, default inline.** Inline + side-by-side from one
  structured data source, with a per-panel toggle (see R2 for placement/persistence).
- **D-008-4 — Live diff via subscriptions.** The open diff target subscribes through
  `set_subscriptions`; updates arrive on `workbench-delta.subscribed_diffs`. No
  polling. The subscribed set is **global across all repos, cap 8** (contract), so
  the frontend owns one reconciled set derived from open panels (see R6/R6a/R12).
- **D-008-5 — Concrete diff types in `contract.ts`.** The first implementation unit
  replaces `FileDiff.hunks: unknown[]` (`src/bus/contract.ts:49`) with the concrete
  `DiffHunk`/`DiffLine` types, including a round-trip test pinning the PascalCase
  `kind` serialization (`"Added"|"Removed"|"Context"`). Additive, no new file.

## Scope

### In scope

- A **Diff panel** type registered into the existing dockview registry, addressable
  by `(repo, path)` so a restored layout reopens the same diff and the same target
  never opens twice (dedup by id, reusing the `openRepoPanel` pattern).
- **Plane 1 diff render** of a file's working-tree change: inline and side-by-side,
  syntax-highlighted, from structured hunks.
- **Full-file view** (`get_file_content`) with changed lines highlighted in context,
  not only the hunk window — a mode of the Diff panel.
- **Live diff:** while a diff target is open, it updates automatically as the agent
  writes, via `set_subscriptions` → `subscribed_diffs` on the delta stream.
- **Untracked / new-file diff:** an all-added file renders from the synthesized
  `subscribed_diffs` (binary/size guards already enforced by the backend).
- **File-level tree expansion** (RDM-007 deferred): the repo-node tree expands to
  files via `list_repo_tree`, with changed files marked from `status` path lists.
- **Drill-through:** open a diff from (a) a repo card / repo panel changed-file entry,
  and (b) a file node in the tree. Routed through `WorkspaceActions` (extended).
- **Diff panel states:** loading, no-changes (clean), load error + retry, binary,
  truncated, repo-not-allowed, renamed-target, and live-paused (over-cap).

### Out of scope (deferred)

- Plane 2 watched-files list (RDM-009).
- Aggregated metrics / passive signals / change highlighting heuristics (RDM-011).
- Commit-diff browsing & timeline navigation (RDM-010) — RDM-008 renders the
  **working-tree** diff; `get_commit_diff`/`get_blob` history browsing is RDM-010.
- Any write operation (Tinto is read-only, §9).
- Word/intra-line character diff (only line-level for the prototype).
- Virtualized line rendering (see OQ2 — resolved out for the prototype).

## Requirements

### Diff rendering

- **R1.** A Diff panel renders the working-tree diff of a `(repo, path)` target from
  structured hunks: context/added/removed lines with correct per-side line numbers.
- **R2.** The panel supports **inline** (stacked removed→added) and **side-by-side**
  (left old / right new) modes via a visible two-button control in the panel's own
  header bar, default inline. The selected mode is panel-local React state and
  **does not** need to survive restart (the panel identity persists; the mode resets
  to inline on reopen — acceptable for the prototype).
- **R3.** Lines are **syntax-highlighted** (Shiki) by the file's language, inferred
  from extension; unknown languages and over-cap files fall back to plain monospace.
- **R4.** A **full-file view** shows the file's current content (`get_file_content`)
  with changed lines highlighted in their surrounding context. The view selector is a
  segmented control **"Hunks · Full file"** in the panel header; the inline/side-by-side
  toggle is disabled (greyed) while Full file is selected. Full-file degrades safely:
  `base64`/binary → the binary guard placeholder (no line mapping); `truncated` → show
  content up to the 1 MiB cut with a "file truncated" notice and **no** highlight past
  the cut; if the fetched content and the current diff revision are skewed mid-edit,
  best-effort render is acceptable (no correctness guarantee on the exact highlighted
  line during an active write).

### Live diff & subscriptions

- **R5.** While a diff target is open, the diff **updates live** as the file changes,
  driven by `subscribed_diffs` on `workbench-delta`; no polling, no manual refresh.
  This requires a **dedicated store slice** for diffs (e.g. `diffs: Record<repo,
  Record<path, FileDiff>>`), because `subscribed_diffs` is a transient per-emit field
  the existing whole-delta `applyDelta` would otherwise discard.
- **R5a.** The diff slice **retains the last-known `FileDiff`** for a target across
  deltas that omit it. A delta with `subscribed_diffs == null` means "no diff update
  this delta" (a status-only recompute, transient error, etc.) — it must **not** blank
  an open diff. The diff is only replaced by a newer `subscribed_diffs` for that
  target, and only cleared on an explicit clean signal (target present in a fresh
  recompute with no changes) or when the panel closes / repo leaves the workbench.
- **R6.** Subscriptions are **managed by panel lifecycle** as a single reconciled set:
  the active set is *derived* from the currently-open diff panels (not imperative
  per-panel add/remove), and pushed via `set_subscriptions`.
- **R6a.** Subscription pushes are **idempotent and coalesced**: transient mount churn
  (React StrictMode double-mount, dockview drag re-parent) must not produce redundant
  `set_subscriptions` calls or backend recalcs. The frontend diffs the desired set
  against the last-pushed set and pushes only on real change (debounced).
- **R7.** **Initial load is mandatory and branches on tracked vs untracked**, because a
  subscription "applies from the next recomputation" and an *idle* file may never
  recompute (so subscribe-and-wait alone can hang in "loading"):
  - **Tracked file:** on open, fire a one-shot `get_worktree_diff(repo)` and pick the
    target's `FileDiff` to paint immediately, *and* subscribe for live updates.
  - **Untracked file:** `get_worktree_diff` excludes untracked, so the one-shot is
    empty; rely on the subscribe-triggered recompute to deliver the synthesized
    all-added `subscribed_diffs`. Show "loading" until it arrives.
  - **Reconciliation:** the one-shot result only fills the slice if no live
    `subscribed_diffs` has arrived for that target yet; once a live delta arrives it
    is authoritative thereafter (the one-shot, which carries no revision, never
    overwrites a live diff).
- **R8.** The repo tree **expands to files** (`list_repo_tree`), building a folder tree
  from the flat entries; changed files are marked with a letter badge matching the
  card convention (M/S/U via the existing `--warn`/`--added` CSS vars), derived from
  the repo's `status`. Markers **update live** as `status` changes while the tree is
  open. Folders are **collapsed by default**; expanding a repo shows a loading state
  while `list_repo_tree` is in flight; a `truncated` tree shows a clear notice.
- **R9.** **Drill-through:** activating a changed-file entry opens (or focuses) that
  file's Diff panel. The activation gesture matches the established `RepoCard` pattern
  — **double-click**, plus `Enter`/`Space` when focused — for card and repo-panel file
  entries and for tree file nodes. Dedup is by target id (an already-open target is
  focused, not reopened).

### States, persistence, performance

- **R10.** The Diff panel handles all states with explicit content: **loading**
  (spinner/message), **clean** (muted "No changes" message; the Full-file view stays
  accessible so an unchanged file is still readable), **load error** (the contract's
  error category + a retry action), **binary** (guard placeholder), **truncated**
  (notice), **repo-not-allowed** (`repo-not-allowed` message), and **renamed-target**
  (see R11). State copy is prototype-light but each state renders a distinct,
  asserted message.
- **R11.** Diff panels are **layout-persistent**: a Diff panel id encodes its target
  (`diff:${repo}:${path}`) so the saved dockview layout reopens the same diff after
  restart. On restore, all diff panels are reconciled through **one batched**
  `set_subscriptions` (per R6), with the R12 cap applied at restore time — never N
  independent subscribe calls. **Renamed target:** if a panel's encoded path no longer
  matches a live target (the file was renamed; the backend matches by current `path`),
  the panel shows a "file was renamed — reload" state rather than silently going stale;
  reopening from the tree (new path) is the recovery.
- **R12.** The **global** subscribed set is bounded to **8** (contract cap, across all
  repos — not per-repo, not per-panel). The frontend computes the bounded set itself
  (most-recently-opened wins; do not rely on the backend's non-deterministic
  `truncate`). Panels outside the bound enter a **live-paused** state: a static
  "Live updates paused (subscription limit) — reload" notice with a manual reload that
  does a one-shot `get_worktree_diff`/`get_file_content`. No silent staleness; no
  automatic re-fetch loop.
- **R13.** Large files stay responsive: highlighting and diff rendering are bounded by
  a **size cap with a plain-monospace + "large file" notice** fallback (no
  virtualization for the prototype). The requirement is **no UI freeze on a big file**.

## Acceptance criteria

- **AE1.** Opening a changed file's diff shows hunks with context/added/removed lines
  and correct old/new line numbers (verified against the structured data).
- **AE2.** Toggling inline ↔ side-by-side re-renders the same change in both layouts
  from one data source; default is inline.
- **AE3.** A known-extension file is syntax-highlighted; an unknown extension or an
  over-cap file renders as plain monospace without error.
- **AE4.** The full-file view shows current content with changed lines marked in
  context; switching Hunks ↔ Full file preserves the target; selecting Full file
  disables the inline/side-by-side toggle; a base64/truncated file degrades per R4.
- **AE5.** Editing a file that has an open diff panel updates the rendered diff via the
  delta stream with no manual refresh; **a subsequent status-only / transient-error
  delta for that repo (carrying `subscribed_diffs == null`) does NOT blank the open
  diff** (R5a).
- **AE6.** The active `set_subscriptions` call reflects exactly the set derived from
  open diff panels, bounded to 8 across all repos; transient remount (StrictMode) does
  not produce redundant calls (R6a).
- **AE7.** An untracked file opens as an all-added diff (after the subscribe-triggered
  recompute); a binary/over-size file shows a guard placeholder.
- **AE8.** Expanding a repo in the tree lists its files (folders collapsed by default,
  foldable), shows a loading state while fetching; changed files are marked and the
  markers update on a later `status` delta; a truncated tree shows the notice.
- **AE9.** Double-clicking (or `Enter`/`Space` on) a changed-file entry on a card/repo
  panel and a tree file node both open/focus the **same** file's Diff panel (dedup by
  target).
- **AE10.** Each state (loading, clean, load error+retry, binary, truncated,
  repo-not-allowed, live-paused, renamed) renders its intended distinct message,
  asserted with inline vitest assertions (no shared test harness needed).
- **AE11.** Restarting with one or more open Diff panels restores them and issues
  **exactly one** batched `set_subscriptions`; restoring >8 diff panels bounds the set
  to 8 and the over-cap panels restore in live-paused mode.
- **AE12.** Opening more than 8 diff panels — including a cross-repo mix (e.g. 5 in
  repo A + 4 in repo B) — keeps the app responsive, keeps the subscribed set at 8, and
  the over-cap panels show the live-paused notice and load on demand.
- **AE13.** A large file (above the highlight/size cap) renders without freezing the UI
  (plain-monospace + notice fallback) — asserted at least at the cap-decision level.

## Contract mapping (no contract change)

| Need | Served by |
|---|---|
| Initial diff of a tracked file | `get_worktree_diff(repo)` → pick target path (one-shot, R7) |
| Live diff of the open target | `set_subscriptions([{repo, path}])` → `workbench-delta.subscribed_diffs` |
| Initial diff of an untracked file | subscribe-triggered recompute → synthesized all-added `subscribed_diffs` (R7) |
| Full-file current content | `get_file_content(repo, path)` (utf8/base64, truncated guard) |
| File-level tree | `list_repo_tree(repo)` (flat entries + `truncated`) |
| Changed-file marking | `workbench-delta.status` path lists |
| Error/edge categories | command `{category,message}`; `repo-not-allowed`, `path-*`, binary/truncated in `FileContent` |

RDM-008 subscribes **file targets only** (`{repo, path}`), never whole-repo
(`{repo, path:null}`) — a whole-repo target returns the full `worktree_diff` in
`subscribed_diffs` (heavy payload) and is not needed here.

Diff data shape (from `src-tauri/src/git/mod.rs`, serialized): `FileDiff { path,
old_path, is_binary, hunks: DiffHunk[] }`; `DiffHunk { old_start, new_start, lines:
DiffLine[] }`; `DiffLine { kind: "Added"|"Removed"|"Context", content, old_lineno,
new_lineno }`. The enum serializes **PascalCase** — the TS types added per D-008-5
must match exactly (round-trip test).

## Open questions (for planning)

- **OQ1. [resolved → R7]** Initial-load is mandatory, tracked/untracked-branched, with
  the one-shot-vs-live reconciliation rule. No longer open.
- **OQ2. [resolved → R13/out-of-scope]** Large-file technique: **cap + notice**;
  virtualization is out of scope for the prototype — revisit only if post-ship
  profiling shows a freeze below the cap.
- **OQ3.** Exact MRU bookkeeping for the global cap (R12): how "most-recently-opened"
  is tracked and how a panel transitions live ↔ live-paused as others open/close.
  (Policy is global-MRU; the mechanics are a plan detail.)
- **OQ4.** Shiki packaging — full bundle vs `shiki/core` with lazy `getHighlighter`
  + on-demand grammar/theme imports — to keep the bundle reasonable under Vite 7.
- **OQ5. [resolved → D-008-5]** Concrete `hunks` typing in `contract.ts` with the
  PascalCase round-trip test. No longer open.

## Risks

- **Live-update correctness** (the load-bearing risk). `subscribed_diffs` is transient
  per-emit; mitigated by the dedicated retaining store slice (R5/R5a). This is the
  single most important thing to get right and to test (AE5).
- **Global cap vs many open diffs** — addressed by R12 (frontend-owned bound +
  live-paused state) and AE11/AE12 (incl. cross-repo and restore cases).
- **Live-update + highlighting on large files** (roadmap: medium). Mitigated by D-008-2
  size cap, plain fallback, and R13's no-freeze requirement.
- **Shiki bundle/async cost** — lazy-load grammars; load is off the diff-structure
  critical path (structure renders first, highlighting layers on).
- **Rename / mid-edit skew** — named states (R11 renamed, R4 skew best-effort) rather
  than silent staleness.

## Review hardening (applied)

From the document-review gate (5 personas):

- **Adversarial P0-1 → R5/R5a + AE5:** backend defaults `subscribed_diffs` to `None` on
  every non-fresh-recalc emit path; the store's whole-delta replace would blank an open
  diff. Added the retaining diff slice + the "null = no-update" rule + the explicit
  no-blank assertion.
- **Adversarial P0-2 → R12/D-008-4 + AE12:** the cap is a *global* set of 8 across repos;
  the frontend owns the bound (backend `truncate` is non-deterministic). Added the
  cross-repo AE12 scenario.
- **Adversarial P1-4 / P2-8 + feasibility/coherence OQ1 → R7:** one-shot initial load is
  mandatory and tracked/untracked-branched (idle files never recompute; `get_worktree_diff`
  excludes untracked) with a one-shot-vs-live reconciliation rule.
- **Adversarial P1-3 → R11/AE11:** layout restore funnels through one batched
  `set_subscriptions` with the cap applied.
- **Adversarial P1-5 → R6/R6a:** single reconciled subscription set, idempotent/coalesced
  pushes (StrictMode/drag churn).
- **Adversarial P2-6 → R11 renamed state; P2-7 → R4 full-file degrade rules; P3-9 →
  file-targets-only note; P3-10 → D-008-5 round-trip test.**
- **Design P0-1..P0-4 → R2 (toggle placement/persistence), R4 (Hunks·Full-file segmented
  control), R8/R9 (gesture + markers), R12 (live-paused visual); P1-1..P1-3 → R10/R8/R12
  state content.**
- **Coherence → AE13 added; "Plane 1/2" spacing; OQ1 reconciled in the contract table.**
- **Scope-guardian → dropped the "reusable for RDM-010" framing; OQ2 closed against
  virtualization; OQ5 promoted to D-008-5; R12 fallback narrowed to a manual reload;
  AE10 inline-assertions note.**
