---
title: RDM-010 WSL fs-events code/security review
status: passed
date: 2026-06-23
work_package: docs/work-packages/RDM-010-wsl-fs-events/2026-06-23-010-wsl-fs-events-work-package.md
review_unit: RU1
threshold: P0-P2
---

# RDM-010 WSL fs-events code/security review

## Scope Reviewed
- WSL agent protocol additions for repo snapshots with file fingerprints.
- Linux-side fingerprint walking in `wsl_agent::runtime`.
- Bus-side fingerprint diffing and `EVENT_FS_EVENTS` emission.
- Contract documentation and message-size guard.

## Findings
- No blocking P0-P2 findings remain.

## Security Notes
- The fingerprint scan does not read file contents; it emits relative path, size, and modified timestamp only.
- `.git` internals are excluded and symlinks are not followed.
- Results are capped at the repo-tree entry guard and protected by the 20 MiB protocol message guard.
- Public `FsEventBatch` shape is unchanged, and local watcher behavior remains untouched.

## Residual Advisory
- A future native WSL inotify stream could reduce polling overhead, but the current package satisfies public event-contract parity without changing the one-shot agent model.

## Verification
- `cargo test --lib wsl_agent`: passed 25/25.
- `cargo test --lib bus -- --test-threads=1`: passed 43/43.
- `cargo build --bin tinto-agent`: passed.
- `npx tsc --noEmit`: passed.
- `git diff --check`: passed with CRLF warnings only.
