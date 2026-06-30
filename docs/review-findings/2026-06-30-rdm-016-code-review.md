---
title: RDM-016 Codex App Server Runtime Code Review
status: passed
date: 2026-06-30
work_package: docs/work-packages/RDM-016-codex-app-server-runtime/2026-06-30-016-codex-app-server-runtime-work-package.md
---

# RDM-016 Codex App Server Runtime Code Review

Review status: passed.

## Scope
- Codex app-server stdio adapter in `src-tauri/src/agent_console/app_server.rs`.
- `AgentProcessEvent` event draining and Agent Lens checkpoint closure integration.
- Existing PTY fallback behavior for non-Codex agents and app-server launch failures.
- Contract and Compound Master documentation for the runtime boundary.

## Findings
- No P0-P2 blocking findings remain.

## Notes
- The adapter uses `codex app-server --stdio`; no WebSocket listener or token-bearing transport is introduced.
- Unknown JSON-RPC notifications are ignored, app-server errors are surfaced as terminal output, and existing PTY marker parsing remains only for fallback agents.
- Review fixes applied before closeout: byte-buffered UTF-8 input handling, smaller app-server runtime context to satisfy `clippy`, collapsed quiet-monitor branching, and an explicit test-only allowance for the dummy child process used to obtain a `ChildStdin`.
- WSL Codex sessions intentionally remain on the existing PTY path for this package.

## Verification Evidence
- Work-package checker passed.
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1` passed: 55 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console::app_server -- --test-threads=1` passed: 5 tests.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1` passed: 47 tests.
- `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run` passed: 44 tests.
- `npm run build` passed with the existing chunk-size warning.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` passed.
- `git diff --check` passed with existing CRLF warnings only.
