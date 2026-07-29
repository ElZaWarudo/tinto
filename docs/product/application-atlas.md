---
atlas_schema_version: 1
status: "stale"
verified_source_commit: "8b7c4c85a3e561eb60f68b10678b59bb45919ccd"
application_fingerprint: "sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0"
tracked_paths: ["README.md", "tinto-design.md", "package.json", "package-lock.json", "index.html", "demo.html", "dashboard-review.html", "agent-runtime.html", "agent-lens-restorable.html", "vite.config.ts", "wdio.conf.ts", "tsconfig.json", "tsconfig.node.json", "eslint.config.js", "src", "src-tauri", "e2e", "scripts", "docs/brainstorms", "docs/roadmaps", "docs/contracts", "docs/build-guide.md", ".github/workflows"]
excluded_paths: ["docs/product/application-atlas.md", "docs/audits/"]
last_verified_at: "2026-07-17"
---

# Application Atlas

This atlas is a stale working-tree update over the reduced-scope baseline validated at source commit `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`. Current uncommitted behavior is mapped from code, focused tests and a visible Windows-native session driven through Pumarejo on 2026-07-28; the `verified_source_commit` and `application_fingerprint` above remain baseline provenance until the changes are committed, the tree is clean and a substantive verification can compute a new fingerprint. Linux, WSL, external, destructive and real-agent paths retain explicit `Unverified` dispositions where they have not been exercised. The Windows observations are a partial working-tree smoke, not installer, release-bundle or cross-platform certification.

Remediation update, 2026-07-29: Code/Tests now cover accessible and keyboard-operable Dockview tabs, day-grouped Timeline entries with full machine-readable timestamps, and repository/provider-specific accessible names on repeated Dashboard actions. The complete frontend suite passes (55 files/718 tests), as do TypeScript, focused ESLint and the production build. A native re-observation with the updated Pumarejo MCP was attempted, but its owned launch returned `APP_START_FAILED` before a Tinto process existed; these remediations therefore remain Code/Tests rather than new Observed evidence. See `docs/audits/2026-07-29-product-polish-regression-evidence.md`.

Evidence labels are `Declared`, `Observed`, `Code`, `Inferred` and `Unverified`.

## 1. Intent

- Product promise [Declared]: keep a local developer aware of repository changes and coding-agent sessions in real time, so they can decide where to look without opening a heavy editor or losing the thread.
- Primary user [Declared]: a local developer acting as supervisor and operator of repositories and agent sessions.
- Primary job [Declared]: understand what changed, where, when and during which agent turn, then guide the work when needed.
- Primary actions [Declared by product owner, 2026-07-17]: two co-primary, complementary actions:
  1. Observe changes through Dashboard, Live Diff and Timeline.
  2. Direct sessions through Agents and inspect them through Agent Lens.
- Success signal [Declared by product owner, 2026-07-17]: repository activity/diff/timeline update visibly and the agent turn completes with a verifiable checkpoint.
- Supported platforms [Declared]: Windows and Linux. WSL is a Windows execution/repository boundary. macOS is not officially supported and is only a technically compilable target.
- Deliberate constraints [Declared]: the monitoring loop is passive, lightweight, local, factual and read-only; mutable workspace tools run only after explicit user action; Plane 2 monitoring is opt-in.
- Unacceptable outcomes [Declared]: silent monitoring-layer mutation; mutation outside a registered repository; exposing secret values; unconfirmed fetch/delete/overwrite/revert/restore; partial restore after failure; orphaned agent processes.
- Sources: product owner response dated 2026-07-17; `README.md:12-65,71-143,190-201`; `docs/roadmaps/2026-07-13-008-post-ux-agent-platform-roadmap.md:33-40`; `docs/contracts/bus-contract.md:15-30,175-188`.

## 2. Platforms and environments

| ID | Platform/environment | Supported | Inputs | Constraints | Evidence |
|---|---|---|---|---|---|
| `PLAT-001` | Windows desktop/Tauri | Yes | Pointer, keyboard, OS dialogs | Visible dirty-working-tree session observed at 800×600 and 1920×1032 through Pumarejo; installer/release bundle not exercised | `README.md:140-143` [Declared]; `src-tauri/tauri.conf.json:14-22` [Code]; `OBS-2026-07-28-001..003` [Observed] |
| `PLAT-002` | Linux desktop/Tauri | Yes | Pointer, keyboard, native shell | Not available in this run | `README.md:140-143` [Declared] |
| `PLAT-003` | WSL2 on Windows | Conditional | Windows UI; Linux paths/processes | No distro installed in audit host | `src/workbench/AddRepoDialog.tsx:209-326` [Code]; [Unverified] |
| `PLAT-004` | Detached Tauri WebViews | Yes | Pointer, keyboard, window controls | `terminal-*` and `consoles` only | `src/main.tsx:12-24`; `src/panels/terminal/detachTerminalWindow.ts:100-241` [Code] |
| `PLAT-005` | Vite browser-only development | Development only | Pointer and keyboard | `/` is a review launchpad; no Rust IPC or Agent processes; links synthetic responsive fixtures | `src/App.tsx:79-91`; `src/workbench/BrowserReviewHome.tsx:1-48`; focused test [Code/Tests]. Fixture observations remain baseline evidence |
| `PLAT-006` | macOS bundle target | No official support | Not examined | Technically compilable only | Product owner, 2026-07-17 [Declared]; `src-tauri/tauri.conf.json:26-38` [Code] |
| `PLAT-007` | Pumarejo-instrumented Tauri development runtime | Development/audit only | MCP snapshot, screenshot, pointer, text and bounded key actions | WebDriver plugin is registered only with `debug_assertions` plus the `pumarejo` feature; it is absent from normal builds | `.pumarejo.json`; `src-tauri/Cargo.toml:26-31`; `src-tauri/src/lib.rs:1-15,104` [Code]; native session [Observed] |

## 3. Actors and permissions

| ID | Actor/role | Goal | Can | Cannot | Evidence |
|---|---|---|---|---|---|
| `ROLE-001` | Local developer/operator | Observe changes and guide agents | Manage workbenches/repos, inspect files/diffs/timeline, run explicit tools and agents | No account/tenant/remote-user capabilities are present | `src/App.tsx:103-173`; `src/workbench/MenuBar.tsx:295-595` [Code] |
| `ROLE-002` | Local coding agent | Execute a guided coding session | Codex, Claude Code or OpenCode can emit sessions, turns and output | Cannot start outside an active registered repo | `src/bus/contract.ts:21-28,162-190`; `src-tauri/src/agent_console/commands.rs:73-106` [Code] |
| `ROLE-003` | Tinto monitor runtime | Surface repository and agent facts | Load snapshots and apply Git/filesystem/agent deltas | Monitoring loop must not mutate repos or remotes | `README.md:25-40` [Declared]; `src/bus/connection.ts:36-118` [Code] |
| `ROLE-004` | WSL runtime bridge | Reach Linux repos and agents from Windows | List distros, browse paths, run bounded commands | Not exposed on native Linux; not verified here | `src-tauri/src/lib.rs:109-118` [Code] |
| `ROLE-005` | Pumarejo MCP audit driver | Observe and exercise a driver-owned development WebView | Launch/close an owned session and act through current semantic references | References become stale after mutation; it is not a production user role and must not broaden the audit's action authorization | Pumarejo MCP contract [Declared]; `OBS-2026-07-28-001` [Observed] |

No authentication, plan, tenant, collaboration or differentiated human-role model was found. Tauri capabilities are window-scoped, not identity-scoped (`src-tauri/capabilities/default.json:3-18`) [Code].

## 4. Surface and navigation map

| ID | Surface | Entry | Exits/return | Roles | Route/window | Evidence |
|---|---|---|---|---|---|---|
| `SURF-001` | Bootstrap/loading | App launch | Workspace, first run or failure | ROLE-001/003 | main | `src/App.tsx:217-249` [Code] |
| `SURF-002` | Native startup failure | Tauri bootstrap error | Retry bootstrap | ROLE-001 | main | `src/workbench/firstRun.tsx:109-152` [Code]; browser invoke failure belongs to the baseline commit only |
| `SURF-003` | First workbench | No active workbench | Created workspace | ROLE-001 | main | `src/workbench/firstRun.tsx:49-107` [Code] |
| `SURF-004` | Shell/menu/window chrome | Successful native bootstrap | All principal panels/dialogs | ROLE-001 | main | Native Ver/Ayuda menu roles, initial menu focus and post-dialog focus return [Observed]; top-level Dockview tabs were exposed as unnamed `div` controls [Observed]; F10/bare Alt entry and modal guard [Code/Tests] |
| `SURF-005` | Dashboard | Default/menu | Repo, Agents, add repo | ROLE-001 | main panel | Two real repository rows, live status and responsive reflow at 800×600 and wide table at 1920×1032 [Observed] |
| `SURF-006` | Dashboard filters | Dashboard | Filtered/reset dashboard | ROLE-001 | inline | Native search `tinto`, unmatched-result state and reset [Observed] |
| `SURF-007` | Workbench manager | Workbench menu | Activate/create/rename/delete/close | ROLE-001 | dialog | `ManageWorkbenchesDialog.tsx:188-443` [Code] |
| `SURF-008` | Add repository | Repos/menu/dashboard | Registered repo or cancel | ROLE-001 | dialog/OS picker | `AddRepoDialog.tsx:14-350` [Code] |
| `SURF-009` | Add-ons manager | Complementos | Install/recheck/close | ROLE-001 | dialog | `AddonsManager.tsx:49-200` [Code] |
| `SURF-010` | Keyboard shortcuts | Ayuda | Close | ROLE-001 | dialog | Native dialog role/name, close autofocus, heading hierarchy, Escape close and focus return [Observed]; `KeyboardShortcuts.tsx:16-87` [Code] |
| `SURF-011` | Repository/project | Dashboard/timeline/glance | File, config, watch, dashboard | ROLE-001 | repo panel | Native repository overview exposed config, signals, changed/untracked files, watched patterns and commit history [Observed]; `RepoPanel.tsx:139-460` [Code] |
| `SURF-012` | Repository configuration | Repo overview | Create config/return | ROLE-001 | inline | `RepoConfigSection.tsx:18-119` [Code] |
| `SURF-013` | Watched files | Repo overview | Save patterns/open event | ROLE-001 | inline | `WatchedFilesSection.tsx:50-220` [Code] |
| `SURF-014` | Project explorer | Repo panel | File/menu/drag-drop | ROLE-001 | nested panel | Current repository tree and changed-state markers exposed with tree/treeitem semantics [Observed]; pending mutation status, repeated-action guard and Ctrl-copy/plain-drag-move [Code/Tests] |
| `SURF-015` | File context menu | Explorer/context key | File action/close | ROLE-001 | menu | `ProjectExplorer.tsx:1108-1152` [Code] |
| `SURF-016` | File dock | Open file | Pin/close/switch | ROLE-001 | nested dock | `fileDock.ts:92-259` [Code] |
| `SURF-017` | File viewer | Tree/status/timeline/Lens | Diff/full/Markdown/media | ROLE-001 | file tab | Small real `.gitignore` diff rendered in a named content region with view controls [Observed]; `FileView.tsx:121-374` [Code] |
| `SURF-018` | Text diff/overview ruler | File viewer | Full/close/exact marker jump | ROLE-001 | file view | Baseline fixture [Observed]; same-line clustering and non-overlapping exact line targets [Code/Tests] |
| `SURF-019` | Full file | File viewer | Diff/close | ROLE-001 | file view | `FullFileView.tsx:14-135` [Code] |
| `SURF-020` | Markdown | File viewer | Other view/close | ROLE-001 | file view | `MarkdownView.tsx:18-73` [Code] |
| `SURF-021` | Media/PDF preview | File viewer | Close | ROLE-001 | file view | `MediaView.tsx:11-80` [Code] |
| `SURF-022` | Overwrite confirmation | Copy/move conflict | Cancel/overwrite | ROLE-001 | dialog | `OverwriteConfirmModal.tsx:15-83` [Code] |
| `SURF-023` | Timeline | Menu/repo activity | Commit/repo/file | ROLE-001 | main panel | Native cross-repo list and commit detail observed; rows from multiple dates displayed time-of-day without date/day grouping [Observed/Code]; prior history remains visible while refreshing and after refresh failure (`TimelinePanel.tsx:115-168`) [Code/Tests] |
| `SURF-024` | Glance mode | Menu/shortcut | Repo/main workspace | ROLE-001 | compact surface | `GlanceMode.tsx:30-130` [Code] |
| `SURF-025` | Agent consoles/navigator | Menu/dashboard | Session, journal, detach | ROLE-001/002 | main panel | Native no-active-Agent state, contextual quick-launch controls and recent-session list observed without starting a process; compact group maximizes/reflows the active terminal [Code/Tests] |
| `SURF-026` | Agent conversation | Session/journal | Details, file, detach | ROLE-001/002 | console tab | Completed/working/journal baseline [Observed]; bounded responsive transcript and localized action copy [Code/Tests] |
| `SURF-027` | Runtime/preset controls | Composer | Select/save/close | ROLE-001 | dialog/inline | Preset baseline [Observed]; production composer order and localized `Rápido` control [Code/Tests] |
| `SURF-028` | Agent Lens/details | Conversation Details | Files/commands/timeline/close | ROLE-001 | complementary panel | Search, details and tabs exercised [Observed] |
| `SURF-029` | Conversation history | Agent navigator | Journal/open/delete | ROLE-001 | navigator/menu | List rendered; real reopen/delete unverified [Observed/Unverified] |
| `SURF-030` | Detached agent terminal | Detach session | Reattach/close | ROLE-001/002 | `terminal-*` | `DetachedTerminalApp.tsx:14-29` [Code] |
| `SURF-031` | Detached consoles | Detach consoles | Reattach/close | ROLE-001/002 | `consoles` | Drag reattach plus explicit `Reanexar Agents` button usable by click/keyboard (`DetachedConsolesApp.tsx:48-75`) [Code/Tests] |
| `SURF-032` | Native notification | Relevant event | Open/dismiss | ROLE-001 | OS | `notifications.ts:23-168` [Code/Unverified] |
| `SURF-033` | Generic integrated shell (retired declaration) | None | None | ROLE-001 | Retired; ID not reusable | README now describes Agent session consoles rather than a generic terminal; no current product surface [Declared/Code] |
| `SURF-900` | Browser review launchpad and fixtures | Browser-only `/` or direct fixture URL | Dashboard, Agents/Lens, Agent runtime or ruler fixture | ROLE-001 | Vite pages | `BrowserReviewHome.tsx` links `dashboard-review.html`, `agent-lens-restorable.html`, `agent-runtime.html` and `demo.html`, and states the no-IPC/no-real-Agent boundary [Code/Tests] |

Navigation is panel/dock based, not URL-router based. The shell opens Dashboard, Timeline, workbenches, repos, add-ons, shortcuts, Glance and Agents. Dashboard/Timeline/Glance open a repo; a repo opens files; Agents opens conversations and optional detached windows [Code].

## 5. Flow registry

| ID | Flow | Actor | Frequency | Consequence | Entry | Completion | Surfaces | Evidence |
|---|---|---|---|---|---|---|---|---|
| `FLOW-001` | Bootstrap/load or browser review entry | ROLE-001/003 | frequent | high | app launch | native shell/actionable failure, or browser review launchpad | 001-004/900 | Native Windows launch [Observed]; browser launchpad Code/Tests; baseline failure Observed |
| `FLOW-002` | Create first workbench | ROLE-001 | occasional | high | no workbench | active workspace | 003/005 | Code |
| `FLOW-003` | Switch workbench | ROLE-001 | frequent | medium | menu/manager | new context loaded | 004/007 | Code |
| `FLOW-004` | Manage workbenches | ROLE-001 | occasional | high | manager | create/rename/delete result | 007 | Code |
| `FLOW-005` | Add local repo | ROLE-001 | occasional | high | add repo | registered/reloaded | 008/005 | Code |
| `FLOW-006` | Add WSL repo | ROLE-001/004 | occasional | high | Windows add repo | registered/reloaded | 008 | Code/Unverified |
| `FLOW-007` | Autodetect repos | ROLE-001 | occasional | medium | menu | results registered | 008/005 | Code |
| `FLOW-008` | Remove repo | ROLE-001 | rare | high | repo action | config/state removed | 005 | Code |
| `FLOW-009` | Passive monitoring | ROLE-003 | frequent | high | loaded workbench | deltas visible | 005/011/023 | Native live dashboard/status [Observed]; Declared/Code |
| `FLOW-010` | Filter/open project | ROLE-001 | frequent | medium | dashboard | project open | 005/006/011 | Native filter, empty/reset and project open [Observed] |
| `FLOW-011` | Inspect repo status | ROLE-001 | frequent | high | repo panel | relevant signal understood | 011-013 | Native status overview [Observed]; Code |
| `FLOW-012` | Open/pin/close file | ROLE-001 | frequent | medium | tree/signal | file context visible | 014/016/017 | Native file open [Observed]; pin/close Code |
| `FLOW-013` | Inspect Live Diff/content | ROLE-001 | frequent | high | changed file | diff/content inspected | 017-021 | Small native working-tree diff [Observed]; ruler Observed |
| `FLOW-014` | Update watched patterns | ROLE-001 | occasional | high | watched-files section | validation/save/reload | 013 | Code |
| `FLOW-015` | Inspect Timeline | ROLE-001 | frequent | high | Timeline | event/commit/file inspected | 023/017 | Native list and commit detail [Observed]; multi-day date ambiguity [Observed/Code] |
| `FLOW-016` | Fetch remote | ROLE-001 | rare | high | repo action | confirmed fetch/reload | 005 | Code/Unverified |
| `FLOW-017` | Install/recheck Gitleaks | ROLE-001 | rare | high | add-ons | verified install/status | 009 | Code/Unverified |
| `FLOW-018` | Create repo config | ROLE-001 | occasional | high | repo config | file created/updated | 012 | Code/Unverified |
| `FLOW-019` | Copy/move/paste file | ROLE-001 | occasional | high | explorer | pending batch committed atomically, or recoverable error/warning | 014/015/022 | Ctrl strategy, batch rollback and cleanup warnings [Code/Tests]; real mutation Unverified |
| `FLOW-020` | Delete/undo file | ROLE-001 | rare | high | explorer | consented batch, repo-bound recovery token or surfaced retry | 014/015 | Backend consent, partial recovery manifest and retry [Code/Tests]; real mutation Unverified |
| `FLOW-021` | Launch Agent | ROLE-001/002 | frequent | high | dashboard/Agents | session visible | 005/025/026 | Native launch surface [Observed]; launch not executed |
| `FLOW-022` | Send Agent turn | ROLE-001/002 | frequent | high | composer | turn completed/checkpoint | 026-028 | Fixture surface Observed; send Unverified |
| `FLOW-023` | Queue/steer active turn | ROLE-001/002 | occasional | high | working composer | queued/steered item visible | 026 | Working state Observed; submission Unverified |
| `FLOW-024` | Edit prior message/branch | ROLE-001/002 | occasional | high | transcript | branched session visible | 026 | Code/Unverified |
| `FLOW-025` | Stop/revert session | ROLE-001/002 | rare | critical | session header | confirmed terminal state or exact safety rollback | 026 | Safety checkpoint for worktree/index [Code/Tests]; effect Unverified |
| `FLOW-026` | Restore turn/revert file | ROLE-001/002 | rare | critical | Details/Lens | restored state or exact safety rollback | 026/028 | Worktree/index transaction [Code/Tests]; control Observed |
| `FLOW-027` | Reopen saved conversation | ROLE-001 | occasional | high | journal | session opens after send; list refresh continues independently | 025/026/029 | Journal surface Observed; idempotent resume [Code/Tests] |
| `FLOW-028` | Delete saved conversation | ROLE-001 | rare | high | journal menu | explicitly consented removal | 029 | Confirmation, active guard and backend consent [Code/Tests]; mutation Unverified |
| `FLOW-029` | Detach/reattach | ROLE-001/002 | occasional | high | console/session | acknowledged transfer, then close or close-only retry | 030/031 | Explicit click/keyboard reattach, request/ack and timeout [Code/Tests]; native window effect Unverified |
| `FLOW-030` | Glance/zoom/shortcuts | ROLE-001 | frequent | medium | menu/keys | view/state updated | 004/024 | F10/isolated Alt entry and modal shortcut suspension [Code/Tests]; baseline partial keyboard Observed |
| `FLOW-031` | Enable/use notification | ROLE-001 | occasional | medium | menu/event | permission/status/notification | 032 | Code/Unverified |
| `FLOW-032` | Launch, observe and close Pumarejo audit session | ROLE-005 | development only | high | explicit debug-feature launch | sampled evidence captured and driver resources released | 004-006/010-011/014/017/023/025 | Read-only/navigation subset [Observed]; integration [Code] |

### FLOW-001 — Native bootstrap, browser review entry and failure recovery

- Preconditions: frontend served inside Tauri for production, or browser-only dev surface.
- Before: no configuration or snapshot is loaded.
- During: native runtime initializes configuration, snapshot and event listeners; browser-only runtime bypasses IPC and renders `SURF-900`.
- After: native workspace, first-run or startup-failure surface; browser review launchpad with links to four synthetic surfaces.
- Failure and recovery: native `SURF-002` provides Retry. The baseline browser invoke failure is superseded in the working tree by a no-IPC launchpad [Code/Tests].
- Data written or exposed: production may read config/state; browser review mode explicitly runs no Rust commands or Agent processes.
- Context that must survive: existing workspace remains mounted during background reloads [Code].

### FLOW-009 / FLOW-013 — Observe repository activity and inspect change

- Preconditions: active workbench with registered repositories.
- Before: snapshot/dashboard state.
- During: Git/filesystem deltas update cards, repo state, diff and Timeline.
- After: user sees activity and inspects the relevant file/diff.
- Failure and recovery: listener failures are attributed per channel, preserve the last successful repo snapshot/Agent session list and retry after 1 s; dashboard degradation, repo retry, paused/reload diff and error/binary/size guards remain [Code/Tests].
- Real conditions: dashboard ready/loading/filter/responsive states were observed; real filesystem causality and Live Diff timing were not.
- Data exposed: local paths, status, diffs and signals; secret values must not be exposed [Declared].
- Context that must survive: workbench, last successful snapshot/session state, dock/file selection and layout.

### FLOW-021 / FLOW-022 — Launch and direct Agent

- Preconditions: active registered repo and available runtime.
- Before: dashboard/Agent navigator and empty composer.
- During: session starting/running; turn working/settling; queue/steer controls can appear.
- After: completed turn plus checkpoint and affected files, the declared success signal.
- Failure and recovery: unavailable runtime, launch/send errors, stop, journal/resume and restore controls [Code].
- Real conditions: completed, working and journal fixtures were observed; no real process or turn was executed.
- Data written or exposed: transcript, attachments, output, checkpoints and journal; limits are 10 files and 4 images per turn [Code].
- Context that must survive: transcript, turn map, selected turn, queue and journal.

### High-consequence mutation flows

- `FLOW-016`: preview remote, explicit confirmation and backend host/user-confirmed revalidation before fetch [Code].
- `FLOW-019`: root-bounded batch with pending status; Ctrl+drag copies and plain drag moves; every object is staged before commit, the whole batch rolls back on failure, and post-commit cleanup failures return explicit non-fatal warnings on local, WSL and export paths [Code/Tests].
- `FLOW-020`: frontend confirmation and explicit backend consent precede filesystem/WSL access; delete records a canonical-repo-bound manifest, distinguishes `completed` from `recovery_required`, and keeps partially applied undo/redo tokens retriable [Code/Tests].
- `FLOW-025`/`FLOW-026`: frontend confirmation plus explicit backend consent; restore creates an ephemeral safety checkpoint, restores the working tree and opaque Git index transactionally on failure, and refuses unsupported dirty symlinks before mutation [Code/Tests].
- `FLOW-028`: irreversible journal deletion confirmation, active-session guard and explicit backend consent before registry/SQLite access [Code/Tests].
- None were executed in this audit [Unverified].

## 6. State catalog

| ID | Surface/flow | State | Expected behavior | Observed/code/unverified | Evidence |
|---|---|---|---|---|---|
| `STATE-001` | Bootstrap | loading | brand and progress status | Code | `firstRun.tsx:22-47` |
| `STATE-002` | Native bootstrap | failure/retry | actionable error and Retry | Code; baseline Observed | `firstRun.tsx`; `OBS-001` applies to the baseline browser root, now replaced by `SURF-900` |
| `STATE-003` | First run | idle/creating/error | named workbench creation | Code | `firstRun.tsx:49-107` |
| `STATE-004` | Shell | background/channel/action error and retry | retain mounted context and expose retry where the operation remains recoverable | Code/Tests | `App.tsx:283-312`; `bus/connection.ts`; `qol/shortcuts.ts` |
| `STATE-005` | Dashboard | loading/ready | stable frame then data | Observed | `OBS-002/005` |
| `STATE-006` | Dashboard | zero repos/no matches | next useful action | Code; no-match partially Observed | `DashboardPanel.tsx:206-310`; `OBS-003` |
| `STATE-007` | Dashboard | 1280/768/390 | responsive without document overflow | Observed | `OBS-006` |
| `STATE-008` | Monitor | watching/channel degraded/reconnecting | identify the failed channel, retain last successful state and clear error after reconnect | Code/Tests | `bus/store.ts:19-78,243-263`; `bus/connection.ts:91-181` |
| `STATE-009` | Repo | pending/missing/error/retry | scoped recovery | Code | `RepoCard.tsx:168-393`; `RepoPanel.tsx:139-210` |
| `STATE-010` | Explorer | loading/error/empty/filter/truncated | recover or explain bounds | Code | `ProjectExplorer.tsx:740-980` |
| `STATE-011` | Diff/file | loading/active/paused/error | inspect or retry | Code | `FileView.tsx:121-374` |
| `STATE-012` | Diff/file | binary/large/long/capped/no hunks | explicit guard/fallback | Code | `DiffView.tsx:80-165` |
| `STATE-013` | Full/Markdown/media | loading/error/binary/truncated/unavailable | explicit limitation | Code | SURF-019/020/021 sources |
| `STATE-014` | File mutation | pending/conflict/error/rollback/recovery-required/warning/undo/redo | serialize the batch, confirm overwrite/delete, restore every committed object on failure, preserve recovery tokens and distinguish completed cleanup warnings | Code/Tests; real mutation Unverified | FLOW-019/020 sources |
| `STATE-015` | Timeline | loading/refreshing/incremental/empty/filtered/error | retain prior history, publish each repo as it resolves and keep aggregate pending/error status | Code/Tests | `TimelinePanel.tsx`; focused delayed/failure tests |
| `STATE-016` | Agent console | empty/loading/error/active/journal/channel degraded | retain prior session and journal lists during refresh failure, retry locally, and open a resumed session without waiting for list refresh | Baseline Observed; Code/Tests | `OBS-007/011/013`; connection/console tests |
| `STATE-017` | Agent session | starting/running/completed/failed/reverted/restoring | truthful header and controls; failed restore returns to the exact working-tree and Git-index state | Working/completed Observed; transaction Code/Tests | `OBS-007/010`; checkpoint/session tests |
| `STATE-018` | Agent turn | waiting/working/settling | pending signal and next action | Waiting/working Observed; settling Code | `OBS-007/010` |
| `STATE-019` | Composer | idle/sending/queued/steering/read-only/error | proportional controls, DOM focus order matching compact visual order, and per-session draft continuity until confirmed send | Empty/read-only Observed; mutation Code/Tests | `OBS-007/010/011`; terminal tests |
| `STATE-020` | Agent Lens | files/commands/timeline/details | selected-turn evidence | Observed | `OBS-008/009` |
| `STATE-021` | Compact Agent | compact container/group | bounded transcript controls; maximize/reflow active terminal when required | Baseline Observed; Code/Tests | `OBS-012`; Agent CSS and `ConsoleDockPanel.test.tsx` |
| `STATE-022` | Agent navigator | sidebar plus transcript | bounded responsive conversation continuity | Baseline Observed; Code/Tests | `OBS-013`; `App.css:11314-11473` |
| `STATE-023` | Runtime fixture | empty composer/preset dialog | production attach/input/send order and localized runtime controls | Baseline Observed; Code/Tests | `OBS-014`; `agentRuntime.test.tsx`; `AgentRuntimeControls.tsx` |
| `STATE-024` | Overview ruler | pointer/keyboard/same-line cluster/dense adjacent lines | one exact target per line, no target overlap and identical pointer/keyboard line activation | Baseline Observed; Code/Tests | `OBS-015/016`; `FileOverviewRuler.test.tsx:156-230` |
| `STATE-025` | Notifications/fetch/WSL/add-on | idle/loading/success/error/denied | boundary-specific recovery | Code/Unverified | source registry |
| `STATE-026` | Pumarejo instrumentation | disabled/enabled/session owned/closed | remain absent unless debug plus feature is explicit; every action invalidates prior refs; close releases WebDriver and child resources | Code/Declared; owned session and clean close Observed | `.pumarejo.json`; `src-tauri/src/lib.rs:1-15`; `OBS-2026-07-28-001,017` |

## 7. Data and context lifecycle

| Data/context | Created | Persisted | Restored | Cleared | Risk | Evidence |
|---|---|---|---|---|---|---|
| Workbenches/repos | user configuration | OS config TOML, atomic write | startup/switch | explicit remove/delete | losing monitoring context | `src-tauri/src/workbench/mod.rs:174-284` [Code] |
| Main layout | dock changes | `ui-state.json` | startup | reset/overwrite | disorientation | `src-tauri/src/ui_state.rs:3-41` [Code] |
| File tabs/tree/zoom/presets | UI actions | `localStorage` | next session | per-feature clear | stale context | `fileDock.ts`; `zoom.ts`; `runtimePresets.ts` [Code] |
| Dashboard filters/glance | UI actions | memory | during process | reset/restart | minor lost context | `qol/state.ts:31-85` [Code] |
| Agent queue | queue action | `localStorage` | session view | processed/cleared | wrong-turn context | `TerminalPanel.tsx:82-104` [Code] |
| Agent draft | composer input | bounded in-process map by `sessionId` | panel remount during the application session | confirmed send or bounded eviction | accidental close/input loss; private paths | `TerminalPanel.tsx`; terminal remount tests [Code/Tests] |
| Agent transcript/journal | agent events | SQLite | journal/reopen immediately after resume/send | explicitly consented confirmed delete | private content/loss | agent journal/commands and terminal tests [Code/Tests] |
| Checkpoints | agent turn or pre-restore safety transaction | local checkpoint storage plus opaque index backup for ephemeral checkpoints | restore action or safety rollback | retention/pruning; explicit ephemeral cleanup | disk use/worktree or staged-state loss | checkpoint/session code [Code/Tests] |
| Copy/move/export replacement | file batch mutation | neighboring staging and backup for every object before commit | whole-batch rollback; incomplete source restore remains manifest-addressable | cleanup after commit with explicit warning | interrupted replacement/content loss | `file_ops/commands.rs`; `wsl_agent/runtime.rs` [Code/Tests] |
| Deleted-file undo | consented delete | canonical-repo-bound backup manifest | idempotent selective undo/redo, including `recovery_required` | expiry/process cleanup | data loss or cross-repo replay | file ops, contract and explorer [Code/Tests] |
| Live connection state | listener/snapshot/session refresh | memory | last successful snapshot/session list survives a channel failure | successful reload/reconnect | stale signal | `bus/store.ts`; `bus/connection.ts` [Code/Tests] |
| Pumarejo audit evidence | explicit development session | driver memory; optional artifacts directory is configured with retention off | not restored by Tinto | session close/driver cleanup | screenshots and semantic snapshots can contain repository or conversation content | `.pumarejo.json`; Pumarejo MCP contract [Code/Declared] |

## 8. External and asynchronous boundaries

| Boundary | Trigger | Pending signal | Success | Failure/retry | Evidence |
|---|---|---|---|---|---|
| Frontend ↔ Tauri IPC | startup/actions | loading/busy or mutation-pending state | command/event result | scoped error/retry; last successful channel state retained | `src-tauri/src/lib.rs`; `bus/connection.ts` [Code/Tests] |
| Local Git/filesystem | monitor/action | activity/loading or mutation pending | snapshot/diff/event or committed transaction | per-channel degraded/retry; mutation rollback/error | `bus/connection.ts`; `file_ops/commands.rs` [Code/Tests] |
| Git remote | confirmed fetch | updating | refreshed refs | error | FLOW-016 [Code/Unverified] |
| WSL | add WSL repo/agent or copy/move/export | distro/path loading or mutation pending | registered repo/session or committed transaction | empty/error/retry or transactional rollback | FLOW-006; `wsl_agent/runtime.rs` [Code/Tests; Runtime Unverified] |
| Agent processes | launch/send/stop/resume/restore | starting/working/settling/restoring | completed/checkpoint or immediate known-session open | channel/journal refresh errors retain prior lists and retry; restore uses safety rollback | connection, terminal and checkpoint tests [Code/Tests] |
| Gitleaks release download | install | installing | verified available | checksum/network error | FLOW-017 [Code/Unverified] |
| Native OS | picker/notification/window | platform state | chosen/notified/detached | denied/unavailable/error | Code/Unverified |
| Pumarejo MCP/WebDriver | explicit audit-only debug launch/action | launch/action request; references scoped to one snapshot generation | semantic snapshot, screenshot or completed action | structured error; refresh refs after mutation; explicit close owns cleanup | FLOW-032; `.pumarejo.json`; Pumarejo MCP contract [Code/Declared/Observed] |

## 9. Destructive and high-consequence actions

| Action | Consequence | Reversible | Protection | Safe test path | Evidence |
|---|---|---|---|---|---|
| Delete workbench | configuration loss | partial via other workbench | confirmation; repos untouched | disposable profile | Code/Unverified |
| Remove repo | monitoring removal | re-add | confirmation; files untouched | fixture config | Code/Unverified |
| Fetch | mutates Git refs/network | Git-dependent | preview + confirmation + backend validation | disposable remote repo | Code/Unverified |
| Overwrite file batch | content replacement | yes during the transaction | explicit conflict dialog + stage-all/commit/whole-batch rollback + cleanup warning | fault-injected temporary repo | Code/Tests; real runtime Unverified |
| Delete file batch | file removal | yes while manifest valid | confirmation + backend consent + repo-bound recovery manifest + visible retry | disposable repo | Code/Tests; real runtime Unverified |
| Stop/revert/restore | process/worktree/index/transcript change | varies | confirmation + backend consent + ephemeral worktree/index checkpoint; dirty symlink fail-closed | synthetic agent/repo | transaction Code/Tests; native effect Unverified |
| Delete journal | conversation loss | no | confirmation + active guard + mandatory backend consent before registry/SQLite | disposable profile | Code/Tests; real runtime Unverified |
| Install add-on | binary/network write | removable outside flow | explicit action + checksum | disposable tool dir | Code/Unverified |

## 10. Content and scale envelopes

| Surface | Empty | Typical | Long/extreme | High volume | File/input limits | Evidence |
|---|---|---|---|---|---|---|
| Dashboard | no-match state and reset [Observed]; zero repos [Code] | 2 real repos [Observed] | native 800 px reflow; baseline fixture 390 px [Observed] | unbounded repos [Unverified] | filters text/select | Dashboard sources; `OBS-2026-07-28-002..006` |
| Explorer | empty/no matches [Code] | current repository with changed/untracked markers [Observed] | truncated state [Code] | 20,000 entries | path-root bounded | `bus/contract.rs:389-393`; `OBS-2026-07-28-013` |
| Text file/diff | no hunks/binary [Code] | 80-line fixture [Observed] | large/long guard [Code] | 2,500 rendered lines | 1 MiB text; 8,000 chars/line | diff limits |
| Media | unavailable [Code] | Unverified | 12 MiB cap | NA | 12 MiB | contract/media code |
| Timeline | empty/filtered [Code] | mixed cross-repo events and commits [Observed] | 8 commits/repo model window; prior rows retained and faster repos publish before slower ones [Code/Tests] | events across repos | orphan window 30 min | timeline model; delayed/failure tests; `OBS-2026-07-28-009..010` |
| Agent transcript | empty/read-only [Observed] | 2 turns [Observed] | 20,000 output chunks; bounded responsive transcript and compact DOM/visual focus parity [Code/Tests] | 2,000 timeline items | 10 files/4 images; 5 sessions; 4 h | agent sources; `App.css`; console tests |
| Overview ruler | zero-line [Code] | 80 lines/16 marks [Observed] | up to 600 rendered mini-lines | same-line clustering and non-overlapping dense targets [Code/Tests] | keyboard slider 1..N with exact pointer/keyboard target | ruler sources; `FileOverviewRuler.test.tsx` |

## 11. Platform, input and accessibility expectations

| Platform/flow | Keyboard | Touch | Focus | Back/deep link | Reduced motion | Assistive tech | Evidence |
|---|---|---|---|---|---|---|---|
| Main Windows/Linux shell | Native menu pointer/Escape path [Observed]; F10/isolated Alt, menu arrows/Enter/Escape and modal shortcut suspension [Code/Tests] | pointer rules [Code] | native shortcuts dialog focus entry/return [Observed]; shared trap/restore [Code/Tests] | panel model; no router | CSS rule [Code] | menu/dialog semantics observed; top-level Dockview tabs lacked exposed tab role/name in Pumarejo snapshots | `MenuBar.tsx`; `shortcuts.ts`; `useAccessibleDialog.ts`; shell tests; `OBS-2026-07-28-008,012,015` |
| Dashboard | native menu/filter controls [Observed] | Unverified | pointer path observed; keyboard traversal inconclusive because the driver retained focus on Minimize | panel return [Code/Observed] | CSS | live/status regions observed; repeated repository controls shared non-contextual accessible names | `OBS-2026-07-28-002..008,016` |
| File ruler | arrows/Page/Home/End/Escape and exact line activation [Code/Tests]; End baseline Observed | exact pointer target [Code/Tests] | one focusable target per line; slider/marker focus [Code/Tests] | file context | CSS | slider values and marker names [Baseline Observed; Code/Tests] | `OBS-015/016`; `FileOverviewRuler.test.tsx` |
| Agent conversation | search/Enter/Escape/tabs [Observed]; compact focus follows visual rows [Code/Tests] | Unverified | dialog/tab semantics and remount draft continuity [Code/Tests] | navigator/journal with immediate post-resume open [Observed/Code/Tests] | CSS | log, status, regions, tabs; bounded responsive content [Baseline Observed; Code/Tests] | `OBS-007..013`; Agent console tests; `App.css` |
| Native/WSL/detached | explicit Reanexar control by click/keyboard plus drag reattachment [Code/Tests] | pointer drag [Code] | reattach control [Code/Tests] | query-window handoff [Code] | Code | screen reader not tested | `DetachedConsolesApp.tsx`; console tests; runtime Unverified |

## 12. Coverage ledger

| Item | Status | Evidence | Last checked | Gap/next probe |
|---|---|---|---|---|
| Intent and product priority | covered | owner + docs | 2026-07-17 | none |
| Supported platforms | covered | owner + README; Windows native partial smoke | 2026-07-28 | Linux and release-bundle runtime pending |
| Surface inventory | covered | code-wide cartography plus native Windows sample | 2026-07-28 | `SURF-033` retired; ID retained |
| Browser root/bootstrap | partial | current launchpad Code/Tests; baseline browser `OBS-001`; native Pumarejo launch | 2026-07-28 | browser launchpad runtime and release-bundle startup |
| Dashboard ready/loading/filter/responsive | covered for sampled states | native real-data ready, matched/unmatched/reset, live status and 800/1920 layouts; baseline loading fixture | 2026-07-28 | true zero-repo and higher-volume native data |
| File ruler keyboard/pointer/cluster | partial | baseline `OBS-015/016`; current exact-target Code/Tests | 2026-07-17 | runtime confirmation with dense real data |
| Agent completed/working/journal/details | partial | baseline `OBS-007..011`; native no-active state, quick launch and recent sessions | 2026-07-28 | real launch/send/checkpoint and native transcript detail |
| Agent compact/navigator/runtime fixture | partial | baseline `OBS-012..014`; native 800 px Agents overview; current responsive/localized Code/Tests | 2026-07-28 | native transcript container sizes |
| Connection channel degradation/reconnect | partial | current `connection.test.ts` and store/connection Code | 2026-07-17 | native listener failure/recovery smoke |
| Timeline refresh continuity | partial | native mixed cross-repo list/detail plus delayed/failure tests | 2026-07-28 | native failure/recovery and high-volume refresh probe |
| Windows native end-to-end | partial | visible Pumarejo-driven dirty-working-tree session at 800×600 and 1920×1032 | 2026-07-28 | disposable profile, installer/release bundle and high-consequence flows |
| Linux native | unverified | unavailable | 2026-07-17 | Linux smoke |
| WSL | unverified | no installed distro | 2026-07-17 | WSL2 Ubuntu disposable repo |
| Mutating/destructive flows | partial | pending/consent, stage-all batch rollback, cleanup warnings, repo-bound recovery and safety-checkpoint fault injection [Code/Tests] | 2026-07-17 | disposable profile/repo native mutation and rollback |
| Network/external flows | out-of-scope | no authorization/data | 2026-07-17 | controlled remote/tool endpoint |
| Screen reader/high contrast/touch | partial | Pumarejo semantic snapshots for sampled native surfaces; no real assistive technology | 2026-07-28 | NVDA/Orca, contrast and touch pass |
| Extreme content/volume | partial | code limits | 2026-07-17 | boundary fixtures |
| Automated frontend verification | covered for code/tests | post-remediation tsc/eslint/Prettier/contract/build pass and 52 Vitest files/681 tests pass | 2026-07-17 | repeat on a clean commit; native tests remain separate |

The baseline coverage gate was accepted for the reduced browser-and-code audit at commit `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`. On 2026-07-28 a reduced Windows-native working-tree review was accepted only for the surfaces explicitly marked Observed above. The atlas remains stale, so the overall coverage gate is not accepted until the changes are committed, the tree is clean, a new fingerprint is computed and targeted runtime observations are refreshed. Neither state is an exhaustive native-platform certification.

## 13. Open intent questions and conflicts

- Resolved 2026-07-17 by product owner: observation and Agent direction are co-primary rather than competing product intentions.
- Resolved 2026-07-17 by product owner: macOS is not officially supported; bundle configuration only indicates technical compilation capability.
- Historical conflict: `tinto-design.md` excludes agent integration and all repo mutation, while current README/roadmaps distinguish read-only monitoring from explicit workspace/Agent tools. Current README and roadmap govern.
- Resolved in the current working tree: README now describes Agent session consoles rather than a generic integrated shell; `SURF-033` is retired and its ID remains reserved.
- Resolved in the current working tree: browser-only `/` is an explicit review launchpad that avoids Rust commands and Agent processes. `OBS-001` describes the baseline commit only.

## 14. Change log

- 2026-07-17: Created the initial atlas from documentation, code cartography, owner intent decisions, browser observations and automated frontend checks at source commit `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`.
- 2026-07-17: Updated the atlas as a stale working-tree draft for the browser review launchpad, retired `SURF-033`, pending and transactional file mutations with explicit consent, per-channel connection retention, Timeline continuity, exact ruler targeting, responsive/localized Agent surfaces and F10/Alt/modal/reattach behavior. The baseline commit and fingerprint remain unchanged pending a clean commit and revalidation.
- 2026-07-17: Extended the stale remediation draft with atomic multi-object transactions, cleanup warnings, repo-bound partial recovery, worktree/index safety checkpoints, immediate journal resume, retained journal refresh state, incremental Timeline publication, isolated-Alt handling, multi-selection, compact focus order and in-process Agent draft continuity. Evidence remains Code/Tests unless explicitly marked Observed.
- 2026-07-28: Added a reduced Windows-native observation pass driven through Pumarejo over the dirty working tree: bootstrap, Dashboard/filter/empty state, menus, Timeline list/detail, Agents overview, shortcuts dialog, repository tree and a small diff. Linux/WSL, external, destructive and real-agent paths remain unverified; source commit and fingerprint remain the 2026-07-17 baseline.
- 2026-07-29: Mapped the Code/Tests remediation of `POL-10-001`, `POL-07-001` and `POL-10-002`. Automated regression passed; the attempted Pumarejo re-observation failed before Tinto launch and did not create new native Observed evidence.
