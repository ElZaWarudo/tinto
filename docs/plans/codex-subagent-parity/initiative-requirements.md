---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
product_contract_source: krt-swarm-seneschal
title: Codex subagent parity initiative contract
status: approved-for-autonomous-execution
date: 2026-08-28
initiative: codex-subagent-parity
---

# Codex subagent parity initiative contract

## Goal capsule

Make Tinto a complete native supervision surface for Codex subagent workflows.
Tinto must reproduce the installed Codex agentic behavior while preserving its
own IADE visual language, persisted Agent Lens history, and passive-monitoring
boundary. Codex remains the first runtime; the internal domain must leave a
small, evidence-led adapter seam for other harnesses without building a generic
orchestrator prematurely.

## Actors

- **A1 — Developer/operator:** directs a primary Codex conversation and inspects
  or controls delegated children in Agent Lens.
- **A2 — Primary agent:** delegates bounded work, coordinates descendants, and
  consolidates results.
- **A3 — Subagent:** owns an inspectable child thread with an independent
  transcript, status, role, runtime configuration, and terminal result.
- **A4 — Codex app-server:** remains orchestration authority for spawn, routing,
  waiting, follow-up, interruption, closure, permissions, and nested threads.
- **A5 — Tinto:** projects, persists, restores, and presents agentic state; it
  never invents provider lifecycle facts or silently performs repository work.

## Product contract

### Agent hierarchy and lifecycle

- **R1 — Complete Codex behavioral baseline.** For the installed supported Codex
  app-server protocol, Tinto exposes every user-relevant agentic behavior:
  spawn and nested descendants, parent/child identity, roles and nicknames,
  active and terminal statuses, activity, follow-up, steer, interrupt, wait,
  close, result collection, and parent consolidation.
- **R2 — Arbitrary-depth tree.** Agent Lens represents descendants at any depth,
  preserves stable provider thread identity, and never flattens nested work into
  unrelated top-level sessions.
- **R3 — Provider-truthful projection.** Codex-specific payloads are parsed at the
  adapter boundary into a small Tinto-owned agent-thread model. Unknown fields
  and future item kinds are tolerated; unsupported behavior remains unavailable
  rather than guessed.
- **R4 — Parent remains authoritative.** Tinto does not implement a competing
  scheduler or synthesize agent results. The parent Codex thread remains
  responsible for delegation and final consolidation.

### Agent Lens experience

- **R5 — Tinto-native presentation.** The feature uses the existing Agent Lens,
  conversation timeline, workspace dock, typography, interaction, and state
  patterns. It does not copy Codex visual design.
- **R6 — Inspectable children.** Users can expand the tree, distinguish active
  and completed children, open any child transcript, view its prompt, role,
  model/reasoning when supplied, status, activity, result, and relationship to
  the parent, then return without losing context.
- **R7 — Direct supervision controls.** Agent Lens exposes direct per-child
  follow-up, interrupt, and close actions with the same underlying semantics as
  Codex. Controls are capability- and state-gated, provide pending/success/error
  feedback, preserve focus, and cannot silently target the wrong child.
- **R8 — Accessible asynchronous state.** Tree navigation and controls are fully
  keyboard-operable, expose names/relationships/status, announce meaningful
  lifecycle changes without flooding live regions, avoid color-only meaning,
  and remain usable at Tinto's supported constrained window and zoom.

### Persistence, recovery, and safety

- **R9 — Full restoration.** Tinto persists and restores the complete hierarchy,
  child transcripts, lifecycle events, results, provider IDs, and parent links
  across restart. Restored history is inspectable before any provider reconnect.
- **R10 — Honest interrupted recovery.** A Tinto/app-server shutdown must not
  imply that an in-flight child completed. On restore, Tinto reconciles with
  provider truth when available; otherwise it marks the last live state as
  interrupted/stale and requires an explicit supported resume or follow-up.
- **R11 — Inherited runtime boundaries.** Child display and control honor Codex
  inheritance for sandbox/permission mode, live runtime overrides, model,
  reasoning effort, tools, skills, and custom agents. Approval requests surface
  with the originating child identity and never borrow approval from another
  thread.
- **R12 — Bounded resource use.** Tinto respects Codex concurrency limits,
  handles capacity rejection explicitly, bounds retained activity and payload
  sizes using existing redaction/normalization rules, and does not add a second
  unbounded execution queue.

### Configuration and compatibility

- **R13 — Built-in and custom agents.** Tinto reflects built-in and project/user
  custom Codex agent roles exposed by the runtime, including resolved model and
  reasoning metadata when available. Tinto does not become a custom-agent TOML
  editor in this initiative.
- **R14 — Existing session compatibility.** Ordinary non-agentic Codex sessions,
  archived conversations, checkpoints, worktree forks, WSL sessions, PTY
  fallback, Claude, Kimi, and OpenCode retain current behavior.
- **R15 — Minimal harness seam.** The shared contract describes only evidenced
  concepts needed by the Tinto experience—thread relationship, lifecycle,
  capability, activity, result, and control outcome. Codex is the only adapter
  implemented now; no speculative provider framework, dependency, daemon, or
  orchestration service is introduced.
- **R16 — Contract and consumer parity.** Rust authority, generated TypeScript
  contracts, frontend stores, docked/detached consumers, and persisted journal
  agree on the same additive semantics and remain forward-compatible.

## Acceptance examples

- **AE1:** A primary Codex thread spawns two children and one grandchild; Agent
  Lens shows the correct nested tree and live statuses without flattening.
- **AE2:** Opening a running child shows its own transcript and activity while
  the parent conversation remains intact; returning restores prior focus.
- **AE3:** A direct follow-up reaches the selected child. Interrupt affects only
  that child's active turn. Close removes it from Active while preserving its
  inspectable history.
- **AE4:** The parent waits for several children and its final response includes
  their collected results; Tinto displays both child results and the parent
  consolidation without duplicating raw output into the parent transcript.
- **AE5:** A child approval request identifies its source thread. Rejection or
  failure stays associated with that child and is visible to the parent flow.
- **AE6:** After restarting Tinto, the complete tree and every child transcript
  reappear. A child that was running at shutdown is not shown as completed unless
  provider reconciliation proves completion.
- **AE7:** A nested/custom agent with model or reasoning overrides shows resolved
  metadata when Codex supplies it; omitted values inherit without Tinto guessing.
- **AE8:** Capacity exhaustion, unknown future event fields, a missing child
  thread, and app-server fallback each produce bounded truthful states without
  crashing or corrupting the main session.
- **AE9:** Existing single-agent, WSL, PTY fallback, checkpoint, archive/resume,
  fork/worktree, and non-Codex regression suites remain green.

## Settled decisions

- Complete Codex agentic behavior is the acceptance baseline; individual
  behaviors are not re-scoped one by one.
- Tinto preserves its IADE visual language rather than reproducing Codex UI.
- Complete hierarchy and child transcripts persist across restart.
- Agent Lens exposes direct child follow-up, interrupt, and close controls.
- Codex remains orchestration authority; Tinto observes, persists, and controls.
- Future harness support is enabled by a minimal neutral domain seam, not by a
  speculative multi-provider orchestration framework.

## Scope boundaries

### Included

- Codex app-server protocol projection, lifecycle/control handling, explicit
  agent hierarchy, journal restoration, bus contracts, Agent Lens UI, direct
  controls, accessibility, and focused/aggregate verification.
- Additive compatibility migration for existing journal rows and session data.
- Documentation and durable orchestration evidence required by this autonomous
  delivery run.

### Deferred

- Implementing subagents for Claude, Kimi, OpenCode, ACP, or generic PTY.
- A custom-agent authoring/editor experience.
- Remote/cloud agents, cross-device trees, autonomous release, or Jira seeding.

### Non-goals

- Reimplementing Codex scheduling, result synthesis, model routing, or approval
  policy.
- Pixel-level Codex UI parity.
- New services, daemons, databases, dependencies, generic plugin frameworks, or
  broad redesign of Agent Lens.

## Invariants and escalation boundaries

- Monitoring remains passive; only explicit user or agent-runtime actions
  mutate sessions or repositories.
- Provider events are untrusted, bounded, normalized, and rendered inertly.
- Existing checkpoint and repo-diff authority stays in Tinto.
- A change that requires a new provider, dependency, destructive migration,
  credential ownership, automatic execution recovery, or weakened permission
  isolation is blocked/deferred rather than inferred.
- Workers never commit, push, open PRs, mutate Jira, request reviewers, merge,
  or perform release actions.

## Success criteria

- Every installed Codex agentic lifecycle/control item needed for the documented
  experience is represented, persisted, and covered by executable tests.
- Nested trees, direct controls, approvals, inheritance, restart restoration,
  capacity/unknown-event failure modes, and parent result consolidation have
  automated evidence.
- Existing Codex and non-Codex session behavior remains compatible.
- Independent review finds no unresolved P0-P2 correctness, contract,
  accessibility, security, or persistence issue.
- Root aggregate contract, format, lint, frontend, Rust, build, and supported
  native checks pass or carry an explicit bounded environment gap.
- The final output is one guarded Release Marshal plan; no shipping mutation
  occurs before later user approval.

## Sources

- Official OpenAI Subagents documentation, consulted 2026-08-28.
- Installed `codex-cli 0.150.0-alpha.12.2` experimental app-server schema.
- `src-tauri/src/agent_console/app_server.rs`
- `src-tauri/src/agent_console/session.rs`
- `src-tauri/src/agent_console/journal.rs`
- `src/bus/contract.ts`
- `src/agent/sessionStore.ts`
- `src/panels/terminal/TerminalPanel.tsx`
- `docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md`
- `docs/plans/2026-06-30-016-codex-app-server-runtime-plan.md`
