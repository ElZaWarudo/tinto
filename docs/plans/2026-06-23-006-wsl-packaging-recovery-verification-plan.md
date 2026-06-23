---
title: WSL packaging, recovery, and verification plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-006
source_requirements: docs/brainstorms/2026-06-23-006-wsl-packaging-recovery-verification.md
planning_status: planned
delivery_approach: phase-based
---

# WSL Packaging, Recovery, And Verification Plan

## Plan Units

- U1 - Add packaged-first Linux agent discovery and WSL install command construction.
- U2 - Route WSL requests through the packaged-first launcher with explicit dev fallback.
- U3 - Add diagnostics/tests for missing packaged agent, install command shape, and fallback behavior.
- U4 - Update contract, smoke docs, work package, state, and security/review notes.

## Risks

- The packaged artifact must be a Linux binary, not the Windows app binary.
- Copying into WSL must not use user-controlled shell interpolation.
- Dev fallback must stay explicit so release behavior does not silently depend on a source checkout.

## Verification

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `cargo build --bin tinto-agent`
- `npx tsc --noEmit`
- work package checker
- `git diff --check`
