---
title: Workbench selection and management completion
status: ci-prevention-ready
roadmap_item: RDM-014
origin_roadmap: docs/roadmaps/2026-06-26-005-workbench-management-completion-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-26-014-workbench-management-completion.md
origin_planning_input: docs/brainstorms/2026-06-26-014-workbench-management-completion.md
origin_plan: docs/plans/2026-06-26-014-workbench-management-completion-plan.md
units: [U1, U2, U3, U4, U5]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Workbench selection and management completion

## Scope

Complete the frontend workbench management experience: MRU workbench ordering, top-level Workbench menu, create/activate flows without browser prompts, post-create Dashboard focus, manage-workbenches modal with active-workbench context, inline rename, delete with confirmation, active-workbench promotion after delete, partial-config tolerance, and focused tests.

## Non-goals

- No backend schema changes.
- No new Tauri command names beyond consuming existing rename/delete commands from the frontend client.
- No repo alias editor.
- No per-workbench layouts.
- No deletion of repos from disk.
- No PR/Jira mutation from this work phase.

## Autonomy Contract

- Mode: guarded.
- Agent may decide without asking: component-local structure, CSS details matching current modal/menu patterns, focused test organization, and exact local verification command selection.
- Agent must record as assumptions: MRU-by-name is sufficient, first remaining workbench is the promotion target after deleting active, and prototype posture allows focused frontend verification.
- Agent must escalate: backend command semantic changes, physical repo deletion, public config schema changes, branch/base strategy, PR/Jira workflow, or scope expansion into repo alias editing/per-workbench layouts.
- Safe fallback: keep backend behavior unchanged; if delete or rename semantics are ambiguous, stop at UI/test/docs and ask.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies

- Requires: RDM-005 workbench manager and RDM-007 dashboard/workbench UI.
- Blocks: none directly; enables future multi-workbench UX without manual TOML edits.

## Production Posture

- Posture: prototype.
- Evidence: current Compound Master state and prior work packages treat Tinto as a prototype desktop app.
- Confidence: high.
- Consequences for this package: compatibility with current config and frontend flows matters, but no deployment migration/rollback plan is required.
- Breaking existing behavior allowed: no for existing workbench CRUD semantics; yes for replacing the old select with a richer Workbench menu.

## Plan Unit Alignment

| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 MRU ordering helper | yes | Needed by both menu and modal ordering. |
| U2 Workbench menu entrypoints | yes | Main selection/create/manage entry surface. |
| U3 Workbench operation wrappers | yes | Shared flow layer for menu/modal actions. |
| U4 Manage workbenches modal | yes | Primary inspection/rename/delete surface. |
| U5 Review and verification | yes | Required before amend. |

Grouping rationale:
- One review unit is chosen because the MRU helper, menu, wrappers, modal, and tests form one cohesive user-visible capability. Splitting would leave partial surfaces that are hard to verify independently. The runtime diff is frontend-focused and uses existing backend commands.

## Implementation Units

- U1. MRU helper in `src/workbench/recentWorkbenches.ts`.
- U2. Workbench menu in `src/workbench/MenuBar.tsx`.
- U3. Rename/delete/create/activate flows in `src/workbench/operations.ts` and `src/bus/client.ts`.
- U4. Manage modal and styling in `src/workbench/ManageWorkbenchesDialog.tsx` and `src/App.css`.
- U5. Tests, review finding, work-package checker, and amend-ready verification.

## Review Units

| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Workbench selection and management completion | `src/workbench/**`, `src/bus/client.ts`, `src/App.css`, focused tests, orchestration docs | `develop` | optional standalone Tarea; omitted unless Jira context exists | Medium frontend slice; destructive config action is confirmation-gated and backend-validated. |

## Reviewability Diagnosis

- Reviewer-experience check: yes. RU1 can be understood as one capability: "manage workbenches from the menu/modal."
- Granularity chosen because: the coarsest independently mergeable capability is the complete management flow; smaller splits would be plumbing without usable UI or UI without shared wrappers.
- Open-stack plan: independent PR/local amend; no stack.
- Jira mapping: single-review-unit PR/local amend -> standalone Tarea if Jira is used.
- Downstream-fix trace: none.
- Failure-mode check: not a deep micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests

- Runtime/UI: `src/workbench/recentWorkbenches.ts`, `src/workbench/MenuBar.tsx`, `src/workbench/ManageWorkbenchesDialog.tsx`, `src/workbench/operations.ts`, `src/bus/client.ts`, `src/App.css`.
- Tests: `src/workbench/recentWorkbenches.test.ts`, `src/workbench/workbench.test.tsx`, `src/workbench/ManageWorkbenchesDialog.test.tsx`, `src/workbench/operations.test.ts`, `src/workbench/wslAbsence.test.ts`.
- Docs: roadmap, brainstorm, plan, this work package, review findings, state update.

## Impact Scan

- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: frontend client now consumes existing `rename_workbench` and `delete_workbench` commands; no backend payload/schema change.
- Consumer scan patterns: `rg "renameWorkbench|deleteWorkbench|workbenches|ManageWorkbenches|recent-workbenches" src src-tauri docs`.
- Consumers found: `MenuBar`, `ManageWorkbenchesDialog`, `operations`, backend workbench commands, workbench tests, WSL absence gate.
- Contract-drift tests searched: frontend workbench tests, operations tests, WSL absence tests, backend command registration references.
- Required consumer tests: focused Vitest for workbench/menu/modal/operations/absence plus TypeScript build.
- Consumer tests run/skipped: work-package checker passed; focused Vitest 130/130 passed; focused Prettier check passed; `npx tsc --noEmit` passed; `npm run build` passed with existing large-chunk warning. CI follow-up after GitHub Actions run `28237903708` passed locally with `npm test -- src/workbench/wslAbsence.test.ts --run` (73/73), full `npm test` (399/399), targeted Prettier check for touched files, `npm run lint` with existing warnings only, `npm run build`, and `git diff --check`. WSL dev-smoke follow-up proved the current CI `tinto-agent-linux-x86_64` artifact handles handshake, directory listing, and repo snapshot for `/home/teb/air-institute/digital-product-passport`; `npm run tauri:dev:wsl -- --dry-run` now resolves that artifact and avoids WSL source builds. WSL remove/recovery follow-up fixed stale frontend config causing WSL entries such as `/home/teb` to be forgotten from the live bus but not removed from persisted TOML, added one-shot retry for pooled WSL agent `ChildExit` / closed stdout limited to read-only/availability requests, and made Prettier `endOfLine: "auto"` so `npm run format:check` passes on this Windows checkout without formatting unrelated files.

## Verification Gate

- `python3 /home/teb/.agents/skills/krt-compound-master/scripts/check_work_package.py docs/work-packages/RDM-014-workbench-management-completion/2026-06-26-014-workbench-management-completion-work-package.md`
- `npm test -- src/workbench/recentWorkbenches.test.ts src/workbench/workbench.test.tsx src/workbench/ManageWorkbenchesDialog.test.tsx src/workbench/operations.test.ts src/workbench/wslAbsence.test.ts`
- `npx prettier --check src/workbench/recentWorkbenches.ts src/workbench/recentWorkbenches.test.ts src/workbench/MenuBar.tsx src/workbench/workbench.test.tsx src/workbench/ManageWorkbenchesDialog.tsx src/workbench/ManageWorkbenchesDialog.test.tsx src/workbench/operations.ts src/workbench/operations.test.ts src/bus/client.ts src/App.css`
- `npx tsc --noEmit`
- Surface-aware evidence: MRU/helper behavior covered by `recentWorkbenches.test.ts`; menu/modal behavior covered by `workbench.test.tsx` and `ManageWorkbenchesDialog.test.tsx`; operation wrapper behavior covered by `operations.test.ts`; WSL absence guard covered by `wslAbsence.test.ts`; TypeScript/build gates passed.
- Latest CI follow-up evidence: run `28237903708` failed in `Frontend / Test` because the strict WSL absence guard saw WSL text in a RepoPanel comment and in the intentional `src/panels/agentAvailability.ts` environment-key helper. The local fix keeps WSL text out of `RepoPanel.tsx` and allowlists the helper because it is required for shared `host` / `wsl:<distro>` agent availability caching.
- Latest WSL dev evidence: `tauri dev` source fallback can fail when Ubuntu lacks GTK/Cairo pkg-config packages, so `scripts/tauri-dev-wsl.mjs` resolves a downloaded Linux agent artifact and exports `TINTO_WSL_AGENT_LINUX_BIN` before launching Tauri. Manual smoke is running through `npm run tauri:dev:wsl`; the persisted config now keeps only valid WSL repo `/home/teb/chat-n-food`, direct agent snapshot for that repo is green (`main`, `initial commit`), and `/home/teb` was removed from `workbenches.toml`.
- Latest WSL remove/stdout verification: `npm run format:check` passed; `npm run lint` passed with existing warnings only; `npm test -- --run` passed 403/403; focused Vitest for WSL remove/add/Dashboard paths passed 52/52; `npm run build` passed with existing chunk warning only; `cargo fmt --check` passed; `cargo clippy --all-targets -- -D warnings` passed; `cargo test` passed 225/225; focused `cargo test wsl_agent::launcher -- --test-threads=1` passed 13/13; `cargo build --target-dir target/codex-build-check` passed; `node --check scripts/tauri-dev-wsl.mjs` passed; `npm run tauri:dev:wsl -- --dry-run` resolved `.ci-artifacts\28237903708\tinto-agent-linux-x86_64`; direct WSL agent handshake plus `repo_snapshot_with_fs_events` for `/home/teb/chat-n-food` returned `main`, `5feb2b38`, `initial commit`, and no repo error; native Tauri window capture showed `chat-n-food` loaded on Dashboard/Repo tree with branch `main` while `/home/teb` no longer appeared; `git diff --check` passed with CRLF warnings only.
- Production posture evidence: prototype; no deployment, migration, auth, network, or public API surface touched.

## Review Gate

- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.
- Current review result: two P2 findings fixed and verified; see `docs/review-findings/2026-06-26-rdm-014-code-review.md`.

## Security Gate

- Run after work-review loop: not required as a separate security review; this is local UI over existing config commands.
- Security Watch during work: enabled lightly because delete is destructive for config state.
- Security Watch notes: delete copy and tests must state repos on disk are not removed; backend command only removes the workbench config entry.
- Security reviewer: inline.
- Security review result: pass.
- Required security verification: focused delete tests and confirmation behavior.

## CI Break-Prevention And Escalation

- CI risk surfaces: frontend typecheck, Vitest, Prettier formatting, app build.
- Preventive evidence: local frontend CI-equivalent steps pass (`npm run format:check`, `npm run lint`, `npm test -- --run` 403/403, focused affected Vitest 52/52, `npm run build`); local Rust CI-equivalent steps pass (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 225/225, focused launcher test 13/13, `cargo build --target-dir target/codex-build-check`); dev WSL launcher dry-run resolves a current Linux agent artifact, direct Ubuntu agent smoke proves `/home/teb/chat-n-food` snapshots through the CI artifact, and native Tauri window capture proves the repo is visible in-app. CI-only gap remains full cross-platform Tauri packaging and replacement GitHub Actions proof after push.
- If CI breaks: invoke `krt-ci-questor` with run/check context; do not poll checks in Compound Master.
- Escalation rule: record a release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs

- Review unit: RU1 - Workbench selection and management completion.
- Branch name: `feat/workbench-management-completion`
- Branch/docs rule: related planning artifacts stay with the capability. User requested amend, so no standalone docs branch.
- PR base: `develop`.
- Suggested commit grouping for this review unit:
  - `feat(workbench): add recent workbench menu and management modal` - MRU helper, Workbench menu, modal, operation wrappers, styles, and tests.
  - `fix(workbench): avoid nested interactive controls in rename flow` - modal rename markup review fix.
  - `docs(orchestration): add workbench management completion artifacts` - roadmap, brainstorm, plan, package, review findings, state.
- PR title: `Workbench management from the app menu`
- PR body bullets:
  - Adds recent-workbench ordering and a Workbench menu for switching plus one management entrypoint.
  - Adds a manage-workbenches modal for inspection, activation, rename, delete, and create-with-Dashboard-return.
  - Keeps repo deletion out of scope; deleting a workbench only removes config membership.
  - Adds focused tests for MRU, menu, modal, operations, post-create Dashboard focus, and partial config recovery.
- Verification results location: this package Verification Gate and `docs/review-findings/2026-06-26-rdm-014-code-review.md`.
- Production/deployment notes: none.
- Autonomous mutation request: none.

## Jira Handoff Inputs

- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone Tarea if Jira is configured.
- PR-to-Jira mapping: single review unit maps to one standalone Tarea.
- Jira summary: Gestionar espacios de trabajo desde el menu de la aplicacion
- Jira description: Completar la gestion de espacios de trabajo en Tinto con menu de recientes, creacion, activacion, inspeccion, renombrado y eliminacion segura de workbenches, sin tocar repositorios en disco.
- Optional-policy fallback: Jira omitted unless existing Jira context/config is available.
