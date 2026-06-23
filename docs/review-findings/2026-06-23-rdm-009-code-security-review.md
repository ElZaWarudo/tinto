---
title: RDM-009 WSL Agent Console code/security review
status: passed
date: 2026-06-23
work_package: docs/work-packages/RDM-009-wsl-agent-console/2026-06-23-009-wsl-agent-console-work-package.md
review_unit: RU1
threshold: P0-P2
---

# RDM-009 WSL Agent Console code/security review

## Scope Reviewed
- Source-aware `start_agent_session` routing for local versus WSL repos.
- WSL PTY command construction and agent availability checks.
- Session checkpoint representation and revert behavior for no-checkpoint sessions.
- Frontend repo-aware availability checks and Terminal Revert affordance.
- Contract documentation for Agent Console WSL behavior.

## Findings
- No blocking P0-P2 findings remain.

## Security Notes
- WSL launch uses `wsl.exe -d Ubuntu -- sh -lc 'cd "$1" || exit 127; shift; exec "$@"' ...`; repo path and agent id are passed as argv instead of interpolated into the shell script.
- Agent ids are validated against the allowlist before WSL availability checks and before WSL launch.
- WSL sessions do not expose a fake host checkpoint. They return `checkpoint: null`, the UI disables Revert, and direct backend revert returns `checkpoint_unsupported`.
- Existing local host checkpoint/revert behavior remains unchanged.

## Residual Advisory
- Remote WSL checkpoint/revert should be implemented as a follow-up hardening package before the product claims full rollback parity for WSL Agent Console sessions.

## Verification
- `cargo test --lib agent_console`: passed 41/41.
- `cargo test --lib invoke_handler`: passed 1/1.
- `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/DashboardPanel.test.tsx src/panels/terminal/TerminalPanel.test.tsx`: passed 47/47.
- `npx tsc --noEmit`: passed.
- `git diff --check`: passed with CRLF warnings only.
