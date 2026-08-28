# Swarm startup — Codex subagent parity

Created: 2026-08-28
Mode: autonomous-team-flow
Documentation gate: user-authorized in the current thread; content receipt required

## Source context

- Initiative contract: `docs/plans/codex-subagent-parity/initiative-requirements.md`
- Roadmap: `docs/product/roadmap.md#active-initiative--codex-subagent-parity`
- Official behavior baseline: OpenAI Codex Subagents documentation.
- Installed protocol baseline: `codex-cli 0.150.0-alpha.12.2` app-server schema.
- Repository grounding: existing Codex app-server adapter, session journal,
  checkpoint, worktree, bus, session-store, and Agent Lens surfaces.

## Autonomy and shipping boundary

- The current user mandate authorizes documentation, worker dispatch, local code
  changes, tests, independent review, fixes, integration, and reconciliation
  without further confirmation.
- The autonomy ledger allows local reversible work only. It intentionally grants
  no branch push, PR, reviewer, Jira, merge, or release mutation.
- Every implementation worker operates in implementation-only/no-shipping mode.
- After every viable unit is release-ready, Seneschal invokes Release Marshal
  exactly once in guarded/manual mode and stops at the visible plan.

## Execution topology

1. Materialize and approve the documentary packet and local-only autonomy ledger.
2. Run read-only deep discovery for the public contract/persistence and Codex
   protocol units.
3. Execute at most two mutable workers only when ownership is disjoint; serialize
   public contracts, generated outputs, central session state, and integration.
4. Capture root observations, require independent reviewer certification for all
   behavior/contract changes, and dispatch bounded fixers only for concrete
   findings or failed checks.
5. Run root aggregate verification once for the final fingerprint, reconcile
   release-ready state, then prepare the guarded release plan.

## Role caps

- Implementer: 2 maximum, default 1 on central contract/session surfaces.
- Read-only deep discovery: 2 when questions are disjoint.
- Reviewer: 2, partitioned across backend contract/persistence and frontend
  interaction/accessibility.
- Fixer: 1, only for named findings or failed checks.
- Integrator: 1 after multiple units touch generated or shared contracts.
- Documenter: root-owned for maintained packet/orchestration docs.
- Security: only if implementation crosses permissions, approval routing,
  credentials, or a new trust boundary; inheritance display alone is reviewed
  as a contract boundary.

## Isolation

- Root owns documentation, queue state, generated contract reconciliation, and
  aggregate evidence in the current checkout.
- Mutable workers receive immutable owned-file manifests. Parallel mutation is
  permitted only for disjoint manifests; otherwise work is serialized in the
  shared checkout and root reconciles before the next dispatch.
- Workers may not create branches, worktrees, commits, pushes, or PRs.

## Verification gates

- Leaf checks are exact contract manifests and at most one natural affected
  suite; workers do not run aggregate checks.
- Root aggregate candidates: `npm run contract:check`, `npm run format:check`,
  `npm run lint`, `npm test`, `npm run build`, Rust formatting, Clippy, tests,
  build, and supported native E2E when the environment is available.
- Independent review binds to the exact contract hash and root-observed diff.
- Acceptance specifically covers nested hierarchy, status/activity, direct
  controls, approvals/source identity, inheritance metadata, restart recovery,
  unknown fields, capacity failure, and existing provider regressions.

## Stop conditions

- A required behavior is absent from installed Codex protocol evidence.
- Implementation would require a new provider, dependency, daemon, credential
  owner, destructive migration, automatic execution restart, or competing
  scheduler.
- A worker exceeds owned files, omits terminal validation, or performs shipping.
- Aggregate verification or independent review leaves an unresolved P0-P2.
- No independent safe unit remains.

## Release policy

No intermediate release phase is allowed. Release Marshal receives one
consolidated handoff only after local implementation, review, verification,
integration, and reconciliation are complete. It prepares a manual approval
plan and executes nothing until the user approves that later plan.
