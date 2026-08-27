---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: krt-swarm-seneschal
title: Tinto gap-closure initiative contract
status: in-review
date: 2026-08-27
initiative: tinto-gap-closure
---

# Tinto gap-closure initiative contract

## Goal capsule

Let a local developer launch an agent with an appropriate reusable MCP
combination for the current project without repeatedly editing provider files,
while also restoring current native evidence and roadmap truth. The work must
preserve Tinto's local, passive monitoring promise and use the smallest
implementation that satisfies provider evidence.

## Actors

- **A1 — Local developer/operator:** observes repositories and directs supported
  coding-agent sessions.
- **A2 — Supported agent runtime:** Codex, Claude, Kimi, OpenCode, or another
  runtime already represented by Tinto's existing runtime catalog.
- **A3 — Tinto backend:** owns filesystem, process, configuration, redaction,
  and session authority.
- **A4 — Maintainer:** reviews evidence, keeps the roadmap coherent, and decides
  whether a change is release-ready.

## Demonstrated gaps

1. The provider-neutral MCP roadmap is active and its product decisions are
   recorded, but no requirements artifact, implementation plan, work package,
   or implementation exists.
2. The MCP initiative reused `RDM-023`, which already identifies the completed
   agent-runtime installation protocol.
3. Native Windows confirmation of the July accessibility fixes was blocked by
   the old Pumarejo launch path. A later change claims to make smoke launches
   reliable, but the evidence and application atlas were never refreshed.
4. The application atlas still identifies itself as stale, and the active
   post-UX roadmap no longer reflects the sequence of delivered agent work.

## Product contract

### Initiative governance

- **R1 — Collision-free initiative identity.** The provider-neutral MCP item is
  `RDM-024`. Historical RDM-023 installation artifacts remain unchanged except
  for links that incorrectly identify the MCP initiative.
- **R2 — Artifact gate before MCP implementation.** RDM-024 must complete its
  focused requirements, plan, work package, document review, and security-aware
  execution gate before product code changes begin.

### MCP capability

- **R3 — Evidence-led provider model.** RDM-024 must first record, per provider
  and Windows/WSL target, evidence for inventory reads, configuration writes,
  launch-time overrides, connectivity signals, and activity attribution.
  Read-only inventory/import may proceed from provider-specific evidence.
  Profiles, synchronization, and launch overrides proceed only for targets with
  a proven safe capability; a shared abstraction requires the same rule in at
  least two providers. Contrary evidence pauses the affected behavior for a
  brokered scope decision rather than forcing a neutral abstraction.
- **R4 — Minimal MCP control plane and flow.** The target flow begins in the
  project MCP section reachable from Agents: explicitly import existing
  definitions, review the inventory, create or select a project-local enablement
  profile, explicitly synchronize supported provider targets, then launch with
  a visible default profile or per-session override. First import creates an
  `Imported` default profile. Profiles support create, rename, and delete; the
  active default cannot be deleted until a replacement is selected. Unsupported
  steps remain visibly unavailable. Existing `/mcp` behavior remains compatible.
- **R5 — Explicit, bounded mutation.** Import is read-only. Synchronization is a
  user action, detects drift, never replaces external edits silently, never
  copies configuration across Windows/WSL trust boundaries, and never stores or
  rewrites provider-owned credentials. Reads and writes stay within canonical
  provider-approved roots for the current target and user; paths, symlinks,
  junctions, or WSL translations that escape are rejected. Provider-derived
  fields, identifiers, statuses, and errors are untrusted: validate their
  schema, bound size, normalize control characters, redact them, and render them
  inertly before bus, UI, logs, or persistence. Import and synchronization show
  loading, empty, success, partial-success, error, and drift-conflict outcomes,
  identify the affected target, and preserve the prior effective configuration
  on failure. Connectivity validation must not execute imported commands or send
  arbitrary provider-defined network requests; use passive evidence or report
  `Unsupported`/`Unknown`.
- **R6 — Truthful activity.** An MCP call may be shown with server and tool
  identity only when the provider emits explicit attribution. Other tool
  activity remains generic. Inputs and outputs are collapsed by default and
  pass existing redaction and size limits.

### Evidence and closeout

- **R7 — Native regression evidence.** Re-run the Windows-native keyboard,
  responsive-layout, Timeline, Dashboard action, Agent, and detached-window
  smoke flows using the repaired launch path. Record failures as evidence; do
  not silently broaden the run into unrelated fixes.
- **R8 — Current product map.** Refresh the application atlas only from observed
  current evidence, and reconcile roadmap statuses/identifiers with delivered
  repository history.
- **R9 — Verification.** Every implementation unit must name focused checks.
  The final wave must pass generated-contract parity, formatting, lint,
  frontend tests/build, Rust formatting/Clippy/tests/build, and the relevant
  Tauri smoke path. Every new MCP control and state must be keyboard-operable,
  expose an accessible name and state, restore focus predictably, announce
  asynchronous results, avoid color-only meaning, and remain usable at Tinto's
  supported constrained window size and zoom.

### Acceptance examples

- **AE1:** A developer can launch an agent with the project's visible default
  MCP profile or an explicit per-session override without individually editing
  provider files.
- **AE2:** Existing Codex MCP configuration can be imported without revealing
  arguments, environment values, headers, or credentials.
- **AE3:** Selecting a project profile does not mutate provider files. Only a
  separate explicit synchronization changes supported targets, and a provider
  file changed externally produces a conflict instead of an overwrite.
- **AE4:** Two identically named servers imported from different providers stay
  independent unless the user explicitly copies one.
- **AE5:** A running provider that cannot reload configuration continues
  uninterrupted; later sessions use the new profile.
- **AE6:** An unattributed tool event is not displayed as an MCP event.
- **AE7:** A partial synchronization identifies each successful and failed
  target, preserves the previous effective configuration for failed targets,
  and offers no unsafe retry or overwrite shortcut.
- **AE8:** Native regression evidence names the exact commit, environment,
  exercised flows, and any unverified surfaces before the atlas loses its
  `stale` status.

## Settled decisions preserved from discovery

- Profiles are project-local and store enablement only.
- Tinto owns the normalized non-sensitive catalog after explicit import.
- Initial import keeps same-named definitions independent and source-bound; an
  explicit `Copy to…` creates a new definition for another provider.
- Provider files are updated only by an explicit synchronization action.
- Credentials remain provider-owned.
- Windows and every WSL distribution are separate trust/synchronization targets.
- Each project persists its managed providers and may exclude detected runtimes.
- Provider drift requires a visible overwrite-or-reimport decision.
- Status keeps definition health (`Ready`/`Error`) separate from provider sync
  (`Synced`/`Pending`/`Unsupported`/`Error`).
- The launcher shows the default profile and allows a per-session override.
- The full management surface is project-scoped and reachable from Agents.
- Explicitly attributable MCP calls appear as compact inline conversation
  events; unattributed activity remains generic.
- Connectivity checks occur when the MCP surface opens and after synchronization,
  not through continuous background polling.
- Guided and pasted provider JSON/TOML definition authoring with normalized
  preview remains a later RDM-024 increment after import/profile/sync proves the
  control plane; it does not expand the first implementation slice.

## Simplicity constraints

- Reuse the existing Rust agent-console authority, bus generation flow,
  provider adapters, session journal, launcher, and project-scoped UI patterns.
- Do not add a new service, daemon, database, plugin framework, generic settings
  engine, persistent index, or background polling subsystem.
- Do not build an MCP client, traffic proxy, marketplace, cloud sync, credential
  vault, automatic server launcher, or automatic tool approval.
- Do not abstract provider behavior until the evidence matrix demonstrates a
  common rule used by at least two supported providers.
- Prefer one reviewable capability slice; split only when contract and UI work
  are independently verifiable. Never create a three-PR stack merely because
  the old roadmap listed three review units.
- A failed native smoke may create a separate bounded defect unit. It does not
  authorize speculative refactoring of the harness or product.

## Global non-goals

- Agent memory, file-content search, mobile companion work, remote/cloud
  operation, and new provider onboarding.
- Jira creation, release tagging, publishing, deployment, or automatic merging.
- Reworking already-delivered dependency upgrades unless current verification
  identifies a regression.

## Escalation boundaries

Stop the affected unit and broker a decision when evidence requires credential
storage, automatic provider-file mutation, cross-boundary Windows/WSL copying,
provider process restarts, a new persistent schema, or weakening attribution or
redaction rules. Independent documentation and QA work may continue.

## Success criteria

- RDM-024 has a reviewed, execution-ready artifact chain with no unresolved
  product or security decision.
- The provider evidence gate either admits a bounded implementation or records
  a brokered no-go/narrowing decision before shared synchronization code exists.
- Its accepted implementation is verified without regressing existing agent
  runtimes or `/mcp`.
- Windows-native regression evidence is current and the atlas truthfully states
  what is observed versus unverified.
- Active roadmap identifiers and statuses are internally consistent.
- No extra infrastructure or speculative feature is introduced.

## Sources

- `docs/orchestration/compound-master-state.md`
- `docs/roadmaps/2026-07-21-010-provider-neutral-mcp-layer-roadmap.md`
- `docs/review-findings/2026-07-21-rdm-023-roadmap-review.md`
- `docs/audits/2026-07-29-product-polish-regression-evidence.md`
- `docs/product/application-atlas.md`
- `src-tauri/src/agent_console/commands.rs`
- Commit `f1314d2` (`fix(dev): make Pumarejo smoke launches reliable`)
