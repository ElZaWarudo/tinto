---
title: Agent Turn Checkpoints Roadmap
status: active
date: 2026-06-27
source_docs:
  - docs/contracts/bus-contract.md
  - docs/brainstorms/2026-06-20-001-agent-terminal-streaming-requirements.md
  - docs/brainstorms/2026-06-20-002-agent-launcher-ui-requirements.md
  - docs/brainstorms/2026-06-23-009-wsl-agent-console.md
  - docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md
---

# Agent Turn Checkpoints Roadmap

## Context Sufficiency Summary
- Product intent is sufficient: the user wants Tinto to separate global Git changes from changes that happened during an open agent console session, using conservative PTY/filesystem quiet detection rather than impractical agent-emitted semantic events.
- Current system shape is sufficient: Agent Console sessions already have lifecycle, PTY output, checkpoint records, change logs, local/WSL checkpoint backends, and explicit revert flows in the bus contract.
- Delivery context is sufficient: the repo uses `develop`, direct local integration by standing preference, optional Jira, and existing frontend/Rust verification gates.

## Source Inventory
| Source | Contribution | Confidence |
|---|---|---|
| User dialogue, 2026-06-27 | Defines Agent Lens as session-turn checkpoints, conservative fallback detection, checkpoints only when files changed, and per-file revert from a checkpoint. | High |
| `docs/contracts/bus-contract.md` | Current AgentSession, output, change-log, checkpoint, and revert contract. | High |
| `docs/brainstorms/2026-06-20-001-agent-terminal-streaming-requirements.md` | Establishes PTY output/input as the terminal surface and defers checkpoint/audit concerns to later work. | Medium |
| `docs/brainstorms/2026-06-20-002-agent-launcher-ui-requirements.md` | Establishes `start_agent_session` launch flow and terminal focus behavior. | Medium |
| `docs/brainstorms/2026-06-23-009-wsl-agent-console.md` | Establishes WSL Agent Console parity expectations and stable additive session contract. | High |
| `docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md` | Establishes local/WSL checkpoint and revert parity, with destructive revert handled honestly. | High |

## Roadmap Items
- RDM-015. **Agent turn checkpoints**
  - Outcome: Tinto shows an Agent Lens for an open console session, grouping file changes into conservative turn checkpoints so users can understand what happened during each agent turn separately from global Git state.
  - Why now: current Git status, session change-log, and terminal output surfaces exist, but they are not explained as batches of work and do not map cleanly to the agent's visible turns.
  - Scope boundary: include PTY/filesystem quiet detection, changed-turn checkpoint history, clear Git-vs-agent presentation, and per-file revert from a checkpoint; exclude agent-emitted semantic events, empty checkpoints, and perfect authorship detection.
  - Hard depends on: None.
  - Soft sequencing preference: preserve existing session checkpoint/revert contract behavior while adding turn-level behavior additively.
  - Blocks/enables: enables future agent review, commit-from-turn, and richer session audit workflows.
  - Risk: high because checkpoint/revert is destructive when invoked and spans local plus WSL session backends.
  - Expected brainstorm: `docs/brainstorms/2026-06-27-015-agent-turn-checkpoints.md`
  - Expected plan: `docs/plans/2026-06-27-015-agent-turn-checkpoints-plan.md`
  - Suggested package: split by backend/session model and UI only if implementation grows; otherwise one integrated review unit with strong tests.

## Dependency Graph
- RDM-015 has no hard predecessor beyond the already-shipped Agent Console and checkpoint capabilities.

## Parallelization Waves
- Wave 1: RDM-015 Agent turn checkpoints.

## Branch and PR Strategy
| Package candidate | Base branch | PR type | Dependency | Notes |
|---|---|---|---|---|
| Agent turn checkpoints | `develop` | review-unit | None | One semantic branch is preferred unless implementation discovers separable backend/UI risk. |

## Blockers and User Decisions
- No blockers.
- Confirm during planning whether per-file revert should restore a file to its prior checkpoint content or remove it when it was created during the turn; default expectation is exact per-file rollback to that checkpoint boundary.
