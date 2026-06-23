---
title: WSL media preview
status: review-passed
roadmap_item: RDM-007
origin_roadmap: docs/orchestration/compound-master-state.md
origin_brainstorm: docs/brainstorms/2026-06-23-007-wsl-media-preview.md
origin_planning_input: docs/brainstorms/2026-06-23-007-wsl-media-preview.md
origin_plan: docs/plans/2026-06-23-007-wsl-media-preview-plan.md
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

# WSL Media Preview

## Scope

Route the existing `get_media_content` command through `tinto-agent` for Ubuntu WSL repos while preserving local behavior and frontend DTOs.

## Non-goals

- No new media UI.
- No streaming or caching.
- No WSL Gitleaks, Agent Console, or fine-grained `fs-events`.
- No release, push, PR, merge, or Jira mutation.

## Autonomy Contract

- Mode: guarded.
- Agent may decide helper visibility and test fixture names.
- Agent must record skipped Windows/Ubuntu smoke as a final-release blocker.
- Agent must escalate public DTO changes or changed media size/security policy.
- Safe fallback: leave unsupported WSL media rejected rather than weakening containment.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-004, RDM-006.
- Blocks: final WSL release parity.

## Production Posture

- Posture: prototype.
- Evidence: active orchestration state.
- Confidence: high.
- Consequences for this package: preserve local behavior and command shape.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Agent request/response is required. |
| U2 | yes | Existing command must route by repo source. |
| U3 | yes | Security parity depends on shared validation/bounds. |
| U4 | yes | Docs/state keep release handoff honest. |

Grouping rationale:
- The package is one small command-path parity change.

## Implementation Units

- U1: Add protocol request/response.
- U2: Route host command.
- U3: Reuse media guards in runtime.
- U4: Update docs/review/state.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | WSL media command parity | `src-tauri/src/bus/commands.rs`, `src-tauri/src/wsl_agent/*`, docs/tests | `develop` with queued WSL work | optional Tarea | Low/medium risk; read-only but file content surface. |

## Files and Tests

- Runtime: `src-tauri/src/bus/commands.rs`, `src-tauri/src/wsl_agent/protocol.rs`, `src-tauri/src/wsl_agent/runtime.rs`.
- Docs: `docs/contracts/bus-contract.md`, this work package, review findings, state.
- Tests: WSL agent, bus, frontend file/media tests, TypeScript, checker, diff check.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: internal WSL agent protocol only; public Tauri command shape unchanged.
- Consumer scan patterns: `rg "get_media_content|MediaContent|media_content" src src-tauri docs`.
- Consumers found: frontend bus wrapper, FileView/MediaView tests, backend command, WSL runtime/protocol.
- Contract-drift tests searched: frontend contract tests and Rust protocol tests.
- Required consumer tests: frontend contract/file/media tests and WSL agent tests.
- Consumer tests run/skipped: WSL agent tests, bus tests, frontend contract/file/media tests, TypeScript, tinto-agent build, work package checker, and `git diff --check` passed. Real Windows/Ubuntu media smoke remains pending for final release.

## Verification Gate

- `cargo test --lib wsl_agent`
- `cargo test --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/file/FileView.test.tsx src/panels/file/MediaView.test.tsx`
- `npx tsc --noEmit`
- Work package checker
- `git diff --check`

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate

- Run after work-review loop: required because media file reads cross the WSL agent boundary.
- Security Watch during work: enabled.
- Security Watch notes: preserve extension allowlist, base64-only response, 12 MiB guard, `.git` rejection, and regular-file checks.
- Security reviewer: inline fallback.
- Security review result: passed by inline fallback on 2026-06-23.
- Required security verification: tests plus direct inspection.

## Implementation Summary

- Added WSL agent `MediaContent` request/response support.
- Routed `get_media_content` through source-aware repo resolution: local repos keep the existing path; WSL repos call `tinto-agent`.
- Reused existing media extension validation, `.git` rejection, repo containment, regular-file-only reads, base64 response shape, and 12 MiB guard.

## Review Result

- Status: `review-passed`.
- Findings path: `docs/review-findings/2026-06-23-rdm-007-code-security-review.md`.
- No P0-P2 findings remain.
- Honest note: final release smoke still needs a real Windows/Ubuntu media preview check.

## Verification Results

- `cargo test --lib wsl_agent`: passed, 22 tests.
- `cargo test --lib bus -- --test-threads=1`: passed, 42 tests.
- `npm test -- src/bus/contract.test.ts src/panels/file/FileView.test.tsx src/panels/file/MediaView.test.tsx`: passed, 38 tests.
- `cargo build --bin tinto-agent`: passed.
- `npx tsc --noEmit`: passed.
- Work package checker: passed.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust protocol/runtime tests, frontend contract tests, TypeScript.
- Preventive evidence: local verification.
- If CI breaks: direct evidence-first triage.
- Escalation rule: final release remains blocked until Windows/Ubuntu smoke covers media preview or the gap is explicitly waived.

## Branch and PR Handoff Inputs

- Review unit: RU1 WSL media command parity.
- Branch name: `feat/wsl-media-preview`
- Branch/docs rule: keep docs with implementation for final batch.
- PR base: `develop`.
- Suggested commit grouping for this review unit:
  - `feat(files): preview media from WSL repos` - backend protocol/routing/tests - enables existing media preview command for WSL repos.
  - `docs(wsl): record media preview parity` - package/state/review docs - records verification and remaining smoke gap.
- PR title: `Preview media from WSL repos`
- PR body bullets:
  - Routes existing PDF/image preview reads through the WSL agent for Ubuntu repos.
  - Preserves local repo behavior, media extension allowlist, `.git` rejection, and 12 MiB read guard.
- Verification results location: this work package and review findings.
- Production/deployment notes: Windows/Ubuntu media smoke remains final-release evidence.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea
- Suggested subtask behavior: standalone `Tarea` unless a real multi-child parent exists.
- Jira summary: `Permitir previsualizar PDFs e imagenes en repos WSL`
- Jira description: `Hacer que la previsualizacion existente de PDF e imagenes funcione tambien en repos Ubuntu WSL, manteniendo las mismas guardas de seguridad y contrato del frontend.`
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" in state/release closeout and continue without asking solely whether Jira should be used.
