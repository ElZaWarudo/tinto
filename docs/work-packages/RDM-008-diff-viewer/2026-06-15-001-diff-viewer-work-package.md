---
title: Diff viewer — structured render, live diff, full-file view, tree drill-through
status: pr-opened
roadmap_item: RDM-008
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-15-rdm-008-diff-viewer-requirements.md
origin_planning_input: docs/brainstorms/2026-06-15-rdm-008-diff-viewer-requirements.md
origin_plan: docs/plans/2026-06-15-002-feat-diff-viewer-plan.md
units: [U1, U2, U3, U4, U5, U6]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Diff viewer — structured render, live diff, full-file view, tree drill-through

## Scope

Implement the diff viewer per the origin plan: a custom renderer over the backend's structured
`DiffHunk`/`DiffLine` data (inline + side-by-side, Shiki syntax highlighting), a full-file view with
changed lines in context, **live diff** that updates while an agent writes (via `set_subscriptions` →
`workbench-delta.subscribed_diffs`), **file-level tree expansion** (`list_repo_tree`), and **drill-through**
to open a diff from a repo card / repo panel changed-file entry and from a tree file node. A new Diff panel
type is registered into the existing dockview registry; the bus store gains a diff slice; a subscription
reconciler owns the global cap-8 subscribed set. Includes the RDM-008 planning artifacts (brainstorm, plan)
and this package on the same branch.

**No backend change.** The frozen contract already serves every need; the `cargo` surface is untouched
(stays at 114 tests). All code is in `src/` plus one dependency (`shiki`).

## Non-goals

- Commit-diff / history browsing and timeline navigation (`get_commit_diff`/`get_blob` history) — RDM-010.
  (The renderer is a plain component; if RDM-010 later reuses it, good — but that is not a goal here.)
- Plane-2 watched-files list / `fs_watch` editing (RDM-009).
- Aggregated metrics / passive signals / change-detection heuristics (RDM-011).
- Word/intra-line character diff (line-level only).
- Virtualized line rendering (size cap + plain fallback instead — brainstorm OQ2 resolved out).
- Any write operation (Tinto is read-only).

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: component/file naming, test organization, CSS/diff styling within the
  dark dev-tool aesthetic, the size cap value for highlighting/large-file fallback, the debounce window for
  the subscription coalescer, Shiki packaging detail (`shiki/core` lazy grammars — OQ4), the MRU
  bookkeeping mechanics (OQ3), fixture choices.
- Agent must record as assumptions: the resolved `shiki` version; the exact size cap chosen; any
  contract-shape detail discovered while typing `DiffHunk`/`DiffLine`; the precise loading→clean transition
  signal used.
- Agent must escalate: any contract gap that blocks a need (would mean the frozen contract is insufficient —
  a product/contract decision; do NOT amend the frozen contract inline — KTD1); behavior changes in
  delivered modules beyond the additive diff-slice change to `store.ts`; branch/base strategy; Jira/PR
  workflow; scope outside the package (009+ features).
- Safe fallback: U2 (diff slice + reconciler) is the internal correctness gate — if its tests (retain on
  null, clean-clear by omission, cap/coalesce) do not pass, stop and report before building U4 on it. If the
  `shiki` bundle balloons, fall back to a lighter highlighter (does not touch the contract or the renderer
  data path) and record it.
- Autonomous ledger: none
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-006 ✅ (frozen contract: `get_worktree_diff`/`get_file_content`/`list_repo_tree`/
  `set_subscriptions` + `subscribed_diffs` on the delta), RDM-007 ✅ (dock shell, panel registry,
  `WorkspaceActions`, bus store/client) — both on `develop`.
- Blocks: RDM-011 (passive signals highlight in the diff).

## Production Posture

- Posture: prototype — greenfield, no users. Confidence: high.
- Evidence: user statement; the diff surface is net-new (RDM-007 shipped the shell without it).
- Consequences for this package: speed/flexibility allowed; dark dev-tool aesthetic acceptable; no
  backward-compat constraints (no prior diff UI).
- Breaking existing behavior allowed: limited — the only change to delivered code is the **additive** diff
  slice + reconciler wiring in the bus store / App shell and making existing inert changed-file entries
  (`RepoCard`/`RepoPanel`/`RepoTreePanel`) activatable. No backend behavior changes.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Concrete diff/tree/content TS types + client wrappers (D-008-5). |
| U2 | yes | Diff store slice + subscription reconciler — internal correctness gate. |
| U3 | yes | Diff renderer (inline + side-by-side + Shiki highlighting). |
| U4 | yes | Diff panel (lifecycle, R7 initial load, live, all states). |
| U5 | yes | File-tree expansion + drill-through wiring. |
| U6 | yes | Integration (registry, openDiff action, restore batching) + verification. |

Grouping rationale:
- **Single RU.** The types (U1), store slice + reconciler (U2), renderer (U3), panel (U4), tree/drill-through
  (U5), and integration (U6) are tightly coupled around one cohesive surface — the diff viewer — and share
  core files (`store.ts`, `panels.ts`, `actions.tsx`, `App.tsx`). Splitting would produce either a
  no-visible-value foundation PR or noisier stacked PRs, consistent with the user's recorded preference for
  fewer cycles before visible value (RDM-007). The principal risk (the live-diff store invariant) is handled
  **internally** by the U2 gate: its tests (retain-on-null, clean-clear-by-omission, cap/coalesce/restore)
  must pass before U4 builds on it.
- **Size:** likely a large RU (estimated ~800–1,000+ human-authored lines incl. tests; a meaningful share is
  Vitest). Per the size guardrail this requires an explicit decision — carried to the release plan as a
  `Decisión de tamaño/alcance` (`aprobar PR grande` if it lands >900 human lines). Planning artifacts ride
  the branch in a separate docs commit; the `shiki` dep lands in its own/deps commit; no generated artifacts
  dominate.

## Implementation Units

(Full detail in `docs/plans/2026-06-15-002-feat-diff-viewer-plan.md`.)

- U1. Concrete `DiffLineKind`/`DiffLine`/`DiffHunk` + `SubscriptionTarget`/`TreeEntry`/`RepoTree`/
  `FileContent` types in `contract.ts` (replace `hunks: unknown[]`); client wrappers `getWorktreeDiff`,
  `getFileContent`, `listRepoTree`, `setSubscriptions`. PascalCase round-trip test (D-008-5).
- U2. Bus store diff slice (retain-on-null, replace-set on non-null → clean-clear by omission;
  `getDiff`/`hasComputedDiffs`/`dropDiff`); `SubscriptionReconciler` (global MRU cap 8, coalesced
  idempotent `set_subscriptions`, batched restore, live vs live-paused). **Internal gate.**
- U3. `DiffView` renderer (inline + side-by-side from one data source, per-side line numbers, theme-var
  styling, binary/large-file fallbacks) + lazy Shiki `highlight.ts` (size cap, language-from-extension).
  Adds `shiki`.
- U4. `DiffPanel` (registers target with reconciler; R7 tracked/untracked initial load; live read from the
  slice; loading→clean signal via `hasComputedDiffs`; `Hunks · Full file` segmented + inline/side-by-side
  toggle; full-file via `get_file_content` with R4 degrade rules; all R10 states incl. live-paused +
  renamed; unmount drops slice + target). `PANEL_DIFF`/`diffPanelId`/`openDiff` wiring.
- U5. `fileTree.ts` builder; `RepoTreePanel` file-level expansion (loading, collapsed default, truncated
  notice, live change markers); make `RepoCard`/`RepoPanel` changed-file entries activatable
  (double-click + Enter/Space) → `openDiff`.
- U6. `App.tsx` integration: register `PANEL_DIFF`, provide `openDiff`, bind `onDidAddPanel`/
  `onDidRemovePanel` to the reconciler, restore batching via the microtask coalescer. Full verification.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Full diff viewer (U1–U6) + planning docs | frontend (`src/bus/{contract,client,store}.ts`, `src/workspace/{panels,openDiff,actions,subscriptions}.ts(x)`, `src/panels/diff/**`, `src/panels/tree/**`, `src/panels/{RepoCard,RepoPanel,RepoTreePanel}.tsx`, `src/App.tsx`, `src/App.css`), deps (`package.json` + `shiki`), tests (Vitest), docs (`docs/**`) | develop | optional: "Jira omitted" unless configured | ~800–1,000+ human lines (meaningful share tests); single cohesive surface; live-diff invariant gated internally (U2); large-PR decision carried to release plan; deps/docs in separate commits |

Rules applied: planning docs ride the branch (brainstorm + plan), committed separately from code; the
live-diff store invariant is an internal U2 gate, not a separate PR; no backend change (no cargo surface).

## Files and Tests

- Frontend code: `src/bus/{contract,client,store}.ts`, `src/workspace/{panels.ts,openDiff.ts,actions.tsx,subscriptions.ts}`,
  `src/panels/diff/{DiffView,DiffPanel,FullFileView}.tsx`, `src/panels/diff/highlight.ts`,
  `src/panels/tree/fileTree.ts`, `src/panels/{RepoCard,RepoPanel,RepoTreePanel}.tsx`, `src/App.tsx`,
  `src/App.css` (+ optional `src/panels/diff/diff.css`).
- Rust: **none** (no backend change — KTD1).
- Deps: `package.json` (+ `shiki`).
- Tests: Vitest — `src/bus/contract.test.ts` (new), `src/bus/store.test.ts` (extend),
  `src/workspace/subscriptions.test.ts` (new), `src/panels/diff/DiffView.test.tsx` (new),
  `src/panels/diff/DiffPanel.test.tsx` (new), `src/panels/tree/fileTree.test.ts` (new),
  `src/panels/{RepoTreePanel,RepoCard,RepoPanel}.test.tsx` (extend), `src/App.test.tsx` (extend).
- AE coverage: AE1–AE13 each map to a named plan test scenario (see plan "Verification criteria").

## Impact Scan

- Changed API contracts: **none on the backend contract** (frozen; consumed read-only). No new Tauri
  commands. Frontend-internal: `BusState` gains a `diffs` slice (additive); `WorkspaceActions` gains
  `openDiff` (additive); `FileDiff.hunks` is typed concretely (was `unknown[]` — additive precision).
- Behavior change in delivered code: changed-file entries in `RepoCard`/`RepoPanel`/`RepoTreePanel` become
  activatable (previously inert) — additive interaction, no removal.
- Consumer scan patterns: `rg "subscribed_diffs|hunks: unknown" src/`; `rg "WorkspaceActions|openRepo\b" src/`;
  `rg "applyDelta|BusState" src/`.
- Consumers found: the store's `applyDelta` is the single delta entrypoint (extend in place); `WorkspaceActions`
  consumers are the panels (add `openDiff` to the noop default + provider). No external consumers of `hunks`
  yet (typed `unknown[]` precisely because RDM-008 is the first consumer).
- Contract-drift tests searched: existing Vitest suites (store, connection, layout, panels, workbench, App) +
  cargo (unchanged). Update any store test asserting the old `BusState` shape.
- Required consumer tests: full `npm test` (existing 58 stay green + new suites) + full `cargo test` (114,
  unchanged).
- Consumer tests run/skipped: run in the Verification Gate.

## Verification Gate

- `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test` (unchanged, **114**) ·
  `cargo build` · `npm run lint` · `tsc --noEmit` · `npm test` (existing 58 + new diff/reconciler/tree
  suites) · `tauri dev` boot smoke covering a live diff updating as a file changes + the inline/side-by-side
  toggle + tree drill-through · `tauri build` exit 0.
- Surface-aware evidence: live-diff invariant → `store.test`/`subscriptions.test` (retain/clean-clear/cap);
  renderer → `DiffView.test`; panel lifecycle/states → `DiffPanel.test`; tree/drill-through →
  `fileTree.test` + panel tests; integration/restore → `App.test` + dev smoke; highlighting/large-file →
  `DiffView`/`highlight` cap-boundary tests + dev smoke (Shiki async not driven in jsdom).
- Production posture evidence: prototype; Linux/WSL dev smoke; no cross-platform matrix (D2). The
  interactive live-diff click-through (edit a file, watch the open diff update) is the dev-smoke gate, since
  the headless env can't drive the webview.

### Verification result (2026-06-15)

PASS. Review-fix tests landed and the full gate re-ran green:

- `npm test` → **106/106** Vitest tests.
- `npm run lint` → PASS.
- `npm run format:check` → PASS.
- `npm run build` → PASS (`tsc` + Vite; Shiki languages/theme/wasm remain lazy chunks).
- `cargo fmt --check` → PASS.
- `cargo clippy --all-targets -- -D warnings` → PASS.
- `cargo test` → **114/114** Rust tests.
- `cargo build` → PASS.
- `npm run tauri build` → PASS; produced deb/rpm/AppImage bundles.
- `npm run tauri dev` boot smoke → PASS for startup/process stability during the observed run; the GUI was
  opened and then stopped. The headless harness did not drive an interactive edit/click-through; live-diff
  behavior is covered by `store.test`, `subscriptions.test`, `DiffPanel.test`, panel drill-through tests, and
  `App.test`.

## Review Gate

- Code review threshold: P0-P2 (default). Findings below threshold: log unless user marks blocking.
- Suggested personas: correctness (the diff-slice dual rule — retain-on-null vs clean-clear-by-omission;
  revision gating; loading→clean signal; reconciler MRU/coalesce), adversarial (subscription lifecycle
  races, StrictMode double-mount, layout-restore thundering herd vs the cap, renamed-target staleness,
  full-file base64/truncated/skew, untracked idle/None-synth hang), testing (AE1–AE13 coverage, Shiki/invoke
  mocking, the clean-clear test as the mirror of AE5), maintainability (renderer/panel/store boundaries,
  contract-mirror drift, `shiki` lazy boundary).

### Code review result (2026-06-15) — 4 personas (correctness, adversarial, testing, maintainability)

Store slice + subscription reconciler judged solid and well-tested. **1 P1 + several P2** found; **fixes
complete** and re-verified:

| Severity | Finding | Fix |
|---|---|---|
| P1 corr | DiffPanel `live ?? oneShot` re-surfaced a stale reverted diff (the one-shot fell back after the slice cleared by omission), defeating the clean-clear invariant | precedence now `hasComputedDiffs ? live : (live ?? oneShot)` + revert-after-live test |
| P2 corr/adv | paused untracked panel showed a false "no changes" + a dead Reload, or a permanent fake spinner | when `!isLive` with no cached diff, render a distinct paused body message (not loading/empty) |
| P2 adv | diff panels for a removed repo orphaned into stale "live" views | `App.removeRepo` now closes the repo's `diff::<repo>::*` panels |
| P2 corr/test | no distinct renamed-target state (AE10 unmet) | lightweight renamed state via `FileDiff.old_path` (`diff-renamed`) |
| P2 test | full-file degrade paths (base64/binary, truncated) untested (AE4) | adding `FullFileView.test.tsx` |
| P2 maint | `reconciler.restore()` was dead code (restore rides the panel-mount coalescer) | deleted; coalescing/cap still covered by the burst + cap tests |
| P3 corr | FullFileView trailing-newline produced a spurious blank line | drop a single trailing empty line |

**Deferred with rationale (recorded, not blocking):** manual-reload cancellation race (P3, prototype-OK);
full-file/diff revision skew (accepted per R4, documented in code); DiffPanel `useDiffData` extraction +
S/M/U mark consolidation (discretionary); reconciler `liveKeys` perf (negligible at cap 8); workbench-switch
diff-panel orphan (bigger flow — defer).

Review-fix coverage added/strengthened:

- `DiffPanel.test.tsx`: reverted-after-live does not resurface the one-shot; live-before-one-shot remains
  authoritative; paused-without-cache renders the paused body instead of false clean/loading; renamed-away
  target renders `diff-renamed`.
- `FullFileView.test.tsx`: utf8/trailing-newline, base64/binary guard, truncated notice, and load-error
  states.
- `subscriptions.test.ts`: cross-repo global cap assertion strengthened; deleted `restore()` coverage removed.
- `App.test.tsx`: removed-repo cleanup closes repo and diff panels for that repo only.

**Review PASS.** No P0-P2 findings remain open after the review-fix loop.

## Security Gate

- Required: no. Rationale: read-only local UI consuming the already-allowlisted bus commands; no new Tauri
  command, no new capability/permission, no new IPC surface. `get_file_content`/`get_worktree_diff`/
  `list_repo_tree` are already containment-guarded (allowlist + path-traversal + `.git` exclusion + bounded
  reads) in RDM-006; RDM-008 only consumes them. The adversarial review persona covers the frontend failure
  modes. If any new command or capability becomes necessary, escalate (would change this determination).
- One new dependency (`shiki`) enters the bundle: it runs in the webview, processes only local file content
  already exposed by the allowlisted read commands, and adds no network or native surface. Note for review,
  not a security gate.

## Reviewability Diagnosis

- Reviewer experience: one cohesive surface (the diff viewer) delivered as a single RU. A reviewer reads it
  bottom-up via the suggested commit grouping: types → store slice/reconciler → renderer → panel → tree/
  drill-through → integration. Each commit is independently legible; the load-bearing logic (the diff-slice
  dual rule + the reconciler cap/coalesce) is concentrated in `store.ts`/`subscriptions.ts` with dedicated
  tests, so the riskiest part is the easiest to review in isolation.
- Why not split into multiple PRs: the units share core files (`store.ts`, `panels.ts`, `actions.tsx`,
  `App.tsx`) and a foundation-only PR would have no visible value (consistent with the user's recorded
  preference, RDM-007). The principal risk is gated internally (U2 tests) rather than by a separate PR.
- Open-stack plan: none — single PR, no stacked/sibling PRs.
- Size note: likely a large PR (~800–1,000+ human lines incl. tests). The `Decisión de tamaño/alcance`
  (approve large PR if it lands >900 human lines) is carried to the release plan; deps (`shiki`) and docs
  land in their own commits so they don't obscure functional review.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Diff viewer (structured render + live diff + full-file + tree drill-through).
- Branch name: `feat/diff-viewer`
- PR: https://github.com/ElZaWarudo/tinto/pull/9
- Merge status: not merged. GitHub gate is clean, but PR #9 has no GitHub-visible human review approval on
  the normal `develop` base, so Release Marshal blocks merge despite the user's immediate merge request.
- Branch/docs rule: this single executable review unit carries the RDM-008 planning artifacts on the same
  branch; no separate docs branch.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `chore(frontend): add shiki dependency` — `package.json`, lockfile
  - `feat(bus): concrete diff/tree/content types and client wrappers` — `src/bus/contract.ts`,
    `src/bus/client.ts`, `src/bus/contract.test.ts`
  - `feat(bus): diff store slice and subscription reconciler` — `src/bus/store.ts`,
    `src/bus/store.test.ts`, `src/workspace/subscriptions.ts`, `src/workspace/subscriptions.test.ts`
  - `feat(ui): structured diff renderer with syntax highlighting` — `src/panels/diff/DiffView.tsx`,
    `src/panels/diff/DiffView.test.tsx`, `src/panels/diff/highlight.ts`
  - `feat(ui): diff panel with live updates and full-file view` — `src/panels/diff/DiffPanel.tsx`,
    `src/panels/diff/DiffPanel.test.tsx`, `src/panels/diff/FullFileView.tsx`,
    `src/workspace/{panels.ts,openDiff.ts,actions.tsx}`
  - `feat(ui): file-tree expansion and diff drill-through` — `src/panels/tree/fileTree.ts`,
    `src/panels/tree/fileTree.test.ts`, `src/panels/{RepoTreePanel,RepoCard,RepoPanel}.tsx` (+ their
    `.test.tsx` extensions)
  - `feat(ui): wire the diff panel into the workspace shell` — `src/App.tsx`, `src/App.css`,
    `src/App.test.tsx`
  - `docs: RDM-008 diff viewer brainstorm, plan, and work package` — `docs/**`
  (Group/merge as the diff settles; keep deps + docs in their own commits.)
- PR title: `Diff viewer: live, syntax-highlighted diffs with full-file view`
- PR body bullets: structured diff renderer (inline + side-by-side, Shiki highlighting) · live diff that
  updates while an agent writes (subscriptions) · full-file view with changes in context · file-level tree
  expansion + drill-through from cards/panels/tree · loading/clean/error/binary/truncated/live-paused/
  renamed states.
- Verification results location: Verification Gate of this package + execution thread.
- Production/deployment notes: none (prototype desktop app).
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: single standalone `Tarea` (one review unit, no multi-child parent).
- Jira summary: "Visor de diffs: diffs en vivo con resaltado de sintaxis y vista de archivo completo"
- Jira description: "Construir el visor de diffs de Tinto: render estructurado de cambios (modo en línea y
  lado a lado, con resaltado de sintaxis), diff en vivo que se actualiza mientras el agente escribe (vía
  suscripciones del bus), vista de archivo completo con los cambios en contexto, expansión del árbol a
  nivel de archivos y navegación desde tarjetas/paneles/árbol hacia el diff."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured"
  in state/release closeout and continue.
