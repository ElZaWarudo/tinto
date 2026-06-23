---
title: WSL Agent Console parity
status: reviewed
date: 2026-06-23
roadmap_item: RDM-009
production_posture: prototype
---

# WSL Agent Console parity

## Context
Tinto can now track Windows and Ubuntu WSL repositories in the same workbench, and WSL read/write/media/Gitleaks paths route through the Linux side instead of accidentally touching host paths. Agent Console is the remaining user-facing repo action that still resolves repos through the local-only backend and rejects WSL entries.

The serious-product bar is that launching an agent from a WSL repo must not silently run in Windows, must not canonicalize a Linux path into a host path, and must keep the same session lifecycle users already rely on for local repos.

## Decisions
- Initial supported WSL distro remains Ubuntu, matching the existing WSL complement decision.
- A WSL session runs inside Ubuntu via `wsl.exe`, with the working directory set to the Linux repo path before launching the selected agent.
- The existing Tauri session contract remains additive and stable: start/list/output/input/resize/stop/revert keep their command names and event names.
- Host-local sessions keep their existing checkpoint and revert behavior unchanged.
- WSL session start validates the selected agent inside Ubuntu, not on the Windows host.
- WSL checkpoint/revert should be handled honestly. If full remote checkpoint parity is not included in the first implementation, the UI and backend must disable or reject WSL revert explicitly instead of pretending the repo is protected.

## Acceptance Criteria
- Starting an agent from a WSL repo does not return `unsupported_repo_source`.
- WSL sessions stream output through `tinto://agent-session-output`, accept base64 input, resize, stop, and appear in `list_agent_sessions`.
- A missing Ubuntu agent binary returns a safe `binary_not_found` style error without leaking secrets or shell command text.
- Local Windows repo sessions remain covered by existing tests and unchanged command names.
- The repo card availability check is repo-aware enough that WSL repos are not blocked by a missing Windows-host agent binary.
- WSL revert/checkpoint behavior is explicit in both UI and contract docs.

## Open Questions
- None blocking for Ubuntu-only implementation. Future distro selection stays outside this package.
