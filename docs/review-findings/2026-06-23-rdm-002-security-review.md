---
title: RDM-002 security review
status: passed
roadmap_item: RDM-002
work_package: docs/work-packages/RDM-002-windows-wsl-agent-bootstrap-protocol/2026-06-23-002-wsl-agent-bootstrap-protocol-work-package.md
review_date: 2026-06-23
review_type: security
threshold: P0-P2
---

# RDM-002 Security Review

## Result

Passed. No remaining P0-P2 findings.

## Reviewed Security Boundary

RDM-002 introduces an internal process-launch seam for a future Windows host to start a Linux-side `tinto-agent` inside the selected Ubuntu WSL distro. The reviewed boundary is process construction, stdio handshake IO, safe error reporting, child cleanup, and absence of public WSL surfaces outside Windows.

## Findings

None at P0-P2 after fixes.

## Controls Verified

- No shell interpolation: the launcher builds an argument vector and the real transport uses `Command::new(program).args(args)`.
- Windows-only real transport: `StdCommandTransport` and spawn-error mapping are compiled only on `target_os = "windows"`.
- Single selected distro: command construction uses `wsl.exe -d Ubuntu -- <agent command>` for the initial smoke path.
- Dev-only availability model: the initial agent command is `cargo run --manifest-path <repo>/src-tauri/Cargo.toml --bin tinto-agent`; no installer, updater, copied binary, or persistence flow was added.
- Bounded protocol messages: handshake messages reject payloads over 64 KiB before JSON parsing.
- Timeout behavior: the host transport kills and waits on the child if startup handshake times out.
- Stderr handling: the host transport drops child stderr to avoid surfacing uncontrolled agent output in host errors.
- Safe error categories: missing WSL, missing distro, missing agent, spawn failure, timeout, protocol mismatch, malformed response, oversized response, and child exit are mapped to stable categories.
- No secret/env leakage: the new protocol sends only handshake type, protocol version, and client/agent versions.
- No public WSL command surface: no Tauri invoke command, frontend wrapper, UI state, settings, empty state, degraded notice, or Linux desktop WSL behavior was added.
- Unsupported-source guard: navigation aliases for unsupported WSL entries fail closed before local repo canonicalization.

## Verification

- `cargo test --lib wsl_agent`: 9 passed.
- `cargo test --lib unsupported -- --test-threads=1`: 8 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `cargo test --lib agent_console`: 36 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npx tsc --noEmit`: passed.
- New WSL Rust files passed `rustfmt --edition 2021 --check`; `src-tauri/src/bus/mod.rs` passed `rustfmt --edition 2021 --check --config skip_children=true`.

## Release Gate Note

Manual Windows/Ubuntu WSL smoke is intentionally deferred to the final batched release. The checklist is recorded at `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`.
