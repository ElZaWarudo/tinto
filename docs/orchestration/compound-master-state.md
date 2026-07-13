---
title: Compound Master State - Tinto
status: in-progress
date: 2026-07-13
initiative: ux-hardening-completion
mode: full
production_posture: prototype
state_format: compact
last_compacted: 2026-07-13
archive_snapshot: docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md
roadmap_next: docs/roadmaps/2026-07-13-008-post-ux-agent-platform-roadmap.md
---

# Compound Master State - Tinto

## Resume Snapshot

- Initiative: close and deliver the UX hardening batch on `codex/feature/ux-hardening-completion` from `develop`.
- Current scope: responsive and accessible frontend, workbench state hardening, generated-contract parity, CI/toolchain maintenance, native Tauri/WSL smoke, and durable documentation.
- Delivery state: implementation and review are in progress; nothing is staged or committed and no PR or Jira issue exists.
- Product posture: prototype. The current branch must pass the full frontend and Rust gates before delivery; the real WSL Agent Console follow-up remains explicitly deferred and open.

## Canonical Sources

- UX and mobile feasibility: `docs/brainstorms/2026-07-10-tinto-mobile-companion-feasibility.md`.
- Build and WSL workflow: `docs/build-guide.md`.
- Native WSL smoke record: `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`.
- Next initiative roadmap: `docs/roadmaps/2026-07-13-008-post-ux-agent-platform-roadmap.md`.
- Full pre-compaction history: `docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md`.

## Current Operating State

- The frontend UX audit batch is implemented with responsive layouts, accessible dialogs and controls, clearer hierarchy, Spanish product copy, state feedback, and regression coverage.
- Explanatory UI copy is being removed while retaining errors, confirmations, actionable empty states, and screen-reader guidance.
- Generated bus contracts now have an enforced parity type map and a CI drift gate; Node 24 and dependency update policy are recorded.
- Heavy Dockview, Markdown, and Shiki code is split into explicit chunks; the main entry bundle is substantially smaller without changing user-visible behavior.
- A pre-fix native Tauri/WSL smoke verified repo add/read, the file tree and Markdown rendering, live Git watcher updates, and Agent Console session creation against a temporary WSL repository. It also exposed a visually hidden Agent session in a compact persisted Dockview layout and failure to resolve the distro-native Codex process.
- Both findings have working-tree fixes and focused tests. The compact console now maximizes and restores its top-level Dockview panel while relaying layout to the nested dock; the focused frontend suites passed `23/23`. WSL preflight and PTY launch now share a resolver that explicitly loads NVM, rejects Windows-mounted candidates, validates the Linux binary, and surfaces non-zero exits; focused Rust suites passed for `shell_env` (`2/2`), availability (`4/4`), and `agent_console` (`80/80`).
- A test-only embedded-WebDriver harness now exercises the real Tauri shell without global desktop input. It passed real Rust IPC, first-run workbench creation, and Dashboard rendering. Config, home, Codex, local data, temp, and WebView state are disposable; feature executables use a dedicated Cargo target, require a runtime marker and port, and are removed after the run.
- Post-fix native UI confirmation is deliberately deferred, not passed: the user chose isolated web QA before a fresh compact session could verify real WSL Codex input/output, stop, checkpoint/change-log, and revert.

## Verification Baseline

- Earlier browser verification passed at desktop and 390/320 px widths with no horizontal overflow or console errors; captures live in `artifacts/ux-verification/`.
- Current isolated browser QA uses the local `demo.html`, `agent-lens-restorable.html`, and `dashboard-review.html` fixtures for frontend-only evidence. The main shell is separately covered by `npm run test:e2e:tauri`, which uses embedded WebDriver and real IPC rather than a browser mock.
- The native-shell E2E passed on Windows with `ping`, isolated workbench creation, Dashboard rendering, screenshot capture, and zero before/after changes in the real WebView profile. It does not claim WSL or Agent Console process coverage.
- Focused frontend suites, TypeScript, ESLint, Prettier, contract generation, Vite build, and `cargo check --locked` have passed for their individual slices.
- The final consolidated frontend and Rust gates and diff audit remain required before delivery. The compact post-fix native retest, real WSL Codex execution, and checkpoint evidence remain an explicitly deferred native follow-up and must not be inferred from fixture QA.

## Delivery Gate

- Preserve the distinction between fixture QA, generic Tauri-shell E2E, and real WSL process coverage.
- Keep the deferred native follow-up open for compact console visibility, real WSL Codex input/output, stop, checkpoint/change-log, and revert.
- Run all frontend, Rust, contract, formatting, lint, build, audit, and diff gates.
- Present the atomic Gitflow commit plan and wait for explicit user approval before staging or committing.

## Next Initiative

- Review `docs/roadmaps/2026-07-13-008-post-ux-agent-platform-roadmap.md`.
- Start with RDM-017: native Codex app-server support inside WSL.
- Sequence memory, additional agent adapters, content search, and the mobile companion bridge behind their documented product and security decisions.

## Archived History

- The complete historical state remains immutable at `docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md`.
