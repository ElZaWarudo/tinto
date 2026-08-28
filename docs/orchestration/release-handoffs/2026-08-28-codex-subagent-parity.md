# Codex subagent parity — guarded release plan

Status: authorized for direct-main release

Prepared: 2026-08-28

Mode: guarded/manual approval

Current branch: `codex/agents-usability-edge-campaign`

Selected base: `main` (`origin/develop` is absent and GitHub reports `main` as the default branch)

The user superseded the guarded PR phase after reviewing this packet and explicitly authorized committing the approved scope and pushing it directly to `main` with no PR. The authorized remote operation is `git push origin HEAD:main`. Jira mutations, reviewer requests, PR creation, and merge operations remain excluded.

The PR-specific proposal below is preserved as the reviewed historical plan; its push and PR operations are superseded by the direct-main authorization above.

## Release scope

- Project the Codex app-server subagent graph into Tinto with arbitrary-depth ancestry, lifecycle, capability, activity, prompt preview, transcript, result, and source identity.
- Persist and restore the complete graph while retaining legacy session compatibility and bounded caches/capacity.
- Route child-local notifications and requests safely, including inherited runtime/approval/permission context and sanitization before emission or persistence.
- Expose targeted follow-up, steer, interrupt, wait, inspect, and parent-mediated close controls without treating close as archive/delete.
- Add the accessible Agent Lens Agents view with hierarchy, focus/collapse state, per-agent drafts, transcripts/results, truthful capability gates, and Tinto-native styling.
- Include the approved requirements, immutable worker contracts, review certificates, aggregate verification, blocker resolution, autonomy ledger, and authoritative reconciliation.

## Proposed commit sequence

1. `docs(agents): define Codex subagent parity`
   - `docs/plans/codex-subagent-parity/`
   - `docs/product/roadmap.md`
   - `docs/jira/seed-plan.md`
   - `docs/swarm/swarm-startup.md`
2. `feat(agent-runtime): project Codex subagent hierarchy`
   - `scripts/generate-bus-contract.mjs`
   - `src-tauri/src/agent_console/`
   - `src-tauri/src/bus/contract.rs`
   - `src-tauri/src/lib.rs`
   - `src/bus/client.ts`
   - `src/bus/contract.generated.ts`
   - `src/bus/contract.ts`
3. `feat(agent-lens): add subagent tree and controls`
   - `src/App.css`
   - `src/agent/`
   - `src/panels/terminal/AgentTreeLens.tsx`
   - `src/panels/terminal/AgentTreeLens.test.tsx`
   - `src/panels/terminal/TerminalPanel.tsx`
   - `src/panels/terminal/TerminalPanel.test.tsx`
4. `docs(orchestration): record Codex subagent delivery evidence`
   - `docs/orchestration/`
   - `docs/swarm/queue-state.yaml`
   - `docs/swarm/blockers.yaml`

The user-owned untracked `output/` directory is explicitly excluded from every commit and from the PR.

## PR grouping

One consolidated PR is proposed because the generated bus contract, backend graph projection, and Agent Lens controls form one compatibility boundary and the UI depends on the runtime contract.

The scope checker reports an oversized human-authored diff and requires explicit approval. Its raw count is inflated by the excluded, user-owned `output/` tree, but the owned implementation itself is still deliberately substantial. Approval of this plan therefore includes approval of this single oversized, coherently grouped PR.

Proposed title:

`feat(agents): add Codex subagent hierarchy and controls`

Proposed body (validated by `format_pr_body.py` and `check_pr_body.py`):

- Projects and persists the full Codex subagent hierarchy, lifecycle, capabilities, activity, transcripts, results, and inherited execution context.
- Adds targeted follow-up, steer, interrupt, wait, inspect, and parent-mediated close controls over the Codex app-server protocol.
- Adds an accessible Agent Lens tree with per-agent transcripts, results, drafts, state restoration, and capability-gated controls in Tinto's existing visual language.

No Jira URL is included because no Jira provider, project, or issue is configured for this repository.

## Exact guarded release operations

After approval, Release Marshal will first revalidate that `origin/main`, the current branch, the working tree scope, and GitHub PR state are unchanged. It will then:

1. Create the four local commits above, excluding `output/`.
2. Re-run the verification gates listed below against the committed tree.
3. Run `check_pr_scope.py --base origin/main...HEAD --fail-on-blocking`; the approved oversized-PR decision is valid only for the scope in this packet.
4. Push the currently absent remote branch with exactly:

   `git push -u origin codex/agents-usability-edge-campaign`

5. Create one ready-for-review PR with base `main`, head `codex/agents-usability-edge-campaign`, the exact title above, and the validated body artifact at `docs/orchestration/runs/codex-subagent-parity-2026-08-28/proposed-pr-body.md`.
6. Inspect recent merged PR approvals and propose a human reviewer; no reviewer request is authorized until the resolved reviewer is shown or separately approved.
7. Perform no merge. Merge requires a later, PR-specific eligibility inspection and explicit authorization.

If `origin/main`, the remote branch, refspec, push mode, scope, or existing-PR state changes, Release Marshal must stop and present a revised plan instead of reusing this approval.

## Verification evidence

- Implementation fingerprint: `sha256:6e4e9f0b81d3e51319154e63049830a7dbcc8fd3df4123568dfd3507da8d8830`.
- Independent backend review: passed with no findings at final diff digest `sha256:4ccd39d6dd27d88bdf1b578cca452671262c010a3dfce197bb36b035bf1cedf1`.
- Independent UI review: passed at final diff digest `sha256:3cec16443b2a901fd9c44b713ad56e3a3eae1df282e78d8f8c3bbca6ab7cdbe9`.
- `npm run contract:check`: passed.
- Scoped Prettier and ESLint over every owned frontend/generated/script file: passed.
- Agent-tree and selector tests: 11 passed.
- Focused TerminalPanel generic-progress regression: passed.
- `npm run build`: passed.
- Rust formatting and clippy with warnings denied: passed.
- Full Rust suite: 469 passed.
- Final `cargo build --manifest-path src-tauri/Cargo.toml`: passed.
- `git diff --check`: passed after documentation whitespace cleanup.

Canonical evidence: `docs/orchestration/runs/codex-subagent-parity-2026-08-28/aggregate-verification.json` and `docs/orchestration/runs/codex-subagent-parity-2026-08-28/authoritative-reconciliation.json`.

## Risks and deferred items

- A live end-to-end run against a production Codex app-server was not available; protocol compatibility is covered by real-shape fixtures, contract generation checks, and Rust integration tests.
- Repository-wide Prettier and ESLint traverse unreadable pre-existing `.pumarejo` artifacts. Every owned file passed scoped checks; repository tooling cleanup is deferred outside this story.
- The root frontend suite reports 761 passing tests and one pre-existing, unowned WSL-absence assertion against `ConsoleDockPanel.tsx`. That baseline issue is deferred outside this story.
- Generated or stale nested worktree discovery remains repository-maintenance work and is not included in the PR.
- The user-owned `output/` directory remains untouched and excluded.

## Jira disposition

No Jira mutation is proposed or authorized because `jira.provider`, `project_key`, and issue mappings are unset. `docs/jira/seed-plan.md` records the future Spanish issue seed; configuring a provider and creating/linking/transiting an issue remains a separate manual decision.

## Approval boundary

Approval authorizes the four proposed commits, the exact normal push command, and creation of the single oversized PR only while all revalidated facts remain unchanged. It does not authorize a reviewer request, Jira mutation, PR comment, force-push, or merge.
