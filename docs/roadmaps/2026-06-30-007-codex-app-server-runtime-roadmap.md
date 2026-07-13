---
title: Codex App Server Runtime Roadmap
status: delivered
date: 2026-06-30
source_docs:
  - docs/roadmaps/2026-06-27-006-agent-turn-checkpoints-roadmap.md
  - docs/plans/2026-06-27-015-agent-turn-checkpoints-plan.md
  - docs/work-packages/RDM-015-agent-turn-checkpoints/2026-06-27-015-agent-turn-checkpoints-work-package.md
---

# Codex App Server Runtime Roadmap

## Context Sufficiency Summary
- Product intent is sufficient: Tinto should make Codex the first high-fidelity agent integration by using `codex app-server` for chat, turn lifecycle, streamed output, and change signals instead of relying on terminal markers.
- Technical evidence is sufficient: the installed Codex app-server schema exposes `turn/started`, `turn/completed`, `turn/diff/updated`, `item/fileChange/patchUpdated`, and `fs/changed`; a local smoke test proved `fs/watch` emits `fs/changed` with exact changed paths.
- Architecture direction is sufficient: keep a replaceable runtime boundary so OpenCode and Claude can later implement the same Tinto turn/change contract through their own adapters or the terminal fallback.

## Source Inventory
| Source | Contribution | Confidence |
|---|---|---|
| User dialogue, 2026-06-30 | Requests a solid Codex implementation using app-server and replaceable pieces for OpenCode/Claude later. | High |
| Codex manual, Codex App Server section | Documents JSON-RPC protocol, transports, lifecycle, turn notifications, and schema generation. | High |
| Generated app-server schema from installed Codex | Confirms event/method shapes for turn, file-change patch, diff, and filesystem watch notifications. | High |
| Local app-server smoke test | Proves `fs/watch` produces `fs/changed` for local filesystem mutations without model turns. | High |
| RDM-015 artifacts | Existing Agent Lens/checkpoint model to preserve and improve. | High |

## Roadmap Items
- RDM-016. **Codex app-server runtime**
  - Outcome: Codex sessions in Tinto run through a Codex app-server adapter that exposes chat and drives Agent Lens turn checkpoints from app-server lifecycle/change events.
  - Why now: marker-based turn completion works as a fallback but is brittle for Codex, where app-server already provides semantic turn and change events.
  - Scope boundary: include Codex chat launch/start/stream/finish, adapter-owned app-server process, event mapping, filesystem watch integration, and checkpoint closure from `turn/completed`; keep terminal marker/quiet-time behavior as fallback for non-Codex agents.
  - Hard depends on: RDM-015 Agent turn checkpoints.
  - Soft sequencing preference: introduce replaceable runtime abstractions first, then wire Codex through them; avoid hard-coding Codex assumptions into generic session storage.
  - Blocks/enables: enables future OpenCode/Claude adapters, richer turn UI, and eventual removal of Codex marker instructions.
  - Risk: high because it changes Codex session orchestration, process management, and checkpoint timing.
  - Expected brainstorm: `docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md`
  - Expected plan: `docs/plans/2026-06-30-016-codex-app-server-runtime-plan.md`
  - Suggested package: one integrated review unit; the runtime adapter, chat path, event mapping, and checkpoint closure must be verified together.

## Dependency Graph
- RDM-016 depends on the RDM-015 session/checkpoint contract but should not remove the RDM-015 fallback detector.

## Parallelization Waves
- Wave 1: RDM-016 Codex app-server runtime.

## Branch and PR Strategy
| Package candidate | Base branch | PR type | Dependency | Notes |
|---|---|---|---|---|
| Codex app-server runtime | `develop` | review-unit | RDM-015 | One semantic branch is preferred because the adapter and checkpoint integration share core session state. |

## Blockers and User Decisions
- No blocker for local implementation.
- Shipping remains governed by the user's standing preference: direct local merge/push only when explicitly requested; no PR unless requested.
