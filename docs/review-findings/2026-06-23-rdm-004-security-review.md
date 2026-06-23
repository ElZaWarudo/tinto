---
title: RDM-004 Security Review
status: passed
date: 2026-06-23
artifact: docs/work-packages/RDM-004-core-wsl-read-watch-path/2026-06-23-004-core-wsl-read-watch-path-work-package.md
review_type: direct-security-fallback
---

# RDM-004 Security Review

## Result

Passed for the implemented scope.

## Reviewed Surfaces

- WSL agent protocol request/response parsing.
- Host WSL launch path.
- Bus source-aware repo resolver.
- WSL read command routing for diff/log/blob/file/tree.
- WSL snapshot/polling delta refresh.
- Local-only guard for mutation-oriented consumers.

## Findings

- No P0-P2 findings.

## Security Evidence

- No shell interpolation is introduced; WSL launch continues to use argument vectors.
- No `\\wsl$` traversal is introduced.
- WSL repo identities remain Linux paths and are not Windows-canonicalized.
- Agent-side handlers reject repo requests outside the provided allowlist.
- File reads reject `.git` and navigation paths inside the agent.
- `get_media_content`, file operations, Gitleaks, and Agent Console remain local-only/deferred for WSL repos because they still use `resolve_repo`.
- Agent/distro/path failures map to per-repo `RepoErrorState`, not global watcher degradation.

## Residual Risk

- Real Windows/Ubuntu WSL smoke is still required before the final batched release.
- Fine-grained WSL `fs-events` are deferred; current WSL tracking refreshes deltas and subscribed diffs by polling.
