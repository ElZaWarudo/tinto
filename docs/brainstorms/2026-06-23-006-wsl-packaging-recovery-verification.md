---
title: WSL packaging, recovery, and verification requirements
status: reviewed
date: 2026-06-23
roadmap_item: RDM-006
source_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
---

# WSL Packaging, Recovery, And Verification Requirements

## Goal

Move the Windows/Ubuntu WSL complement from dev-only agent launch toward software a user can run outside the source checkout.

## Decisions

- Supported WSL baseline remains WSL 2 with one selected Ubuntu distro for this initiative.
- A serious install path requires a Linux `tinto-agent` binary. A Windows-built binary is not executable inside Ubuntu.
- The host should prefer an app-managed Linux agent binary and keep the existing dev-source launch only as an explicit development fallback.
- Releases remain deferred until the final batch.

## Functional Requirements

- FR1: On Windows, Tinto shall discover a host-side Linux `tinto-agent` artifact from an explicit environment variable first, then a stable app-relative packaged location.
- FR2: Tinto shall install the Linux agent into a versioned WSL path under `~/.local/share/tinto/agents/`.
- FR3: Tinto shall launch the installed WSL agent with argv arrays only, with no shell interpolation for request execution.
- FR4: Tinto shall report safe error categories for missing WSL, missing distro, missing packaged agent, install failure, timeout, protocol mismatch, malformed response, and child exit.
- FR5: Tinto shall retain dev-source launch only when an explicit development fallback variable is enabled or no packaged artifact is available during development.
- FR6: The manual smoke checklist shall verify packaged-agent discovery/install, dev fallback, protocol mismatch recovery, and local/WSL repo coexistence.

## Non-Goals

- No full auto-updater.
- No multi-distro install matrix.
- No WSL Gitleaks, media preview, Agent Console routing, or fine-grained `fs-events`.
- No PR, push, merge, or release in this package.

## Acceptance Criteria

- AC1: Unit tests cover packaged artifact discovery, WSL install command shape, dev fallback gating, and argv/no-shell behavior.
- AC2: Existing WSL read/file-operation code paths call the packaged-first launcher helper.
- AC3: Contract/docs/state record that the first serious runtime expects a packaged Linux agent artifact.
- AC4: Verification includes Rust WSL tests, bus tests, `cargo build --bin tinto-agent`, TypeScript checks where contract docs/front-end are touched, work-package checker, and `git diff --check`.
