---
title: Kimi Code and OpenCode ACP agent support
status: completed
roadmap_item: RDM-022
origin_roadmap: docs/roadmaps/2026-07-18-009-kimi-opencode-agent-support-roadmap.md
origin_brainstorm: none
origin_planning_input: docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md
origin_plan: docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md
units: [U1, U2, U3, U4, U5, U7, U6]
unit_alignment: complete
review_units: [RU1, RU2]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Kimi Code and OpenCode ACP agent support

## Scope

Implement the reviewed RDM-022 Product Contract completely: probe both current CLIs, add source-aware provider readiness, deliver a shared bounded ACP v1 adapter and a usable Kimi vertical first, then add OpenCode parity through descriptor/capability differences only, and close with conformance, security, regression, and six-cell platform evidence.

## Non-goals

- No Claude structured adapter, RDM-018 memory, HTTP transport owned or consumed by Tinto, network listener managed by Tinto, credential capture, automatic approval, ANSI semantic parsing, checkpoint redesign, PTY removal, or capability outside session load, attachments, models, and modes.
- No refactor of unrelated provider metadata, journal persistence, Agent Console layout, Codex app-server, Claude, or generic PTY behavior.
- No commit, branch creation, push, PR, Jira mutation, deployment, or release in this execution.

## Autonomy Contract

- Mode: guarded
- Agent may decide without asking: package-local names, equivalent focused test commands, fixture details observed in the bounded provider probes, exact internal type shapes, and implementation details that follow existing repository patterns without changing product behavior.
- Agent must record as assumptions: low-risk path choices, exact bounded constants already fixed by the plan, provider/platform limitations allowed by R18, and any skipped verification with its blocker.
- Agent must escalate: any change to R1-R23 or AE1-AE11; inability to enforce the user-approved KTD10 containment; inability to guarantee structured-process reaping; credential or paid-resource use; destructive data operations; public contract breakage outside the planned additive fields; scope beyond this package; or any request to ship.
- Safe fallback: continue deterministic fixtures, tests, and provider-independent implementation that do not depend on the blocked decision. A provider/platform may remain visibly PTY-only only where the Product Contract explicitly permits that outcome; never claim ACP-ready or waive a functional requirement.
- Autonomous ledger: none
- Allowed external mutation classes: none. Official packages may be downloaded into verified temporary roots for the read/probe step; no account, remote project, tracker, VCS remote, or production state may be mutated.

## Dependencies

- Requires: resolved — the user approved KTD10's narrow contained-loopback exception on 2026-07-18; `develop` at the preflight baseline or a later compatible integration head; Node 24/npm 11 and the repository's current Rust/Cargo toolchain; the existing RDM-017 WSL execution source; stable host-context functions already present in Agent Console.
- Blocks: none recorded outside this package.

## Resolved Decision

- Current `opencode acp` 1.18.3 always starts an internal HTTP listener. On 2026-07-18 the user explicitly approved the narrow KTD10 exception.
- Tinto communicates only over ACP stdio. The OpenCode descriptor must force loopback, port `0`, mDNS off, and an ephemeral random server password; any failed assertion keeps OpenCode in visible PTY compatibility mode.
- Security review must still verify the containment implementation before RU2 closes.

## Production Posture

- Posture: prototype
- Evidence: current Compound Master state and active roadmap classify Tinto as a prototype.
- Confidence: high
- Consequences for this package: implementation may add an internal contract surface directly, but it must preserve existing Codex, Claude, generic PTY, journal, checkpoint, and WSL behavior because those are explicit acceptance gates.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---:|---|
| U1 | yes | Both-provider probe and provider/source discovery are the execution gate for every later unit. |
| U2 | yes | Creates the shared bounded adapter and proves it first with Kimi. |
| U3 | yes | Supplies the Kimi lifecycle, fallback, recovery, resume, and reaping required for a usable structured session. |
| U4 | yes | Supplies backend-authoritative Kimi permissions before the UI can expose decisions. |
| U5 | yes | Completes the independently reviewable Kimi-first value slice. |
| U7 | yes | Adds OpenCode through the already-reviewed shared boundary with no transport fork. |
| U6 | yes | Closes all functional acceptance evidence and the platform matrix. |

Grouping rationale:

- The roadmap explicitly calls for at most two review units: a Kimi-first shared-client slice followed by OpenCode parity. That boundary gives reviewers a complete, independently usable Kimi capability before reviewing the smaller provider-parity delta.
- RU1 deliberately keeps U1-U5 together even though it is expected to exceed 1,000 human-authored lines. Splitting transport, lifecycle, permission contract, and UI would create half-wired bus/process states that are not independently mergeable or verifiable. Review is kept usable through focused commits and surface-specific tests inside the one Kimi capability slice.
- RU2 keeps OpenCode and final evidence together because the evidence is what proves its descriptor did not fork shared behavior. It is reviewed after the Kimi checkpoint but grouped into the same later PR because this execution is intentionally uncommitted and both units modify the same ACP core. Planning/orchestration docs remain with the capability they explain.

## Implementation Units

- U1. Probe provider behavior and add source-aware discovery — implemented and verified.
- U2. Implement the bounded shared ACP v1 adapter with Kimi — implemented and verified.
- U3. Integrate the Kimi lifecycle, fallback, retry, load, and process reaping — implemented and verified.
- U4. Add backend-authoritative Kimi permission handling — implemented and verified.
- U5. Complete the usable Kimi vertical in the existing Agents UI — implemented and verified.
- U7. Add OpenCode parity through the shared adapter — implemented and verified: current 1.18.3 attempts ACP under the approved `--port 0` assumption and retains visible PTY fallback for real pre-session failures.
- U6. Complete conformance, platform evidence, and regression validation — functional evidence and repository-wide verification are complete, with the permitted R18/AE8 limitations recorded in the manual smoke.

## Execution Status

- RU1 review: complete. Kimi-first shared adapter, lifecycle, resume, permissions and UI passed focused review after fixes for bounded cleanup, trusted timeline framing, auth-resume handoff and permission state.
- RU2 review: complete. OpenCode uses the same supervisor/transport; its descriptor forces loopback, `--port 0`, no mDNS and an ephemeral child-only password. The observed fixed port is accepted as residual risk and does not prevent the ACP attempt.
- Final P0-P2 review: no open findings. Retry concurrency, session/checkpoint/transcript preservation, load-to-context bridge and post-ready no-replay all have observable tests.
- Downstream-fix trace: none; all review findings were resolved inside this work package.
- Closure status: completed. The functional ledger, package review and every repository-wide Verification Gate pass.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1-U5: dual probe, provider/source readiness, shared ACP schema/transport, full Kimi lifecycle/permissions/UI | Rust process/session/bus, WSL launch/reap, generated+curated TS contract, React Agents UI, fixtures, tests, probe evidence | `develop` | optional parent + Kimi subtask | Expected 1,500-2,500 human lines plus generated contract. Above guardrail; retained as one complete Kimi capability because every smaller cut shares the live session contract and is not independently usable. Use surface-focused commits. |
| RU2 | U7+U6: OpenCode descriptor/capability parity, listener containment, provider fixture, matrix and final regressions | Existing ACP/session seams, focused UI tests, OpenCode fixture, docs/evidence | `develop`, grouped with RU1 in one later semantic PR | optional OpenCode sibling subtask | Expected 500-900 human lines plus evidence. Must contain no second wire loop or duplicated lifecycle. |

## Reviewability Diagnosis

- Reviewer-experience check: yes. RU1 is a coherent Kimi capability with one product story and complete verification; RU2 is a visibly smaller parity delta whose review question is whether OpenCode uses the same boundary safely.
- Granularity chosen because: Kimi-first is independently valuable and OpenCode adds a distinct provider-internal listener risk. Further subdivision would force reviewers to reason across unmergeable contract/process/UI fragments.
- Open-stack plan: no stack. Because this authorized execution creates no intermediate branch/commit and both review units overlap the ACP core, a later Release Marshal should group RU1 and RU2 into one semantic PR from `develop` while retaining separate internal review and Jira trace.
- Jira mapping: if Jira is used, one shared parent with two real sibling subtasks, one per review unit; the grouped PR backlinks both. No parent with only one child.
- Downstream-fix trace: none.
- Failure-mode check: two internal semantic review slices in one later PR, with Kimi reviewed before OpenCode; no unreviewable stacked delta must be reconstructed from an uncommitted checkout.

## Files and Tests

RU1 primary files:

- Backend/runtime: `src-tauri/src/agent_console/acp.rs` (new), `validation.rs`, `pty.rs`, `session.rs`, `mod.rs`, `commands.rs`, `journal.rs`, `checkpoint.rs`; `src-tauri/src/wsl_agent/mod.rs`; `src-tauri/src/wsl_agent/shell_env.rs`; `src-tauri/src/bus/contract.rs`; `src-tauri/src/lib.rs`; `src-tauri/Cargo.toml`; `src-tauri/Cargo.lock`.
- Contracts/UI: `scripts/generate-bus-contract.mjs`; `src/bus/contract.generated.ts`; `src/bus/contract.ts`; `src/bus/contract.test.ts`; `src/bus/client.ts`; `src/agent/sessionStore.ts`; `src/panels/agentAvailability.ts`; `src/panels/RepoCard.tsx`; `src/panels/terminal/TerminalPanel.tsx`; `AgentRuntimeControls.tsx`; `AgentConversationTab.tsx`; `ConsoleDockPanel.tsx`; `src/workspace/consoleDock.ts`; existing focused tests for those surfaces; `src/App.css` only for the required states/permission controls.
- Direct readiness/session consumers: `src/panels/RepoPanel.test.tsx`; `src/panels/DashboardPanel.test.tsx`; `src/demo/dashboardReview.tsx`; `src/demo/agentLensRestorable.tsx` and their affected fixtures.
- Fixtures/evidence: `src-tauri/src/agent_console/test_fixtures/kimi-acp-v1.jsonl` (new); `docs/manual-smoke/2026-07-18-kimi-opencode-agent-support.md` (new).

RU2 primary files:

- `src-tauri/src/agent_console/acp.rs`; `pty.rs`; `session.rs`; `commands.rs`; `src-tauri/src/agent_console/test_fixtures/opencode-acp-v1.jsonl` (new); the same bus contract surfaces only where observed OpenCode capability data requires it; focused `TerminalPanel` tests; `docs/manual-smoke/2026-07-18-kimi-opencode-agent-support.md`; `docs/contracts/bus-contract.md`; `docs/orchestration/compound-master-state.md`.

Expected focused tests include backend allowlist/source resolution, ACP framing/schema/correlation/limits, Kimi then OpenCode conformance fixtures, non-blocking handshake, generation mux, pre/post-session fallback rules, auth, resume/load replay suppression, WSL process-group reaping, permission races/timeouts, host-context golden parity, readiness cache/recheck, six-state UI matrix, generic ACP model/mode controls, accessibility, and Codex/Claude/PTY regression.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive `AgentSession` readiness/runtime/capability/fallback/permission fields; structured provider readiness result; permission-response and confirmed ACP-retry commands; provider descriptor/factory identity; schema-only ACP dependency; Kimi/OpenCode fixtures. No tenant or ownership model.
- Consumer scan patterns: `AgentSession`, `AgentSessionStatus`, `agent_binary_available_for_repo`, `agent_readiness`, `AgentProcess`, `AgentProcessFactory`, `spawn_agent`, `runtime_options`, `runtime_catalog`, `isCodexSession`, `opencode`, `claude`, `codex`, `provider_session_id`, `stop_agent_session`, `session_from_journal`.
- Consumers found: Rust session registry/commands/journal/checkpoints/lib registration; bus generator/generated/curated contracts and client; session store; RepoCard; TerminalPanel/runtime controls/conversation tab; ConsoleDockPanel/workspace dock; demo command mocks; their tests.
- Contract-drift tests searched: generator `INCLUDE` list, generated-vs-curated exact maps, allowlists and provider option arrays, journal reconstruction, runtime option normalization, availability cache, provider label/logo branches, session status expectations.
- Required consumer tests: Rust unit/integration tests in touched Agent Console/WSL modules; `src/bus/contract.test.ts`; focused RepoCard/TerminalPanel/AgentConversationTab/ConsoleDockPanel/workspace/availability tests; full `npm test` and `cargo test` gates.
- Consumer tests run: generated/curated contract parity, full Rust library suite (397/397), focused ACP suite, focused TerminalPanel/contract suite (127/127), all affected Repo/Dashboard/dock consumers, ESLint and Prettier. Full frontend test result is recorded below. No affected consumer was intentionally skipped.

## Verification Gate

- `npm run contract:generate`
- `npm run contract:check`
- `npm run format:check`
- `npm run lint`
- `npm test`
- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo build --manifest-path src-tauri/Cargo.toml`
- Pre-RU1 baseline: use the repository-supported Node 24/npm 11 runtime, record Rust/Cargo versions, and require clean contract parity plus focused existing Agent Console/frontend tests before attributing failures to this package.
- Surface-aware evidence: each changed runtime, contract, UI, security, compatibility, and platform surface must have the exact automated assertion or matrix row required by the origin plan. A documented R18 limitation cannot replace a functional gate for any cell declared structured-supported.
- Production posture evidence: regression coverage for Codex, Claude, generic PTY, WSL, journal/resume, checkpoints, host context, generated contracts, and frontend consumers is required despite prototype posture.

## Verification Results

| Gate | Result | Evidence |
|---|---|---|
| `npm run contract:generate` | PASS | Generated contract refreshed from the Rust source. |
| `npm run contract:check` | PASS | Generated/curated contract parity is clean. |
| `npm run format:check` | PASS | Prettier passed after the final documentation reconciliation. |
| `npm run lint` | PASS | ESLint completed without findings. |
| `npm test` | PASS | Exact gate: 52 files and 691/691 tests. Test files run deterministically without competing fork workers; the composite alias flow has a local 10-second timeout. Focused TerminalPanel/contract passes 127/127. |
| `npm run build` | PASS | TypeScript and Vite production build pass after using ES2020-compatible last-element access in `FileOverviewRuler.tsx`. |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | PASS | Rust formatting is clean. |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` | PASS | Warnings-denied Clippy passes after four mechanical lint corrections. |
| `cargo test --manifest-path src-tauri/Cargo.toml` | PASS | 397/397 library tests. |
| `cargo build --manifest-path src-tauri/Cargo.toml` | PASS | Native backend build completed. |
| Focused TerminalPanel + bus contract | PASS | 127/127 tests. |
| WSL absence/source gate | PASS | 97/97 tests after allowing the generated source-aware contract. |
| Windows descendant reaping | PASS | Kill-on-close descendant test passed. |
| `git diff --check` | PASS | No whitespace errors; only line-ending notices. |
| R18 platform matrix | PASS with allowed limitations | Six cells recorded; unsupported or unauthenticated cells are not represented as structured support. |

The functional R1-R23 and AE1-AE11 ledger is complete in `docs/orchestration/compound-master-state.md`. The package is completed: its Definition of Done and every repository-wide Verification Gate pass.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless the user marks them blocking.
- Result: passed for this package. Final implementation review found no open P0-P2, and the warnings-denied Clippy gate passes.

## Security Gate

- Run after work-review loop: required because this package changes provider subprocesses, environment/auth handling, permission authorization, untrusted protocol input, a dependency, WSL process cleanup, and OpenCode loopback containment.
- Security Watch during work: enabled for R13-R15 and R23 boundaries, permission decisions, process replacement/reaping, schema dependency changes, and OpenCode listener assertions.
- Security Watch notes: never persist raw stderr/provider error data or environment values; never advertise file/terminal capabilities; treat loss of loopback, authentication, no-mDNS, process ownership or process-tree cleanup as ACP-unavailable. The effective fixed port alone is the explicitly approved exception.
- Security reviewer: `krt-security-sentinel`.
- Security review result: complete with no open P0-P2. The implementation uses the schema-only ACP dependency, bounded frames/queues/pending work/text/stderr, exact generation/session/turn/request correlation, deny-safe permissions, unadvertised reverse-method rejection, provider-specific environment allowlists, sanitized persistence, bounded process-tree cleanup and a Windows kill-on-close job. OpenCode secrets stay in the child environment and never argv; the user-approved policy accepts the effective fixed port while preserving the other controls and fail-closed behavior for actual ACP failures.
- Required security verification: package identity, exact version, official registry source, and integrity provenance before ephemeral execution; empty non-sensitive probe repository and isolated credential-free profile; KTD10 flags plus cryptographically random per-launch `OPENCODE_SERVER_PASSWORD` from the first probe onward; effective socket/auth checks proving loopback-only, no mDNS, wrong/missing credential rejection, secret absence from argv/logs/persistence, and socket/process cleanup; boundary/slow-accumulation limits; stale/duplicate/mismatched correlation; currently advertised authentication method IDs bound to provider/generation/session with unknown or late methods rejected; inert bounded auth descriptions; concurrent permission first-winner and deny-safe invalidation; config-only secret canary; environment-name allowlists; unadvertised file/terminal reverse request rejection; Windows and distro-side process-tree cleanup.
- Security closure: all P0-P2 findings were fixed and re-reviewed. The explicit residual risk is that OpenCode 1.18.3 may materialize the requested `--port 0` as `4096`; this no longer blocks structured mode, while loopback, authentication, no-mDNS, process ownership and cleanup remain mandatory.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust/TypeScript contract generation and exact maps, Rust compile/MSRV/dependency lock, clippy, React typecheck/build, fixture portability, Windows/WSL process code, provider label allowlists, formatting, full tests.
- Preventive evidence: run every Verification Gate command locally; use deterministic fake clocks/factories/fixtures for behavior unavailable to CI; record matrix-only external limitations separately.
- Result: all package-specific and repository-wide compile, contract, lint, formatting, test, conformance, lifecycle, security, UI and process-reaping gates pass.
- If CI breaks: invoke `krt-ci-questor` with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: record a release-follow-up blocker until the CI incident has a cause, owner, and next action.

## Branch and PR Handoff Inputs

No branch or PR action is authorized now. If later handed to Release Marshal:

### Grouped RU1 + RU2 handoff

- Review unit: grouped RU1 + RU2 delivery after both internal review gates pass.
- Review units: RU1 — shared ACP and Kimi support; RU2 — OpenCode parity and closeout.
- Branch name: `codex/feature/kimi-opencode-acp-support`
- Branch/docs rule: carry roadmap, plan, package, state, and evidence with the semantic implementation branch; do not create a docs-only branch.
- PR base: `develop`
- Suggested commit grouping, preserving the internal review checkpoints:
  - `feat(agents): add source-aware Kimi readiness` — allowlist, provider discovery, contracts, selectors, and focused tests.
  - `feat(agents): run Kimi through bounded ACP` — schema dependency, adapter, process/session lifecycle, WSL cleanup, fixtures, and backend tests.
  - `feat(agents): expose safe Kimi permissions and recovery` — bus/UI state, accessible controls, UI tests, and related evidence.
  - `feat(agents): add OpenCode ACP parity` — startup descriptor, containment, fixture, negotiated behavior, and focused tests.
  - `docs(agents): record ACP platform support evidence` — support matrix, bus contract, orchestration state, and final acceptance ledger.
- PR title: `feat: add structured Kimi and OpenCode agent sessions`
- PR body bullets: Kimi-first readiness and ACP lifecycle; safe permissions/recovery; OpenCode shared-adapter parity and approved containment; platform matrix and preserved existing agents.
- Verification results location: work-package Verification Gate plus `docs/manual-smoke/2026-07-18-kimi-opencode-agent-support.md`.
- Production/deployment notes: desktop prototype; no migration or service rollout; new schema-only Rust dependency and provider process behavior require regression evidence. The superseding 2026-07-19 decision adds no HTTP transport: OpenCode's same-process listener remains loopback-only, authenticated and owned by the supervised child; actual process, protocol or cleanup failures still fail closed.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea, with a shared parent only if both review-unit subtasks are actually created.
- Suggested subtask behavior: two sibling subtasks are justified for the Kimi-first and OpenCode-parity review units. If only one is created, collapse it to a standalone Tarea rather than creating a parent with one child.
- PR-to-Jira mapping: one subtask per review unit; a grouped PR backlinks both and transitions both.
- RU1 Jira summary: `Añadir sesiones nativas de Kimi Code mediante ACP`
- RU1 Jira description: `Incorporar descubrimiento por origen, transporte ACP acotado, ciclo de vida, fallback PTY, permisos y experiencia accesible de Kimi Code sin alterar los agentes existentes.`
- RU2 Jira summary: `Completar la paridad ACP de OpenCode`
- RU2 Jira description: `Habilitar OpenCode sobre el adaptador ACP común, contener su listener interno, validar capacidades y cerrar la evidencia multiplataforma sin duplicar el transporte.`
- Optional-policy fallback: if Jira role/config/context is missing or Jira is not separately authorized, record `Jira omitted: not part of the requested local implementation` and continue.
