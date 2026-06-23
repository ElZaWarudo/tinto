---
title: WSL media preview plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-007
source_requirements: docs/brainstorms/2026-06-23-007-wsl-media-preview.md
planning_status: planned
delivery_approach: single-review-unit
---

# WSL Media Preview Plan

## Plan Units

- U1 - Add a `MediaContent` WSL agent request/response path.
- U2 - Route `get_media_content` by repo source.
- U3 - Reuse existing media validation/read bounds in the agent.
- U4 - Update contract/docs/state and verification evidence.

## Verification

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/file/FileView.test.tsx src/panels/file/MediaView.test.tsx`
- `npx tsc --noEmit`
- `git diff --check`
