---
title: RDM-011 code and security review
status: passed
date: 2026-06-23
package: docs/work-packages/RDM-011-wsl-agent-console-checkpoints/2026-06-23-011-wsl-agent-console-checkpoints-work-package.md
review_unit: RU1
---

# RDM-011 code and security review

## Result
Passed. No remaining P0-P2 findings after implementation review.

## Scope Reviewed
- `src-tauri/src/agent_console/checkpoint.rs`
- `src-tauri/src/agent_console/mod.rs`
- `src-tauri/src/agent_console/session.rs`
- `src-tauri/src/wsl_agent/protocol.rs`
- `src-tauri/src/wsl_agent/runtime.rs`
- `docs/contracts/bus-contract.md`

## Checks
- WSL Agent Console start creates a checkpoint through `tinto-agent` before spawning the PTY in production.
- WSL scan/revert uses Linux checkpoint paths and routes through `tinto-agent`; local sessions keep the existing in-process checkpoint implementation.
- Agent-side checkpoint create, scan, and revert are constrained by an explicit allowed repo list.
- Destructive revert remains gated by `revert_session(..., user_consent=true)` and rejects running sessions.
- `checkpoint: null` remains handled for legacy/fallback sessions with `checkpoint_unsupported` and disabled UI affordance.

## Verification
- `cargo test --lib wsl_agent` passed 26/26.
- `cargo test --lib agent_console` passed 41/41.

## Residual Risk
Real Windows + Ubuntu WSL smoke remains required in the final packaging/smoke package because this environment only validates the one-shot agent protocol and registry behavior with unit tests.
