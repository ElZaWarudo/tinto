---
title: WSL installer resource and smoke closure
status: review-passed
roadmap_item: RDM-006
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-012-wsl-installer-resource-smoke.md
origin_planning_input: docs/brainstorms/2026-06-23-012-wsl-installer-resource-smoke.md
origin_plan: docs/plans/2026-06-23-012-wsl-installer-resource-smoke-plan.md
units: [U1, U2, U3, U4]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: local-final-batch
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# WSL Installer Resource And Smoke Closure

## Scope

Close the packaging gap left by RDM-006: wire the Ubuntu-built `tinto-agent-linux-x86_64` artifact into the Tauri bundle resource set, add Windows bundle CI proof, and update final smoke docs.

## Non-goals

No updater, signing, release publishing, Jira mutation, PR creation, or multi-distro expansion.

## Autonomy Contract

- Mode: guarded.
- Agent may decide CI artifact destination paths and resource target name as long as the launcher can find the packaged agent.
- Agent must record if real Windows/Ubuntu smoke cannot run in this workspace.
- Agent must escalate release timing, signing credentials, or installer distribution changes.
- Safe fallback: missing resource should fail packaging/CI rather than silently ship without WSL agent support.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-006 packaged-first launcher and RDM-011 WSL Agent Console checkpoint closure.
- Blocks: final batched release.

## Production Posture

- Posture: prototype.
- Evidence: active local desktop development state.
- Confidence: high.
- Consequences for this package: validate packaging shape without claiming real-machine WSL smoke unless executed.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Tauri needs an explicit resource declaration. |
| U2 | yes | CI must stage the Linux artifact into that resource path. |
| U3 | yes | Windows installer proof is the relevant release surface. |
| U4 | yes | Docs/state must preserve the manual smoke gate. |

Grouping rationale:
- One RU keeps resource config, CI staging, and smoke docs together because splitting them can create a CI state that references a missing artifact.

## Implementation Units

- U1: add `src-tauri/resources/` staging and git ignore rules for the generated Linux agent.
- U2: add Tauri `bundle.resources` mapping for `tinto-agent-linux-x86_64`.
- U3: update CI bundle jobs to download the Ubuntu-built agent before bundling; add Windows bundle job.
- U4: update manual smoke/contract/state/review findings and verify.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL agent installer resource and bundle CI | `.github/workflows/ci.yml`, `src-tauri/tauri.conf.json`, `src-tauri/resources/`, docs | develop | Optional standalone Tarea | Medium risk; release packaging and CI, no runtime auth/data changes. |

## Files and Tests

- Expected files: `.github/workflows/ci.yml`, `.gitignore`, `src-tauri/tauri.conf.json`, `src-tauri/resources/.gitkeep`, `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`, `docs/contracts/bus-contract.md`, this package, review findings, state.
- Expected tests: `npx tsc --noEmit`, `cargo test --lib wsl_agent`, Tauri config parse/build dry check where feasible, work package checker, `git diff --check`.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: no public API changes; CI and Tauri packaging config only.
- Consumer scan patterns: `rg "tinto-agent-linux-x86_64|resources|request_ubuntu_agent|packaged_agent_candidates|TINTO_WSL_AGENT" .github src-tauri docs`.
- Consumers found: `.github/workflows/ci.yml`, `src-tauri/tauri.conf.json`, `src-tauri/src/wsl_agent/launcher.rs`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/agent_console/mod.rs`, `src-tauri/src/agent_console/session.rs`, `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`, and `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: WSL launcher/runtime tests, Tauri config parse, CI resource guard inspection.
- Required consumer tests: WSL launcher tests and config/CI inspection.
- Consumer tests run/skipped: `cargo test --lib wsl_agent` passed 26/26; `cargo build --bin tinto-agent` passed; `npx tsc --noEmit` passed; `npm run tauri -- info` parsed the config but reported missing local Visual Studio Build Tools/MSVC; `npx prettier --check` passed for changed config/docs; `git diff --check` passed with CRLF warnings only. Real Windows installer build and Ubuntu WSL smoke are delegated to CI/manual smoke because local MSVC Build Tools are not installed.

## Verification Gate

- `cargo test --lib wsl_agent`
- `npx tsc --noEmit`
- Tauri config parse/build dry check if available without a real Windows installer environment.
- `python C:\Users\Mayor\.agents\skills\krt-compound-master\scripts\check_work_package.py docs\work-packages\RDM-006-final-wsl-installer-resource-smoke\2026-06-23-012-wsl-installer-resource-smoke-work-package.md`
- `git diff --check`

Verification results:
- `npm run tauri -- info` passed config parsing; local environment reports missing Visual Studio Build Tools/MSVC.
- `cargo test --lib wsl_agent` passed 26/26.
- `cargo build --bin tinto-agent` passed with pre-existing warnings only (`GITLEAKS_GO_PACKAGE`, `PollDetected.repo`).
- `npx tsc --noEmit` passed.
- `npx prettier --check .github/workflows/ci.yml src-tauri/tauri.conf.json docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md docs/contracts/bus-contract.md` passed.
- `git diff --check` passed with CRLF warnings only.
- Work package checker passed.

## Review Gate

- Code review threshold: P0-P2.

## Security Gate

- Run after work-review loop: required because packaged executable resource and installer CI are release-sensitive.
- Security Watch during work: verified no generated binary is committed (`.gitignore` excludes `src-tauri/resources/tinto-agent-linux-x86_64`), CI fails before bundle if the artifact is missing, and the resource target is copied to the resource root where the launcher candidate can find it.
- Security reviewer: inline fallback if dedicated role unavailable.
- Security review result: passed. Findings path: `docs/review-findings/2026-06-23-rdm-006-final-code-security-review.md`.
- Required security verification: `.gitignore` excludes generated agent binary and CI download path is explicit.

## CI Break-Prevention And Escalation

- CI risk surfaces: Tauri config schema, artifact dependency, Windows bundle job.
- Preventive evidence: config inspection and existing WSL launcher tests; full Windows bundle proof will run in GitHub Actions after release push.
- If CI breaks: use direct evidence-first triage; no CI polling in Compound Master.
- Escalation rule: signing credentials or publishing changes are out of scope.

## Branch and PR Handoff Inputs

- Review unit: RU1 WSL agent installer resource and bundle CI.
- Branch name: `feat/wsl-agent-installer-resource`
- Branch/docs rule: keep docs with implementation in final batch.
- PR base: `develop`.
- Suggested commit grouping:
  - `ci(wsl): bundle packaged linux agent resource`
  - `docs(wsl): record final installer smoke gate`
- PR title: `Bundle the WSL agent with Tauri installers`
- PR body bullets:
  - Declare the Linux `tinto-agent` artifact as a Tauri resource.
  - Download the Ubuntu-built artifact before Tauri bundle jobs.
  - Add Windows bundle CI proof and preserve the manual Ubuntu WSL smoke gate.
- Verification results location: this package and `docs/review-findings/2026-06-23-rdm-006-final-code-security-review.md`.
- Production/deployment notes: final release still needs real Windows/Ubuntu smoke unless this machine can run it.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone Tarea unless a WSL complement parent exists.
- Jira summary: `Empaquetar el agente WSL en el instalador`
- Jira description: `Incluir el binario Linux tinto-agent como recurso del bundle Tauri y validar que el instalador Windows se construye con ese recurso antes del smoke final en Ubuntu WSL.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.
