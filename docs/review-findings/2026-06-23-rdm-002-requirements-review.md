---
title: Windows WSL agent bootstrap requirements review
status: passed
date: 2026-06-23
artifact: docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md
review_type: requirements-review-fallback
reviewers:
  - compound-master-lead
  - requirements-weaver-quality-checklist
---

# Windows WSL agent bootstrap requirements review

## Result

Requirements review passed after user decision.

The packet is coherent, traceable to the reviewed WSL complement roadmap, and keeps the Linux absence boundary intact. Functional requirements, non-functional requirements, business rules, acceptance criteria, and non-goals are explicit enough for stakeholder validation. The two product decisions that blocked planning were resolved on 2026-06-23: first support is WSL 2 only, with one selected distro per WSL repo, and Ubuntu is the first manual smoke target.

## Blocking Findings

### P1 - WSL baseline must be chosen before planning - resolved

The roadmap explicitly requires a decision before RDM-002 planning: `WSL 2 only` versus `WSL 1 best effort`. This affects launcher expectations, smoke validation, health diagnostics, and whether the plan can rely on WSL 2 semantics. Planning without this answer would invent product support policy.

Resolution:
- User chose `WSL 2 only` on 2026-06-23.

### P1 - First-release distro scope must be chosen before planning - resolved

The roadmap also requires a decision before RDM-002 planning: support multiple distros in the first release, or support one selected distro per WSL repo. This affects DTOs, launcher lifecycle ownership, state shape, validation matrix, and later RDM-003 UX assumptions. Planning without this answer would invent product behavior.

Resolution:
- User chose `one selected distro per WSL repo` on 2026-06-23.
- User identified `Ubuntu` as the first expected manual smoke distro.

## Deferred Implementation Blocker

### P1 - `tinto-agent` availability model blocks implementation, not requirements validation

The packet correctly records the install/update/dev model as open: manual binary path, app-managed copy per distro, or dev-only build/run from source. This can remain open during stakeholder validation if the plan explicitly keeps RDM-002 to DTO/launcher/handshake scaffolding. It becomes blocking before implementation because it determines the launched command, packaging assumptions, and Windows/WSL smoke checklist.

## Non-Blocking Notes

- The packet correctly keeps WSL repo picker UI, read/watch routing, file mutations, Gitleaks/secret scan routing, media preview, and agent-console routing out of RDM-002.
- The packet correctly distinguishes existing local `agent_console` PTY behavior from the new Linux-side `tinto-agent` protocol.
- The packet correctly records that this workspace can cover mocked launcher/protocol tests, while final confidence requires manual Windows/WSL smoke.

## Review Checklist Summary

- Scope boundary: passed after resolving WSL baseline and distro scope.
- Traceability to roadmap: passed.
- Linux absence requirement: passed.
- Acceptance criteria: passed for requirements-level validation.
- Planning readiness: passed.

## Next Action

Proceed to the RDM-002 plan and work package. Keep the `tinto-agent` availability model as an explicit implementation blocker unless the plan limits executable work to artifacts that do not depend on the launched command.
