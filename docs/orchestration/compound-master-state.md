---
title: Compound Master State - Tinto MCP Layer
status: completed
date: 2026-07-21
initiative: provider-neutral-mcp-layer
mode: full
production_posture: unknown
state_format: compact
last_reconciled: 2026-08-27
verification_status: passed
archive_snapshot: docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md
---

# Compound Master State - Tinto MCP Layer

## Current Resume Snapshot

- Initiative: deliver the evidence-admitted first provider-neutral MCP control-plane slice.
- Requested pipeline: `mode:full`, `production:unknown`, `jira-policy:optional`, `parallel:false`, `delegation:auto`, `worktree-policy:avoid`, `autonomy:guarded`, `review-threshold:P0-P2`.
- Current phase: RDM-024 implementation, review and aggregate verification are complete locally. RDM-023 remains the completed installation protocol.
- Branch/worktree: existing `develop` checkout; no branch or worktree was created. Existing user changes remain untouched.
- Delivered boundary: Codex Windows/local read-only inventory, source-bound
  project profiles, explicit sanitized MCP activity and Agents management UI.
  Synchronization, provider-file writes, active probes, WSL/non-Codex import
  and launcher application remain gated.
- Verification: full Rust 448, Clippy, format, generated-contract parity,
  TypeScript, production build, root-only frontend tests, real native IPC E2E,
  and bounded Pumarejo lifecycle passed. The MCP Details content remains
  Code/Tests rather than post-change native observation.
- External mutation: none. No Jira, commit, branch, push, PR, reviewer, merge, deployment or release action is authorized or performed.

The canonical closeout is
`docs/orchestration/compound-master/gap-closure-rdm-024-mcp/summary.md`.

## Role Resolution

| Logical role | Canonical skill | Resolution | Status |
|---|---|---|---|
| Roadmap generator | `krt-roadmap-cartographer` | Exact canonical skill | Ready |
| Brainstorm | `ce-brainstorm` | Compound Engineering plugin exact skill | Ready |
| Plan | `ce-plan` | Compound Engineering plugin exact skill | Ready |
| Document review | `document-review` | Official current plugin replacement `ce-doc-review` | Ready |
| Work | `ce-work` | Compound Engineering plugin exact skill | Ready |
| Code review | `ce-review` | Official current plugin replacement `ce-code-review` | Ready |
| Security review | `krt-security-sentinel` | Exact canonical skill | Ready |
| Release handoff | `krt-release-marshal` | Exact canonical skill | Ready |
| CI investigation | `krt-ci-questor` | Exact canonical skill | Ready, optional |

Compound Engineering 3.19.0 is installed and enabled. Its own manifest and documentation establish `ce-doc-review` and `ce-code-review` as the current document/code review skills; those names resolve the older logical role labels retained by Compound Master. Execution remains serial in the current checkout under `worktree-policy:avoid`.

## Context And Policy

- Repository context: sufficient for discovery; existing Codex app-server, ACP runtime, `/mcp` host command, bus contract and prior runtime work packages provide concrete source material.
- Jira: optional degraded path. `krt-jira-scribe` is available, but no Jira URL/user/token environment configuration or initiative key was found; this is non-blocking until release handoff.
- Production posture: unknown. Compatibility with existing Codex, ACP, PTY, WSL, journal and permission behavior must be treated as a hard boundary until the roadmap establishes otherwise.
- Delegation: no worker or reviewer delegation started. Missing role resolution would add an unverifiable loop rather than reduce one.

## Active Artifact Set And Exact Resume

- Roadmap: `docs/roadmaps/2026-07-21-010-provider-neutral-mcp-layer-roadmap.md` (`roadmap-review-passed`).
- Roadmap review: `docs/review-findings/2026-07-21-rdm-023-roadmap-review.md` (`passed`; no open blocking findings).
- Brainstorm: in progress; no requirements artifact has been written yet. Grounding dossier target: `C:/Users/User/AppData/Local/Temp/compound-engineering-user/ce-brainstorm/mcp-layer-d1e7722020804ee987af0fec0f789bab/grounding.md`.
- Brainstorm evidence: the user normally leaves every MCP server active and wants an easier way to manage them.
- Brainstorm settled decision: the first management surface centers on reusable profiles/presets containing MCP combinations (`session-settled: user-directed` — chosen over direct per-server toggles, full configuration editing, or one global switch because profiles make recurring tool sets easier to manage).
- Brainstorm settled decision: profiles are project-local (`session-settled: user-directed` — chosen over globally reusable profiles or one application-wide active profile to keep each repository's MCP context isolated).
- Brainstorm settled decision: a profile stores only which already-defined MCP servers are enabled or disabled (`session-settled: user-directed` — chosen over embedding connection fields or credentials so reusable activation state stays separate from server definitions).
- Brainstorm settled decision: Tinto owns a provider-neutral MCP definition catalog and synchronizes provider-specific configurations from it (`session-settled: user-directed` — chosen over directly editing each provider's files or merely opening them externally so one project model can serve multiple runtimes).
- Brainstorm settled decision: Tinto imports existing MCP configurations from installed agent runtimes such as Codex, Claude, Kimi, and OpenCode (`session-settled: user-directed` — chosen over public MCP catalogs or marketplaces; discovery remains local and provider-config based).
- Brainstorm settled decision: import is an explicit initial/ad-hoc action; afterward Tinto is the source of truth and synchronizes outward only on an explicit user action (`session-settled: user-directed` — chosen over continuous bidirectional sync or provider-owned live overlays to avoid hidden writes and conflict machinery).
- Brainstorm settled decision: profile changes attempt live reload only where the runtime supports it; unsupported running sessions remain unchanged without a restart prompt, while future sessions consume the updated profile (`session-settled: user-directed` — chosen over next-session-only behavior or automatic restarts to preserve continuity without hiding a forced interruption).
- Brainstorm settled decision: each project remembers a default MCP profile and the agent launcher shows it with an optional per-session override (`session-settled: user-directed` — visual option A; chosen over mandatory selection on every launch or silently inheriting a project-active profile so the common path remains one gesture while the effective profile stays visible). Disposable probe: `C:/Users/User/AppData/Local/Temp/compound-engineering-user/ce-brainstorm-visual/mcp-profile-flow-20260721/screens/001-profile-selection.html`.
- Brainstorm settled decision: MCP status uses two independent axes: definition health (`Ready` or `Error`) and per-provider synchronization (`Synced`, `Pending`, `Unsupported`, or `Error`) (`session-settled: user-directed` — chosen over one combined state or a detailed runtime lifecycle so configuration validity is not confused with provider delivery).
- Brainstorm settled decision: explicit sync detects provider-file drift since the last known sync and, on conflict, shows the differences and requires the user to choose overwrite or reimport (`session-settled: user-directed` — chosen over automatic overwrite or automatic reimport; external edits are never replaced silently).
- Brainstorm settled decision: credentials remain provider-owned; Tinto stores and synchronizes only non-sensitive MCP configuration, preserves provider-specific secret material, and reports when authentication must be completed in that runtime (`session-settled: user-directed` — chosen over environment-variable references or a Tinto-managed system credential vault).
- Brainstorm settled decision: catalog, profiles, import, non-sensitive configuration, conflict review, and synchronization live in a project-scoped MCP section reachable from Agents; the launcher only exposes the effective profile and optional override (`session-settled: user-directed` — chosen over global settings or putting the full management surface in the launcher).
- Brainstorm settled decision: explicitly attributable MCP tool calls render as compact inline conversation events with visible server, tool, and running/completed/error state; inputs and outputs are collapsed by default (`session-settled: user-directed` — chosen over a side panel or timeline-only summary). Activity without runtime evidence of MCP attribution remains generic tool activity.
- Brainstorm settled decision: each project has an explicit set of managed providers; all detected runtimes are selected initially during import, and the user can persistently exclude providers from later syncs (`session-settled: user-directed` — chosen over always targeting every installed provider or choosing targets on every sync).
- Brainstorm settled decision: Windows and each WSL distribution are independent trust and synchronization targets with explicit import, provider selection, and sync; configuration never crosses those boundaries automatically (`session-settled: user-directed` — chosen over mirroring the Windows catalog into WSL or omitting WSL management).
- Brainstorm settled decision: initial import never deduplicates MCP definitions across providers; every imported definition remains independent and retains its source attribution even when names match (`session-settled: user-directed` — chosen over conservative equivalence merging or automatic name-based merging).
- Brainstorm settled decision: adding or editing a non-sensitive MCP definition supports both a guided form and pasted provider JSON/TOML, with a normalized preview before saving and sensitive fields excluded (`session-settled: user-directed` — chosen over form-only or paste-only authoring).
- Brainstorm settled decision: the first import creates an `Imported` default profile that mirrors the enabled/disabled state observed in each source runtime (`session-settled: user-directed` — chosen over enabling everything or starting with every MCP disabled so adoption preserves existing behavior).
- Brainstorm settled decision: every imported definition remains bound to its source provider and synchronizes only back to that provider; an explicit `Copy to…` action creates a new independent definition for another provider (`session-settled: user-directed` — prevents non-deduplicated imports from being multiplied across all managed runtimes).
- Brainstorm settled decision: definition health includes automatic connectivity validation when the project MCP section opens and after synchronization; the UI shows the check in progress and does not continuously poll in the background (`session-settled: user-directed` — chosen over a manual test action or learning connectivity only during agent use).
- Brainstorm open decision: none at the product-behavior level; prepare the Phase 2.5 scoping synthesis and confirmation.
- Brainstorm, plan and work package: not started. No implementation started.
- Blockers: none for roadmap review.
- Exact resume invocation:
  `krt-compound-master "Tinto provider-neutral MCP layer" mode:full production:unknown jira-policy:optional parallel:false delegation:auto worktree-policy:avoid autonomy:guarded review-threshold:P0-P2`

## Previous Completed Initiative

The remaining historical sections describe the completed Kimi/OpenCode ACP initiative and are retained as evidence. Its durable closeout is `docs/orchestration/2026-07-18-kimi-opencode-agent-support-compound-master-summary.md`.

## Previous Initiative Resume Snapshot

- Initiative: RDM-023, a consent-gated installation protocol for missing supported agent runtimes on the exact local-host or WSL target that owns the repository.
- Current phase: RDM-023 implementation, review, security and verification are complete locally; shipping boundaries remain unchanged.
- Status: `completed`.
- Previous initiative: RDM-022 Kimi/OpenCode agent support remains completed on `develop`; its closeout is preserved in `docs/orchestration/2026-07-18-kimi-opencode-agent-support-compound-master-summary.md`.
- Branch/base: `develop` at `d1f2659`, aligned with `origin/develop` at preflight. The working tree was clean before this artifact transition.

## Operating Posture

- Production posture: `prototype`, based on the prior accepted project posture. Existing installed-agent launch behavior is a compatibility boundary.
- Autonomy: guarded local artifact work only. No external mutation classes are authorized.
- Delegation: inline; repository instructions require sequential main-thread execution. No subagents used.
- Parallel: false. Worktree policy: avoid.
- Jira policy: optional. No Jira mutation or required Jira context is part of the current artifact phase.
- Review threshold: P0-P2.
- Global-software mutation boundary: roadmap and planning may describe recipes, but no real provider installation, elevation, branch, commit, push, PR, Jira, deployment or release action is authorized.

## Resolved Roles

- Roadmap generator: `krt-roadmap-cartographer` (used; context sufficient).
- Brainstorm: `compound-engineering:ce-brainstorm` (existing requirements will be reconciled, not duplicated).
- Plan: `compound-engineering:ce-plan`.
- Document review: `compound-engineering:ce-doc-review`.
- Work: `compound-engineering:ce-work` (completed inline).
- Code review: `compound-engineering:ce-code-review` (completed sequentially; no open P0-P2 findings).
- Security review: `krt-security-sentinel` (required during execution because the initiative mutates global software and crosses process/supply-chain boundaries).
- Release handoff: `krt-release-marshal` (not requested; Compound Master does not ship).

## Context And Artifact Set

- Context readiness: passed. Product intent, current host/WSL launch shape, bus/UI boundaries, security constraints and CI gates are documented.
- Roadmap: `docs/roadmaps/2026-07-21-001-agent-runtime-installation-protocol-roadmap.md` (`roadmap-review-passed`; no open P0-P2 findings).
- Planning input: `docs/brainstorms/2026-07-20-023-agent-runtime-installation-protocol.md` (`planning-input-review-passed`; one coherence/security fix applied, no open P0-P2 findings).
- Plan: `docs/plans/2026-07-21-023-feat-agent-runtime-installation-protocol-plan.md` (`implementation-ready`; review passed with no open P0-P2 findings).
- Work package: `docs/work-packages/RDM-023-agent-runtime-installation-protocol/2026-07-21-023-agent-runtime-installation-protocol-work-package.md` (`execution-ready`; checker and document review passed).

## Gates

- Roadmap Generator Gate: passed; exactly one roadmap was produced.
- Roadmap review: passed with coherence, feasibility, product, design, security and adversarial lenses; no open P0-P2 findings.
- Brainstorm: existing product discovery is captured; no skipped brainstorm override.
- Planning input review: passed after making elevation recipe/preflight-controlled and authorization-bound rather than installer-output-controlled.
- Plan and plan review: passed after Windows shell-free launcher and no-persistence corrections.
- Work package and Reviewability Gate: passed. The checker reports `work package review-unit checks passed`; review found and fixed RU2 base ambiguity.
- Execution: U1-U4 complete. Focused Rust protocol tests pass 14/14 and focused frontend/contract tests pass 55/55.
- Full gates: contract, Prettier, ESLint, frontend build and 711/711 Vitest tests pass. Rust fmt, Clippy and build pass. The combined Rust run passed 408/412; all four unrelated historical timing/order failures pass isolated, while all RDM-023 tests passed in the combined run.

## Risks And Decisions

- High-risk surfaces: global package installation, official-source provenance, process/output bounds, target-runtime isolation, cache invalidation and exactly-once launch continuation. Native elevation is explicitly excluded from the shipped recipes.
- Every provider/runtime recipe was revalidated against current official documentation on 2026-07-21. Unsupported or ambiguous future combinations must fail closed rather than use guessed commands.
- Official-source planning result: use direct npm argv for all four providers. Remote-script installers are excluded because they require shell/PowerShell evaluation. Shipped recipes declare no elevation; npm prerequisite or permission failures return safe manual guidance.
- Plan-review correction: native Windows npm shims are `.cmd`/`.ps1` and cannot satisfy shell-free execution directly. The plan now requires a validated existing `node.exe` plus associated `npm-cli.js` launcher and fails unsupported if that pair cannot be proven.
- Automated verification must use fake recipes, installers, elevation and probes. Any real manual installation requires separate explicit user consent.
- Final code/security review fixed cancellation-vs-launch ordering, Unix forced cleanup, descendant pipe cleanup, minimal environment, URL credential redaction and fake process/runtime coverage. No P0-P2 finding remains.

## Blockers And Next Action

- Blockers: none.
- Completed now: U4, Impact Scan, full verification, structured code review and Security Sentinel.
- Branch/PR strategy: RU1 uses `feat/agent-runtime-installation-core`; wait for it to merge into `develop`, then branch RU2 as `feat/agent-runtime-installation-ui`. No open stacked chain.
- Next action: optional Release Marshal handoff if the user later authorizes branch/commit/PR work; Compound Master performed no shipping mutation.

## Archived History

- Prior detailed history remains at `docs/orchestration/archive/compound-master-state/2026-07-13-codex-app-server-runtime-full-state.md`.
