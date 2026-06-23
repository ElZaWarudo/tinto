---
title: WSL Gitleaks parity plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-008
source_requirements: docs/brainstorms/2026-06-23-008-wsl-gitleaks-parity.md
planning_status: planned
delivery_approach: single-review-unit
---

# WSL Gitleaks Parity Plan

## Plan Units

- U1 - Make Gitleaks setup/result DTOs reusable in the agent protocol.
- U2 - Add WSL agent requests for setup status, install, and repo config creation.
- U3 - Add repo-aware host commands/wrappers while preserving existing global commands.
- U4 - Update docs/state/review evidence.

## Verification

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`
- `npx tsc --noEmit`
- work package checker
- `git diff --check`
