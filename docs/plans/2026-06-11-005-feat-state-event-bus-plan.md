---
title: "feat: State/Event bus — backend→frontend integration (RDM-006)"
type: feat
date: 2026-06-11
origin: docs/brainstorms/2026-06-11-rdm-006-state-event-bus-requirements.md
---

# feat: State/Event bus — backend→frontend integration (RDM-006)

## Summary

Add the `bus` module: a central task that owns the `FsWatcher`, consumes its messages, selectively recomputes git via `Git2Engine` inside `spawn_blocking`, holds the live state of the active workbench, and emits lightweight deltas to the frontend with `AppHandle::emit`; heavy reads are served through `invoke` commands (snapshot, diffs, log, blob, current content, repo tree, retry, subscription). The event/command contract is **documented and frozen** (`docs/contracts/`), validated with a dry-run against the needs of RDM-007..010. Complete lifecycle wiring in `lib.rs` (debt from RDM-004/005), with degraded startup if the watching backend fails.

## Requirements Trace

- R1, R2, R2b (live state, snapshot, monotonic revision) → U1 (types) + U2 (state) + U3 (snapshot/emission)
- R3, R4 (watcher consumption, selective recompute) → U2 (pure decision logic + FsEvents) + U3 (execution)
- R5, R6 (error classes, isolation) → U2 (logic) + U3 (flow)
- R7, R8 (lightweight deltas, bounded coalescing + budget ≤2s p95) → U1 (types) + U2 + U3 (emission)
- R9, R9b (multi-target subscription, synthesized untracked) → U2 (types) + U3 (diffs in emits) + U4 (command)
- R10 (dedicated Plane 2 event) → U1 (types) + U2 + U3
- R11, R12 (invoke commands, serialized errors) → U1 (shapes) + U4
- R13 (lifecycle wiring, synchronous channel, degraded) → U5
- R14, AE1–AE11 → tests for U3 (bus with real watcher over fixtures) + U4 (commands) + U5 (startup)
- Freeze dry-run (Key Decision condition) → U1
- Origin: see `docs/brainstorms/2026-06-11-rdm-006-state-event-bus-requirements.md`

## Key Technical Decisions

- **The bus is an owning task, not shared state.** An async task (`tokio::spawn` inside `tauri::async_runtime`) owns the `FsWatcher`, the watcher's single `Receiver`, and the live state. The outside world talks to it through a **command channel** (`mpsc::unbounded`) whose `Sender` is created **synchronously in `setup`** and lives as managed state (`BusHandle`): early `invoke` calls enqueue without race or panic (R13). This avoids `Mutex<Option<FsWatcher>>` and long-held locks: `watch_workbench(&mut)` and `shutdown(self)` stay natural inside the task.
- **Command channel messages:** `BusCommand::{ SetWorkbench(Vec<RepoEntry>), GetSnapshot(reply), Subscribe(targets, reply), RetryRepo(path, reply), Shutdown(ack) }` + heavy reads do NOT go through the task: the diff/log/blob/content/tree `invoke` commands run directly in `spawn_blocking` over `Git2Engine::open`/FS (they are pure reads with no bus state), keeping the bus task free. **Diffs for subscribed targets are also computed outside the task:** inside the same recompute `spawn_blocking` closure, capturing the subscription snapshot when it is spawned (a subscription change takes effect on the next recompute/emission) — the task only holds state and emits.
- **Repo identity = canonical path.** The watcher canonicalizes on mount and emits everything with the canonical path. The bus canonicalizes the `RepoEntry` values in `SetWorkbench` (same fallback: `canonicalize().unwrap_or(original)`) and uses the canonical path as the sole key for live state, emitted deltas, and command lookups (`retry_repo`, `get_worktree_diff`...). Without this, a repo with a symlink component produces watcher messages that do not match state and it never emits.
- **Selective recompute:** per watcher batch, the task decides the set to recompute (`GitMeta`⇒branch+head+status; `Plane1`⇒status; `.gitignore` Plane1 or `RescanNeeded`⇒full) and runs it in `spawn_blocking` (engine `Git2Engine::open(path)` inside the closure — cheap in libgit2). **Recompute concurrency cap = 2** (tokio semaphore): bounds the `RescanNeeded` broadcast to N repos without killing the lightweight principle; the rest wait in queue.
- **Minimal coalescing and latency budget:** batches already arrive at ≤1/repo/s (watcher). The bus only consolidates when a repo's recompute is in flight and more triggers for the same repo arrive (per-repo "recompute pending" flag → one more recompute when it finishes, not a queue). No additional emission throttle: each finished recompute emits its delta — write→emit stays ≤ ~1.6s p95 (300ms debounce + ≤1s watcher throttle + recompute), within the ≤2s budget.
- **Contract frozen in `docs/contracts/bus-contract.md`:** events `tinto://workbench-delta` (per-repo delta: monotonic revision, counts, branch, last commit, changed files, activity, error {class, category, message}, diffs of subscribed targets), `tinto://fs-events` (Plane 2: type, path, timestamp, size, size delta), `tinto://watching-state` (available/degraded). Commands: `get_workbench_snapshot`, `get_worktree_diff(repo)`, `get_commit_diff(repo, commit)`, `get_commit_log(repo, offset, limit)`, `get_blob(repo, commit, path)`, `get_file_content(repo, path)`, `list_repo_tree(repo)`, `set_subscriptions(targets)`, `retry_repo(repo)`. Payloads built on the git engine's `Serialize` types; **blob/file content as `{ encoding: "utf8"|"base64", content: String, truncated: bool }`** with a size guard (1 MiB) and binary detection; tree as a flat list of relative paths + dir flag (the frontend builds the tree), walked with `ignore::WalkBuilder` and a **cap of 20,000 entries + `truncated` flag** (pinned in the contract before the freeze). New dependency: `base64` (binary encoding of `FileContent` — there is no importable base64 today). The document includes the **dry-run table** (RDM-007..010 need → event/command that serves it) — U1.
- **Synthesized untracked (R9b):** for a subscribed target that shows up in `status.untracked`, the bus builds an all-added `FileDiff` by reading the file (binary/size guards); once it becomes tracked it enters the normal `worktree_diff`.
- **Plane 2 with size:** when forwarding `Plane2` events, the task does best-effort `fs::metadata` and keeps `last_known_size` per watched path in its state (R1 extended); `delta = new - last`; fields omitted if the file does not exist (deletions).
- **Errors in two classes:** `RepoErrorState { class: Transient|Terminal, category, message }`. Transient (GitError) is cleared on the next OK recompute. Terminal (watcher) persists; `retry_repo` re-invokes `watch_workbench` with the current set (the watcher's `dead_roots` mechanism does the actual remount); switching and `GetSnapshot` also retry repos in terminal state.
- **Global degraded mode:** if `FsWatcher::new()` fails, the bus task starts without a watcher, emits/exposes `watching: unavailable`, and keeps serving snapshot on demand and read commands (R13/AE11). The `lib.rs` smoke ping/tick stays intact.

## Output Structure

```text
docs/contracts/
└── bus-contract.md         # frozen contract: events, commands, payloads, RDM-007..010 dry-run
src-tauri/src/
├── lib.rs                  # + `pub mod bus;`, setup: synchronous channel + task spawn, .manage(BusHandle), command registration, run-loop with RunEvent (shutdown)
├── workbench/commands.rs   # only: set_active_workbench notifies the BusHandle (U5, declared minimal change)
└── bus/
    ├── mod.rs              # bus task: live state, watcher consumption, recompute, emission, BusCommand/BusHandle
    ├── contract.rs         # contract types: RepoDelta, FsEvent, WatchingState, RepoErrorState, SubscriptionTarget, TreeEntry, FileContent
    └── commands.rs         # invoke commands (direct heavy reads + proxies to the bus channel)
```

## Implementation Units

### U1. Frozen contract + dry-run against consumers

- **Goal:** Pin the contract (doc + Rust types) and prove it serves RDM-007..010 before building the bus.
- **Requirements:** freeze Key Decision; R2b, R7, R10, R12 (shapes); input for R11.
- **Dependencies:** None.
- **Files:** `docs/contracts/bus-contract.md`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs` (`pub mod bus;`), `src-tauri/src/bus/mod.rs` (minimal skeleton that compiles).
- **Approach:** Write the contract doc with the dry-run table: for each view (dashboard cards 007, diff viewer + full file 008, Plane 2 list + fs_watch editor 009, timeline 010) enumerate its reads and map them to an event/command. Define the `Serialize` types in `contract.rs` (RepoDelta with `revision: u64`, FsEvent, WatchingState, RepoErrorState, SubscriptionTarget, TreeEntry, FileContent). Any unservable need is resolved here (blocks the freeze).
- **Test scenarios:** Functional coverage arrives with U3/U4; U1 includes a serialization test of `RepoDelta`/`FileContent` (stable JSON shape) as the contract anchor.
- **Verification:** complete doc with a gap-free dry-run table; clean `cargo check` + clippy; green JSON-shape test.

### U2. Bus live state

- **Goal:** The in-memory per-repo state model and its pure update logic (no real watcher or Tauri).
- **Requirements:** R1, R2b (revision), part of R3/R4 (per-batch recompute decision, FsEvents with size), part of R7/R9/R10 (delta construction), R5 (error classes).
- **Dependencies:** U1.
- **Files:** `src-tauri/src/bus/mod.rs` (state + pure logic, `#[cfg(test)]` inline).
- **Approach:** `RepoLiveState { status, branch, head, last_activity_ms, error: Option<RepoErrorState>, revision, last_known_sizes: HashMap<PathBuf, u64> }`; pure functions: apply recompute result (→ delta with incremented revision; clears transient error), apply `RepoError` (terminal class), build `FsEvent`s from a Plane2 batch + sizes, decide the recompute set from a batch (`GitMeta`/`Plane1`/`.gitignore`/`RescanNeeded`).
- **Patterns to follow:** pure state and tests like `paths::PathClassifier` (I/O-free logic, table-testable).
- **Test scenarios:** monotonic revision per application; transient error cleared by an OK recompute and terminal not; recompute decision by batch content (table: GitMeta⇒branch+head+status, Plane1⇒status, gitignore⇒full, Rescan⇒full); FsEvents with size/delta and omission on deletions.
- **Verification:** green `cargo test` for U2; clean clippy.

### U3. Bus task: watcher + recompute + emission

- **Goal:** The full owning task: consumes the watcher, recomputes with cap=2, emits deltas and FS events, handles subscriptions, snapshot, and retry.
- **Requirements:** R2, R3, R4, R5, R6, R7, R8, R9, R9b, R10; AE1–AE6, AE8, AE10.
- **Dependencies:** U1, U2.
- **Files:** `src-tauri/src/bus/mod.rs` (task + `BusCommand`/`BusHandle`), inline integration tests (real watcher over `tempfile` fixtures, no Tauri: emission is abstracted behind an `Emitter` trait/closure to test without AppHandle).
- **Approach:** `select!` loop over: the command channel, the watcher receiver, and a **results channel** of in-flight recomputes (preferred over `JoinSet` — `join_next()` over an empty set resolves `None` immediately and would spin the `select!`; if `JoinSet` is used, the arm carries a guard `if !set.is_empty()`). Recompute = `spawn_blocking(move || { let engine = Git2Engine::open(&path)?; ... })` with a cap=2 semaphore and a per-repo "pending" flag (coalescing); **diffs for subscribed targets are computed inside the same closure** (captures the subscription snapshot when spawned): target with a file → `FileDiff` filtered from `worktree_diff`; repo target without a file → full `Vec<FileDiff>`; untracked → synthesized all-added with guards. Subscriptions: set cap N=8. `SetWorkbench`: canonicalizes paths, remount (watch_workbench + full snapshot). `GetSnapshot`/`RetryRepo`/`Shutdown` reply over a oneshot. The real emission (`AppHandle::emit`) is injected as a closure **`DeltaSink`** (do not call it `Emitter`: it collides with `tauri::Emitter`, already imported in lib.rs) so tests use a channel.
- **Execution note:** test-first on the central flow (Plane1 batch → recompute → delta with revision) before adding subscriptions/retry.
- **Test scenarios:** AE1 (delta only for the touched repo), AE2 (GitMeta updates branch/head; corrupt repo isolates the error), AE3 (diff only for the subscribed; on clear, lightweight deltas), AE4 (overlapping triggers ⇒ consolidated emissions), AE5 (RescanNeeded ⇒ full recompute; broadcast with cap respected), AE6 (Plane2 ⇒ FsEvent with size), AE8 (retry revives terminal; transient clears itself), AE10 (synthesized all-added untracked), AE7-switching (`SetWorkbench` with a new set remounts and produces snapshot/deltas for the new workbench; the no-freeze of the main thread is left to the U5 smoke).
- **Verification:** green `cargo test` for U3 (real git fixtures with the git module's `test_fixtures` if applicable); clean clippy.

### U4. Invoke commands

- **Goal:** The full `invoke` surface of the contract.
- **Requirements:** R11, R12; AE9.
- **Dependencies:** U1 (types), U3 (BusHandle for snapshot/subscription/retry).
- **Files:** `src-tauri/src/bus/commands.rs`, `src-tauri/src/lib.rs` (registration in `generate_handler!`), `src-tauri/Cargo.toml` (+ `base64` for binary `FileContent`).
- **Approach:** Direct heavy reads: `get_worktree_diff`/`get_commit_diff`/`get_commit_log`/`get_blob` = `spawn_blocking` + `Git2Engine::open` (thin wrappers, no caching — review note); `get_file_content` = FS read with 1 MiB guard + binary detection → `FileContent{encoding, content, truncated}`; `list_repo_tree` = `ignore::WalkBuilder` with an entry cap → `Vec<TreeEntry>`. Proxies to the bus: `get_workbench_snapshot`, `set_subscriptions`, `retry_repo` via `BusHandle` (oneshot). Errors → category+message serialization (`WorkbenchError` pattern).
- **Test scenarios:** AE9 per command (incl. tree that excludes gitignored entries and respects the cap; binary content → base64/truncated; file >1 MiB → truncated); serialized errors with category; **traversal: a relative path with `../` that escapes the repo (verified after canonicalizing) is rejected with a categorized error** (a requirement from the package's Security Watch).
- **Verification:** green `cargo test`; clean clippy.

### U5. App lifecycle wiring

- **Goal:** Everything connected at startup: synchronous channel, bus task, managed state, startup with the active workbench, switching, degraded mode, shutdown.
- **Requirements:** R13; AE7, AE11.
- **Dependencies:** U3, U4.
- **Files:** `src-tauri/src/lib.rs` (setup + manage + handler), `src-tauri/src/workbench/commands.rs` (only: `set_active_workbench` notifies the `BusHandle` after persisting — declared minimal change).
- **Approach:** In `setup`: create the command channel synchronously, `.manage(BusHandle)`, and `tauri::async_runtime::spawn` the bus task (which attempts `FsWatcher::new()`; if `BackendInit` ⇒ degraded mode with `watching-state`). The task loads the active workbench from the `WorkbenchStore` and mounts. `set_active_workbench` gains a `State<'_, BusHandle>` parameter and sends `BusCommand::SetWorkbench` after persisting (there are no existing tests over the command-wrappers — the workbench tests live over the store; do NOT use `Option<State<_>>`: Tauri does not support it as a command argument). **On-exit:** replace `.run(generate_context!)` with `.build(generate_context!)?.run(|app, event| ...)` and, on `RunEvent::ExitRequested`, send `BusCommand::Shutdown(ack)` and `tauri::async_runtime::block_on` the ack so that `watcher.shutdown().await` (flush of pending batches) completes before the process dies — without this the best-effort `Drop` runs (abort, no flush). Ping/tick intact.
- **Test scenarios:** AE7/AE11 at the task level (startup with a persisted workbench mounts and produces a snapshot; without a backend ⇒ degraded with live reads) — with injected emission; the end-to-end visual smoke arrives with RDM-007. Test expectation for the real Tauri wiring: manual smoke `tauri dev` (ping/tick still run + log of the bus mounted).
- **Verification:** full `cargo test` green; `npm test`/`lint` intact; `cargo build`; `tauri dev` smoke (window opens, no panics, bus mounted in log).

## Scope Boundaries

- Same as the origin: no UI (RDM-007+), no persistence of live state, no passive signals (RDM-011), no notifications (RDM-012). Changes to delivered modules limited to the declared wiring (`lib.rs` + notification in `set_active_workbench`).

### Deferred to Follow-Up Work

- TS contract types for the frontend → RDM-007 (derives them from the frozen doc).
- Optimization/caching of heavy reads → when its real consumer exists (RDM-008/010).
- Recompute perf baseline on large repos (CLI escape-hatch criterion) → still deferred (RDM-002).

## Open Questions

- None blocking. Planning resolutions: state lives in the bus task (no shared Mutex); heavy reads outside the task; recompute cap = 2; subscription cap N=8; coalescing via pending-flag with no extra throttle (≤2s budget met); blob/content as `{encoding, content, truncated}` with a 1 MiB guard; flat tree with cap (exact value at execution time); snapshot is served via `invoke` and is also emitted as deltas after mounting (the R2b revision resolves ordering); injected emission for testability.

## Risks & Dependencies

- **Frozen contract with no consumer built:** mitigated by the U1 dry-run (blocks the freeze if something is unservable) and the additive-first policy; residual risk accepted.
- **Large-repo recompute blocks the cap:** with cap=2, two slow repos delay the rest; the pending-flag avoids losses. Recorded as a point for the deferred perf baseline.
- **Bus tests without Tauri:** injected emission avoids AppHandle in tests; the real `setup` wiring is only smoke-verified (`tauri dev`) until RDM-007.
- **`set_active_workbench` gains a dependency on the BusHandle:** a minimal and declared change; the delivered workbench tests live over the store (there are no tests of the command-wrappers), so the required `State<'_, BusHandle>` parameter breaks nothing. Alternative if real optionality is needed: `AppHandle` + `try_state::<BusHandle>()`.

## Verification Strategy

Local ladder: `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo test` (U2 pure + U3 integration with real watcher + U4 commands) → `cargo build` → `npm test` + `npm run lint` (intact) → `tauri dev` smoke (window + bus mounted). The contract doc with a complete dry-run is the U1 gate before implementing U3/U4.
