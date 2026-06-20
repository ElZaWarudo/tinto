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

### `tinto://agent-session-output` - PTY output chunk for ONE agent session

```jsonc
{
  "session_id": "sess-1",
  "chunk_base64": "SG9sYQ0K",
  "timestamp_ms": 1760000000000
}
```

Chunks are live only; the first ACI-002 stream bridge does not replay historical output produced before a frontend listener attaches. `chunk_base64` preserves PTY bytes, ANSI escape sequences, and partial UTF-8 boundaries. The frontend decodes the bytes at the terminal surface boundary.

### `tinto://agent-session-change-log` - changed paths for ONE agent session

```jsonc
{
  "session_id": "sess-1",
  "changes": [
    { "path": "src/a.ts", "kind": "modified", "timestamp_ms": 1760000000000 }
  ]
}
```

`kind` is `"created" | "modified" | "removed"`. Change logs are emitted when sessions are listed or reverted; they are also embedded in `AgentSession.change_log`.

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
| `start_agent_session` | `repo, agent_type` | `session_id: string` | Starts an allowlisted agent (`claude`, `codex`, `opencode`) in a PTY for a repo in the active workbench. |
| `stop_agent_session` | `session_id` | `()` | Stops the tracked PTY process/session. |
| `list_agent_sessions` | none | `Vec<AgentSession>` | Returns known sessions after refreshing completed process statuses. |
| `agent_binary_available` | `agent_type` | `bool` | Checks the allowlisted agent binary through PATH lookup. Known missing binaries return `false`; unsupported agent ids return `unsupported_agent`. |
| `write_agent_session_input` | `session_id, input_base64` | `()` | Writes decoded bytes to a running session's PTY stdin. Invalid base64 returns `invalid_input`; stopped/exited sessions return `session_not_running`. |
| `resize_agent_session` | `session_id, cols, rows` | `()` | Resizes a running session's PTY. `cols` and `rows` must be positive; invalid dimensions return `invalid_terminal_size`. |
| `revert_session` | `session_id, user_consent` | `AgentSession` | Restores the repo to the session checkpoint. `user_consent=false` returns `consent_required`; running sessions return `session_still_running`; repeated revert is idempotent. |

- `FileContent`: `{ encoding: "utf8" | "base64", content: string, truncated: bool }` — 1 MiB guard (truncated) and binary detection (→ base64). Validated relative paths: after canonicalizing they must stay within the repo (no `../`).
- `get_media_content` returns the same `FileContent` shape but always uses `"base64"` and a 12 MiB guard, so visual previews can build `data:` URLs without ambiguity. Supported extensions: `pdf`, `avif`, `bmp`, `gif`, `ico`, `jpeg`, `jpg`, `png`, `svg`, `webp`; anything else returns `unsupported-media`.
- `TreeEntry`: `{ path: string, is_dir: bool }` — flat list; the frontend builds the tree.
- `set_active_workbench` (existing, RDM-005) now additionally triggers the watcher remount and the snapshot of the new workbench (asynchronous — the deltas arrive via the stream).

## Agent Console Session Types (ACI-001/ACI-002)

The agent console backend exposes session lifecycle metadata through additive contract types. ACI-002 adds live PTY output, input, and resize without changing the lifecycle metadata shape.

```jsonc
{
  "id": "sess-1",
  "repo": "/canonical/path/to/repo",
  "agent_type": "codex",
  "status": "running",              // "starting" | "running" | "exited" | "error"
  "pid": 12345,                      // null before spawn or when unavailable
  "started_at_ms": 1760000000000,
  "ended_at_ms": null,
  "exit_code": null,                 // process exit code after completion, when available
  "error": null,                     // AgentSessionError | null
  "checkpoint": {
    "checkpoint_type": "git_ref",    // "git_ref" | "fs_snapshot"
    "git_hash": "abc...",
    "snapshot_files": []
  },
  "change_log": [
    { "path": "src/a.ts", "kind": "modified", "timestamp_ms": 1760000000000 }
  ],
  "reverted_at_ms": null
}
```

- `AgentSessionStatus`: `"starting" | "running" | "exited" | "error" | "completed" | "failed" | "reverted"`. `exited` remains in the additive contract for compatibility; new completed sessions report `completed` for exit code 0 and `failed` for non-zero exit.
- `AgentSessionError`: `{ category: string, message: string }`; messages are safe for UI display and must not include secrets.
- `agent_type`: canonical supported agent id, currently planned as `"claude"`, `"codex"`, or `"opencode"`.
- `repo`: canonical repo identity, using the same opaque path convention as `RepoDelta.repo`.
- `start_agent_session` rejects repos outside the active workbench before spawning. Errors use the same `{ category, message }` command-error shape as other Tauri commands.
- `AgentSessionOutput`: `{ session_id: string, chunk_base64: string, timestamp_ms: number }`; emitted on `tinto://agent-session-output` for live PTY output chunks.
- `AgentSessionCheckpoint`: git checkpoints are used only when the repo is clean and HEAD is readable; dirty or non-git repos use a filesystem snapshot under `~/.tinto/checkpoints/<repo-hash>/<session-id>/` with bounded size and per-repo retention.
- `AgentSessionChangeLog`: `{ session_id, changes }`; emitted on `tinto://agent-session-change-log` and mirrored in the session record.

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
