---
title: State/Event bus — backend→frontend integration
status: review-passed
roadmap_item: RDM-006
origin_roadmap: docs/roadmaps/2026-06-10-001-tinto-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-11-rdm-006-state-event-bus-requirements.md
origin_planning_input: docs/brainstorms/2026-06-11-rdm-006-state-event-bus-requirements.md
origin_plan: docs/plans/2026-06-11-005-feat-state-event-bus-plan.md
units: [U1, U2, U3, U4, U5]
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

# State/Event bus — backend→frontend integration

## Scope

Implement the `bus` module per the origin plan: frozen contract with dry-run (U1: `docs/contracts/bus-contract.md` + types), pure live state (U2), the watcher-owning task with selective recompute/cap=2/subscriptions/snapshot/retry (U3), the complete `invoke` commands (U4), and lifecycle wiring in the app including clean shutdown (U5). Includes the RDM-006 planning artifacts and the wave 3 closeout docs on the same branch (branch/docs rule).

## Non-goals

- UI (RDM-007+), persistence of the live state, passive signals (RDM-011), notifications (RDM-012).
- Changes to delivered modules beyond the declared wiring: `lib.rs` (setup/run-loop/handler) and `set_active_workbench` (bus notification).
- Caching/optimization of heavy reads (waits for its consumer: RDM-008/010).
- TS types for the contract (RDM-007 derives them from the doc).

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: internal names, test organization, exact values within the pinned bounds (tree cap 20k, 1 MiB guard, N=8, recompute cap 2), details that follow the KTDs.
- Agent must record as assumptions: the resolved `base64` version, any gap discovered in the U1 dry-run and how it was resolved, deviations from the contract shape.
- Agent must escalate: any RDM-007..010 need NOT serviceable by the contract (blocks the freeze — a product decision), behavior changes in delivered modules outside the declared wiring, external mutation, scope outside the package.
- Safe fallback: U1/U2 depend on neither Tauri nor the watcher; on a blocker in U3/U5, report the exact decision.
- Autonomous ledger: none
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-002 ✅, RDM-004 ✅, RDM-005 ✅ (on `develop`).
- Blocks: RDM-007, RDM-008, RDM-009, RDM-010, RDM-011.

## Production Posture

- Posture: prototype — greenfield with no users. Confidence: high.
- Consequences: speed permitted; the frozen contract is self-discipline (dry-run + additive-first), not external compatibility.
- Breaking existing behavior allowed: yes (no bus consumers yet; ping/tick smoke intact).

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Contract + dry-run; gate for the rest. |
| U2 | yes | Pure state logic; same module. |
| U3 | yes | Bus task; integrates U1+U2 with watcher/git. |
| U4 | yes | Invoke commands over the same types. |
| U5 | yes | App wiring; touches lib.rs + 1 workbench line. |

Grouping rationale:
- **Single RU.** The contract-vs-orchestration split from the roadmap was evaluated and discarded: U1 alone (types+doc) has no real independent verification (nothing exercises it without the bus) and would produce a small, noisy stacked PR — the template's combination criterion, same reasoning as RDM-004. The U1 dry-run acts as a sequential internal gate, not a PR boundary.
- Planning docs + wave 3 closeout (already on the branch) in separate docs commits; `Cargo.lock` separately.

## Implementation Units

- U1. Frozen contract (`docs/contracts/bus-contract.md` with the RDM-007..010 dry-run table) + `bus/contract.rs` (Serialize types, JSON shape test). — gate: an unserviceable need blocks.
- U2. Pure live state: `RepoLiveState`, monotonic revision, error classes, per-batch recompute decision, FsEvents with size/delta.
- U3. Bus task: select! over commands/watcher/results (results channel, not a JoinSet without a guard), spawn_blocking with `Git2Engine::open` + cap=2 semaphore + pending flag, subscribed diffs inside the closure (subscription snapshot), identity = canonical path, injected `DeltaSink`, SetWorkbench/GetSnapshot/RetryRepo/Shutdown(ack).
- U4. Invoke commands: direct heavy reads (diffs/log/blob/content with guards + base64/tree with cap 20k) + bus proxies; errors with category+message.
- U5. Wiring: synchronous channel in setup, manage(BusHandle), task spawn (degraded on BackendInit), `set_active_workbench` with `State<BusHandle>`, run-loop `.build().run(|app,event|)` with `RunEvent::ExitRequested` → Shutdown(ack) + block_on.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Full bus (U1–U5) + contract + docs | runtime (`src-tauri/src/bus/**`, `lib.rs`, `workbench/commands.rs` 1 change), deps (`Cargo.toml` + lock: base64), contract (`docs/contracts/`), inline tests, docs (`docs/**`) | develop | optional: "Jira omitted" unless configured | ~700–900 lines of human-authored Rust (incl. tests) + contract/docs; lockfile and docs separate; medium/high risk (integration point) mitigated by dry-run and injected emission |

## Files and Tests

- Code: `src-tauri/src/lib.rs`, `src-tauri/src/bus/{mod,contract,commands}.rs`, `src-tauri/src/workbench/commands.rs` (1 change), `src-tauri/Cargo.toml` (+`base64`).
- Contract: `docs/contracts/bus-contract.md` (events, commands, payloads, dry-run).
- Tests: U1 JSON shape; U2 pure logic in a table; U3 integration with a real watcher (`tempfile`, channel emission); U4 per command; U5 task-level (startup/degraded) + `tauri dev` smoke.
- Docs (separate commits): RDM-006 brainstorm/plan/package, orchestration state, wave 3 closeout (already on the branch).

## Impact Scan

- Changed API contracts: **new frozen contract** (events + invoke commands) — no consumers yet (RDM-007+ will consume them; the U1 dry-run is the early validation). `set_active_workbench` gains a State parameter (no existing wrapper tests — verified). `lib.rs` changes the run-loop (ping/tick smoke must keep working).
- Consumer scan patterns: `rg "set_active_workbench" src/` (frontend) → only template types; vitest does not invoke it.
- Consumers found: none broken; `src/App.tsx` uses ping/tick (intact).
- Contract-drift tests searched: delivered suites (git/paths/workbench/watcher/vitest) do not depend on the bus.
- Required consumer tests: the full suite must stay green (`cargo test`, `npm test`).
- Consumer tests run/skipped: run in the Verification Gate.

## Verification Gate

- [x] `cargo fmt --check` ✓ · [x] `cargo clippy --all-targets -- -D warnings` ✓ · [x] `cargo test` **106/106** (97 base + 9 new from the review) · [x] `cargo build` ✓ · [x] `npm test` 3/3 + `npm run lint` ✓ · [x] `tauri dev` smoke ✓ (window up, no panics, bounded exit) · [x] `docs/contracts/bus-contract.md` with a gap-free dry-run (U1 gate) ✓.
- Surface-aware evidence: runtime → cargo test+clippy; contract → shape test + dry-run doc; wiring → dev smoke; deps → build with base64.
- Production posture evidence: prototype; Windows best-effort until CI (D2).
- **Post-fixes verification (2026-06-15):** resumed after unblocking Bash. Fixed 1 flaky test (non-deterministic initial snapshot A/B order → accept any order); then 2 compile errors (`Manager` trait for `try_state` + `block_on` inference). All green after the code-review fix batch.

## Review Gate

- Code review threshold: P0-P2 (default). Findings below threshold: log unless user marks blocking.
- Suggested personas: correctness (task/channel concurrency, cap, canonical identity), testing, maintainability (BusHandle API for RDM-007), adversarial (snapshot/delta races, storm, degraded).

### Code review result (2026-06-15) — 4 personas (correctness, adversarial, testing, maintainability)

**Applied fixes (P0-P2 + 1 cheap P3), all verified:**

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | P1 corr | `fs-events` emitted an absolute path (the contract requires relative) | strip the repo prefix; `abs` only for stat, `rel` as identity/key |
| 2 | P1 corr | `revision` reset to 0 when re-adding a repo (breaks the contract's monotonicity) | durable `revisions` map; `set_workbench` persists and seeds |
| 3 | P1 corr | a late recompute result resurrected the state of an unmounted repo (zombie delta) | `get_mut` guards in the Batch/RepoError/results arms; drop if not mounted |
| 4 | **P0 adv** | arbitrary file read: `repo` not validated against the workbench | `BusHandle::is_known` allowlist + `ensure_known` in the 6 read commands |
| 5 | P1 adv | exposure of `.git/config` (credentials) via `get_file_content` | reject a `.git` component in `resolve_within` (`path-forbidden`) |
| 6 | P1 adv | OOM: whole file in RAM before truncation | `read_file_content_bounded` (rejects non-regular files; reads ≤ limit+1) |
| 7 | P2 corr | a successful recompute did not clear a terminal error (retry did not revive) | `apply_recalc` clears any error; snapshot triggers recompute of terminals |
| 8 | P2 maint | `GitError`→category mapping duplicated (contract string drift) | `GitError::category()` as the single source; used by delta and command |
| 9 | P2 adv | starvation from special files (FIFO) | covered by the non-regular-file rejection (#6) |
| 10 | P3 adv | a large UTF-8 file degraded to base64 by splitting a char at the cut | preserve the valid UTF-8 prefix (`valid_up_to`) when `error_len()==None` |
| 11 | P3 adv | `ExitRequested` could hang the shutdown | `block_on` with `timeout(3s)` |

**Tests added (9):** exact traversal (separate `path-traversal` vs `not-found` branches), symlink-escape (unix), `.git` rejection, UTF-8 boundary, bounded read + non-regular, tree cap (`truncated`), AE11 degraded startup (`run_bus_inner` seam), allowlist, durable revision after re-add, AE7 purge with no zombie delta, B revive after retry (AE8).

**Scope decision — allowlist (P0) implemented despite being deferred in the Security Gate:** the package had accepted the missing repo-membership validation as out of scope for the prototype. We decided to implement it anyway: it is cheap, does not change behavior for the legitimate frontend (it only passes repos from the snapshot), and hardens consistently with the project's posture (RDM-001 CSP/capabilities). **Surfaced in the release plan for user objection.** This closes the Security Gate's "future hardening."

**Deferred with rationale (non-blocking; follow-up):**
- *Maint P2 — 9-arg functions → struct (`RecalcEngine`/`BusState`):* a behavior-preserving internal refactor; it would churn the concurrency core right at the freeze. Better as a dedicated cleanup unit. (Backlog.)
- *Adv P2 — unbounded command channel / flood:* a design change (backpressure); single-user prototype trust model. (Backlog.)
- *Adv P1 — `get_blob` with no size cap:* the bound lives in the `git` module (`blob_at`), outside this diff; the risk is in already-committed objects (less flexible for an attacker). (Git engine backlog.)
- *P3 — duplicated `now_ms`, extraction of `subscribed_diffs`, `RescanNeeded` integration test.* (Backlog.)

## Security Gate

- Run after work-review loop: not required — no auth/secrets/network/PII. Surfaces: local reads + internal app IPC.
- Security Watch during work: enabled, lightweight — `get_file_content`/`list_repo_tree`/`get_worktree_diff` receive a repo path + a relative path from the frontend: watch that **the requested path, after canonicalization, stays contained within the repo received as the argument** (no `../` escape) and that the size guards are applied before reading. Validating that the repo belongs to the workbench is accepted as not required under the prototype posture (the argument comes from the snapshot the bus itself served); it is recorded as future hardening if the posture changes.
- Security Watch notes: the note above is an implementation requirement of U4 (a traversal test is included in its scenarios).
- Security reviewer: n/a unless the worker is elevated.
- Security review result: not required (covered by the code review's adversarial persona; see Review Gate).
- Required security verification: a U4 test that rejects paths outside the repo (`../`). **MET and extended** (2026-06-15): exact traversal, symlink-escape, `.git` rejection, workbench-membership allowlist, bounded read. The allowlist's "future hardening" is now implemented (see Review Gate).

## CI Break-Prevention And Escalation

- CI risk surfaces: build (base64), clippy, tests (watcher integration: already mitigated with tolerant assertions), dev smoke not automatable.
- Preventive evidence: complete local ladder; CI-only gap = Windows + visual smoke.
- If CI breaks: krt-ci-questor.
- Escalation rule: release-follow-up blocker if the dev smoke fails or if the U1 dry-run finds an unserviceable need (escalates to the user as a product decision).

## Branch and PR Handoff Inputs

- Review unit: RU1 — complete State/Event bus
- Branch name: `feat/state-event-bus` (already created from `develop`)
- Branch/docs rule: carries the RDM-006 planning + the wave 3 closeout docs — **in the branch's working tree, not committed yet**; they go in the final `docs(orchestration)` commit. No separate docs branch.
- PR base: `develop`
- Suggested commit grouping:
  - `docs(contract): bus event and command contract with dry-run` — `docs/contracts/bus-contract.md` — the frozen contract first.
  - `feat(bus): contract types and per-repo live state` — `bus/contract.rs`, pure state in `bus/mod.rs`, `lib.rs` (`pub mod bus;`) — the model.
  - `feat(bus): bus task with selective recompute and subscriptions` — `bus/mod.rs` (task) — the orchestration.
  - `feat(bus): invoke commands and lifecycle wiring` — `bus/commands.rs`, `lib.rs` (setup/run-loop/handler), `workbench/commands.rs`, `Cargo.toml` — the app surface.
  - `chore(generated): update Cargo.lock`.
  - `docs(orchestration): bus brainstorm, plan, and package; wave 3 closeout` — `docs/brainstorms/**`, `docs/plans/**`, `docs/work-packages/**`, `docs/orchestration/**` (excludes `docs/contracts/`, already committed first).
- PR title: `feat: state bus with a frozen event contract for the frontend`
- PR body bullets:
  - In-memory state bus that consumes the watcher, recomputes git only for the affected repo (bounded concurrency), and emits lightweight per-repo deltas with a monotonic revision.
  - Frozen and documented backend↔frontend event and command contract, validated with a dry-run against the intended views (dashboard, diff viewer, watched files, timeline).
  - Subscription to open targets: the delta includes the live diff of the open file/repo, with a synthesized diff for new untracked files.
  - On-demand commands: snapshot, diffs, history, file content (with size/binary guards), full repo tree respecting .gitignore, and retry of repos in error.
  - Complete lifecycle wiring: startup with the persisted active workbench, switching without freezing the UI, degraded mode if watching is unavailable, and clean shutdown.
  - Includes the contract and the planning artifacts in separate docs commits.
- Verification results location: this package's Verification Gate + the execution thread.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional
- Suggested issue type: Task
- Suggested subtask behavior: a single standalone task (one RU only).
- Jira summary: State bus with an event contract for the interface
- Jira description: Build the central component that connects the file watcher, the git read layer, and the workspaces: it maintains each repository's live state, recomputes only what each change affects, and emits lightweight updates to the interface, with heavy data (diffs, history, content, repo tree) served on demand. Defines and documents the event and command contract that every interface view will consume.
- Optional-policy fallback: "Jira omitted: no Jira context/config at preflight" and continue.
