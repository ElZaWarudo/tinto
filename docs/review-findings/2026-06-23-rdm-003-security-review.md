---
title: RDM-003 security review
status: passed
roadmap_item: RDM-003
work_package: docs/work-packages/RDM-003-windows-wsl-workbench-path-ux/2026-06-23-003-wsl-workbench-path-ux-work-package.md
review_date: 2026-06-23
review_type: security
threshold: P0-P2
---

# RDM-003 Security Review

## Result

Passed. No remaining P0-P2 findings.

## Reviewed Boundary

RDM-003 introduces user-provided WSL Linux paths and Windows-only WSL config commands. It does not introduce agent launch, process execution, WSL filesystem traversal, live repo monitoring, or remote file operations.

## Controls Verified

- Backend WSL path validation is lexical and rejects blank, relative, Windows drive, UNC-like, backslash, and `..` paths.
- Backend WSL distro validation accepts only `Ubuntu`.
- WSL config commands are registered only behind `#[cfg(target_os = "windows")]`.
- WSL config entries are visible on Windows but remain `is_runtime_supported() == false`, so the bus classifies them as unsupported and does not mount/watch/read them.
- Non-Windows frontend runtime does not render WSL controls.
- Visible Windows WSL UI only calls the isolated `add_wsl_repo` wrapper after frontend path validation.
- No `tinto-agent` launch command, shell command, environment access, secret handling, or filesystem canonicalization through `\\wsl$` was added.
- Project menu and live workspace surfaces still derive from bus snapshots, preventing configured WSL entries from opening project tabs before RDM-004.

## Verification

- `cargo test --lib workbench`: 33 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `npm test -- src/workbench/workbench.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts src/bus/contract.test.ts`: 109 passed.
- `npx tsc --noEmit`: passed.
- Targeted Rust format and Prettier checks passed.
- `git diff --check`: passed, with CRLF normalization warnings only.

## Release Gate Note

RDM-003 is configuration-only. Release notes must not claim live WSL monitoring; RDM-004 owns read/watch/event forwarding.
