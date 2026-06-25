---
title: Terminal input regression handoff
date: 2026-06-25
status: resolved
initiative: post-closeout-terminal-input-regression
---

# Terminal Input Regression Handoff

## Summary

RESOLVED. The embedded terminal input regression was misdiagnosed as a focus/key-routing bug but
was actually an output-rendering bug. The frontend was correctly capturing and sending keyboard
input to the backend; the terminal simply stopped displaying new output (including the echo of
typed characters) once the output buffer filled up, which made it look like typing stopped working.

## Root cause

`AgentSessionStore.appendOutput` in `src/agent/sessionStore.ts` capped the per-session output
chunk array at 400 entries with `.slice(-400)`. `TerminalPanel` tracked written chunks using a
plain index (`writtenOutputRef`) into that sliding array:

```ts
for (let index = writtenOutputRef.current; index < output.length; index += 1) {
  terminal.write(decodeBase64(output[index].chunk_base64));
}
writtenOutputRef.current = output.length;
```

Once the array reached 400 chunks (≈3.2 MB at 8 KB/chunk), `.slice(-400)` kept the array length
at 400 while `writtenOutputRef` was already 400. The loop condition `index < output.length`
became `400 < 400` and **never executed again**, so every new chunk appended to the tail of the
array was silently dropped from rendering. Codex produces heavy output during startup, so the
buffer filled right as startup completed — making it look like the terminal died exactly when
Codex became ready.

The diagnostic overlay confirmed this: `onData: 214` (xterm was receiving keys), `textarea
focused`, `fallback: 0` (fallback never needed to fire). The input pipeline was healthy; only
the output pipeline was broken.

## Fix

- `src/agent/sessionStore.ts` — added `outputTotal: Record<string, number>` to
  `AgentSessionState`, incremented monotonically in `appendOutput` (never trimmed). Raised
  `MAX_OUTPUT_CHUNKS_PER_SESSION` from 400 to 20000 so a normal session no longer trims.
- `src/panels/terminal/TerminalPanel.tsx` — replaced the sliding-array index with a monotonic
  counter. `writtenOutputRef` now tracks the total number of chunks ever appended for this
  session; new chunks are written from `Math.max(0, sessionOutput.length - newChunkCount)`
  through the end of the current array, correctly handling any trimming that does occur.

Files changed in the fix:

- `src/agent/sessionStore.ts`
- `src/panels/terminal/TerminalPanel.tsx`

## Diagnostic overlay (added then removed)

A temporary in-app diagnostic overlay was added to `TerminalPanel` to confirm where keyboard
routing died. It showed `document.activeElement`, `textarea` focus equality, `api.isActive`,
window focus/blur, `xterm.onData` fire count, and global `keydown` fallback fire count. The
overlay evidence (`onData: 214`, `textarea focused`, `fallback: 0`) proved the input pipeline
was healthy and redirected the investigation to the output pipeline. The overlay and its CSS
were removed after the fix was confirmed by the user.

## User confirmation

- After the fix (monotonic counter + raised cap), the user confirmed typing works after Codex
  startup: "Bien echo chico".
- A small residual of garbled characters was reported with the original 400 cap; raising the cap
  to 20000 resolved that too (the garble was from chunks lost during trimming at the 400
  boundary).

## Verification

- `npm test -- TerminalPanel.test.tsx --run` — 18/18 passed.
- `npm test -- src/bus/connection.test.ts src/bus/contract.test.ts src/agent --run` — 26/26 passed.
- `npx tsc --noEmit` — clean.
- `npx prettier --check` on changed files — clean.
- `npm run build -- --mode development` — built successfully.
- Manual `tauri dev` with Codex Agent Console — user confirmed typing works after startup.

## Pre-existing mitigations retained

These focus/input mitigations were added during the investigation and remain in place as
defensive hardening even though they were not the root cause:

- Stop backend terminal sessions when the terminal panel closes (with StrictMode remount delay).
- Track panel activation through Dockview `api.onDidActiveChange`.
- Explicit `focusTerminal()` (activate panel → focus surface → `terminal.focus()` → focus textarea).
- Custom key handling only for paste shortcuts; ordinary keys flow through native xterm.
- Fallback global `keydown` and `paste` capture listeners when the terminal tab is active but the
  `textarea` is not the effective target.
- Re-focus after terminal output writes when the panel is active.
- Stabilize the hidden `xterm` helper `textarea` (nonzero dimensions, transparent style,
  `pointer-events: none`).
- Blur recovery when focus falls into no meaningful control.
