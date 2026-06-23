---
title: WSL Agent Console remote checkpoints plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-011
origin_brainstorm: docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md
production_posture: prototype
---

# WSL Agent Console remote checkpoints plan

## U1 - Protocol checkpoint operations
Add WSL agent request/response variants for checkpoint create, change-log scan, and revert using the existing checkpoint record and session change DTOs.

## U2 - Runtime checkpoint handlers
Use existing `agent_console::checkpoint` functions inside `tinto-agent`, so Linux paths, git state, filesystem snapshots, and revert actions stay in Ubuntu.

## U3 - Registry remote checkpoint backend
Teach `AgentSessionRecord` whether its checkpoint is local or WSL-backed. Local sessions call existing functions; WSL sessions call `tinto-agent` for scan/revert.

## U4 - WSL start integration
Create the remote checkpoint before spawning the WSL PTY. If checkpoint creation fails, fail start safely.

## U5 - Verification and review
Run WSL agent, Agent Console, invoke/frontend regressions where relevant, typecheck, and diff hygiene.
