# Native hardening reconciliation — 2026-09-07

Local work on `fix/reconcile-native-hardening`, starting at `67560d3`.
This record supersedes historical readiness claims only where new evidence is
listed below. No product commit, push, PR or release was performed.

## Repairs

- Restored typed local-path onboarding through the existing canonical `add_repo`
  backend. Enter submits the local path, concurrent submissions are guarded,
  and a configuration refresh failure does not invite a duplicate persisted add.
- Persisted journal exit codes on insert, update and reopen, retaining unknown
  codes as null. Archived activity displays known and unknown outcomes explicitly.
- Removed the synchronous availability-state update from the quick-launch effect,
  preserving invalidation when configuration or visible launches change.
- Reconciled the WSL source-policy test with the intentional availability UI and
  tested that unavailable WSL launches stay disabled.
- Restored Cargo's existing manifest-attributed Pumarejo provider registration.
  Doctor reports ready and integration dry-run reports no required changes.
  This is a local integration: Cargo references the ignored staged provider under
  `.pumarejo/provider`. A fresh checkout needs supported provider staging before
  treating these Cargo changes as portable release configuration.

## Verification

| Check | Result |
| --- | --- |
| Complete frontend suite | 59 files, 783 tests passed |
| Lint, TypeScript, production build, contract check | Passed |
| Complete Rust library suite, serial | 479 tests passed |
| Rust journal after final schema adjustment | 16 tests passed |
| Final focused frontend verification | 150 tests passed |
| Default parallel Rust suite | 477 passed; two ACP readiness/timeouts failed, then passed individually and in the full serial run |
| Pumarejo doctor | Ready; existing custody leases verified closed |
| Unchanged Gitleaks directory scan, eight-second timeout | Exit 0, complete parseable report, two findings |

The scanner used the application's existing flags, including `--exit-code 0`.
Successful completion does **not** mean no findings. Only redacted rule/file/line
metadata was inspected: `stripe-access-token` in `src/demo/main.tsx:23` and
`generic-api-key` in `src/panels/timeline/TimelinePanel.test.tsx:95`.
The historical nested worktree copy was absent; no cleanup or scanner weakening
was needed. Finding triage and in-app scanner presentation remain outstanding.

## Native observations

Used the public Pumarejo MCP tools through the installed SDK/CLI transport.
Computer use was limited to window observation and one successful activation.
An attempted later activation was refused after user input was detected; it was
not retried. No computer-use typing or clicking was needed.

- Typed-path onboarding registered a disposable local Git fixture at
  `%TEMP%/tinto-native-acceptance-20260907` and opened its canonical project tab.
- One Codex turn ran the read-only `(Get-Location).Path` command and returned
  `TINTO_NATIVE_OK` with that exact fixture directory. The UI showed one command,
  zero changes and an idle agent after completion. This proves local fixture
  scope, not ICook/WSL execution or long-transcript performance.
- Minimizing then calling Pumarejo restore reproduced the outstanding defect:
  the tool claimed restored but returned `x=-32000, y=-32000, width=160, height=28`
  and `no_observable_change`.
- Pumarejo closed that session successfully with state `idle`.
- After relaunch, the fixture's canonical registration remained present. The
  dashboard still showed WSL timeout errors for ICook and digital-product-passport;
  initial launch buttons were disabled, while a later snapshot showed them enabled
  alongside the errors. This inconsistent readiness presentation needs follow-up.
- A later snapshot (`030`) attributed ICook's failure to WSL child exit code 1;
  stderr was unavailable. No ICook command was submitted in this pass.
- Fixture removal was attempted through its workbench remove button, but the
  registration remained and Pumarejo detected no pending dialog. The fixture and
  QA conversation remain as local evidence; cleanup through the native confirmation
  route is still needed. No fixture files were deleted.

Raw local evidence is in `%TEMP%/tinto-native-20260907`, especially snapshots
`010`, `019`, restore `021`, close `022`, and restart snapshots `026`/`028`. Test and scanner logs are under
`%TEMP%/tinto-reconcile-*`. These temporary files are supporting local evidence,
not release artifacts.

## Remaining work

1. Repair Pumarejo minimized-window restoration and verify native postconditions.
2. Complete ICook/WSL execution readiness with attributable provider diagnostics,
   correct workspace scope and usable tools.
3. Verify folder-picker select/cancel and parent recovery, persisted ICook identity,
   archived conversation recovery, detached windows and representative long output.
4. Complete one-child interruption with sibling continuity and high-context testing.
5. Verify native scanner results and triage its two findings.

Existing broader roadmap initiatives remain outside this local hardening pass.
The scoped simplification review found no reuse defect. Its schema consistency,
callback naming and test typing suggestions were applied. A proposed availability
cache redesign was omitted because it would change invalidation semantics without
a demonstrated need.

## Snapshot cancellation follow-up — historical blocked attempt

The adjacent Pumarejo baseline is clean on `main` at
`41db8368e48b11b8fb79b2d980a93e42f4e1dc35`. The reproduced caller-cancellation
policy can close its owned application/session; attribution of the historical
ICook interruptions remains conditional on missing lifecycle receipts.

The user authorized a narrow TypeScript cancellation patch, regression tests and
Pumarejo build. Its source/build paths are separate from Tinto's staged provider,
so that scope does not require a Tinto watcher restart. However, this Agent's
managed writable scope excludes the adjacent Pumarejo checkout, and escalation
is disabled. No Pumarejo source edits, fix tests or build were attempted; Tinto
remains open. This is not a completed fix or native continuity acceptance.

The existing [bounded diagnosis](2026-09-07-daily-use-bounded-diagnosis.md) now
records the exact execution blocker, preservation baseline and public MCP timeout
acceptance recipe. Baseline hashes are retained in
`%TEMP%/tinto-cancellation-fix-baseline-20260907.json`. Only these two existing
orchestration documents were extended during this attempt.

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
The [latest bounded diagnosis](2026-09-07-daily-use-bounded-diagnosis.md#latest-old-driver-receipts-and-preservation)
records old-driver timeout/recovery receipts, SHA-256 hashes and preservation
checks. Reconciliation Agent `c006f94c` is archived/stopped; its existing document
updates were retained. Recovery snapshots do not prove uninterrupted native work.
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


## Remaining daily-use gaps — diagnosis update, 2026-09-07

See [bounded diagnosis](2026-09-07-daily-use-bounded-diagnosis.md#remaining-daily-use-gaps--bounded-diagnosis-2026-09-07)
for receipts, exact patch/test scope, no-watch launch args and controlled Agent
continuity acceptance. Prior cancellation 83 tests/build/native runtime pass stands.
Confirmed implementation gaps: restore checks maximization but never unminimizes;
runtime diagnostics capture the previous operation name; synchronous WSL enumeration
and browsing block IPC, and readiness/queue deadlines do not bound all blocking work.
Their contribution to each 30-second observation failure remains unproven.

Fresh controller evidence: sky twice saw MINIMIZED; public restore and screenshots
failed at 30s; sky activation restored the window, but a subsequent snapshot still
timed out at 65s. Raw 014 status is ready, PID 20724; controller reports the same
active Agent continues without archive/resume. This is positive active-continuity
observation, not the full task-completion/identity/cleanup acceptance. Sky is fallback.

WSL list/basic exec passed in 149/716 ms; ICook registered Linux path resolves.
Plain login lacks node/codex but the actual profile/NVM resolver exceeds the bounded
8-second probe, so native tool availability and the specific slow environment stage
remain unresolved. No distro reset/install is justified. Profile/COM/lock/DOM stage
attribution precedes optimization. Current watch-enabled host must be safely isolated
by the controller using supported --no-watch config before Rust/provider edits;
frontend Vite remains live. No source edits or native control occurred this turn.

Diagnosis preservation check: 620/620 pre-existing dirty files outside the
three authorized documents retain their baseline hashes. Dirty counts remain
Pumarejo 5, Tinto 38, Windows ICook 580; no new repository artifacts.


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
