---
title: RDM-015 Agent Turn Checkpoints Security Review
status: pass
date: 2026-06-27
work_package: docs/work-packages/RDM-015-agent-turn-checkpoints/2026-06-27-015-agent-turn-checkpoints-work-package.md
reviewer: krt-security-sentinel
---

# RDM-015 Agent Turn Checkpoints Security Review

Security status: pass.

## Scope
- Changed surfaces: Agent Console turn checkpoint model, local checkpoint scan/revert, WSL checkpoint protocol/runtime, Tauri command `revert_session_turn_file`, TerminalPanel Agent Lens UI, and contract mirrors.
- Primary assets: user repository files, `.git` internals, WSL repo allowlist, checkpoint snapshots under Tinto-managed storage, and session registry state.
- Out of scope: live external scanning, production telemetry, dependency advisory scanning, and auth/tenant systems not touched by this package.

## Findings
- No P0-P2 blocking security findings remain.

## Remediated During Review
- [P2] Symlink ancestor escape during checkpoint restore/delete.
  - Evidence: per-file revert writes or deletes `repo.join(rel)`. If a path ancestor inside the repo is replaced by a symlink after checkpoint creation, restore/delete could target outside the repo on Unix/WSL filesystems.
  - Remediation: `checkpoint.rs` now validates current path ancestors before checkpoint delete/restore and refuses symlink ancestors; restore also removes a symlink at the final target before copying snapshot content.
  - Verification: added Unix-only coverage for symlink ancestor escape and reran focused checkpoint tests plus clippy.

## Verification Evidence
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console::checkpoint -- --test-threads=1`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- Previous focused WSL coverage verifies `AgentCheckpointRevertFile` routes through the WSL allowlist and reverts only the selected file.
- `git diff --check` reports only Windows CRLF conversion warnings.

## Residual Risk
- Turn attribution is intentionally temporal, not proof of author identity. UI copy says "changes during turn" rather than "agent-authored changes".
- Checkpoint operations remain local destructive actions and require explicit user confirmation from the UI.
- The Unix symlink escape test is compiled only on Unix; Windows host coverage relies on the non-symlink containment tests plus WSL/Linux CI for symlink behavior.

## Release Notes
- Safe to proceed after broad gates remain green.
