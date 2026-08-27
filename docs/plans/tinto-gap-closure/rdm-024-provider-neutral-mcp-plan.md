---
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
title: RDM-024 provider-neutral MCP control plane implementation plan
status: plan-review-passed
date: 2026-08-27
initiative: tinto-gap-closure
roadmap_item: RDM-024
origin_requirements: docs/plans/tinto-gap-closure/rdm-024-provider-neutral-mcp-requirements.md
initiative_contract: docs/plans/tinto-gap-closure/initiative-requirements.md
compound_run_id: gap-closure-rdm-024-mcp
---

# RDM-024 implementation plan

## Plan boundary

This plan turns the accepted evidence gate into the smallest executable
capability slice. It uses the existing Rust Agent Console, Workbench TOML
store, Tauri bus, session journal, and Agents surface. It does not create a
provider framework, a second persistence service, a polling loop, or an MCP
client. Unsupported provider/target behaviors remain explicit deferrals rather
than placeholders that pretend to work.

The first slice admits only:

- Codex local Windows read-only inventory/import using the already-proven
  config path and parser seam.
- Source-bound, non-sensitive catalog projection and project-local enablement
  profiles.
- Explicitly attributed Codex MCP activity with generic fallback when
  attribution is absent.

The first slice does not write provider files, apply a profile to a provider
launcher, probe connectivity, or import from unproven Claude, Kimi, OpenCode,
or WSL configuration roots.

## Evidence gate and implementation admission

| Capability | Admitted target | Decision | Required proof before widening |
| --- | --- | --- | --- |
| Inventory/import | Codex Windows/local | GO | Parser keeps names/source/target and omits args/env/headers/credentials; malformed/empty/read errors are bounded. |
| Inventory/import | Other providers or WSL | NO-GO | Target-specific config-root, identity, parser, and fixture evidence. |
| Synchronization | None in first slice | NO-GO | Atomic write, drift fingerprint, overwrite/reimport choice, rollback, and target containment evidence. |
| Profiles | Project-local neutral enablement state | GO | Existing Workbench TOML persistence supports additive profile fields without secrets or provider writes. |
| Launcher application | None in first slice | NO-GO | A provider-specific, safe session input/override path that does not restart or mutate hidden state. |
| Connectivity evidence | Passive status only | GO for Unknown/Unsupported; NO-GO for active probe | Provider-emitted status or documented passive evidence; never execute imported commands or arbitrary URLs. |
| Activity attribution | Codex explicit MCP event | GO | `mcptoolcall` server/tool fields are explicit, bounded, redacted, and otherwise projected as generic activity. |

Contrary evidence pauses only the affected unit and returns a brokered
decision request to Seneschal. It never authorizes a neutral adapter by
default.

## Implementation units

### U1 — Safe Codex-local inventory and catalog contract

**Depends on:** inherited RDM-016/RDM-022 runtime and bus seams; none within
this plan.

**Purpose:** Replace the string-only `/mcp` interpretation with an additive,
source-bound non-sensitive DTO that can represent inventory outcomes without
breaking the existing host command.

**Exact repository surfaces:**

- `src-tauri/src/agent_console/commands.rs` — reuse the existing Codex config
  root and TOML parser; add bounded normalization, target identity, safe
  outcome, and source-bound definition projection.
- `src-tauri/src/agent_console/app_server.rs` — retain explicit
  `mcptoolcall` detection and apply the same bounded identity/redaction rule;
  do not infer MCP from names.
- `src-tauri/src/bus/contract.rs` — add only the additive catalog/outcome types
  and command/event fields required by the admitted slice.
- `src-tauri/src/lib.rs` — register only the concrete inventory command if the
  existing host command cannot carry the structured response.
- `src/bus/contract.ts` — curate compatibility notes and adjunct types.
- `src/bus/contract.generated.ts` — regenerate from Rust; never hand-edit.
- `src/bus/client.ts` — add the typed invoke/listener wrapper only when the
  Rust command is registered.

**Focused tests:**

- Colocated `commands.rs` tests for Codex TOML names, duplicate names,
  omission of args/env/headers, empty/malformed input, bounded provider text,
  and target/source labels.
- Colocated `app_server.rs` tests for explicit `mcptoolcall` projection,
  generic fallback, control-character removal, size limits, and secret
  redaction.
- `src/bus/contract.test.ts` and `npm run contract:check` for additive command,
  event, enum, and generated-contract parity.

**Security boundary:** no command execution, network request, provider-file
write, credential access, or cross-target path translation.

### U2 — Project-local profile state over imported definitions

**Depends on:** U1.

**Purpose:** Persist the minimum profile state needed to select enabled
source-bound definitions for a project while keeping provider credentials and
provider-owned configuration outside Tinto's model.

**Exact repository surfaces:**

- `src-tauri/src/workbench/mod.rs` — extend the existing Workbench TOML model
  with additive MCP catalog/profile fields, bounded names, default-profile
  lifecycle, and corrupt/partial-config preservation behavior.
- `src-tauri/src/workbench/commands.rs` — expose explicit list/import/profile
  lifecycle commands through the existing store lock and atomic persistence
  seam.
- `src-tauri/src/lib.rs` — register the concrete Workbench MCP commands.
- `src-tauri/src/bus/contract.rs` — define the profile/catalog command response
  and status axes without conflating definition health and provider delivery.
- `src/bus/contract.ts` and `src/bus/client.ts` — expose the typed frontend
  surface and safe error states.
- `src/bus/contract.generated.ts` — regenerate and parity-check the public
  mirror.

**Focused tests:**

- `src-tauri/src/workbench/mod.rs`/`commands.rs` tests for first-import
  `Imported` default, create/rename/delete, active-default replacement,
  source-bound same-name definitions, atomic failure preservation, and no
  secret serialization.
- `src/bus/contract.test.ts` for exact command names, additive DTOs, and the
  independent definition-health/provider-sync axes.

**Security boundary:** profile mutations are explicit and project-local; they
  never synchronize provider files, copy across Windows/WSL targets, or store
  credential material.

### U3 — Agents profile surface and truthful activity states

**Depends on:** U1 and U2.

**Purpose:** Make the admitted catalog/profile flow usable from Agents and
  preserve truthful unsupported states, accessibility, and existing `/mcp`
  compatibility.

**Exact repository surfaces:**

- `src/panels/terminal/TerminalPanel.tsx` — add the project MCP entry point,
  inventory/profile states, default/override display without pretending to
  apply unsupported provider settings, and explicit activity rendering.
- `src/panels/terminal/TerminalPanel.test.tsx` — exercise the user flow,
  keyboard/focus restoration, async announcements, source-bound duplicates,
  lifecycle errors, and unsupported actions.
- `src/App.css` — add only the styles needed for constrained-window, zoom,
  non-color-only state, and focus-visible presentation.
- `src/bus/client.ts`, `src/bus/contract.ts` — consume the stable U1/U2 surface
  without duplicating normalization or provider logic.

**Focused tests:**

- ```text
  npx vitest run src/panels/terminal/TerminalPanel.test.tsx src/bus/contract.test.ts
  ```
- Assertions cover loading, empty, success, partial, error, conflict/not
  supported messaging; accessible names/state; keyboard operation; focus
  restoration; and collapsed redacted activity details.

**Security boundary:** provider-derived text is rendered inertly; no raw
  command, env, header, URL, or tool payload is inserted into markup or logs.

## Deferred evidence units

These are recorded for traceability but are not execution-ready units in this
package:

- `D1` — provider-specific WSL and non-Codex inventory/import adapters. Requires
  per-target root, identity, parser, and fixture evidence.
- `D2` — explicit synchronization and drift conflict handling. Requires
  provider-specific atomic write/rollback evidence and a user-approved
  overwrite-or-reimport interaction.
- `D3` — launcher application and per-session provider overrides. Requires a
  safe provider session contract and proof that unsupported running sessions
  remain uninterrupted.
- `D4` — active connectivity validation. Requires a non-executing,
  provider-defined-safe signal; active commands and arbitrary network requests
  stay prohibited.
- `D5` — guided/pasted definition authoring and `Copy to...`, retained as a
  later increment after import/profile/sync evidence proves the control plane.

## Dependency and wave order

```text
Wave A: U1 (Codex local catalog + activity evidence)
Wave B: U2 (project-local profile persistence)
Wave C: U3 (Agents surface and accessibility)
Deferred: D1-D5 after new provider evidence and brokered scope review
```

All mutable implementation is serial because U1/U2 change additive public
contracts, central persistence, and the shared Agents surface. A later unit
must consume the refreshed integration base rather than stack against an
unmerged sibling.

## Impact scan

- **Public contracts:** additive Rust/TypeScript catalog, profile, outcome,
  and activity fields; generated TypeScript parity is required.
- **Persistence:** additive fields in existing Workbench TOML only; no new
  database, index, service, or migration framework.
- **Auth/tenant/ownership:** no account or tenant boundary is introduced;
  source/target identity and current-user canonical roots are mandatory.
- **Consumers:** `src/bus/client.ts`, `src/bus/contract.ts`, generated mirror,
  `TerminalPanel.tsx`, and existing host command tests.
- **Contract-drift checks:** `npm run contract:check`, exact command-name tests,
  serialized Workbench compatibility fixtures, and existing `/mcp` host command
  tests.

## Verification contract

Each unit runs its focused tests above. The root wave owns aggregate
`pnpm`/`npm` frontend and Rust formatting, lint, build, test, and relevant
Windows/WSL smoke gates. No unit claims aggregate or native evidence from a
focused check. A failure in a pre-existing unrelated test is recorded by the
root as a baseline gap; an owned failure receives at most two fix rounds.

## Review and security gates

- Document review threshold: P0-P2; findings at or above threshold block the
  package until corrected.
- Security review is inline for this artifact stage and must revisit each
  parser, normalization, path, persistence, and rendering boundary before code
  execution. A dedicated Security Sentinel is required if a later execution
  unit adds credentials, network listeners, provider process control, or
  destructive synchronization.
- Release readiness remains false until the root observes the real diff,
  focused checks, aggregate verification, and the applicable security review.
