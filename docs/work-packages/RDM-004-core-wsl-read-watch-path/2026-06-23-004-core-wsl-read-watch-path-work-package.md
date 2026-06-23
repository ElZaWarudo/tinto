---
title: Core WSL read/watch path and event forwarding
status: review-passed
roadmap_item: RDM-004
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-004-core-wsl-read-watch-path.md
origin_planning_input: docs/brainstorms/2026-06-23-004-core-wsl-read-watch-path.md
origin_plan: docs/plans/2026-06-23-004-core-wsl-read-watch-path-plan.md
units: [U1, U2, U3, U4]
unit_alignment: complete
review_units: [RU1, RU2]
base_branch: develop
pr_strategy: local-final-batch
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Core WSL Read/Watch Path And Event Forwarding

## Scope

Enable Windows workbenches to track local Windows repos and configured Ubuntu WSL repos at the same time. Local repos keep the existing local git/watcher path. WSL repos route read and watch-like monitoring through `tinto-agent`, then rejoin the existing bus contract as normal `RepoDelta`, `FsEventBatch`, snapshot, and subscribed-diff payloads.

## Non-goals

- No WSL repo mutations, file operations, Gitleaks config creation, Gitleaks scans/install/status, media preview, or Agent Console sessions.
- No distro discovery or non-Ubuntu UX.
- No `\\wsl$` filesystem path translation.
- No Linux desktop WSL UI, commands, settings, empty states, degraded notices, or behavior.
- No agent packaging/auto-install/recovery UX beyond the approved dev-source launch model.
- No release handoff until the end of the active batch.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: helper names, DTO module boundaries, equivalent test names, local fixture shape, and whether narrowly shared helper extraction is cleaner than duplication.
- Agent must record as assumptions: mocked WSL transport behavior, skipped real Windows/Ubuntu smoke, and any bounded polling interval chosen for first WSL event forwarding.
- Agent must escalate: enabling mutations/Gitleaks/media/Agent Console for WSL repos, changing distro policy, changing release timing, adding shell interpolation, weakening Linux absence, or modifying public frontend event names.
- Safe fallback: complete RU1 read/snapshot support and mark RU2 blocked if watch/event forwarding needs a product or packaging decision.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-001 delivered to `origin/develop`.
- Requires locally: RDM-002 and RDM-003 implemented and review-passed in the current dirty worktree.
- Blocks: RDM-005 and RDM-006.

## Production Posture

- Posture: prototype.
- Evidence: `docs/orchestration/compound-master-state.md` records prototype posture and local desktop iteration flow.
- Confidence: high.
- Consequences for this package: mocked Linux/CI tests can prove routing and contract behavior; final confidence still needs manual Windows/Ubuntu WSL smoke before batched release.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Required to make the Linux agent return existing DTOs for reads and status. |
| U2 | yes | Required to mount local and WSL repos in one runtime workbench safely. |
| U3 | yes | Required for actual tracking, subscriptions, and event forwarding. |
| U4 | yes | Required to preserve contract docs, Linux absence, and verification evidence. |

Grouping rationale:
- U1 and U2 are tightly coupled because routing cannot be verified without agent read DTOs.
- U3 is split into RU2 because event loops/subscriptions add risk after the read path.
- U4 spans both review units because docs and verification must stay current with each runtime step.

## Implementation Units

- U1 - Protocol DTOs and agent read handlers.
- U2 - Host backend selection and read command routing.
- U3 - WSL watch, subscriptions, and event forwarding.
- U4 - Contract, absence, and verification.

## Review Unit Progress

| Review unit | Status | Notes |
|---|---|---|
| RU1 | review-passed | Protocol DTOs, WSL agent read handlers, mixed local/WSL runtime mounting, snapshot/retry/read command routing implemented and verified. |
| RU2 | review-passed-with-gap | Bounded WSL polling refreshes repo deltas and subscribed diffs through `tinto-agent`; fine-grained WSL `fs-events` are deferred as a recorded hardening gap. Contract docs, verification, and security review are complete for the implemented scope. |

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL read/snapshot backend path | `src-tauri/src/wsl_agent/*`, `src-tauri/src/bin/tinto-agent.rs`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, focused Rust tests, package/state docs | `develop` with queued RDM-002/RDM-003 changes | optional Tarea | High risk; core backend routing and external process protocol. Keep event loop out of RU1 if review size grows. |
| RU2 | WSL watch/subscription/event forwarding and closeout | `src-tauri/src/wsl_agent/*`, `src-tauri/src/bus/mod.rs`, `docs/contracts/bus-contract.md`, frontend contract/store tests if needed, security/review docs | RU1 integrated | optional Tarea | High risk; long-running activity and event semantics. Requires security review. |

## Files and Tests

Expected files:
- `src-tauri/src/wsl_agent/protocol.rs`
- `src-tauri/src/wsl_agent/launcher.rs`
- `src-tauri/src/wsl_agent/mod.rs`
- `src-tauri/src/bin/tinto-agent.rs`
- `src-tauri/src/bus/mod.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/bus/contract.rs`
- `src-tauri/src/workbench/mod.rs`
- `src/bus/contract.ts`
- `src/bus/store.test.ts`
- `src/workbench/wslAbsence.test.ts`
- `docs/contracts/bus-contract.md`
- `docs/orchestration/compound-master-state.md`

Expected tests:
- WSL protocol request/response round trips and rejects malformed/oversized/unsupported messages.
- Agent allowlist rejects repo paths outside the active WSL repo set.
- Mixed local/WSL workbench snapshot contains both repos on Windows.
- Local watcher receives only local entries.
- WSL repos do not enter local canonicalization/read code paths.
- WSL read commands return existing DTOs for tree, file content, blob, log, commit diff, and worktree diff.
- WSL agent failures map to per-repo errors.
- Mixed local/WSL subscriptions refresh each repo through the correct backend.
- WSL deltas and subscribed diff refreshes use existing event names and shapes.
- Fine-grained WSL fs-events remain deferred until the WSL event stream is hardened.
- Non-Windows WSL absence tests remain green.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal protocol DTOs, bus backend resolver, WSL runtime handling, read command routing, watcher/event forwarding, contract docs.
- Consumer scan patterns: `rg "resolve_repo|ensure_known|RepoResolveError|RepoEntry|RepoSource|set_workbench|watch_workbench|SubscriptionTarget|RepoDelta|FsEventBatch|wsl_agent|tinto-agent|unsupported_repo_source" src src-tauri docs/contracts`.
- Consumers found: bus task, bus commands, workbench runtime projection, watcher, frontend bus client/store, file view consumers, workbench absence tests, contract docs.
- Contract-drift tests searched: Rust `RepoDelta` shape test, TypeScript contract tests, frontend store snapshot/delta tests, WSL absence tests.
- Required consumer tests: targeted Rust bus/wsl_agent/command tests, targeted frontend contract/store/absence tests, typecheck.
- Consumer tests run/skipped: targeted Rust bus and WSL agent suites, invoke handler, tinto-agent build, targeted frontend contract/store/absence tests, typecheck, targeted Rust formatting, and `git diff --check` passed. Real Windows/Ubuntu WSL smoke is pending for final batched release.

## Verification Gate

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `cargo test --lib invoke_handler`
- Targeted Rust command tests for WSL read routing.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/wslAbsence.test.ts`
- `npx tsc --noEmit`
- Targeted `rustfmt --edition 2021 --check --config skip_children=true <changed Rust files>`
- Targeted `npx prettier --check <changed TS/TSX files>`
- `git diff --check`
- Manual final-release smoke on Windows/Ubuntu WSL: local repo plus `/home/...` WSL repo in one workbench, snapshot/diff/tree/file read/event refresh.

Verification results:
- `cargo test --lib wsl_agent`: 15 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `cargo build --bin tinto-agent`: passed.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/wslAbsence.test.ts`: 107 passed.
- `npx tsc --noEmit`: passed.
- Targeted `rustfmt --edition 2021 --config skip_children=true ...`: applied, then focused Rust tests passed.
- `git diff --check`: passed with CRLF warnings only.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Artifact review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-004-work-package-review.md`.
- Code review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-004-code-review.md`.

## Security Gate

- Run after work-review loop: required because this package launches/uses WSL agent protocol and reads Linux repo files through an external process.
- Security Watch during work: enabled.
- Security Watch notes: WSL repo paths stay source-aware and are not Windows-canonicalized; local-only `resolve_repo` still rejects WSL so mutations/Gitleaks/media/Agent Console remain disabled; read routing uses agent allowlists; host launch uses argument vectors; real `\\wsl$` traversal is not introduced.
- Security reviewer: `krt-security-sentinel` or direct evidence-based fallback if unavailable.
- Security review result: passed for implemented RDM-004 scope. Findings path: `docs/review-findings/2026-06-23-rdm-004-security-review.md`.
- Required security verification: no shell interpolation, no `\\wsl$`, agent allowlist, bounded messages, bounded reads, `.git` exclusion, safe error categories, per-repo failure isolation, non-Windows absence.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust compile/tests, Windows-only cfg, protocol serialization, bus timing tests, TypeScript contract/store tests, formatting.
- Preventive evidence: focused Rust/TS verification passed locally; real Windows/Ubuntu WSL smoke remains a final-release manual gate.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1/RU2 batched only after all active work packages complete, unless the user changes release timing.
- Branch name: `feat/wsl-read-watch-runtime`
- Branch/docs rule: related planning artifacts ship with the final batched release; no intermediate release for this package.
- PR base: `develop` if the user requests PR flow; otherwise final local no-PR release targets `develop` over `origin/develop`.
- Suggested commit grouping for this review unit:
  - `feat(wsl): route WSL repo reads through the agent` - protocol DTOs, agent read handlers, bus resolver, read command routing, tests.
  - `feat(wsl): forward WSL repo activity into the workbench bus` - WSL watch/subscriptions/event forwarding, per-repo errors, tests.
  - `docs(orchestration): add WSL read-watch runtime artifacts [skip ci]` - requirements, plan, package, findings, and state.
- PR title: `Track Windows and WSL repos together`
- PR body bullets:
  - Route configured Ubuntu WSL repos through the Linux agent while local repos keep the local backend.
  - Preserve existing snapshot, delta, fs-event, tree, diff, and file-read contracts.
  - Keep mutations, Gitleaks, media preview, and Agent Console routing deferred.
- Verification results location: this package and `docs/orchestration/compound-master-state.md`.
- Production/deployment notes: final release requires manual Windows/Ubuntu WSL smoke because CI cannot exercise real WSL.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea unless later WSL packages are grouped under a parent.
- Jira summary: `Trackear repos locales y WSL juntos en Windows`
- Jira description: `Permitir que Tinto monitorice repos locales de Windows y repos Ubuntu WSL en el mismo workbench, preservando el contrato actual y dejando mutaciones y consola de agentes fuera de esta entrega.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue.
