# Agents functionality campaign

Status: complete — 11 passed, 0 product failures, 3 blocked by harness

Branch: `codex/agents-usability-edge-campaign`

Campaign kit: `docs/edge-tests/agents-usability-2026-08-27`

## Outcome

The full 14-case Agents campaign completed against the native Tinto application through
Pumarejo. All executable product oracles passed after the fix wave. Three cases remain
explicitly blocked by limitations of the current Windows automation adapter:

- keyboard focus progression does not expose a focused semantic node after bounded Tab
  actions;
- native resize to 640 by 480 is rejected as `UNSUPPORTED_ACTION`;
- activating an Agent Lens tab causes the semantic snapshot to collapse to the application
  shell, although the enabled Archivos, Comandos, and Timeline tabs are observable and
  focused component tests pass.

These are recorded as harness blocks, not product passes or failures.

## Product fixes delivered

1. Agents home exposes project-scoped MCP configuration for registered projects before a
   provider is launched.
2. Quick launch shares the provider-readiness check used by Dashboard, remains disabled
   while checking or unavailable, explains its state, and offers a forced retry.
3. Missing live sessions are no longer classified as active. Persisted starting or running
   journal rows without a matching live process render as `Archivada`.
4. Truncated saved-session diagnostics expose their complete bounded value through
   accessible name and title.
5. Exited, cancelled/canceled, and waiting transcript states are localized.
6. `list_workbenches` now returns the plural JSON field `workbenches` expected by the
   TypeScript contract while preserving the singular persisted TOML key `[[workbench]]`.

The native MCP retest no longer produces the generic inventory alert. For this repository
it truthfully reports the existing multiple-workbench ownership ambiguity and keeps the
saved-transcript panel read-only.

## Controlled live evidence

A new local Codex conversation launched to a ready composer. The campaign then:

- observed the exact `Detener respuesta` control during a long response;
- interrupted only that response without opening the session-stop confirmation;
- observed composer recovery;
- received the exact assistant response `TINTO_AGENT_CAMPAIGN_OK`;
- closed and reopened Tinto;
- restored one Agents tab containing both prompts and the exact response.

The repository status was identical before and after the controlled conversation, and each
Pumarejo run closed in `idle`.

## Verification

- Campaign kit validator: valid, including all oracle digests.
- ConsoleDockPanel focused suite: 28 passed.
- Terminal MCP and lifecycle slice: 8 passed.
- Production frontend build: passed.
- Rust `list_workbenches` slice: 2 passed.
- Rust `workbench` slice: 41 passed.
- `git diff --check`: passed.
- Independent UI review: passed after one correction round.
- Independent native wire-contract review: passed with no release-blocking findings.

## Evidence handling

Sanitized outcomes are versioned in the campaign kit. Raw native snapshots and screenshots
remain local and unversioned under
`C:/Users/User/AppData/Local/Temp/tinto-agents-campaign` because saved conversations may
contain private content.

The confirmation popup reported during the campaign was the expected safety dialog for
`Detener sesión`. It was opened by an overly broad automation matcher, cancelled without
stopping the session, and the final runner uses an exact `Detener respuesta` match.
