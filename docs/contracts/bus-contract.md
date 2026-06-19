# Bus contract: backend↔frontend events and commands

> **Frozen 2026-06-11.** The frontend views (dashboard, diff viewer, watched files, timeline) consume this contract without renegotiating it; later changes are additive-first (new fields/events/commands, never renames or removals without migration). Source of the types: `src-tauri/src/bus/contract.rs` (Rust, `Serialize`) — the frontend TS types are derived by hand from this document in the view that first introduces them.

## Conventions

- Repo paths: **canonical** (the backend canonicalizes; the frontend uses the path exactly as it receives it in the snapshot/deltas, as an opaque identity).
- File paths: relative to the repo root.
- Timestamps: epoch ms (`u64`) unless otherwise noted.
- Command errors: `{ category: string, message: string }` (safe message, no secrets) — `WorkbenchError` pattern. Git categories: see `error.class` below. Read-containment categories: `repo-not-allowed` (the repo is not in the active workbench), `path-traversal` (the path escapes the repo after canonicalizing), `path-forbidden` (`.git` is not exposed), `not-a-file` (not a regular file), `not-found`, `repository-not-found`.
- **Read allowlist:** every read command (`get_worktree_diff`, `get_commit_diff`, `get_commit_log`, `get_blob`, `get_file_content`, `get_media_content`, `list_repo_tree`) requires `repo` to belong to the active workbench; if not, `repo-not-allowed`. Per-file containment: `get_file_content` and `get_media_content` confine the path within the repo and exclude `.git`; they read with bounds (≤ limit + binary/media→base64).
- Revision: `revision: u64`, monotonic **per repo** and **durable**: if a repo is unmounted and comes back, the counter continues (it does not reset to 0). Consumer rule: apply a delta/snapshot only if its `revision` is greater than the one already known.

## Events (backend `emit` → frontend)

### `tinto://workbench-delta` — state delta for ONE repo

```jsonc
{
  "repo": "/canonical/path/to/repo",
  "revision": 42,
  "status": {                      // RepoStatus (git) — counts derivable from the lists
    "modified": ["src/a.rs"],
    "staged": [],
    "untracked": ["new.txt"]
  },
  "branch": {                      // BranchInfo
    "name": "main", "detached": false, "unborn": false,
    "ahead": 1, "behind": 0        // null if there is no upstream
  },
  "head": {                        // CommitInfo | null (unborn)
    "id": "abc...", "summary": "...", "author": "...", "timestamp": 1760000000
  },
  "last_activity_ms": 1760000000000,
  "error": null,                   // RepoErrorState | null — see classes below
  "metrics": {                      // RepoMetrics (RDM-011)
    "changed_files": 2,
    "lines_added": 30,
    "lines_removed": 8
  },
  "signals": [                      // Vec<PassiveSignal>, omitted when empty
    { "kind": "possible_secret",
      "severity": "critical",
      "path": "src/config.ts",
      "message": "Possible secret marker added" }
  ],
  "subscribed_diffs": null         // Vec<FileDiff> | null — only if the repo has subscribed targets
}
```

- `error.class`: `"transient"` (recomputation GitError) | `"terminal"` (watcher: repo removed/mount failure/classifier). Both classes are cleared when a later recomputation succeeds: the transient one on the next valid recomputation; the terminal one when a successful remount/retry (`retry_repo`, workbench switch, or `get_workbench_snapshot`) triggers a recomputation that re-reads the repo.
- `subscribed_diffs`: target with a file → a list with one `FileDiff` (the file's); full-repo target → the full `worktree_diff`. Subscribed untracked → an all-added synthesized `FileDiff` (with binary/size guards).
- `metrics`: current lightweight repo metrics. `changed_files` counts the current changed path set from status/diff; `lines_added` and `lines_removed` come from the structured worktree diff.
- `signals`: bounded deterministic passive facts. Kinds: `"sensitive_path"`, `"possible_secret"`, `"large_delete"`, `"config_change"`, `"test_change"`. Severities: `"info"`, `"warning"`, `"critical"`. Messages must not include matched secret values or raw added-line content.

### `tinto://fs-events` — Plane 2 events for ONE repo

```jsonc
{
  "repo": "/canonical/path/to/repo",
  "events": [
    { "path": ".env", "kind": "modified",      // "created" | "modified" | "removed"
      "timestamp_ms": 1760000000000,
      "size": 1024,                             // null if the file no longer exists
      "size_delta": 12,                         // null if there is no known previous size
      "signals": [                              // omitted when empty
        { "kind": "sensitive_path",
          "severity": "warning",
          "path": ".env",
          "message": "Sensitive watched file changed" }
      ] }
  ]
}
```

### `tinto://watching-state` — watching availability (workbench-level)

```jsonc
{ "available": false, "reason": "no se pudo inicializar el backend de watching: ..." }
```

Emitted at startup and on changes. `available: false` = degraded mode: data arrives only on demand (`invoke`); deltas do not flow.

## Commands (frontend `invoke` → backend)

| Command | Args | Response | Notes |
|---|---|---|---|
| `get_workbench_snapshot` | — | `{ watching: WatchingState, repos: Vec<RepoDelta> }` | Full current state; the `revision` values let you stitch with the stream. Retries the remount of repos in terminal error. |
| `get_worktree_diff` | `repo` | `Vec<FileDiff>` | Working tree vs HEAD (includes staged; untracked NOT — they live in status). |
| `get_commit_diff` | `repo, commit_id` | `Vec<FileDiff>` | Commit vs first parent. |
| `get_commit_log` | `repo, offset, limit` | `Vec<CommitInfo>` | Paginated by offset. |
| `get_blob` | `repo, commit_id, path` | `FileContent` | Content at a commit. |
| `get_file_content` | `repo, path` | `FileContent` | CURRENT working-tree content (full-file view). |
| `get_media_content` | `repo, path` | `FileContent` | CURRENT working-tree PDF/image content for previews; always base64, 12 MiB guard; rejects non-media extensions. |
| `list_repo_tree` | `repo` | `{ entries: Vec<TreeEntry>, truncated: bool }` | Full working-tree tree respecting `.gitignore` (`ignore` walk), cap 20,000 entries. |
| `set_subscriptions` | `targets: Vec<{repo, path?}>` | `()` | Set of open targets (cap 8); applies from the next recomputation. |
| `retry_repo` | `repo` | `()` | Retries the remount of a repo in terminal error. |

- `FileContent`: `{ encoding: "utf8" | "base64", content: string, truncated: bool }` — 1 MiB guard (truncated) and binary detection (→ base64). Validated relative paths: after canonicalizing they must stay within the repo (no `../`).
- `get_media_content` returns the same `FileContent` shape but always uses `"base64"` and a 12 MiB guard, so visual previews can build `data:` URLs without ambiguity. Supported extensions: `pdf`, `avif`, `bmp`, `gif`, `ico`, `jpeg`, `jpg`, `png`, `svg`, `webp`; anything else returns `unsupported-media`.
- `TreeEntry`: `{ path: string, is_dir: bool }` — flat list; the frontend builds the tree.
- `set_active_workbench` (existing, RDM-005) now additionally triggers the watcher remount and the snapshot of the new workbench (asynchronous — the deltas arrive via the stream).

## Dry-run: view needs → contract

| View (item) | Read/render need | Served by |
|---|---|---|
| Dashboard cards (007) | modified/staged/untracked counts per repo | `workbench-delta.status` / snapshot |
| Dashboard cards (007) | branch, ahead/behind, last commit | `workbench-delta.branch/head` |
| Dashboard cards (007) | live activity indicator | `workbench-delta.last_activity_ms` + delta frequency |
| Dashboard cards (007) | card error states (invalid path, git failure, permission) | `workbench-delta.error` (transient/terminal classes) + `retry_repo` |
| Dashboard/onboarding (007) | create/switch workbench and view its state | RDM-005 commands + `get_workbench_snapshot` + stream |
| Left tree (007/008) | full repo tree, highlighting changes | `list_repo_tree` + `workbench-delta.status` (path lists) |
| Diff viewer (008) | working-tree diff of a repo | `get_worktree_diff` |
| Live diff (008) | diff updated only while the agent writes | subscription (`set_subscriptions`) → `workbench-delta.subscribed_diffs` |
| Highlighted full file (008) | CURRENT content + hunks | `get_file_content` + the target's `FileDiff` |
| PDF/image preview | CURRENT media bytes + extension-derived MIME | `get_media_content` |
| New agent file (008) | diff of an untracked file | all-added synthesized `subscribed_diffs` |
| Plane 2 (009) | list of FS events with kind/timestamp/size/delta | `tinto://fs-events` |
| fs_watch editor (009) | edit per-repo patterns | RDM-005 commands (`update_repo`) — outside this contract |
| Timeline (010) | cross-repo chronological feed | frontend accumulation of `workbench-delta`/`fs-events` (timestamps) |
| Timeline (010) | navigation through commits with their diffs | `get_commit_log` + `get_commit_diff` |
| Timeline (010) | orphan detection (dirty without a commit for a while) | `status` + `head.timestamp` + `last_activity_ms` (frontend heuristic) |
| Signals (011) | raw facts for highlights/metrics | `RepoDelta.metrics`, `RepoDelta.signals`, `FsEvent.signals` additive fields |
| Degraded | "watching unavailable" visible | `tinto://watching-state` |

No gaps: each need maps to an existing event/command. (RDM-011 will add additive fields; RDM-012 consumes via the views.)
