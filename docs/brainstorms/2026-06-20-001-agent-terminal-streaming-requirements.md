---
date: 2026-06-20
topic: agent-terminal-streaming
---

# Agent Terminal Streaming Requirements

## Summary

Add live PTY input/output bridging so a running agent session can be viewed and driven from a dockable terminal panel. The first version attaches to ephemeral backend sessions from ACI-001; launching affordances, checkpoints, revert, multi-agent orchestration, and resource limits stay in later roadmap items.

---

## Problem Frame

ACI-001 can start and stop allowlisted agent processes, but the app still has no way to see or interact with the PTY. Without streaming, the session lifecycle is useful only as backend plumbing and cannot support the intended "agent console" experience inside the project workspace.

---

## Requirements

**PTY stream contract**

- R1. The backend emits output chunks for a session with the session id and safe binary payload representation.
- R2. The frontend can send input bytes to a running session without shell expansion or command reconstruction.
- R3. The frontend can resize a running session with terminal cols and rows, and invalid dimensions are rejected deterministically.
- R4. Stream listeners can attach and detach without killing the session.

**Terminal panel**

- R5. A dockable terminal panel renders session output with an xterm-compatible terminal surface.
- R6. Keystrokes in the terminal panel are forwarded to the backend session input command.
- R7. Terminal resize uses a debounced fit measurement and propagates the measured cols and rows to the backend.
- R8. Closing a terminal panel detaches the view and leaves the backend session lifecycle unchanged.

**Workspace behavior**

- R9. Terminal panels use stable panel ids derived from session ids so repeated opens focus the existing panel.
- R10. Terminal panel layout participates in the existing dockview persistence rules without corrupting existing file or project tabs.

---

## Key Decisions

- **Binary-safe chunks:** Output chunks should travel as base64 bytes rather than assuming UTF-8. PTY streams can contain ANSI escapes and partial multibyte sequences, so xterm should receive decoded bytes at the panel boundary.
- **Session continues after detach:** Closing the panel is a UI action, not a process stop. Process lifecycle remains owned by explicit stop/revert flows.
- **No launcher yet:** This item creates attachable terminal capability. Repo card launch and binary availability UX belong to ACI-003.

---

## Actors

- A1. Local developer using Tinto to observe and guide coding-agent sessions.
- A2. Backend session registry managing PTY handles and lifecycle.
- A3. React workspace rendering dockable panels and preserving layout.

---

## Key Flows

- F1. Terminal attach
  - **Trigger:** A terminal panel opens for an existing session id.
  - **Actors:** A1, A2, A3
  - **Steps:** The panel subscribes to session output events, refreshes session metadata, creates an xterm surface, and writes incoming chunks to that surface.
  - **Covered by:** R1, R4, R5, R9

- F2. Interactive input
  - **Trigger:** The developer types in the terminal panel.
  - **Actors:** A1, A2, A3
  - **Steps:** The panel captures terminal data, sends it to the backend input command, and the PTY process receives bytes on stdin.
  - **Covered by:** R2, R6

- F3. Terminal resize
  - **Trigger:** The dock panel size changes or attaches for the first time.
  - **Actors:** A2, A3
  - **Steps:** The panel measures fit dimensions, debounces updates, and asks the backend to resize the PTY.
  - **Covered by:** R3, R7

---

## Acceptance Examples

- AE1. **Covers R1, R5.** Given a running session emits `hello`, when the terminal panel is attached, then `hello` appears in the xterm surface without requiring a page refresh.
- AE2. **Covers R2, R6.** Given a running echo-like session, when the user types `test`, then the backend input command receives the bytes and echoed output appears in the panel.
- AE3. **Covers R3, R7.** Given the terminal panel is resized, when fit dimensions settle, then the backend receives one resize command with the current cols and rows.
- AE4. **Covers R4, R8.** Given a session is running, when its terminal panel closes, then the session remains listed as running until explicitly stopped or exited.
- AE5. **Covers R9, R10.** Given a terminal panel already exists for a session, when the app opens that session again, then the existing panel is focused rather than duplicating layout state.

---

## Scope Boundaries

- ACI-003 owns repo-card launch UI and binary availability messaging.
- ACI-004 owns checkpoints, change logs, audit trails, and revert.
- ACI-005 owns auto-splitting decisions across multiple concurrent terminal panels.
- ACI-006 owns resource limits, telemetry, and lifetime caps.

---

## Dependencies / Assumptions

- ACI-001 session lifecycle commands and in-memory registry are available on `develop`.
- Terminal sessions remain ephemeral across app restarts for this item.
- The first stream bridge can use Tauri events rather than a durable replay buffer; sessions that produced output before a panel attaches do not need historical replay in this item.
