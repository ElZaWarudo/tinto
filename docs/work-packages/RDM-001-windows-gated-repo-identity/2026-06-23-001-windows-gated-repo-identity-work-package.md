---
title: Windows-gated repo identity and backend boundary
status: review-passed
roadmap_item: RDM-001
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md
origin_planning_input: docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md
origin_plan: docs/plans/2026-06-23-001-windows-gated-repo-identity-plan.md
units: [U1, U2, U3]
unit_alignment: complete
review_units: [RU1, RU2, RU3]
base_branch: develop
pr_strategy: local-fast-forward
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Windows-gated repo identity and backend boundary

## Scope

Implement the RDM-001 foundation for a Windows-only WSL complement: internal repo-source identity, persisted/runtime config projection, guarded local backend routing, strict Linux absence, and local-regression coverage.

## Non-goals

- No `wsl.exe` launch.
- No `tinto-agent` binary.
- No WSL repo picker or WSL-facing UI on any platform.
- No WSL read/watch implementation.
- No public WSL bus identity, unless a reviewed implementation detail requires an additive compatibility field.
- No WSL routing for media preview, Gitleaks/secret findings, file operations, or agent console sessions.
- No SSH, cloud, container, or generic remote repo support.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: local naming of internal helper types, test fixture structure, exact placement of a shared guard module, and equivalent targeted verification commands.
- Agent must record as assumptions: any internal compatibility choice that preserves public local-repo behavior but affects future WSL source integration.
- Agent must escalate: public bus contract additions, product-visible WSL UI, Linux-visible WSL behavior, destructive config migration, branch/base strategy changes, or any implementation that launches/probes WSL.
- Safe fallback: continue local compatibility tests and guard extraction that do not depend on the blocked decision; otherwise return the exact question.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: reviewed roadmap, reviewed requirements, reviewed plan.
- Blocks: RDM-002 Windows WSL agent bootstrap, RDM-004 core WSL read/watch path, RDM-005 capability policy.

## Production Posture

- Posture: prototype.
- Evidence: `docs/orchestration/compound-master-state.md` records prototype posture for the app and post-closeout iterative delivery.
- Confidence: high.
- Consequences for this package: compatibility with current local repo behavior is still required, but no production migration/rollback plan is needed.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Repo source model and persisted/runtime config projection are the first foundation. |
| U2 | yes | Shared repo-source guard/routing seam is required before later WSL code can safely avoid local path/git handling. |
| U3 | yes | Linux absence and local regression coverage are binding user requirements and must ship with the foundation. |

Grouping rationale:
- The units together form one capability foundation, but review is split into three focused review units to keep conceptual model, routing changes, and absence/regression coverage separately reviewable.
- RU1 should land first because RU2 and RU3 depend on the source model and persisted/runtime projection.
- RU2 is a separate review because it touches multiple repo-scoped backend command families, secret-scan/Gitleaks command call sites, and bus runtime mount/recalc paths.
- RU3 is separated if test/gate/docs changes would obscure RU2; if implementation shows RU3 is tiny, it may be grouped with RU2 only after preserving reviewability.

## Implementation Units

- U1 - Repo Source Model and Config Compatibility.
- U2 - Repo-Scoped Guard and Local Backend Routing Seam.
- U3 - Windows Gate and Linux Absence Regression Coverage.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Internal repo-source model, local config compatibility, persisted/runtime projection | `src-tauri/src/workbench/mod.rs`, `src-tauri/src/workbench/commands.rs`, possible source helper module, workbench tests, contract docs if needed | `develop` | optional Tarea | Medium risk: config compatibility and future WSL entries must not leak into Linux runtime/UI. |
| RU2 | Shared guard/router for repo-scoped backend paths, local behavior preserved | `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/bus/secret_scan.rs` or command call sites, `src-tauri/src/file_ops/commands.rs`, `src-tauri/src/agent_console/commands.rs`, guard tests | RU1 branch/base | optional Tarea/subtask | Higher risk: multiple backend command families, but one focused behavior: unsupported future WSL source fixtures never enter local FS/git/secret-scan/session paths. |
| RU3 | Linux absence, no WSL-facing UI in RDM-001, regression tests and compatibility note | `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/bus/contract.ts`, workspace/menu/dashboard tests if applicable, `docs/contracts/bus-contract.md` or focused note | RU2 branch/base | optional Tarea/subtask | Mostly verification/gates/docs; may be grouped with RU2 only if still reviewable. |

## Review Unit Progress

| Review unit | Status | Notes |
|---|---|---|
| RU1 | review-passed | Implemented and verified on 2026-06-23. Code review findings recorded in `docs/review-findings/2026-06-23-rdm-001-ru1-code-review.md`; no remaining P0-P2 findings known. |
| RU2 | review-passed | Implemented and verified on 2026-06-23. Shared bus resolver blocks unsupported future WSL fixtures before local canonicalization/git/file/session paths; review findings recorded in `docs/review-findings/2026-06-23-rdm-001-ru2-ru3-code-review.md`. |
| RU3 | review-passed | Implemented and verified on 2026-06-23. Linux/non-WSL absence checks cover command registration and all non-test frontend runtime TS/TSX sources; public WSL bus identity remains deferred in `docs/contracts/bus-contract.md`. |

## Reviewability Diagnosis

- Reviewer-experience check: yes. RU1 reviews identity/config shape; RU2 reviews backend guard/routing; RU3 reviews absence/regression evidence.
- Granularity chosen because: each unit has different risk and verification. The split is for human reviewability and independent verification, not for Jira shape.
- Open-stack plan: default delivery is local fast-forward into `develop` with no PR, matching current project preference. If the user explicitly requests PR flow, use a shallow stack target 2, max 2; if RU1 is not merged/released before RU3 is ready, wait for parent integration or collapse RU2/RU3 only if the combined diff remains reviewable.
- Jira mapping: optional Jira. If Jira is unavailable, record omission. If used, one parent with subtasks only if two or more review units are tracked together; otherwise standalone Tarea per review unit.
- Downstream-fix trace: none yet.
- Failure-mode check: not a deep micro-PR stack and not a deferred mega-consolidation PR. RU2/RU3 may be grouped only when that improves review.

## Files and Tests

Expected files:
- `src-tauri/src/workbench/mod.rs`
- `src-tauri/src/workbench/commands.rs`
- `src-tauri/src/bus/mod.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/bus/secret_scan.rs` or the command call sites that invoke secret scanning/Gitleaks setup
- `src-tauri/src/file_ops/commands.rs`
- `src-tauri/src/agent_console/commands.rs`
- possible new `src-tauri/src/repo_source.rs` or local module
- `src/bus/contract.ts`
- `src/bus/client.ts`
- `src/bus/store.ts`
- affected tests near those surfaces
- `docs/contracts/bus-contract.md` or a focused compatibility note if public contract docs need an additive note

Expected tests:
- Workbench config compatibility and persisted/runtime projection.
- Local repo config load/add/remove/update/reorder/switch flows.
- WSL future-entry preservation on Linux without runtime/UI exposure.
- Internal `RepoSource::Wsl` fixture blocked before local path/git/file/session paths.
- Local backend path still works.
- No WSL-facing UI in RDM-001 on any platform and no Linux WSL command/runtime surface.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: likely yes for internal workbench config schema/helpers and repo-scoped command guards; public bus contract changes should be avoided unless explicitly justified in implementation.
- Consumer scan patterns: `rg "RepoEntry|RepoDelta|SubscriptionTarget|repo: PathBuf|is_known|canonicalize|Git2Engine|list_workbenches|set_active_workbench|watch_workbench|start_agent_session|copy_to_repo|get_media_content|secret_scan|gitleaks|create_repo_gitleaks_config" src src-tauri docs/contracts`.
- Consumers found: expected in `src-tauri/src/workbench/*`, `src-tauri/src/bus/*`, `src-tauri/src/file_ops/*`, `src-tauri/src/agent_console/*`, `src/bus/*`, dashboard/repo panels, and contract tests.
- Contract-drift tests searched: Rust serialization tests in `src-tauri/src/bus/contract.rs`; TS contract tests in `src/bus/contract.test.ts`; workbench config tests in `src-tauri/src/workbench/mod.rs`.
- Required consumer tests: frontend bus/store/contract tests and backend workbench/bus/file_ops/agent_console tests listed below.
- RU1 result: complete for internal workbench config/schema/helper projection. Public bus contract unchanged; frontend `RepoEntry` contract remains local-visible only because `list_workbenches` returns runtime projection.
- RU1 consumers found: `src-tauri/src/workbench/mod.rs`, `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs`, bus/watcher test fixtures using `RepoEntry`, frontend `src/bus/contract.ts`, `src/bus/store.ts`, `src/workbench/*`, `src/panels/RepoCard*`, `src/panels/RepoPanel*`, and existing bus/file_ops/agent_console command families for later RU2 guard work.
- RU1 consumer tests run/skipped: workbench, startup helper, bus, watcher, agent_console, frontend contract/store/workbench/repo panels, and typecheck run. `file_ops` filter matched 0 tests; RU2 still owns explicit file_ops guard tests.
- RU2/RU3 result: complete for repo-scoped command guard routing and Linux/non-WSL absence. Public bus contract received only a compatibility note: public WSL repo identity is deferred beyond RDM-001; current public `repo` values remain local canonical paths only.
- RU2/RU3 consumers found: `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/file_ops/commands.rs`, `src-tauri/src/agent_console/commands.rs`, `src-tauri/src/lib.rs`, `docs/contracts/bus-contract.md`, frontend bus/workbench/panel runtime sources scanned by `src/workbench/wslAbsence.test.ts`.
- RU2/RU3 contract-drift tests searched/run: Rust bus command guard tests, agent-console error mapping tests, invoke-handler/startup tests, frontend raw-source absence test, frontend bus/contract/store/workbench/repo panel tests, and TypeScript typecheck.
- RU2/RU3 run/skipped results: complete. `file_ops` uses the shared `ensure_known` guard; its filtered suite still has 0 matching tests, but full serialized Rust lib tests cover compile/runtime integration of the shared command path.

## Verification Gate

- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`
- `npx tsc --noEmit`
- `cd src-tauri && cargo test --lib workbench`
- `cd src-tauri && cargo test --lib bus`
- `cd src-tauri && cargo test --lib file_ops`
- `cd src-tauri && cargo test --lib agent_console`
- Any new absence/source-gate tests added by the implementation.
- Surface-aware evidence: every changed repo identity/config/guard/UI absence surface must have either a targeted test or a documented reason why an existing test covers it.
- Production posture evidence: prototype, but local repo behavior compatibility is mandatory.

RU1 verification results:
- `cargo test --lib workbench`: 30 passed.
- `cargo test --lib bus`: 37 passed.
- `cargo test --lib watcher`: 29 passed.
- `cargo test --lib agent_console`: 35 passed.
- `cargo test --lib file_ops`: 0 matched, 172 filtered.
- `cargo test --lib bus::tests::ae8_repo_removido_estado_terminal`: 1 passed after one full-suite timing failure.
- `cargo test --lib -- --test-threads=1`: 172 passed.
- `cargo fmt --check`: passed.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`: 77 passed.
- `npx tsc --noEmit`: passed.

RU1 verification notes:
- Two default/parallel Rust full-suite runs hit existing timing-sensitive watcher/bus tests in different places. Targeted tests passed, and the full library suite passed when serialized with `--test-threads=1`.
- No WSL launch/probe, `tinto-agent`, WSL UI, public WSL bus identity, media/Gitleaks/file/session WSL routing, or Windows path translation was added.
- Runtime projection preserves future WSL entries on disk, hides workbenches that contain only unsupported sources, remaps runtime `active` to the first visible workbench when needed without persisting that remap, and clears malformed local `distro` values from runtime output.
- Local add/remove/update/reorder flows match only `RepoSource::Local`, so hidden future WSL entries cannot block or be mutated by local path commands.

RU2/RU3 verification results:
- `cargo test --lib wsl_source`: 3 passed.
- `cargo test --lib unsupported_wsl`: 2 passed.
- `cargo test --lib unsupported_repo_resolve_error_maps_to_safe_category`: 2 passed.
- `cargo test --lib invoke_handler_does_not_register_wsl_commands_for_rdm_001`: 1 passed.
- `cargo test --lib initial_runtime_repos`: 2 passed.
- `cargo test --lib -- --test-threads=1`: 179 passed.
- `cargo fmt --check`: passed.
- `npm test -- src/workbench/wslAbsence.test.ts`: 63 passed.
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/workbench/wslAbsence.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`: 140 passed.
- `npx tsc --noEmit`: passed.

RU2/RU3 verification notes:
- `BusHandle::resolve_repo` centralizes repo command allowlisting and returns `unsupported_repo_source` for hidden future WSL entries before local canonicalization.
- `Subscribe` and `RetryRepo` now route through the same resolver, preventing unsupported WSL aliases such as `local_repo/.` from canonicalizing into a mounted local repo.
- Frontend absence coverage uses Vite raw-source glob scanning for every non-test, non-declaration `.ts`/`.tsx` file under `src`, so newly added runtime UI/settings/empty-state files are covered without maintaining a hand-written file list.
- New and changed functions/test names and comments added for RDM-001 are in English; user-facing existing Spanish error strings were left unchanged.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- RU1 code review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-ru1-code-review.md`.
- RU2/RU3 code review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-ru2-ru3-code-review.md`.

## Security Gate

- Run after work-review loop: not required by default because this package should not add new repo read/write authority or launch WSL.
- Security Watch during work: enabled lightly for any change that could allow unsupported future WSL sources into local file/git/session paths.
- Security Watch notes: ensure `unsupported_repo_source` happens before local path handling for WSL fixtures.
- Security reviewer: fallback inline unless implementation adds unexpected filesystem authority.
- Security review result: passed for RU2/RU3 via read-only security reviewer. No P0-P2 findings.
- Required security verification: completed for current RDM-001 surfaces. WSL fixtures are hidden from snapshots/runtime mount and blocked from shared command resolution before local filesystem/git/session paths; no WSL launch/probe or `tinto-agent` registration was added.

## CI Break-Prevention And Escalation

- CI risk surfaces: Rust tests, TypeScript typecheck, frontend unit tests, config serialization, command registration, frontend absence checks.
- Preventive evidence: targeted tests above; full `npm test` and broader `cargo test` may be run if touched surfaces expand.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: release follow-up is blocked until CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RDM-001 package - RU1, RU2, and RU3.
- Branch name: `feat/windows-wsl-repo-identity`
- Branch/docs rule: first executable review unit carries this package and related planning artifacts on the same semantic branch; do not ship a separate docs-planning branch.
- Integration base: `develop`
- Delivery default: local fast-forward into `develop` and push, no PR, unless the user explicitly requests PR flow.
- Suggested commit grouping for this review unit:
  - `feat(workbench): add source-aware repo identity foundation` - workbench config/model/tests.
  - `test(workbench): preserve local repo compatibility and hidden future sources` - focused compatibility fixtures if large enough to split.
  - `docs(orchestration): add Windows WSL identity artifacts [skip ci]` - roadmap/requirements/plan/package artifacts if release flow keeps docs in a separate commit.
- PR title if PR flow is explicitly requested: `Add source-aware repo identity foundation`
- PR body bullets if PR flow is explicitly requested:
  - Add a source-aware repo identity foundation for the Windows-only WSL complement.
  - Preserve existing local repo behavior and config compatibility.
  - Keep future WSL entries absent from Linux runtime/UI surfaces.
- Verification results location: package closeout/state and `docs/review-findings/2026-06-23-rdm-001-ru2-ru3-code-review.md`.
- Production/deployment notes: none beyond prototype compatibility requirement.
- Autonomous mutation request: none.

### Completed Review Unit Handoff Inputs

- RU2 branch name: `feat/windows-wsl-repo-guard`
- RU2 integration base: RU1 integrated into `develop` or explicit parent branch if PR flow is requested.
- RU2 suggested commit grouping:
  - `feat(repo): guard unsupported future repo sources before local handling` - bus/file_ops/agent_console/secret-scan command call sites and guard tests.
  - `test(repo): block future WSL fixtures from local git and file paths` - focused guard fixtures if large enough to split.
- RU2 PR title if PR flow is explicitly requested: `Guard future WSL sources before local repo handling`
- RU2 PR body bullets if PR flow is explicitly requested:
  - Add a shared repo-source guard before local filesystem/git/session paths.
  - Keep current local repo command behavior unchanged.
  - Cover secret-scan/Gitleaks, file operations, media reads, and agent-console paths.
- RU3 branch name: `test/windows-wsl-linux-absence`
- RU3 integration base: RU2 integrated into `develop` or explicit parent branch if PR flow is requested.
- RU3 suggested commit grouping:
  - `test(wsl): prove complement absence outside Windows` - frontend/backend absence and regression tests.
  - `docs(contracts): record WSL public identity deferral` - compatibility note if needed.
- RU3 PR title if PR flow is explicitly requested: `Prove WSL complement absence outside Windows`
- RU3 PR body bullets if PR flow is explicitly requested:
  - Add absence coverage for non-Windows WSL command/UI/runtime surfaces.
  - Keep RDM-001 free of WSL-facing picker UI on all platforms.
  - Document that public WSL bus identity is deferred to the read/watch slice.

RU2/RU3 completion note:
- RU2 and RU3 were implemented together after RU1 because the backend guard and absence tests are tightly coupled for this Windows-only complement foundation.
- Review passed after fixing command-path and coverage findings. No remaining P0-P2 findings are known.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: create/reuse subtasks only if multiple review units are tracked under one parent; otherwise standalone Tarea.
- PR-to-Jira mapping: Jira unit is review unit.
- Jira summary: `Preparar identidad de repos para el complemento WSL en Windows`
- Jira description: `Agregar la base interna para distinguir repos locales de futuros repos WSL en Windows, preservando el comportamiento local actual y manteniendo el complemento ausente en Linux.`
- Optional-policy fallback: if Jira role/config/context is missing, record `Jira omitted: jira-env-not-configured` in state/release closeout and continue.
