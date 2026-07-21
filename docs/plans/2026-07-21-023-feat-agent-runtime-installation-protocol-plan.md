---
title: Agent Runtime Installation Protocol - Plan
type: feat
date: 2026-07-21
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: legacy-requirements
origin: docs/brainstorms/2026-07-20-023-agent-runtime-installation-protocol.md
execution: code
roadmap_item: RDM-023
---

# Agent Runtime Installation Protocol - Plan

## Goal Capsule

Add one consent-gated path from an unavailable supported agent to a verified global installation in the repository's exact host or WSL runtime, then start the original request exactly once. Preserve the existing launch path when the provider is already available and fail closed whenever the recipe, prerequisite, authorization, process cleanup, or verification is uncertain.

Authority order: the reviewed product contract governs behavior; immutable recipe metadata governs executable and arguments; the resolved repository source governs every probe/install/verify/start step. Stop instead of guessing if official provider guidance, target-runtime identity, or process ownership cannot be proven.

Execution profile: high-risk, serial implementation on `develop`, no worktree, no real global installation in automated tests. Compound Master owns artifact/review gates; `ce-work` may implement but must not ship; Release Marshal owns any later commit/PR/Jira handoff.

## Product Contract

### Summary

Tinto will turn the existing unavailable-provider state into an explicit installation offer for `claude`, `codex`, `kimi`, and `opencode`. The user sees the exact provider, runtime, source, executable, arguments, global effect, recipe revision, prerequisite and privilege expectation before confirming. A successful installer exit is followed by a fresh same-runtime provider probe; only that verified result may consume the pending request and enter the existing session launch path.

### Requirements

- **R1:** Existing usable providers follow the current launch path without installation UI or process changes.
- **R2:** An absent provider creates no session and may resolve only an application-owned recipe for the exact provider and target runtime.
- **R3:** The confirmation binds provider, repository, runtime identity, recipe revision, executable, argv, source, global effect and privilege expectation to one opaque attempt.
- **R4:** Repository text, provider output and caller-supplied values cannot alter recipe selection, executable, argv, environment allowlist, runtime or privilege behavior.
- **R5:** Decline, cancellation, timeout, spawn failure, non-zero exit, cleanup failure and verification failure never launch an agent and return distinct safe outcomes.
- **R6:** Installer runtime, captured output, emitted progress, cancellation and descendant cleanup are bounded; secrets and raw unrestricted output are not persisted.
- **R7:** Installer success invalidates readiness state and performs executable resolution plus a bounded minimal version probe in the same runtime.
- **R8:** Verification consumes and starts the original pending launch at most once; stale, changed, repeated or concurrently confirmed attempts are rejected.
- **R9:** Local repositories never install in WSL and WSL repositories never install on Windows or another distribution.
- **R10:** Unsupported recipes or missing prerequisites return manual guidance and execute nothing.
- **R11:** Authorization is attempt-specific and is invalidated by any recipe or runtime identity change.
- **R12:** Authentication, provider terms, upgrades, downgrade, uninstall, repair and prerequisite installation remain outside the protocol.
- **R13:** Confirmation, progress, cancellation, failure, retry and success are keyboard-operable and announced through existing accessible status patterns.
- **R14:** Tests use injected recipes, fake processes, fake runtime probes and fake elevation only; CI never installs or changes a real global package.
- **R15:** This first slice persists no installation audit or installer output. Bounded in-memory state and the terminal UI result may contain only provider, runtime identity, recipe revision, timestamps, outcome and verified version; never credentials, environment values or raw unrestricted output.
- **R16:** No shipped recipe in this first slice requests elevation. A permission failure returns manual guidance; the protocol types reserve a privilege expectation so a future elevated recipe cannot bypass a new reviewed plan and authorization display.

### Acceptance Examples

- **AE1:** Given `codex` is already available in a local repo, selecting Start calls the existing session command and never prepares an install attempt.
- **AE2:** Given `kimi` is absent in `Ubuntu-24.04` and `npm` satisfies the recipe prerequisite, the dialog shows that distro and fixed `npm install -g @moonshot-ai/kimi-code`; confirmation runs only there, verifies `kimi --version`, and starts once.
- **AE3:** Given the user declines or closes confirmation, no installer or session starts and the launcher remains usable.
- **AE4:** Given installer exit zero but the fresh provider probe fails, the result is `verification_failed` and the attempt cannot be replayed.
- **AE5:** Given two confirmations race for one attempt, exactly one may execute; the other receives a stale/consumed result and cannot start a second session.
- **AE6:** Given a WSL recipe, every prerequisite, install and verification process addresses the registered distro and never falls back to host execution.
- **AE7:** Given repository/provider text resembling a command or environment assignment, the executed argv remains byte-for-byte equal to the compiled recipe.
- **AE8:** Given `npm` is absent, too old for the selected provider, or its global prefix is not writable, Tinto reports `missing_prerequisite` or safe manual guidance and does not elevate or install.

### Scope Boundaries

In scope: the four allowlisted providers, local Windows/Linux and existing Windows-to-WSL repositories, immutable npm recipes, attempt-bound consent, bounded progress/cancel, same-runtime verification, readiness invalidation, exactly-once continuation, accessible launcher UI and fake-only automated evidence.

Out of scope: remote-script installers, shell pipelines, package-manager discovery beyond the selected npm recipe, installing Node/npm/Git, native elevation, credentials/login, upgrades, uninstall/repair, arbitrary providers, mirrors, background installation, automatic retry and macOS-specific behavior.

## Planning Contract

### Key Technical Decisions

- **KTD1 — One compiled npm recipe family with a shell-free launcher:** Ship fixed package argv for `@anthropic-ai/claude-code`, `@openai/codex`, `@moonshot-ai/kimi-code`, and `opencode-ai@latest`. On Linux/WSL, execute the resolved npm launcher directly with discrete argv. On Windows, do not execute `npm.cmd` or `npm.ps1`: resolve the associated trusted `node.exe` and existing npm `npm-cli.js`, then execute `node.exe <npm-cli.js> install -g <compiled-package>` with discrete argv. If that association cannot be proven, mark the prerequisite unsupported. Never use `cmd.exe`, `sh -c`, `bash -c`, PowerShell expression evaluation, pipes or downloaded scripts.
- **KTD2 — Prerequisites fail closed:** Probe `npm` and provider-specific minimum Node compatibility in the exact target runtime before offering confirmation. Tinto does not install prerequisites. Use the strictest documented bound only where the provider requires it; do not invent a universal version requirement.
- **KTD3 — Backend-owned attempt ledger:** `prepare_agent_install` resolves repo/runtime/readiness and stores a short-lived opaque attempt containing the immutable recipe identity and pending launch. `confirm_agent_install` atomically claims it, runs install and verification, then enters the same internal start function once. The frontend never reconstructs recipes or independently replays Start.
- **KTD4 — No shipped elevation:** The official npm path is treated as an unprivileged per-user/global-prefix operation. Anthropic explicitly forbids `sudo npm install -g`; permission failures do not trigger `sudo`, `runas`, or a revised command. Privilege metadata remains visible and typed as `none` for every shipped recipe.
- **KTD5 — One process abstraction for host and WSL:** Reuse existing source resolution and WSL invocation boundaries, but introduce an injectable installer runner with explicit executable/argv/env, timeout, output caps and owned cleanup. Do not reuse provider PTY/ACP session machinery or create a session before verification.
- **KTD6 — Structured outcomes, no new persistence:** Add additive bus types for preview, progress snapshot and terminal outcome categories. Keep bounded attempt/output state in memory only and expose sanitized excerpts needed for user action. Do not add an audit file, config field, journal row or session record in this slice.
- **KTD7 — Cache invalidation is explicit:** Add a targeted `invalidateAgentAvailability(environmentKey, agentType)` frontend helper and always force the authoritative backend re-probe after installation. TTL expiry alone is not proof.

### Official Recipe Matrix

| Provider | Fixed npm argv | Shipped targets | Prerequisite | Verification | Privilege |
|---|---|---|---|---|---|
| Claude Code | `npm install -g @anthropic-ai/claude-code` | Local Windows/Linux; WSL Linux | `npm`; Node compatibility from current official docs | resolve `claude`; `claude --version` | none |
| Codex | `npm install -g @openai/codex` | Local Windows/Linux; WSL Linux | `npm` | resolve `codex`; `codex --version` | none |
| Kimi Code | `npm install -g @moonshot-ai/kimi-code` | Local Windows/Linux; WSL Linux | `npm`; Node `>=22.19.0` | resolve `kimi`; `kimi --version` | none |
| OpenCode | `npm install -g opencode-ai@latest` | Local Windows/Linux; WSL Linux | `npm` | resolve `opencode`; `opencode --version` | none |

The implementer must re-check these official sources immediately before encoding recipe revisions. On Windows the trusted launcher is `node.exe` plus its associated existing `npm-cli.js`; the displayed command remains the semantic npm command and the confirmation also identifies the resolved launcher. If a source no longer documents the package argv, or the runtime cannot prove a shell-free launcher, omit that combination as unsupported rather than substitute an installer.

### High-Level Technical Design

```mermaid
sequenceDiagram
  participant UI as RepoAgentLauncher
  participant API as Tauri commands
  participant Ledger as InstallAttemptRegistry
  participant Run as Host/WSL installer runner
  participant Start as Existing session start
  UI->>API: prepare_agent_install(repo, provider)
  API->>API: resolve repo/runtime + fresh readiness + prerequisite
  API->>Ledger: store immutable short-lived attempt
  API-->>UI: exact preview + attempt_id
  UI->>API: confirm_agent_install(attempt_id)
  API->>Ledger: atomic claim
  API->>Run: fixed executable + argv in target runtime
  Run-->>API: bounded terminal result
  API->>API: fresh resolve + version probe
  alt verified
    API->>Start: existing internal start(repo, provider)
    API-->>UI: verified_and_started(session_id)
  else any failure
    API-->>UI: structured failure; no session
  end
```

### Security and Reliability Constraints

- Attempt IDs are unguessable, short-lived, single-use and correlated to repo, resolved source/distro, provider and recipe revision. Never accept executable, argv, environment or runtime fields from confirmation.
- The runner starts a direct executable with discrete arguments and a minimal environment derived from existing safe runtime helpers. No shell command strings, remote script evaluation or inherited secret dump.
- Bound wall time, stdout/stderr bytes, progress count, concurrent attempts and tombstones. Cancellation invalidates the attempt and owns descendant cleanup before returning.
- Re-resolve the repo immediately before execution; runtime drift invalidates authorization. Recheck recipe revision at claim time.
- Do not persist installation metadata or output. Sanitize package-manager errors before UI projection.

### Dependencies and Sequencing

`U1 -> U2 -> U3 -> U4`. U1 establishes the additive contract and recipe/preflight core. U2 adds the backend attempt lifecycle and exactly-once integration. U3 exposes the user flow. U4 closes cross-surface verification, security evidence and documentation. Keep contract generation with each unit that changes Rust contract types.

## Implementation Units

### U1. Recipe catalog, preflight and additive contract

**Goal:** Represent official recipes and safe outcomes without executing installers.

**Requirements:** R2-R4, R9-R12, R16; AE2, AE6, AE8.

**Files:** `src-tauri/src/agent_console/install.rs` (new), `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/bus/client.ts`, `src/bus/contract.test.ts`.

**Approach:** Add closed provider/runtime recipe data, stable revision digests, prerequisite probes and serializable preview/outcome types. Resolve repository source through the existing registered-repo path. Generate the TypeScript mirror; do not hand-diverge it.

**Test scenarios:** all four recipes map to exact fixed package argv; POSIX resolves a direct npm launcher; Windows resolves only a paired `node.exe` plus `npm-cli.js` and rejects `.cmd`/`.ps1` execution; unknown providers/runtime combinations fail unsupported; local and WSL identities remain distinct; missing/old prerequisites fail without an execution request; serialized contract pins all outcome categories and privilege `none`; hostile caller text cannot enter recipe fields.

**Verification:** focused Rust install/catalog tests; `npm run contract:generate`; `npm run contract:check`; `npm test -- src/bus/contract.test.ts --run`.

### U2. Single-use backend installation and launch lifecycle

**Goal:** Execute a confirmed attempt safely, verify it and enter the existing launch path at most once.

**Requirements:** R1-R12, R14-R16; AE1, AE3-AE8.

**Files:** `src-tauri/src/agent_console/install.rs`, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/wsl_agent/shell_env.rs` only if a new argv-safe WSL primitive is required, and colocated Rust tests.

**Approach:** Register `AgentInstallAttemptRegistry`; split current start logic into an internal function reused by `start_agent_session` and confirmed install. Add prepare/confirm/cancel commands. Atomically claim attempts, bound concurrency/output/time, inject runners/probes in tests, verify same-runtime binary/version, then call start once. Never route npm permission failures into elevation.

**Test scenarios:** installed fast path is unchanged; decline/cancel never spawns; fixed host and exact-distro WSL argv; timeout/non-zero/spawn/cleanup/verification categories; concurrent and repeated confirmation; expiry/runtime drift/recipe drift; cancellation cleanup; output/secret canaries are truncated/redacted; verified flow produces one session and one launch event.

**Verification:** focused `cargo test` filters for install and command lifecycle; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`.

### U3. Accessible launcher confirmation and progress flow

**Goal:** Let the user understand, authorize, observe, cancel and retry installation without allowing frontend state to widen authority.

**Requirements:** R1-R5, R7-R13; AE1-AE6, AE8.

**Files:** `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/agentAvailability.ts`, `src/panels/DashboardPanel.test.tsx`, `src/bus/client.ts`, `src/App.css`.

**Approach:** Replace the disabled-only absent state with an Install action when preview is supported. Render one focused dialog containing exact recipe/runtime/effects and explicit Confirm/Cancel. Drive progress and terminal messages from structured backend state; on success invalidate only the provider/runtime cache and accept the backend-started session result. Keep recheck/manual guidance for unsupported prerequisites.

**Test scenarios:** no dialog for available providers; exact preview for local and WSL; keyboard focus/escape/confirm behavior; accessible live status; decline and close execute nothing; double-click/late response cannot confirm twice; all failure categories preserve retry/manual paths; provider/runtime changes invalidate an open preview; success clears stale availability and does not call the old launch callback again.

**Verification:** `npm test -- src/panels/RepoCard.test.tsx src/panels/DashboardPanel.test.tsx src/bus/contract.test.ts --run`; `npm run lint`; `npm run build`.

### U4. Cross-surface security evidence and operator documentation

**Goal:** Prove the protocol cannot install real software in automation and document safe manual validation.

**Requirements:** R4-R16; AE4-AE8.

**Files:** `docs/contracts/bus-contract.md`, `docs/manual-smoke/2026-07-21-agent-runtime-installation-protocol.md` (new), affected Rust/TypeScript tests, and RDM-023 orchestration artifacts.

**Approach:** Add fake host/WSL installer fixtures and canaries; exercise limits, source isolation, attempt correlation and exact argv. Document a manual smoke that defaults to preview/cancel and requires separate explicit consent before any real install. Record official recipe source URLs and known unsupported prerequisite/elevation cases.

**Test scenarios:** full suites contain no real package-manager mutation; fake runner proves argv equality and distro targeting; raw output/credentials never reach UI or persistence; automation fails if a test selects the production runner; bus docs match generated types; manual smoke covers preview, decline, fake success/failure and optional separately-authorized real validation.

**Verification:** full Verification Contract below plus `git diff --check` and the Compound Master work-package checker after package derivation.

## Verification Contract

- Contract: `npm run contract:generate`, then `npm run contract:check` with no diff.
- Frontend: focused U3 tests, then `npm run format:check`, `npm run lint`, exact `npm test`, and `npm run build`.
- Rust: `cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml`, and `cargo build --manifest-path src-tauri/Cargo.toml`.
- Security: search the diff for shell interpolation, `cmd.exe`, `sudo`, `runas`, remote install URLs, unrestricted environment inheritance, persisted stdout/stderr and recipes accepted from IPC. Prove every production runner call originates from compiled recipe data and that Windows invokes only a validated `node.exe`/`npm-cli.js` pair.
- Cross-platform: fake local Windows, local Linux and named WSL cases run in automation. Real global installation is never a CI gate and requires a separate user-approved manual action.
- Hygiene: `git diff --check`; generated contract and `docs/contracts/bus-contract.md` remain aligned; no unrelated refactor or new dependency unless a demonstrated blocker is escalated.

## Definition of Done

- Every R1-R16 requirement and AE1-AE8 example maps to passing automated evidence or an explicitly authorized manual-only check.
- Available providers retain their current launch behavior; unavailable providers cannot create a session before verified installation.
- All shipped recipes match re-verified official documentation, use shell-free npm execution (including validated `node.exe` plus `npm-cli.js` on Windows), declare privilege `none`, and fail closed on missing prerequisites or permissions.
- Attempt correlation, single-use claim, cancellation, time/output bounds, cleanup, same-runtime verification and exactly-once launch have regression tests.
- Confirmation and status UI pass keyboard and accessible-name/live-region tests.
- Full frontend/Rust/contract/build gates pass; code and mandatory Security Sentinel review have no open P0-P2 findings.
- Documentation names limitations and never implies Tinto installs credentials, prerequisites, upgrades or unsupported recipes.
- Abandoned experiments, debug output, real-install test hooks and unrelated changes are absent from the final diff.

## Sources

- Product contract: `docs/brainstorms/2026-07-20-023-agent-runtime-installation-protocol.md`.
- Existing runtime contract: `docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md` and `docs/contracts/bus-contract.md`.
- Anthropic official installation docs: `https://code.claude.com/docs/en/getting-started`.
- OpenAI official Codex repository installation: `https://github.com/openai/codex/blob/main/README.md`.
- Kimi official CLI guide: `https://www.kimi.com/code/docs/kimi-code-cli/guides/getting-started`.
- OpenCode official repository installation: `https://github.com/anomalyco/opencode`.
