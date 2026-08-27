---
artifact_kind: roadmap
artifact_path: docs/roadmaps/2026-07-13-008-post-ux-agent-platform-roadmap.md
title: Tinto - Mobile companion backlog
status: backlog
date: 2026-07-13
last_reconciled: 2026-08-27
initiative: post-ux-agent-platform
backlog_item: RDM-021
retired_items:
  - RDM-018
  - RDM-019
  - RDM-020
production_posture: prototype
source_docs:
  - README.md
  - docs/brainstorms/2026-07-10-tinto-mobile-companion-feasibility.md
  - docs/product/application-atlas.md
---

# Tinto - Mobile companion backlog

## Scope decision — 2026-08-27

- RDM-018 (agent memory), RDM-019 (remaining native-agent adapter work), and
  RDM-020 (file-content search) are retired from the product roadmap. They are
  not planned, deferred implementation, or candidates for the next initiative.
- Existing code and historical artifacts remain intact. Retirement does not
  reverse already delivered provider/runtime work.
- RDM-021 remains in the backlog for possible future reconsideration.
- RDM-021 is not the next initiative. Current priorities will be defined
  separately; this document does not infer or reserve that slot.

## Backlog item

### RDM-021 — Mobile companion with Tinto Desktop as authority

Potential outcome: a read-mostly mobile client can pair with Tinto Desktop on
a local network, recover after suspension or reconnection, and observe selected
desktop state without direct filesystem or shell access.

This is retained as a product direction, not as an approved implementation
plan. Before activation it requires a new requirements and security gate that
decides:

- whether the first user can only observe or can also intervene;
- LAN-only versus any wider network scope;
- desktop-host lifecycle and availability expectations;
- the first mobile platform;
- pairing, revocation, encryption, permissions, and audit behavior; and
- a closed list of any remote actions.

If activated, the safe sequence remains transport separation, pairing and
security design, a LAN read-only client, and only then evidence-backed audited
actions. Internet relay, arbitrary shell access, destructive offline actions,
desktop docking parity, and a full mobile terminal remain outside the backlog
item.

## Activation rule

Do not create a branch, plan, work package, or implementation unit for RDM-021
until the user explicitly selects it as a current initiative and approves its
requirements/security packet. Until then, Tinto has no next initiative recorded
in this roadmap.

## Historical context

The earlier version of this roadmap sequenced RDM-017 through RDM-021. RDM-017
was reconciled through the truthful WSL PTY limitation, and the OpenCode/Kimi
portion of the former adapter direction was delivered separately as RDM-022.
Those delivery records remain in their focused roadmaps and work packages.
