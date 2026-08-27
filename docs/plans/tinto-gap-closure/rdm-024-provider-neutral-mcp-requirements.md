---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: inherited initiative contract plus focused item evidence
title: RDM-024 provider-neutral MCP control plane requirements
status: planning-input-review-passed
date: 2026-08-27
initiative: tinto-gap-closure
roadmap_item: RDM-024
compound_run_id: gap-closure-rdm-024-mcp
---

# RDM-024 provider-neutral MCP control plane

## Outcome

Give a developer one project-scoped place to inspect the non-sensitive MCP
definitions already known to a supported runtime, retain source attribution,
and choose a reusable enablement profile. The first implementation slice is
deliberately evidence-bounded: it admits the existing Codex local inventory
path and the provider-neutral profile/catalog model, while leaving provider
file writes, cross-boundary copying, unsupported launch overrides, and active
connectivity probes out until their target-specific evidence exists.

The historical unfinished initiative was previously labelled RDM-023. This
artifact, its plan, package, state, and roadmap use RDM-024. The completed
agent-runtime installation protocol remains RDM-023 and is not changed by this
run.

## User flow and acceptance outcome

1. From the project's Agents surface, the developer opens the MCP section.
2. Tinto performs a bounded, read-only inventory for each admitted target and
   shows an empty, success, partial, or safe error result without exposing
   arguments, environment values, headers, or credentials.
3. The developer can keep same-named definitions independent by source,
   create or select a project-local enablement profile, and see the effective
   profile without changing a provider file.
4. A later synchronization or launch-override action is shown as unavailable
   until its target has passed the evidence gate. No unsupported action is
   presented as successful.
5. A session may render MCP activity only when its provider emits explicit MCP
   attribution; otherwise the activity remains generic.

## Evidence gate

The table is a go/no-go admission record for the current repository evidence.
`GO` means the bounded first slice may be planned. `NO-GO` means the behavior
is deferred and must not be generalized from absence of evidence.

| Provider | Target | Inventory/import | Synchronization | Profiles | Launcher application | Connectivity evidence | Activity attribution | Evidence and decision |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Codex | Windows/local | GO | NO-GO | GO, project-local only | NO-GO for provider override | NO-GO for active probing; passive/Unknown only | GO only for explicit `mcptoolcall` events | `commands.rs` reads the Codex config path and lists names/command availability while suppressing sensitive fields; no safe write or launch-override proof exists. |
| Codex | each WSL distribution | NO-GO pending target-specific config-root proof | NO-GO across trust boundaries | GO as neutral project state after an admitted import | NO-GO | NO-GO for active probing | GO only when the native provider event is explicit | WSL routing and containment exist, but this item has no provider-config fixture proving the WSL root, identity, and write boundary for MCP. |
| Claude | Windows/local | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO without an explicit attribution event | No current Claude MCP config parser or attribution evidence is in the named context. |
| Claude | each WSL distribution | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO | No target-specific parser, root, identity, or attribution evidence is available. |
| Kimi | Windows/local | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO without an explicit attribution event | ACP capability fixtures prove negotiated MCP capability, not local config inventory or attribution. |
| Kimi | each WSL distribution | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO | ACP capability evidence does not prove a safe target config root or write path. |
| OpenCode | Windows/local | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO without an explicit attribution event | ACP capability fixtures prove negotiated MCP capability, not local config inventory or attribution. |
| OpenCode | each WSL distribution | NO-GO | NO-GO | GO as neutral state only | NO-GO | NO-GO | NO-GO | ACP capability evidence does not prove a safe target config root or write path. |

The neutral profile `GO` cells are storage and selection only; they never grant
permission to write a provider file or claim that the provider consumed the
profile. A shared provider abstraction remains prohibited until the same
parsing, containment, or mutation rule is demonstrated for at least two
current providers.

## Functional requirements

### RDM24-R1 — Source-bound, secret-safe inventory

- Import is read-only and bounded to the canonical provider root for the
  current user and target.
- Each definition retains provider and target attribution. Identical names
  from two sources never merge automatically; an explicit `Copy to...` is a
  future, separate action.
- The normalized catalog contains only the non-sensitive fields needed for
  display and enablement. Arguments, environment values, headers, tokens,
  and credential references are not rendered or persisted by this slice.
- Empty, malformed, unavailable, and partially readable sources produce
  target-labelled safe states and preserve any prior catalog.

### RDM24-R2 — Project-local profiles

- A profile is a project-local enablement set over imported source-bound
  definitions; it does not contain provider credentials or arbitrary config.
- The first successful import creates an `Imported` default reflecting the
  observed enabled state. The active default cannot be deleted without first
  selecting a replacement.
- Create, rename, delete, and default selection are explicit actions. The UI
  announces loading, empty, success, partial, error, and unsupported outcomes.
- Until a target passes the evidence gate, profile state remains neutral and
  no provider synchronization is implied.

### RDM24-R3 — Explicit activity attribution

- A timeline event is labelled MCP only when the provider event identifies MCP
  and supplies bounded server/tool identity.
- Inputs and outputs remain collapsed by default and use existing redaction and
  size limits. Unattributed tool activity stays generic.
- Provider-derived strings, identifiers, statuses, and errors are untrusted:
  validate shape and size, remove control characters, redact secrets, and
  render them as inert text before bus, UI, journal, or logs.

### RDM24-R4 — Boundary and compatibility protection

- Windows and each WSL distribution are separate trust and synchronization
  targets. Canonical provider roots are resolved for the current user; paths,
  symlinks, junctions, and WSL translations that escape are rejected.
- No imported command is executed and no arbitrary provider-defined network
  request is sent for connectivity. Unsupported or unproven checks report
  `Unsupported` or `Unknown`.
- Existing `/mcp` remains compatible while the project surface is introduced.
- New controls are keyboard-operable, have accessible names and state, restore
  focus predictably, announce asynchronous outcomes, do not rely on color
  alone, and work at the supported constrained window size and zoom.

## Non-goals and explicit deferrals

- Provider-file synchronization, drift overwrite, and reimport conflict UI are
  not admitted for any target until a provider-specific write/rollback and
  drift fixture exists.
- Launcher application of a profile or per-session override is not admitted
  until the target runtime proves a safe way to consume the selection without
  restart or hidden mutation.
- Active connectivity checks, automatic server launch, tool approval, MCP
  client/proxy behavior, cloud sync, marketplace/catalog discovery, credential
  storage, and new persistence infrastructure are excluded.
- No provider-neutral framework or adapter interface is introduced for one
  provider. Generalize only after two providers share an evidenced rule.

## Verification contract

- Rust unit tests cover Codex parsing, source/target identity, sensitive-field
  omission, size/control-character normalization, empty/malformed outcomes,
  and path/symlink/junction rejection fixtures.
- Generated-contract parity and frontend contract tests cover additive DTOs,
  command names, status axes, and compatibility with `/mcp`.
- UI tests cover the Agents entry point, keyboard/focus behavior, default
  profile lifecycle, same-name source independence, async outcomes, and
  unsupported actions.
- Existing Rust and frontend natural suites, formatting, lint, build, and the
  relevant Windows/WSL smoke path remain release gates owned by the root wave.

## Decision closure

The admitted first slice has no open product, authorization, tenant, data,
public-contract, or security decision. Any evidence that contradicts the
Codex-local read-only path, requires credential ownership, crosses Windows/WSL
boundaries, or requires a new persistent service must return to Seneschal as a
brokered decision instead of being inferred.
