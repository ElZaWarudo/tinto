---
title: RDM-001 requirements review
status: resolved
date: 2026-06-23
artifact: docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md
review_type: requirements-review-fallback
reviewers:
  - ce-spec-flow-analyzer
---

# RDM-001 requirements review

## Result

Requirements review passed after fixes.

## Findings Resolved

- Defined the Linux absence boundary: non-Windows builds must compile/register/export no WSL commands, launcher paths, UI flags, menus, settings, empty/degraded states, runtime paths, or behavior.
- Defined copied/shared config behavior: future WSL entries are preserved on disk on Linux but excluded from runtime state and UI.
- Kept public WSL bus identity out of RDM-001 unless a reviewed plan proves an additive compatibility field is needed.
- Required the repo-source guard/router to run before local path handling so WSL fixtures cannot reach `canonicalize`, `Git2Engine`, file ops, Gitleaks, media reads, or agent-console local launch paths.
- Added WSL identity normalization rules: exact distro name plus normalized absolute Linux path, with no Windows path translation or case folding.
- Added explicit local regression flows for config load, repo CRUD/reorder, workbench switch, snapshot/delta delivery, dashboard display names, subscriptions, and degraded watcher state.

## Remaining Advisory Notes

- RDM-004 must decide the eventual public WSL repo identity shape.
- RDM-001 may model Windows capability but must not probe or launch WSL.
