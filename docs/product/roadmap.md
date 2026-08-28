# Tinto delivery roadmap

## Active initiative — Codex subagent parity

Created: 2026-08-28
Status: approved for autonomous local implementation; release requires manual approval

### Product framing

Tinto will become a complete native supervision surface for local Codex
subagent workflows. Codex continues to orchestrate; Tinto projects, persists,
restores, and controls the hierarchy through Agent Lens without copying Codex's
visual design or inventing a second scheduler.

### MVP boundary

#### CSP-001 — Agent-thread domain and persistence

- Add an explicit additive parent/child thread model, lifecycle/capability
  metadata, complete transcript restoration, and honest interrupted recovery.
- Preserve existing session, turn, event, checkpoint, provider-session, WSL,
  worktree, and archived-conversation compatibility.
- Exit: journal round-trips arbitrary-depth trees and legacy rows reconstruct
  unchanged.

#### CSP-002 — Codex protocol projection and controls

- Consume the installed app-server's agentic thread/item/status contract,
  including nested descendants, role/nickname metadata, activity, results,
  follow-up/steer, interrupt, wait, close, approvals, and inherited runtime
  metadata.
- Tolerate unknown fields and future item kinds; do not implement orchestration.
- Exit: structured adapter tests prove lifecycle/control correlation and bounded
  failure behavior.

#### CSP-003 — Tinto-native Agent Lens tree

- Render active/done nested threads, inspectable child transcripts, metadata and
  results, plus direct state-gated follow-up, interrupt, and close controls.
- Preserve keyboard navigation, focus, announcements, constrained layout, docked
  and detached consumers, and existing single-agent behavior.
- Exit: focused component/store tests cover hierarchy, controls, recovery, and
  accessibility states.

#### CSP-004 — Integration, review, and release readiness

- Reconcile generated contracts and all consumers; independently review backend
  contract/persistence and frontend interaction/accessibility surfaces.
- Run one aggregate verification fingerprint after the integrated diff settles.
- Exit: no unresolved P0-P2 finding, all viable units are release-ready, and one
  guarded Release Marshal plan is ready for user approval.

### Dependencies

```text
CSP-001 domain/persistence ─┐
                            ├─> CSP-002 protocol integration ─> CSP-004
CSP-003 UI shell/tests ─────┘             │
                 final contract/store binding ────────────────┘
```

CSP-001 and the initial CSP-003 view shell may run in parallel only while their
owned files and contracts remain disjoint. Protocol, generated-contract,
session-store binding, and final integration are serialized.

### Deferred

- Non-Codex subagent adapters.
- Custom-agent authoring UI.
- Remote/cloud agent trees and cross-device recovery.
- Automatic release, Jira mutation, or provider-independent orchestration.

### Start criteria

- The initiative contract is approved and content-bound in the swarm gate.
- Immutable executable worker contracts define exact ownership and checks.
- Deep public-contract/persistence work completes read-only discovery before
  mutation.
- Every implementation worker is no-shipping.

---

## Completed initiative — Tinto gap closure

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
