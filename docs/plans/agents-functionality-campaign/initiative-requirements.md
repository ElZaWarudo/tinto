---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
initiative: agents-functionality-campaign
status: approved-by-explicit-user-mandate
date: 2026-08-27
---

# Agents functionality campaign requirements

## Intent

Exercise the shipped Agents experience through native Tinto boundaries, distinguish
product defects from automation limitations, fix confirmed usability and functional
gaps, and rerun the same campaign against one consolidated branch.

## Actors

- A developer launching an Agent for a registered local project.
- A developer returning to saved conversations and inspecting prior work.
- A keyboard or constrained-window user who must retain access to primary actions.

## In scope

- Agents entry, tab identity, quick launch, provider readiness, and launch failure copy.
- Live conversation send, progress, interrupt, recovery, close, and resume.
- Saved-session lifecycle labels, diagnostics, search, Details, Agent Lens, and MCP profiles.
- Native Pumarejo evidence plus focused deterministic tests for states that cannot be
  safely or reliably produced through the current native harness.

## Out of scope

- New Agent providers, persistence services, provider protocols, or MCP capabilities.
- Provider configuration writes, credential handling changes, or destructive testing
  against user conversations.
- Pumarejo redesign; unsupported focus/resize observations remain harness blockers.
- Broad visual redesign or unrelated refactoring.

## Invariants

- A launch affordance never contradicts known provider readiness; unknown readiness is
  described truthfully and remains retryable.
- Saved records are not described as currently running without a matching live session.
- Project MCP management is discoverable before a provider process is launched.
- Full diagnostics remain programmatically available when visual copy is truncated.
- Session lifecycle labels are localized consistently across the Agents home and transcript.
- Native controlled-write cases never authorize repository or profile mutation and always
  record cleanup plus before/after git state.

## Success criteria

1. The 14-case campaign kit validates and every case has pass, fail, or evidence-backed
   blocked disposition.
2. Confirmed product failures receive focused regression tests and minimal fixes inside
   existing Agents seams.
3. Launch, harmless send, explicit response interruption, composer recovery, and saved
   transcript reopen pass through Pumarejo without repository mutation.
4. The post-fix native campaign contains no unresolved product failure at or above high
   priority; harness limitations remain clearly separated.
5. Root aggregate verification passes on the consolidated branch.

## Settled decisions

- The user's explicit autonomous mandate approves the exact campaign, isolated worker
  dispatch, product-code fixes, reconciliation, and local consolidation described here.
- Implementation units are serialized because their minimal edit paths overlap in
  `ConsoleDockPanel.tsx`.
- Existing `AgentMcpPanel`, provider-readiness helpers, and session lifecycle models are
  reused; no new infrastructure or dependency is introduced.
- The campaign-created Agent prompt forbids tools and file access.

## Escalation boundaries

- Stop a unit if it requires public bus/backend contract changes, provider writes,
  credentials, dependency changes, or destructive user-data cleanup.
- Treat unsupported Pumarejo window resize and focus observation as harness blockers,
  not permission to substitute coordinate automation.
- Record a native failure as environment-specific when it cannot be reproduced by the
  same boundary and deterministic contract checks.

