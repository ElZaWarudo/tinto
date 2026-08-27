# Swarm startup — Tinto gap closure

Created: 2026-08-27  
Mode: manual, documentary planning  
Documentation gate: draft until validated, then in review

## Source context

- Initiative contract: `docs/plans/tinto-gap-closure/initiative-requirements.md`
- Roadmap: `docs/product/roadmap.md`
- Existing MCP roadmap/state: the active July artifacts named in the initiative
  contract.
- Native QA evidence: `docs/audits/2026-07-29-product-polish-regression-evidence.md`
- Current base: clean `develop`, aligned with `origin/develop` at preflight.

## Operating policy

- No product code, Jira, branch, commit, PR, or release mutation occurs before
  explicit documentation approval and the downstream gate that owns it.
- Use one mutable implementation worker at a time. The MCP work shares central
  bus, generated-contract, adapter, and terminal surfaces; parallel edits would
  create more reconciliation cost than throughput.
- Native QA and read-only discovery may run beside artifact work. Documentation
  aggregation stays root-owned.
- Prefer root-direct execution for bounded documentation and QA units. Use a
  nested Compound Master only for RDM-024 because its artifact/security pipeline
  is incomplete and its provider/configuration contract is high-risk.

## Child Compound envelope

```yaml
orchestrator: seneschal
run_id: gap-closure-rdm-024-mcp
state_path: docs/orchestration/compound-master/gap-closure-rdm-024-mcp/state.md
interaction: brokered
initiative_contract: docs/plans/tinto-gap-closure/initiative-requirements.md
mode: artifacts
target:
  roadmap: docs/product/roadmap.md
  roadmap_item: GC-001/RDM-024
  work_package: null
  review_unit: null
artifact_namespace: tinto-gap-closure/RDM-024-provider-neutral-mcp
shared_decisions:
  - docs/plans/tinto-gap-closure/initiative-requirements.md
depends_on: []
parallel: false
shipping: disabled
```

Exact artifact-run invocation after approval:

```text
krt-compound-master "RDM-024 provider-neutral MCP control plane" mode:artifacts interaction:brokered parallel:false jira-policy:optional worktree-policy:avoid autonomy:guarded review-threshold:P0-P2
```

The child receives the envelope above as inherited context. It may not dispatch
sibling initiatives or perform release mutations.

## Initial execution topology

1. Root-direct identity reconciliation and current-state refresh.
2. In parallel only where read-only: RDM-024 artifact child and Windows-native
   regression.
3. RDM-024 implementation units derived from its approved work package, executed
   serially unless the package proves non-overlap.
4. Root aggregate verification, independent review/security certification when
   triggered, atlas/roadmap reconciliation, then optional Release Marshal
   handoff under separate authorization.

## Role caps

- Implementer: 1 mutable worker.
- Compound Master child: 1 active artifact or implementation pipeline.
- Reviewer: 1 only when behavior, contracts, security, or compatibility changes.
- Security Sentinel: required for provider configuration, secret handling, and
  trust-boundary changes.
- Fixer: only for a named failed check or review finding.
- Integrator: omitted unless at least two independently implemented units later
  acquire a real dependency or merge conflict.
- Documenter: root-owned; no separate worker for narration.

## Isolation

- Documentation and root-direct QA use the clean current checkout.
- Mutable implementation should use one Codex worktree rooted from the approved
  shared documentation revision. If a shared revision does not exist, execution
  remains serial in the current checkout until the repository's git workflow
  provides one.

## Verification gates

- Leaf units: contract-named focused checks and at most one natural affected
  suite.
- Aggregate: `npm run contract:check`, `npm run format:check`, `npm run lint`,
  `npm test`, `npm run build`, Rust fmt/Clippy/test/build, and relevant Tauri E2E.
- Native evidence: exact commit, host/runtime facts, flow list, screenshots or
  semantic evidence where available, and explicit unverified surfaces.
- Review: independent code review for behavior/contract changes; Security
  Sentinel for provider configuration or trust-boundary changes.

## Stop conditions

- Unapproved documentation, an unresolved product/security decision, unsafe
  provider mutation, secret exposure, cross-boundary copying, overlapping
  mutable ownership, failed authoritative verification, or unavailable safe
  isolation.
- A native harness failure records a bounded blocker; it does not authorize
  redesigning Pumarejo or Tinto.

## Release policy

Seneschal stops at release-ready evidence. Only `krt-release-marshal`, under a
separate explicit request, may commit, push, open a PR, mutate Jira, request
reviewers, or merge.

