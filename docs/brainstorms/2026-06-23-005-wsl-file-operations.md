---
title: WSL file operations parity
status: reviewed
date: 2026-06-23
roadmap_item: RDM-005
source_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
validation_status: validated
---

# WSL File Operations Parity

## Problem And Goal

Tinto now tracks Windows and Ubuntu WSL repos together. The file explorer already supports drag/drop, paste/cut/copy, delete, undo, redo, and export for local repos. Those workflows must continue to work when the target repo is WSL.

Goal: route file operations for WSL repos through `tinto-agent`, preserving the current frontend commands and conflict/undo semantics.

## Scope In

- `copy_to_repo` for Windows OS paths into a WSL repo, translating Windows absolute paths to `/mnt/<drive>/...` for the agent.
- `copy_within_repo` and `move_within_repo` inside WSL repos.
- `delete_from_repo`, `restore_deleted_from_repo`, and `redo_deleted_from_repo` inside WSL repos.
- `export_from_repo` from WSL repos to a Windows destination translated to `/mnt/<drive>/...`.
- Same conflict categories and result shapes as local file operations.
- Agent-side path containment and `.git` mutation rejection.

## Scope Out

- Gitleaks creation/install/status for WSL.
- Agent Console sessions for WSL.
- Media preview for WSL.
- Cross-distro or non-Ubuntu behavior.
- Shell-based copy/delete commands.

## Acceptance Criteria

- Given a WSL repo, drag/drop from Windows into the repo calls the same frontend command and succeeds through the agent.
- Given a WSL repo, internal copy/move/paste/delete/undo/redo use the same result DTOs as local repos.
- Given a WSL repo, operations reject traversal, `.git`, missing sources, and overwrite conflicts safely.
- Given a local repo, existing file operation behavior remains unchanged.
- Given a WSL repo, local-only surfaces not in this scope remain blocked.
