---
title: WSL Agent Console remote checkpoints
status: reviewed
date: 2026-06-23
roadmap_item: RDM-011
production_posture: prototype
---

# WSL Agent Console remote checkpoints

## Context
RDM-009 enables WSL Agent Console launch/lifecycle, but intentionally avoids fake host checkpoints: WSL sessions expose no checkpoint and cannot be reverted. That is safer than misleading rollback, but not enough for a serious agent workflow.

## Decision
Move Agent Console checkpoint creation, change-log scanning, and revert for WSL sessions into Ubuntu through `tinto-agent`. The host registry keeps the same `AgentSession` contract, but the checkpoint record points to Linux paths and all destructive revert work runs inside WSL.

## Acceptance Criteria
- WSL session start creates a remote checkpoint before launching the agent.
- WSL stopped/completed sessions show change logs from Ubuntu.
- `revert_session` on WSL sessions restores through the WSL agent after explicit consent.
- Local session checkpoint behavior remains unchanged.
- If remote checkpoint creation fails, the WSL session does not start.
