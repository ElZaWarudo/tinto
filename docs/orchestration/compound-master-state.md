# Compound Master — Live state

> Compact resume entrypoint. Long historical detail is archived in `archive/compound-master-state/`.

## Initiative

- **Project:** Tinto — a desktop app (Tauri 2) for read-only monitoring of git repos edited by code agents.
- **Design source:** `tinto-design.md` (architecture-level design, not an implementation spec).
- **Repo:** `/home/teb/personal-proyects/tinto` (**Linux/WSL checkout since 2026-06-11**; earlier runs in `C:\Users\Mayor\Documents\Caribbean\tinto`, Windows). Remote: https://github.com/ElZaWarudo/tinto. Integration: `develop` (gitflow, D1).
- **Linux toolchain verified (2026-06-11):** Rust 1.93.1, node 24.13, npm 11.6, `gh` authenticated (ElZaWarudo). The compound-engineering plugin 3.12.0 was **reinstalled** on this host (user scope, EveryInc marketplace); the ce-* SKILL.md files are read from the cache `~/.claude/plugins/cache/.../3.12.0/skills/`.

## Resolved arguments

| Arg | Value | Notes |
|---|---|---|
| mode | full | artifacts + start execution of the first review unit |
| production | prototype | greenfield, no live system |
| jira-policy | optional | no Jira context detected → degraded handoff without Jira |
| pr-granularity | auto (review-unit) | |
| parallel | false | |
| delegation | auto → autonomy:guarded | |
| worktree-policy | avoid | |
| autonomy | guarded | no ledger → local autonomy only, no external mutation |
| review-threshold | P0-P2 | |

## User decisions (preflight)

- **Scope:** mode:full — generate artifacts and start execution.
- **Git:** initialized in the tinto folder (branch `main`).
- **Frontend:** **React** (the design marked it optional; the user fixes it explicitly).

## Resolved roles

| Role | Skill | Runtime |
|---|---|---|
| roadmap_generator | krt-roadmap-cartographer | native |
| brainstorm | ce-brainstorm | compound-engineering plugin 3.12.0* |
| plan | ce-plan | compound-engineering plugin 3.12.0* |
| document_review | ce-doc-review | compound-engineering plugin 3.12.0* |
| work | ce-work | compound-engineering plugin 3.12.0* |
| code_review | ce-code-review | compound-engineering plugin 3.12.0* |
| security_review | krt-security-sentinel | native |
| project_pr | krt-release-marshal | native |

\* Plugin installed mid-session (user scope, EveryInc/compound-engineering-plugin marketplace). In this session the SKILL.md files are executed by reading them from `C:\Users\Mayor\.claude\plugins\cache\compound-engineering-plugin\compound-engineering\3.12.0\skills\`; after restarting the session they will be natively invocable.

## Pipeline — progress

- [x] 1. Preflight (roles, repo, branch, jira, production, delegation, context)
- [x] 2. Roadmap generated: `docs/roadmaps/2026-06-10-001-tinto-roadmap.md` (12 items RDM-001..012, 8 waves; decisions D1 gitflow, D2 CI, D3 React recorded)
- [x] 3a. Roadmap review (ce-doc-review headless: coherence, feasibility, design-lens, scope-guardian — 4 reviewers in parallel). 2 safe_auto applied (Blocks/enables drift in RDM-001/005). 3 user decisions applied: editor fs_watch → RDM-009; first-run onboarding → RDM-007; secrets → simple patterns in RDM-011. Boundary RDM-011↔008/009 clarified. The rest routed to the roadmap's "Deferred / Open Questions".
- [x] 3b-RDM-001. Brainstorm captured and reviewed: `docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md` (TypeScript; npm ignore-scripts; GitHub remote now + CI deferred → D2 resolved; AE3: Windows hard, Linux best-effort until CI; R1 with no React 18 pin → template version).
- [x] 4-RDM-001. Plan written and reviewed: `docs/plans/2026-06-10-001-feat-esqueleto-tauri-react-plan.md` (U1 scaffold, U2 tooling, U3 smoke bridge via tauri::async_runtime + mockIPC, U4 deliver-handoff-only). Work package derived, checker OK (justified docs-mixing warning) and review "PACKAGE SOUND": `docs/work-packages/RDM-001-esqueleto-tauri/2026-06-10-001-esqueleto-tauri-work-package.md` — single RU1, branch `feat/tauri-react-skeleton`, base `main`.
  - Items RDM-002..012: brainstorm/plan/package pending; generated as their wave approaches (the UI ones depend on the event contract that RDM-006 freezes).
- [ ] 5. Execute RU1 of RDM-001 — **IN PROGRESS** (inline, branch `feat/tauri-react-skeleton`)
  - Delegation resolved: inline (matrix: same-file coupled work + heavy CLI). User approved execution and provided UI direction (per-project tabs + left tree) → recorded in roadmap Open Questions RDM-007/008.
  - **Toolchain actions (executed assumptions, reversible):** rustup 1.29/Rust 1.96 MSVC installed via winget; Windows 11 SDK 10.0.26100.8249 installed (elevated, UAC accepted by user) — VS2019 BuildTools MSVC 14.29 and WebView2 already existed. Sanity link test OK.
  - U1 ✔: react-ts template scaffolded (React 19.1, Vite 7, TS 5.8, Tauri 2) via temp dir → root; identity renamed (tinto / dev.tinto.app / Tinto); `.npmrc ignore-scripts=true` BEFORE install; **assumption verified: clean install with no lifecycle scripts** (esbuild/rollup/tauri via optionalDependencies); tokio time + cargo check OK.
  - U2 ✔: ESLint flat + Prettier + Vitest jsdom + testing-library; lint/format/test scripts; format applied; lint green; `.prettierignore`.
  - U3 ✔ (code+tests): ping/tick with tauri::async_runtime + Emitter; App.tsx with instrumentation; vitest 3/3 (AE1, AE2, error path — mockIPC + vi.mock of the event module); cargo test 2/2; fmt+clippy -D warnings clean.
  - Visual smoke `tauri dev` ✔ (window "Tinto" PID 5532, ping/tick visible).
- [ ] 6. Code review (ce-code-review role, 4 personas: correctness/testing/maintainability/adversarial) — **fixes applied**:
  - CSP `default-src 'self'` + ipc (was `null`) [correctness P1 + adversarial].
  - Removed unused `tauri-plugin-opener` (Cargo.toml, lib.rs, package.json) and its `opener:default` grant in capabilities [maintainability + adversarial].
  - Test AE2: assertion strengthened to a real-time pattern; removed mid-test `clearMocks()` [testing P1/P2].
  - Tinto README (replaces template) and `<title>Tinto</title>` [maintainability P2/P3].
  - Dismissed with announce: core:default does NOT include fs/shell in Tauri 2 (adversarial overclaim); StrictMode double-listen cleans up each unlisten of its own; tick task dies with the process. Recorded as residual notes.
  - Post-fix re-verification ✔: vitest 3/3, lint, fmt, clippy, cargo test 2/2.
  - Re-smoke dev with CSP ✔ (window up). `tauri build` exit 0: exe 8.6MB + MSI + NSIS; release binary smoke ✔ (window "Tinto" with production CSP). **Pipeline items 5 and 6 COMPLETE; review unit RU1 implementation-complete + review PASS + verification PASS.** Package → `implemented-verified-awaiting-release`.
- [x] 7. Handoff to krt-release-marshal **COMPLETE** (2026-06-10):
  - Release plan approved by user (including the size/scope decision: approve a large PR — bulk = lockfiles generated in their own commit; human authorship ~600 lines of code + ~890 docs).
  - Bootstrap: `main` with root commit `4b7438d` (empty init); branch `feat/tauri-react-skeleton` with 4 commits: `e9c46e5` feat(app) 600+, `8f16675` chore(tooling) 66+, `26003e1` chore(generated) 9198+, `44f69a7` docs(orchestration) 890+. gitflow-knight's env-ignore guard OK (created `.krt/env/.gitignore`, committed in tooling).
  - Rebase: unnecessary (new linear history).
  - Jira: **omitted** — checker `ok: false` diagnostic `env-loaded-without-project-secret-file` (missing `.krt/env/jira-scribe.env` in the checkout). No backlink or transition.
  - Private GitHub repo created: https://github.com/ElZaWarudo/tinto; pushed `main` and `feat/tauri-react-skeleton`.
  - **PR #1: https://github.com/ElZaWarudo/tinto/pull/1** (`feat/tauri-react-skeleton → main`, ready). Body validated with format/check_pr_body (5 bullets, no internal IDs).
  - Reviewers: omitted with a note — new repo with no collaborators.
  - **Merge: NOT attempted** — requires a visible gate on GitHub + explicit user authorization for that exact merge.
  - R12 ✔ (private origin remote with main pushed). RDM-001 delivered for review.

## Merge checkpoint (2026-06-11)

- **PR #1 MERGED** with explicit user authorization ("complete the merge"); visible gate: MERGEABLE, no required checks or reviews (main unprotected, repo with no collaborators). Merge commit `62e5653`; feature branch deleted locally and remotely.
- **D1 RESOLVED:** `develop` created from main and pushed. Subsequent features base on `develop`; PRs `feat/* → develop`.
- Post-PR docs (this state + 2026-06-10 summary) in the working tree; they travel with the RDM-002 branch.

## Wave 2 — IN PROGRESS

- RDM-001 ✅ DELIVERED AND MERGED (PR #1).
- RDM-002 ✅ DELIVERED AND MERGED (**PR #2**, merge `b9465e9`, explicit authorization in the approved release plan; 4 semantic commits; 3-persona review → 7 tests added + 1 real bug caught: log() unborn returned Internal; 24/24 tests). Artifacts: brainstorm/plan/package 2026-06-11 + gates updated.
- **Wave 2 gates (user decision 2026-06-11): auto until release plan** — I only stop at the per-PR release plan and on real product decisions.
- RDM-003 ✅ DELIVERED AND MERGED (**PR #3**, merge `2011005`; merge authorized under the standing rule). `paths` module (classifier with real git semantics, BFS pruning, zero I/O in classify).
- **Flow agreements (user 2026-06-11):** (a) update and commit the compound master files at the close of each unit; (b) **pre-authorized merge for the program's PRs whenever the PR includes the updated compound master state** (the release plan must affirm that condition).
- RDM-005 ✅ DELIVERED AND MERGED (**PR #4**, merge `a199d0e`, under the standing rule — state included in `9e6f670`). `workbench` module: atomic TOML store (per-PID tmp, `.corrupt` backup), CRUD + BFS-4 autodetection with worktrees, 9 Tauri commands. 53/53 tests; dual review with 4 P1 fixed.

## ✅ WAVE 2 COMPLETE (2026-06-11)

PRs #2 (git engine), #3 (classifier), #4 (workbenches) merged to `develop`. Backend ready for integration: 53 tests, clippy clean.

## Run pause (user decision 2026-06-11: "approve and pause afterward")

- ~~Branch `feat/fs-watcher` already created~~ — that branch was created **only in the Windows checkout** and was never pushed; **recreated in the Linux checkout** from `develop` (2026-06-11, this run).
- After RDM-004: wave 4 RDM-006 (bus — freeze the event contract; resolve the spawn_blocking wrapping of the GitEngine; remember the user's UI direction for RDM-007/008: per-project tabs + left tree + management of open files).

## Linux resume (2026-06-11) — reconciliation incident and decisions

- **Incident (resolved):** the run resumed on this host started with the checkout on `main` (stale, 14 commits behind) and without inspecting the contents of `origin/develop`; it regenerated the brainstorm/plan/work-package of **RDM-002**, which was already delivered (PR #2). The work package's feasibility review caught it (conf 100). **Correction:** duplicate local work discarded (user decision), checkout resynced to `develop`. *Operational lesson: the resume preflight must inspect the remote integration base (`git log origin/main..origin/develop`), not just verify that it exists.*
- **User decision — opt-in fetch → BACKLOG (2026-06-11):** during the regeneration the user chose (3 confirmations) to add an opt-in `fetch` to the git engine. develop delivered RDM-002 deliberately without network (`git2 0.20` without ssh/https features, "Tinto never does fetch/push"). The user decided to preserve the fetch design as a **backlog/follow-up item** (future amendment to the git engine), not to execute it now nor discard it. Design preserved in `docs/backlog/2026-06-11-fetch-opt-in-backlog.md` (decisions + the review's security requirements: host fail-closed without CertificatePassthrough, credential scoping to the host confirmed, error sanitization, manual known_hosts, honest staleness).

## Blockers

- None. Jira is still omitted (missing `.krt/env/jira-scribe.env`; creating it would enable it).

## Wave 3 — RDM-004 (watcher) — IN PROGRESS

- [x] Brainstorm: `docs/brainstorms/2026-06-11-rdm-004-watcher-requirements.md`. User decision: **scope = active workbench only** (configurable deferred). Boundary 004/006 resolved: debounce/throttle over raw FS events (004) vs delta coalescing + emit-throttle (006). Single channel with a `Batch | RepoError` message; remount via explicit API (wiring → RDM-006).
- [x] Brainstorm review (coherence, feasibility, scope-guardian). Fixes: normalization of notify kinds (renames→Removed+Created, Any→Modified, Access dropped); is_dir without stat with default false; AE5/R3 reformulated (notify does not emit an error on deleting root → synthesis); deterministic testability criterion for debounce; R12 expanded; roadmap annotated (scope resolved + dep RDM-005).
- [x] Plan: `docs/plans/2026-06-11-004-feat-fs-watcher-plan.md` (U1 types+normalize, U2 debounce/throttle paused clock, U3 FsWatcher+lifecycle). Review (coherence, feasibility) applied: full tokio features (prod time/sync/macros/rt + dev test-util), lifecycle KTD without contradiction (managed state → RDM-006; tests build directly), `tokio::spawn` with required context, `shutdown(self)` async + Drop best-effort, `pub mod watcher`, tolerate WatchNotFound, rebuild flag in debounce.rs with a signal in the batch, risks added.
- [x] Work package: `docs/work-packages/RDM-004-watcher/2026-06-11-001-fs-watcher-work-package.md` — single RU1, branch `feat/fs-watcher`, base `develop`. Checker OK (2 justified warnings); coherence review → 1 safe_auto applied (Cargo.toml in commit grouping). **PACKAGE SOUND.**
- [x] **RU1 execution COMPLETE** (inline): U1 normalize+types (7 tests), U2 debounce/throttle with paused clock (deadline = max(floor, min(quiet, ceiling)); 11 tests, 0.00s), U3 FsWatcher (canonicalized mount, classifier-before-watch, dead_roots for real remount, re-assert of overlapping roots, RescanNeeded on kernel overflow, shutdown with flush; 10 FS integration tests). 1 fixture fix during execution (AE2: .env must be gitignored to be Plane2).
- [x] **RU1 verification PASS:** fmt ✓ clippy ✓ (`replace_box`, `ptr_arg` fixed) `cargo test` 81/81 ✓ build ✓ clean npm install + vitest 3/3 + eslint ✓.
- [x] **Code review PASS** (4 personas): 12 fixes applied (3 P1: no-op remount from divergent `mounted` → dead_roots; nested unwatch poisoned subtree → re-assert; swallowed Rescan → `RescanNeeded`; plus 5 P2 + 3 P3 + safe_autos) + 9 new tests from the review. Dismissed with announce: inline rebuild (risk accepted in plan), rename watch_workbench, Serialize of errors (→ RDM-006). Detail in the package's Review Gate. Re-verification 81/81.
- [x] Security gate: not required (RU1 with no high-risk surface, per the package).
- Package → `implemented-verified-awaiting-release`.
- [x] **Release COMPLETE (2026-06-11):** release plan approved by user (incl. `Size/scope decision: approve a large PR` — ~1,746 human lines, ~372 docs separately, ~half of the Rust is tests; broad RU approved in the package). 6 semantic commits on `feat/fs-watcher` (3 feat with incremental history of mod.rs, chore lockfile, docs, + reconciliation f7ea5d5). Scope guardrail: BLOCKING size resolved with the plan's decision. Jira omitted (`jira-env-not-configured`). **PR #6 https://github.com/ElZaWarudo/tinto/pull/6 MERGED to `develop`** (merge `c008d77`, visible gate MERGEABLE/CLEAN with no required checks or reviews, merge pre-authorized by standing rule — state included in commit `8c3e21e`). Branch deleted locally and remotely. Reviewers omitted with a note (repo with no collaborators). **RDM-004 CLOSED.**

## ✅ WAVE 3 COMPLETE (2026-06-11)

PR #6 (watcher) merged to `develop`. Backend: git engine + classifier + workbenches + watcher; 81 tests, clippy clean.

## Wave 4 — RDM-006 (State/Event bus) — ✅ COMPLETE (shipped 2026-06-15)

- **PR #7 MERGED** to `develop` (merge commit `92f2446`, gate MERGEABLE/CLEAN, no required checks/reviews; merged under the standing rule with state included). Branch deleted local+remote. **Event contract frozen** (`docs/contracts/bus-contract.md`). Backend complete: git engine + classifier + workbenches + watcher + state/event bus; 106 tests, clippy clean.
- **Language switch (user directive 2026-06-15):** mid-merge the user set this repo to **English going forward**. PR #7 was given a full English rewrite (3 commit messages + PR title/body + the new docs: brainstorm, plan, work package, contract, and this state file). Code-file comments/strings remain Spanish (consistent with RDM-001..005); English applies to net-new code from RDM-007 on. WIP backup branch `checkpoint/state-event-bus` is now obsolete (local+remote) — prune when convenient.
- Branch: `feat/state-event-bus` (from develop; carried the wave 3 closeout docs).
- [x] Brainstorm: `docs/brainstorms/2026-06-11-rdm-006-state-event-bus-requirements.md`. User decisions 2026-06-11: (1) **lightweight push + subscription to the open diff**; (2) **full repo tree** in the UI ⇒ the contract freezes a tree-listing command (walk with the `ignore` crate).
- [x] Brainstorm review (coherence, feasibility, scope-guardian, adversarial). Hardening applied: dry-run of the freeze against RDM-007..010 as a condition; errors in 2 classes (transient vs terminal of the watcher) + retry command; global degraded state via `BackendInit`; untracked with synthesized FileDiff (R9b); working-tree current-content command; subscription = a set capped at N; monotonic revision snapshot+deltas (R2b); Plane 2 with best-effort stat + last known size (size delta for RDM-009); RescanNeeded broadcast with bounded concurrency; live diff latency budget ≤2s p95 (coalescing only for bursty cases); wiring constraints (watcher owned by the bus, synchronous channel in setup, switching notifies — nothing inline on the main thread); blob Vec<u8> carve-out; cite Git2Engine (not a trait); AE8–AE11.
- [x] Plan: `docs/plans/2026-06-11-005-feat-state-event-bus-plan.md` (U1 contract+dry-run, U2 pure state, U3 bus task, U4 commands, U5 wiring). Review (coherence, feasibility) applied: run-loop `.build().run(RunEvent)` + Shutdown(ack)+block_on for clean exit; identity = canonical path throughout the bus; subscribed diffs inside the recompute closure (snapshot of subscriptions); results channel (not JoinSet without guard); `DeltaSink` (not `Emitter`, collides); `State<BusHandle>` required (not Option — Tauri does not support it; no wrapper tests); dep `base64`; tree cap 20k pinned; trace/structure fixes.
- [x] Work package: `docs/work-packages/RDM-006-state-event-bus/2026-06-11-001-state-event-bus-work-package.md` — **single RU** (contract-vs-orchestration split evaluated and dismissed: U1 has no independent verification; dry-run as an internal gate). Checker OK; coherence review → 3 fixes (Security Watch aligned to verifiable containment + traversal test; "already on the branch"→working tree; glob docs not overlapping the contract). **PACKAGE SOUND.**
- [~] **RU1 execution — code complete, VERIFICATION PENDING** (inline, branch `feat/state-event-bus`):
  - U1 ✔ code: `docs/contracts/bus-contract.md` (contract + dry-run table RDM-007..010 with no gaps) + `src-tauri/src/bus/contract.rs` (Serialize types + shape tests).
  - U2 ✔ code: pure logic in `bus/mod.rs` (`RepoLiveState`, `recalc_scope`, monotonic revision, error classes, `fs_events` with size/delta) + pure tests.
  - U3 ✔ code: `run_bus` task (select! commands/watcher/results-channel, `Git2Engine::open` in spawn_blocking + semaphore cap=2 + pending flag, subscribed diffs inside the closure, canonical identity, injected `DeltaSink`, degraded via BackendInit) + 5 integration tests (AE1/AE7/AE3/AE10/AE8).
  - U4 ✔ code: `bus/commands.rs` (serializable CommandError; get_worktree_diff/commit_diff/commit_log/blob/file_content/list_repo_tree in spawn_blocking; resolve_within anti-traversal; FileContent utf8/base64/truncated; tree via `ignore::WalkBuilder` cap 20k; snapshot/subscribe/retry proxies) + tests. Dep `base64` added.
  - U5 ✔ code: wiring in `lib.rs` (synchronous channel + manage(BusHandle) + spawn run_bus with AppHandle::emit sink + initial of the active workbench; `.build().run(RunEvent::ExitRequested→shutdown+block_on)`); `set_active_workbench` gains `State<BusHandle>` and notifies the bus.
  - **✅ UNBLOCKED and VERIFIED (2026-06-15):** Bash restored. Fix of 1 flaky test (non-deterministic order of the initial A/B snapshot) + 2 compile errors (`Manager` trait for `try_state`; `block_on` inference). Verification PASS: fmt ✓ clippy ✓ `cargo test` 106/106 ✓ build ✓ npm lint+vitest 3/3 ✓ `tauri dev` smoke ✓.
- [x] **Code review COMPLETE (4 personas, 2026-06-15)** — see the table in the package's Review Gate. **11 fixes applied** (1 P0 allowlist, 5 P1, 3 P2, 2 P3) + **9 new tests**. Deferred with rationale: 9-arg struct, bounded channel, blob bound (git module), various P3. **Surfaceable decision:** the workbench-membership allowlist (P0 adv) was implemented despite being deferred in the package's Security Gate — cheap, does not change legitimate frontend behavior, hardens consistently with RDM-001's CSP/capabilities. Closes the "future hardening". Contract updated (`bus-contract.md`: containment categories, allowlist, durable revision, terminal cleanup).
- [x] Security gate: not required (covered by the adversarial persona; traversal/`.git`/allowlist surface now has tests).
- Package → `implemented-verified-awaiting-release`. **Pipeline items 5 and 6 COMPLETE.**

## Wave 5 — RDM-007 (Dashboard UI) — EN CURSO

- **Scope decided with user (2026-06-15):** RDM-007 expanded to a VS Code–style **dockable/splittable workspace** (build the shell now, user chose against splitting into 007a/007b). Panel model: Dashboard card grid + per-repo detail tabs + repo-node tree. Global persisted layout. Core workbench mgmt (create/switch/add/remove). Repo tree is **repo-node-level only**; file expansion deferred to RDM-008. RDM-008 = Wave 6 (hard-depends on 007).
- [x] Brainstorm: `docs/brainstorms/2026-06-15-rdm-007-dashboard-ui-requirements.md`. Reviewed (coherence, feasibility, scope-guardian, design-lens). Hardened: differentiated Repo panel (commit log + status list vs card), git edge states, zero-repos/loading/degraded states, activity-indicator spec, dock-library-first gate, plugins/capabilities directive, `list_workbenches` name join.
- [x] Plan: `docs/plans/2026-06-15-001-feat-dashboard-ui-plan.md` (U1 deps+Tauri wiring, U2 dock shell GATE, U3 bus client+store, U4 dashboard, U5 repo panel, U6 tree, U7 workbench mgmt, U8 integration). Reviewed (coherence, feasibility). Fixes applied: KTD1 dockview; mutation commands need `workbench` name arg; timestamp units (ms vs s); `dialog:allow-open`; ui_state write-failure + flush-on-quit; **KTD5 canonical-path join** (canonicalize stored repo path on add — small RDM-005 amendment).
- [x] Work package: `docs/work-packages/RDM-007-dashboard-ui/2026-06-15-001-dashboard-ui-work-package.md` — **single RU** (reviewers' split recommendation declined: user prefers fewer cycles + U2 internal gate de-risks). Checker OK; coherence review → 1 fix (dangling finding-ID reference). **PACKAGE SOUND.** Large PR (>1000 lines, ~half tests) → `aprobar PR grande` decision carried to release plan.
- [~] **Execute RU1 (U1–U8)** — branch `feat/dashboard-ui` from develop. **IN PROGRESS.**
  - [x] U1 deps + Tauri wiring: dockview-react 6.6.1 + @tauri-apps/plugin-dialog 2.7.1; Rust `ui_state` command (`get_ui_state`/`set_ui_state`, atomic write, corrupt-tolerant) + `tauri_plugin_dialog::init()` + `dialog:allow-open` capability; KTD5 canonicalize-on-add in `workbench/mod.rs` (+ remove matches canonical). **111 cargo tests** (106 + 4 ui_state + 1 canonicalization), clippy/fmt clean.
  - [x] U2 dock shell **GATE PASSED**: `src/workspace/{panels,layout,DockWorkspace}.tsx` — dockview `themeVisualStudio`, restore-or-default, debounced save, flush-on-quit, empty-workspace guard; persistence via `get_ui_state`/`set_ui_state`. App.tsx replaced (placeholders), dark dev-tool App.css, ResizeObserver/matchMedia polyfills. **10 vitest** (layout 5, DockWorkspace 4, App 1), tsc/lint clean. `tauri dev` boot smoke ✓ (dockview mounts, no panics). Interactive split/drag deferred to user; wiring covered by DockWorkspace.test.
  - [ ] U3 bus client/store · U4 dashboard · U5 repo panel · U6 tree · U7 workbench mgmt · U8 integration — REMAINING.
  - Foundation committed as checkpoints on the branch (deps / app / ui-shell + docs).

## Next action

Execute RU1 unit by unit: U1 (deps: dockview-react + plugin-dialog; Rust `ui_state` command + `dialog:allow-open` capability + canonicalize-on-add in workbench store) → U2 dock shell GATE (validate dockview split/drag/serialize-restore in `tauri dev` before panels) → U3 bus client/store → U4–U7 panels + workbench mgmt → U8 integration. Verify per the Verification Gate (cargo + vitest + lint + tauri dev smoke AE1–AE12 + tauri build), then code review (4 personas), then handoff to krt-release-marshal (PR `feat/dashboard-ui`→develop, merge under standing rule with state included, large-PR decision affirmed, KTD5 delivered-code touch surfaced).

- Standing flow agreements: (a) update+commit compound-master docs at each unit close; (b) merge pre-authorized for program PRs whenever the PR includes the updated compound-master state.
- Jira omitted (missing `.krt/env/jira-scribe.env`).
- All new code/docs in English (Wave 4 language directive).
