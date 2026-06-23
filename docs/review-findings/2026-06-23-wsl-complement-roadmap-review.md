---
title: Windows-only WSL complement roadmap review
status: resolved
date: 2026-06-23
artifact: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
review_type: document-review-fallback
reviewers:
  - ce-coherence-reviewer
  - ce-feasibility-reviewer
  - ce-scope-guardian-reviewer
---

# Windows-only WSL complement roadmap review

## Result

Roadmap review passed after document fixes.

## Findings Resolved

### P0/P1 - Linux absence must be strict, not no-op

Reviewers found that "no-op/hidden" wording could allow inert WSL commands or runtime paths on Linux, which contradicted the binding user clarification that this is a Windows-only complement. The roadmap now requires Linux desktop builds to expose no WSL UI, commands, settings, empty states, degraded notices, runtime entry points, or inert WSL surfaces.

### P1 - Scope was broader than a complement

Reviewers found that "transport-neutral backend service boundary", full JSON-RPC framing, and broad bus parity were too large for a Windows-only add-on. The roadmap now narrows RDM-001 to opaque repo identity plus the smallest backend routing seam, narrows RDM-002 to a minimal stdio protocol and explicit `tinto-agent` binary, and narrows RDM-004 to the core WSL read/watch workflow.

### P1 - Repo identity decision was too late

Reviewers found that the current codebase is path-keyed and a WSL `{distro, linux_path}` identity cannot safely be deferred until after backend boundary extraction. RDM-001 now owns the opaque repo identity model.

### P1/P2 - Dependency inconsistencies

Reviewers found inconsistent dependency declarations around RDM-003, RDM-005, and RDM-006. The roadmap now removes the circular soft sequencing note from RDM-003, makes RDM-006 hard-depend on RDM-005, and keeps RDM-005 dependent on RDM-004 with RDM-002/RDM-003 as transitive dependencies.

### P2 - WSL health semantics were underspecified

Reviewers found that global `WatchingState` could not represent a single WSL distro failure cleanly. RDM-004 now requires WSL agent/distro failures to map to per-repo `RepoErrorState` for affected WSL repos, keeping global `WatchingState` for workbench-wide local watcher degradation.

## Remaining Advisory Notes

- Full command parity, Gitleaks/secret findings, media preview, file mutations, and agent console routing remain out of the first WSL read/watch path unless later policy and user decisions approve them.
- Final confidence still requires manual Windows/WSL smoke because this workspace cannot run a real Windows host with WSL.
