---
title: RDM-001 RU2/RU3 code review findings
status: passed-after-fixes
date: 2026-06-23
review_unit: RDM-001 RU2/RU3
package: docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md
threshold: P0-P2
---

# RDM-001 RU2/RU3 code review findings

## Result

Review passed after fixes. No remaining P0-P2 findings are known for RU2/RU3.

## Reviewers

- Correctness reviewer: read-only subagent.
- Security reviewer: read-only subagent.
- Testing reviewer: read-only subagent.
- Lead synthesis/re-review: direct inspection after fixes.

## Findings Fixed

### P2 - Repo-scoped bus commands bypassed unsupported-source resolution

Files: `src-tauri/src/bus/mod.rs`.

Finding: `Subscribe` and `RetryRepo` canonicalized their raw repo paths directly instead of using the new source-aware resolver. A hidden future WSL entry shaped like `local_repo/.` could canonicalize into a mounted local repo before the unsupported source check.

Fix: `Subscribe` now resolves each target through `resolve_repo_for_command` and keeps only supported local targets. `RetryRepo` resolves through the same guard before remount/recalc.

Verification:
- `unsupported_wsl_subscription_does_not_canonicalize_into_local_repo`
- `unsupported_wsl_retry_does_not_canonicalize_into_local_repo`
- `cargo test --lib unsupported_wsl`

Re-review: correctness reviewer confirmed the original P2 is resolved and found no remaining P0-P2 issues.

### P1 - Hidden WSL fixture tests missed retry/subscription paths

Files: `src-tauri/src/bus/mod.rs`.

Finding: WSL guard tests covered snapshot, `is_known`, and `resolve_repo`, but not repo-scoped `retry_repo` and `subscribe` paths.

Fix: added exact hidden WSL `retry_repo`/`subscribe` assertions and canonicalization-regression tests for unsupported aliases that would previously resolve into local repos.

Verification:
- `wsl_source_is_not_mounted_and_resolves_as_unsupported`
- `unsupported_wsl_subscription_does_not_canonicalize_into_local_repo`
- `unsupported_wsl_retry_does_not_canonicalize_into_local_repo`

Re-review: testing reviewer confirmed the P1 is resolved.

### P2 - Frontend absence test covered only a hand-maintained subset

Files: `src/workbench/wslAbsence.test.ts`.

Finding: the first absence test scanned nine manually listed frontend files, which could miss future runtime UI/settings/empty-state files.

Fix: replaced the hand-maintained list with `import.meta.glob("../**/*.{ts,tsx}", { eager: true, import: "default", query: "?raw" })`, excluding tests and declarations. The test now scans every non-test, non-declaration frontend runtime TS/TSX source under `src`.

Verification:
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npx tsc --noEmit`: passed.

Re-review: testing reviewer confirmed the P2 is resolved.

## Security Review

Security reviewer found no P0-P2 findings.

Reviewed surfaces:
- `src-tauri/src/bus/mod.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/agent_console/commands.rs`
- shared `ensure_known` consumers such as `src-tauri/src/file_ops/commands.rs`
- `src-tauri/src/lib.rs`
- `docs/contracts/bus-contract.md`

Security result:
- Unsupported future WSL entries are not mounted, watched, or exposed in snapshots.
- Repo command guards fail closed for unsupported sources before local path handling.
- Error messages use safe categories and do not include unsupported repo paths.
- No WSL launch/probe or `tinto-agent` registration was added.

## Verification Summary

- `cargo test --lib wsl_source`: 3 passed.
- `cargo test --lib unsupported_wsl`: 2 passed.
- `cargo test --lib unsupported_repo_resolve_error_maps_to_safe_category`: 2 passed.
- `cargo test --lib invoke_handler_does_not_register_wsl_commands_for_rdm_001`: 1 passed.
- `cargo test --lib initial_runtime_repos`: 2 passed.
- `cargo test --lib -- --test-threads=1`: 179 passed.
- `cargo fmt --check`: passed.
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/workbench/wslAbsence.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`: 140 passed.
- `npx tsc --noEmit`: passed.

## Residual Risk

No remaining P0-P2 risk is known for RDM-001. Later RDM packages still own actual Windows WSL agent bootstrap, Linux-side read/watch routing, and capability policy decisions.
