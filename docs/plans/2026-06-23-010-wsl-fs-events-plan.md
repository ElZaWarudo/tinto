---
title: WSL filesystem events plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-010
origin_brainstorm: docs/brainstorms/2026-06-23-010-wsl-fs-events.md
production_posture: prototype
---

# WSL filesystem events plan

## U1 - Protocol fingerprint DTO
Add a serializable fingerprint DTO and request/response pair to `tinto-agent`: repo path, active allowlist, and returned relative files with size plus modified timestamp.

Acceptance:
- Unsupported or outside-workbench repos return the existing safe agent error shape.
- Protocol validation includes the new request variant.

## U2 - Agent-side scan
Implement a Linux-side file walker that respects ignore behavior, skips `.git`, follows no symlinks, caps entries, and returns deterministic fingerprints.

Acceptance:
- Created/modified/removed detection has stable input ordering.
- Scan does not read file contents.

## U3 - Bus diff and event emission
Store previous WSL fingerprints in `RepoLiveState`, compare new snapshots on WSL poll results, emit `EVENT_FS_EVENTS` for non-empty batches, and then continue applying the existing `RepoDelta`.

Acceptance:
- First snapshot primes without emitting.
- Later snapshots emit created/modified/removed batches with size and size_delta where known.
- Local `fs_events` path remains untouched.

## U4 - Protocol size and docs
Raise the agent message guard enough for bounded media/fingerprint responses and document WSL event behavior in the bus contract.

Acceptance:
- Existing protocol tests still pass.
- Docs distinguish agent-side event batching from future native WSL inotify streaming.

## U5 - Verification and review
Run WSL agent, bus, frontend store/contract if affected, typecheck, and diff hygiene. Record a code/security review because this expands remote filesystem enumeration.

Acceptance:
- Targeted tests pass.
- Work package/state/review findings are current.
