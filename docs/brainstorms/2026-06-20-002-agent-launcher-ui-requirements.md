---
title: ACI-003 Agent Launcher UI Requirements
date: 2026-06-20
topic: agent-launcher-ui
origin_roadmap: docs/roadmaps/2026-06-19-002-agent-console-integration.md
status: accepted
---

# ACI-003 Agent Launcher UI Requirements

## Problem

Tinto can now create PTY-backed agent sessions and render terminal panels, but there is no repo-local UI to start one. The next step is a compact launcher on each repo card that lets the user choose an agent type, validates whether the binary exists, starts exactly one backend session per launch, and opens the matching terminal panel.

## Requirements

- R1. Repo cards expose a compact launch affordance without changing the single-click behavior that opens the project tab.
- R2. The launcher supports `claude`, `codex`, and `opencode` agent ids, displayed as Claude Code, Codex, and OpenCode.
- R3. The backend exposes `agent_binary_available(agent_type)` and rejects unsupported agent ids deterministically.
- R4. Missing binaries disable the launch button and show a non-blocking message.
- R5. A valid launch calls `start_agent_session(repo, agent_type)` exactly once and opens/focuses the terminal panel for the returned session id.
- R6. Launcher controls stop event propagation so selecting/launching does not open the repo card accidentally.
- R7. No checkpoint, revert, multi-agent auto-layout, resource limits, or shell argument editing is introduced in this item.

## Acceptance Evidence

- AE1. Missing binary for the selected agent disables launch and does not call `start_agent_session`.
- AE2. Selecting a valid agent and clicking launch calls `start_agent_session` with the repo path and agent id.
- AE3. After successful launch, the terminal panel opener receives `{ sessionId, repo, agentType }`.
- AE4. Clicking select or launch controls does not trigger the card `onOpen`.
- AE5. Unsupported backend agent id returns `unsupported_agent`; missing known binary returns `false` from availability.

## Assumptions

- Binary availability can be checked through the same allowlist and PATH lookup used by session launch.
- The first launcher stores selected agent type locally per card; no global preference is required yet.
- Launch arguments remain the default binary invocation from ACI-001; typed argument builders can be added later when needed.
