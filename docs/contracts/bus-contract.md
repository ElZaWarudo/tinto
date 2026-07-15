# Bus contract: backend↔frontend events and commands

> **Frozen 2026-06-11.** The frontend views (dashboard, diff viewer, watched files, timeline) consume this contract without renegotiating it; later changes are additive-first (new fields/events/commands, never renames or removals without migration). Source of the types: `src-tauri/src/bus/contract.rs` (Rust, `Serialize`) — the frontend TS types are derived by hand from this document in the view that first introduces them.

As of 2026-07-08, `src/bus/contract.generated.ts` is generated from the Rust bus contract and selected Git/command DTOs by `npm run contract:generate`; `npm run contract:check` fails when that generated mirror drifts. `src/bus/contract.ts` remains the curated frontend facade for compatibility notes and adjunct types that do not live in `src-tauri/src/bus/contract.rs`.

## Conventions

- Repo paths: **canonical/opaque by source**. Local repos are canonicalized by the backend. Windows WSL repos use the configured Linux absolute path as the opaque repo identity; the Windows host must not translate it through `\\wsl$` or canonicalize it as a Windows path.
- Windows WSL repo configuration is additive as of RDM-003 and live read/snapshot routing starts in RDM-004. On Windows, `list_workbenches` may include configured WSL repos with `source = "wsl"` and `distro = "Ubuntu"`; bus snapshots and `tinto://workbench-delta` may include those WSL Linux paths. Local repos keep the in-process local backend. WSL repo status, tree, diff, log, blob, working-tree file reads, media reads, file operations, repo-scoped Gitleaks checks, and Agent Console checkpoint/validation operations route through `tinto-agent`. RDM-006 makes packaged Linux agent discovery/install the preferred launch model; dev-source launch is an explicit development fallback only. On Linux/non-Windows runtime, WSL configuration UI and WSL command surfaces remain absent.
- `remove_repo_entry(workbench, path)` is the source-neutral removal command used by the UI. The backend resolves runtime paths against persisted local, extended-length Windows, or WSL identities, persists the mutation when found, reseeds the active bus, and returns `true`; it returns `false` for a bus-only orphan so the frontend can call `forget_repo` without pretending the TOML changed.
- `write_agent_session_turn(session_id, text, attachment_paths, options)` is the provider-neutral file-turn boundary. Tinto accepts up to ten regular local files per turn. Up to four PNG, JPEG, WebP, or GIF images (20 MB each) are mapped by the Codex adapter to App Server `localImage` inputs; every other file is exposed to the agent as an explicit local path because the public App Server protocol has no generic `file` input variant. Windows drive paths and matching `\\wsl.localhost\\<distro>\\...` / `\\wsl$\\<distro>\\...` paths are translated for WSL sessions. PTY fallbacks receive the same paths in provider-neutral prompt context, so future Claude or OpenCode adapters can add native file inputs without changing the composer contract.
- File paths: relative to the repo root.
- Timestamps: epoch ms (`u64`) unless otherwise noted.
- Command errors: `{ category: string, message: string }` (safe message, no secrets) — `WorkbenchError` pattern. Git categories: see `error.class` below. Read-containment categories: `repo-not-allowed` (the repo is not in the active workbench), `path-traversal` (the path escapes the repo after canonicalizing), `path-forbidden` (`.git` is not exposed), `not-a-file` (not a regular file), `not-found`, `repository-not-found`.
- **Read allowlist:** every read command (`get_worktree_diff`, `get_commit_diff`, `get_commit_log`, `get_blob`, `get_file_content`, `get_media_content`, `list_repo_tree`) requires `repo` to belong to the active workbench; if not, `repo-not-allowed`. WSL read support routes through `tinto-agent` for the same public command surface as local repos. Per-file containment: `get_file_content` and `get_media_content` confine the path within the repo and exclude `.git`; they read with bounds (≤ limit + binary/media→base64).
- Revision: `revision: u64`, monotonic **per repo** and **durable**: if a repo is unmounted and comes back, the counter continues (it does not reset to 0). Consumer rule: apply a delta/snapshot only if its `revision` is greater than the one already known.

- **File-operation allowlist:** RDM-005 routes `copy_to_repo`, `copy_within_repo`, `move_within_repo`, `export_from_repo`, `delete_from_repo`, `restore_deleted_from_repo`, and `redo_deleted_from_repo` through `tinto-agent` for WSL repos while keeping local repos on the existing local filesystem implementation. Public command names and response DTOs stay unchanged. WSL execution remains active-workbench allowlisted, contained to the Linux repo root, and blocked from `.git` mutations.

- **WSL media reads:** RDM-007 routes `get_media_content` through `tinto-agent` for WSL repos. The response shape remains `FileContent`, always base64, with the existing media extension allowlist, `.git` rejection, regular-file requirement, repo containment, and 12 MiB guard.

- **WSL Gitleaks:** RDM-008 keeps `get_gitleaks_setup_status` and `install_gitleaks` as host-scoped Addons commands, and adds repo-aware `get_repo_gitleaks_setup_status` / `install_repo_gitleaks` commands that route WSL repos through `tinto-agent`. `create_repo_gitleaks_config` is source-aware as of RDM-008: local repos write on the host, WSL repos write inside the Linux repo through the agent. WSL secret findings in `RepoDelta.secret_findings` are produced by the agent-side recomputation path.

- **WSL agent message guard:** RDM-010 raises the line-delimited `tinto-agent` message guard to 20 MiB so bounded media responses and bounded filesystem fingerprint snapshots can use the same one-shot protocol safely.
- **WSL packaged agent resource:** The Linux agent artifact is named `tinto-agent-linux-x86_64` and is bundled as a Tauri resource at the resource root. On Windows, packaged-agent discovery checks `TINTO_WSL_AGENT_LINUX_BIN` first, then app/resource-relative candidates; missing packaged artifacts fail closed unless `TINTO_WSL_AGENT_ALLOW_DEV_SOURCE=1` is explicitly enabled for development.

## Events (backend `emit` → frontend)

Frontend event listeners are active only when the Tauri event bridge is present. Browser-only Vite smoke runs do not have that bridge; the TypeScript client treats the known missing-bridge `transformCallback` listener failure as a no-op subscription with a no-op cleanup function, so UI surfaces can render for inspection. Unexpected listener setup failures must still reject instead of being hidden. If the initial snapshot cannot be loaded, the frontend marks the bus as loaded with an empty degraded watcher state rather than leaving the dashboard in an indefinite loading state; later refresh failures keep prior state. This fallback is frontend-only and does not change backend event emission or any payload shape below.

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
  "gitleaks_configured": true,
  "secret_scan_status": {
    "state": "findings",            // not_run | clean | findings | degraded
    "engine": "gitleaks",           // gitleaks | heuristic
    "version": "8.30.1",
    "checked_at_ms": 1760000000000
  },
  "secret_findings": [
    { "path": "src/config.ts", "line": 12,
      "rule_id": "generic-api-key", "description": "Possible secret" }
  ],
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
- `secret_scan_status`: observable scan health. `clean` and `findings` mean the Gitleaks engine completed; `degraded` means Tinto used its narrower heuristic fallback and includes a safe `failure_category`/`message`; `not_run` means there were no changed paths to analyze. Lightweight recalculations preserve the previous analysis instead of replacing it with an empty result.
- `secret_findings`: findings filtered to changed files and added lines. Secret values and raw matching lines are never included in the contract.

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

For Windows WSL repos, `repo` is the configured Linux repo path. Initial fingerprint scans prime state without emitting a batch; later WSL poll cycles emit created/modified/removed events when relative path, size, or modified timestamp changes. `.git` internals are excluded and the response is capped by the same repo-tree entry guard.

### `tinto://watching-state` — watching availability (workbench-level)

```jsonc
{ "available": false, "reason": "no se pudo inicializar el backend de watching: ..." }
```

Emitted at startup and on changes. `available: false` = degraded mode: data arrives only on demand (`invoke`); deltas do not flow.

`WatchingState` describes the local watcher backend. WSL agent/distro/path failures are reported per repo in `RepoDelta.error`. RDM-004 refreshes WSL repo deltas and subscribed diffs through bounded polling via `tinto-agent`. RDM-010 adds WSL `tinto://fs-events` batches by comparing agent-side file fingerprints during that WSL polling cycle; this preserves the public event shape but is not a long-lived WSL inotify stream.

RDM-005 file operations preserve the existing frontend command contract for local and WSL repos. Windows host paths used for drag/drop paste sources and export destinations are translated to `/mnt/<drive>/...` before they are sent to the WSL agent; Linux absolute paths pass through unchanged.

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

### `tinto://agent-session-timeline` - native agent timeline item

```jsonc
{
  "session_id": "sess-1",
  "id": "sess-1:1760000000000:7",
  "kind": "agent_message", // "user_message" | "agent_message" | "command_output" | "activity" | "lifecycle"
  "text": "I updated the file.",
  "timestamp_ms": 1760000000000
}
```

Timeline items are the primary stream for the IADE agent UI. `agent-session-output` remains additive compatibility for raw PTY bytes and debugging surfaces. Items are emitted live, mirrored into `AgentSession.timeline`, and appended to the local agent journal so remounted views can replay native turns instead of reconstructing chat from terminal bytes. Codex app-server sessions, both local and WSL-native, avoid terminal prompts/echo in output so timeline rendering can show user turns, agent messages, lifecycle notices, and future command output as native UI blocks.

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
| `get_gitleaks_setup_status` | none | `{ installed: bool, version: string | null, binary_path: string | null }` | Checks whether `gitleaks` is available either in the Tinto-managed addon location or on the host system and returns version/path when found. |
| `install_gitleaks` | none | `{ installed: bool, version: string | null, binary_path: string | null, method: string | null, message: string }` | Attempts a Tinto-managed install first, verifies the release archive against the publisher's SHA-256 checksum, and only then activates it in Tinto's addon directory. Falls back to host installers (winget/choco/scoop on Windows; brew/go on macOS; brew/apt/dnf/yum/pacman/zypper/apk/go on Linux). Returns updated status plus a diagnostic message. |
| `get_repo_gitleaks_setup_status` | `repo` | `{ installed: bool, version: string | null, binary_path: string | null }` | Source-aware Gitleaks status for a repo: local repos inspect the host; WSL repos inspect the selected Ubuntu agent environment. |
| `install_repo_gitleaks` | `repo` | `{ installed: bool, version: string | null, binary_path: string | null, method: string | null, message: string }` | Source-aware Gitleaks install for a repo: local repos reuse host install behavior; WSL repos install/check inside Ubuntu through `tinto-agent`. |
| `get_repo_fetch_preview` | `repo` | `{ remote: string, host: string, sanitized_url: string }` | Local-repo opt-in fetch preview. Resolves the current upstream remote, strips URL userinfo before returning it to the UI, and returns the exact host the user must confirm. WSL repos return `unsupported-repo-source`. |
| `fetch_repo` | `repo, remote, confirmed_host, user_consent` | `{ remote: string, host: string, fetched_at_ms: number }` | Performs an explicit local-repo `git fetch --prune <remote>` only when `user_consent=true` and the current remote host still matches `confirmed_host`. Runs non-interactively (`GIT_TERMINAL_PROMPT=0`), sanitizes command errors, writes only Git remote refs/objects under `.git`, and reloads state from the frontend flow after success. |
| `set_subscriptions` | `targets: Vec<{repo, path?}>` | `()` | Set of open targets (cap 8); applies from the next recomputation. |
| `retry_repo` | `repo` | `()` | Retries the remount of a repo in terminal error. |
| `start_agent_session` | `repo, agent_type` | `session_id: string` | Starts an allowlisted agent (`claude`, `codex`, `opencode`) for a repo in the active workbench. Codex sessions prefer `codex app-server --stdio` in their own execution channel: local repos launch the host binary and WSL repos launch the Linux binary inside the configured distro with the opaque Linux repo path. Both channels fall back to their native PTY path if app-server cannot start. Claude and OpenCode keep their existing PTY adapters until provider-specific structured adapters are added. WSL session start still creates its reversible checkpoint through `tinto-agent` before spawning; if that checkpoint cannot be created, the session does not start. |
| `stop_agent_session` | `session_id` | `()` | Stops the tracked PTY process/session. |
| `list_agent_sessions` | none | `Vec<AgentSession>` | Returns known sessions after refreshing completed process statuses and applying lifetime limits. |
| `get_agent_runtime_catalog` | `session_id, refresh` | `AgentRuntimeCatalog \| null` | Returns the active agent runtime's provider-neutral capability catalog. Local and WSL-native Codex app-server sessions fill it from paginated `model/list` responses; `refresh=true` requests a new catalog. PTY runtimes currently return `null`. This boundary is intentionally reusable by future Claude and OpenCode adapters. |
| `agent_binary_available` | `agent_type` | `bool` | Host-scoped compatibility check for the allowlisted agent binary through PATH lookup. Known missing binaries return `false`; unsupported agent ids return `unsupported_agent`. |
| `agent_binary_available_for_repo` | `repo, agent_type` | `bool` | Source-aware availability check. Local repos inspect the host PATH; WSL repos ask the persistent Ubuntu `tinto-agent` to resolve the allowlisted binary inside Linux, so host misses do not block Linux agents. |
| `write_agent_session_input` | `session_id, input_base64, options?` | `()` | Writes decoded bytes to a running session. If the session has host goal/personality/context-summary state, the backend prefixes a visible Tinto host-context block before the submitted turn. If plan mode is enabled, the existing visible plan instruction is included in that same turn. PTY-backed sessions receive terminal input and ignore runtime options. Codex app-server sessions echo line input locally and submit a Codex turn on Enter, applying supported per-turn runtime options to `turn/start`. Invalid base64 returns `invalid_input`; stopped/exited sessions return `session_not_running`. |
| `run_agent_host_command` | `session_id, command, argument?` | `AgentHostCommandResult` | Runs a Tinto host command for an agent session without sending it as agent prompt text. Initial supported commands are `status`, `init`, `goal`, `plan`, `personality`, `feedback`, `comments`, `compact`, `branch`, `fork`, `mcp`, `review`, `details`, and runtime-command guidance for `model`/`reasoning`/`effort`/`fast`; known but unimplemented host commands return `status="unavailable"` with a safe message. Composer aliases such as `/objective`, `/code-review`, and `/lateral` are UI conveniences that route to canonical host commands such as `goal`, `review`, and `fork`. |
| `resize_agent_session` | `session_id, cols, rows` | `()` | Resizes a running session's PTY. `cols` and `rows` must be positive; invalid dimensions return `invalid_terminal_size`. |
| `revert_session` | `session_id, user_consent` | `AgentSession` | Restores the repo to the session checkpoint. `user_consent=false` returns `consent_required`; running sessions return `session_still_running`; sessions without a checkpoint return `checkpoint_unsupported`; repeated revert is idempotent. |
| `revert_session_turn_file` | `session_id, turn_checkpoint_id, path, user_consent` | `AgentSession` | Restores one file from the selected turn checkpoint. `user_consent=false` returns `consent_required`; running sessions return `session_still_running`; unknown turn checkpoints return `turn_checkpoint_not_found`; containment rejects path traversal and `.git`. |

- `FileContent`: `{ encoding: "utf8" | "base64", content: string, truncated: bool }` — 1 MiB guard (truncated) and binary detection (→ base64). Validated relative paths: after canonicalizing they must stay within the repo (no `../`).
- `get_media_content` returns the same `FileContent` shape but always uses `"base64"` and a 12 MiB guard, so visual previews can build `data:` URLs without ambiguity. Supported extensions: `pdf`, `avif`, `bmp`, `gif`, `ico`, `jpeg`, `jpg`, `png`, `svg`, `webp`; anything else returns `unsupported-media`.
- `TreeEntry`: `{ path: string, is_dir: bool }` — flat list; the frontend builds the tree.
- `set_active_workbench` (existing, RDM-005) now additionally triggers the watcher remount and the snapshot of the new workbench (asynchronous — the deltas arrive via the stream).
- File operation commands preserve conflict reporting, backup tokens, restore, and redo DTOs across local and WSL repos.

## Agent Console Session Types (ACI-001/ACI-002)

The agent console backend exposes session lifecycle metadata through additive contract types. ACI-002 adds live PTY output, input, and resize without changing the lifecycle metadata shape.

```jsonc
{
  "id": "sess-1",
  "repo": "/canonical/path/to/repo",
  "agent_type": "codex",
  "wsl_distro": null,
  "status": "running",              // "starting" | "running" | "completed" | "failed" | ...
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
  "turn_status": "settling",        // "waiting" | "working" | "settling"
  "runtime_options": {
    "model": "gpt-5.5",
    "reasoning_effort": "high",
    "speed": "standard"
  },
  "goal": {
    "text": "Build the host command harness",
    "updated_at_ms": 1760000000200
  },
  "personality": {
    "name": "precise",
    "updated_at_ms": 1760000000250
  },
  "plan_mode": {
    "enabled": true,
    "updated_at_ms": 1760000000255
  },
  "feedback": [
    {
      "kind": "feedback",
      "text": "Keep the composer controls native.",
      "created_at_ms": 1760000000260
    }
  ],
  "context_summary": {
    "text": "Session: sess-1...\nRecent timeline:\n- AgentMessage: I updated the file.",
    "created_at_ms": 1760000000300,
    "source_events": 4,
    "source_turns": 1
  },
  "turn_checkpoints": [
    {
      "id": "sess-1:turn-1",
      "index": 1,
      "started_at_ms": 1760000000000,
      "ended_at_ms": 1760000005000,
      "checkpoint": {
        "checkpoint_type": "fs_snapshot",
        "git_hash": null,
        "snapshot_files": ["src/a.ts"]
      },
      "changes": [
        { "path": "src/a.ts", "kind": "modified", "timestamp_ms": 1760000005000 }
      ]
    }
  ],
  "timeline": [
    {
      "session_id": "sess-1",
      "id": "sess-1:1760000000000:7",
      "kind": "agent_message",
      "text": "I updated the file.",
      "timestamp_ms": 1760000000000
    }
  ],
  "reverted_at_ms": null,
  "active_sessions": 1,
  "age_ms": 1200,
  "output_bytes_per_second": null
}
```

- `AgentSessionStatus`: `"starting" | "running" | "exited" | "error" | "completed" | "failed" | "reverted"`. `exited` remains in the additive contract for compatibility; new completed sessions report `completed` for exit code 0 and `failed` for non-zero exit.
- `AgentSessionError`: `{ category: string, message: string }`; messages are safe for UI display and must not include secrets.
- `agent_type`: canonical supported agent id, currently planned as `"claude"`, `"codex"`, or `"opencode"`.
- `provider_session_id`: optional opaque conversation id supplied by the active provider adapter. Tinto persists it without interpreting its format so archived sessions can request native continuation from Codex today and from Claude/OpenCode adapters later.
- `wsl_distro`: optional distro name for WSL-backed sessions. Missing/null means a host-local session.
- `repo`: canonical repo identity, using the same opaque path convention as `RepoDelta.repo`.
- `start_agent_session` rejects repos outside the active workbench before spawning. Errors use the same `{ category, message }` command-error shape as other Tauri commands. Local Codex sessions start `codex app-server --stdio`, initialize a thread with the repo as `cwd`, subscribe to `fs/watch`, and stream app-server deltas into the same output event used by PTY sessions. If app-server spawn/initialization fails, local Codex falls back to the PTY command. WSL session start creates the reversible checkpoint and validates the allowlisted agent binary through `tinto-agent` before spawning the PTY. The interactive WSL PTY process is then launched via `wsl.exe -d <distro> --exec bash -lc ...`; repo path and agent id are passed as argv, not interpolated into the shell script.
- `resume_agent_journal_session(session_id)` returns `{ session_id, mode }`, where `mode` is `"native" | "context_bridge"`. When an archived provider id exists, the adapter resumes that provider conversation (`thread/resume` for Codex app-server). Older or fallback archives start a new active provider session, remap the saved transcript into it, and carry a bounded context summary into the next user turn. The command preserves provider-neutral runtime options and Tinto host context; UI code must not assume that every provider calls a conversation a thread.
- `AgentSessionOutput`: `{ session_id: string, chunk_base64: string, timestamp_ms: number }`; emitted on `tinto://agent-session-output` for live session output chunks. PTY sessions preserve terminal bytes and ANSI sequences; Codex app-server sessions keep this stream as compatibility output, while the product UI consumes the native timeline event below.
- `AgentSession.timeline`: bounded replay buffer of `AgentSessionTimelineItem` values for this session. `list_agent_sessions` returns it so tabs, detached windows, and late-mounted panels can hydrate the native conversation model without waiting for new live events.
- `AgentSessionTimelineItem`: `{ session_id, id, kind, text, timestamp_ms }`; emitted on `tinto://agent-session-timeline` and persisted in the local agent journal. `kind` is `"user_message" | "agent_message" | "command_output" | "activity" | "lifecycle"`. `activity` carries provider-neutral progress such as command starts, tool use, file changes, and public reasoning summaries; adapters for Codex, Claude, and OpenCode can map their native events to it. The Agents panel uses this as the primary conversation model and falls back to raw output only when timeline items are unavailable.
- `AgentSession.runtime_options`: optional per-session echo of the latest runtime selection. The Tauri input command accepts `options?: { model?: string, reasoning_effort?: string, speed?: string }`; the Codex app-server adapter forwards these as `model`, `effort`, and, when non-standard, `serviceTier` in `turn/start`. PTY-backed sessions ignore runtime options. The session/process contract stays provider-neutral so Claude and OpenCode can translate the same intent into their own capabilities later.
- `AgentRuntimeCatalog`: `{ status, source, models?, default_model?, error?, updated_at_ms }`. Models expose stable runtime ids, display metadata, ordered reasoning efforts, service tiers, and the provider's default selections. The frontend renders this catalog instead of maintaining a hard-coded Codex model list, preserves explicit values that have temporarily fallen outside the catalog, and exposes catalog failures as a retryable control state rather than writing them into the agent transcript.
- `AgentSession.goal`: optional persistent session objective set through `/goal <text>`, inspected with `/goal`, or cleared with `/goal clear` (`borrar` and `limpiar` are equivalent localized arguments). Tinto normalizes whitespace and rejects objectives longer than 1,200 characters so the stored value always matches the complete value injected into later turns. A successful command confirms the journal write before announcing the new state; archived or natively resumed sessions reconstruct the objective from `goal.text` and `goal.updated_at_ms`. When present, outgoing turn input is prefixed with a visible provider-neutral Tinto host-context block that includes this goal.
- `AgentSession.personality`: optional persistent response-style preference set through `/personality <name>` or cleared with `/personality clear`. `/personality` without an argument reports the current value. The journal persists the name and update time so archived sessions can reconstruct the preference. When present, outgoing turn input is prefixed with a visible Tinto host-context block that includes this preference. This is Tinto host state; it does not implement memory.
- `AgentSession.plan_mode`: optional persistent session mode toggled through `/plan on`, `/plan off`, `/plan toggle`, or the composer palette's `Modo plan` entry. `/plan` without an argument reports the current value. When enabled, the session backend prefixes outgoing turn input with a visible one-line instruction asking Codex to provide a concise implementation plan before editing files. The journal persists `enabled` and `updated_at_ms` so archived sessions can reconstruct the mode. This is Tinto host state and does not implement memory.
- `AgentSession.feedback`: persistent local session notes created through `/feedback <text>` or `/comments <text>` and cleared with `/feedback clear` or `/comments clear`. This stores user feedback/commentary in Tinto's local journal only; it does not send feedback to an external service and does not implement memory.
- `AgentSession.context_summary`: optional persistent session summary created through `/compact` and cleared with `/compact clear`. This is Tinto host context, not a guarantee that Codex app-server has internally discarded prior context. The journal persists summary text, creation time, and source event/turn counts for archived session reconstruction. When present, outgoing turn input is prefixed with a visible Tinto host-context block that includes a bounded single-line compact context value.
- `AgentHostCommandResult`: `{ command, status, message, session_id?, repo?, agent_type?, review_summary?, review_findings? }`, where `status` is `"completed" | "unavailable"`. Host commands are Tinto app actions, not agent prompts; unsupported or pending host commands must return `unavailable` instead of being sent to the agent as raw slash text. `/branch`, `/fork`, and `/lateral` return the optional session fields when they start a child session. `/review` and `/code-review` return optional `review_summary` data when Git summary collection succeeds, plus optional `review_findings` for deterministic host-side findings. Active local and WSL same-repo forks create isolated Git worktrees under `~/.tinto/worktrees`, add them to the active workbench, reseed the bus, and start the child session there. Local or WSL repos without a `HEAD` commit return `unavailable` with a specific message.
- `AgentReviewSummary`: `{ branch, changed_files, working_shortstat?, staged_shortstat?, files, truncated_count }`. The file list is bounded for UI display; `message` remains the human-readable compatibility summary.
- `AgentReviewFinding`: `{ severity, title, detail, path?, line? }`. Findings are bounded, deterministic review hints produced from local or WSL Git state. Current rules flag sensitive path changes, conflict markers in changed text files, and `package-lock.json` changes without a matching `package.json` change. They are review prompts, not a full semantic code-review verdict.
- `/mcp` inspects the local Codex config (`$CODEX_HOME/config.toml` or `~/.codex/config.toml`) for `mcp_servers` / `mcpServers` entries and reports server names plus safe command availability counts. It does not print args/env values and does not start MCP servers as a health check.
- `/review` and `/code-review` produce a Git review summary for local and WSL repos: current branch, changed-file count, unstaged/staged shortstat, a capped changed-file list, and deterministic findings for high-signal local hazards. This is a host-side orientation surface for code review, not a claim that semantic bug review has completed. WSL summaries/findings are resolved through the allowlisted Linux-side `tinto-agent` protocol and remain read-only.
- `AgentSessionCheckpoint`: local git checkpoints are used only when the repo is clean and HEAD is readable; dirty or non-git local repos use a filesystem snapshot under `~/.tinto/checkpoints/<repo-hash>/<session-id>/` with bounded size and per-repo retention. WSL Agent Console sessions create equivalent checkpoint records inside Ubuntu through `tinto-agent`; change-log scan and explicit-consent revert also run inside Ubuntu and are allowlisted to the active WSL repo. Sessions with `checkpoint: null` can still exist for legacy/fallback states and return `checkpoint_unsupported`; the UI disables Revert for those sessions.
- `AgentSessionChangeLog`: `{ session_id, changes }`; emitted on `tinto://agent-session-change-log` and mirrored in the session record.
- `AgentSessionTurnStatus`: `"waiting" | "working" | "settling"`. PTY sessions derive this from fallback signals: PTY output activity, optional Tinto turn-done marker, and stable filesystem/checkpoint scans. Local Codex app-server sessions additionally consume structured app-server events: file/diff/watch notifications mark activity and `turn/completed` force-closes the turn through the existing checkpoint scanner. Tinto remains the authority for final changed files.
- `AgentSessionTurnCheckpoint`: `{ id, index, started_at_ms, ended_at_ms, checkpoint, changes }`. The checkpoint is the pre-turn boundary for the listed "changes during turn"; empty turns are not recorded. Per-file revert uses this checkpoint and never reverts the whole turn as one operation.
- `AgentSessionLimits`: `{ max_sessions, max_sessions_per_repo, max_lifetime_ms }`; default runtime limits are 5 active sessions per workbench, 1 active session per repo, and 4 hours max lifetime. Capacity errors are `max_sessions_reached`, `max_sessions_per_repo_reached`, and `session_lifetime_exceeded`.
- Telemetry fields are best-effort and local-only. `active_sessions` is the current active count at listing time, `age_ms` is computed from `started_at_ms`, and `output_bytes_per_second` is reserved for sampled PTY throughput.

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
