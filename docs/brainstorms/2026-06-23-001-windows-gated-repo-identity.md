---
title: Windows-gated repo identity and backend boundary requirements
status: reviewed
date: 2026-06-23
roadmap_item: RDM-001
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
artifact_kind: requirements-brainstorm
planning_input: true
source_docs:
  - docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
  - docs/review-findings/2026-06-23-wsl-complement-roadmap-review.md
  - docs/contracts/bus-contract.md
  - src-tauri/src/workbench/mod.rs
  - src-tauri/src/bus/contract.rs
  - src/bus/store.ts
---

# Windows-gated repo identity and backend boundary requirements

## Problem and Goal

Tinto currently treats a repo identity as a canonical local path string across workbench config, backend bus deltas, and frontend state. That works for local Windows and Linux repos, but it is not enough for a Windows-only WSL complement because a WSL repo identity is scoped by distro plus Linux path and must not be represented as a Windows filesystem path.

Goal: define the smallest repo identity and backend routing foundation that lets later Windows WSL work add `{distro, linux_path}` repos without breaking existing local repos or exposing any WSL feature surface in Linux desktop builds.

## Vocabulary

- Repo source: the internal classification of where a repo lives. Current source is `local`; future Windows-only source is `wsl`.
- Repo identity: the stable internal key for a repo source. Local identity remains the canonical local path. WSL identity is future-only and consists of distro plus normalized absolute Linux path.
- Backend routing seam: the narrow guard that decides whether a repo-scoped command may use the existing local backend or must reject/defer because the source is unsupported in the current slice.
- Linux absence boundary: non-Windows desktop builds must not compile, register, export, render, log as user-visible state, or execute WSL commands, launchers, menus, settings, empty/degraded states, UI flags, runtime paths, or behavior.

## Stakeholders and Users

- Primary user: Tinto desktop user on Windows who wants to monitor repos stored inside WSL.
- Existing users: Tinto desktop users on Windows or Linux monitoring local repos.
- Maintainer/reviewer: future implementer reviewing backend/frontend contract changes for compatibility.

## Scope In

- Introduce an internal repo-source identity model that can represent:
  - local repos using the existing canonical local path semantics;
  - future Windows WSL repos using a Windows-only WSL source with distro plus Linux path.
- Preserve the existing external bus contract shape for current local repos. RDM-001 shall not introduce a public WSL bus identity; it shall define the internal source model and compatibility plan so RDM-004 can add public WSL contract shape only if needed.
- Preserve existing local repo behavior, tests, and user-visible display names.
- Add the minimum backend routing seam needed for repo-scoped command families to prevent unsupported sources from falling into local filesystem/git paths.
- Add Windows-only capability gating for WSL source support.
- Ensure Linux desktop builds compile/register/export no WSL commands, launcher paths, UI flags, menus, settings, empty/degraded states, runtime paths, or behavior.
- Define behavior for copied/shared configs containing future WSL entries on Linux: preserve entries on disk without surfacing them in runtime state or UI.
- Record migration behavior for existing workbench configs.

## Scope Out

- Launching `wsl.exe`.
- Creating or running the Linux-side `tinto-agent`.
- Adding WSL repo picker UI beyond internal identity foundations.
- Adding any WSL repo picker or WSL-facing UI in this slice on any platform.
- Implementing WSL read/watch commands.
- Routing file mutations, Gitleaks, media preview, secret findings, or agent console sessions through WSL.
- Supporting SSH, cloud remotes, containers, or arbitrary remote hosts.

## Constraints

- Tinto remains a prototype, but local Windows/Linux repo behavior must not regress.
- The complement is Windows-only. Linux desktop builds must not expose a WSL feature surface.
- Existing config files must keep loading.
- If a config containing future WSL entries is opened on Linux, Tinto must not delete those entries, must not show them, and must not emit Linux degraded/error UI for them.
- Existing frontend consumers are keyed by current repo strings; any new identity representation must include a compatibility plan before it reaches user-facing state.
- The frozen bus contract is additive-first: renames/removals require explicit migration, not silent changes.
- Jira is optional and unavailable in this runtime.

## Functional Requirements

- FR1: The system shall distinguish local repo sources from future Windows WSL repo sources internally.
- FR2: The system shall preserve canonical local path identity for existing local repos.
- FR3: The system shall define a stable future WSL repo identity as distro plus Linux path, without translating it into a Windows path for identity.
- FR4: The system shall keep existing workbench config entries valid after the identity model is introduced.
- FR5: The system shall provide one shared repo-source guard/router for repo-scoped command families before local path handling occurs.
- FR6: The system shall keep local backend command behavior unchanged when the repo source is local.
- FR7: The system shall make the WSL source model available only behind Windows host capability gates and shall not probe or launch WSL in RDM-001.
- FR8: The system shall make WSL source support absent and unreachable in Linux desktop builds/runtimes, including command registration, frontend wrappers, menus, settings, empty/degraded states, launch paths, and runtime behavior.
- FR9: The system shall return a safe structured `unsupported_repo_source` error when internal Windows-side code reaches an unsupported source in this slice, without mapping that error to Linux UI degraded state.
- FR10: The system shall preserve existing repo display names and aliases for local repos.
- FR11: The system shall leave WSL UI entry points unimplemented in this slice except for any internal type coverage required to prove they are not present on Linux.
- FR12: The system shall preserve future WSL config entries on Linux without loading them into runtime repo state, dashboard lists, watcher state, or command surfaces.

## Non-Functional Requirements

- NFR1: Compatibility: existing local workbench configs and local repo monitoring tests must keep passing.
- NFR2: Reviewability: the first implementation slice should be understandable without reviewing WSL process launch or full bus parity.
- NFR3: Portability: Linux desktop builds must compile and run without WSL-specific host-launch dependencies.
- NFR4: Contract safety: changes to backend/frontend repo identifiers must be additive or compatibility-preserving.
- NFR5: Security: no new repo read or write authority is introduced in this slice.

## Business Rules

- BR1: A local repo is still identified by its canonical local path for existing behavior.
- BR2: A WSL repo identity is meaningful only on a Windows host and is scoped by distro plus Linux path.
- BR3: The Linux desktop app must behave as though the WSL complement does not exist.
- BR4: Later WSL capabilities must use the identity/routing model from this slice rather than introducing parallel identity rules.
- BR5: WSL identity normalization is `(exact distro name, normalized absolute Linux path)`: no Windows path translation, no path case folding, no `~` expansion as identity input, no relative paths, and no symlink resolution until the Linux-side agent validates paths in later slices.
- BR6: Unsupported-source handling must occur before `canonicalize`, `Git2Engine`, file operations, Gitleaks, media reads, or agent-console local launch code can run.

## Acceptance Criteria

- AC1: Existing local workbench config fixtures/tests load without schema-breaking changes.
- AC2: Existing local repo dashboard/store behavior keeps using the same display identity for local repos.
- AC3: Non-Windows checks prove Rust command registration excludes WSL commands/launchers, frontend exports/rendered UI exclude WSL menus/settings/empty states, and Linux runtime state excludes WSL sources from dashboard/watch surfaces.
- AC4: A test proves local repos route to the existing local backend path.
- AC5: A test fixture can represent a future Windows WSL repo source as distro plus Linux path without requiring a Windows path.
- AC6: No `wsl.exe`, `tinto-agent`, remote watcher, Gitleaks, media preview, file mutation, or agent console routing is implemented by this slice.
- AC7: A WSL-source fixture cannot reach local `canonicalize`, `Git2Engine`, file ops, Gitleaks, media read, or agent-console local launch paths; it returns `unsupported_repo_source` before local path handling.
- AC8: A copied/shared config containing a future WSL repo entry remains persisted when opened on Linux, but the WSL entry is absent from Linux runtime repo state and UI.
- AC9: Local regression coverage includes startup with existing config, add/remove/update/reorder repo, switch workbench, snapshot/delta delivery, dashboard display name, subscriptions, and existing degraded watcher state.
- AC10: RDM-001 produces a compatibility note explaining that public bus identity for WSL repos is deferred to RDM-004 unless an additive field is required by the reviewed plan.

## Assumptions

- The implementation can add an internal repo-source wrapper while preserving the existing public `repo: string` shape for local repos in the first slice.
- Public WSL bus identity stays out of RDM-001 unless the reviewed plan proves an additive compatibility field is necessary.
- Windows-specific runtime checks can be covered by unit tests/mocks in this Linux workspace, while final Windows smoke waits for later roadmap items.

## Open Questions

- OQ1: Should first-release WSL identity support one selected distro per repo or allow multiple distros in the same workbench? This affects RDM-003 more than RDM-001; RDM-001 should model distro as part of identity either way.
- OQ2: Should the future public WSL repo identity in RDM-004 be a URI-like opaque ID, or should the public bus keep a display string plus an additive source field? RDM-001 should not expose either publicly unless required for compatibility scaffolding.

## Validation Status

Requirements status: ready for review.

Planning input path: `docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md`.
