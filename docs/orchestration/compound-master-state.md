---
title: Compound Master State - Tinto
status: active
date: 2026-06-24
initiative: windows-only-wsl-complement
mode: artifacts
production_posture: prototype
state_format: compact
last_compacted: 2026-06-16
archive_snapshot: docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md
final_summary: docs/orchestration/2026-06-16-compound-master-summary.md
---

# Compound Master State - Tinto

## Resume Snapshot

- Current phase/status: Windows-only WSL repo complement initiative, RDM-001 RU1/RU2/RU3 implemented, reviewed, and delivered to `origin/develop`; RUL-001, RDM-002 through RDM-011, and the RDM-006 final installer/resource follow-up are implemented, verified, security-reviewed where required, review-passed, committed, and pushed to `origin/develop` in the final batched release. The release was pushed directly to `develop` without PR or Jira, matching standing project preference and the approved oversized batch release plan. CI is green on validated code tip `3a994ef` (`fix(wsl): preserve shell vars during agent install`) with final Windows bundle artifacts downloaded locally. Windows/Ubuntu-family WSL packaged smoke passed for app launch, MSI contents, `Ubuntu-24.04` repo tracking, and WSL agent backend operations. Remaining honest gap: packaged UI Agent Console start/stop/change-log/revert still lacks interactive desktop evidence.
- Latest local package: RDM-006 final WSL installer resource and smoke closure is implemented, verified, security-reviewed, review-passed, CI-passed, and smoke-tested on host. Tauri bundles `src-tauri/resources/` to the resource root, CI downloads the Ubuntu-built `tinto-agent-linux-x86_64` before bundle jobs, CI verifies the resource exists before bundling, and the Windows bundle job uploads MSI/NSIS artifacts. Follow-up fixes accepted Ubuntu-family distro names such as `Ubuntu-24.04`, routed WSL bus/read/file-operation requests through each repo's stored distro, preserved the WSL Agent Console distro for checkpoint scan/revert, forced Tauri to package `tinto.exe` instead of auxiliary `tinto-agent.exe`, and escaped shell variables in the `wsl.exe -- sh -lc` install script so `$HOME`/`$install_dir` survive Windows-to-WSL argument passing. GitHub Actions run `28087369767` passed Frontend, Rust, Linux Tauri bundle, and Windows Tauri bundle on commit `3a994efce47ba2ff27349ac31ac6a838456ec9ac`. Downloaded artifacts live under `.ci-artifacts/28087369767/`: agent sha256 `c22bbd71c00918afd9256f4015376869365cf53804a218d4fe4418d816e9a140`, MSI sha256 `7948185c36175a4c5e3b73224f8edc645bb5f25a65b6ebbfaaed5734ffe73c0d`, NSIS sha256 `a7cb1f6d750ec6b8c9d30f3acf48a41bb8ce9086fca4acefdfc64c2e48c7515c`.
- Active roadmap: `docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md`. Roadmap review passed through approved document-review subagent fallback; findings recorded in `docs/review-findings/2026-06-23-wsl-complement-roadmap-review.md`. RDM-001 requirements, plan, and work package are reviewed and ready.
- Paused release-ready work: RUL-001 file overview ruler VS Code parity, RU1/RU2/RU3 implemented, verified, and review-passed. Release is intentionally deferred until all active work packages are complete; do not invoke Release Marshal for this partial batch unless the user changes that instruction. Latest dev fixture smoke served `http://127.0.0.1:1422/demo.html` with HTTP 200.
- Branch/base: `develop`; `origin/develop` is at `3a994efce47ba2ff27349ac31ac6a838456ec9ac` after final WSL packaged smoke fixes. Local docs evidence updates are pending commit/push.
- Open PR/Jira: none. Standing user preference: local fast-forward merge into `develop` and push, no PR. Jira omitted (`jira-env-not-configured`).
- Blockers: No blockers for implemented code/package local verification, CI, artifact production, or packaged Windows/Ubuntu-family WSL smoke at the Dashboard/backend-agent level. Remaining release-environment gap: a human desktop pass should still confirm Agent Console start/stop/change-log/revert through the packaged UI; backend checkpoint/file-operation parity already passed agent-level smoke. Local Windows installer build still cannot run because this machine lacks Visual Studio Build Tools/MSVC, but CI-produced Windows artifacts are available. RDM-002 planning blockers are resolved: user chose WSL 2 only, one selected distro per WSL repo, Ubuntu as the initial manual smoke distro, and dev-only build/run from source for the initial `tinto-agent` availability model on 2026-06-23. RDM-006 supersedes the release availability model with packaged Linux agent discovery/install first and explicit dev-source fallback. Canonical roles remain unavailable (`document-review`, `ce-brainstorm`, `ce-plan`), so this run records approved functional fallbacks: roadmap/artifact review via document-review subagent fan-out plus lead synthesis; brainstorm/requirements via `krt-requirements-weaver`; plan via `krt-delivery-navigator`. User decision captured on 2026-06-23: this is a Windows-only complement/add-on; when Tinto runs on Linux, no WSL UI, commands, empty states, settings, degraded notices, or behavior should appear.
- Required user decisions: no blocker for RDM-005 file operations; user chose to support tracking/projects that include both WSL and Windows repos, so repo-writing file commands are routed through the Linux agent for WSL repos. No blocker remains for WSL Agent Console launch or remote checkpoint/revert.
- Next action: commit and push these documentation evidence updates with `[skip ci]`; then decide whether the remaining Agent Console UI-only smoke gap needs a human/manual pass before declaring the WSL complement fully release-smoked.

## Source Documents

- Design source: `tinto-design.md`.
- Closed roadmaps: `docs/roadmaps/2026-06-10-001-tinto-roadmap.md`, `docs/roadmaps/2026-06-19-002-agent-console-integration.md`.
- Active WSL roadmap: `docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md`.
- Final closeout summary: `docs/orchestration/2026-06-16-compound-master-summary.md`.
- Full archived pre-compaction state: `docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md`.
- RUL-001: roadmap/brainstorm/plan/work package were recreated in this revision. The current work package and code-review findings are the source of truth for RU1/RU2/RU3.
- Latest RU1 review findings: `docs/review-findings/2026-06-23-rul-001-ru1-demo-review.md`.
- RUL-001 current requirements: `docs/brainstorms/2026-06-23-003-file-overview-ruler-parity.md`.
- RUL-001 current plan: `docs/plans/2026-06-23-003-file-overview-ruler-parity-plan.md`.
- RUL-001 current work package: `docs/work-packages/RUL-001-file-overview-ruler-parity/2026-06-23-003-file-overview-ruler-parity-work-package.md`.
- RUL-001 artifact review: `docs/review-findings/2026-06-23-rul-001-artifact-review.md`; mechanical checker passed.
- RUL-001 implementation/review: `review-passed` for RU1/RU2/RU3. Code review findings path: `docs/review-findings/2026-06-23-rul-001-code-review.md`. Verification: focused Vitest suite 28/28 passed, `npx tsc --noEmit` passed, `npx prettier --check <changed files>` passed, `git diff --check` passed, and Vite served `http://127.0.0.1:1422/demo.html` with HTTP 200. Automated screenshot verification skipped because Browser tooling was not exposed and Playwright/Puppeteer are not installed.
- RDM-002 WSL agent bootstrap requirements: `docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md`; status `reviewed`.
- RDM-002 requirements review findings: `docs/review-findings/2026-06-23-rdm-002-requirements-review.md`; status `passed`.
- RDM-002 plan: `docs/plans/2026-06-23-002-wsl-agent-bootstrap-protocol-plan.md`; status `plan-review-passed`.
- RDM-002 plan review findings: `docs/review-findings/2026-06-23-rdm-002-plan-review.md`; status `passed`.
- RDM-002 work package: `docs/work-packages/RDM-002-windows-wsl-agent-bootstrap-protocol/2026-06-23-002-wsl-agent-bootstrap-protocol-work-package.md`; status `review-passed`.
- RDM-002 work package review findings: `docs/review-findings/2026-06-23-rdm-002-work-package-review.md`; status `passed`; mechanical checker passed with an accepted warning that orchestration docs and runtime files are mixed but split into RU1/RU2/RU3.
- RDM-002 code review findings: `docs/review-findings/2026-06-23-rdm-002-code-review.md`; status `passed`.
- RDM-002 security review findings: `docs/review-findings/2026-06-23-rdm-002-security-review.md`; status `passed`.
- RDM-002 manual Windows/Ubuntu WSL smoke checklist: `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`; status `pending` until final batched release.
- RDM-003 WSL workbench path UX requirements: `docs/brainstorms/2026-06-23-003-wsl-workbench-path-ux.md`; status `reviewed`.
- RDM-003 requirements review findings: `docs/review-findings/2026-06-23-rdm-003-requirements-review.md`; status `passed`.
- RDM-003 plan: `docs/plans/2026-06-23-003-wsl-workbench-path-ux-plan.md`; status `plan-review-passed`.
- RDM-003 plan review findings: `docs/review-findings/2026-06-23-rdm-003-plan-review.md`; status `passed`.
- RDM-003 work package: `docs/work-packages/RDM-003-windows-wsl-workbench-path-ux/2026-06-23-003-wsl-workbench-path-ux-work-package.md`; status `review-passed`.
- RDM-003 work package review findings: `docs/review-findings/2026-06-23-rdm-003-work-package-review.md`; status `passed`; mechanical checker passed with an accepted warning that orchestration docs and runtime files are mixed but split into RU1/RU2/RU3.
- RDM-003 RU1 code review findings: `docs/review-findings/2026-06-23-rdm-003-ru1-code-review.md`; status `passed`.
- RDM-003 RU2 code review findings: `docs/review-findings/2026-06-23-rdm-003-ru2-code-review.md`; status `passed`.
- RDM-003 RU3 code review findings: `docs/review-findings/2026-06-23-rdm-003-ru3-code-review.md`; status `passed`.
- RDM-003 security review findings: `docs/review-findings/2026-06-23-rdm-003-security-review.md`; status `passed`.
- RDM-004 WSL read/watch requirements: `docs/brainstorms/2026-06-23-004-core-wsl-read-watch-path.md`; status `reviewed`.
- RDM-004 requirements review findings: `docs/review-findings/2026-06-23-rdm-004-requirements-review.md`; status `passed`.
- RDM-004 plan: `docs/plans/2026-06-23-004-core-wsl-read-watch-path-plan.md`; status `plan-review-passed`.
- RDM-004 plan review findings: `docs/review-findings/2026-06-23-rdm-004-plan-review.md`; status `passed`.
- RDM-004 work package: `docs/work-packages/RDM-004-core-wsl-read-watch-path/2026-06-23-004-core-wsl-read-watch-path-work-package.md`; status `review-passed`.
- RDM-004 work package review findings: `docs/review-findings/2026-06-23-rdm-004-work-package-review.md`; status `passed`; mechanical checker passed with an accepted warning that orchestration docs and runtime files are mixed but split into RU1/RU2.
- RDM-004 code review findings: `docs/review-findings/2026-06-23-rdm-004-code-review.md`; status `passed`.
- RDM-004 security review findings: `docs/review-findings/2026-06-23-rdm-004-security-review.md`; status `passed`.
- RDM-005 WSL file operations requirements: `docs/brainstorms/2026-06-23-005-wsl-file-operations.md`; status `reviewed`.
- RDM-005 plan: `docs/plans/2026-06-23-005-wsl-file-operations-plan.md`; status `plan-review-passed`.
- RDM-005 work package: `docs/work-packages/RDM-005-wsl-file-operations/2026-06-23-005-wsl-file-operations-work-package.md`; status `review-passed`.
- RDM-005 code/security review findings: `docs/review-findings/2026-06-23-rdm-005-code-security-review.md`; status `passed`.
- RDM-006 WSL packaging/recovery requirements: `docs/brainstorms/2026-06-23-006-wsl-packaging-recovery-verification.md`; status `reviewed`.
- RDM-006 plan: `docs/plans/2026-06-23-006-wsl-packaging-recovery-verification-plan.md`; status `plan-review-passed`.
- RDM-006 work package: `docs/work-packages/RDM-006-wsl-packaging-recovery-verification/2026-06-23-006-wsl-packaging-recovery-verification-work-package.md`; status `review-passed`.
- RDM-006 code/security review findings: `docs/review-findings/2026-06-23-rdm-006-code-security-review.md`; status `passed`.
- RDM-007 WSL media preview requirements: `docs/brainstorms/2026-06-23-007-wsl-media-preview.md`; status `reviewed`.
- RDM-007 plan: `docs/plans/2026-06-23-007-wsl-media-preview-plan.md`; status `plan-review-passed`.
- RDM-007 work package: `docs/work-packages/RDM-007-wsl-media-preview/2026-06-23-007-wsl-media-preview-work-package.md`; status `review-passed`.
- RDM-007 code/security review findings: `docs/review-findings/2026-06-23-rdm-007-code-security-review.md`; status `passed`.
- RDM-008 WSL Gitleaks parity requirements: `docs/brainstorms/2026-06-23-008-wsl-gitleaks-parity.md`; status `reviewed`.
- RDM-008 plan: `docs/plans/2026-06-23-008-wsl-gitleaks-parity-plan.md`; status `plan-review-passed`.
- RDM-008 work package: `docs/work-packages/RDM-008-wsl-gitleaks-parity/2026-06-23-008-wsl-gitleaks-parity-work-package.md`; status `review-passed`.
- RDM-008 code/security review findings: `docs/review-findings/2026-06-23-rdm-008-code-security-review.md`; status `passed`.
- RDM-009 WSL Agent Console requirements: `docs/brainstorms/2026-06-23-009-wsl-agent-console.md`; status `reviewed`.
- RDM-009 plan: `docs/plans/2026-06-23-009-wsl-agent-console-plan.md`; status `plan-review-passed`.
- RDM-009 work package: `docs/work-packages/RDM-009-wsl-agent-console/2026-06-23-009-wsl-agent-console-work-package.md`; status `review-passed`.
- RDM-009 code/security review findings: `docs/review-findings/2026-06-23-rdm-009-code-security-review.md`; status `passed`.
- RDM-010 WSL fs-events requirements: `docs/brainstorms/2026-06-23-010-wsl-fs-events.md`; status `reviewed`.
- RDM-010 plan: `docs/plans/2026-06-23-010-wsl-fs-events-plan.md`; status `plan-review-passed`.
- RDM-010 work package: `docs/work-packages/RDM-010-wsl-fs-events/2026-06-23-010-wsl-fs-events-work-package.md`; status `review-passed`.
- RDM-010 code/security review findings: `docs/review-findings/2026-06-23-rdm-010-code-security-review.md`; status `passed`.
- RDM-011 WSL Agent Console checkpoint requirements: `docs/brainstorms/2026-06-23-011-wsl-agent-console-checkpoints.md`; status `reviewed`.
- RDM-011 plan: `docs/plans/2026-06-23-011-wsl-agent-console-checkpoints-plan.md`; status `plan-review-passed`.
- RDM-011 work package: `docs/work-packages/RDM-011-wsl-agent-console-checkpoints/2026-06-23-011-wsl-agent-console-checkpoints-work-package.md`; status `review-passed`.
- RDM-011 code/security review findings: `docs/review-findings/2026-06-23-rdm-011-code-security-review.md`; status `passed`.
- RDM-006 final installer resource smoke requirements: `docs/brainstorms/2026-06-23-012-wsl-installer-resource-smoke.md`; status `reviewed`.
- RDM-006 final installer resource smoke plan: `docs/plans/2026-06-23-012-wsl-installer-resource-smoke-plan.md`; status `plan-review-passed`.
- RDM-006 final installer resource smoke work package: `docs/work-packages/RDM-006-final-wsl-installer-resource-smoke/2026-06-23-012-wsl-installer-resource-smoke-work-package.md`; status `review-passed`.
- RDM-006 final installer resource smoke review findings: `docs/review-findings/2026-06-23-rdm-006-final-code-security-review.md`; status `passed`.

## Current Compound Master Initiative - Windows-Only WSL Repo Complement

- Artifact kind: roadmap.
- Artifact path: `docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md`.
- Status: `review-passed` for package; RDM-001 RU1, RU2, and RU3 are all `review-passed`.
- User clarification: this is a complement/add-on, Windows-only. Linux builds/runs must keep the WSL feature absent and unreachable while preserving existing local Linux repo behavior.
- Roadmap review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-wsl-complement-roadmap-review.md`.
- RDM-001 brainstorm/requirements: `docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md`; review passed via approved fallback.
- RDM-001 requirements review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-requirements-review.md`.
- RDM-001 plan: `docs/plans/2026-06-23-001-windows-gated-repo-identity-plan.md`; review passed via approved fallback.
- RDM-001 plan review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-plan-review.md`.
- RDM-001 work package: `docs/work-packages/RDM-001-windows-gated-repo-identity/2026-06-23-001-windows-gated-repo-identity-work-package.md`; mechanical checker passed after final edits; document review passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-work-package-review.md`.
- RDM-001 RU1 implementation: review-passed after fixes. Changed files: `src-tauri/src/workbench/mod.rs`, `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/watcher/mod.rs`, plus package/state/review findings docs. Implementation adds internal `RepoSource`, local constructor/helpers, runtime workbench projection, startup/list/reseed projection helpers, source-aware local mutations, and tests for WSL persistence/runtime absence. Future WSL entries are preserved on disk but hidden from runtime/UI; WSL-only workbenches are hidden; runtime `active` remaps to the first visible workbench without persisting that remap; local commands do not mutate hidden WSL entries. No WSL launch/probe, `tinto-agent`, WSL UI, public WSL bus identity, or WSL routing was added.
- RDM-001 RU1 code review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-ru1-code-review.md`. Fixed P2 findings for path-only local mutations, WSL-only empty runtime workbenches, duplicated reseed filtering, malformed local `distro` runtime leakage, and missing boundary tests.
- RDM-001 RU1 Impact Scan: complete. Changed contract/helper surface: internal persisted workbench config schema and runtime projection; public bus contract unchanged. Scan pattern used: `rg "RepoEntry|RepoDelta|SubscriptionTarget|repo: PathBuf|is_known|canonicalize|Git2Engine|list_workbenches|set_active_workbench|watch_workbench|start_agent_session|copy_to_repo|get_media_content|secret_scan|gitleaks|create_repo_gitleaks_config" src src-tauri docs/contracts`. Consumers found in workbench, lib startup, bus/watcher fixtures, file_ops/agent_console command families for later RU2 guard coverage, frontend bus contract/store/workbench/repo panels, and contract docs.
- RDM-001 RU1 verification: `cargo test --lib workbench` 30 passed; `cargo test --lib bus` 37 passed; `cargo test --lib watcher` 29 passed; `cargo test --lib agent_console` 35 passed; `cargo test --lib file_ops` 0 matched/172 filtered; `cargo test --lib bus::tests::ae8_repo_removido_estado_terminal` 1 passed after one full-suite timing failure; `cargo test --lib -- --test-threads=1` 172 passed; `cargo fmt --check` passed; `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx` 77 passed; `npx tsc --noEmit` passed. Note: two default/parallel full Rust suite runs hit different existing timing-sensitive bus/watcher tests; targeted tests and serialized full suite passed.
- RDM-001 RU1 Security Watch: light pass complete. No new filesystem authority, external process launch, WSL probing, secret handling, public API/auth, or destructive operation was added. RU2 still owns explicit guard tests before local filesystem/git/secret-scan/file/session command paths.
- RDM-001 RU2 implementation: review-passed after fixes. Changed files: `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/commands.rs`, and `src-tauri/src/agent_console/commands.rs` plus shared consumers through `ensure_known`. Implementation adds `BusHandle::resolve_repo`, `RepoResolveError`, unsupported-entry tracking in the bus, source-aware resolver use for repo read/media/diff/file/session command allowlists, and safe `unsupported_repo_source` command error mapping. Future WSL entries are retained for guard lookup but are not mounted, watched, snapshotted, subscribed, retried, or routed into local git/filesystem/session paths.
- RDM-001 RU3 implementation: review-passed after fixes. Changed files: `src-tauri/src/lib.rs`, `src/workbench/wslAbsence.test.ts`, and `docs/contracts/bus-contract.md`. Implementation seeds the bus only from runtime-projected local repos, asserts no WSL/tinto-agent invoke commands are registered for RDM-001, scans every non-test/non-declaration frontend runtime TS/TSX source for WSL-facing UI/settings/errors, and records that public WSL repo identity is deferred beyond RDM-001. New and changed functions/test names/comments added for RDM-001 are in English; user-facing existing Spanish error strings were left unchanged.
- RDM-001 RU2/RU3 code review: passed after fixes. Findings path: `docs/review-findings/2026-06-23-rdm-001-ru2-ru3-code-review.md`. Fixed correctness P2 for `Subscribe`/`RetryRepo` bypassing unsupported-source resolution; fixed testing P1 for missing hidden WSL retry/subscription coverage; fixed testing P2 for hand-maintained frontend absence scan. Security reviewer found no P0-P2 findings.
- RDM-001 RU2/RU3 Impact Scan: complete. Changed contract/helper surface: internal bus resolver/command guard behavior, command error categories for unsupported sources, startup runtime repo seed, frontend absence regression surface, and a bus contract compatibility note. Scan patterns used: `rg "ResolveRepo|resolve_repo|unsupported_repo_source|UnsupportedRepoSource|RepoSource|source:|distro|is_runtime_supported|list_workbenches|invoke_handler|tinto_agent|tinto-agent|\\bwsl\\b" src src-tauri docs/contracts` and `rg "ensure_known|is_known\\(|resolve_repo\\(|CommandError::new|unsupported_repo_source" src-tauri/src src`. Consumers found in bus, bus commands, file_ops shared `ensure_known`, agent_console, lib startup/invoke registration, frontend bus/workbench/panel runtime files, and contract docs.
- RDM-001 RU2/RU3 verification: `cargo test --lib wsl_source` 3 passed; `cargo test --lib unsupported_wsl` 2 passed; `cargo test --lib unsupported_repo_resolve_error_maps_to_safe_category` 2 passed; `cargo test --lib invoke_handler_does_not_register_wsl_commands_for_rdm_001` 1 passed; `cargo test --lib initial_runtime_repos` 2 passed; `cargo test --lib -- --test-threads=1` 179 passed; `cargo fmt --check` passed; `npm test -- src/workbench/wslAbsence.test.ts` 63 passed; `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/workbench/wslAbsence.test.ts src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx` 140 passed; `npx tsc --noEmit` passed.
- RDM-001 RU2/RU3 Security Watch: complete. Unsupported future WSL entries are hidden from runtime snapshots and command allowlists, fail closed before local path handling, do not leak unsupported repo paths in command errors, and cannot register WSL/tinto-agent commands in RDM-001. No new filesystem authority, external process launch, auth, tenant, public API, or secret-handling surface was added.
- RDM-002 brainstorm/requirements: `docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md`; status `reviewed`. User resolved planning decisions on 2026-06-23: WSL 2 only, one selected distro per WSL repo, Ubuntu as initial manual smoke target, and dev-only build/run from source as the initial `tinto-agent` availability model. Implementation/review status: RU1/RU2/RU3 `review-passed`. Findings paths: `docs/review-findings/2026-06-23-rdm-002-requirements-review.md`, `docs/review-findings/2026-06-23-rdm-002-plan-review.md`, `docs/review-findings/2026-06-23-rdm-002-work-package-review.md`, `docs/review-findings/2026-06-23-rdm-002-code-review.md`, and `docs/review-findings/2026-06-23-rdm-002-security-review.md`.
- RDM-002 implementation summary: added `src-tauri/src/wsl_agent/{mod.rs,protocol.rs,launcher.rs}`, `src-tauri/src/bin/tinto-agent.rs`, and `pub mod wsl_agent` in `src-tauri/src/lib.rs`; no public Tauri WSL command or frontend WSL UI was registered. The launcher builds `wsl.exe -d Ubuntu -- cargo run --manifest-path <repo>/src-tauri/Cargo.toml --bin tinto-agent` for dev-source smoke, uses argument vectors with no shell interpolation, bounds handshake messages to 64 KiB, and compiles the real process transport only on Windows. During review, `src-tauri/src/bus/mod.rs` was hardened so unsupported WSL navigation aliases cannot canonicalize into local repo command paths. Verification passed: `cargo test --lib wsl_agent`, `cargo build --bin tinto-agent`, compatible/incompatible local binary smoke, `cargo test --lib unsupported -- --test-threads=1`, `cargo test --lib bus -- --test-threads=1`, `cargo test --lib agent_console`, `cargo test --lib invoke_handler`, `npm test -- src/workbench/wslAbsence.test.ts`, `npx tsc --noEmit`, `git diff --check`, WSL new-file `rustfmt --edition 2021 --check`, isolated `rustfmt --edition 2021 --check --config skip_children=true src\bus\mod.rs`, and work package checker. Global `cargo fmt --check` remains avoided as a signal because of pre-existing unrelated formatting drift in `src-tauri/src/bus/secret_scan.rs`.
- RDM-003 implementation summary: RU1 backend persistence/command registration is complete. Added WSL-specific workbench errors, `RepoEntry::wsl`, lexical Ubuntu/Linux-path validation, `WorkbenchStore::add_wsl_repo`, `WorkbenchStore::remove_wsl_repo`, Windows-visible WSL config projection, Windows-only `add_wsl_repo`/`remove_wsl_repo` Tauri commands, and invoke-handler tests proving the WSL config commands are `#[cfg(target_os = "windows")]` while no `tinto-agent` launcher is registered. The bus still treats WSL entries as unsupported and does not mount/read/watch them before RDM-004. RU1 verification passed: `cargo test --lib workbench` 33 passed, `cargo test --lib invoke_handler` 1 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, isolated Rust format check passed, and `git diff --check` passed with CRLF warnings only. RU2 must update `src/bus/contract.ts`, client wrapper/platform gate, frontend absence tests, and likely `docs/contracts/bus-contract.md`.
- RDM-003 RU2 summary: frontend contract/gate is complete. Added optional `RepoEntry.source`/`RepoEntry.distro`, isolated `addWslRepo`/`removeWslRepo` client wrappers, `src/workbench/platform.ts` with mockable `isWindowsHost`, updated WSL absence tests to isolate WSL text to contract/wrapper/gate modules, and updated `docs/contracts/bus-contract.md` to document Windows config metadata while live bus repos remain local-only until RDM-004. RU2 verification passed: targeted Vitest 91/91, `npx tsc --noEmit`, and targeted Prettier check.
- RDM-003 RU3 summary: Windows-only visible UX is complete. Added `src/workbench/AddWslRepoDialog.tsx`, Windows-gated Repos menu item, configured WSL labels in the Repos menu, WSL path normalization flow, and tests proving non-Windows hides WSL actions while Windows can submit `Ubuntu + /home/...` without creating project entries. Verification passed: targeted Vitest 109/109, `npx tsc --noEmit`, targeted Prettier, and `git diff --check`. Security review passed: no process launch, no shell, no `\\wsl$` traversal, no WSL live bus routing before RDM-004.
- RDM-004 implementation summary: RU1/RU2 are review-passed for the implemented live tracking path. Added generic WSL agent protocol DTOs, typed agent runtime handlers, host request exchange, dev-source agent root discovery via `TINTO_WSL_AGENT_ROOT_LINUX` or `/mnt/<drive>/...`, WSL runtime mounting in the bus on Windows, `ResolvedRepo`/`resolve_repo_identity` for read routing, WSL read command routing for worktree diff, commit diff/log, blob, file content, and repo tree, plus bounded 3s WSL polling that refreshes repo deltas and subscribed diffs. Local repos remain canonicalized/watched locally. `resolve_repo` remains local-only so file operations, Gitleaks, media preview, and Agent Console still reject WSL repos pending RDM-005. Fine-grained WSL `fs-events` are deferred as a recorded hardening gap. Verification passed: `cargo test --lib wsl_agent` 15 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, `cargo test --lib invoke_handler` 1 passed, `cargo build --bin tinto-agent` passed, targeted frontend tests 107 passed, `npx tsc --noEmit` passed, targeted Rust formatting applied, and `git diff --check` passed with CRLF warnings only.
- RDM-005 implementation summary: WSL file operations are implemented and review-passed for the existing Explorer/editor file operation commands. `copy_to_repo`, `copy_within_repo`, `move_within_repo`, `export_from_repo`, `delete_from_repo`, `restore_deleted_from_repo`, and `redo_deleted_from_repo` now route by `RepoSource`: local repos keep the existing local filesystem path, and Ubuntu WSL repos execute through `tinto-agent`. Windows drag/drop paste sources and export destinations are translated to `/mnt/<drive>/...` without shell interpolation; Linux absolute paths pass through unchanged. The WSL agent reuses active-workbench allowlisting, repo containment, `.git` rejection, conflict DTOs, delete backup manifests, restore, and redo semantics. Honest gap: WSL Gitleaks, media previews, Agent Console sessions, packaging/recovery, and fine-grained WSL `fs-events` remain deferred. Verification passed: `cargo test --lib file_ops`, `cargo test --lib wsl_agent` 17 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, `cargo build --bin tinto-agent`, targeted frontend tests 35 passed, and `npx tsc --noEmit`.
- RDM-006 implementation summary: WSL packaging/recovery hardening is implemented and review-passed for the launcher boundary. Added packaged Linux agent discovery via `TINTO_WSL_AGENT_LINUX_BIN` plus app-relative packaged candidates, versioned WSL install under `$HOME/.local/share/tinto/agents/<version>/tinto-agent`, and packaged-first `request_ubuntu_agent` routing for WSL bus/read/file-operation calls. The old dev-source launch is retained only when `TINTO_WSL_AGENT_ALLOW_DEV_SOURCE=1` is set, so release behavior cannot silently depend on an Ubuntu source checkout. CI now builds the Linux `tinto-agent` on Ubuntu and uploads it as `tinto-agent-linux-x86_64`; the final installer/resource wiring still needs to consume that artifact. The manual smoke checklist now covers packaged install, compatible/incompatible handshakes, local+WSL repo coexistence, and WSL file operations. Honest gap: Windows/Ubuntu packaged smoke remains pending before release. Verification passed: `cargo test --lib wsl_agent` 21 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, and work package checker.
- RDM-007 implementation summary: WSL media preview is implemented and review-passed. `get_media_content` now routes by repo source: local repos keep the local read path, and Ubuntu WSL repos call `tinto-agent` through the packaged-first request helper. The WSL runtime reuses the existing media extension allowlist, `.git` rejection, repo containment, regular-file-only read guard, base64 response shape, and 12 MiB cap. No frontend command or UI change was required. Honest gap: real Windows/Ubuntu smoke should open a PDF/image from a WSL repo before final release. Verification passed: `cargo test --lib wsl_agent` 22 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, targeted frontend media tests 38 passed, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, and work package checker.
- RDM-008 implementation summary: WSL Gitleaks parity is implemented and review-passed. Existing global `get_gitleaks_setup_status` and `install_gitleaks` remain host-scoped. New repo-aware commands `get_repo_gitleaks_setup_status` and `install_repo_gitleaks` route local repos to the host and WSL repos through `tinto-agent`; `create_repo_gitleaks_config` is now source-aware and writes `.gitleaks.toml` inside the Linux repo for WSL repos after active-workbench allowlist checks. Agent-side `RepoSnapshot` already produces `gitleaks_configured` and `secret_findings` through Linux recomputation. Honest gap: real Windows/Ubuntu smoke should verify WSL Gitleaks status/config/secret findings before final release. Verification passed: `cargo test --lib wsl_agent` 23 passed, `cargo test --lib bus -- --test-threads=1` 42 passed, `cargo test --lib invoke_handler` 1 passed, targeted frontend Gitleaks tests 47 passed, `cargo build --bin tinto-agent`, and `npx tsc --noEmit`.
- RDM-009 implementation summary: WSL Agent Console parity is implemented and review-passed for session launch/lifecycle. `start_agent_session` now resolves repo identity and routes local repos through the existing host PTY/checkpoint path while WSL repos launch inside Ubuntu via `wsl.exe -d Ubuntu -- sh -lc ...` with repo path and agent id passed as argv. Added `agent_binary_available_for_repo` so WSL repo cards check Ubuntu agent availability instead of Windows host PATH. Existing session output/input/resize/stop/list semantics are reused. WSL sessions intentionally expose `checkpoint: null`; Terminal disables Revert and backend direct revert returns `checkpoint_unsupported`, avoiding a fake rollback guarantee. Honest gap: remote WSL checkpoint/revert remains required before claiming full Agent Console rollback parity. Verification passed: `cargo test --lib agent_console` 41 passed, `cargo test --lib invoke_handler` 1 passed, affected frontend tests 47 passed, `npx tsc --noEmit`, and `git diff --check`.
- RDM-010 implementation summary: WSL filesystem events are implemented and review-passed. Added `RepoSnapshotWithFsEvents`, `FileFingerprint`, and `RepoFileFingerprintSnapshot` to the WSL protocol; raised the line-delimited agent message guard to 20 MiB; and added a Linux-side fingerprint walker that returns relative path, size, and mtime without reading file contents, following symlinks, or exposing `.git`. The bus now diffs WSL fingerprints per repo, primes first snapshots without events, emits existing `EVENT_FS_EVENTS` batches for created/modified/removed WSL files, and then applies the normal WSL `RepoDelta`. Honest gap: this is polling-cycle event parity, not a long-lived WSL inotify stream. Verification passed: `cargo test --lib wsl_agent` 25 passed, `cargo test --lib bus -- --test-threads=1` 43 passed, `cargo build --bin tinto-agent`, `npx tsc --noEmit`, and `git diff --check`.
- Reviewability Gate: passed. Chosen granularity is three review units: RU1 repo-source/config compatibility, RU2 guarded backend routing, RU3 Linux absence/regression coverage. Default delivery remains local fast-forward into `develop`, no PR unless explicitly requested; if PR flow is requested, keep stack target 2/max 2.
- Ready work: RDM-001 is delivered. RUL-001 and RDM-002 through RDM-010 are implemented, verified, review-passed, and queued for final-batch release at the end of the active Compound Master run.
- Context readiness result: sufficient for roadmap generation. Product intent, current system shape, technical execution context, bus/interface contract, delivery context, and existing scope were covered by current docs and source files. Remaining WSL-specific choices are recorded as user decisions rather than invented behavior.
- Resolved roles: `roadmap_generator` = `krt-roadmap-cartographer`. Approved fallback roles for this run: `document_review` = document-review subagent fan-out plus lead synthesis; `brainstorm` = `krt-requirements-weaver`; `plan` = `krt-delivery-navigator`. Jira optional and currently omitted because environment is not configured.
- Production posture: `prototype`, supported by state history and local desktop project posture.
- Branch/base strategy: continue from `develop`; release preference remains local fast-forward/no PR unless the user requests PR flow.
- Reviewability posture: RDM-001 work package passed Reviewability Gate with RU1/RU2/RU3. Default delivery is local fast-forward into `develop`, no PR unless explicitly requested; if PR flow is requested, stack target 2/max 2.
- Next exact invocation: continue Compound Master with WSL Agent Console or fine-grained `fs-events`; or run final Windows/Ubuntu WSL manual smoke before the final batched Release Marshal if the active WSL scope is considered complete.

## Completed Delivery

- RDM-001: Tauri 2 + React skeleton.
- RDM-002: read-only Git engine.
- RDM-003: Git/path classifier.
- RDM-004: filesystem watcher.
- RDM-005: workbench manager and config persistence.
- RDM-006: state/event bus and frozen backend/frontend contract.
- RDM-007: dockable dashboard/workbench UI.
- RDM-008: diff viewer and live diff.
- RDM-009: watched-files UI and pattern editor.
- RDM-010: timeline/history panel.
- RDM-011: passive signals and lightweight metrics.
- RDM-012: filters/search, redacted native notifications, and glance mode.

## Post-Closeout Initiative — Workbench IDE Overhaul

User-directed UX initiative after roadmap closeout. Goal: reshape the dockable workbench into a project-centric, VS Code-style IDE. Iterated turn-by-turn; not a roadmap-planned RDM package and not gated through brainstorm/plan/work-package artifacts.

Shipped tranche — merged to `develop` at `2a701e3` (local fast-forward, no PR), 5 atomic commits:

- `fix(watcher): tolerate permission-denied subdirectories when mounting` — a permission-denied subtree (e.g. a mongo data dir under a worktree) no longer degrades the whole repo watch; accessible watches are kept.
- `fix(workbench): live-reflect repo mutations and open the added repo's tab` — `add_repo`/`remove_repo`/`update_repo` reseed the live bus so changes appear without restart; `add_repo` returns the canonical path so the new repo's tab opens.
- `feat(workspace): project-centric tabbed workspace with menu bar and per-project explorer` — VS Code-style menu bar (replaces the top bar), level-1 dockview tabs for Dashboard/Timeline/projects, per-project file explorer, project-tab change indicator, folder change indicators, dashboard filters.
- `feat(files): file viewer with diff/normal/markdown views and syntax highlighting` — unified FileView (diff for changed files, normal highlighted view otherwise, rendered Markdown for `.md`), Shiki highlighting in the full-file view, `react-markdown` + `remark-gfm` added.
- `feat(qol): per-file text zoom with Ctrl +/-/0` — scales only the open file's content via a `--file-zoom` CSS variable, persisted to `localStorage`.

Follow-up tranche — implemented, verified, internally reviewed, and release-approved:

- Level-1 project tabs enlarged (dockview height override) and the change-indicator dot given a fixed slot so the title no longer shifts.
- Level-2 file tabs migrated to a NESTED dockview per project (`fileDock` registry) so files can be dragged into splits and two files can sit on screen at once; VS Code preview/pin re-implemented on top (single reused italic preview panel; double-click pins). Open-file layout (files + splits) persists across sessions in `localStorage` per repo.
- Single-click/double-click pattern replaced double-click abuse: single click previews, double click pins.
- Dashboard redesigned as a bento grid: cards lost the confusing expand toggle, show key health at a glance, fix text overflow, and feature attention-worthy repos with wider tiles.
- Internal review gate (2026-06-16): local Compound Master code review of the nested dock, persistence, click/double-click behavior, dashboard card simplification, and tests found no P0-P2 findings.
- Impact Scan (2026-06-16): no backend, Tauri command, auth, persistence schema, API payload, generated binding, fixture contract, or CI workflow contract changed. Changed surfaces are frontend UI state and browser `localStorage` keys (`tinto:filedock:<repo>`), covered by `src/workspace/fileDock.test.ts`.
- Excluded from this review unit: untracked `brand/wordmark.png`; no references to `wordmark` or `brand/` exist in the source tree.

## Post-Closeout Iteration — WSL2 Filesystem Watcher Fix (2026-06-22)

User-reported bug: files pasted from Windows Explorer into WSL2 repos did not appear in the project tree. Root cause: `inotify` cannot detect filesystem changes made from Windows on 9P/mounted filesystems (`/mnt/...`).

Fix: added a periodic polling fallback (`poll_loop`) in `src-tauri/src/watcher/mod.rs` that scans mounted repo roots every 2 seconds, compares modification times against a snapshot, and emits synthetic `PollDetected` events into the existing router pipeline. The router classifies and debounces these events identically to native `notify` events, so the downstream contract is unchanged.

Implementation details:
- `FsWatcher` gained `poll_roots: Arc<Mutex<Vec<PathBuf>>>` (shared with the poller) and `poll_handle: Option<JoinHandle<()>>`.
- `poll_loop` maintains per-root `HashMap<PathBuf, SystemTime>` snapshots. First scan captures the baseline without emitting events (avoids false positives on pre-existing files).
- `scan_directory` recursively walks the repo, ignoring `.git` and `node_modules`.
- `RouterInput::PollDetected` variant carries `Vec<PollChange>` (path, is_dir, kind).
- `route_poll_changes` classifies via the existing `PathClassifier` and feeds `DebounceInput::Event`, reusing the full debounce/rebuild/forward pipeline.
- `shutdown()` and `Drop` abort the poll handle.

Also fixed stale-closure risk in `src/panels/tree/ProjectExplorer.tsx`: `handleOsDropRef`, `handleTreeDropRef`, `handlePasteRef` were declared as `useRef` but never synced. Added `useEffect` hooks to keep refs current with the handler functions, and corrected the ref type signatures to include `| null` with explicit `null` initial values.

Verification: `cargo test --lib watcher::` 29/29, `cargo test --lib` 158/158, `npx tsc --noEmit` clean.

## Post-Closeout Iteration — Gitleaks Addon and Repo Configuration Flow (2026-06-22)

User-directed security-tooling UX iteration: replace heuristic-only explanations with an optional Gitleaks addon flow, but keep repo configuration contextual instead of forcing shell commands or modal-only guidance.

Delivered behavior:
- Added additive Tauri commands `get_gitleaks_setup_status`, `install_gitleaks`, and `create_repo_gitleaks_config`.
- Gitleaks detection now checks both the host system and a Tinto-managed addon location.
- Automatic install no longer depends only on the GitHub API. Tinto first resolves the latest public release, downloads the current asset for the active OS/architecture, extracts it into Tinto's addon directory, and uses that binary directly. Host package-manager fallbacks remain in place.
- The Complementos modal now acts as a global addon manager only: it shows status and lets the user explicitly request installation, but it does not auto-install on open.
- Repo deltas gained additive field `gitleaks_configured: bool`, computed from whether `.gitleaks.toml` or `gitleaks.toml` exists at the repo root.
- Repo UI surfaces a per-repo alert when local Gitleaks configuration is missing. That alert no longer asks the user to copy a template manually; it calls `create_repo_gitleaks_config` so Tinto writes `.gitleaks.toml` directly into the repo root.

Implementation notes:
- Backend: `src-tauri/src/bus/secret_scan.rs`, `src-tauri/src/bus/commands.rs`, `src-tauri/src/bus/mod.rs`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`, and `src-tauri/Cargo.toml`.
- Frontend: `src/bus/client.ts`, `src/bus/contract.ts`, `src/workbench/AddonsManager.tsx`, `src/panels/GitleaksConfigNotice.tsx`, `src/panels/RepoCard.tsx`, `src/panels/RepoPanel.tsx`, and `src/App.css`.
- Contract docs updated in `docs/contracts/bus-contract.md`.

Verification:
- `rtk tsc --noEmit`
- `rtk npm run test -- src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx src/bus/contract.test.ts`
- `cd src-tauri && rtk cargo test --lib secret_scan -- --nocapture`
- `cd src-tauri && rtk cargo check`

Known constraints:
- Managed install still needs network access to GitHub release assets.
- Host package-manager fallbacks can fail due to missing installers, missing privileges, or unavailable distro packages.
- The per-repo `.gitleaks.toml` creation is intentionally minimal and idempotent; deeper rule editing remains outside this slice.
## Post-Closeout Iteration — RUL-001 File Overview Ruler Parity (RU1 re-applied with dev fixture, 2026-06-22)

RU1 (foundation: always-visible track, scroll-synced caret, click-to-jump on the full track, active-marker highlight with scroll-past clear, configurable width/density, keyboard nav, a11y, and a `source` discriminator on `FileOverviewMarker` for future marker types) is re-applied on `develop` and ready for review.

**Re-applied surfaces (this iteration):**
- `src/panels/file/FileOverviewRuler.tsx` — rewrite: always renders (no `return null` when no markers), scroll-synced caret via `useOverviewScrollSync`, click-to-jump on the full track, active-marker highlight with `useEffect` scroll-past clear, `source?: "alert" | "hunk" | "search"` field on `FileOverviewMarker` (default `"alert"`), `role="slider"` + `aria-valuemin/max/now/valuetext` + keyboard nav (ArrowUp/Down/Home/End), `aria-hidden="true"` on the caret.
- 2026-06-23 visual correction: `FileOverviewRuler` now renders a VS Code-style minimap-width overview with mini-code content, marker stripes, and a viewport overlay rather than a thin marker-only rail. The overview is sticky inside the file scroll container and sizes itself from the visible body height so it remains visible while the file content scrolls. The minimap track is adaptive: 96 px minimum, 4 px per line until it fills the visible container, and a 600-row sampled rendering cap for huge files. Marker metadata uses `data-marker-line`; scroll-sync and jump lookup ignore `.file-overview-ruler` descendants.
- `src/panels/file/useOverviewScrollSync.ts` (new) — passive scroll listener + `ResizeObserver` + `requestAnimationFrame` coalescing. "First visible" semantic is `rect.bottom > bodyRect.top` (strict), with `visibleLineCount` returned for the minimap viewport overlay. Accepts an optional `bodyRef` and falls back to `topLine=1` when the ref is null.
- `src/panels/diff/DiffView.tsx` and `src/panels/diff/FullFileView.tsx` — forward an optional `bodyRef`; the rail hosts the hook and passes `topLine` + `bodyRef` to the ruler. `FullFileView` derives `lines` via `useMemo` so the hook recomputes only when the content changes.
- `src/panels/file/FileView.tsx` — owns `bodyRef` via `useRef<HTMLDivElement>(null)` on `.file-view__body`; forwards to 3 `FullFileView` calls and 1 `DiffView` call.
- `src/App.css` — new `--has-track` class (track clickable + 2 px right-edge gradient), `--empty` class for the empty state, caret indicator style, hunk-marker style placeholder, active-marker outline + glow.
- `src/panels/file/FileOverviewRuler.test.tsx` (new, 7 tests) and `src/panels/file/useOverviewScrollSync.test.tsx` (new, 2 tests).
- `src/panels/diff/FullFileView.test.tsx`, `src/panels/diff/DiffView.test.tsx`, `src/panels/file/FileView.test.tsx` — test id format updated to `overview-marker-{line}-{index}` to disambiguate stacked markers.

**Dev-only browser fixture (new this iteration):**
- `demo.html` (Vite multi-page entry) + `src/demo/main.tsx` + `src/demo/demo.css` — a standalone page that mounts `FileOverviewRuler` + `useOverviewScrollSync` against a mock 80-line file with 12 secret-pattern lines. No Tauri, no bus, no real repo. Safe to keep: the Tauri app never opens `/demo.html`, so this code is dev-only and never reaches the production bundle.
- Review URL: `http://127.0.0.1:1420/demo.html` (run `npm run dev` first).
- Side panel shows `topLine` and `activeLine` in real time so the user can confirm the hook is firing.

**What to test (the user's review checklist):**

1. Open `http://127.0.0.1:1420/demo.html` in Chrome/Firefox with DevTools (F12) open.
2. **Track always visible**: the right-side rail is rendered even with zero markers. The 2 px right-edge gradient should be visible on a dark background.
3. **Caret follows scroll**: scroll the body with the mouse wheel or scrollbar. The white caret inside the rail moves in lockstep with the visible top line. The `topLine` value in the side panel updates on every scroll.
4. **Click on the rail (empty region)**: clicking anywhere on the track (between markers) scrolls the body so the corresponding line is centered, and the `activeLine` value in the side panel updates.
5. **Click on a red alert chip**: scrolls the body to that line and applies the `--active` outline to the chip. The `activeLine` value in the side panel updates.
6. **Click on a line in the body**: activates that line and updates `activeLine` in the side panel.
7. **Keyboard nav on the track**: Tab into the track (focus ring visible), then ArrowUp/ArrowDown move ±1 line, Home jumps to line 1, End jumps to line 80.
8. **Active-marker scroll-past clear**: click a marker (e.g. line 7), then scroll down past it. The `--active` outline clears.
9. **12 critical markers** should be visible at lines 7, 13, 14, 23, 24, 25, 26, 38, 39, 46, 47, 57, 58, 59, 73, 76.
10. **No console errors** in DevTools Console (warnings from the `react-hooks/set-state-in-effect` rule are expected on the scroll-past useEffect and are documented in the file).

**Verification results (this iteration):**
- `npx tsc --noEmit` clean.
- `npx vitest run` 241/241.
- `npx prettier --check <changed files>` clean.
- Vite dev server serves `demo.html` (HTTP 200) and `src/demo/main.tsx` (transformed).
- RU1 review-fix verification (2026-06-23): `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx` passed 11/11.
- RU1 affected suite (2026-06-23): `npm run test -- src/panels/file/FileOverviewRuler.test.tsx src/panels/file/useOverviewScrollSync.test.tsx src/panels/file/FileView.test.tsx src/panels/diff/DiffView.test.tsx src/panels/diff/FullFileView.test.tsx` passed 33/33.
- RU1 typecheck/format (2026-06-23): `npx tsc --noEmit` clean; `npx prettier --check <changed files>` clean.
- RU1 minimap visual verification (2026-06-23): browser inspection of `http://127.0.0.1:1420/demo.html` confirmed a 122 px minimap surface, 80 mini-code lines, 16 expected marker lines, viewport overlay, no console errors, marker line 57 click centers code line 57, and Home returns to `topLine: 1`.
- RU1 sticky minimap verification (2026-06-23): after scrolling the fixture body from `scrollTop=0` to `scrollTop=650`, the minimap stayed fixed at `top=153` / `bottom=882` while `topLine` updated from 1 to 37.
- RU1 adaptive minimap verification (2026-06-23): the 80-line fixture renders a 320 px minimap track (`80 * 4px`) inside the sticky 729 px available area. Tests cover 2-line files collapsing to the 96 px minimum and 5000-line files rendering only 600 sampled minimap rows.
- RU1 scroll-past-end verification (2026-06-23): open file and diff surfaces add bottom scroll space equal to the visible file body height minus one line. In the fixture, scrolling to the end places line 80 at the top of the file body (`offset=0`) with 723 px of scroll space below it, so users can read final lines without looking at the bottom edge.
- RU1 viewport-indicator smoothness (2026-06-23): the minimap viewport indicator is now driven by continuous `scrollTop / maxScroll` progress instead of discrete `topLine`, and updates with `transform: translate3d(...)` plus fixed pixel height. The transition was removed so the indicator tracks scroll frames directly instead of lagging behind.
- RU1 marker semantics (2026-06-23): the minimap now includes a compact legend badge for marker meanings, currently `posibles secretos` plus future-ready hunk/search grouping. The badge exposes accessible labels and hover/title summaries.
- RU1 inline marker labels (2026-06-23): full-file and diff rows with overview markers now show a right-side non-overlapping label such as `Possible secret` inside the reserved gutter before the minimap. The label has a title with line context and keeps `pointer-events: none` so it does not interfere with selection/click navigation.
- No backend changes; no `cargo` commands expected. RUL-001 is frontend-only.

**Direct Compound Master browser review (inline, no subagent, 2026-06-23):** `review-passed` after fixes. Initial review of `http://127.0.0.1:1420/demo.html` found P1/P2 issues where marker buttons reused `data-line` inside the same scroll body, causing `useOverviewScrollSync` and `jumpToLine()` to select rail markers instead of file rows; the fixture side panel also missed rail-driven `activeLine` updates, and the console had a favicon 404. Fixes applied: marker buttons now use `data-marker-line`, scroll-sync and jump lookup ignore `.file-overview-ruler` descendants, the fixture subscribes to `onActiveLineChange`, and `demo.html` declares an embedded favicon. Follow-up user visual review clarified that the desired result is effectively a VS Code minimap, so the surface was widened and changed to mini-code content with a viewport overlay and marker stripes. A second follow-up clarified the minimap must remain fixed while scrolling; the surface is now sticky and body-height-sized. A third follow-up clarified that small files should not stretch and huge files need bounded rendering; the minimap now uses adaptive track height and samples huge files at 600 rendered rows. Re-review passed: initial `topLine=1`, click rail updates `activeLine`, marker line 57 centers and activates code line 57, Home/End report first visible content lines, scroll-past clears marker 7, console has no errors, the 80-line fixture renders a 320 px minimap inside a 122 px wide surface, and its top/bottom remain unchanged after deep scroll. Finding details and resolution evidence are in `docs/review-findings/2026-06-23-rul-001-ru1-demo-review.md`.

**Release handoff readiness:** RUL-001 release flow covering RU1/RU2/RU3 is ready, but intentionally deferred until the end of the active Compound Master run by user instruction on 2026-06-23. Final release should include this queued unit plus any later completed WSL work packages in a fresh Release Marshal plan. Suggested commit grouping remains: `docs(orchestration): record WSL agent bootstrap decision gate [skip ci]`, `feat(files): add persistent file overview minimap`, and `docs(orchestration): add file overview ruler parity artifacts [skip ci]`.

## Canonical Artifact Roots

- Brainstorms: `docs/brainstorms/`.
- Plans: `docs/plans/`.
- Work packages: `docs/work-packages/`.
- Contract docs: `docs/contracts/`.
- Backlog: `docs/backlog/`.

## Current Release-Ready Work — PDF/Image Viewer

Post-closeout UX iteration requested on 2026-06-19: add visual previews for PDFs and common image formats inside the existing file tab surface.

- Intended implementation shape: extend the existing read-only file viewer, not a separate app route. Keep diff/text/Markdown behavior unchanged.
- Implementation status: complete. Added `get_media_content` as an additive Tauri bus command, a 12 MiB media read guard, backend media-extension validation, frontend PDF/image detection, `MediaView`, CSP allowances for data/blob media rendering, and contract docs/tests.
- Scope guard: media previews skip the live-diff subscription path, so opening PDFs/images does not consume the diff subscription cap or show diff-paused UI.
- Impact Scan (2026-06-19): changed IPC command contract, frontend client wrapper, Tauri command registration, CSP media directives, and file-view UI. Consumers found via `rg get_media_content|MEDIA_CONTENT_MAX_BYTES|mediaKind|iframe|svg`: `src/bus/client.ts`, `src/panels/file/FileView.tsx`, `src/panels/file/MediaView.tsx`, `src/panels/file/mediaTypes.ts`, `src/bus/contract.test.ts`, `src/panels/file/FileView.test.tsx`, `src/panels/file/MediaView.test.tsx`, and `docs/contracts/bus-contract.md`. Required consumer tests added/updated.
- Security Watch (2026-06-19): media command preserves active-workbench repo allowlist, canonical containment, `.git` exclusion, regular-file-only reads, bounded allocation, and rejects non-media extensions with `unsupported-media`. CSP is widened only for `img-src`, `frame-src`, and `object-src` data/blob media. No auth, tenant, secrets, persistence, external integration, or destructive behavior changed; focused Security Sentinel not required for this prototype-local read-only preview.
- Internal review gate (2026-06-19): direct Compound Master fallback review used because subagent spawning requires explicit user authorization in this runtime. Review found and fixed one P2 stale-content bug where `MediaView` could keep showing the prior file while a new media path loaded. No remaining P0-P2 findings after fix.
- Verification (2026-06-19): targeted `npm test -- FileView.test.tsx MediaView.test.tsx contract.test.ts` passed 29/29 before the stale-path fix; targeted `npm test -- MediaView.test.tsx FileView.test.tsx` passed 20/20 after the fix; full `npm test` passed 158/158; `npm run lint`, `npm run format:check`, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` passed 121/121; `git diff --check` clean.
- Visual/server smoke (2026-06-19): Vite dev server is running at `http://127.0.0.1:1420`; `curl -I http://127.0.0.1:1420` returned HTTP 200. Earlier Chrome DevTools MCP screenshot verification remained blocked by browser-target tooling (`Protocol error (Target.setDiscoverTargets): Target closed`), and the available DevTools session currently points at an unrelated app with no navigation tool exposed. Treat this as a tooling/environment blocker for screenshot verification, not evidence that the app failed to serve. Re-attempt visual verification later with a working Chrome DevTools MCP session, Tauri manual smoke, or packaged app run.
- Release handoff readiness: ready for `krt-release-marshal` with Jira policy optional/no-Jira fallback. Current branch/base: `develop` over `origin/develop`. Suggested semantic commit grouping: `feat(files): preview PDFs and images in file tabs` covering bus contract/command, frontend media view, CSP, docs, and tests. Suggested release title: `Preview PDFs and images in file tabs`. Suggested release bullets: add read-only PDF/image previews in existing file tabs; keep text, Markdown, and diff behavior unchanged; bound media reads and reject unsupported media extensions.
- Release Marshal preflight and local release (2026-06-19): starting branch `develop`, selected base `origin/develop`, origin remote `https://github.com/ElZaWarudo/tinto.git`, no existing PR for `develop`, Jira readiness `jira-env-not-configured`, scope guard `human_lines=193`, `generated_lines=0`, `orchestration_doc_lines=0`, `untracked_files_count=3` with no blocking scope warning. User approved local no-PR merge; local `develop` was fast-forwarded; push remains intentionally out of scope.

## Release State

- CI-gate fix for overview ruler delivery (2026-06-24): pushed directly to `origin/develop`
  without PR or Jira, matching standing project preference. Commit: `9fe58fa`
  (`fix(file): satisfy overview ruler CI gates`). It fixed the deterministic
  `Frontend / Check formatting` failure from run `28080950878` by formatting
  `src/App.tsx` and `src/panels/diff/DiffView.test.tsx`, removing a dead comment in
  `src/App.tsx`, and deferring overview-ruler scroll-past active-marker clearing outside the
  synchronous effect body in `src/panels/file/FileOverviewRuler.tsx` and `src/demo/main.tsx`.
  Local verification before push: `npm run format:check`, `npm run lint` (warnings only),
  targeted overview/App Vitest suite 25/25, full `npm test` 323/323, `npm run build`, and
  `git diff --check`. GitHub Actions run `28082169551` passed and published Windows artifacts.
- Packaged WSL agent smoke evidence (2026-06-24): CI artifacts from run `28082169551` were
  downloaded under `.ci-artifacts/28082169551/`. Artifact hashes: NSIS setup
  `689e1a3d87a8058eeffbafd588cb50214da41e45a1365a9f11588ab39b056f85`, MSI
  `da95854ec5b7ec0a5dd07423cccbd55ffdfa86d6ebe07ee6941a04415c8466aa`, Linux agent
  `60e596fc0ab3b0bcb944bf388772dd69080fc37c1655cd39559863b00fce0bcc`. WSL host evidence:
  `Ubuntu-24.04` running on WSL 2. MSI extraction contained
  `PFiles/Tinto/tinto-agent-linux-x86_64`, and it matched the downloaded Linux agent byte-for-byte.
  Compatible handshake, incompatible-protocol safe failure, installed-path handshake, repo
  tree/content/snapshot/fingerprint, copy/delete/restore/redo, and checkpoint scan/revert smoke
  checks passed. Extracted Windows executable started and remained running for 5 seconds before the
  smoke harness stopped it. Remaining gap: interactive packaged Tinto UI smoke.
- Final Windows-only WSL complement batch (2026-06-24): pushed directly to `origin/develop` without PR or Jira, matching the approved release plan. Delivered commits: `a054101` (`feat(files): add file overview minimap`), `35b4771` (`feat(workbench): add Windows WSL repo tracking`), `f9d7611` (`feat(wsl): route repo reads and file operations through agent`), `f672e49` (`feat(agent-console): run and revert sessions in WSL`), `e8ee2e1` (`ci(wsl): bundle packaged Linux agent resource`), `236a5e3` (`docs(orchestration): document WSL complement delivery [skip ci]`), `cc216a0` (`docs(orchestration): record WSL complement release`), `02aeaf4` (`ci: fix release formatting gates`), `53d61f2` (`ci: satisfy rust clippy gates`), `9466415` (`test: align runtime expectations with WSL support`), and `62f9ce5` (`test: stabilize WSL runtime CI expectations`). Jira omitted because checkout readiness reported `env-loaded-without-project-secret-file`. Final GitHub Actions validation passed on run `28062526450`; Windows installer artifacts are available from the `tinto-windows-bundle` workflow artifact and were also downloaded locally under `.ci-artifacts/` for handoff inspection (`Tinto_0.1.0_x64-setup.exe`, `Tinto_0.1.0_x64_en-US.msi`).
- RDM-001 Windows-gated repo identity and backend boundary release (2026-06-23): pushed directly to `origin/develop` without PR or Jira, matching standing project preference. Commits: `b79e3b4` (`docs(orchestration): reconcile preflight state [skip ci]`), `bc3c13f` (`feat(repo): add source-aware Windows WSL repo boundary`), and `2462f04` (`docs(orchestration): add Windows WSL identity delivery artifacts [skip ci]`). Jira omitted because checkout reported `jira-env-not-configured`.
- PR-based deliveries: PR #1, #2, #3, #4, #6, #7, #8, #9, #10, #11, and #12.
- RDM-012 delivery: local fast-forward merge into `develop` and push to `origin/develop`, intentionally without PR by user request.
- Post-closeout state archive: `5631c0e` (`docs(orchestration): compact compound master state`).
- CI workflow delivery: `e0e91ba` (`ci(github): add validation workflow`), merged locally to `develop` and pushed to `origin/develop` without PR.
- CI runtime maintenance: `8ebbdd3` (`ci(github): opt actions into node 24 runtime`), merged locally to `develop` and pushed to `origin/develop` without PR.
- Prior orchestration reconciliation: `2f894d8` (`docs(orchestration): finalize ci maintenance state [skip ci]`), pushed directly to `origin/develop`.
- Workbench IDE Overhaul shipped tranche: `2a701e3` (`feat(qol): per-file text zoom…`), the tip of a 5-commit `feat/project-workspace-ide` set, fast-forward merged into `develop` and pushed to `origin/develop` without PR.
- Workbench IDE Overhaul follow-up release: `8230397` (`feat(workspace): add nested file dock and bento dashboard`) plus orchestration docs, fast-forward merged into `develop` and pushed to `origin/develop` without PR.
- Brand and UX improvements release (2026-06-19): `bf75b09` (`docs(orchestration): add Agent Console Integration artifacts`), the tip of a 6-commit `feat/brand-and-ux-improvements` set, fast-forward merged into `develop` and pushed to `origin/develop` without PR. Commits: `chore(brand): replace default icons and add Tinto brand assets`, `feat(workbench): show brand wordmark and compact menu bar`, `feat(qol): add global keyboard shortcuts and help menu`, `feat(explorer): add context menu and collapsible project tree`, `docs(orchestration): add Agent Console Integration artifacts`.

## Verification Baseline

- Roadmap-closeout frontend gate: `npm test` 140/140, `npm run lint`, `npm run format:check`, `npm run build`.
- Roadmap-closeout Rust/Tauri gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, `cargo build`, `npm run tauri build`, and `rtk timeout 25s npm run tauri dev`.
- Workbench IDE Overhaul shipped tranche (`develop` `2a701e3`): `npm test` 148/148, `npm run lint`, `npm run build` green; `cargo test` 118/118, `cargo clippy -- -D warnings`, `cargo fmt --check` green.
- Workbench IDE Overhaul follow-up release: `npm test` 149/149, `npm run lint`, `npm run format:check`, `npm run build` green on 2026-06-16; backend unchanged from the shipped tranche (`cargo test` 118/118). CI was not re-run before push; GitHub Actions will validate the pushed `develop` tip.
- GitHub Actions CI workflow exists at `.github/workflows/ci.yml`.
- Local pre-push CI workflow validation: `npm run format:check`, `npm run lint`, `npm test` 140/140, `npm run build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 117/117, and `npm run tauri build`.
- GitHub Actions run `27601639210`: passed. Frontend passed in 58s, Rust passed in 8m44s, and Tauri bundle passed in 10m46s.
- GitHub Actions run `27602696319`: passed after the Node 24 runtime opt-in. Frontend passed in 1m0s, Rust passed in 2m2s, and Tauri bundle passed in 4m27s.
- GitHub Actions run `28062526450`: passed on the final WSL complement release tip `62f9ce5`. Frontend, Rust, WSL agent artifact packaging, Linux Tauri bundle, and Windows Tauri bundle all completed successfully. Windows artifacts published: `Tinto_0.1.0_x64-setup.exe` and `Tinto_0.1.0_x64_en-US.msi`.
- GitHub Actions run `28082169551`: passed on `develop` tip `9fe58fa`. Frontend passed in
  1m17s, Rust passed in 5m19s, Linux Tauri bundle passed in 4m50s, and Windows Tauri bundle passed
  in 5m50s. Windows artifacts published: `Tinto_0.1.0_x64-setup.exe` and
  `Tinto_0.1.0_x64_en-US.msi`.
- CI warning cleanup: `.github/workflows/ci.yml` sets `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true`. GitHub now reports that Node 20-targeting actions are forced to run on Node 24; the remaining annotation is informational until the upstream actions target Node 24 natively.

## Post-Closeout Maintenance

- Branch cleanup completed for merged local branches: `docs/compact-compound-state`, `docs/compound-master-closeout`, `feat/diff-viewer`, `feat/passive-signals`, `feat/quality-of-life`, `feat/timeline-history`, and `feat/watched-files-ui`.
- Remote cleanup completed for merged feature branches: `origin/feat/diff-viewer`, `origin/feat/passive-signals`, `origin/feat/timeline-history`, and `origin/feat/watched-files-ui`.
- Remote `origin/checkpoint/state-event-bus` and local `checkpoint/state-event-bus` were retained because the remote branch was not listed as merged into `develop`.
- Final docs-only reconciliation used `[skip ci]`; no third CI run was expected or required.
- Delivery workflow preference updated by user: avoid GitHub PR merges for this project unless explicitly requested; use local integration into `develop` and push. On 2026-06-23 the user further instructed: do releases at the end, so completed units may queue locally while Compound Master continues.

## Residual Backlog

- Opt-in Git fetch: `docs/backlog/2026-06-11-fetch-opt-in-backlog.md`.
- Phantom-repo generation token after workbench switch.
- TypeScript/Rust contract code generation.
- Keyboard arrow navigation/Escape polish.
- Diff viewer hardening/polish: manual-reload cancellation race, full-file/diff revision skew hardening, `useDiffData` extraction, S/M/U mark consolidation, and workbench-switch diff-panel orphan handling.

## Archive Status

- State archive status: compacted.
- Compact state path: `docs/orchestration/compound-master-state.md`.
- Archive snapshot: `docs/orchestration/archive/compound-master-state/2026-06-16-compound-master-state-full-state.md`.
- Notes: archive contains full historical run detail; this compact state is only the resume entrypoint.
