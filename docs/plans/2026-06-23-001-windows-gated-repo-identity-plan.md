---
title: Windows-gated repo identity and backend boundary plan
status: reviewed
date: 2026-06-23
roadmap_item: RDM-001
origin_roadmap: docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md
planning_input: docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md
artifact_kind: delivery-plan
---

# Windows-gated repo identity and backend boundary plan

## Planning Source

- Requirements packet: `docs/brainstorms/2026-06-23-001-windows-gated-repo-identity.md`.
- Review findings resolved: `docs/review-findings/2026-06-23-rdm-001-requirements-review.md`.
- Roadmap: `docs/roadmaps/2026-06-23-004-windows-wsl-agent-roadmap.md`.

## Scope Summary

RDM-001 introduces the internal repo-source model and guard/routing seam required by the Windows-only WSL complement. It preserves current local repo behavior, keeps public WSL bus identity out of this slice unless a compatibility field is unavoidable, and proves Linux desktop builds expose no WSL feature surface.

This plan does not launch WSL, create `tinto-agent`, implement WSL reads/watchers, or add WSL-facing UI.

## Delivery Approach

Hybrid plan: implement a narrow compatibility-preserving foundation with focused review units. The first unit establishes the data model and config compatibility. The second unit wires the guard/routing seam across repo-scoped command families without changing behavior for local repos. The third unit adds absence/regression tests and documentation.

## Implementation Units

### U1 - Repo Source Model and Config Compatibility

Goal: add an internal repo source representation while preserving local repo config behavior.

Tasks:
- Define internal repo source types for `local` and future Windows-only `wsl`.
- Define a persisted-vs-runtime projection: persisted config may retain future WSL entries, but runtime workbench state, bus mounting, and frontend-visible config on Linux include only local entries.
- Preserve `RepoEntry.path` local semantics for existing configs or add an additive wrapper/migration that keeps existing configs valid.
- Define future WSL identity normalization as exact distro name plus normalized absolute Linux path; no Windows path translation, no case folding, no relative paths, and no `~` identity input.
- Define Linux handling for copied/shared future WSL entries: preserve on disk, exclude from runtime repo state and UI.
- Ensure `list_workbenches`, `set_active_workbench`, and initial bus mount use the runtime projection, not raw persisted future WSL entries, on Linux.
- Add compatibility tests for existing local workbench config loading and future WSL-entry preservation on Linux.

Acceptance mapping: FR1-FR4, FR10, FR12, BR1-BR5, AC1, AC2, AC5, AC8.

Expected surfaces:
- `src-tauri/src/workbench/mod.rs`
- `src-tauri/src/workbench/commands.rs`
- new or nearby repo-source module if needed
- workbench tests
- contract docs only if an additive config shape is introduced

### U2 - Repo-Scoped Guard and Local Backend Routing Seam

Goal: ensure repo-scoped operations route through a shared guard before local filesystem/git handling.

Tasks:
- Add a shared internal repo-source lookup/guard used by repo-scoped command families and bus runtime mount/recalc paths. Because public repo command arguments are currently path-only, RDM-001 shall keep public WSL identity out of the bus and test WSL handling through internal `RepoSource::Wsl` fixtures only.
- Keep local repos routed to existing local backend behavior.
- Return safe structured `unsupported_repo_source` only for internal Windows-side future WSL source fixtures in this slice, before calling local `canonicalize`, `Git2Engine`, file ops, Gitleaks, media reads, or agent-console launch paths.
- Ensure Linux filters future WSL persisted entries before runtime/command/UI surfaces, so Linux never exposes `unsupported_repo_source` for WSL entries as user-visible state.
- Keep WSL backend implementation absent in this slice.
- Add tests proving WSL fixtures cannot reach local path/git/file/session paths.

Acceptance mapping: FR5-FR6, FR9, BR6, AC4, AC6, AC7.

Expected surfaces:
- `src-tauri/src/bus/mod.rs`
- `src-tauri/src/bus/commands.rs`
- `src-tauri/src/file_ops/commands.rs`
- `src-tauri/src/agent_console/commands.rs`
- `src-tauri/src/bus/secret_scan.rs` or command call sites if needed
- shared guard module
- focused backend tests

### U3 - Windows Gate and Linux Absence Regression Coverage

Goal: prove the WSL complement has no Linux desktop feature surface.

Tasks:
- Add Windows-only capability gates for WSL source support.
- Ensure non-Windows builds compile/register/export no WSL commands, launchers, frontend wrappers, menus, settings, empty states, degraded notices, runtime paths, or behavior.
- Add frontend/build-time tests or source checks for absence of WSL UI flags/menus/settings on Linux and no WSL-facing repo picker/UI on any platform in RDM-001.
- Add local regression tests for startup with existing config, repo add/remove/update/reorder, workbench switch, snapshot/delta delivery, dashboard display name, subscriptions, and existing degraded watcher state.
- Add a compatibility note documenting that public WSL bus identity is deferred to RDM-004.

Acceptance mapping: FR7-FR8, FR11, NFR1-NFR5, AC3, AC9, AC10.

Expected surfaces:
- `src-tauri/src/lib.rs`
- `src/bus/client.ts`
- relevant frontend workspace/menu/settings sources if touched by gates
- `docs/contracts/bus-contract.md` or a focused docs note if public identity compatibility needs recording
- frontend/backend regression tests

## Review Units

### RU1 - Repo Source Model and Local Compatibility

Includes U1 only.

Why this is independently reviewable:
- It changes identity/config foundations without touching command routing or broad command families.
- It can be verified through config and store compatibility tests.
- It carries the highest conceptual risk and should be reviewed before routing code builds on it.

### RU2 - Guarded Repo-Scoped Routing

Includes U2 only.

Why this is independently reviewable:
- It touches multiple command families but has one focused behavior: prevent unsupported sources from entering local path/git handling while preserving local behavior.
- It can be verified with WSL-source fixtures and existing local command tests.

### RU3 - Linux Absence and Regression Coverage

Includes U3 only, unless the code changes are small enough to keep with RU2 without harming review.

Why this is independently reviewable:
- It proves the binding product constraint that Linux shows no WSL surface.
- It is mostly tests/gates/docs and can be reviewed separately if it would otherwise obscure routing changes.

## Dependencies

- RU1 has no package-internal dependency.
- RU2 depends on RU1 because the guard/router needs the repo-source model.
- RU3 depends on RU1 and may be implemented alongside RU2 if the code paths are tightly coupled. RU1 must include enough persisted-vs-runtime projection gating that future WSL entries cannot reach Linux runtime state even before RU3 adds broader absence tests.

## Risks and Mitigations

- Risk: public repo identity changes could break frontend state keyed by repo string.
  - Mitigation: keep public WSL bus identity out of RDM-001 and preserve current local `repo: string` behavior.
- Risk: Linux absence could degrade into hidden/no-op WSL commands.
  - Mitigation: require absence from command registration, frontend exports/UI, runtime state, and degraded notices.
- Risk: touching many repo-scoped command families in RU2 could be broad.
  - Mitigation: use a shared guard and keep behavior changes limited to unsupported-source prevention.
- Risk: future WSL config entries could be deleted by Linux config writes.
  - Mitigation: preserve future entries on disk while filtering from Linux runtime state/UI.

## Verification Plan

Required before package handoff:
- `npm test -- src/bus/contract.test.ts src/bus/store.test.ts src/workbench/workbench.test.tsx src/panels/RepoCard.test.tsx src/panels/RepoPanel.test.tsx`
- `npx tsc --noEmit`.
- `cd src-tauri && cargo test --lib workbench`.
- `cd src-tauri && cargo test --lib bus`.
- `cd src-tauri && cargo test --lib file_ops`.
- `cd src-tauri && cargo test --lib agent_console`.
- Any new absence/source-gate tests added for this package.

Windows/WSL manual smoke is not required for RDM-001 because this slice does not launch WSL or run `tinto-agent`.

## Open Decisions

- OQ1 from requirements remains deferred to RDM-003: first release may support one or multiple WSL distros per workbench, but RDM-001 models distro as part of identity either way.
- OQ2 remains deferred to RDM-004: eventual public WSL repo identity shape.

## Planning Status

Planning status: ready for review.
