---
title: Agent Turn Checkpoints Requirements
status: ready-for-planning
date: 2026-06-27
origin_roadmap: docs/roadmaps/2026-06-27-006-agent-turn-checkpoints-roadmap.md
---

# Agent Turn Checkpoints Requirements

## Summary
Tinto should add an Agent Lens for console sessions that explains changes as turn-based checkpoints. A turn closes only after conservative PTY and filesystem quiet periods, and a checkpoint is created only when files changed during that turn.

## Problem Frame
Today Tinto can show Git status, watched filesystem events, terminal output, and a session-level change log, but those surfaces blur two different questions: "what is changed in the repo?" and "what happened during this agent turn?" Users need a reviewable story for agent work without requiring agents to emit custom structured lifecycle events.

## Requirements
- R1. Tinto shall keep Git working-tree state separate from the Agent Lens; Git remains the global repo truth.
- R2. Tinto shall model an agent turn as a temporal window anchored to a console session, PTY output activity, user input, and repo file activity.
- R3. Tinto shall close a turn conservatively only after PTY output and filesystem activity have both stayed quiet long enough to avoid splitting one agent response into multiple checkpoints.
- R4. Tinto shall create a turn checkpoint only when files changed since the previous checkpoint boundary.
- R5. Tinto shall not create empty checkpoints for messages with no file changes.
- R6. Tinto shall include all file changes that occur during the turn window in that turn, even when Tinto cannot prove the agent authored them.
- R7. Tinto shall present turn checkpoints as "changes during turn" or equivalent wording, not as "agent-only changes".
- R8. Tinto shall allow reverting from a turn checkpoint by file, not by reverting the entire checkpoint as one operation.
- R9. Per-file revert shall be direct once the user chooses the file revert action; the product accepts the temporal attribution trade-off.
- R10. Local and WSL sessions shall preserve parity wherever existing checkpoint backends already provide parity.

## Key Decisions
- D1. Detection uses fallback signals only: PTY output quiet plus filesystem quiet. Agent-emitted semantic message events are out of scope because they are impractical across tools.
- D2. The detector prefers waiting longer over creating early checkpoints. Late checkpoints are better than fragmented turns.
- D3. Attribution is temporal. If a manual edit, formatter, or tool writes during the agent turn, the change belongs to the turn for review purposes.
- D4. Revert granularity is file-level. A checkpoint is the review boundary, but rollback is selected per file.

## Scope Boundaries
- In scope: active session status, turn state, changed-turn checkpoint list, diff/review affordances, and per-file revert from a checkpoint.
- Deferred: commit-from-turn, naming turns from model text, automatic PR descriptions, and richer agent quality summaries.
- Out of scope: perfect authorship attribution, semantic events emitted by Codex/Claude/opencode, and turn checkpoints for messages with no file changes.

## Acceptance Examples
- AE1. While an agent is producing output and editing files, the session shows a working state and no checkpoint is created yet.
- AE2. After output and file activity stay quiet past the conservative thresholds, a changed turn becomes a checkpoint.
- AE3. If a turn has no file changes, the session returns to waiting without adding a checkpoint row.
- AE4. If a file changes manually during the agent turn, that file appears in the turn checkpoint.
- AE5. A user can revert one file from a turn checkpoint without reverting the other files in that checkpoint.
- AE6. Existing Git status still shows all current repo changes, even when Agent Lens groups a subset by turn.

## Risks
- Conservative quiet detection may delay checkpoint visibility.
- Temporal attribution can include manual edits in an agent turn.
- Per-file revert touches destructive behavior and needs strong local/WSL containment tests.

## Open Questions
- None blocking. Planning should decide exact quiet thresholds, but the product direction is conservative.
