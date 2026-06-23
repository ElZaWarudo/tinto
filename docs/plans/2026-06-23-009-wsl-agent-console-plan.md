---
title: WSL Agent Console parity plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-009
origin_brainstorm: docs/brainstorms/2026-06-23-009-wsl-agent-console.md
production_posture: prototype
---

# WSL Agent Console parity plan

## U1 - Source-aware session start
Route `start_agent_session` through `resolve_repo_identity`. Local repos continue using the current canonical local PTY path. WSL repos use their resolved Linux path and distro identity to start a WSL PTY command.

Acceptance:
- WSL repo start no longer maps to `unsupported_repo_source`.
- Local repo start still rejects unknown repos and unsupported sources through safe categories.

## U2 - WSL PTY launch
Extend the PTY factory with a WSL-specific spawn path. It should execute `wsl.exe -d Ubuntu -- sh -lc ...`, `cd` to the Linux repo path, then `exec` the selected allowlisted agent with existing agent-specific flags such as Codex `--no-alt-screen`.

Acceptance:
- Shell interpolation is limited to a static script; repo and agent values are passed as argv.
- The host environment remains sanitized.
- Process output/input/resize/kill reuse the existing PTY handle behavior.

## U3 - Repo-aware availability
Add an additive repo-aware availability command/client wrapper. Local repos use host binary resolution. WSL repos check `command -v` inside Ubuntu. Repo cards call the repo-aware wrapper so a Windows-host miss does not block a valid Ubuntu launch.

Acceptance:
- Existing global `agent_binary_available(agent_type)` remains host-scoped for compatibility.
- Frontend tests prove repo cards pass the repo path when checking availability.

## U4 - Checkpoint/revert honesty
Keep local checkpoint/revert behavior unchanged. For WSL, either implement remote checkpoint/revert through the Linux side or store sessions without a checkpoint and disable/reject revert with a clear `checkpoint_unsupported` category.

Acceptance:
- WSL sessions never expose a fake local checkpoint.
- The Terminal panel does not offer Revert when the session has no checkpoint.
- Contract docs state current WSL behavior precisely.

## U5 - Verification and review
Run targeted Rust tests for `agent_console`, invoke handler coverage, frontend contract/repo-card/terminal tests, typecheck, and diff hygiene. Record security review because the package launches external processes and touches repo mutation safeguards.

Acceptance:
- Targeted tests pass.
- Code/security review finding file is recorded.
- State and package artifacts reflect implementation status.
