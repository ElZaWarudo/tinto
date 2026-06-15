---
date: 2026-06-11
topic: rdm-006-state-event-bus
---

# RDM-006 — State / Event bus (backend→frontend integration): requirements

## Summary

Build the integration core (§6/§7): an in-memory state bus that connects what has already been delivered — watcher (RDM-004), `GitEngine` (RDM-002), workbenches (RDM-005) — and exposes it to the frontend. It consumes the watcher's messages, triggers a git recompute of **only the affected repo** (via `spawn_blocking`), keeps the live state of the active workbench, coalesces deltas, and emits **lightweight** events to the frontend; the heavy data (diffs, log, blobs, repo tree) is served on demand via `invoke` commands, with a **subscription** that adds the live diff of the open targets. This item **freezes the backend↔frontend event and command contract** that the entire frontend (RDM-007..011; RDM-012 consumes it indirectly through those views) will rely on.

## Key Decisions

- **Lightweight push + subscription to the open diff.** The bus always pushes lightweight per-repo deltas: status counts (modified/staged/untracked), branch/ahead-behind, latest commit, list of changed files, and an activity marker. Diffs, log, and blobs are requested via `invoke`. For the live diff (§7, RDM-008): the frontend declares which repo/file it has open (subscription), and the bus includes that target's diff in the emits for that repo. Lightweight by default; the real live diff only where the user is actually looking. User decision 2026-06-11.
- **The contract is frozen here — with a dry-run against the consumers.** Event names, payload shapes, and `invoke` command signatures are defined and documented in this item; RDM-007..011 consume them without renegotiating (additive-first afterwards). **Freeze condition:** the plan must enumerate the read/render needs of RDM-007..010 and demonstrate that each one is serviceable by a named event or command; a need that cannot be served blocks the freeze — it is not patched in later. The card→diff drill-through is expressible via the subscription.
- **Full repo tree in the UI (user decision 2026-06-11):** the left tree is an explorer of the full repo (editor-style, highlighting changes), not just changed files. The contract freezes a **repo tree listing** command (a walk of the working tree respecting `.gitignore` via the `ignore` crate, already a dependency; without touching `GitEngine`), with pagination/limits for large repos defined in the plan.
- **Layers without duplication (RDM-004 boundary already resolved):** the watcher debounces **and throttles** raw FS batches (≤1 batch/repo/interval already guaranteed upstream); the bus **coalesces computed deltas** and only applies emit throttling to genuinely bursty cases (RescanNeeded in a burst, overlapping snapshot+deltas) — not a second blind throttle over already-limited batches. **End-to-end latency budget for the live diff: write→emit ≤ 2 s p95** (300 ms debounce + ≤1 s watcher throttle + recompute); the values the plan sets must respect this. The bus never re-debounces FS; the watcher never recomputes git.
- **Selective, blocking-isolated recompute.** Each watcher batch triggers a recompute of **only the affected repo**, wrapping the (synchronous) git engine in `tauri::async_runtime::spawn_blocking`. The `GitEngine` trait has no constructor: the bus uses `Git2Engine::open` directly (opening a repo is cheap in libgit2 — a contract documented on `Git2Engine`, not on the trait) or receives a factory — the plan decides. `GitMeta` (commit/branch) recomputes branch/head/status; `Plane1` recomputes status (+ diff of subscribed targets); `Plane2` is forwarded as FS events without git.
- **Lifecycle wiring (registered debt from RDM-004/005):** the bus creates the `FsWatcher` **inside `tauri::async_runtime`** (not in the synchronous body of `setup`). Constraints of the delivered API: `watch_workbench` requires `&mut` and `shutdown` consumes `self`, so the watcher is **owned by the bus task** (or `Mutex<Option<FsWatcher>>` as managed state) — the plan fixes the form. **The command channel into the bus is created synchronously in `setup`** so that `invoke` commands arriving before the async init finishes are neither lost nor panicked. **Switching workbenches only notifies the bus** (send over the channel); the remount and snapshot run in the bus task / `spawn_blocking`, never inline in the command (this avoids freezing the main thread with classifier walks + git recomputes). The RDM-001 ping/tick smoke instrumentation is kept intact (RDM-007 replaces it).
- **Watcher contracts honored:** `RescanNeeded` ⇒ full recompute of the repo, **with bounded concurrency** (the watcher emits it for ALL mounted repos on a kernel overflow; the N full recomputes are serialized or capped with a small limit so as not to violate the lightweight principle precisely under saturation); a batch with a `.gitignore` Plane1 ⇒ full status recompute; `RepoError` ⇒ repo error state emitted to the frontend.
- **Two-class error semantics (contract for RDM-007):** **transient** errors (a `GitError` from a recompute) clear themselves on the next valid recompute. **Terminal watcher** errors (`RepoRemoved`, `MountFailed`, `ClassifierInit`) do NOT self-clear — the repo stops producing events until an explicit remount. The contract exposes the distinction and a **repo retry `invoke` command** (re-calls `watch_workbench` with the current set); additionally the bus retries the remount when switching workbenches or when a snapshot is requested. Without this, RDM-007 would design recovery states that do not exist.
- **Global degraded state:** if `FsWatcher::new()` fails (`BackendInit`, e.g. the inotify instance limit), the app starts **degraded with a signal** — the snapshot and the contract expose a workbench-level "watching unavailable" state; never a setup crash. On-demand reads (`invoke`) keep working without watching.

## Requirements

**Live state and snapshot**

- R1. The bus keeps the active workbench's state in memory: per repo, the latest `RepoStatus` (counts + lists), `BranchInfo`, latest commit, recent-activity marker, and error state if any.
- R2. On loading/switching a workbench, the bus takes an **initial snapshot** of all its repos (full git recompute per repo) and exposes it; the frontend can request it via a read command (`invoke`) at any time (full current state, not just deltas).
- R2b. Snapshot and deltas carry a **monotonic per-repo revision** (or emission timestamp) with the "apply only if newer" rule, so consumers can stitch snapshot (pull) + delta stream (push) without race conditions (Tauri listeners miss events emitted before they register).

**Watcher consumption**

- R3. The bus consumes the watcher's channel: a batch from a repo triggers a recompute of that repo only, according to its content: `GitMeta` ⇒ branch/head/status; `Plane1` ⇒ status; `Plane2` ⇒ forwarded as FS events with type, path, timestamp, and **size + size delta** — the watcher does not provide these: **the bus does a best-effort `stat()` when forwarding and keeps the last known size per watched path** (extending the R1 state); the field is omitted if the file no longer exists. (This closes the data point RDM-009 promises; without it the frozen contract would make it unrenegotiable.)
- R4. `RescanNeeded` from a repo ⇒ full recompute of that repo, with the bounded concurrency from the Key Decision (the watcher broadcasts it to all repos at once). A batch containing a `.gitignore` `Plane1` ⇒ full status recompute of the repo.
- R5. Two-class errors (see Key Decision): a transient `GitError` clears on the next valid recompute; terminal watcher errors (`RepoRemoved`/`MountFailed`/`ClassifierInit`) persist until remount — via the retry command (R11), the workbench switch, or the snapshot request. The contract distinguishes both classes in the payload.
- R6. The recompute uses `GitEngine` via `spawn_blocking`; git failures (`GitError`) are mapped to the repo's error state without taking down the bus or the other repos.

**Emission to the frontend (event contract)**

- R7. The bus emits **lightweight** per-repo events with the state delta (counts, branch, latest commit, changed files, activity, error). No diffs, except the subscribed target (R9).
- R8. Coalescing bounded to the genuinely bursty cases (watcher batches already arrive at ≤1/repo/interval): consolidated emissions when recomputes of the same repo overlap (RescanNeeded + batch + subscription change), repos independent of one another, and the live-diff latency budget (write→emit ≤ 2 s p95) is respected.
- R9. **Subscription (set of targets, small cap N):** the frontend declares the open targets — each one a repo, optionally with a file. While subscribed, the emits for those repos include their diff(s): a `FileDiff` for an open file, the working tree's `Vec<FileDiff>` list for an open repo. The set (not a single target) keeps splits/multiple open files expressible (a roadmap UI open question) without breaking the contract; the UI's v1 can use just one. Changing/clearing the subscription is an `invoke` command.
- R9b. **Untracked with synthesized diff:** a subscribed untracked target (the most common case: the agent CREATES a file) produces a synthesized all-added `FileDiff` from the working tree content (git's `worktree_diff` excludes untracked — without this, the live diff would show an empty panel precisely in the highest-value case). Binary/size guard applied.
- R10. Plane 2 events (R3) are emitted in their own event, separate from the git delta, suitable for the watched-files section (RDM-009).

**Invoke commands (on-demand read contract)**

- R11. `invoke` commands for: snapshot of the active workbench state (R2), working tree diff of a repo, diff of a commit, paginated log of a repo, blob content (commit+path), **current content of a working tree file** (direct FS read with binary/size guards — needed by RDM-008's full-file view; `blob_at` only serves content at a commit), **repo tree listing** (a walk respecting `.gitignore` via the `ignore` crate, with limits for large repos — feeds the UI's full tree), **retry of a repo in terminal error** (R5), and subscription management (R9). The responses use the existing `Serialize` types from `GitEngine` (`RepoStatus`, `BranchInfo`, `CommitInfo`, `FileDiff`) **except** blob/file content (`Vec<u8>` — its serialized representation, e.g. UTF-8 text or base64, is defined in the plan along with the shapes) and the tree (a new shape from the plan).
- R12. Errors crossing to the frontend (from git, watcher, or bus) are serialized with category + safe message, mappable to UI states (the already-delivered `WorkbenchError` pattern).

**Lifecycle**

- R13. The bus and the `FsWatcher` are created at app startup inside `tauri::async_runtime`; the watcher is owned by the bus task (or `Mutex<Option<_>>` — the plan decides); the bus command channel is created synchronously in `setup` (no race with early `invoke`s); the persisted active workbench is mounted if it exists; switching notifies the bus (remount + snapshot off the main thread). If `FsWatcher::new()` fails (`BackendInit`), the app starts degraded with the contract's workbench-level signal, without a crash. Clean shutdown when the app closes.
- R14. There are tests for: initial snapshot + monotonic revision, selective recompute by batch type, handling of `RescanNeeded` (incl. the broadcast's bounded concurrency)/`RepoError` (transient vs terminal classes + retry)/`.gitignore`, emission coalescing, multi-target subscription (diff only when subscribed; synthesized untracked), invoke commands (incl. tree and current content), degraded state from `BackendInit`, and per-repo error isolation.

## Acceptance Examples

- AE1. **Covers R2, R7.** When activating a workbench with two repos, the frontend receives (or can request) the full state of both; touching a file in repo A produces a lightweight delta for A only, with its updated counts.
- AE2. **Covers R3, R6.** A `GitMeta` batch (new commit) updates the branch/latest commit of the affected repo; a git failure in a corrupt repo produces its error state without affecting the other repo.
- AE3. **Covers R9.** With no subscription, deltas carry no diffs. After subscribing to repo A (file X), the next delta from A includes X's `FileDiff`; on clearing the subscription, the lightweight deltas return.
- AE4. **Covers R8.** A burst of batches from the same repo produces consolidated emissions at a bounded rate, not one emission per batch.
- AE5. **Covers R4.** A `RescanNeeded` for repo A produces a full recompute and a fresh delta of A.
- AE6. **Covers R10.** Touching a Plane 2 file (a watched `.env`) produces its own FS event with type/path/timestamp, with no git recompute.
- AE7. **Covers R13.** On starting the app with a persisted active workbench, the watcher ends up mounted and the snapshot available without user action; `set_active_workbench` remounts and produces the new workbench's snapshot without freezing the main thread.
- AE8. **Covers R5, R12.** A terminal `RepoError` for repo A produces its delta with a serialized error state (class + category + safe message); the retry command after recreating the repo revives it; a transient `GitError` clears itself on the next valid recompute.
- AE9. **Covers R11.** Each `invoke` command (snapshot, diffs, log, blob, current content, tree, retry, subscription) responds with its serialized type or an R12 error; the tree of a repo with a gitignored `node_modules/` does not include it.
- AE10. **Covers R9b.** With a freshly created (untracked) file subscribed, the delta carries its synthesized all-added `FileDiff`; once it becomes tracked, it moves to the normal diff.
- AE11. **Covers R13 (degraded).** If the watching backend cannot be initialized, the app starts, the snapshot exposes the "watching unavailable" state, and the read commands keep serving data.

## Scope Boundaries

- **Includes:** the in-memory bus, the lifecycle wiring (watcher owned by the bus, synchronous command channel, remount from switching, degraded mode from `BackendInit`), the selective recompute, the full event + `invoke` command contract documented (incl. repo tree, current content, retry), the multi-target subscription with synthesized untracked, the best-effort `stat()` + last-known-size for Plane 2, the bounded coalescing, and the tests.
- **Excludes:** all rendering/UI (RDM-007+ consumes the contract), persistence of the live state (memory only; SQLite is a future step §8), passive signals/highlights (RDM-011 — the bus only carries facts), notifications (RDM-012), and scope changes in the delivered modules beyond the declared wiring.

## Dependencies / Assumptions

- Depends on RDM-002 (`GitEngine` ✅), RDM-004 (watcher ✅), and RDM-005 (workbenches ✅), all on `develop`.
- Assumes the watcher's documented contract: `Batch | RepoError | RescanNeeded` messages, remount via API, `FsWatcher::new()` requires a tokio context.
- Assumes a synchronous git engine wrapped in `spawn_blocking`, constructed from the path inside the closure — a contract documented on `Git2Engine` ("opening a repo is cheap in libgit2"); the trait has no constructor, so the bus uses `Git2Engine::open` or a factory (plan).
- The ping/tick smoke instrumentation stays until RDM-007.

## Outstanding Questions

- **Deferred to Planning:** the exact event names (`emit`) and payload shapes; the exact signature of each `invoke` command; where the state lives (bus task vs `Mutex<Option<_>>`); coalescing values within the latency budget; encoding of blob/file content (UTF-8 vs base64) and the tree's shape/limits; the form of the contract document (`docs/` or hand-written TS types in RDM-007?).
- **Deferred to Planning:** per-file diff semantics (filter `worktree_diff` or a targeted diff?), the subscription's cap N, and whether the initial snapshot is emitted as an event or only served via `invoke` (interacts with the R2b monotonic revision).
- **For the work-package:** evaluate the roadmap's suggested split (event contract vs orchestration) against the criterion of noisy stacked PRs. Commands without an immediate consumer (log/blob/commit-diff → RDM-008/010) are kept as thin wrappers with no caching or speculative state.
- **Cross-item (RDM-007/008):** the card→diff drill-through is covered by the subscription; the UI's full tree is covered by the listing command (user decision 2026-06-11).
