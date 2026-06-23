---
title: RDM-003 RU1 code review
status: passed
roadmap_item: RDM-003
review_unit: RU1
work_package: docs/work-packages/RDM-003-windows-wsl-workbench-path-ux/2026-06-23-003-wsl-workbench-path-ux-work-package.md
review_date: 2026-06-23
review_type: code
threshold: P0-P2
---

# RDM-003 RU1 Code Review

## Result

Passed. No remaining P0-P2 findings.

## Reviewed Scope

- `src-tauri/src/workbench/mod.rs`
  - Adds WSL-specific workbench errors.
  - Adds `RepoEntry::wsl`.
  - Adds `add_wsl_repo` and `remove_wsl_repo` store methods.
  - Adds lexical Ubuntu/Linux-path validation.
  - Makes WSL entries runtime-visible on Windows while keeping `is_runtime_supported()` local-only for bus mounting.
- `src-tauri/src/workbench/commands.rs`
  - Adds Windows-only `add_wsl_repo` and `remove_wsl_repo` Tauri commands.
  - Reseeds the active bus after WSL config changes.
- `src-tauri/src/lib.rs`
  - Registers WSL config commands only behind `#[cfg(target_os = "windows")]`.
  - Updates invoke-handler and startup tests for RDM-003 semantics.

## Findings

None at P0-P2.

## Impact Scan

Changed surfaces:
- Workbench persisted config helper behavior.
- Workbench command surface on Windows only.
- Initial active workbench seed now passes Windows-visible WSL config entries to the bus, which still classifies them as unsupported and does not mount them.

Consumer scan pattern used:

```powershell
rg "RepoEntry|RepoSource|runtime_config|active_runtime_repos_for|list_workbenches|add_repo|remove_repo|update_repo|wslAbsence|displayName|sortedRepoPaths|addRepoFlow|MenuBar|invoke_handler|add_wsl_repo|remove_wsl_repo" src src-tauri docs\contracts
```

Noted follow-up for RU2:
- `src/bus/contract.ts` and frontend workbench surfaces still need the additive `source`/`distro` contract and Windows platform gate.
- `docs/contracts/bus-contract.md` still reflects RDM-001 absence language and should be updated when the frontend contract is updated.

## Verification

- `cargo test --lib workbench`: 33 passed.
- `cargo test --lib invoke_handler`: 1 passed.
- `cargo test --lib bus -- --test-threads=1`: 42 passed.
- `rustfmt --edition 2021 --check --config skip_children=true src\workbench\mod.rs src\workbench\commands.rs src\lib.rs`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.

Pre-existing warnings remained:
- `GITLEAKS_GO_PACKAGE` is never used.
- `RouterInput::PollDetected.repo` is never read.

## Residual Risk

RU1 intentionally leaves frontend type/UI work to RU2/RU3. Do not release RDM-003 until the frontend contract, Windows-only UI gate, absence tests, and security review are complete.
