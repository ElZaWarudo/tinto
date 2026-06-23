---
title: WSL file operations parity plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-005
source_requirements: docs/brainstorms/2026-06-23-005-wsl-file-operations.md
planning_status: planned
delivery_approach: phase-based
---

# WSL File Operations Parity Plan

## Plan Units

- U1 - Make file operation DTOs deserializable by the host and agent.
- U2 - Extend the WSL agent protocol/runtime with copy, move, delete, restore, redo, and export handlers.
- U3 - Route existing Tauri file operation commands by repo source, preserving local behavior.
- U4 - Update contract/docs/state and run focused verification/security review.

## Risks

- Windows paths must be translated to WSL mount paths without shell interpolation.
- Mutations must remain contained inside repo roots and outside `.git`.
- Undo tokens are created by the agent, so restore/redo for WSL must route to the agent too.

## Verification

- `cargo test --lib file_ops`
- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- targeted frontend contract/file operation tests
- `npx tsc --noEmit`
- targeted Rust format and `git diff --check`
