---
title: Provider-neutral MCP control plane
status: execution-ready
roadmap_item: RDM-024
origin_roadmap: docs/roadmaps/2026-07-21-010-provider-neutral-mcp-layer-roadmap.md
origin_brainstorm: docs/plans/tinto-gap-closure/rdm-024-provider-neutral-mcp-requirements.md
origin_planning_input: docs/plans/tinto-gap-closure/rdm-024-provider-neutral-mcp-requirements.md
origin_plan: docs/plans/tinto-gap-closure/rdm-024-provider-neutral-mcp-plan.md
initiative_contract: docs/plans/tinto-gap-closure/initiative-requirements.md
compound_run_id: gap-closure-rdm-024-mcp
compound_state_path: docs/orchestration/compound-master/gap-closure-rdm-024-mcp/state.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1, RU2]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: unknown
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Provider-neutral MCP control plane

## Scope

Implement the first evidence-admitted RDM-024 slice: a safe Codex Windows
inventory/import projection, source-bound non-sensitive catalog, project-local
enablement profiles, explicit Codex MCP activity attribution, and an accessible
Agents surface that reports unsupported provider/target actions truthfully.
Preserve the existing `/mcp` host command as a compatibility entry point.

The slice is intentionally narrow. It may read the current user's Codex
configuration through the existing bounded path, but it does not write provider
files or execute imported commands.

## Non-goals

- Provider-file synchronization, drift overwrite, reimport conflict handling,
  or cross-boundary Windows/WSL copying.
- Claude, Kimi, OpenCode, and WSL configuration import before target-specific
  parser/root/identity evidence exists.
- Applying profiles to a provider launcher or restarting a running provider.
- Active connectivity probes, automatic server launch, tool approval, MCP
  client/proxy behavior, marketplace/cloud sync, or credential storage.
- A new provider framework, generic settings engine, database, daemon, service,
  polling subsystem, or speculative extension point.
- Changes to the completed RDM-023 runtime-installation implementation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: bounded internal names, additive DTO field
  order, colocated fixture organization, and equivalent focused test commands
  that preserve the requirements and existing seams.
- Agent must record as assumptions: repository conventions inferred from the
  existing Workbench TOML store and Agent Console command/event paths; any
  compatible adjustment to a listed surface; any focused check skipped with
  its exact reason.
- Agent must escalate: provider behavior not proven by the evidence matrix,
  credential handling, cross-target copying, automatic restarts, new
  persistence services, public contract removal, branch/base or Jira/PR
  strategy, destructive operations, or scope outside this package.
- Safe fallback: leave the affected target visibly `Unsupported` or `Unknown`,
  preserve existing configuration, and return a brokered decision request to
  Seneschal while continuing independent safe work.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: the existing RDM-016/RDM-022 runtime, bus, session, journal, and
  project workbench seams on `develop`.
- Blocks: later provider-specific inventory, synchronization, launcher
  application, and active connectivity units until their evidence gates pass.

## Production Posture

- Posture: unknown.
- Evidence: the inherited initiative contract and current repository contain
  live-compatible agent sessions, WSL routing, persistence, and `/mcp`; no
  explicit deployment environment was established for this initiative.
- Confidence: medium.
- Consequences for this package: preserve additive bus and Workbench TOML
  compatibility, fail closed on unsupported targets, and keep all writes
  explicit and bounded. Do not claim migration or rollback evidence beyond the
  existing atomic Workbench store behavior.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
| --- | --- | --- |
| U1 | yes | Safe Codex-local inventory and explicit activity attribution establish the additive catalog contract and provide independently testable value. |
| U2 | yes | Project-local profiles consume the stable source-bound catalog and use the existing Workbench persistence seam. |
| U3 | yes | The Agents surface is the usable consumer of U1/U2 and closes accessibility, unsupported-state, and `/mcp` compatibility behavior. |
| D1-D5 | deferred | Provider/target synchronization, launcher application, active connectivity, authoring, and non-Codex/WSL imports lack the required evidence and remain separate future decisions. |

Grouping rationale:

- RU1 groups U1's backend, bus, parser, normalization, and explicit activity
  work because they establish one authoritative non-sensitive contract. A
  schema-only or activity-only PR would have no independently reviewable
  consumer value and would create a public-contract stack.
- RU2 groups U2 and U3 because profile lifecycle and its Agents interaction are
  one user-visible project capability with one focused UI/contract verification
  path. The backend contract from RU1 is the stable base; RU2 is still
  independently mergeable after RU1.

## Implementation Units

- **U1 — Safe Codex-local inventory and catalog contract:** extend existing
  `commands.rs` parsing and bus DTO seams; normalize untrusted provider values;
  preserve `/mcp`; project only explicit `mcptoolcall` activity.
- **U2 — Project-local profile state over imported definitions:** add bounded,
  additive fields to existing Workbench TOML; expose explicit profile
  lifecycle commands; preserve prior state on persistence failure.
- **U3 — Agents profile surface and truthful activity states:** consume the
  stable contract in `TerminalPanel.tsx`, expose accessible inventory/profile
  states, and show unsupported actions without implying provider mutation.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
| --- | --- | --- | --- | --- | --- |
| RU1 | U1 safe catalog/contract and explicit activity attribution | Rust Agent Console commands/app-server, bus contract/client/generated mirror, colocated Rust/contract tests | `develop` | standalone Tarea if Jira is later enabled | Target <=500 human-authored lines; high input-redaction and public-contract risk; generated mirror reviewed separately within the same PR. |
| RU2 | U2 project profile persistence plus U3 Agents surface | Rust Workbench store/commands, Tauri registration, TerminalPanel, App.css, frontend/bus tests | refreshed `develop` after RU1 | standalone Tarea if Jira is later enabled | Target <=500 human-authored lines; persistence/accessibility risk; keep UI and its focused tests together for reviewer comprehension. |

## Reviewability Diagnosis

- Reviewer-experience check: yes. RU1 answers whether the source-bound catalog
  and activity contract are safe and additive; RU2 answers whether a developer
  can use the admitted profile capability from Agents. Each has focused tests
  and can merge after its dependency is integrated.
- Granularity chosen because: two capability slices isolate the parser/public
  contract risk from profile persistence/UI risk. Combining them would mix
  source trust, persistence, and accessibility in one difficult review; further
  splitting would create a schema-only or UI-only micro-PR without independent
  value.
- Open-stack plan: no open stack. Merge RU1 into `develop`, refresh the base,
  then branch RU2. Never open RU2 against an unmerged RU1 branch.
- Jira mapping: each independent review unit maps to one standalone `Tarea` if
  Jira context is later resolved; no parent with one child is created.
- Downstream-fix trace: none initially; if RU2 fixes a finding from a still-open
  RU1 review, record `addresses finding from PR #X` in the child state and
  handoff.
- Failure-mode check: two substantive capability PRs, not a micro-PR stack and
  not a deferred mega-consolidation PR.

## Files and Tests

- RU1 backend/contract: `src-tauri/src/agent_console/commands.rs`,
  `src-tauri/src/agent_console/app_server.rs`,
  `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`,
  `src/bus/contract.ts`, `src/bus/contract.generated.ts`, `src/bus/client.ts`,
  and their colocated tests.
- RU2 persistence/UI: `src-tauri/src/workbench/mod.rs`,
  `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs`,
  `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/bus/client.ts`,
  `src/panels/terminal/TerminalPanel.tsx`,
  `src/panels/terminal/TerminalPanel.test.tsx`, and `src/App.css`.
- Expected focused tests: Codex parse/redaction/containment tests, Workbench
  profile lifecycle tests, bus command/DTO tests, and the TerminalPanel
  accessibility/async-state tests described in the origin plan.
- Generated `src/bus/contract.generated.ts` is not hand-edited; it is produced
  by the repository contract generator and checked for parity.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri MCP catalog/profile DTOs, commands, and explicit activity projection; no auth or tenant contract change.
- Consumer scan patterns: `rg -n "run_agent_host_command|AgentSessionTimeline|WorkbenchConfig|list_workbenches|start_agent_session|mcptoolcall|mcp" src src-tauri`.
- Consumers found: existing host command/client, Agent Console session/timeline, Workbench persistence and snapshot, TerminalPanel, bus contract tests, and generated-contract parity checks.
- Contract-drift tests searched: `npm run contract:check`, exact registered command-name expectations, Workbench TOML round trips, existing `/mcp` command tests, and TerminalPanel host-command tests.
- Required consumer tests: colocated Rust parser/store tests, `src/bus/contract.test.ts`, `src/panels/terminal/TerminalPanel.test.tsx`, and the focused contract generator check.
- Consumer tests run/skipped: not run in artifact phase; implementation workers and root aggregate own the listed checks.

## Verification Gate

- Required focused commands are the origin plan's per-unit Rust/TypeScript/UI
  tests plus `npm run contract:check`; the root owns aggregate build, lint,
  formatting, Rust, and native smoke evidence.
- Surface-aware evidence: RU1 must show bounded parser and explicit attribution
  fixtures; RU2 must show profile lifecycle, unsupported states, keyboard/focus
  behavior, and no secret/provider-file mutation.
- Production posture evidence: unknown posture requires additive compatibility,
  no hidden writes, and explicit unsupported/unknown handling; no deployment or
  migration claim is made here.

## Review Gate

- Code/document review threshold: P0-P2.
- Findings below threshold: log them in the review record; any P0-P2 finding
  blocks execution-ready status until resolved.

## Security Gate

- Run after work-review loop: required for RU1/RU2 because provider-derived
  input, filesystem roots, persistence, and public bus/UI data are touched.
- Security Watch during work: enabled; inspect path containment, symlink/junction
  handling, secret redaction, inert rendering, and no-command/no-network rules.
- Security Watch notes: no credential ownership, arbitrary command execution,
  arbitrary provider network request, or cross-boundary write is admitted.
- Security reviewer: `krt-security-sentinel` when execution adds a high-risk
  write/process surface; inline artifact review for the current requirements.
- Security review result: pending execution; artifact-stage P0-P2 security
  review passed with the explicit deferrals above.
- Required security verification: adversarial fixtures for untrusted fields,
  canonical root escapes, links, WSL translations, malformed input, and secret
  omission before bus/UI/journal persistence.

## CI Break-Prevention And Escalation

- CI risk surfaces: additive Rust/TypeScript contracts, generated mirror,
  Workbench TOML compatibility, frontend accessibility tests, and existing
  agent session behavior.
- Preventive evidence: focused tests and contract checker per review unit;
  root aggregate verification and relevant native smoke are required before
  release handoff.
- If CI breaks: invoke `krt-ci-questor` with the run/check context; do not poll
  checks in Compound Master.
- Escalation rule: record a release-follow-up blocker until the incident has a
  cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1 safe catalog/contract, followed by RU2 project profile and
  Agents surface.
- Branch name: `feat/provider-neutral-mcp-control-plane` for RU1; refresh
  `develop` and use `feat/project-mcp-profiles` for RU2.
- Branch/docs rule: carry these planning artifacts with RU1's first semantic
  implementation branch; do not create a docs-only branch.
- PR base: `develop` for RU1; refreshed `develop` after RU1 for RU2.
- Suggested commit grouping for RU1:
  - `feat(agent-console): expose safe project MCP inventory` — Rust parser,
    additive bus types, generated mirror, and focused contract tests — one
    source-bound catalog authority.
  - `feat(agent-console): preserve explicit MCP activity attribution` — bounded
    Codex event projection and tests — one activity trust boundary.
- Suggested commit grouping for RU2:
  - `feat(workbench): persist project MCP profiles` — additive Workbench model,
    commands, and persistence tests — one profile lifecycle.
  - `feat(agents): expose MCP profile states` — Agents UI, accessibility styles,
    and consumer tests — one user-visible capability.
- PR title: `Add a safe project MCP control plane`
- PR body bullets:
  - Inspect Codex-local MCP definitions without exposing sensitive fields.
  - Keep project profiles source-bound and explicit without hidden provider writes.
  - Show MCP activity only when provider attribution is explicit.
  - Keep unsupported providers and targets visibly unsupported until evidenced.
- Verification results location: child Compound state and review record, then
  root wave reconciliation.
- Production/deployment notes: unknown posture; preserve existing `/mcp`, agent
  sessions, Workbench TOML compatibility, and WSL trust boundaries.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: two standalone review-unit tasks only if Jira
  context is resolved; do not create a parent with one child.
- PR-to-Jira mapping: one standalone task per RU; no Jira mutation is authorized
  in this artifact-only run.
- Jira summary: Gestionar el inventario MCP seguro por proyecto
- Jira description: Añadir inventario MCP local acotado, perfiles de proyecto y
  atribución explícita de actividad sin exponer secretos ni escribir archivos
  de proveedores de forma implícita.
- Optional-policy fallback: Jira omitted: provider/project context is not
  configured; release flow may resolve it later without blocking this package.
