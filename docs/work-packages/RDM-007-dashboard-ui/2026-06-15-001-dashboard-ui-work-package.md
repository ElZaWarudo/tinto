---
title: Dashboard UI — dockable workspace, repo cards, workbench management
status: ready
roadmap_item: RDM-007
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md
origin_planning_input: docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md
origin_plan: docs/plans/2026-06-15-001-feat-dashboard-ui-plan.md
units: [U1, U2, U3, U4, U5, U6, U7, U8]
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

# Dashboard UI — dockable workspace, repo cards, workbench management

## Scope

Implement the first Tinto frontend per the origin plan: a VS Code–style dockable workspace (dockview,
split/drag/tab-group, globally-persisted layout) consuming the frozen bus contract, with three panel
types (Dashboard card grid, per-repo detail tab, repo-level tree), a top-bar workbench switcher, Core
workbench curation (create/switch/add/remove), live data binding, and loading/degraded/error/edge states.
Includes the RDM-007 planning artifacts (brainstorm, plan) and this package on the same branch. Includes
a small RDM-005 correctness amendment: canonicalize the stored repo path on add so config paths match the
canonical paths in snapshot/deltas (KTD5).

## Non-goals

- Diff viewer, full-file view, live diff, card→diff / file→diff drill-through (RDM-008).
- File-level tree expansion and `list_repo_tree` usage (RDM-008).
- `set_subscriptions`-driven live diffs (RDM-008).
- Plane-2 UI / `fs_watch` editing (RDM-009), timeline (RDM-010), passive signals (RDM-011).
- Rename/delete workbench, `update_repo`/alias editing, per-workbench layouts.

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: component/file naming, test organization, CSS/styling details within
  the dark dev-tool aesthetic, exact dockview wiring, store shape, fixture choices, the pinned values
  already in the plan (commit-log page size N, activity window ~5s/fade ~2s, default layout).
- Agent must record as assumptions: resolved dockview-react version, the exact `dialog` permission token
  used, any contract-shape detail discovered while writing the TS mirror, the canonicalization fallback
  behavior on the workbench add path.
- Agent must escalate: any contract gap that blocks a panel (would mean the frozen contract is
  insufficient — a product/contract decision), behavior changes in delivered modules beyond the declared
  KTD5 canonicalization amendment, branch/base strategy, Jira/PR workflow decisions, scope outside the
  package (008+ features).
- Safe fallback: U1/U3 (deps, Rust command, bus client+store) do not need the dock engine; if dockview
  fails the U2 gate, report the exact decision (fall back library vs proceed) before building panels.
- Autonomous ledger: none
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-005 ✅, RDM-006 ✅ (both on `develop`; frozen contract + workbench/bus commands).
- Blocks: RDM-008, RDM-009, RDM-010 (the UI surfaces that add panels into this shell).

## Production Posture

- Posture: prototype — greenfield, no users. Confidence: high.
- Evidence: user statement; greenfield frontend (only ping/tick smoke exists).
- Consequences for this package: speed/flexibility allowed; dark dev-tool aesthetic acceptable as a first
  pass; no backward-compat constraints (no prior UI).
- Breaking existing behavior allowed: yes — the ping/tick smoke in `src/App.tsx` is replaced (the
  webview↔Rust bridge stays proven by the bus listeners + existing backend tests).

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Deps + Tauri wiring (dialog, ui_state command, KTD5 canonicalization). |
| U2 | yes | Dock shell + layout persistence — internal gate. |
| U3 | yes | Bus client: contract types, listeners, revision-gated store. |
| U4 | yes | Dashboard panel (cards). |
| U5 | yes | Repo panel (commit log + status list). |
| U6 | yes | Repo-node tree panel. |
| U7 | yes | Workbench switcher + first-run + curation. |
| U8 | yes | Integration: registry, default layout, keyboard, app shell. |

Grouping rationale:
- **Single RU.** The frontend shell, data layer, and panels are tightly coupled: panels render the U3
  store inside U2's dockview registry; the store, shell, and integration share core files. Splitting would
  produce either a no-visible-value shell PR (which the user explicitly chose against — "more cycles
  before visible value") or noisier stacked PRs. The dock-engine risk (the main reason the reviewers
  suggested a split) is handled **internally** by the U2 gate: validate dockview's split/drag/tab-group +
  serialize/restore in the Tauri webview before building U4–U8 on it. The brainstorm/plan reviews
  (scope-guardian + feasibility personas) recommended splitting into a shell-foundation unit and a
  dashboard-content unit; that recommendation was considered and declined for the reasons above (user
  preference for fewer cycles + the internal U2 gate); recorded here.
- **Size:** this is the largest RU in the program (estimated >1,000 human-authored lines incl. tests).
  Per the size guardrail this requires an explicit decision — carried to the release plan as
  `aprobar PR grande` (≈ half is Vitest tests; one cohesive surface). Planning artifacts (brainstorm,
  plan) ride the branch in a separate docs commit; no generated artifacts dominate.

## Implementation Units

(Full detail in `docs/plans/2026-06-15-001-feat-dashboard-ui-plan.md`.)

- U1. Dependencies + Tauri wiring (dockview-react, plugin-dialog + `dialog:allow-open`, `ui_state` Rust
  command, KTD5 canonicalize-on-add).
- U2. Dock workspace shell + layout persistence (GATE: validate dockview in the webview).
- U3. Bus client: TS contract mirror, StrictMode-safe listeners, revision-gated store, snapshot load,
  `list_workbenches` name join (canonical-path aligned).
- U4. Dashboard panel: card grid, compact/expanded, activity indicator, git edge states, loading
  skeleton, error/degraded states + retry.
- U5. Repo panel: commit log (`get_commit_log`) + full status list, dedup-by-path open.
- U6. Repo-node tree panel (no file expansion).
- U7. Workbench management: top-bar switcher, first-run, create/add(dialog+autodetect)/remove (by active
  workbench name), zero-repos state.
- U8. Integration: panel registry, default layout, keyboard floor, replace smoke `App.tsx`.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Full dashboard UI (U1–U8) + planning docs | frontend (`src/bus/**`, `src/workspace/**`, `src/panels/**`, `src/workbench/**`, `src/App.tsx`), Tauri (`src-tauri/src/ui_state.rs`, `lib.rs`, `workbench/mod.rs` 1 amendment, `capabilities/default.json`, `Cargo.toml`), deps (`package.json`), tests (Vitest + cargo), docs (`docs/**`) | develop | optional: "Jira omitted" unless configured | >1,000 human lines (≈half tests); single cohesive shell; dock-engine risk gated internally (U2); large-PR decision carried to release plan; deps/docs in separate commits |

Rules applied: planning docs ride the branch (brainstorm + plan), committed separately from code; the
dock-engine validation is an internal U2 gate, not a separate PR.

## Files and Tests

- Frontend code: `src/bus/{contract,client,store}.ts`, `src/workspace/{DockWorkspace.tsx,panels.ts,layout.ts}`,
  `src/panels/{DashboardPanel,RepoCard,RepoPanel,RepoTreePanel}.tsx`,
  `src/workbench/{TopBar,WorkbenchControls,firstRun}.tsx`, `src/App.tsx`.
- Rust: `src-tauri/src/ui_state.rs` (new), `src-tauri/src/lib.rs`, `src-tauri/src/workbench/mod.rs`
  (canonicalize-on-add), `src-tauri/capabilities/default.json`, `src-tauri/Cargo.toml`.
- Deps: `package.json` (+ dockview-react, @tauri-apps/plugin-dialog).
- Tests: Vitest — `src/bus/store.test.ts`, `src/workspace/layout.test.ts`,
  `src/panels/{RepoCard,RepoPanel,RepoTreePanel}.test.tsx`, `src/workbench/workbench.test.tsx`,
  `src/App.test.tsx` (replaces smoke); cargo — `ui_state` tests + a workbench canonicalization test.
- AE coverage: AE1–AE12 each map to a named plan test scenario (`Covers AEn` tags in the plan).

## Impact Scan

- Changed API contracts: **none on the backend contract** (frozen; consumed read-only). New: the
  `get_ui_state`/`set_ui_state` commands (new surface, no consumers yet). Behavior change in delivered
  code: `workbench` add path canonicalizes the stored repo path (KTD5) — aligns config with the bus's
  existing canonicalization; verify existing workbench tests stay green.
- Consumer scan patterns: `rg "App\.tsx|ping|tick" src/` (replacing the smoke); `rg "add_repo|store the path"
  src-tauri/src/workbench/` (canonicalization amendment).
- Consumers found: `src/App.tsx` (smoke, intentionally replaced); the bus/snapshot already canonicalize,
  so aligning the workbench store reduces drift, not adds it.
- Contract-drift tests searched: existing cargo suites (workbench/bus/git/paths/watcher) + vitest smoke.
  The workbench tests assert stored paths — update any that assumed a raw (non-canonical) stored path.
- Required consumer tests: full `cargo test` (existing 106 stay green, adjusted for canonicalization) +
  full `npm test`.
- Consumer tests run/skipped: run in the Verification Gate.

## Verification Gate

- `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test` (existing + ui_state +
  workbench canonicalization) · `cargo build` · `npm run lint` · `npm test` (new Vitest suites) ·
  `tauri dev` smoke covering AE1–AE12 · `tauri build` exit 0.
- Surface-aware evidence: dock engine → U2 webview gate + layout.test; data layer → store.test; panels →
  testing-library component tests; Tauri command → cargo ui_state tests; wiring/keyboard → App.test +
  dev smoke; canonicalization → cargo workbench test.
- Production posture evidence: prototype; Windows best-effort until CI (D2). Relaxed: no cross-platform
  matrix; dev smoke on Linux/WSL.

## Review Gate

- Code review threshold: P0-P2 (default). Findings below threshold: log unless user marks blocking.
- Suggested personas: correctness (store/revision-gating, listener lifecycle, dedup-by-path), testing
  (AE coverage, dialog/invoke mocking), maintainability (panel/store boundaries, contract mirror drift),
  adversarial (StrictMode double-subscribe, layout-persist corruption/flush-on-quit, canonical-path join
  failure modes, dockview focus/keyboard).

## Security Gate

- Run after work-review loop: not required — read-only local UI over the existing IPC surface; no auth,
  secrets, PII, network, or public API. The folder dialog grants only `dialog:allow-open` (user-initiated
  picker); the new `ui_state` command writes a layout JSON to the config dir (no sensitive data).
- Security Watch during work: light — ensure `ui_state` does not write outside the config dir and tolerates
  corrupt input (already in the plan); ensure the dialog capability is the minimal open permission, not a
  broad fs grant.
- Security Watch notes: none beyond the above.
- Security reviewer: n/a.
- Security review result: not required.
- Required security verification: `ui_state` corrupt-tolerant + config-dir-scoped (cargo tests).

## CI Break-Prevention And Escalation

- CI risk surfaces: TS typecheck/build, eslint, Vitest, cargo build/clippy/test/fmt, new deps resolving,
  Tauri capability change, `tauri build`.
- Preventive evidence: local Verification Gate covers each; CI workflow itself is still deferred (D2), so
  documented local verification is the break-prevention evidence.
- If CI breaks: invoke krt-ci-questor with PR/run/check context; do not poll checks here.
- Escalation rule: record a release-follow-up blocker until any CI incident has cause, owner, next action.

## Branch and PR Handoff Inputs

- Review unit: RU1 — Dashboard UI (dockable workspace + cards + workbench management).
- Branch name: `feat/dashboard-ui`
- Branch/docs rule: this first (and only) executable review unit carries the RDM-007 planning artifacts on
  the same branch; no separate docs branch.
- PR base: develop
- Suggested commit grouping for this review unit:
  - `chore(frontend): add dockview and dialog plugin deps` — `package.json`, `Cargo.toml`, lockfiles
  - `feat(app): ui-state persistence command and canonical repo paths` — `src-tauri/src/ui_state.rs`,
    `lib.rs`, `workbench/mod.rs`, `capabilities/default.json`
  - `feat(ui): dockable workspace shell with persisted layout` — `src/workspace/**`
  - `feat(ui): bus client store and live data binding` — `src/bus/**`
  - `feat(ui): dashboard, repo, and tree panels` — `src/panels/**`
  - `feat(ui): workbench switcher, first-run, and curation` — `src/workbench/**`
  - `feat(ui): compose the workspace shell` — `src/App.tsx`, registry/default layout, `App.test.tsx`
  - `docs: RDM-007 dashboard brainstorm, plan, and work package` — `docs/**`
  (Group/merge as the diff settles; keep deps + docs in their own commits.)
- PR title: `Dashboard UI: dockable workspace with live repo cards`
- PR body bullets: dockable workspace (split/drag/persist) · live repo cards from the bus stream ·
  per-repo detail panels + repo tree · workbench switcher, first-run, add/remove repos · degraded/error/
  edge states.
- Verification results location: Verification Gate of this package + execution thread.
- Production/deployment notes: none (prototype desktop app).
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Tarea
- Suggested subtask behavior: single standalone `Tarea` (one review unit, no multi-child parent).
- Jira summary: "Interfaz de panel: espacio de trabajo acoplable con tarjetas de repos en vivo"
- Jira description: "Construir la primera interfaz de Tinto: un espacio de trabajo estilo VS Code (paneles
  divisibles/reordenables con disposición persistente) que consume el contrato del bus y muestra en vivo
  el estado de los repos del workbench activo (tarjetas, panel por repo, árbol de repos), con cambio de
  workbench, primer arranque y alta/baja de repos."
- Optional-policy fallback: if Jira config/context is missing, record "Jira omitted: jira-env-not-configured"
  in state/release closeout and continue.
