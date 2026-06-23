---
title: WSL media preview requirements
status: reviewed
date: 2026-06-23
roadmap_item: RDM-007
source_state: docs/orchestration/compound-master-state.md
---

# WSL Media Preview Requirements

## Goal

Let existing PDF/image previews work for Ubuntu WSL repos without changing frontend behavior.

## Functional Requirements

- FR1: `get_media_content` shall route by repo source.
- FR2: Local repos shall keep the existing local media read path.
- FR3: WSL repos shall read media through `tinto-agent`.
- FR4: WSL media reads shall preserve existing extension allowlist, `.git` rejection, repo containment, regular-file-only reads, base64 response shape, and 12 MiB guard.

## Non-Goals

- No new preview UI.
- No streaming media.
- No WSL Gitleaks, Agent Console, or fine-grained `fs-events`.

## Acceptance Criteria

- Existing frontend media calls are unchanged.
- WSL agent tests cover media content and unsupported media rejection.
- Bus tests continue to pass.
