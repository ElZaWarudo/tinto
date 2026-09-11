# Tinto daily-use hardening: bounded diagnosis — 2026-09-07

Original diagnosis scope (historical; latest reconciliation below). Windows cwd `C:\Users\User\Documents\personal\tinto`, branch
`fix/reconcile-native-hardening`. Read root AGENTS.md and the existing native
reconciliation; no nested AGENTS.md found in the inspected repositories. Preserve
all 16 modified files and the untracked reconciliation document. No source edits,
stash, branches, installs, builds, app launches, restarts, agents, commits or push.
The user explicitly selected diagnosis only, overriding the debug skill's stash
experiment and fix-choice prompt. There is no issue of record; remote issue/PR
search was not performed in this bounded local investigation.

## Finding and confidence

**Recommend a snapshot-cancellation policy correction in Pumarejo first, not a
speculative Tinto recovery or PATH rewrite.** The destructive cancellation
mechanism is reproduced with high confidence. Attribution of the two historical
interruptions to that mechanism is strongly supported, but remains conditional:
the saved evidence lacks the actual timeout notifications and process-exit trace.
The precise cause of slow deep traversal is not established.

### Facts and smallest checks

Native evidence: `C:/Users/User/AppData/Local/Temp/icook-through-tinto-20260907`.
Only selected `result.structuredContent` fields were used for the evidence summary.

| Evidence | Observation |
| --- | --- |
| User report | Two maxDepth100 calls exceeded client 65000 ms; timeout calls themselves are absent from the numbered result files. |
| 024 | Successful depth100 region/status snapshot took 44025 ms despite returning only six nodes. |
| 025 / 049 | Pumarejo `idle`, `lastAction: snapshot`, at 08:42:05.286Z / 08:59:12.789Z. |
| 027 / 050 | `SESSION_NOT_ACTIVE`; post-loss diagnostics cannot recover the deleted active diagnostic context. |
| 032 | ICook session `b5e1eea7-2f01-46f1-bd6f-415cc9d4bd58` archived with 1846 events. |
| 055 | That session and `f02af164-58af-4e2a-9bab-4a4aa727ba37` archived; the latter has 2352 events. |
| 057 | Archived state and composer observation responsive in 6177 ms at depth50. |
| 060 / 061 / 063 | Depth50 status observations took 9814 / 8086 / 9072 ms. Responsive, not evidence of consistently fast operation. |

These UUIDs identify Tinto Agent sessions, not Pumarejo control sessions.
History recovery is observed; uninterrupted work and complete provider-state
recovery are not proven by those snapshots.

Two small tests ran without a native process or source edits:

1. In-memory installed MCP SDK Client/Server with a deliberately pending tool and
   30 ms request timeout: client error `-32001`, server AbortSignal aborted `true`.
2. Adjacent Pumarejo existing test:
   `node node_modules/vitest/vitest.mjs run tests/unit/mcp-runtime.test.ts -t 'cancels an active call and closes every resource' --no-cache --configLoader runner`.
   Exit 0; one passed, 27 skipped; selected test 14 ms. It explicitly asserts
   manager/artifact closure, subsequent `SESSION_NOT_ACTIVE`, and
   `{state: "idle", lastAction: "snapshot"}`. This reproduces the undesirable
   existing contract, not a fixed product.

Local detailed diagnostics:
`%TEMP%/tinto-daily-diagnosis-20260907-cancellation-test.log` and
`%TEMP%/tinto-daily-diagnosis-20260907-evidence.json` (selected metadata only).

### Causal chain and competing explanations

Verified local code chain, paths relative to adjacent `../pumarejo` unless noted:

- Installed SDK `node_modules/@modelcontextprotocol/sdk/dist/esm/shared/protocol.js:670`
  sends `notifications/cancelled` on timeout (`:713`); the in-memory check confirms
  server cancellation delivery.
- `src/mcp/server.ts:125` passes the caller signal to snapshot.
- `src/mcp/runtime.ts:704` runs snapshot through `run`; `:1270` combines caller
  and internal cancellation; `:1310` calls `closeNow` on an aborted active call.
- `runtime.ts:1338` clears the active session, `:1361` closes the manager, and
  closes diagnostics. `:1314` sets idle while preserving lastAction.
- `src/session/manager.ts:769` drains cleanup; the application-process callback
  at `:415` invokes owned-process termination (`:805`, custody terminate `:841`).
  The built adjacent `dist/mcp/runtime.js:832` contains the same abort-close path.
- Tinto `src-tauri/src/agent_console/journal.rs:695` projects unfinished stored
  Starting/Running sessions as Exited on archival read. This explains why history
  can survive loss of the live host; it does not identify the original exit cause.

Ranked hypotheses:

1. **Caller timeout cancellation closes the Pumarejo-owned process tree.** Exact
   status signature reproduced, SDK bridge verified, and cleanup code traced.
   Historical missing link: actual timeout notification and owned-process exit
   receipt from those two runs; tested adjacent build provenance is not proof of
   the precise binary used by the historical controller.
2. **Development watcher restart or other host/process exit.** Watch-enabled
   launch supports this possibility, but no restart/crash receipt was recovered.
   Such an exit alone does not establish the observed runtime abort-cleanup path.
   Transport closure also invokes runtime shutdown (`src/mcp/server.ts:303`).
3. **Tinto spontaneously loses the Agent or dirty recovery code regresses.**
   Existing journal/frontend changes make this a legitimate competing hypothesis;
   preservation rules prohibit a clean-tree comparison. No evidence independently
   links those changes to the control-session teardown. Do not patch them on timing.

Raw `.pumarejo/artifacts` access was denied. Adjacent Git history access hit dubious
ownership; no global Git setting was changed. Missing lifecycle telemetry limits
historical attribution. No destructive native reproduction was attempted.

Performance is a separate investigation: `src/observation/browser-entry.ts:643`
limits traversal depth, but `:657` reads element text before role filtering at
`:676`. A small output/roles filter does not imply cheap traversal. Do not optimize
this or Tinto transcript rendering without per-stage timings on the same transcript.

## Windows Agent tool environment

Current terminal resolves node from Codex's runtime `dependencies/node/bin/node.exe`,
git from `C:\Program Files\Git\cmd\git.exe`, and cargo from the user's `.cargo/bin`.
Neither `npm` nor `npm.cmd` resolves. This is not proof npm is uninstalled.

PATH still contains `C:\nvm4w\nodejs`; that link targets
`C:\ProgramData\nvm\v24.13.0`. Access to its `npm.cmd` is denied in this terminal.
The accessible fallback Node directory contains only `node.exe`; PATHEXT includes
`.CMD`. The terminal account reported by Git is `CodexSandboxOffline`, distinct
from the owning user. These observations establish restricted access at the
terminal boundary, not a missing PATH entry. They do not distinguish an OS ACL
from the managed sandbox's access restrictions.

Tinto prefers Codex app-server (`src-tauri/src/agent_console/pty.rs:364`), whose
builder (`app_server.rs:868`) inherits the process environment and does not clear
PATH. Workspace permission maps to `workspace-write` (`app_server.rs:975`). The
PTY fallback (`pty.rs:627`) also inherits its base environment; portable-pty 0.9.0
`src/cmdbuilder.rs:74` copies `std::env::vars_os`. Terminal overlays at `pty.rs:732`
do not replace PATH. ACP allowlists explicitly retain PATH, but are not evidence
of the active Codex route. No Tinto PATH-dropping defect is established.

**Minimal truthful remedy:** report npm as unavailable/inaccessible in this Agent
environment. Have the controller compare npm resolution/readability in its launch
environment and in the managed Agent, then use the runtime's supported mechanism
to expose an existing accessible npm distribution if available. If the fallback
runtime intentionally bundles Node alone, that is a runtime packaging/access gap,
not a reason to invent a Tinto PATH manager. Do not hardcode these observed paths,
install tools, disable sandboxing, or broaden permissions as this diagnosis's fix.

## Self-hosting gate and exact proposed scope

`.pumarejo.json:4` launches `npm run tauri -- dev --features pumarejo --config
{tauriConfig}`. There is no `--no-watch`. `src-tauri/tauri.conf.json:8` starts Vite
with `npm run dev`; `vite.config.ts` ignores src-tauri in Vite's watcher only.
Rust source edits can still restart Tinto through Tauri's watcher.

Supported **Rust no-watch** route: controller supplies the existing launch args
with `--no-watch` after `dev`, preserving feature/config arguments and the single
`{tauriConfig}` placeholder. Installed `node node_modules/@tauri-apps/cli/tauri.js
dev --help` confirmed the flag. Pumarejo's schema (`src/config/schema.ts:37`) accepts
the args, and `src/config/load.ts:124` materializes them without removing the flag.
This requires the controller's working npm environment. It was not launched here.
Vite HMR remains active: this is not a fully frozen frontend route. No existing
fully frozen launch profile was established; do not call no-watch fully frozen.

Before implementation the controller must safely end/recover active work, relaunch
through that supported route, and verify launch args/process identity and Agent
scope. This Agent must not restart its own host. Never rebuild/replace its running
executable during the session. Frontend edits need separate reload isolation.

Historical proposed patch (now implemented in adjacent Pumarejo; see latest reconciliation below):

- `../pumarejo/src/mcp/runtime.ts`: distinguish **caller cancellation of snapshot**
  from explicit close/shutdown cancellation. Release the failed observation/FIFO
  without `closeNow` for that caller-only case. Preserve explicit ownership cleanup
  and all other operation policies. Do not infer readiness if the provider died.
- `../pumarejo/tests/unit/mcp-runtime.test.ts`: replace the existing snapshot
  abort-closes-session expectation with session survival; add an in-memory SDK
  timeout-through-runtime case in this same existing test home. Assert no manager
  or artifact close, a usable subsequent shallow snapshot, and diagnostic access.
  Retain explicit close/shutdown tests, add timeout/close race and queued-call
  cancellation coverage, and verify failed captures do not publish valid refs.
- Controller launch configuration only: append `--no-watch` to Tinto's existing
  `.pumarejo.json` args when preparing the safe relaunch. No change made here.
- No Tinto Rust/frontend/env patch is justified by current evidence. In particular,
  do not amend the pre-existing journal or terminal changes for this symptom.

At diagnosis time, the cancellation test passed because it encoded the old cleanup policy;
changing it is an intentional contract change, not merely correcting a bad mock.
Start with those focused tests, then related runtime/snapshot cleanup tests; no
broad test pass can substitute for the native acceptance below.

## Native acceptance (controller-owned, after patch)

1. Record actual controller package/build identity, launch args, Tinto/provider PIDs,
   Agent session/thread identity and cwd. Confirm the no-watch route first.
2. With representative long history and a bounded active Agent task, capture shallow
   status, then cause one observation client timeout. Record timeout, cancellation,
   cleanup decision and process identity; do not rely on elapsed time alone.
3. Require the same Tinto process and Agent to remain live, the pending task to
   complete, diagnostics to remain readable, and a fresh shallow snapshot/composer
   to work without archival/resume. A dead provider must be reported truthfully.
4. Separately verify explicit close still cleans owned resources; relaunch and check
   persisted history/resume. Record live continuity separately from history recovery.
5. Compare controller/Agent `Get-Command node,npm,git,cargo` and npm file readability;
   after any supported environment remedy, run `node --version` / `npm --version`
   inside the same Agent scope. No npm-present claim from Node availability alone.

This pass establishes a bounded failure mechanism and proposed correction, not
long-session performance or broad daily-use maturity.

## Authorized implementation attempt — historical blocker before source edits

The follow-up authorized the narrow Pumarejo patch, its regression tests and build,
without a Tinto restart. Actual adjacent baseline: root
`C:/Users/User/Documents/personal/pumarejo`, branch `main`, HEAD
`41db8368e48b11b8fb79b2d980a93e42f4e1dc35`, clean `git status --short` (zero dirty
files, hence no existing dirty-file hashes there). Git used a command-local
`-c safe.directory=...` exception; no Git configuration was written.

SHA-256 preservation baseline is saved at
`%TEMP%/tinto-cancellation-fix-baseline-20260907.json`: Pumarejo clean; Tinto 18
dirty files; adjacent Windows ICook 580 dirty/untracked files on unborn `main`.
This describes that actual adjacent checkout, not a claim about the WSL campaign
repository. No ICook files were edited.

Watcher check: canonical Pumarejo `src` and `dist` are outside Tinto; Tinto's
canonical staged provider is `.pumarejo/provider`, and Cargo references that
staged copy. The Pumarejo browser/provider build scripts write inside its own
`dist`; they do not stage files into Tinto. No relaunch is needed for the proposed
adjacent TypeScript/test patch and build alone. Do not run integration/staging or
replace the active controller connection as part of that patch.

**Execution blocker:** this Agent's managed filesystem policy allows writes under
Tinto and temporary directories, but not the adjacent Pumarejo checkout; approval
escalation is disabled (`never`). User authorization is present, but the execution
boundary has not changed. No write bypass, source edit or build was attempted.
No new test evidence exists for a fix. Resume requires a controller-configured
execution scope that makes this adjacent checkout writable, while keeping the
current native Tinto host open. The earlier cancellation reproduction remains
valid; historical incident attribution remains conditional.

### Public MCP controller recipe, after implementation and build

Use the supported MCP Client `callTool` API on a controller that has loaded the
patched build. An already-running server does not automatically acquire rebuilt
JavaScript. Do not disconnect the current owning MCP transport to swap builds:
transport shutdown owns cleanup and can terminate Tinto. Arrange the patched
controller's native acceptance session separately when active work is safely saved.
This implementation attempt did not do that.

With an already connected `client` owning that patched acceptance session:

```js
const call = (name, args = {}, timeout = 65000) =>
  client.callTool({ name, arguments: args }, undefined, { timeout });
const before = (await call("tauri_status")).structuredContent;
const diagnosticsBefore = (await call("tauri_diagnostics", { maxRecords: 10 }))
  .structuredContent;
// Record these and the active Tinto Agent session/thread identity beforehand.
let timedOut = false;
try {
  await call("tauri_snapshot", {
    maxDepth: 50, maxNodes: 64, maxTextLength: 2000, roles: ["status"]
  }, 100);
} catch (error) {
  if (error.code !== -32001) throw error;
  timedOut = true;
}
const after = (await call("tauri_status")).structuredContent;
const diagnosticsAfter = (await call("tauri_diagnostics", { maxRecords: 20 }))
  .structuredContent;
const fresh = await call("tauri_snapshot", {
  maxDepth: 50, maxNodes: 64, maxTextLength: 2000,
  roles: ["status", "textbox"]
});
```

Require `timedOut`, before/after state ready, equal nonempty `ownedPid`, equal
nonempty diagnostics `sessionId`, readable diagnostics and a successful fresh
snapshot. Inspect selected diagnostic fields to establish that cancellation reached
an active observation; a request cancelled before execution is insufficient proof.
If it finishes within 100 ms, record timeout not induced rather than claiming pass.
Do not close/relaunch between these calls. Also verify the same Tinto Agent
session/thread remains live and completes its bounded task without archival or
resume; Pumarejo readiness alone is insufficient. Explicit teardown acceptance is
a separate later `tauri_close` on the disposable acceptance session after work ends,
followed by `tauri_status` idle and owned-resource cleanup checks.

## Latest documentation reconciliation — source fix complete, native acceptance blocked

The [Pumarejo cancellation evidence](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md)
supersedes the historical implementation blocker above. The local patch changes
`src/mcp/runtime.ts` and two test files, with an evidence note and prepared
acceptance client. Caller-only snapshot cancellation retains the session;
explicit close/shutdown cleanup remains intact. No commit, push or release.

Pumarejo records **83/83 tests across 3 files**, typecheck, targeted lint, diff
review/check and acceptance-client syntax passing; format pass is controller
reported. These checks were not rerun in this documentation pass. Three build
stages passed (clean, TypeScript, provider bundle), but the browser bundle failed:
esbuild cannot read `../../..` (`C:/Users/User`, access denied), even with explicit
settings and the invocation-only recovery probe. The compiled runtime contains
the fix, but `dist/observation/snapshot-browser.js` remains missing (also checked
read-only here). This is an incomplete build, with **no native acceptance**.

Before starting a new patched controller, complete
`node scripts/build-browser-bundle.mjs` in a permitted Pumarejo environment and
require a nonempty browser artifact. Do not execute it here or bypass permissions.
The prepared acceptance client is syntax-checked only. Native process/Agent
continuity, diagnostic access after timeout, and explicit owned-resource cleanup
still require controller-owned acceptance after the build. The controller owns
final close/status after these docs; no close, launch or transport swap occurred
in this pass.

The npm permission/access problem is distinct from the browser resolver's ancestor
read denial. Neither establishes a Tinto PATH-drop defect. WSL slow/failing
readiness remains a separate unresolved track. No duplicate repair is needed.
### Latest old-driver receipts and preservation

Source directory: `C:/Users/User/AppData/Local/Temp/tinto-daily-readiness-20260907`.
These receipts describe the old driver, not acceptance of the patched build:

| Receipt | Observation |
| --- | --- |
| 039 | `idle`, `lastAction: snapshot`, after a controller-reported 65000 ms snapshot timeout. The receipt itself does not capture the timeout or process-exit chain. |
| 043 | `cleanup_failed`, `APP_START_FAILED` in launch, PID 6800, application-process cleanup pending. |
| 045 | Diagnostics retained: provider loopback ready, then `starting_proxy` / `app_start_failed` and `session_not_active`; watcher activity is recorded. This does not identify the initial timeout's process-exit cause. |
| 046 | Explicit close returned `idle`, `alreadyClosed: false`. |
| 048 | Relaunch reached `ready`, owned PID 10224. Recovery is observed; continuity is not established. |
| 078 | `idle`, `lastAction: screenshot`, following the controller-reported ENTER timeout. Do not relabel this as a snapshot receipt. |
| 084 | Archive lists reconciliation session `c006f94c-8cc0-4791-92c1-e8fa38c044ab`, 4453 events. Controller confirms it stopped after the ENTER timeout; no duplicate worker or repair was started. |

Six small raw JSON receipts (24479 bytes total) were copied into the existing
Tinto evidence area. Source and destination SHA-256 values were compared. The
large archive snapshot/transcript was not copied; only its relevant observation
is summarized above. Receipt 084 remains in the source directory.

| Copied receipt | SHA-256 |
| --- | --- |
| [039-tauri_status.json](evidence/2026-09-07-daily-039-tauri_status.json) | `3c04ab8031730a8609cfc4e1e09998928d2ce8933d73720455f31cc9afcfa63a` |
| [043-tauri_status.json](evidence/2026-09-07-daily-043-tauri_status.json) | `982e16fd1bef65e36e4ba69fe603b88b275903d150dd1786485ffb85a87c63cd` |
| [045-tauri_diagnostics.json](evidence/2026-09-07-daily-045-tauri_diagnostics.json) | `b6dac60fba6768178d493e4e66a4151cc2d52722305dd0acda4ca40d1cf0f9e3` |
| [046-tauri_close.json](evidence/2026-09-07-daily-046-tauri_close.json) | `a557774dc69ef76b2d1bcab6a517c97ac321bbf2774805cb466a00ecee7483d9` |
| [048-tauri_status.json](evidence/2026-09-07-daily-048-tauri_status.json) | `8931ec63c71ef1307b6bc59752455a6521c66e74d4f8f88b25f4fd5662bfb6b4` |
| [078-tauri_status.json](evidence/2026-09-07-daily-078-tauri_status.json) | `cc2eb117a2925237dcf3e5bbfe8aae182b17b215b9d543c77567faffc77b5d0e` |

Original 084 SHA-256: `9040a6004e78df8ded1970cda4c81f657bfdfdb01d9f5bd0ff8640a2e375ca0a`.

Preservation comparison used the existing
`%TEMP%/tinto-cancellation-fix-baseline-20260907.json`. All 14 recorded Tinto
source/config/lock/test files and the two other pre-existing docs
(`docs/product/roadmap.md`, `docs/swarm/blockers.yaml`) match their baseline
SHA-256 values. The two orchestration docs already differed before this pass;
their existing text was retained, with historical labels clarified and this
reconciliation appended. No identity claim extends to files absent from that
baseline; adjacent Pumarejo intentionally differs because its authorized patch
is complete. No Tinto product source edits, tests, builds, installs, agents,
commits, pushes, app close or launch were performed in this reconciliation.
## Full-access build recovery — 2026-09-07

This later pass supersedes the incomplete-build status above; earlier observations
remain historical facts. Cwd and Git root were verified as
`C:/Users/User/Documents/personal/pumarejo`, branch `main`. Effective session
instructions are `danger-full-access`, unrestricted filesystem, network enabled,
and approval policy `never`. The controller confirmed **Acceso completo** in Tinto.
PowerShell read every ancestor through `C:/`; Node `readdirSync` also successfully
read the previously blocked `C:/Users/User` (118 entries).

Resolved executables: Node `C:/nvm4w/nodejs/node.exe` (v24.13.0), npm
`C:/nvm4w/nodejs/npm.ps1` (11.6.2), Git `C:/Program Files/Git/cmd/git.exe`.
These are this pass's observations, not corrections to the prior restricted scope.

**Normal `npm run build`: exit 0**, completing clean, TypeScript, browser bundle,
and provider bundle. Before clean, canonical `dist` was verified inside Pumarejo
and no tracked dist files were found. `dist/observation/snapshot-browser.js` now
exists and is nonempty (**24616 bytes**). `node dist/cli/index.js --help` loaded
successfully (exit 0); runtime and acceptance-client syntax checks also exited 0.
Build log: `%TEMP%/pumarejo-cancellation-full-access-build.log`.

Unexpected build side effect: the existing `pnpm clean` invocation automatically
performed dependency reconciliation, reporting five packages added from cache,
zero downloaded, and an up-to-date lockfile (pnpm 11.19.0). No separate install
command was invoked; this pass cannot claim that no installation activity occurred.
No new tracked package/lockfile changes appeared in Pumarejo status.

No source patch changes were needed, so the prior **83 passing tests were not
repeated**. Existing Pumarejo/Tinto/ICook work was retained; edits in this pass are
limited to this evidence and the two existing Tinto orchestration records, plus
normal generated build output and the dependency side effect above. No commits,
pushes, release, other Agents, provider staging, Tinto launch/close, or active
controller transport swap occurred.

### Controller-reported computer-use fallback and remaining acceptance

The controller used **sky** to reach Tinto controls and the native permission
dialog because the missing browser bundle broke Pumarejo semantic extraction.
`tauri_dialog` falsely reported `provider_dialog_absent`. This is computer-use
fallback evidence, **not Pumarejo capability proof**; the successful build does
not establish that native dialog detection is repaired.

Native acceptance remains **pending the controller run**. From Pumarejo, after
saving active work and preparing the separate disposable no-watch launch:

```powershell
node docs/evidence/2026-09-07-cancellation-acceptance.mjs --run-disposable-native C:/Users/User/Documents/personal/tinto
```

Arguments are the required `--run-disposable-native` flag, required Tinto project
path, and optional final evidence-directory path (default: timestamped
`%TEMP%/pumarejo-native-cancellation-*`). The unchanged client starts the patched
MCP CLI over stdio, launches its disposable visible session, checks ready/PID and
diagnostics, induces a 100 ms snapshot timeout (depth 100, 64 nodes, status role),
then checks status, diagnostic session identity and a fresh depth-4 snapshot.
It requires a cancellation notification and stable owned PID/session; failure to
induce timeout fails acceptance. It explicitly closes its disposable session even
on failure after a launch attempt and checks idle, retaining raw responses/errors,
wire cancellation, stderr, build hashes and launch configuration.

Controller review must still prove cancellation reached an active observation,
the actual Tinto/provider processes and same Agent thread survive and finish work
without archival/resume, and explicit close removes owned OS resources. The client
has not been run here; CLI loading and build completion are not native acceptance.


## Native runtime-session acceptance reconciled — 2026-09-07

**PASS: native runtime-session cancellation acceptance**, resolving the earlier
pending runtime-acceptance status after the successful full-access build. Earlier
blocked attempts remain historical. See the
[canonical Pumarejo reconciliation](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#native-runtime-session-acceptance-reconciled--2026-09-07)
and [copied evidence/hash manifest](evidence/2026-09-07-full-access-native-manifest.json).

Controller source: `%TEMP%/tinto-full-access-native-20260907`; exit 0 is
controller-reported, while protocol-checks and explicit-close receipts directly
report pass. Raw SDK timeout `-32001` (100 ms) and wire cancellation for request
9 were inspected. Owned PID **23940** remained ready before/after cancellation
and after a fresh generation-2 snapshot; runtime diagnostic session
`f25ba4fca6a45a321cadd8862580e063` stayed the same. The depth-4 snapshot returned
one real status node (“Observación de archivos activa”); maxDepth truncation is
expected. Explicit close returned idle, followed by idle status without owned PID.

This is **not active Agent task continuity or long-session maturity proof**.
Active-observation attribution remains limited by ambiguous diagnostics
(`internal_error`, a 121 ms invocation labelled `diagnostics`, then snapshot).
Saved launch args lacked `--no-watch`; watcher output confirms this was not
proof of the proposed no-watch route. The sky permission-dialog fallback and
false `provider_dialog_absent` remain non-Pumarejo capability evidence.

Independent Win32_Process inspection at `2026-09-07T10:45:44.4923905Z` found
PID 23940 absent. No process was killed. Without acceptance-time OS creation
identity and descendant/resource inventory, PID reuse and full cleanup limits
remain; idle alone does not establish complete OS cleanup.

Before edits this Agent reverified Pumarejo cwd/root, main, danger-full-access,
unrestricted filesystem, network enabled, approval never, and successful Node
read of the previously blocked ancestor. Dirty baseline: Pumarejo 5 entries,
Tinto 24, Windows ICook 580. Only the same three docs and 14 new evidence/manifest
files are in this pass's write scope; pre-existing dirty-file hashes are checked.
The prior build's five cached package additions (zero downloads) remain disclosed;
there was no separate install command then, and no install/build/test/source change
in this pass. No commits, pushes, restart or other Agents; controller owns close.

Preservation verification completed: **606/606** pre-existing dirty files outside
the three authorized docs retain their pre-edit SHA-256 values. Final dirty counts
are Pumarejo **5** (3 modified, 2 untracked), Tinto **38** (16 modified, 22
untracked), Windows ICook **580** (all untracked). All 13 copied/generated receipt
hashes and both current build hashes match their recorded provenance.


## Remaining daily-use gaps — bounded diagnosis, 2026-09-07

Scope reverified before edits: actual cwd/root is Pumarejo
`C:/Users/User/Documents/personal/pumarejo`, main; adjacent Tinto root is
`C:/Users/User/Documents/personal/tinto`, fix/reconcile-native-hardening.
Windows ICook remains on unborn main. Effective instructions: danger-full-access,
unrestricted filesystem, network enabled, approval never. Ancestor read succeeds.
Node v24.13.0 and npm 11.6.2 resolve under C:/nvm4w/nodejs; Git 2.52.0.windows.1
under C:/Program Files/Git/cmd; WSL under C:/WINDOWS/system32/wsl.exe.
Pre-edit hashes/status: `%TEMP%/tinto-remaining-gaps-baseline-20260907.json`;
Pumarejo 5, Tinto 38, Windows ICook 580 dirty files. Prior 83 tests, complete build
and native runtime-session cancellation acceptance remain passed, not rerun.
Prior build-triggered dependency reconciliation (5 cached additions, 0 downloads)
remains disclosed. Existing source edits are potential contributors, not ruled out
by a clean-tree comparison: no stash, source edit, restart or alternate Agent was
permitted. ce-debug was used within the user's explicit diagnosis-only scope;
no issue of record or remote tracker search was introduced.

### Observation and restore evidence: confirmed versus unresolved

Current raw receipts in `%TEMP%/tinto-remaining-gaps-20260907`: snapshots 003,
006, 008 took 14321, 1673, 9616 ms; screenshots 011/013 failed at 30008/30011 ms;
012 restore failed INTERNAL_ERROR at 30020 ms. In the prior
`tinto-full-access-reconcile-20260907` receipts, screenshot 010 succeeded in
13612 ms, 011 failed at 30023 ms, snapshot 012 took 49512 ms for six nodes, and
screenshot 013 subsequently succeeded in 1972 ms. These timings confirm variability,
not its cause. No failure-stage profile was captured by these public envelopes.

Controller additionally reports sky observed MINIMIZED twice, sky activation
successfully restored the actual window after public restore failed, and the
subsequent depth-50 public snapshot timed out at 65000 ms. Raw 014 status was
independently read: ready, ownedPid 20724, generation 7, lastAction snapshot.
The controller observed this same active Agent continue with no archive/resume
since task start. This is positive real active-continuity evidence with a
controller-observation provenance; it is not yet the complete controlled acceptance
with task completion, provider/OS creation identities, and cleanup receipts.
All sky actions are fallback, never Pumarejo capability proof. Restoration did not
eliminate the later timeout, so minimization alone is not established as its cause.

Confirmed code defects/gaps (paths relative to Pumarejo unless prefixed Tinto):

- `src/webdriver/client.ts:1088` restores a cached 'restored' window by executing
  JS that checks only isMaximized/unmaximize; it never checks isMinimized or
  unminimizes. It can return 'restored' from a rectangle without verifying native
  visibility. The alternate JS route at :1186 also only unmaximizes. This explains
  the missing minimized-recovery behavior, not the particular timeout's complete
  causal chain. It also makes recovery depend on the WebView it is trying to recover.
- `src/mcp/runtime.ts:1287` captures the previous lastAction before invoking the
  operation; snapshot sets its name inside the operation (:710). Thus its invocation
  can be labelled diagnostics, as in native acceptance. Fix attribution before
  interpreting operation-labelled durations. Capture a stable explicit action and
  queue-start/dispatch/end/cancellation phase; do not infer active capture from status.
- Manager `src/session/manager.ts:121` configures 30000 ms HTTP requests.
  Windows provider `vendor/tauri-plugin-wdio-webdriver/src/platform/windows.rs:265`
  schedules CapturePreview through with_webview, then awaits a callback with the
  script timeout (:308). Public screenshot capture wraps causes as SCREENSHOT_FAILED
  (`src/observation/screenshot.ts:283`). The receipts cannot distinguish HTTP
  deadline, UI dispatch delay, COM failure, or missing/late callback.
- Provider ExecuteScript uses a per-window lock (:257, :615); acquisition precedes
  the callback timeout. Async evaluation holds the lock across wrapper evaluation
  and async callback waiting. Public cancellation does not itself prove browser JS
  or a COM callback stopped. Pending work/lock wait after timeout is a concrete
  investigation target; no cancellation propagation/late-callback trace proves it
  caused these incidents. Do not remove serialization or broadly extend timeouts.

Ranked measurement targets: (1) minimized recovery and pending native/WebView
operations, supported by minimized observations and restore code; (2) blocking WSL
IPC, below; (3) DOM/transcript cost under active work. Snapshot extraction reads
innerText before role filtering (browser-entry.ts:659), and shallow ancestors can
still aggregate a long subtree. Snapshot stages include execute_script, handle
materialization and title reads, so total duration is not a single JS duration.
Tinto TerminalPanel.tsx:727 recomputes agentTurns on timeline/output/session changes;
:8355 walks the timeline, while sessionStore.ts appends/copies and notifies consumers.
These are profiling candidates, not demonstrated 49-second hot spots. App-server
stdout is consumed on a dedicated Rust thread (app_server.rs:1005), and bus WSL
recalculation already uses spawn_blocking (bus/mod.rs:1265); neither fact proves
absence of lock contention or frontend work. No live profiler was attached in this
bounded turn. Preserve causal uncertainty rather than optimizing transcript by guess.

### WSL: independent bounded probes and blocking paths

All probes used Windows wsl.exe with an 8000 ms parent deadline, hidden windows,
and read-only commands against the already-running Ubuntu-24.04. No shutdown,
reset, install, distro edits or managed tinto-agent/AI launch was performed.
Compact receipts: `%TEMP%/tinto-remaining-gaps-wsl-*-20260907.json`.

- list --verbose: exit 0, 149 ms; Ubuntu-24.04 and docker-desktop Running, WSL2.
- Ubuntu /bin/sh basic execution: exit 0, 716 ms, user teb, Linux git found.
- bash -lc tools/path probe: exit 0, 1154 ms; cargo found, node/codex not found,
  npm points to /mnt/c/nvm4w/nodejs/npm. Existing managed 0.1.0 helper executable
  exists; ICook Git root resolves as /mnt/c/Users/User/Documents/personal/ICook,
  matching the historical registered path. Binary existence is not health proof.
- Crucially, the actual shell_env.rs profile/NVM resolver, executed read-only up
  to native binary resolution, exceeded 8000 ms (8021 ms, ETIMEDOUT, no output).
  An initial extraction used CRLF and failed shell syntax; it was discarded and
  corrected to LF, matching Rust string semantics. That first probe is a probe
  construction error, not a product bug. Parent timeout bounds the invoking WSL
  client; it does not prove cleanup of any descendants created by user profiles.

Conclusion: distro enumeration/basic execution are healthy at these instants;
Tinto's fuller shell environment path is slow in this probe. It sources four
profiles and nvm (wsl_agent/shell_env.rs:1), filters Windows-mounted executables,
then readiness launches the resolved binary with --version. Plain login results
do not prove Codex absent under that fuller resolver. Exact slow profile/NVM
statement and native Codex health remain external/environment blockers to attribute,
not authorization to reset WSL or install anything. Historical timeout/child_exit1
with absent stderr remains unattributed; do not replace those facts with this probe.

Tinto blocking paths are independently identifiable: AddRepoDialog.tsx:52 eagerly
lists distros even for local onboarding, then :77 requests WSL home listing.
workbench/commands.rs:242 is a synchronous Tauri command running unbounded
wsl.exe --list --quiet .output(); :269 synchronously requests the WSL helper.
Installed tauri-macros-2.6.3 wrapper.rs:50/:399 emits an inline blocking command,
not an offloaded task. This can hold the invoking UI/IPC path during external
work. Readiness commands (agent_console/commands.rs:967/:986) are async but call
blocking WSL exchange directly, occupying runtime workers. In launcher.rs:819,
the I/O mutex wait and stdin write occur before recv_timeout(:857), so the 30s
response timer is not a full request deadline. These code paths justify isolation
and deadline tests; no receipt ties a particular historic screenshot to them.

### Exact next implementation/test scope

1. Pumarejo runtime.ts + existing tests/unit/mcp-runtime.test.ts: stable action
   attribution and bounded queue/dispatch/cancel stage records. Assert a snapshot
   following diagnostics is labelled snapshot, and distinguish queued cancellation
   from active dispatch. Preserve the proven snapshot-only cancellation policy,
   close/shutdown races and ownership semantics; do not silently extend it to actions.
2. Pumarejo webdriver/client.ts, native-control.ts and the existing provider window
   handler/executor/router: implement native minimized-state detection/unminimize
   through the authenticated provider, independent of JS evaluation, with observable
   non-minimized/usable-rect postconditions. Update existing webdriver-client and
   provider-source tests; controlled Windows platform acceptance must cover externally
   minimized windows, failed restore, and truthful capability/failure results.
   Provider Windows capture/eval instrumentation should record native dispatch,
   lock wait, callback/late callback and deadline without logging transcript contents.
   Use those timings to select a later pending-operation fix; no speculative lock removal.
3. Tinto workbench/commands.rs + existing command/module tests: move enumeration
   and directory requests off UI dispatch, bound subprocess/queue/request lifetimes,
   preserve structured exit/timeout/stderr distinctions. agent_console/commands.rs
   readiness must offload blocking exchange. launcher.rs needs a total-deadline
   design covering mutex wait/write/response with isolated fixture tests for stalled
   startup, queue wait and child exit. Reuse existing launcher timeout/exit tests.
   Verify a stalled WSL request leaves local IPC responsive; frontend mocks alone
   cannot prove it. Preserve source/config/lock changes already present.
4. AddRepoDialog.tsx/operations.ts with workbench.test.tsx/operations.test.ts:
   defer WSL home browsing until the WSL path is chosen; show enumeration failure
   explicitly instead of silently substituting fallback names, keeping local add
   usable. No readiness-cache redesign or transcript rewrite absent measured need.
5. shell_env.rs/native availability: next read-only attribution should time each
   profile/NVM stage with redacted output and a total deadline; only then select
   a bounded environment-resolution change. Do not claim or install missing Codex
   based solely on the plain login probe. TerminalPanel/sessionStore/app_server
   changes are conditional on same-workload profiling, not yet a patch prescription.

### Safe self-host route and controlled continuity acceptance

No Rust/provider edits may run against this watch-enabled host. Current
Tinto .pumarejo.json has no --no-watch. Installed Tauri dev --help confirms it;
Pumarejo config schema/load accept ordinary launch args and preserve the single
{tauriConfig} placeholder. Controller must save active work and select a separate
safe acceptance launch by setting that config's launch.args to:

`["run","tauri","--","dev","--no-watch","--features","pumarejo","--config","{tauriConfig}"]`.

Preserve command npm, pathPrepend and other fields. This is the supported config
route, not a new CLI config flag. Verify actual process args and absence of Tauri
Rust watcher before implementing/building Rust; no-watch does not freeze Vite HMR.
Use a controller-approved separate runtime/output and frontend reload isolation;
do not replace this host executable, stage over its active provider, or disconnect
its owning MCP transport. Existing native acceptance used watch mode and cannot
satisfy this gate retroactively. Configuration was not changed in this turn.

Extend the existing Pum cancellation acceptance client, not a duplicate worker:
controller owns the disposable native session and submits one bounded read-only
Agent task (six deterministic package-file SHA-256 reads with numbered progress
messages across a short, e.g. 30-second interval, then a final expected hash/count).
Capture Tinto session ID, underlying app-server thread/turn IDs, Pum session ID,
app/provider PIDs plus OS creation times and owned descendants before starting.
Observe at least two real task progress events. While work remains active, issue
one public SDK snapshot with a 100 ms timeout, record wire cancellation and provider
active-dispatch receipt; cancellation before dispatch is inconclusive for that gate.
Require further progress and final result from the same Agent/turn without archive,
resume or relaunch; compare all identities and diagnostics, then fresh shallow UI.
A non-induced timeout is inconclusive/failure, never pass. Apply an overall bounded
watchdog (e.g. 120 seconds); retain failure receipts without retry loops. Only after
task completion should the controller explicitly close its disposable session,
require idle, and compare the saved creation-identified process/descendant inventory
for cleanup. PID-only absence is insufficient. Run long-history and minimized-window
variants separately; the basic task does not prove long-session maturity.

This turn ends diagnosis-only. Root/controller owns safe-route selection and later
implementation dispatch; no new Agent, app action, test/build/install, commit or
push was performed. Historical records and unresolved external attribution remain.

Diagnosis preservation check: 620/620 pre-existing dirty files outside the
three authorized documents retain their baseline hashes. Dirty counts remain
Pumarejo 5, Tinto 38, Windows ICook 580; no new repository artifacts.


## Controller-selected no-watch configuration — 2026-09-07

Configuration-only preparation completed in verified Pumarejo cwd/root
C:/Users/User/Documents/personal/pumarejo, adjacent Tinto root
C:/Users/User/Documents/personal/tinto. Effective instructions remain
danger-full-access, unrestricted filesystem, network enabled, approval never;
Node successfully read C:/Users/User (118 entries).

Changed only the existing tracked Tinto .pumarejo.json launch.args by inserting
--no-watch immediately after dev; all other values and formatting are preserved.

Previous command:

`npm run tauri -- dev --features pumarejo --config {tauriConfig}`

Selected command:

`npm run tauri -- dev --no-watch --features pumarejo --config {tauriConfig}`

Validation: installed Tauri dev --help lists --no-watch; the existing compiled
Pumarejo projectConfigSchema and loadProjectConfig accepted the actual edited file.
materializeLaunchProfile retained exactly one --no-watch and replaced the single
{tauriConfig} placeholder with an inside-project validation path. No generated
config was written and no launch occurred. runtime.ts:1391 loads this project
configuration when creating the runtime, so the next fresh Pum MCP process using
--project C:/Users/User/Documents/personal/tinto will consume the selected argv.
An already-running runtime is not reconfigured by this file edit.

SHA-256 before: 8ec711697bec2c64c02b9ace71934529497a64120ab01c33e360aafd282363f6
SHA-256 after: 782aa3eb3ff15e6cd1c9df433613f1dc2013f0b01e84ee1af8f0cf7c13bab7b7

Integrity implications: Tinto's tracked configuration is now intentionally dirty;
old acceptance build.json launchConfig records remain historical and unchanged.
No installer manifest, provider staging or build hash was rewritten; any prior
configuration fingerprint must be interpreted against the above deliberate change,
not silently refreshed. Rollback is removal of this single argv token.

Controller owns closing the idle session, starting the fresh driver/native host,
checking effective --no-watch arguments and absence of Rust Watching output, then
reopening the same history. No runtime no-watch claim is made before that check.
Vite HMR remains live: no frontend edits until safe reload handling is established.
No Rust/provider/frontend edits, restart, close, tests/build/install, agents, commit
or push occurred. All 622 pre-existing dirty files outside this note/config
were verified byte-identical after preparation.


## Focused gap implementation — 2026-09-07

The implementation and exact controller acceptance commands are recorded in [Pum cancellation evidence](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#focused-gap-implementation-and-handoff--2026-09-07). Earlier blocked/diagnosis facts remain historical.

Full access and the live OS chain were verified before edits: node 20124 with --no-watch -> cargo 25324 -> cargo 27248 -> Tinto 6544 (created 11:13:59 UTC); same processes remained at completion. No frontend edits, provider staging, host restart or close occurred. Configuration is unchanged from the approved no-watch selection; Vite remains live. Sky/controller submission and restore are fallback, not Pum capability proof.

Pum now preserves sessions on caller-cancelled actions as well as observations, records uncertain mutating outcomes without redispatch, and retains explicit-close cleanup. Current-operation attribution is fixed. Native restore unminimizes/unmaximizes through the authenticated provider boundary without WebView JS or automatic postrestore semantic refresh; callers request a fresh snapshot separately. Minimal queue/script-lock/screenshot callback timings support future stall attribution, not a retrospective cause claim.

Tinto changes are limited to src-tauri/src/windows_process.rs, src-tauri/src/workbench/commands.rs and src-tauri/src/agent_console/commands.rs: five-second owned enumeration subprocess deadline, bounded output/cleanup, and blocking WSL directory/readiness/binary work offloaded from async runtime threads. Pooled helper mutex/write waits and the intermittent resolver cause remain limited. New bounded resolver/helper probes passed in 1094/1534 ms and found Codex via NVM; this does not prove the native ICook task or explain earlier timeouts. No distro changes are justified by these results.

Validation: 242 TypeScript tests, 89 WSL Rust tests, and one bounded-subprocess Rust test passed; typecheck, Pum emitted/bundled build, CLI load, and canonical provider offline check/build passed. ESLint remains blocked by existing Ajv resolution mismatch (requires 6.x, resolves 8.x); no dependencies installed. Prior 83-test/build/native runtime acceptance is retained. Tinto tests compiled its library; live tinto.exe and staged provider are unchanged. Controller must review supported init --dry-run, stage/link after closing this host, start fresh no-watch Pum/Tinto, and run minimize/restore, fresh observations, uncertain ENTER cancellation, exact-ID active-Agent completion, WSL route and explicit-close/process-cleanup acceptance. Native fixes and long-session maturity are not yet proven.

Preserved all 619 pre-existing dirty files outside the five intentionally updated baseline paths; final dirty entries Pum 15, Tinto 42, ICook 580. Earlier cached dependency reconciliation remains intact. No commits/push/releases/installs/other Agents or app lifecycle operations.


## Fresh recovery staging — 2026-09-07

**STAGING_READY, controller apply/native acceptance pending.** See the
[canonical recovery handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#fresh-recovery-staging--2026-09-07)
for exact offline steps, source/acceptance scope and
[artifact hashes](../../../pumarejo/docs/evidence/2026-09-07-recovery-staging.json).

The deliberate --no-watch config has intact attributed fields but fails init's
full-file config hash gate. Cargo/Rust projections are intact. Three old staged
provider sources explain doctor registration drift; a pinned, offline-only helper
prepares actual source replacements and their real hashes without rewriting the
config fingerprint. Complete copied-fixture integration checks are ready.

Recorded archive 7f935ca3 has 10,208 events. The synchronous per-event journal replay
copied only 4,762 in 10 seconds before UI work; a two-file backend patch batches
history atomically off the async runtime and suppresses history-as-live UI replay.
All events persisted in 779 ms / 1,237 ms in isolated runs. 18 journal and 36 command
tests pass; patch apply-check passes. Both existing dirty Rust files and the live
frontend remain unchanged until controller apply after close. Library test builds
used the existing target cache; no running executable/DLL was replaced.

The resumed 4ee1d4a6 journal contains copied history plus lifecycle, not a new
follow-up. Provider 01a07b6c still last completed at 11:51:16.986Z; the follow-up never
reached it. This fixes a measured replay bottleneck, not a proven whole-UI blank/lock
cause. The 61,983 ms empty 022 observation remains separately unresolved.

The existing client now prepares public restore, harmless ENTER cancellation/no
client replay, exact-ID bounded task continuity/completion and PID+creation Windows
cleanup. Five parser and four cancellation-wire checks pass; native flow not run.
Full-access native confirmation, navigation and failed-resume sky submission remain
fallback, not Pum capability proof. WSL Linux identity/cleanup and active provider
cancellation-dispatch attribution need separate receipts. The fresh tools lacked
the owning Pum stdin handle, so no live UI action or host lifecycle was attempted.
Current host is public 012/Tinto PID 18500, created 12:47:15.7118480Z, with --no-watch.
No other Agents/install/release/project commit/push; test commits exist only in
disposable Git fixtures. The prior 242 TS + 90 Rust results remain historical valid
coverage; the staging receipt records this pass's preservation and hashes.


## Native recovery verified; WSL transport repair — 2026-09-07

The [canonical Pum handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#native-recovery-verified-focused-wsl-transport-repair--2026-09-07) records exact identities, hashes, test logs and controller commands. Earlier staging-pending statements are now historical: controller applied/rebuilt, and verified-native-checks.json proves public restore (2348 ms), screenshot (95 ms), uncertain ENTER cancellation without replay, and one public follow-up on the 10,208-event archive completing all six hashes in the same task after snapshot cancellation. Session 4065d799-7ef0-4d82-820d-4dde9a69e58a, task 01a07c12-f3b2-71f0-a2d0-af969a498a66, app 25952 and provider 19228 retain exact creation identities. No fallback was used for those native actions; journal/OS inspection is separate observation fallback. Large-transcript observations still take 9–26 seconds. Do not run --daily-use: its direct ShowWindow setup is not the required sky API route; controller used thin public SDK only.

Public click 034 dispatched, then snapshot 035 showed the ICook helper child_exit/exit1/stderr unavailable; no new WSL Agent was recorded. Correct managed-helper handshake and Codex availability probe passed in 1984 ms without AI dispatch or distro changes. Exact historical failing stage cannot be recovered from that generic error.

Two deterministic regression failures proved helper-pool races: late response reuse after timeout and stale cleanup evicting a replacement. Tinto src-tauri/src/wsl_agent/launcher.rs now retires failed streams under the I/O lock and removes only matching pool identities; src-tauri/src/agent_console/mod.rs labels startup binary_check/checkpoint_create/provider_spawn failures without changing categories or enabling retries. The two races are fixed; their involvement in 034 is not proven. Checkpoint mutations remain non-retryable. Mutex/pipe total deadlines and intermittent native route attribution remain limitations.

24 launcher tests and the broader 92 WSL tests passed, including no recorded/retried session on failed provider spawn. Rust test compilation passed; executable linking/native acceptance is controller-owned after closing this host. Full access, both roots and node 22780 --no-watch -> cargo 29652/28344 -> Tinto 25952 were verified before edits. No frontend edits, host restart/close, AI dispatch, distro reset/install, other Agents, commit/push/release. Controller must link after close, start fresh no-watch, verify no uncertain Linux task, attempt one public ICook start, capture exact stage on failure or exact session/Linux provider identity and task completion on success, then verify explicit-close/Linux cleanup. Windows-only cleanup is insufficient.

Preservation final: 637/637 pre-existing dirty files outside the three updated handoffs retain their baseline SHA256. Dirty entries: Pum 18, Tinto 44, ICook 580. Both changed Rust files were clean at this turn’s baseline. Source hashes above are final; existing replay, provider, frontend and cached-dependency work was preserved.


## WSL readiness/start route — 2026-09-07

See the [canonical route handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#wsl-readinessstart-route--2026-09-07) and the wslRoute member of the existing recovery-staging receipt for source hashes and exact offline patch/build/acceptance steps. Public close 041/status 042 and explicit-close-process-check.json passed (idle, no recorded Windows survivors); controller's Linux post-close probe found no Tinto helper/app-server. Unrelated 05:43 Codex/headroom processes are excluded and remain untouched.

New host 20200 and Tauri 28236 --no-watch were independently verified with unchanged creation identities/full-access roots. One start 006 dispatched, no new WSL journal session appeared, and screenshot011 shows generic failure even though ICook scan completed 580 files. Readiness only checks the binary; startup also creates a checkpoint and spawns the provider. RepoCard discarded all backend category/stage information, directly explaining missing stage labels. The exact rejected backend exception is not present in supplied receipts and was not reconstructed from timing; no retry was issued.

Reproduced and fixed the helper's unbounded admission/write waits (30 ms queue test took 506 ms; 40 ms write test took 4.03 s). Launcher now shares one exchange budget across queue/write/read and remaining safe retry budget; queue expiry does not retire the busy owner's helper, and dispatched uncertainty does not enable mutation replay. Offline frontend changes share pending readiness probes beyond the ten-second result TTL, protect replacements from stale rejection, and display safe category/stage without falsely asserting no session began. Three live frontend files remain unchanged; the controller applies the hash-checked patch only after close.

Validation passed: 94 WSL Rust tests (including both new deadline regressions), 64 offline UI tests, final focused UI checks, isolated typecheck and Vite build. Backend library tests compiled; live executable was not linked/restarted. Patch apply-check passed. Native ICook startup/task completion and Linux cleanup remain controller acceptance, using one public attempt only after resolving any uncertain state; preserve unrelated processes. Existing native recovery/continuity remains verified and large-transcript 9–26-second observations remain a limitation. No installs/distro changes/other Agents/direct UI API/commit/push/release.

Final preservation: 637/637 pre-existing dirty files outside the five authorized baseline paths retain their SHA256. Dirty entries Pum 19, Tinto 44, ICook 580. All three live frontend preimage hashes still match; backend and staged patch/source hashes match the receipt. Existing replay, pool race, provider, ICook and dependency work was preserved.


## Recovered click006 checkpoint exception — 2026-09-07

Controller's read-only native DevTools fallback recovered child_exit: WSL startup [checkpoint_create]: agente WSL retirado tras fallo de transporte; solicitud no enviada. No code was executed/start retried; tauri_type020 was STALE_ELEMENT_REF with no dispatch. This supersedes the prior unknown-exception limit and is observation fallback, not Pum capability proof. The start had reached checkpoint creation but rejected an already-retired helper before sending the checkpoint; provider spawn/session insertion was not reached. The earlier request responsible for retiring the helper is still unidentified.

The existing deadline/cache/display fixes did not recover this proven-not-sent mutation. A narrow launcher change now permits one fresh helper only for an internal retired-before-dispatch flag, within the original budget; error text never authorizes retry. Any possible mutation write/lost response remains non-retryable. Deterministic real-wrapper tests reproduced the exact failure, then verified one fresh dispatch and no replay after a helper read/lost its response. All 96 WSL tests passed. Full access/roots and unchanged Tauri28236 --no-watch/Tinto20200 creation identities were verified; host/frontend were not changed or restarted.

The [canonical handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#recovered-click006-checkpoint-exception--2026-09-07) and existing recovery-staging.json wslRoute member contain final source hash/evidence. The staged frontend patch/preimages remain unchanged. Controller must close after this turn, hash-check/apply that patch, build offline with --features pumarejo --bin tinto, then launch fresh no-watch and accept one public ICook start with exact Linux provider/task/cleanup identities. No install or Linux helper rebuild is needed. Native route success and observation maturity remain pending; unrelated Linux processes stay untouched.

Recovered-exception preservation check: 638/638 other pre-existing dirty files retain baseline SHA256. Dirty entries Pum 19, Tinto 44, ICook 580. Live frontend preimages, staged frontend sources and patch hashes match; final backend hash matches the receipt. No unrelated changes.


## Binary-check contention and corrected build cwd — 2026-09-07

The frontend patch is applied: all final hashes match, including agentAvailability.ts copied as exact staged LF bytes. Do not reapply. Previous public close024/status025 and Windows identity cleanup passed; controller found no surviving Tinto Linux helper, with old05:43 Codex/headroom untouched. Current no-watch Tauri18956/Tinto28168, actual roots/tools/full access and dirty baseline were independently verified.

New one-click native acceptance reached timeout; binary_check, without a new journal Agent/Linux Codex. Windows snapshots remain responsive2–4s. The binary request shared a serialized helper with scans. A deterministic held-scan-lock test reproduced queue timeout despite direct managed probes succeeding (Tinto cwd1445ms, available true). Only AgentBinaryAvailable now uses one owned, short-lived bounded helper exchange; it never joins/retires the scan pool. No new generic pools, increased timeout, cache or mutation retry. Original checkpoint no-replay/retired-before-dispatch rules remain. All97 WSL tests passed; live host/frontend were unchanged.

Corrected build instructions and hashes are in the [canonical handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#binary-check-contention-and-corrected-build-cwd--2026-09-07) and existing recovery-staging.json wslRoute. Controller's old Pum-root command selected GNU/dlltool; Tinto root selects the existing MSVC directory override and passed. This Agent's environment override currently also selects MSVC, and child checks without it verified the directory difference. After close, Set-Location C:/Users/User/Documents/personal/tinto, verify rustup show active-toolchain is MSVC, then cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --features pumarejo --bin tinto. No install or frontend/provider restaging is required.

Start fresh supported no-watch; verify readiness during scans, then one public ICook start after resolving uncertainty. Require exact session/Linux provider identity and bounded task completion, followed by owned cleanup excluding unrelated processes. Capture a remaining stage error without retry. The individual native timeout's queue-vs-execution substage is not visible in supplied UI evidence; the contention defect is independently reproduced. Native WSL route success and long-session maturity remain pending. No host lifecycle, AI/checkpoint mutation probe, other Agent, install/distro reset, commit/push/release occurred.

Binary-contention preservation: 641/641 other pre-existing dirty files retain baseline SHA256. Dirty entries Pum 19, Tinto 47, ICook 580. All applied frontend final hashes and updated backend source hash match the staging receipt. No unrelated changes.


## Complete startup helper-contention path — 2026-09-07

Native binary-acceptance receipt now proves checkpoint_create timed out in the queue before dispatch, after binary preflight progressed. One click005, no new Agent/no retry. The exact DevTools receipt is read-only Computer Use fallback, not Pum capability proof. Previous public016 close/017 idle and Windows identity cleanup passed. Current Tauri19348 --no-watch/Tinto26888 identities, roots/tools/full access were reverified; no lifecycle operation occurred here.

The fix covers the remaining start/status dependencies: AgentCheckpointCreate and AgentCheckpointScan now join binary availability on the existing owned-helper mechanism. Existing-session refresh can call both, so it is covered too. Each selected operation gets one bounded exchange; checkpoint mutation uncertainty never enables replay. Repository watcher state stays on the persistent helper. Codex app-server and PTY fallback directly spawn wsl.exe/bash with private stdio, and session insertion has no helper call; they cannot inherit the scan-pool lock. No generic new pool/cache/framework or timeout increase.

Both checkpoint held-lock tests reproduced the exact queue failure; afterward all99 WSL tests passed, including lost-response/no-replay and direct provider/PTY argv coverage. The exact provider shell script in ICook cwd with --version only passed1505ms/exit0; no app-server, Agent task or native checkpoint mutation was launched. Final source/probe/evidence hashes and complete path table are in the [canonical handoff](../../../pumarejo/docs/evidence/2026-09-07-cancellation-fix.md#complete-startup-helper-contention-path--2026-09-07) and existing recovery-staging.json wslRoute.startupContention.

Frontend remains applied; do not reapply. After controller close, build from C:/Users/User/Documents/personal/tinto with existing MSVC: cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --features pumarejo --bin tinto. No install/restaging is needed. Start fresh supported no-watch and accept ONE public ICook start during background scans, exact session/Linux provider identity and bounded task completion, then owned cleanup excluding unrelated05:43 processes. Native startup execution and other possible provider/filesystem failures still require acceptance; observation maturity is not claimed. All previous work retained, no host restart, other Agent, commit/push/release.

Startup-contention preservation: 641/641 other dirty files retain baseline SHA256. Dirty entries Pum 19, Tinto 47, ICook 580. Applied frontend hashes and final backend hash match the receipt; all unrelated work is preserved.


## Native WSL startup acceptance resolved — 2026-09-07

The latest startup-isolation fix passed native acceptance. This supersedes earlier WSL-route pending/blocker statements for this bounded path; earlier failures and their attribution remain historical evidence. Controller built from C:/Users/User/Documents/personal/tinto using the existing MSVC directory override: cargo build --offline --locked --manifest-path src-tauri/Cargo.toml --features pumarejo --bin tinto, PASS 44.86s. Backend SHA256 f6ad6f23e872906380ccf413434ca98776e7000a8e1b66dc0bd985df5b919b85; frontend was already applied and was not reapplied. Prior 99 WSL regression passes remain historical validation; no tests or build were repeated in this docs turn.

Public launch001/ready003 owned13520, Tinto8976, no-watch; readiness004 enabled ICook despite failed background scans. ONE click005 created Tinto session 4d2e4f9d-5009-44a1-a7a3-33231765bf7a and provider 01a07c74-7dc2-7631-9891-eaa29ab0b430. Linux Node214367 and Codex214847 share PGID214367, started17:20:00/02 local; proc stat and creation identities are retained in the receipt. Click dispatch alone was not used as proof of completion.

Public007 type/008 ENTER preflight completed at /mnt/c/Users/User/Documents/personal/ICook, Ubuntu24.04 WSL2, with git/node/npm/cargo,580 dirty files and package.json SHA256 40aa54478e354d760b571db804df0c22bd841533f6dc02bafbdb944647a91675. Effective WSL scope was workspace-write, restricted network, approval never; this is NOT WSL full-access acceptance. Public010 type/011 ENTER started exact task01a07c77-85d0-7641-adcd-d9584f39979c at15:23:23.449Z and completed15:24:11.638Z, with three matching reads and five-second gaps. Active public snapshot012 took418ms (Trabajando); screenshot013 took1890ms and visibly confirms IC_WSL_ACCEPT_DONE count=3, idle. All native actions were PUBLIC Pum; read-only journal/OS verification is separate observation fallback.

Public014 close/015 idle passed recorded Windows identity and Linux provider PID/group/helper cleanup, with no owned survivors. This is bounded close-time evidence, not a global process absence or future PID-reuse claim. Unrelated Linux workloads remain untouched; their large command lines were not copied. Controller launch016 was solely for this docs turn (Pum3446 owns lifecycle); current Tauri25576 --no-watch/Tinto11492 were independently observed. Actual Pum/Tinto/ICook roots and Windows tools were checked; this docs Agent is danger-full-access, network enabled, approval never, distinct from the accepted WSL scope. No lifecycle action occurred here.

Remaining limits: background repository scan timeouts, long-history action latency, and untested WSL full access. No full daily-use maturity claim. Original Windows ICook app readiness remains separate. The 21-case ICook campaign was NOT rerun: retain17 pass +1 lifecycle pass with limits +3 blocked (2 fixture/contract,1 Pumarejo keyboard); no release or gate promotion.

Durable [native success receipt](evidence/2026-09-07-wsl-startup-native-success.json) (SHA256 bb6c6552ee60fb203af8bf1b4346a1a8b20d744a6e951092067959dcf3a3f594) embeds task proof, sanitized identities/cleanup and raw-file hashes; [final public screenshot](evidence/2026-09-07-wsl-startup-native-final.png). Raw source: %TEMP%/tinto-wsl-startup-acceptance-20260907. Earlier blocked history is retained as resolved for WSL startup only.

Final docs preservation: 637/637 other pre-existing dirty files retain their baseline SHA256; no unexpected changes. Dirty totals Pum19/Tinto49/ICook580 include authorized evidence additions. Source, cached dependency reconciliation and staged frontend bytes remain preserved. No source edits, retests, builds, installs, dispatch, host lifecycle, other Agents, commits/push/releases in this turn.
