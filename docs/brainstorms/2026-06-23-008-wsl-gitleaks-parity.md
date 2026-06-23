---
title: WSL Gitleaks parity requirements
status: reviewed
date: 2026-06-23
roadmap_item: RDM-008
source_state: docs/orchestration/compound-master-state.md
---

# WSL Gitleaks Parity Requirements

## Goal

Make Gitleaks support source-aware so Windows/local repos use the host environment and Ubuntu WSL repos use the Linux agent environment.

## Functional Requirements

- FR1: WSL repo snapshots shall continue to produce `gitleaks_configured` and `secret_findings` from inside the Linux agent.
- FR2: Creating `.gitleaks.toml` for a WSL repo shall write inside the Linux repo through `tinto-agent`.
- FR3: Additive repo-aware Gitleaks setup/status and install commands shall route to the host for local repos and to `tinto-agent` for WSL repos.
- FR4: Existing global Addons commands shall remain host-scoped and backward compatible.
- FR5: All WSL Gitleaks commands shall enforce active-workbench allowlisting and safe error categories.

## Non-Goals

- No Gitleaks UI redesign.
- No automatic install without user action.
- No multi-distro support beyond Ubuntu.
- No release, push, PR, or Jira mutation.

## Acceptance Criteria

- Existing frontend Gitleaks calls keep working.
- New repo-aware wrappers are additive.
- WSL runtime tests cover repo config creation and setup/install request paths.
- Bus tests and frontend contract tests pass.
