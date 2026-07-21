---
title: Agent runtime installation protocol
status: completed
roadmap_item: RDM-023
origin_roadmap: docs/roadmaps/2026-07-21-001-agent-runtime-installation-protocol-roadmap.md
origin_brainstorm: docs/brainstorms/2026-07-20-023-agent-runtime-installation-protocol.md
origin_planning_input: docs/brainstorms/2026-07-20-023-agent-runtime-installation-protocol.md
origin_plan: docs/plans/2026-07-21-023-feat-agent-runtime-installation-protocol-plan.md
units: [U1, U2, U3, U4]
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

# Agent runtime installation protocol

## Scope

Implement the reviewed RDM-023 product and planning contracts: immutable official npm recipes for four supported providers, shell-free host/WSL execution, attempt-bound consent, bounded process handling, same-runtime verification, exactly-once continuation, accessible launcher UI and fake-only automated evidence.

## Non-goals

Remote-script installers, arbitrary commands/providers, prerequisite installation, credentials/login, upgrades, downgrade, uninstall, repair, native elevation, background installation, automatic retry, unofficial mirrors, real package mutation in CI and unrelated Agent Console refactors.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: internal names, module-private data shapes, equivalent focused test commands, fixture organization and small convention-following adjustments that preserve the reviewed contracts.
- Agent must record as assumptions: inferred repository conventions, low-risk path choices, compatible file-list adjustments and any skipped verification with its exact blocker.
- Agent must escalate: changed product behavior; any shell/elevation/remote-script path; credentials; public contract removal; destructive operations; new persistence; branch/base or Jira/PR strategy; real global installation; new dependency; or scope outside this package.
- Safe fallback: continue read-only investigation, fake-runner tests and package-local work that does not depend on the blocked decision; otherwise return the exact blocker and question.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-022 and current provider availability/launch paths integrated on `develop`.
- Blocks: future agent upgrade/repair management only; no current package.

## Production Posture

- Posture: prototype.
- Evidence: accepted prior Compound Master state and current roadmap/plan.
- Confidence: high.
- Consequences for this package: iteration is allowed, but existing installed-provider launch behavior, source isolation and consent/security boundaries remain compatibility requirements.
- Breaking existing behavior allowed: no, except with explicit approval.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Establishes the recipe, prerequisite and additive contract required by every later unit. |
| U2 | yes | Implements the backend attempt/execution/verification/start lifecycle. |
| U3 | yes | Exposes the reviewed consent and progress flow to the launcher. |
| U4 | yes | Closes security, full verification and operator documentation for the same capability. |

Grouping rationale:

- One package preserves one product contract and prevents backend/frontend semantics from drifting.
- RU1 groups U1 and U2 because recipe identity, contract types, attempt correlation and the internal start refactor must be reviewed and verified together; splitting them would create a schema-only PR with no independently usable value.
- RU2 groups U3 and U4 because the UI consumes RU1 and its security/documentation evidence explains the shipped interaction. It remains a focused consumer/evidence review rather than mixing into the already-dense process-authority review.

## Implementation Units

- U1: completed; exact recipe, prerequisite, launcher and generated-contract verification passes.
- U2: completed; fake local/WSL execution, single-use attempt, cancellation/launch linearization, containment, verification and exactly-once continuation pass.
- U3: completed; accessible launcher, consent, cancellation, terminal-state and cache tests pass.
- U4: completed; security hardening, official-source verification, operator documentation and full gates recorded.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | U1+U2 backend authority and generated contract | Rust install/commands/state/WSL runner, generated TypeScript contract/client, focused Rust/contract tests | `develop` | shared parent + backend subtask if Jira available | High security/process risk; target 700-950 human-authored lines. Generated contract reviewed separately within the PR. Split again if human-authored diff exceeds ~1,000 lines. |
| RU2 | U3+U4 launcher experience and closure evidence | React launcher/cache/styles/tests, bus docs, manual smoke, orchestration docs | refreshed `develop` after RU1 merge | shared parent + UI/evidence subtask if Jira available | Target 450-700 human-authored lines; sequencing depends on merged RU1, then the PR has an unambiguous integration base. |

## Reviewability Diagnosis

- Reviewer-experience check: yes. RU1 presents the complete authority/process boundary and exact contract; RU2 presents the complete user interaction and proof against that stable boundary.
- Granularity chosen because: each PR has one primary reviewer question and its own natural verification, while a single combined PR would mix roughly four risk domains and likely exceed the warning threshold.
- Open-stack plan: no open stack. Wait for RU1 to merge into `develop`, refresh the integration base, then branch RU2. Do not open RU2 against the unmerged RU1 branch without a new explicit branch-strategy decision.
- Jira mapping: two review units share one parent with one subtask per RU when Jira is available; each PR backlinks its subtask. If Jira context is unavailable, record the optional omission.
- Downstream-fix trace: none initially; RU2 must record `addresses finding from PR #X` if it changes a surface flagged on still-open RU1.
- Failure-mode check: two substantive PRs, not a micro-PR chain and not a deferred consolidation PR.

## Files and Tests

- Backend/contract: `src-tauri/src/agent_console/install.rs`, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/lib.rs`, optional `src-tauri/src/wsl_agent/shell_env.rs`, `src-tauri/src/bus/contract.rs`, `src/bus/contract.ts`, `src/bus/client.ts`, `src/bus/contract.test.ts` and colocated Rust tests.
- Frontend/docs: `src/panels/RepoCard.tsx`, `src/panels/RepoCard.test.tsx`, `src/panels/agentAvailability.ts`, `src/panels/DashboardPanel.test.tsx`, `src/App.css`, `docs/contracts/bus-contract.md`, `docs/manual-smoke/2026-07-21-agent-runtime-installation-protocol.md` and orchestration artifacts.
- Required tests and commands are defined in the origin plan Verification Contract.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive Tauri commands/types, generated TypeScript bus mirror/client, source-aware readiness cache invalidation, process-runner fixtures and internal session-start factoring. No auth/tenant/persistence contract.
- Consumer scan patterns: `rg -n "start_agent_session|agent_provider_readiness_for_repo|AgentProviderReadiness|checkAgentAvailabilityForRepo|RepoAgentLauncher" src src-tauri`.
- Consumers found: `src/bus/client.ts`, `src/bus/contract.ts`, `src/panels/agentAvailability.ts`, `src/panels/RepoCard.tsx`, dashboard/launcher tests, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/lib.rs` and bus contract tests/docs.
- Contract-drift tests searched: generated bus contract check, exact command-name/client expectations, provider allowlist/readiness tests, launcher mocks and WSL source/distro tests.
- Required consumer tests: focused Rust install/command tests; `src/bus/contract.test.ts`; `src/panels/RepoCard.test.tsx`; `src/panels/DashboardPanel.test.tsx`; full frontend/Rust suites.
- Consumer tests run/skipped: focused Rust protocol tests 14/14 pass; focused RepoCard + bus contract tests 55/55 pass. Frontend full suite passes 711/711 with one fork worker and 30-second timeouts. Rust full run passes 408/412 together; the four unrelated historical timing/order failures pass 4/4 when isolated. No RDM-023 test failed.

## Verification Gate

- Run the complete Verification Contract from `docs/plans/2026-07-21-023-feat-agent-runtime-installation-protocol-plan.md`.
- Surface-aware evidence: exact recipe/launcher and attempt lifecycle in Rust tests; generated-contract check for IPC; keyboard/status/double-confirm behavior in React tests; fake local/WSL runners for source isolation; diff searches for shell/elevation/remote scripts and persistence.
- Production posture evidence: prototype does not relax installed-provider compatibility, consent, exact-runtime, cleanup, no-shell or no-real-CI-install requirements.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless the user marks them blocking.

## Security Gate

- Run after work-review loop: required because the feature executes global package installation and crosses supply-chain, process, consent and WSL boundaries.
- Security Watch during work: enabled for both RUs.
- Security Watch notes: reject shell wrappers, remote scripts, elevation, IPC-supplied recipes, secret-bearing output/persistence, runtime fallback and replayable attempts.
- Security reviewer: `krt-security-sentinel`.
- Security review result: pass; no open P0-P2 findings. Security Watch findings on output draining, minimal environment, cancellation/launch ordering, bounded attempts and descendant containment were fixed and verified.
- Required security verification: exact compiled argv provenance; Windows `node.exe`/`npm-cli.js` association; attempt entropy/TTL/single claim; bounds/cleanup/redaction; same-runtime verification; fake-only automation.

## CI Break-Prevention And Escalation

- CI risk surfaces: generated contract drift, Rust platform conditionals, WSL tests, React async dialog tests, formatting/lint, full suites and Linux/Windows bundles.
- Preventive evidence: plan Verification Contract plus explicit Windows/WSL fake runner cases and full CI-equivalent commands.
- If CI breaks: invoke `krt-ci-questor` with PR/run/check context; do not poll checks in Compound Master.
- Escalation rule: hold release follow-up until the incident has evidence, ownership and a next action; never bypass the no-real-install test boundary.

## Branch and PR Handoff Inputs

- Review unit: RU1, backend installation authority.
- Branch name: `feat/agent-runtime-installation-core`.
- Branch/docs rule: RU1 carries the related planning artifacts on the same semantic branch; no planning-only branch.
- PR base: `develop`.
- Suggested commit grouping for this review unit:
  - `feat(agent-runtime): add governed installation lifecycle` — Rust recipe/attempt/runner/start surfaces and focused tests — one authority boundary.
  - `feat(agent-runtime): expose installation contract` — generated contract/client and contract tests — isolate generated review noise without splitting the PR.
- PR title: `feat(agent-runtime): add governed installation flow`.
- PR body bullets: add immutable shell-free recipes and single-use consent; verify in the exact host/WSL runtime before starting; keep output/processes bounded and automation fake-only.
- Verification results location: work package and Compound Master state.
- Production/deployment notes: no migration; additive IPC; global installation occurs only after explicit confirmation.
- Autonomous mutation request: none.

RU2 handoff after RU1 merges:

- Review unit: RU2, launcher experience and closure evidence.
- Branch name: `feat/agent-runtime-installation-ui`.
- PR base: refreshed `develop` containing RU1.
- Suggested commit grouping: `feat(agent-runtime): add installation consent experience` for React/cache/tests; `docs(agent-runtime): document installation safety and verification` for contract/manual-smoke/orchestration evidence.
- PR title: `feat(agent-runtime): add installation consent experience`.
- Verification results location: work package and Compound Master state.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: use one shared parent only because this package has two real review units; create one subtask per RU and backlink the matching PR. If Jira context/config is missing, omit Jira non-blockingly.
- PR-to-Jira mapping: RU1 and RU2 remain separate Jira subtasks even if later grouped into one PR; backlink and transition every covered subtask.
- Jira summary: `Añadir instalación gobernada de agentes ausentes`.
- Jira description: `Permitir que Tinto muestre y ejecute, con consentimiento explícito, recetas oficiales e inmutables en el runtime correcto, verifique el binario y continúe el lanzamiento una sola vez.`
- Optional-policy fallback: if Jira role/config/context is missing, record `Jira omitted: optional context unavailable` and continue.
