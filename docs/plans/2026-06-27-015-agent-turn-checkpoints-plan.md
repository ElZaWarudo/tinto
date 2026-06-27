---
title: Agent Turn Checkpoints Plan
status: ready
date: 2026-06-27
origin: docs/brainstorms/2026-06-27-015-agent-turn-checkpoints.md
---

# Agent Turn Checkpoints Plan

## Problem Frame
Agent Console sessions already expose PTY output, session status, checkpoints, and change logs. The missing layer is a turn-oriented Agent Lens that batches file changes by conservative console activity windows and supports per-file rollback from those batch boundaries.

## Scope
Implement turn checkpoint tracking for Agent Console sessions across local and WSL repos, expose it through the additive session contract, and render it clearly in the terminal/session UI. Keep Git status and global repo diffs separate from the Agent Lens.

## Non-goals
- Agent-emitted semantic message events.
- Empty checkpoints for turns with no file changes.
- Perfect authorship attribution.
- Commit-from-turn or PR generation.

## Key Technical Decisions
- KTD1. Turn detection is local and conservative: output activity and filesystem/checkpoint scan activity hold a turn open; quiet timers close it.
- KTD2. Turn checkpoint records are additive to existing session checkpoint behavior and should not break existing session-level revert.
- KTD3. Revert is file-level from a selected turn checkpoint. Whole-turn revert is not part of this plan.
- KTD4. UI copy must frame the batch as "changes during turn" to avoid claiming exact agent authorship.

## Implementation Units
- U1. Session turn model and detector
  - Add a session-owned turn state that tracks activity windows, quiet/settling/waiting states, and changed file detection since the previous checkpoint boundary.
  - Preserve existing lifecycle states while adding enough metadata for the UI to distinguish working, settling, waiting, and finished.
- U2. Turn checkpoint storage and scan
  - Create turn checkpoint records only when changes exist.
  - Store enough baseline information to compute the files changed during each turn and support per-file revert.
  - Preserve local and WSL checkpoint containment guarantees.
- U3. Per-file revert command
  - Add an explicit per-file revert path from a turn checkpoint.
  - Reject paths outside the checkpoint's repo and keep `.git` internals protected.
  - Keep existing session-level revert behavior compatible unless deliberately deprecated later.
- U4. Frontend Agent Lens presentation
  - Show current turn state, changed-turn checkpoints, changed files per turn, and per-file revert affordances.
  - Separate Agent Lens from Git working-tree state in labels and layout.
- U5. Tests and contract documentation
  - Update backend contract docs and TypeScript mirrors.
  - Add Rust tests for turn detection, checkpoint creation/no-op, local per-file revert, and WSL protocol routing where applicable.
  - Add frontend tests for state labels, no-empty-checkpoint behavior, and per-file revert action.

## Expected Touch Surface
- `src-tauri/src/agent_console/session.rs`
- `src-tauri/src/agent_console/checkpoint.rs`
- `src-tauri/src/agent_console/commands.rs`
- `src-tauri/src/wsl_agent/protocol.rs`
- `src-tauri/src/wsl_agent/runtime.rs`
- `src-tauri/src/bus/contract.rs`
- `src/bus/contract.ts`
- `src/agent/sessionStore.ts`
- `src/panels/terminal/TerminalPanel.tsx`
- `docs/contracts/bus-contract.md`

## Test Scenarios
- T1. A running session with output and file changes stays working until both output and file activity settle.
- T2. A turn with changed files creates exactly one checkpoint after quiet detection.
- T3. A turn with output but no file changes creates no checkpoint.
- T4. A late file change during settling reopens or extends the turn instead of creating two checkpoints.
- T5. Per-file revert restores a modified file to the selected checkpoint boundary.
- T6. Per-file revert removes a file that was created after the selected checkpoint boundary.
- T7. WSL per-file revert routes through `tinto-agent` and rejects repos outside the allowlist.
- T8. The frontend displays Agent Lens checkpoints separately from Git status and does not label them as agent-only changes.

## Verification
- `npm test -- src/panels/terminal/TerminalPanel.test.tsx src/agent/sessionStore.test.ts`
- `npm test -- src/bus/contract.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib agent_console`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib wsl_agent`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus`
- `npm run lint`
- `npm run build`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Risks
- Per-file revert is destructive and needs focused containment tests.
- Quiet detection can be flaky if tests depend on wall-clock timing; prefer injectable clocks or deterministic state-machine tests where practical.
- WSL parity can expand protocol surface; keep payloads additive and guarded.

## Open Questions
- Exact quiet thresholds are execution-time tuning, not a product blocker. Start conservatively and expose constants or test fixtures rather than hard-coding behavior across tests.
