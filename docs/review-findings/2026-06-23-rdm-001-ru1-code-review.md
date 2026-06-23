---
title: RDM-001 RU1 code review findings
status: passed-after-fixes
date: 2026-06-23
review_unit: RDM-001 RU1
package: docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md
threshold: P0-P2
---

# RDM-001 RU1 code review findings

## Result

Review passed after fixes. No remaining P0-P2 findings are known for RU1.

## Reviewers

- Correctness reviewer: read-only subagent.
- Maintainability reviewer: read-only subagent.
- Testing reviewer: read-only subagent.
- Lead synthesis/re-review: direct inspection after fixes.

## Findings Fixed

### P2 - Hidden WSL entries could affect local path-only mutations

Files: `src-tauri/src/workbench/mod.rs`.

Finding: `add_repo`, `remove_repo`, `update_repo`, and `reorder_repos` compared persisted entries by path only. A hidden future `source = "wsl"` entry could block adding a visible local repo at the same path string, or be removed/updated/reordered through local commands.

Fix: local command helpers now operate only on `RepoSource::Local` entries. Future WSL entries remain preserved on disk and untouched by local add/remove/update/reorder flows.

Verification:
- `comandos_locales_no_colisionan_con_fuentes_wsl_ocultas`
- `cargo test --lib workbench`

### P2 - WSL-only workbench projected as a visible empty runtime workbench

Files: `src-tauri/src/workbench/mod.rs`, `src-tauri/src/lib.rs`.

Finding: filtering repos but preserving every workbench could turn a persisted WSL-only workbench into an active empty workbench on Linux, which violates the requirement that no WSL empty/degraded runtime or UI state appears.

Fix: runtime projection hides workbenches that contain only unsupported sources. If persisted `active` points to a hidden workbench, runtime `active` remaps to the first visible workbench when one exists; otherwise it is `None`. This remap is not persisted.

Verification:
- `runtime_config_oculta_workbench_solo_wsl_y_remapea_active_visible`
- `set_active_rechaza_workbench_solo_wsl_en_runtime`
- `initial_runtime_repos_no_monta_workbench_solo_wsl`
- `cargo test --lib workbench`
- `cargo test --lib initial_runtime_repos`

### P2 - Runtime filtering duplicated at reseed boundary

Files: `src-tauri/src/workbench/commands.rs`.

Finding: `reseed_if_active` repeated filtering logic separately from the store runtime projection, making later WSL eligibility changes drift-prone.

Fix: command reseed now calls `active_runtime_repos_for`, which delegates to `WorkbenchStore::active_workbench_runtime()`.

Verification:
- `active_runtime_repos_for_filtra_wsl_en_reseed_activo`
- `cargo test --lib workbench`

### P2 - Local malformed entries could leak WSL-specific `distro` field

Files: `src-tauri/src/workbench/mod.rs`.

Finding: a malformed local config entry with `distro = "..."` could remain visible through `runtime_config`.

Fix: runtime projection clears `distro` for local entries while preserving the persisted TOML unchanged.

Verification:
- `runtime_config_limpia_distro_de_entradas_locales_malformadas`
- `cargo test --lib workbench`

### P2 - Boundary coverage missing

Files: `src-tauri/src/workbench/mod.rs`, `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs`.

Finding: initial tests covered the store helper but not the Tauri command boundary, startup initial mount helper, active reseed helper, or full WSL-on-disk field preservation.

Fix: added focused tests for command projection, active reseed projection, startup initial mount projection, WSL field preservation, active remap, and local/WSL same-path behavior.

Verification:
- `cargo test --lib workbench`
- `cargo test --lib initial_runtime_repos`

## Verification Summary

- `cargo test --lib workbench`: 30 passed.
- `cargo test --lib bus`: 37 passed.
- `cargo test --lib watcher`: 29 passed.
- `cargo test --lib agent_console`: 35 passed.
- `cargo test --lib file_ops`: 0 matched, 172 filtered.
- `cargo test --lib bus::tests::ae8_repo_removido_estado_terminal`: 1 passed after one full-suite timing failure.
- `cargo test --lib -- --test-threads=1`: 172 passed.
- `cargo fmt --check`: passed.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`: 77 passed.
- `npx tsc --noEmit`: passed.

Note: two parallel/default Rust full-suite runs hit existing watcher/bus timing-sensitive tests in different places. The targeted tests passed, and the full library suite passed with `--test-threads=1`.

## Residual Risk

RU2 still owns guard/routing tests for repo-scoped backend command families before local filesystem/git/secret-scan/file/session handling. RU3 still owns broader Linux absence checks across frontend exports, UI, menus/settings, and contract notes.
