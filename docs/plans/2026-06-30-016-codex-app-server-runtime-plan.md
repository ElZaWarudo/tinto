---
title: Codex App Server Runtime Plan
status: ready
date: 2026-06-30
origin: docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md
---

# Codex App Server Runtime Plan

## Problem Frame
Codex has a structured programmatic interface that already emits turn lifecycle and change events. Tinto should use that interface for Codex sessions while preserving the generic PTY/fallback path for other agents.

## Scope
Implement an app-server-backed Codex runtime path that can start chat turns, stream output, receive turn/change notifications, and close Agent Lens checkpoints through structured lifecycle events.

## Non-goals
- Removing terminal-backed agents.
- Implementing OpenCode or Claude adapters in this package.
- Committing or PR generation from a turn.
- Full chat redesign separate from the existing Agents surface.
- Generated app-server schema artifacts checked into the repo.

## Key Technical Decisions
- KTD1. Introduce a generic runtime/event boundary in backend code before adding Codex-specific app-server transport.
- KTD2. Implement app-server JSON-RPC over stdio first. Keep WebSocket/unix socket support out of scope.
- KTD3. Parse app-server messages tolerantly with `serde_json::Value` plus small typed helpers for required fields.
- KTD4. Map `turn/started`, `turn/completed`, `turn/diff/updated`, `item/fileChange/patchUpdated`, and `fs/changed` into Tinto session activity.
- KTD5. On `turn/completed`, ask the existing session checkpoint scanner to close the turn immediately and create a follow-up checkpoint when changes exist.
- KTD6. Use app-server `fs/watch` as an activity accelerator, not as the final source of changed-file truth.
- KTD7. Keep WSL and unavailable-app-server cases on the existing PTY path until a dedicated WSL app-server strategy is proven.

## Implementation Units
- U1. Runtime boundary and session model
  - Add a backend abstraction for structured agent events that can be produced by Codex app-server now and other adapters later.
  - Extend session records with optional runtime metadata such as runtime kind, external thread id, external turn id, and structured diff/activity hints.
- U2. Codex app-server transport
  - Spawn `codex app-server --stdio`, initialize it with Tinto client info, start/resume a Codex thread, and start turns with repo `cwd`.
  - Add a small JSON-RPC request/response/notification loop with timeout/error handling.
- U3. Event mapping and checkpoints
  - Convert app-server turn and filesystem notifications into session activity.
  - Use `turn/completed` to force-close the current turn checkpoint through the existing checkpoint scanner.
  - Preserve quiet-time/marker fallback for PTY sessions.
- U4. Frontend chat/input wiring
  - Let Codex sessions send user chat text through app-server-backed input while preserving terminal-backed input for other agents.
  - Display app-server streamed text/output in the existing session output area with minimal UI changes.
- U5. Tests and documentation
  - Add Rust unit tests for app-server message parsing and event-to-session mapping.
  - Add frontend tests for runtime metadata and Codex-backed send behavior where practical.
  - Update orchestration/contract docs with the new Codex runtime path.

## Expected Touch Surface
- `src-tauri/src/agent_console/*`
- `src-tauri/src/bus/contract.rs`
- `src-tauri/src/lib.rs`
- `src/bus/contract.ts`
- `src/bus/client.ts`
- `src/agent/sessionStore.ts`
- `src/panels/terminal/TerminalPanel.tsx`
- `docs/contracts/bus-contract.md`

## Test Scenarios
- T1. App-server parser accepts initialize responses and ignores unknown notifications.
- T2. `turn/started` marks the session turn as working and records external ids.
- T3. `fs/changed` records activity without creating an immediate empty checkpoint.
- T4. `turn/completed` force-closes the current turn and creates a changed checkpoint when files changed.
- T5. App-server text deltas append to the existing session output stream.
- T6. If app-server fails to launch, Codex can fall back to the existing PTY path.
- T7. Non-Codex agents still use the existing PTY/marker/quiet-time path.

## Verification
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run`
- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `git diff --check`

## Risks
- App-server process loops can deadlock if stdin/stdout handling is not threaded carefully.
- Experimental app-server schema can drift; tests should cover tolerant parsing.
- Chat-first UX may need a later dedicated component; this package should not overbuild UI.

## Open Questions
- Exact visual treatment for chat-vs-terminal can be refined after the runtime is working.
- WSL app-server strategy remains a follow-up unless local evidence proves it is safe inside this package.
