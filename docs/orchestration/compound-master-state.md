---
title: Compound Master State - Tinto
status: completed
date: 2026-07-18
initiative: kimi-opencode-agent-support
mode: execute
production_posture: prototype
state_format: compact
last_reconciled: 2026-07-19
verification_status: passed
archive_snapshot: docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md
---

# Compound Master State - Tinto

## Resume Snapshot

- Initiative: add Kimi Code and OpenCode support through one bounded ACP v1 runtime while preserving PTY compatibility and every existing agent path.
- Phase: U1-U7 and both internal review units are implemented. The complete functional contract R1-R23 and AE1-AE11 is satisfied, with only the R18/AE8 platform limitations explicitly permitted by the plan.
- Strict closeout: `completed`. The functional ledger and every exact repository-wide Verification Gate pass.
- Delivery boundary: local implementation, validation and documentation only. No branch, commit, push, pull request, Jira mutation, deployment or release was requested or performed.
- User decision superseding the KTD10 port clause on 2026-07-19: requesting `--port 0` is sufficient for OpenCode ACP eligibility; the observed effective port `4096` is accepted while loopback, authentication, no-mDNS, process ownership and cleanup controls remain required.

## Operating Posture

- Production posture: `prototype`; existing Codex, Claude, generic PTY, journal, checkpoint and WSL behavior remains a hard compatibility boundary.
- Autonomy: guarded local execution. External mutation classes: none.
- Worktree policy: avoid. The implementation remains in the user's existing checkout.
- Review threshold: P0-P2. Final code and security reviews report no open findings at that threshold.
- Scope discipline: no Claude ACP adapter, RDM-018 memory, Tinto HTTP transport, credential ownership, auto-approval, ANSI semantic parser, checkpoint redesign or unrelated cleanup.

## Artifact Set

- Roadmap: `docs/roadmaps/2026-07-18-009-kimi-opencode-agent-support-roadmap.md` (`completed`).
- Unified product contract and implementation plan: `docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md`.
- Work package: `docs/work-packages/RDM-022-kimi-opencode-agent-support/2026-07-18-022-kimi-opencode-acp-work-package.md` (`completed`).
- Bus contract: `docs/contracts/bus-contract.md`.
- Platform/probe evidence: `docs/manual-smoke/2026-07-18-kimi-opencode-agent-support.md`.
- Final handoff summary: `docs/orchestration/2026-07-18-kimi-opencode-agent-support-compound-master-summary.md`.

## Implemented Units And Review Units

- U1: dual provider probe, Kimi allowlist/selectors and source-aware local/WSL readiness.
- U2: shared typed ACP v1 schema/core with bounded framing, queues, requests, updates, text, stderr and writer deadlines.
- U3: Kimi lifecycle, pre-session PTY fallback, post-session fail-without-replay, auth retry, serialized ACP retry, load/context bridge and bounded cleanup.
- U4: backend-authoritative permissions with exact correlation, first-winner tombstones and deny-safe expiry/disconnect behavior.
- U5: usable six-state Kimi UI with accessible permissions and negotiated image/model/mode controls only.
- U7: OpenCode parity through the same supervisor and wire core; current 1.18.3 attempts ACP despite materializing `--port 0` as `4096`, with retryable PTY reserved for real pre-session failures.
- U6: conformance, regression, security and six-cell platform evidence. Functional evidence and global verification are complete.
- RU1: shared core plus the complete Kimi vertical, reviewed with no open P0-P2.
- RU2: OpenCode descriptor/parity and final evidence, reviewed with no open P0-P2.
- Downstream-fix trace: none; every task finding was resolved inside this package.

## Impact Scan

- Runtime: additive `agent_console::acp` core/supervisor, provider descriptors, stable output reader, PTY compatibility process and Windows kill-on-close process job.
- Public contract: additive source-aware readiness, six ACP states, negotiated configuration/images, bounded live permissions, retry/config/permission commands and provider-neutral stop/input semantics.
- Persistence/resume: existing journal/checkpoint/timeline/goal paths are reused; live ACP permissions and transient runtime state are not reconstructed from archives. Native load or ContextBridge is chosen without replay.
- Frontend consumers: generated and curated bus contracts/client, readiness cache, Repo cards, console dock, TerminalPanel, detached window/workspace fixtures and affected tests.
- Dependency: the official schema-only `agent-client-protocol-schema = 1.4.0`; no ACP runtime framework or other new dependency.
- Security boundaries: provider stdout/stderr are untrusted and bounded; reverse file/terminal capabilities are not advertised; child environments are allowlisted; credentials are neither received nor persisted by Tinto.

## Verification Results

| Gate | Result | Evidence |
|---|---|---|
| Contract generation/check | PASS | `npm run contract:generate`; `npm run contract:check`. |
| Frontend format/lint | PASS | `npm run format:check`; `npm run lint`, including the final documentation reconciliation. |
| Full frontend tests | PASS | Exact `npm test`: 52 files and 691/691 tests. Focused TerminalPanel/contract: 127/127; corrected WSL gate: 97/97. |
| Focused ACP UI/contract | PASS | TerminalPanel plus bus contract: 127/127. |
| Frontend production build | PASS | `npm run build`; last-element access in `FileOverviewRuler.tsx` is compatible with the ES2020 target. |
| Rust format/check | PASS | `cargo fmt --all -- --check`; `cargo check --lib`. |
| Rust Clippy | PASS | Warnings-denied Clippy passes after four mechanical lint corrections. |
| Rust tests | PASS | 397/397 library tests, including ACP conformance/lifecycle and Windows descendant reaping. |
| Rust build | PASS | `cargo build --manifest-path src-tauri/Cargo.toml`. |
| Diff hygiene | PASS | `git diff --check`; only line-ending notices. |
| Review/security | PASS | No open P0-P2. |
| R18 matrix | PASS with allowed limitations | Six cells recorded; no unsupported cell is claimed as structured support. |

## Acceptance Ledger R1-R23

| Criterion | Status | Evidence |
|---|---|---|
| R1 | Satisfied | `validation::accepts_kimi_agent`; Kimi/OpenCode selectors and labels; source-aware contract includes all four canonical agent IDs. |
| R2 | Satisfied | Contract test pins all six ACP states and two modes; TerminalPanel renders and gates every state. |
| R3 | Satisfied | Typed ACP v1 initialization and both provider fixtures; legacy/pre-session incompatibility enters PTY compatibility. |
| R4 | Satisfied | WSL readiness never falls back to the host probe; readiness distinguishes source and distro. |
| R5 | Satisfied | Shared baseline normalizes prompt, updates, permission, completion, timeline and opaque provider session ID. |
| R6 | Satisfied | Negotiated images/models/modes are projected and enforced; absent capabilities remain hidden and rejected. |
| R7 | Satisfied | A real pre-session process, handshake or protocol failure enters visible PTY; retry requires confirmation and an idle turn. |
| R8 | Satisfied | `post_session_disconnect_fails_without_pty_fallback_or_turn_replay`. |
| R9 | Satisfied for declared structured modes | Cancel invalidates permissions, sends `session/cancel`, bounds cleanup and reaps descendants; Windows kill-on-close passes. WSL is not declared structured. |
| R10 | Satisfied | Public permission IDs bind generation, provider/Tinto session, turn, request/method and tool call; mismatch/duplicates are rejected. |
| R11 | Satisfied | First-winner concurrency, local deny distinct from cancel, deterministic expiry and accessible exact-option UI. |
| R12 | Satisfied | Auth errors are sanitized; provider-owned login guidance and retry never create a false native resume or silent PTY fallback. |
| R13 | Satisfied | Boundary tests cover frames, queues, pending RPC/permissions, cumulative updates/text, NDJSON, stderr and blocked stdin. |
| R14 | Satisfied | File/terminal client capabilities are not advertised; unsupported reverse requests receive method-not-found without side effects. |
| R15 | Satisfied | Kimi environment allowlist, ephemeral OpenCode child-only password, secret-canary removal and sanitized persistence. |
| R16 | Satisfied | Native load where available; failed/unavailable load creates a fresh ACP session plus ContextBridge without replay; goal/timeline/journal/checkpoint state is preserved. |
| R17 | Satisfied | Official schema validates versioned Kimi and OpenCode fixtures through the same core. |
| R18 | Satisfied with permitted limitations | Six platform cells are complete in the manual smoke. Linux/WSL and authenticated behavior retain explicit limitations and do not waive R9. |
| R19 | Satisfied | Repo-scoped readiness reports provider/source/prerequisite and forced recheck bypasses the frontend miss cache. |
| R20 | Satisfied | Serialized concurrent retry creates one generation, reuses one output reader and preserves session ID, transcript and checkpoint without replay. |
| R21 | Satisfied | Real probes record versions/startup/handshake/capabilities and exact auth/containment blockers; fixtures deterministically cover unreachable exchanges. |
| R22 | Satisfied | A pending permission remains authoritative until an explicit terminal decision; invalidated cards remain visible and non-actionable. |
| R23 | Satisfied | Stale, duplicate, unknown and mismatched messages are rejected using generation/session/turn/request/method correlation and bounded tombstones. |

## Acceptance Ledger AE1-AE11

| Criterion | Status | Evidence |
|---|---|---|
| AE1 | Satisfied | Kimi fixture reaches ready and exercises the shared baseline with timeline, journal, checkpoint and host context; unauthenticated real smoke is limited only under R18. |
| AE2 | Satisfied | Auth-required is sanitized, actionable and retryable without declaring a false resume mode or silently falling back. |
| AE3 | Satisfied | Kimi failure before a provider session produces visible PTY, lost capabilities and confirmed retry. |
| AE4 | Satisfied | Disconnect after ready produces failed, invalidates permissions, reaps the process tree and performs zero PTY replay. |
| AE5 | Satisfied | Authority, mismatch, expiry, disconnect, first-winner and deny-versus-cancel scenarios pass. |
| AE6 | Satisfied | Missing optional capabilities are hidden/rejected while the required baseline remains usable. |
| AE7 | Satisfied | Hostile/oversized input, unadvertised reverse RPC, environment canary and sanitization tests produce no out-of-root or `.git` mutation. |
| AE8 | Satisfied with R18 limitations | Both fixtures pass one conformance suite and all six cells have evidence or a named permitted limitation. |
| AE9 | Satisfied | Readiness and recovery name provider/source/prerequisite and query the correct source again. |
| AE10 | Satisfied | Dual probes and provider descriptors share one supervisor/wire loop; OpenCode differences remain descriptor/capability data. |
| AE11 | Satisfied | Host-context and plan wrapping plus ACP goal restoration match existing behavior and add no RDM-018 memory field. |

## Review And Security Closure

- Review Gate: passed for RU1, RU2 and final diff; no open P0-P2.
- Security Gate: passed with no open P0-P2. Correlation, limits, redaction, environment allowlists, reverse-method rejection, permission invalidation and process cleanup have observable tests.
- Superseding decision: OpenCode is invoked with loopback, `--port 0`, no mDNS and a 96-character UUID-derived per-launch password available only in the child environment. The observed `127.0.0.1:4096` is accepted as residual risk and no longer blocks an ACP attempt; failures of the actual process or ACP exchange still fail closed.
- Tinto creates or consumes no HTTP transport and stores no provider credential.

## Assumptions And Platform Limitations

- Current real probes: Kimi Code 0.27.0 and OpenCode 1.18.3, isolated from user credentials and persistent profiles.
- The Kimi real probe did not have provider authentication; deterministic fixtures cover structured exchanges without claiming an authenticated manual session.
- Linux-native and Ubuntu WSL cells were not available on this Windows host. Their exact prerequisites and PTY-only/cleanup limitations are recorded under R18/AE8.
- Kimi/OpenCode in WSL stay in PTY compatibility because verified distro process-group collection for structured ACP is not available in this cut.
- The real OpenCode 1.18.3 probe observed a valid ACP v1 handshake but did not complete `session/new`; native ACP eligibility is implemented without claiming that the unauthenticated smoke reached `ACP ready`.

## Blockers And Next Action

- Implementation blockers: none.
- Strict Definition-of-Done blockers: none.
- Initiative status: completed; the functional ledger and every repository-wide Verification Gate pass.
- Next action inside this package: none.
- Release Marshal, Jira and PR handoff remain optional and require a separate request.

## Archived History

- Full prior initiative history remains at `docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md`.
