---
title: RDM-005 Code And Security Review
status: passed
date: 2026-06-23
artifact: docs/work-packages/RDM-005-wsl-file-operations/2026-06-23-005-wsl-file-operations-work-package.md
review_type: direct-security-fallback
---

# RDM-005 Code And Security Review

## Result

Passed. No P0-P2 correctness or security findings remain.

## Reviewed Surfaces

- `src-tauri/src/file_ops/mod.rs`
- `src-tauri/src/file_ops/commands.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/wsl_agent/protocol.rs`
- `src-tauri/src/wsl_agent/runtime.rs`
- `src-tauri/src/wsl_agent/launcher.rs`
- `docs/contracts/bus-contract.md`

## Findings

- No open findings.

## Security Notes

- WSL file operations are routed through `tinto-agent` only after source-aware active-workbench resolution.
- Linux repo mutations use the same safe relative path joining helpers as local operations and reject `.git`.
- Drag/drop and paste source paths are translated from Windows host paths to WSL mount paths without shell interpolation.
- Export destinations are translated the same way and preserve local command semantics.
- WSL restore/redo tokens remain agent-side operations because backup manifests are created inside the Linux repo context.
- Gitleaks, media previews, Agent Console sessions, packaging/recovery, and fine-grained WSL `fs-events` are intentionally deferred.

## Verification Evidence

- `cargo test --lib file_ops`: passed.
- `cargo test --lib wsl_agent`: passed, 17 tests.
- `cargo test --lib bus -- --test-threads=1`: passed, 42 tests.
- `cargo build --bin tinto-agent`: passed.
- `npm test -- src/bus/contract.test.ts src/panels/tree/ProjectExplorer.test.tsx`: passed, 35 tests.
- `npx tsc --noEmit`: passed.
