---
title: Codex App Server Runtime Requirements
status: ready-for-planning
date: 2026-06-30
origin_roadmap: docs/roadmaps/2026-06-30-007-codex-app-server-runtime-roadmap.md
---

# Codex App Server Runtime Requirements

## Summary
Tinto should treat Codex as the first native IADE agent runtime. Instead of launching Codex only as a terminal process and asking it to print a marker, Tinto should run Codex through `codex app-server`, stream chat/events into the existing Agents UI, and close Agent Lens checkpoints from Codex turn lifecycle/change events.

## Problem Frame
The current Agent Lens implementation can close turns through quiet-time heuristics or a terminal marker. That is useful for arbitrary agents, but Codex app-server already exposes structured turn, item, diff, and filesystem events. Codex should use those events directly so Tinto can be more precise without losing the generic fallback path for other agents.

## Requirements
- R1. Tinto shall introduce a replaceable agent runtime boundary so Codex-specific app-server behavior does not leak into future OpenCode or Claude adapters.
- R2. Codex sessions shall be able to start a chat turn through app-server, using the selected repo as `cwd`.
- R3. Tinto shall stream Codex assistant text and command output into the existing session output/event model.
- R4. Tinto shall map `turn/started` and `turn/completed` to Agent Lens turn state without requiring a terminal marker.
- R5. Tinto shall capture Codex file-change evidence from app-server events such as `turn/diff/updated`, `item/fileChange/patchUpdated`, and/or `fs/changed`.
- R6. Tinto shall create or close turn checkpoints from app-server lifecycle events and then verify final changes through Tinto's existing checkpoint/change scanner.
- R7. Tinto shall keep marker/quiet-time detection available for non-Codex agents and as a Codex fallback if app-server cannot be launched.
- R8. The UI shall continue to present Codex as an agent session inside the Agents surface, not as a separate product mode.
- R9. The implementation shall preserve WSL/non-WSL boundaries. Windows Codex app-server support is first-class; WSL support may remain fallback if app-server cannot run inside the target environment yet.
- R10. The contract shall be additive and compatible with existing session store consumers.

## Key Decisions
- D1. Codex app-server is the preferred runtime for Codex, while PTY remains the fallback runtime for other agents.
- D2. Tinto remains the authority for checkpoints and final repo diffs. App-server events provide timing and evidence, not the only source of truth.
- D3. The first implementation should avoid generated schema dumps in the repo. Parse only the event fields Tinto needs and tolerate unknown fields.
- D4. Filesystem watch events are useful for fast activity signals, but final turn contents must come from checkpoint scanning.
- D5. App-server process management should be owned by the backend so frontend tabs are insulated from transport details.

## Acceptance Examples
- AE1. Starting a Codex session from Tinto creates an app-server-backed session and streams assistant text into the terminal/chat panel.
- AE2. When Codex emits `turn/completed`, Tinto closes the current Agent Lens turn without waiting for the marker.
- AE3. When a Codex turn edits files, the turn checkpoint appears after final checkpoint scanning with changed files.
- AE4. When app-server emits `fs/changed`, Tinto marks the turn active/settling without polling the whole repo every time.
- AE5. If app-server cannot start or is unsupported for an environment, Tinto can still run the existing terminal-backed Codex flow.
- AE6. The new runtime boundary can describe a future OpenCode or Claude adapter without renaming Codex-specific types everywhere.

## Risks
- App-server is documented as experimental and may change schema; parsing must be tolerant.
- Process lifetime and JSON-RPC backpressure can affect UI responsiveness.
- WSL Codex app-server availability may not match Windows app-server availability.
- Checkpoint creation remains destructive only when reverted, but timing mistakes can make Agent Lens confusing.

## Open Questions
- Should Codex app-server replace the xterm-style terminal display with a chat-first UI later? Not blocking; first cut can bridge events into existing session output.
- Should WSL Codex app-server run inside WSL or host Windows with remote cwd mapping? Not blocking; first cut can prefer Windows local repos and preserve WSL fallback.
