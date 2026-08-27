# Tinto gap-closure roadmap

Created: 2026-08-27  
Status: completed locally; release not requested

## Product framing

Tinto already has a broad desktop feature set. This roadmap closes incomplete
work and stale evidence before opening another strategic initiative. It does
not expand the product into memory, search, or mobile work.

## MVP boundary

### GC-001 — Reconcile identity and artifact authority

- Rename the unfinished MCP initiative from the conflicting `RDM-023` to
  `RDM-024` in active MCP/orchestration artifacts.
- Preserve the completed RDM-023 installation history.
- Resume RDM-024 through one brokered Compound Master artifact run.
- Exit: focused requirements, implementation plan, work package, and required
  document/security gates agree on scope and identifiers.

### GC-002 — Implement the smallest useful MCP control plane

- Begin only after GC-001 is execution-ready.
- Start with provider evidence plus read-only inventory/import. Admit profiles,
  synchronization, and launch overrides only for proven provider targets; do
  not force a shared abstraction when the evidence matrix falsifies it.
- Deliver the admitted RDM-024 product contract without introducing new
  infrastructure.
- Let the child work package define implementation units from repository
  evidence; do not pre-commit to three layers or three PRs.
- Exit: accepted behavior, compatibility, redaction, and provider evidence pass
  focused and aggregate verification.

### GC-003 — Restore native evidence and current product mapping

- Re-run the previously blocked Windows-native regression on the current commit.
- Record an audit addendum with observed, failed, and unverified flows.
- Update the application atlas and old active-roadmap statuses only from that
  evidence and delivered git history.
- Exit: the atlas is either current with a new verified commit/fingerprint or
  remains explicitly stale with a precise blocker.

## Dependencies

```text
Documentation approval
  ├─> GC-001 artifact reconciliation ─> GC-002 MCP implementation
  └─> GC-003 native smoke ────────────> atlas refresh

GC-001 + GC-003 ─> roadmap status reconciliation
```

GC-003 may run while the RDM-024 artifact pipeline is active because it does
not touch MCP contracts or implementation surfaces. Mutable product-code work
remains serial.

## Future scope

- Guided authoring refinements beyond the minimum profile/import/sync flow.
- Additional providers not already represented by Tinto.
- Release/tag work and dependency-maintenance batching.
- RDM-021 mobile companion remains backlog-only and is not the next initiative.

RDM-018 agent memory, the remaining RDM-019 adapter work, and RDM-020 content
search were retired from the product roadmap on 2026-08-27. Existing code and
historical artifacts are preserved, but no future implementation is planned.

## Risks and controls

- **Secrets/provider files:** explicit actions, redaction, drift detection, and
  Security Sentinel review.
- **Contract overlap:** one mutable worker at a time while bus, generated
  contracts, session adapters, or `TerminalPanel` are in scope.
- **False native confidence:** keep `Code/Tests` separate from `Observed`.
- **Scope growth:** new infrastructure or capabilities require a demonstrated
  acceptance gap and an explicit contract revision.

## Completion evidence

- GC-001 completed the approved RDM-024 artifact chain with no unresolved
  document or security gate.
- GC-002 implemented and reviewed the admitted U1-U3 slice. Aggregate Rust,
  frontend (159 focused tests), contract, type, lint, build and native IPC
  checks pass.
- GC-003 records current Windows-native evidence. Dashboard and Timeline were
  observed; keyboard coverage remains partial, and resize, live Agent launch,
  detached windows, and the new MCP Details content remain explicitly
  unverified in Pumarejo.
- GC-004 reconciled RDM-024 and the atlas. The atlas intentionally remains
  `stale` because the delivery is an uncommitted working tree and therefore has
  no new source commit/fingerprint.
- No branch, commit, push, PR, Jira, deployment or release mutation was made.
