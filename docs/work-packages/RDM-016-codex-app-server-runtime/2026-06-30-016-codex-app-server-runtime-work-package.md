---
title: Codex app-server runtime
status: review-passed
roadmap_item: RDM-016
origin_roadmap: docs/roadmaps/2026-06-30-007-codex-app-server-runtime-roadmap.md
origin_brainstorm: docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md
origin_planning_input: docs/brainstorms/2026-06-30-016-codex-app-server-runtime.md
origin_plan: docs/plans/2026-06-30-016-codex-app-server-runtime-plan.md
units: [U1, U2, U3, U4, U5]
unit_alignment: complete
review_units: [RU1]
base_branch: develop
pr_strategy: independent
max_open_stack: n/a
jira_policy: optional
production_posture: prototype
autonomy: guarded
autonomous_ledger: none
allowed_mutation_classes: []
---

# Codex App Server Runtime

## Scope
Implement a Codex app-server-backed agent runtime for Tinto sessions. The runtime should start Codex turns, stream output, consume app-server turn/change notifications, and close Agent Lens checkpoints through structured Codex lifecycle events.

## Implementation Evidence
- RU1 implementation completed and reviewed locally on 2026-06-30.
- Backend:
  - Added `src-tauri/src/agent_console/app_server.rs` with a Codex app-server stdio process adapter.
  - Local Codex sessions prefer app-server through `PortablePtyFactory`; app-server launch failure falls back to the existing PTY runtime.
  - The adapter initializes app-server, starts an ephemeral thread with repo `cwd`, subscribes to `fs/watch`, converts line input into `turn/start`, streams assistant/command deltas as session output, and maps `turn/completed` / `fs/changed` / diff/file-change notifications into structured process events.
  - `AgentProcess` now supports drainable structured events so Codex checkpoint closure no longer relies on an injected terminal marker. The marker remains only for PTY/fallback agents.
- Contract/docs:
  - Updated `docs/contracts/bus-contract.md` to document app-server-preferred local Codex sessions and PTY fallback.
- Local app-server smoke:
  - Node smoke against installed Codex app-server proved `thread/start` and `fs/watch` responses for a temp repo.
- Focused verification passed:
  - `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1` (55/55)
  - `cargo test --manifest-path src-tauri/Cargo.toml agent_console::app_server -- --test-threads=1` (5/5)
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1` (47/47)
  - `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run` (44/44)
  - `npm run build` passed with the existing chunk-size warning
  - `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed
  - `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` passed
  - `git diff --check` passed with existing CRLF warnings only

## IADE Continuation Handoff
- User-requested handoff/push note (2026-07-01): the current thread should refresh Compound Master files for another agent, then commit and push all current changes with local merge handling. This does not prove the full "all remaining work packages" objective is complete. Resume from the pushed `develop` state after this handoff, preserve unrelated existing changes, and continue with the exact next recommended slice below.
- User-facing goal to continue: build the Tinto Agents experience as a native IADE surface, not as boring terminal tabs and not as a literal Codex clone. Take creative liberties as long as the UI feels like Tinto and keeps runtime pieces replaceable for OpenCode/Claude later.
- Current worktree status: dirty, intentionally uncommitted. Do not revert unrelated existing changes. Do not commit, push, merge, or create PRs unless explicitly requested.
- Current implemented IADE direction:
  - `TerminalPanel` renders a product agent interface with header, agent logo, session status, loading session status-strip title, session status-strip facet titles, Agent activity fact titles, Agent session overview section titles, Agent session overview metric titles, Agent session overview metrics container titles, Agent session overview latest-activity titles, Agent session overview turn-map chip titles, Agent session overview turn-map container titles, Agent conversation container titles, Agent transcript tools container titles, Transcript secondary actions group titles, transcript search container titles, transcript search count status titles, active transcript search-result position status titles, transcript search navigation label titles, transcript search visible label titles, transcript search input placeholder titles, transcript secondary-action label titles, transcript clear-search label titles, focused-turn action label titles, focused-turn action container titles, focused-turn utility label titles, focused-turn utility container titles, focused-turn card container titles, focused-turn facts container titles, focused-turn command-summary container titles, focused-turn artifact-summary container titles, conversation turn card container titles, conversation turn header container titles, conversation turn title container titles, conversation turn metadata container titles, conversation message block container titles, conversation message content container titles, collapsed command block container titles, collapsed command summary row titles, conversation turn touched-files container titles, conversation turn command-summary container titles, conversation turn artifact-summary container titles, conversation turn copy label titles, message block copy label titles, conversation turn index label titles, focused-turn heading label titles, focused-turn selected index label titles, focused-turn idle state label titles, focused-turn idle helper text titles, focused-turn selected fallback text titles, focused-turn hidden file overflow titles, focused-turn file row titles, focused-turn files container titles, conversation turns, message role label titles, message header container titles, Markdown messages, technical command blocks with collapsed command output label titles, transcript search/copy/latest actions with status-aware titles, previous/next and keyboard search-result navigation with active result position, accessible status hints, and disabled-state navigation titles, responsive secondary transcript actions, clear-search no-results recovery with status title, search-match chip groups with explanatory titles, Escape-to-clear search reset, focused-result reset on search clear, focus return after clear-search recovery, Escape shortcut discovery, quick actions, focused-turn rail, Agent Lens, and composer.
  - Agent Lens includes Files/Commands/Timeline tabs with titles that explain each view, tab label/count titles that distinguish tab names from per-tab counts, tablist titles that summarize available views and counts, a Focus/Session scope control with titles that explain focused-turn versus full-session scope, header scope-label titles that explain `Turn N` / `N turns`, heading label titles that distinguish the static `Agent Lens` label from the titled inspector container, heading container titles that frame the active focused/session inspector, metric label titles that explain turn state and active file scope, metric value titles that distinguish live turn-state/file-count values from metric labels, header container titles that explain scope controls and metrics, touched-file filtering with filter/count titles that explain visible results, file filter label titles that distinguish static filter label text from the titled filter controls, file filter-wrapper titles that distinguish filter controls from filtered-result counts, file empty-state and preview placeholder titles that explain why the file pane is empty or what the preview area represents, selected-file preview placeholder titles that distinguish no-file/no-hunk placeholders from hunk preview content, command/timeline event and empty-state titles that explain captured output and timeline rows, command/timeline event metadata and captured-text titles that distinguish event timing/type from captured output text, list container titles that explain scoped pane contents, live-context container titles that explain repo status/diff chips, file group container titles that explain grouped touched-file categories, file-group header titles that distinguish artifact headings from group counts, file-group kind label titles that distinguish artifact labels from group headings/counts, file action-group titles that explain Preview/Open/Ask/Revert controls act on a touched file, file action label titles that distinguish visible action labels from status-aware button titles, individual file action titles that explain preview/open/ask/revert state for each touched file, preview container titles that explain selected-file summary and hunk detail grouping, focused-turn file/command/timeline inspection, artifact grouping by code/test/doc/config/other, compact per-turn artifact summaries, file-row titles that explain focused/session touched-file scope, file-row metadata-span titles that distinguish row timing/scope, path, and change type, file group count titles that explain artifact group totals, status/diff context chip titles that explain repo status versus live diff summaries, selected-file preview titles that explain hunk count, diff totals, and first-hunk range, a selected-file hunk micro-preview from existing bus diff data, file-tab open actions, artifact/hunk-aware file-specific follow-up prompt drafting, lightweight per-file live status/diff context from the bus, read-only session change-log fallback, per-file checkpoint revert where available, and timeline/command summaries.
  - The side rail includes a `Focused turn` card; its idle/selected card containers, heading, selected `Turn N` label, idle `Idle` / `No turn selected` labels, empty-state helper text, selected `No text captured.` fallback, hidden-file overflow labels, visible file rows, file-list container, facts container, command-summary container, artifact-summary container, action container, and utility container explain their short text, grouped contents, or whole rail state through compact titles; selecting a turn from the map highlights the matching conversation card, `Jump` scrolls to it and now explains that scroll behavior through a title, `Copy focus` copies the turn transcript with artifact context and explains focused-context copy through status-aware titles, while conversation `Copy turn` explains full-turn transcript copy. Individual message/command block `Copy` buttons now explain which turn's block is copied through status-aware title text. The action pad drafts contextual `Continue`, `Review`, `Test`, or `Handoff` prompts with the same code/test/doc/config summary into the composer and exposes titles that clarify these controls draft prompts instead of sending work immediately. The focused card, conversation turn cards, turn map, and transcript search now use compact recent-command context when available; focused-turn and conversation-card timestamps explain relative turn timing through title text; focused-turn and conversation-card summary lines explain transcript, command, and file counts through title text; conversation-card touched-file meta labels explain the per-turn touched-file total through title text, and individual touched-file chips explain the file path/change kind and turn index; focused-turn latest-activity text explains selected-turn latest captured activity; focused-turn command/file fact chips explain that they count selected-turn commands and files; focused and conversation recent-command chips explain through title text that they summarize recent command output; focused and conversation artifact summary groups explain through title text that `Code` / `Tests` / `Docs` / `Config` chips summarize touched artifact categories, and each individual artifact chip explains its category count; filtered conversation turns show compact match-cause chips for message, command, or file matches; and `Test` / `Handoff` prompts include that same command summary.
  - `ConsoleDockPanel` has quick launch/recent/journal navigation so agent sessions can be reopened as UI sessions instead of only active terminal tabs.
  - Backend/session plumbing includes Codex app-server events, timeline items, turn checkpoints, session change logs, and SQLite journal support.
- Latest local verification after the collapsed command summary row title slice:
  - Work-package checker passed.
  - Direct code review passed with no remaining P0-P2 findings. One P2 compatibility regression was found and fixed before closeout: focused Lens scope initially hid session-only change-log files for PTY/fallback sessions with transcript output but no `turn_checkpoints`.
  - Follow-up direct review passed after adding file actions, file action-group titles, file action label titles, individual file action titles, preview container titles, lightweight per-file status/diff context, artifact grouping, Agent Lens tab titles, Agent Lens tab label/count titles, Agent Lens tablist container titles, Agent Lens scope titles, Agent Lens header scope-label titles, Agent Lens heading container titles, Agent Lens heading/filter label titles, Agent Lens metric label titles, Agent Lens metric value titles, Agent Lens header container titles, Agent Lens file filter/count titles, Agent Lens file filter-wrapper titles, Agent Lens file empty/preview titles, Agent Lens selected-file preview placeholder titles, Agent Lens command/timeline event and empty-state titles, Agent Lens command/timeline event metadata and captured-text titles, Agent Lens list container titles, Agent Lens live-context container titles, Agent Lens file group container titles, Agent Lens file-group header titles, Agent Lens file-group kind label titles, Agent Lens file-row titles, Agent Lens file-row metadata-span titles, Agent Lens file group count titles, Agent Lens file status/diff context titles, Agent Lens selected-file preview titles, selected-file hunk micro-preview, artifact/hunk-aware `Ask` prompts, compact per-turn artifact summaries, artifact-aware copied focus/handoff text, loading session status-strip title, session status-strip facet titles, Agent activity fact titles, Agent session overview section titles, Agent session overview metric titles, Agent session overview metrics container titles, Agent session overview latest-activity titles, Agent session overview turn-map chip titles, Agent session overview turn-map container titles, Agent conversation container titles, Agent transcript tools container titles, Transcript secondary actions group titles, transcript search container titles, transcript search count status titles, active transcript search-result position status titles, transcript search navigation label titles, transcript search visible label titles, transcript search input placeholder titles, transcript secondary-action label titles, transcript clear-search label titles, focused-turn action label titles, focused-turn action container titles, focused-turn utility label titles, focused-turn utility container titles, focused-turn card container titles, focused-turn facts container titles, focused-turn command-summary container titles, focused-turn artifact-summary container titles, conversation turn card container titles, conversation turn header container titles, conversation turn title container titles, conversation turn metadata container titles, conversation message block container titles, conversation message content container titles, collapsed command block container titles, collapsed command summary row titles, conversation turn touched-files container titles, conversation turn command-summary container titles, conversation turn artifact-summary container titles, conversation turn copy label titles, message block copy label titles, conversation turn index label titles, focused-turn heading label titles, focused-turn selected index label titles, focused-turn idle state label titles, focused-turn idle helper text titles, focused-turn selected fallback text titles, focused-turn hidden file overflow titles, focused-turn file row titles, focused-turn files container titles, collapsed command output label titles, message role label titles, message header container titles, focused-turn Copy focus and conversation Copy turn status titles, per-message/per-command copy titles, focused-turn action-pad prompt-draft titles, focused-turn Jump scroll title, focused-turn and conversation-card timestamp titles, focused-turn and conversation-card summary-line titles, conversation-card touched-file meta and chip titles, focused-turn latest-activity title, focused-turn fact chip titles, focused-turn and conversation recent-command chip titles, focused-turn and conversation artifact-summary group titles, focused-turn and conversation artifact-summary chip titles, recent-command context for focused `Test`/`Handoff` prompts, visible focused-turn/conversation-card command summaries, turn-map command hints, transcript search matching for compact command summaries, search-match cause chips, search-match chip group titles, previous/next transcript search-result navigation, Enter/Shift+Enter keyboard navigation from the search input, active search-result position, accessible search status hints, disabled-state navigation titles, Copy visible status titles, Latest status titles, responsive transcript toolbar secondary-action grouping, clear-search no-results recovery with status title, Escape-to-clear search reset, focused-result reset on search clear, focus return after clear-search recovery, and Escape shortcut discovery. No remaining P0-P2 findings.
  - `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 35/35 after the collapsed command summary row title continuation.
  - `npx tsc --noEmit` passed.
  - `npm test -- src\panels\terminal\ConsoleDockPanel.test.tsx src\panels\terminal\TerminalPanel.test.tsx src\bus\contract.test.ts --run` passed 68/68 after the collapsed command summary row title continuation.
  - `npm run build` passed with the existing Vite dynamic-import and chunk-size warnings after the collapsed command summary row title continuation.
  - `git diff --check` passed with existing CRLF warnings only after the collapsed command summary row title continuation.
- Exact next recommended slice: inspect the worktree first, then add a compact title to the focused-turn title container (`agent-panel__turn-focus-title`) so hover users can distinguish the selected Turn label and transcript summary group from the focused-turn heading, timing, latest activity, facts, command/artifact summaries, file list, action pad, and utilities, without changing focus selection, prompt drafting, clipboard payloads, backend, session, or bus contracts.
- Verification expected for the next slice: targeted Vitest for the changed agent UI behavior, `npx tsc --noEmit`, the short terminal/contract suite, `npm run build`, and `git diff --check`.

## Non-goals
- Removing the existing PTY runtime.
- Implementing OpenCode or Claude native adapters.
- Committing, PR generation, or Jira mutation from turn data.
- Full chat redesign outside the current Agents surface.
- Generated Codex app-server schema artifacts committed to the repo.

## Autonomy Contract
- Mode: guarded.
- Agent may decide without asking: internal Rust module names, tolerant parser helper shape, exact test fixture messages, and equivalent focused verification commands.
- Agent must record as assumptions: Codex app-server is experimental, Windows local repos are first-class for this package, and WSL can remain fallback unless proven safe.
- Agent must escalate: removing PTY fallback, changing destructive revert semantics, changing public branch/PR/Jira workflow, introducing external credentials, or broad UI redesign.
- Safe fallback: keep Codex app-server as an optional runtime path and preserve terminal-backed Codex sessions if app-server launch fails.
- Autonomous ledger: none.
- Allowed external mutation classes: none.

## Dependencies
- Requires: RDM-015 Agent turn checkpoints.
- Blocks: future native OpenCode/Claude adapters and richer IADE turns view.

## Production Posture
- Posture: prototype.
- Evidence: existing Compound Master state identifies Tinto as prototype and current work is local desktop behavior.
- Confidence: medium.
- Consequences for this package: preserve existing user-visible agent sessions and keep app-server behavior additive/fallback-capable.
- Breaking existing behavior allowed: no.

## Plan Unit Alignment
| Plan unit | Included in this package | Reason |
|---|---|---|
| U1 | yes | Runtime boundary is required before app-server can be replaceable. |
| U2 | yes | Codex app-server transport is the core capability. |
| U3 | yes | Turn/checkpoint integration is the main product reason for app-server. |
| U4 | yes | User needs chat with Codex, not only background event capture. |
| U5 | yes | Contract and runtime changes need focused tests/docs. |

Grouping rationale:
- The units are tightly coupled for review: chat start, app-server transport, event mapping, and checkpoint closure must be verified together to prove the Codex runtime works. Splitting would create a stack where early PRs cannot demonstrate user value independently.

## Implementation Units
- U1. Runtime boundary and session model.
- U2. Codex app-server transport.
- U3. Event mapping and checkpoints.
- U4. Frontend chat/input wiring.
- U5. Tests and documentation.

## Review Units
| Review unit | Scope | Expected changed surfaces | PR base | Jira issue/subtask | Size/risk note |
|---|---|---|---|---|---|
| RU1 | Codex app-server chat and checkpoint runtime | backend runtime/process/event mapping, additive contracts, existing Agents UI wiring, tests/docs | develop | optional new Tarea | High integration risk; keep schema parsing narrow and preserve PTY fallback. |

## Reviewability Diagnosis
- Reviewer-experience check: yes. One PR can be understood as a single capability slice: Codex sessions use app-server events for chat and turn checkpoints while other agents keep fallback behavior.
- Granularity chosen because: the surfaces are coupled by runtime lifecycle and cannot be independently verified without test-only scaffolding.
- Open-stack plan: independent PR; no stack.
- Jira mapping: standalone `Tarea` if Jira is used.
- Downstream-fix trace: none.
- Failure-mode check: this is not a deep micro-PR stack and not a deferred mega-consolidation PR.

## Files and Tests
- Expected files: `src-tauri/src/agent_console/*`, `src-tauri/src/bus/contract.rs`, `src-tauri/src/lib.rs`, `src/bus/contract.ts`, `src/bus/client.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `docs/contracts/bus-contract.md`.
- Expected tests: Rust `agent_console` tests for app-server parsing/event mapping; bus contract tests; terminal/session store tests where frontend contract changes are visible.

## Impact Scan
- Changed API contracts/endpoints/bindings/helpers/schemas/payloads/auth/tenant/ownership/test fixtures: additive session runtime metadata and/or app-server chat input command if required.
- Consumer scan patterns: `AgentSession`, `start_agent_session`, `write_agent_session_input`, `turn_checkpoints`, `TerminalPanel`, `codex`.
- Consumers found: `src-tauri/src/agent_console/*`, `src-tauri/src/lib.rs`, `src/bus/client.ts`, `src/agent/sessionStore.ts`, `src/panels/terminal/TerminalPanel.tsx`, `src/panels/terminal/ConsoleDockPanel.tsx`, `src/panels/DashboardPanel.tsx`, `src/bus/contract.test.ts`, `src/panels/terminal/TerminalPanel.test.tsx`, `docs/contracts/bus-contract.md`.
- Contract-drift tests searched: `src/bus/contract.test.ts`, Rust `agent_console` tests, terminal panel tests.
- Required consumer tests: backend agent_console tests, TS contract tests, and terminal panel tests.
- Consumer tests run/skipped: `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1` passed 55/55; `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1` passed 47/47; `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run` passed 44/44.

## Verification Gate
- `cargo test --manifest-path src-tauri/Cargo.toml agent_console -- --test-threads=1`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib bus -- --test-threads=1`
- `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx --run`
- `npm run build`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `git diff --check`
- Surface-aware evidence: include a local app-server smoke or unit-equivalent evidence for `fs/watch`/turn notification parsing.
- Production posture evidence: prototype posture does not relax compatibility for existing PTY sessions.

## Review Gate
- Code review threshold: P0-P2.
- Findings below threshold: log unless user marks blocking.

## Security Gate
- Run after work-review loop: not automatically required unless implementation introduces network listeners, credential handling, destructive behavior beyond existing revert, or public API exposure.
- Security Watch during work: enabled for app-server process launch, local transport, and fallback behavior.
- Security Watch notes: prefer stdio transport; do not expose unauthenticated WebSocket listeners; avoid printing tokens or app-server auth details.
- Security reviewer: inline fallback unless high-risk surfaces appear.
- Security review result: inline review passed; no P0-P2 findings remain.
- Required security verification: app-server launch uses stdio only; unknown JSON-RPC notifications are ignored; no WebSocket listener or token-bearing transport is introduced.

## CI Break-Prevention And Escalation
- CI risk surfaces: Rust process management, async/thread handling, frontend type contracts, terminal UI tests.
- Preventive evidence: focused Rust/frontend tests plus build before release handoff.
- If CI breaks: invoke krt-ci-questor with run/check context; do not poll checks in Compound Master.
- Escalation rule: record release-follow-up blocker until the CI incident has cause, owner, and next action.

## Branch and PR Handoff Inputs
- Review unit: RU1 Codex app-server chat and checkpoint runtime.
- Branch name: feat/codex-app-server-runtime.
- Branch/docs rule: first executable review unit carries related planning artifacts on the same semantic branch.
- PR base: develop.
- Suggested commit grouping for this review unit:
  - `feat(agent-console): add codex app-server runtime` - backend runtime, event parsing, checkpoint integration, tests.
  - `feat(agents): route codex chat through structured runtime` - frontend contract/UI wiring and tests.
  - `docs(agent-console): document codex app-server runtime` - contract and orchestration artifacts.
- PR title: Use Codex app-server for agent sessions
- PR body bullets:
  - Run Codex sessions through app-server lifecycle and change events.
  - Close Agent Lens turn checkpoints from structured Codex turn completion.
  - Preserve terminal-backed fallback behavior for other agents.
- Verification results location: update this work package and Compound Master state after execution.
- Production/deployment notes: none beyond preserving fallback behavior.
- Autonomous mutation request: none.

## Jira Handoff Inputs
- Jira policy: optional.
- Suggested issue type: Tarea.
- Suggested subtask behavior: standalone `Tarea`.
- PR-to-Jira mapping: single-review-unit PR maps to one standalone `Tarea`.
- Jira summary: Usar Codex app-server para sesiones de agente
- Jira description: Integrar Codex mediante app-server para capturar chat, eventos de turno y cambios estructurados, manteniendo el runtime de terminal como fallback para otros agentes.
- Optional-policy fallback: if Jira role/config/context is missing, record "Jira omitted: jira-env-not-configured" or the actual reason in state/release closeout and continue.
