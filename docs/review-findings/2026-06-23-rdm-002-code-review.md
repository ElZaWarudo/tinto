---
title: RDM-002 code review
status: passed
roadmap_item: RDM-002
work_package: docs/work-packages/RDM-002-windows-wsl-agent-bootstrap-protocol/2026-06-23-002-wsl-agent-bootstrap-protocol-work-package.md
review_date: 2026-06-23
review_type: code
threshold: P0-P2
---

# RDM-002 Code Review

## Result

Passed. No remaining P0-P2 findings.

## Reviewed Scope

- `src-tauri/src/wsl_agent/protocol.rs` adds the line-delimited JSON handshake DTOs, protocol/version constants, bounded encode/parse helpers, compatibility checks, and safe error categories.
- `src-tauri/src/wsl_agent/launcher.rs` adds the internal WSL launcher seam, argument-vector construction, dev-source Ubuntu command shape, mocked transport tests, and a Windows-only real process transport.
- `src-tauri/src/bin/tinto-agent.rs` adds the Linux-side handshake skeleton.
- `src-tauri/src/lib.rs` exposes the internal `wsl_agent` module without registering a Tauri invoke command.
- `src-tauri/src/bus/mod.rs` tightens unsupported-source resolution so navigation aliases cannot canonicalize an unsupported WSL entry into a local repo command path.
- `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md` records the Windows/Ubuntu WSL smoke procedure for the final release gate.

## Findings

None at P0-P2 after fixes.

## Fixes Made During Review

- The real `StdCommandTransport` is now `#[cfg(target_os = "windows")]`, keeping the `wsl.exe` process-launch implementation out of non-Windows builds while preserving mockable protocol and command-shape tests.
- The bus resolver now checks unsupported navigation aliases before canonicalizing to local paths. This prevents paths such as `repo\..\repo` from bypassing unsupported-source guards when the unsupported entry is WSL-owned.

## Impact Scan

Changed internal surfaces:
- `wsl_agent::protocol`
- `wsl_agent::launcher`
- `tinto-agent` binary
- `bus::resolve_repo_for_command` unsupported-source guard behavior

No public Tauri command, frontend wrapper, UI entrypoint, settings surface, or contract document change was introduced for WSL launching.

Consumer scan pattern used:

```powershell
rg "wsl_agent|tinto_agent|tinto-agent|invoke_handler|RepoSource|UnsupportedRepoSource|agent_console|CommandError|unsupported_entry_matches_request|normalize_path_lexically" src-tauri/src src docs/contracts
```

Consumers found:
- `src-tauri/src/lib.rs`
- `src-tauri/src/bin/tinto-agent.rs`
- `src-tauri/src/wsl_agent/*`
- `src-tauri/src/bus/mod.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/agent_console/commands.rs`
- `src/workbench/wslAbsence.test.ts`
- existing workbench `RepoSource` tests

## Verification

- `cargo test --lib wsl_agent`: 9 passed.
- `cargo build --bin tinto-agent`: passed.
- Local binary smoke with compatible handshake: returned `{"type":"handshake","protocol_version":1,"agent_version":"0.1.0","status":"ok"}`.
- Local binary smoke with incompatible protocol: returned safe `protocol_mismatch`.
- `cargo test --lib unsupported -- --test-threads=1`: 8 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `cargo test --lib agent_console`: 36 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npx tsc --noEmit`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.
- `rustfmt --edition 2021 --check src\wsl_agent\mod.rs src\wsl_agent\protocol.rs src\wsl_agent\launcher.rs src\bin\tinto-agent.rs`: passed.
- `rustfmt --edition 2021 --check --config skip_children=true src\bus\mod.rs`: passed.
- Work package checker: passed with the accepted warning that the package mixes orchestration docs and runtime files, justified by RU1/RU2/RU3 split.

## Residual Risk

- Manual Windows/Ubuntu WSL smoke remains pending until the final batched release gate.
- Global `cargo fmt --check` is not currently a clean signal because of pre-existing unrelated formatting drift in `src-tauri/src/bus/secret_scan.rs`; touched Rust files were checked directly, and `bus/mod.rs` was checked with `skip_children=true` to avoid traversing that unrelated child module.
