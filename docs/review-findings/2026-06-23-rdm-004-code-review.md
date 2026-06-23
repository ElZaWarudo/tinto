---
title: RDM-004 Code Review
status: passed
date: 2026-06-23
artifact: docs/work-packages/RDM-004-core-wsl-read-watch-path/2026-06-23-004-core-wsl-read-watch-path-work-package.md
review_type: direct-compound-master-fallback
---

# RDM-004 Code Review

## Result

Passed for the implemented scope.

## Findings

- No P0-P2 blockers.
- Advisory: WSL activity currently refreshes repo deltas and subscribed diffs through bounded polling. Fine-grained WSL `fs-events` are intentionally deferred and documented as a hardening gap.

## Review Notes

- Local repos remain on the existing local watcher and git/filesystem path.
- WSL repos enter runtime snapshots on Windows as Linux-path identities.
- `resolve_repo` remains local-only, so mutation-oriented consumers keep rejecting WSL repos.
- `resolve_repo_identity` is the new read-routing path for WSL-aware read commands.
- WSL agent requests are typed, bounded by existing protocol framing, and carry an active repo allowlist.

## Verification Considered

- `cargo test --lib wsl_agent`: passed.
- `cargo test --lib bus -- --test-threads=1`: passed.
- `cargo test --lib invoke_handler`: passed.
- `cargo build --bin tinto-agent`: passed.
- Targeted frontend contract/store/absence tests and TypeScript typecheck: passed.
- `git diff --check`: passed with CRLF warnings only.
