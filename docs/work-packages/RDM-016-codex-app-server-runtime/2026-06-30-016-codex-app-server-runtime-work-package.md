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
- Current release status: ready PR #13 is open from `codex/agent-lens-readiness` to `develop`: https://github.com/ElZaWarudo/tinto/pull/13. Jira was omitted because checkout readiness reported `jira-env-not-configured`; no merge was performed.
- Visual-quality / turn-restore update (2026-07-01): the latest approved IADE direction is Codex-like but not a Codex clone. Composer commands live in a floating `/` menu, skills live in a floating `$` mention menu, and the focused-turn rail is no longer a prompt/action surface. Treat turns as logbook restore points: the focused-turn card should expose only one primary action, `Restore here`, which restores files and the chat view to the selected completed turn. Do not reintroduce Plan/Review/Test/Handoff, Jump, or Copy focus controls inside the focused-turn panel; keep navigation/copy affordances in transcript-level tooling instead.
- Restore backend/journal update (2026-07-01): `restore_session_turn` is now an additive Tauri command. It uses each turn's post-turn `restore_checkpoint`, blocks restore while the session is running or starting, sets `restored_to_turn_index`, trims the frontend turn list to that index, and persists/reconstructs `restored_to_turn_index` through `agent_sessions.restored_to_turn_index` in the SQLite journal. Future agents should preserve this durability so restored files and restored chat do not diverge after archive/session reconstruction.
- Completion-oriented status reconciliation (2026-07-01): current work-package inventory now has no implementation/release false-pending statuses. GitHub PR state confirms PR #1, #8, #9, #10, #11, and #12 are merged; RDM-001, RDM-007, RDM-008, RDM-009, RDM-010, RDM-011, and RDM-014 frontmatter was reconciled to `completed`. Inventory after reconciliation: `completed` 8, `delivered` 3, `review-passed` 21, `shipped-merged` 1; no package remains in `implemented-verified-awaiting-release`, `pr-opened`, `ci-passed`, `in-progress`, `execution-ready`, `package-review-passed`, `implementation-complete`, `review-fix-needed`, `blocked`, `ci-blocked`, or `security-blocked`.
- CI format gate note (2026-07-01): latest remote `develop` CI run `28497553867` failed Frontend / Check formatting on commit `e2a5f91bad48220f62decaaa1929867ef32e74c1`. Local reproduction found Prettier drift in `src/App.css`, `src/panels/terminal/TerminalPanel.tsx`, and `src/panels/terminal/TerminalPanel.test.tsx`; targeted Prettier fixed those files. Local verification after the fix: `npm run format:check`, `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` (40/40), and `npx tsc --noEmit`. The formatting fix is included in PR #13 for GitHub CI validation.
- Current implemented IADE direction:
  - `TerminalPanel` renders a product agent interface with root agent panel container titles, agent workspace container titles, chat shell container titles, side rail container titles, composer container titles, composer quick-action/button/label titles, composer input-row/input/send titles, error banner titles, empty-chat state container/label/helper/action titles, header lifecycle control/button/label titles, header, header container titles, agent logo container titles, agent identity container titles, agent display-name label titles, agent repo label titles, agent header actions container titles, agent logo, session status, loading session status-strip and loading-label titles, session status-strip facet titles, Agent activity strip/main-status/pulse/status-text/headline/detail/fact titles, state-aware quick-action/composer availability titles for writable, read-only, unavailable, and empty-draft states, Agent session overview section titles, Agent session overview metric titles, Agent session overview metrics container titles, Agent session overview latest-activity titles, Agent session overview turn-map chip titles, Agent session overview turn-map container titles, Agent conversation container titles, Agent transcript tools container titles, Transcript secondary actions group titles, transcript search container titles, transcript search count status titles, active transcript search-result position status titles, transcript search navigation label titles, transcript search visible label titles, transcript search input placeholder titles, transcript secondary-action label titles, transcript clear-search label titles, focused-turn restore label/container titles, focused-turn card container titles, focused-turn facts container titles, focused-turn command-summary container titles, focused-turn artifact-summary container titles, conversation turn card container titles, conversation turn header container titles, conversation turn title container titles, conversation turn metadata container titles, conversation turn touched-files container titles, conversation turn command-summary container titles, conversation turn artifact-summary container titles, conversation turn copy label titles, message block container titles, message content container titles, message block copy label titles, conversation turn index label titles, focused-turn heading label titles, focused-turn selected index label titles, focused-turn idle state label titles, focused-turn idle helper text titles, focused-turn selected fallback text titles, focused-turn hidden file overflow titles, focused-turn file row titles, focused-turn files container titles, conversation turns, message role label titles, message header container titles, Markdown messages, technical command blocks with collapsed command output container, summary-row, and label titles, transcript search/copy/latest actions with status-aware titles, previous/next and keyboard search-result navigation with active result position, accessible status hints, and disabled-state navigation titles, responsive secondary transcript actions, clear-search no-results recovery with status title, search-match chip groups with explanatory titles, Escape-to-clear search reset, focused-result reset on search clear, focus return after clear-search recovery, Escape shortcut discovery, quick actions, focused-turn restore rail, Agent Lens, and composer.
  - Agent Lens includes Files/Commands/Timeline tabs with titles that explain each view, tab label/count titles that distinguish tab names from per-tab counts, tablist titles that summarize available views and counts, stable per-session tab/panel ids, `aria-controls`, `aria-labelledby`, named `tabpanel` content regions, horizontal tablist orientation, roving tab focus, keyboard navigation with arrows/Home/End, a root inspector title that summarizes scope, active view, file/command/timeline counts, and turn state, active Files/Commands/Timeline view container titles including empty-state views, a Focus/Session scope control with titles that explain focused-turn versus full-session scope, header scope-label titles that explain `Turn N` / `N turns`, heading label titles that distinguish the static `Agent Lens` label from the titled inspector container, heading container titles that frame the active focused/session inspector, metric label titles that explain turn state and active file scope, metric value titles that distinguish live turn-state/file-count values from metric labels, header container titles that explain scope controls and metrics, touched-file filtering with filter/count titles that explain visible results, file filter label titles that distinguish static filter label text from the titled filter controls, file filter-wrapper titles that distinguish filter controls from filtered-result counts, live file-filter status linked with `aria-describedby`, query-aware Escape-to-clear file filtering, visible Clear actions for filtered and no-results states, file empty-state and preview placeholder titles that explain why the file pane is empty or what the preview area represents, selected-file preview placeholder titles that distinguish no-file/no-hunk placeholders from hunk preview content, command/timeline event and empty-state titles that explain captured output and timeline rows, command/timeline event metadata and captured-text titles that distinguish event timing/type from captured output text, list container titles that explain scoped pane contents, live-context container titles that explain repo status/diff chips, file group container titles that explain grouped touched-file categories, file-group header titles that distinguish artifact headings from group counts, file-group kind label titles that distinguish artifact labels from group headings/counts, file action-group titles that explain Preview/Open/Ask/Revert controls act on a touched file, file action label titles that distinguish visible action labels from status-aware button titles, individual file action titles that explain preview/open/ask/revert state for each touched file, preview container titles that explain selected-file summary and hunk detail grouping, focused-turn file/command/timeline inspection, artifact grouping by code/test/doc/config/other, compact per-turn artifact summaries, file-row titles that explain focused/session touched-file scope, file-row metadata-span titles that distinguish row timing/scope, path, and change type, file group count titles that explain artifact group totals, status/diff context chip titles that explain repo status versus live diff summaries, selected-file preview titles that explain hunk count, diff totals, and first-hunk range, a selected-file hunk micro-preview from existing bus diff data, file-tab open actions, artifact/hunk-aware file-specific follow-up prompt drafting, lightweight per-file live status/diff context from the bus, read-only session change-log fallback, per-file checkpoint revert where available, and timeline/command summaries.
  - Agent Lens Commands and Timeline now include scoped query filters with live result counts, Escape-to-clear behavior, visible Clear controls, query-specific empty states, and filtered view/list titles for long sessions.
  - Agent Lens Files preview now behaves as a navigable mini-inspector: it shows the current preview position, moves through the filtered file set with Previous/Next controls, supports ArrowLeft/ArrowUp and ArrowRight/ArrowDown when focused, and keeps focused-state styling plus compact navigation titles without changing open/ask/revert semantics.
  - Agent Lens Files preview now also exposes direct selected-file shortcuts for Open, Ask, and checkpoint Revert, using the existing row action handlers while keeping preview action names/titles distinct from row-level actions.
  - The side rail container now exposes a compact title for the focused-turn and Agent Lens column. It includes a `Focused turn` card, but this card is now a restore-point inspector rather than a command/action pad. Its idle/selected card containers, heading, selected `Turn N` label, idle `Idle` / `No turn selected` labels, empty-state helper text, selected `No text captured.` fallback, hidden-file overflow labels, visible file rows, file-list container, facts container, command-summary container, artifact-summary container, and restore container explain their short text, grouped contents, or whole rail state through compact titles. Selecting a turn from the map highlights the matching conversation card. The only focused-turn action is `Restore here`; it returns files and chat view to that completed turn when a post-turn restore checkpoint exists and the session is not running. `Jump`, `Copy focus`, and prompt-drafting actions were intentionally removed from the focused-turn surface so turns no longer feel like an invasive command console. Conversation-level `Copy turn`, individual message/command block `Copy` controls, transcript search, and transcript navigation remain transcript-level tools. The focused card, conversation turn cards, turn map, and transcript search still use compact recent-command context when available; focused-turn and conversation-card timestamps explain relative turn timing through title text; focused-turn and conversation-card summary lines explain transcript, command, and file counts through title text; conversation-card touched-file meta labels explain the per-turn touched-file total through title text, conversation-card touched-file containers explain grouped touched-file chips, and individual touched-file chips explain file path/change kind and turn index; focused-turn latest-activity text explains selected-turn latest captured activity; focused-turn command/file fact chips explain selected-turn command/file counts; focused and conversation recent-command chips summarize recent command output; focused and conversation artifact summary groups explain `Code` / `Tests` / `Docs` / `Config` touched artifact categories; and filtered conversation turns show compact match-cause chips for message, command, or file matches.
  - `ConsoleDockPanel` has quick launch/recent/journal navigation so agent sessions can be reopened as UI sessions instead of only active terminal tabs.
  - Backend/session plumbing includes Codex app-server events, timeline items, turn checkpoints, session change logs, and SQLite journal support.
- Latest local verification after the Agent Lens command/timeline filter batch:
  - Work-package checker passed.
  - Agent Lens command/timeline filter batch added scoped filters, live count status, Escape-to-clear recovery, visible Clear actions, query-specific no-results states, filtered panel/list titles, and focused coverage for Commands and Timeline. No runtime, backend, session, bus, prompt text shape, file action semantics, search outside Agent Lens, copy, revert semantics, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - Agent Lens preview action shortcuts batch added direct selected-file Open, Ask, and checkpoint Revert controls to the preview panel, plus focused CSS layout and tests proving preview Open/Ask reuse the existing workspace/prompt flows. No runtime, backend, session, bus, prompt text shape, file action semantics, search, copy, revert semantics, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - Agent Lens preview navigation batch added position text, Previous/Next controls, focused preview keyboard navigation, preview navigation titles, and CSS for stable preview focus/navigation layout. No runtime, backend, session, bus, file action semantics, prompt, search, copy, revert, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - Direct code review passed with no remaining P0-P2 findings. One P2 compatibility regression was found and fixed before closeout: focused Lens scope initially hid session-only change-log files for PTY/fallback sessions with transcript output but no `turn_checkpoints`.
  - Follow-up direct review passed after adding file actions, file action-group titles, file action label titles, individual file action titles, preview container titles, lightweight per-file status/diff context, artifact grouping, Agent Lens tab titles, Agent Lens tab label/count titles, Agent Lens tablist container titles, Agent Lens scope titles, Agent Lens header scope-label titles, Agent Lens heading container titles, Agent Lens heading/filter label titles, Agent Lens metric label titles, Agent Lens metric value titles, Agent Lens header container titles, Agent Lens file filter/count titles, Agent Lens file filter-wrapper titles, Agent Lens file empty/preview titles, Agent Lens selected-file preview placeholder titles, Agent Lens command/timeline event and empty-state titles, Agent Lens command/timeline event metadata and captured-text titles, Agent Lens list container titles, Agent Lens live-context container titles, Agent Lens file group container titles, Agent Lens file-group header titles, Agent Lens file-group kind label titles, Agent Lens file-row titles, Agent Lens file-row metadata-span titles, Agent Lens file group count titles, Agent Lens file status/diff context titles, Agent Lens selected-file preview titles, selected-file hunk micro-preview, artifact/hunk-aware `Ask` prompts, compact per-turn artifact summaries, artifact-aware copied focus/handoff text, loading session status-strip title, session status-strip facet titles, Agent activity fact titles, Agent session overview section titles, Agent session overview metric titles, Agent session overview metrics container titles, Agent session overview latest-activity titles, Agent session overview turn-map chip titles, Agent session overview turn-map container titles, Agent conversation container titles, Agent transcript tools container titles, Transcript secondary actions group titles, transcript search container titles, transcript search count status titles, active transcript search-result position status titles, transcript search navigation label titles, transcript search visible label titles, transcript search input placeholder titles, transcript secondary-action label titles, transcript clear-search label titles, focused-turn action label titles, focused-turn action container titles, focused-turn utility label titles, focused-turn utility container titles, focused-turn card container titles, focused-turn facts container titles, focused-turn command-summary container titles, focused-turn artifact-summary container titles, conversation turn card container titles, conversation turn header container titles, conversation turn title container titles, conversation turn metadata container titles, conversation turn touched-files container titles, conversation turn command-summary container titles, conversation turn artifact-summary container titles, conversation turn copy label titles, message block container titles, message content container titles, message block copy label titles, conversation turn index label titles, focused-turn heading label titles, focused-turn selected index label titles, focused-turn idle state label titles, focused-turn idle helper text titles, focused-turn selected fallback text titles, focused-turn hidden file overflow titles, focused-turn file row titles, focused-turn files container titles, collapsed command output container, summary-row, and label titles, root agent panel container titles, agent workspace container titles, chat shell container titles, side rail container titles, composer container titles, composer quick-action/button/label titles, composer input-row/input/send titles, error banner titles, empty-chat state container/label/helper/action titles, header lifecycle control/button/label titles, main agent header container titles, agent logo container titles, agent identity container titles, agent display-name label titles, agent repo label titles, agent header actions container titles, message role label titles, message header container titles, focused-turn Copy focus and conversation Copy turn status titles, per-message/per-command copy titles, focused-turn action-pad prompt-draft titles, focused-turn Jump scroll title, focused-turn and conversation-card timestamp titles, focused-turn and conversation-card summary-line titles, conversation-card touched-file meta and chip titles, focused-turn latest-activity title, focused-turn fact chip titles, focused-turn and conversation recent-command chip titles, focused-turn and conversation artifact-summary group titles, focused-turn and conversation artifact-summary chip titles, recent-command context for focused `Test`/`Handoff` prompts, visible focused-turn/conversation-card command summaries, turn-map command hints, transcript search matching for compact command summaries, search-match cause chips, search-match chip group titles, previous/next transcript search-result navigation, Enter/Shift+Enter keyboard navigation from the search input, active search-result position, accessible search status hints, disabled-state navigation titles, Copy visible status titles, Latest status titles, responsive transcript toolbar secondary-action grouping, clear-search no-results recovery with status title, Escape-to-clear search reset, focused-result reset on search clear, focus return after clear-search recovery, and Escape shortcut discovery. No remaining P0-P2 findings.
  - Work-package inventory audit found the apparent RUL-001 `in-review` package was a stale 2026-06-22 artifact. It is superseded by `docs/work-packages/RUL-001-file-overview-ruler-parity/2026-06-23-003-file-overview-ruler-parity-work-package.md`, where RU1/RU2/RU3 are implemented, verified, and review-passed; the stale artifact was reconciled to avoid false remaining-work signals.
  - Release-readiness audit after RUL reconciliation found no active implementation package still waiting for code. Remaining non-final implementation-adjacent statuses are release/shipping states: RDM-001, RDM-007, RDM-009, RDM-010, and RDM-011 are `implemented-verified-awaiting-release`; RDM-008 is `pr-opened` at PR #9 with clean gates but no GitHub-visible human approval for merge; and RDM-014 is `ci-passed` with GitHub Actions run `28250263625` passing on commit `1f28c5d66911dc3024e6dec3f9cd34fca924499d`. No runtime, backend, session, bus, Jira, branch, commit, push, or PR mutation was performed by this audit.
  - Agent Lens root/view orientation batch added a titled inspector root summarizing scope, active tab, counts, and turn state; added titled active-view containers for Files, Commands, and Timeline, including empty command/timeline states; and added a lightweight `agent-panel__lens-view` wrapper for consistent view spacing. No runtime, backend, session, bus, file action, prompt, search, copy, revert, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - Agent Lens tab semantics and keyboard navigation batch added stable per-session tab/panel ids, `aria-controls`, named `tabpanel` regions linked with `aria-labelledby`, horizontal tablist orientation, roving tab focus, and ArrowLeft/ArrowRight/ArrowUp/ArrowDown/Home/End keyboard navigation. No runtime, backend, session, bus, file action, prompt, search, copy, revert, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - Agent Lens file-filter recovery batch added `aria-describedby` live filter status, query-aware filter titles, Escape-to-clear behavior with focus restoration, visible Clear controls for filtered and no-results states, and a query-specific no-results recovery message/action. No runtime, backend, session, bus, file action, prompt, search, copy, revert, Jira, branch, commit, push, or PR mutation was performed by this batch.
  - `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 37/37 after the prompt-action availability title batch.
  - `npx tsc --noEmit` passed after correcting the helper call to pass `session ?? null`.
  - `npx tsc --noEmit` initially caught the helper accepting an undefined repo and a nonexistent `AgentType` type; the helper was corrected to accept `string`/optional repo, then `npx tsc --noEmit` passed.
  - `npm test -- src\panels\terminal\ConsoleDockPanel.test.tsx src\panels\terminal\TerminalPanel.test.tsx src\bus\contract.test.ts --run` passed 70/70 after the prompt-action availability title batch.
  - `npm run build` passed with the existing Vite dynamic-import and chunk-size warnings after the prompt-action availability title batch.
  - Latest Agent Lens root/view orientation verification: `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 37/37; `npx tsc --noEmit` passed; `npm test -- src\panels\terminal\ConsoleDockPanel.test.tsx src\panels\terminal\TerminalPanel.test.tsx src\bus\contract.test.ts --run` passed 70/70; `npm run build` passed with the existing Vite dynamic-import and chunk-size warnings.
  - Latest Agent Lens tab semantics and keyboard navigation verification: `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 38/38; `npx tsc --noEmit` passed. The short terminal/contract suite was intentionally not re-run for this batch to honor the user's request to avoid excessive checks after focused coverage and typecheck passed.
  - Latest Agent Lens file-filter recovery verification: `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 39/39 after updating one stale no-results expectation for the query-aware title; `npx tsc --noEmit` passed. The short terminal/contract suite and build were intentionally not re-run for this UI-only batch to honor the user's request to avoid excessive checks after focused coverage and typecheck passed.
  - Latest Agent Lens preview navigation verification: RDM-016 work-package checker passed before artifact edits; `npx tsc --noEmit` passed; `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 40/40 after updating one stale preview-title expectation for the new position text. The short terminal/contract suite and build were intentionally not re-run for this UI-only batch to honor the user's request to avoid excessive checks after focused coverage and typecheck passed.
  - Latest Agent Lens preview action shortcuts verification: RDM-016 work-package checker passed before artifact edits; `npx tsc --noEmit` passed; `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 40/40 after differentiating the preview Revert title from the row Revert title. The short terminal/contract suite and build were intentionally not re-run for this UI-only batch to honor the user's request to avoid excessive checks after focused coverage and typecheck passed.
  - Latest Agent Lens command/timeline filter verification: RDM-016 work-package checker passed before artifact edits; `npx tsc --noEmit` passed; `npm test -- src\panels\terminal\TerminalPanel.test.tsx --run` passed 40/40 after disambiguating the intentional duplicate Clear controls in the test query. The short terminal/contract suite and build were intentionally not re-run for this UI-only batch to honor the user's request to avoid excessive checks after focused coverage and typecheck passed.
  - Latest visual-quality / turn-restore verification: Warden/Inquisitor loop completed and final Inquisitor verdict approved with only optional P3 polish. The first critique found P2 issues for extra focused-turn utility buttons and non-durable chat restore state; both were fixed. Verification passed: `npx tsc --noEmit`; `npm test -- src/bus/contract.test.ts src/panels/terminal/TerminalPanel.test.tsx` (62/62); `cargo check`; `cargo test agent_console::journal::tests::journal_reconstructs_restored_turn_index --lib`; `cargo test agent_console::session::tests::changed_turn_closes_after_output_and_filesystem_quiet --lib`; `cargo test bus::contract --lib`; and `git diff --check` with CRLF warnings only. Render smoke against Vite at `1440x900` and `390x844` found no dashboard horizontal overflow, but pure-browser visual proof could not show a live Agent session and emitted existing Tauri callback warnings.
  - `git diff --check` passed with existing CRLF warnings only after the prompt-action availability title batch.
- Exact next recommended batch: if the user wants shipping mutation, hand the audited release set to `krt-release-marshal`; otherwise continue reviewed package-owned local implementation work. For the IADE Agents track, prioritize meaningful orientation/operability or the optional P3 cleanup from the Inquisitor: rename invisible "focused turn actions/tools" wording to restore-point language and capture a real completed/restorable session at desktop and mobile widths. Do not re-expand focused turns into command pads.
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
