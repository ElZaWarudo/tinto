# Delivery: what the coordinator and the user do that Tinto could do in code

Date: 2026-10-30. Research only: no code was changed.

## Sources and scope

- **Delivery store:** `~/.config/tinto/delivery.sqlite`, opened read-only. Only the two real runs are counted:
  - `run-11e6d688`: DPP, 16 tasks.
  - `run-04f38e9c`: oasis, 8 tasks.
  - The history runs from 2026-10-27 12:54 to 2026-10-30 09:44 UTC. Sandbox runs under `/tmp` are left out.
  - In those two runs: 320 jobs, 124 approvals, 61 decisions, 35 rounds, about 1,700 events.
- **Coordinator transcripts:** two Claude Code sessions from 2026-10-30, 08:16–09:45 UTC, under `~/.claude/projects/-home-teb-Documentos-air-institute-digital-product-passport/`:
  - `0c0efa38-….jsonl`: DPP coordinator, 86 assistant turns.
  - `14d330a4-….jsonl`: oasis coordinator, 192 assistant turns.

  The Oct 27–29 coordinator sessions were deleted (only `*.desktop-released.json` stubs remain), so the transcript numbers cover about 1.5 hours. The store covers the whole history.
- **Sources with nothing to use:**
  - `agent-journal.sqlite` only holds sandbox sessions.
  - No Codex coordinator ran on these repos.
  - The repos' `.agent/` folders hold issues and context, not transcripts.
- **Code:** read at the `develop` working tree as of today, with other sessions' uncommitted edits included. Line numbers are approximate where those edits touch the file.

## Headline numbers

- **The user approved all 124 steps as asked.**
  - Commit, push, PR, merge and Jira steps: 0 `step_text_edited` events and 0 rejections.
  - The only text the user rewrote was PR replies: 20 `pr_reply_edited` events on 2 replies.
- **Most coordinator turns are bookkeeping.** In the oasis transcript, 64 of 192 turns did nothing but request approvals, wait or record steps. Those turns read 19.8M cached tokens, 44% of the session's cache reads, about 310k tokens per turn.
- **wait_run did not wait.** 26 of 36 calls returned `waited:false` because events were already pending. The median block was 0 s.
- **The coordinator requested steps Tinto had already identified.** Of 37 idle-task phases returned to it:
  - 19 were `pr`, 7 `unblocked`, 5 `next_stage` and 4 `record jira`.
  - 2 named the step itself (`next_step commit` and `next_step push`).
- **The coordinator ran the Jira steps by hand.** Neither repo has Jira set in Tinto (`repo_settings.jira = null` for both), so the coordinator ran all 14 Jira steps itself (`approval_completed jira: …` events). In the transcripts that took 19 Bash calls to the krt-jira-scribe REST scripts.
- **The coordinator ran checks in its own shell.** The oasis coordinator ran tests and builds itself 19 times (docker mongo, `npm ci`, mocha, vitest, eslint, `cargo test`/`clippy`/`fmt`), against the server's own rule.
- **Most blocking decisions went to the recommended option.** 27 of 36 blocking decisions were answered with it, 8 with another option, and 1 was a permission. The 26 non-blocking ones were recorded by the coordinator itself.

---

## Quick wins

These are config changes, or code changes of under a day each.

### Q1. Set Jira in Tinto for DPP and oasis (config), then have Tinto request the Jira steps

**Today:**
- `repo_settings.jira` is null for both repos, so `request_approval` (`coordination.rs` ~1076) records no `jira_statuses`. On approval, `tinto_runs_jira` is false (~1470), and the step goes back to the coordinator as `record` (phase.rs ~987).
- The coordinator then:
  1. writes a title and body;
  2. waits for approval;
  3. runs GET status, GET transitions, POST transition and POST remotelink through the krt-jira-scribe scripts;
  4. reports with `complete_approval`.
- The texts are templates:
  - "DPP-203: enlazar el PR #199 y pasar a En Revisión"
  - "DPP-203 a Terminado"
  - outcomes such as "X movida de En ejecución a Cerrado (transición Hecho, id 31); PR #N ya enlazado". All 7 after-merge steps say this.

**Frequency:**
- 14 Jira steps in the store (DPP 5, oasis 9), all run by the coordinator.
- In the transcripts: 7 after-merge steps took 19 Bash calls plus about 14 tinto calls.
- User wait on the DPP Jira approvals: 19 min (DPP's run is not unattended).

**Deterministic replacement:**
1. **Config, now.** In Ajustes → Jira for both repos, set the host (`jira.air-institute.com`, which `.krt/env/jira-scribe.env` already has for DPP) and the token. Set `after_pr = ["En ejecución"]` and `after_merge = ["Cerrado"]`.
   - The workflow's review status is called "En ejecución". Its "En Revisión" transition lands there, as every outcome in the store reports.
   - `jira::run_step` (jira.rs:92) then does the link and the transitions, and is idempotent through the PR's `globalId`.
2. **Code.** Tinto requests the step itself, with a generated title:
   - `after_pr` once the PR step is executed;
   - `after_merge` once the merge is executed or adopted (`check_pr_steps`, prs.rs ~2145).

   The package logic (link-only, close on the last package; coordination.rs ~1036–1071) is already in Tinto, and so is approval through `step_allowed`.
   - Where: a small `request_jira_step(task, stage)` in `coordination.rs`, called from the code that executes or adopts the PR and merge steps.
   - Leave the coordinator's `request_approval rung=jira` for the cases settings do not cover.

**Risk, and what stays with people:**
- The user still approves when `autonomy.jira` is off.
- Free-form Jira comments stay with the coordinator, for example DPP-218's "comment listing the follow-ups". So does `qa_jira_comment`: jira.rs has no comment call.
- Reading the token from the krt env file automatically would move a secret without the user's consent. Keep entering it as a user action in Settings, or at most offer it pre-filled for one confirmation.

**Savings:**
- About 3 coordinator turns and 2–4 Bash calls per Jira step. Over this history that is about 40 turns, or about 12M cached tokens at the measured 310k per turn.
- The DPP user waits go away too, if Q2 is also done.

### Q2. Let DPP's batch run commit, push and Jira on its own (config)

**Today:** `run-11e6d688` is not unattended, so `step_allowed` (coordination.rs ~1348) never applies. Every DPP commit, push and Jira step waited for a click:

| Step | Count | User wait |
|---|---|---|
| commit | 9 | 70 min |
| push | 14 | 151 min |
| jira | 5 | 19 min |

None was rejected or edited. oasis became unattended at 2026-10-30 09:19 (`run_policy` seq 1568). After that its pushes and commits were approved in under 2 s.

**Deterministic replacement:** mark the DPP batch unattended, as oasis is. The repo's autonomy defaults already allow commit, sync, push and Jira, and keep PR, merge and pr_reply with the user.

**Risk:** commit messages go out unread. The evidence (0 edits in 124 steps) says the review added nothing here. PR text, merge and replies, where the user does read and edit, stay manual.

**Savings:** about 240 min of wall time on DPP's critical path, with no change to agent tokens.

### Q3. Use the repo's merge method and a generated merge title

**Today:**
- `merge_pr` uses `approval.merge_method.unwrap_or("squash")` (coordination.rs ~2904), even though `merge_method()` (~1782, `tasks::base_merge_method`) already reads the base branch's history.
- The coordinator chose squash for all 2 DPP merges and rebase for all 6 oasis merges by hand. It copied the PR title as the merge title, sometimes with a `[skip ci]` body.

**Replacement:**
- Default to `self.merge_method(&task.id)` when the approval has none.
- Prefill the merge title as `<PR title> (#N)` for squash. Rebase needs no title.

**Risk:** none beyond today. The user still approves merges.

**Savings:** small: 8 merges, about 1 turn each of the coordinator looking up the convention. The fix is a one-line change.

### Q4. Pick a default agent for `dispatch_job` when the stage has none

**Today:** `mcp.rs` ~479 refuses with "falta agent: el usuario no fijó en Tinto un agente para esta etapa". The DPP coordinator hit it on 5 explore dispatches in a row (`0c0efa38` L263–L272) and resent all 5 with `"agent":"claude"` (L296–L304).

**Replacement:** fall back to the agent of the repo's implementation stage, or the batch's coordinator agent, and say so in the job's `substituted` field, as Windows substitution already does.

**Risk:** a stage running on an unintended agent. The `substituted` note covers it.

**Savings:** 5 wasted calls and 1 turn per batch.

### Q5. Re-run a task's checks through Tinto

**Today:**
- The MCP surface has no tool to re-run a task's checks on demand. Checks are queued only after a stage that changed code or after a sync (service.rs ~3596, coordination.rs ~1584).
- When the coordinator wanted to confirm a fix or a sync, it ran the suites in its own shell. The oasis coordinator did this 19 times (`14d330a4` L306–L363, L426–L541), including its own Mongo container and `npm ci`, and once committed with `git commit` outside Tinto (L573).

**Replacement:** a `run_checks(task_id, commands?)` tool, or a `checks` value of `set_task_state` that re-queues them. It goes through `queue_task_checks` and closes a failing-checks round when they pass (`settle_round`, rounds.rs ~396).

**Risk:** none. It is the path Tinto already uses.

**Savings:**
- About 19 Bash turns (about 6M cached tokens) in 1.5 hours of oasis coordination.
- Results are recorded where the user can see them.
- Fewer hand-made environments: see L3.

### Q6. Queue a step until its precondition holds, instead of refusing it

**Today:** in 1.5 hours the oasis coordinator got 14 `request_approval` refusals for preconditions Tinto itself checks:
- `base_moved` ×2
- `pr_not_ready` ×3 (checks still running)
- `job_running` ×2
- `merge_in_progress`
- `nothing_to_commit`
- `single_commit_unjustified`
- `nothing_to_push`
- `base_merged`

Each refusal cost a turn and a retry, and often an extra `wait_run`.

**Replacement:** for the transient ones (`job_running`, `pr_not_ready` while checks or CI are running, `merge_in_progress`), keep the approval pending with a `waits_for` note. Tinto runs it when the condition clears, through the same watch loop that settles blockers (blockers.rs ~467). Permanent errors stay as they are: `nothing_to_commit` and `single_commit_unjustified` mean the request was wrong.

**Risk:** a queued step that runs later than the user expects. Show it in the panel as "espera a …".

**Savings:** about 10 calls and 5–8 turns per busy hour.

---

## Larger changes, ranked by value

### L1. Tinto advances the publishing ladder itself

This covers push, sync and restack, Jira and the round's commit and push.

**Today:**
- `phase_in_round` already computes `NextStep { rung }` (phase.rs ~1273, ~1288) and, for drifted pushes, `Drifted` (~1008).
- Tinto initiates only two steps: `auto_sync` on conflicts or after another merge (coordination.rs ~1860–1903), and the commit that concludes a merge (`conclude_merge`, ~2152).
- Everything else is the coordinator echoing the phase back:
  - The MCP instructions say "When a push drifts … ask for the push step again" (mcp.rs ~177).
  - The `package_retargeted` event says "pide el paso sync, que la rebasa sobre main, y después el push" (prs.rs ~1989).
- The texts are templates, and the push title is never used by `run_push`:
  - "Publish DPP-222 with main merged in"
  - "Republicar … (igual que origin)"
  - "Merge delivery/dpp-219-2 into delivery/dpp-220"

**Frequency:**
- In the store, 40 pushes, 24 syncs and 5 `package_retargeted` restacks were requested by the coordinator, all with mechanical titles. The 5 "Republicar …" pushes followed drift.
- Between 09:39 and 09:45, the oasis transcript has about 26 `request_approval` calls in 6 minutes. Four tasks (DPP-220, DPP-222, DPP-223, SIM-FIXTURES) each went through sync → (commit) → push → merge → jira after `origin/main` moved.
- The base package's fix cascaded twice into 3 dependent branches, as 3 syncs plus 3 pushes each time.

**Replacement:** when a task's phase is `NextStep` for **push, sync or restack, or Jira**, and the run is unattended with the rung allowed, Tinto opens the approval itself with a generated title. The `step_allowed` path then executes it.

Triggers:
- **Push:**
  - after an executed commit when the branch is published, or when the next step is the PR;
  - after a sync that changed the tree;
  - after a drift when the local tip is ahead.
- **Restack:** after `package_retargeted` with `restack_from`. `auto_sync` already builds the title.
- **Sync of dependents:** when a package's base branch gets a new push.
- **Jira:** see Q1.

Where:
- a single `advance_ladder(task)` in `coordination.rs`, called after `save_approval` of an executed step and from the PR and blocker watch passes;
- `idle_tasks` stops listing those tasks.

**What stays with the agent or the user:**
- Commit plans (message text and grouping), PR title and body, and replies are authored text.
- PR and merge stay with the user (`autonomy.pr` and `autonomy.merge` are false).
- Attended runs only get the request prefilled. The user still clicks.

**Risk:**
- Pushing at the wrong moment, for example mid-round. Gate it on `round_step` (phase.rs ~843) and on no job running.
- Restack conflicts. These already open a sync round for the agent.

**Savings:** the largest item. About 50 turns and about 15M cached tokens in 1.5 hours of the oasis session, and all 64 `request_approval` calls for push and sync in the store's history. It also cuts the median 11 s per hop of coordinator latency from every step.

### L2. Make `wait_run` coalesce, and do not wake the coordinator for what Tinto handles

**Today:**
- `wait_run` (feed.rs ~189–262, default 50 s) returns as soon as one event is pending: 26 of 36 calls returned `waited:false`, each a full model turn at about 310k cached tokens.
- Many of the events are ones the coordinator only acknowledges: `check_ended`, `approval_approved` for steps approved automatically, `setup_command_ended`, `pr_reply_edited`.

**Replacement:**
- After the first pending event, wait a short settle window (2–5 s) and return everything that arrived.
- Mark the event kinds that need coordinator action, using the same waiter that `phase_in_round` computes (`Waiter::Coordinator`).
- Return early only for those; deliver the rest in the next batch.

Where: `feed.rs` and `mcp.rs` `wait_run`.

**Risk:** a few seconds of extra latency per hop, and an event the coordinator should have seen being marked as informational. Derive the marking from phases, not from a hand-written list.

**Savings:** probably 30–50% of the `wait_run` turns: about 15 turns and about 5M cached tokens per hour of busy coordination. This compounds with L1, which removes the events that trigger most of those turns.

### L3. Tinto brings up the environment instead of the coordinator and the job agents

**Today:**
- The DPP coordinator started MongoDB 8 (`dpp-tinto-mongo`, replica set rs0, port 27117) and Besu by hand (`settings_proposed` seq 354). The checks then carried environment probes as checks: `curl … eth_blockNumber` and `node -e "require('net').connect(27117…)"`.
- Seven check jobs queued on `job_waits_environment`.
- Implementation agents started their own containers (`docker run -d --name dpp235-mongo … mongo:7 --replSet rs0`) and polled their own background test logs with `until grep …; do sleep 10; done` loops of up to 10 min each.
- Implementation jobs spent 73 of 228 wall-minutes on DPP, and 23 of 94 on oasis, running commands. The slowest were these waits and narrower mocha runs.

**Replacement:** a per-repo environment recipe in settings (`environment_up`: compose file or commands, plus the existing `environment_probes`). Tinto runs it when a probe fails before checks, QA or `needs_environment` jobs. Per-task databases already come from `check_env`'s `{task}`.
- Move the DPP probes out of `checks` into `environment_probes` (config).
- Tell agents in the prompt that the environment is up (adapters.rs, beside the full-checks note at ~417).

**Risk:**
- Tinto starting containers on the user's machine. Make the recipe opt-in per repo, proposed through `propose_settings` and approved by the user.
- Port clashes with the user's own containers.

**Savings:** several coordinator turns per batch, plus the job time agents spend standing up and polling services, roughly 10–20 job-minutes per affected implementation stage. QA and checks stop failing or waiting on an environment nobody started.

### L4. Dispatch a round's fix and re-review stages automatically when unattended

**Today:**
- Rounds open on their own (rounds.rs ~472–539) and the round's items already go into the stage's instructions.
- The coordinator still has to dispatch implementation and then review for each round. The phase is `NextStage`, and phase.rs ~649–726 picks the role exactly.

**Frequency:** 35 rounds: 12 failing checks, 15 review findings, 3 sync, 2 PR, 3 QA.

**Replacement:** for unattended runs, Tinto dispatches the `next_stage` of a round with a prompt it builds from the round's `items`, the task's issue, its decisions and `read_dirs`, the same parts `adapters.rs` already assembles.
- Start with failing-checks rounds. Their items are the failing command's output, which is the most mechanical case.
- Review-findings rounds next.

**What stays with the coordinator:**
- Deciding whether a finding is wrong and should be resolved (`resolve_job`).
- Whether a finding needs a user decision.
- The first implementation prompt, explore scoping and QA assignments. Their prompts are task-specific, a median of 900–1,800 characters.

**Risk:** a fix stage launched on a finding the coordinator would have rejected. Mitigations:
- Let the coordinator veto within the settle window from L2.
- Apply it only to checks rounds at first.

**Savings:** about 2 turns per round (70 over this history), plus the coordinator's reading of the round items.

### L5. Have the implementation job propose the commit plan

**Today:**
- The coordinator authors every commit plan: 49 messages, 46 conventional, grouped 1–7 per step.
- To do so it reads the diff, which is part of the Bash git-inspection turns (about 30 in the transcripts).
- Tinto already has `commit_draft` (coordination.rs ~1789, changed files plus recent subjects), `change_area` (~66) and `check_single_commit` (~90).

**Replacement:**
1. Add `commit_plan` to the writing stages' structured result.
2. Validate it with the same rules.
3. Request the commit step from it when the round or the stages settle. With L1, this is part of the ladder.

The coordinator only edits it when it disagrees.

**Risk:**
- The job knows its own change but not other stages' changes. Validate that the plan covers every changed file, a check `request_approval` already makes.
- The user still sees and approves, or autonomy commits it.

**Savings:** about 1–3 turns per commit (21 commits), and the diff tokens the coordinator reads for them.

### L6. QA problem bookkeeping

**Today:** `update_qa_problems` asks the coordinator to set `fixing` when it dispatches a fix, and `verified` when QA confirms it (mcp.rs ~1385). `record_qa_problems` only matches dismissed problems again (qa_problems.rs ~288).

**Replacement:**
- Set `fixing` when an implementation stage is dispatched in a `qa_problems` round.
- Set `verified` or `still_failing` from the next QA record, matched with `same_problem`.

**Frequency:** only 2 `qa_problems_sent` in this history, so the value is low today and grows with browser QA.

### L7. Make `delivery_overview` fit

**Today:** two compact overviews were 123k and 65k characters, past the tool limit. The coordinator read the saved file back with `jq` (`0c0efa38` L129–L182): 4 Bash calls and 5 overview calls.

**Replacement:** when the result would pass the limit, return `phases` and `rounds` only, with a note naming the `fields` and `task_id` to ask for. The option exists today; it should be the default when the result does not fit.

**Savings:** a few turns per takeover.

---

## Already addressed since this history (verify, no new work)

- **Re-authored commits caused push drift.** 12 `commit_rewritten` and 10 `step_drifted` events on 2026-10-29 11:36–15:37: commits re-authored as another email, followed by "Republicar … (igual que origin)" pushes. The author notice (`author_notice`, coordination.rs ~1158) and the adoption of a rewritten remote with the same content (`adopt_push`, 4 `step_adopted` events on Oct 30) landed in 540f003 (2026-10-29 17:12).
- **Agents ran Tinto's full checks inside their jobs.** The full-checks guard (adapters.rs ~417, ~1452; 7edf64f, 2026-10-26) covers this. The oasis run's `autonomy.full_access = true` exempts full-access jobs, though. `cargo test --workspace 2>&1 | grep …` still ran inside oasis implementation jobs, at about 2 min each. Decide whether full access should also skip Tinto's checks.
- **Missing submodules broke checks.** "No such file or directory" broke 6 oasis cargo checks after the `tools/pumarejo` submodule arrived, and the coordinator had to diagnose it. Tinto now checks out submodules before each job and after a sync (`submodules_note`, coordination.rs ~3176).
- **Checks killed by an app restart.** 5 `npm run unit-tests` failures read "Tinto se reinició mientras corría". Checks and setup are now always retried (service.rs ~631–650).

## What should stay with the agent or the user

- **Decisions.** 27 of 36 blocking decisions took the recommendation, but 8 did not: about one in four or five rejected the recommended option. `accept_recommended` and `settle_unattended` already cover the unattended case. Do not answer them automatically in attended runs.
- **PR title and body.** Users waited a median of 10–16 min on PR steps (90 and 450 min in total) and did not edit them. The PR is the review point, so leave it manual and make the text cheaper with L5.
- **PR replies.** These are the only texts the user rewrote (20 edits on 2 replies).
- **Merge.** Reviewers' approvals and CI gate it (`pr_not_ready` for a missing review). It stays with the user.
- **Job prompts for first implementation, explore and QA.** These are task-specific (median 900–1,800 characters, using the plan and the decisions).

## Summary table

| # | Change | Kind | Evidence (count) | Savings |
|---|---|---|---|---|
| L1 | Tinto requests push, sync, restack and Jira itself | code, medium | 40 pushes, 24 syncs, 5 restacks, about 26 requests in 6 min | about 50 turns and 15M cached tokens per busy 1.5 h |
| Q1 | Jira set in Tinto, after_pr and after_merge requested by Tinto | config + small code | 14 Jira steps by hand, 19 Bash | about 40 turns and 12M tokens over the history |
| Q2 | DPP batch unattended | config | 28 approvals, 0 edits | about 240 min of user wait |
| L2 | wait_run coalesces, only wakes for coordinator work | code, medium | 26 of 36 returns with `waited:false` | about 15 turns and 5M tokens per hour |
| Q5 | `run_checks` tool | small code | 19 suites in the coordinator's shell | about 19 turns and 6M tokens |
| Q6 | Queue transient refusals | small code | 14 refusals | 5–8 turns per hour |
| L4 | Auto-dispatch round stages (checks first) | code, medium | 35 rounds | about 70 turns |
| L3 | Environment recipe run by Tinto | code, large | Mongo and Besu by hand, 7 environment waits, agent containers | job-minutes plus turns |
| L5 | Commit plan from the job result | code, medium | 21 commits, 49 messages | 1–3 turns per commit |
| Q4 | Default agent for dispatch | small code | 5 refusals | 1 turn per batch |
| Q3 | Repo merge method and merge title | one line | 8 merges | small |
| L7 | Overview fits by default | small code | 2 oversized results | a few turns per takeover |
| L6 | QA problem statuses from records | small code | 2 rounds | low today |

The token figures use the oasis transcript's average of about 310k cached tokens read per bookkeeping turn. They come from only 1.5 hours of transcripts, so treat them as orders of magnitude. The counts from the Delivery store cover the whole history.

## Implementation status (2026-10-30, uncommitted)

| # | Status | Where |
|---|---|---|
| Q1 | Code done: Tinto requests the Jira steps itself (L1). **Waiting on the user** to set Jira in each repo's settings. | `coordination.rs` `advance_ladders` |
| Q2 | Nothing to do: both batches are unattended now. | — |
| Q3 | Done. Without a method, the repo's latest merge through Tinto picks it, else the base's history. Without a title, a squash or merge uses the PR's approved title plus ` (#N)`. | `coordination.rs` `merge_method`, `request_approval` |
| Q4 | Done. A stage with no agent set runs on the implementation stage's agent. | `mcp.rs` `dispatch_job` |
| Q5 | Done: new `run_checks` tool. | `service.rs` `run_checks`, `mcp.rs` |
| Q6 | Deferred. L1 removes the coordinator's own push, sync and Jira requests in unattended runs, which caused most of the refusals. What remains (a merge asked while CI runs) needs a new waiting state for approvals in the model and the panel. | — |
| L1 | Done. Past the brainstorm, Tinto requests push, restack, Jira (with Tinto's Jira) and commit (with L5). This runs from the blocker watch after every change. A drifted push stays with the coordinator. | `coordination.rs` `advance_ladders`, `blockers.rs` |
| L2 | Done. `wait_run` waits for 1.5 s of quiet (at most 8 s) after a change before it returns. | `feed.rs` `settle_changes` |
| L3 | Proposed, not built: an opt-in per-repo "start the environment" recipe. | — |
| L4 | Done. Past the brainstorm, the first two failing-checks rounds of a task get their implementation stage from Tinto. | `coordination.rs` `fix_failing_checks` |
| L5 | Done. Writing jobs return a `commit_plan`. Past the brainstorm, Tinto requests the commit from those plans when every writing stage since the last commit gave one. | `adapters.rs`, `model.rs`, `coordination.rs` `proposed_commit_plan` |
| L6 | Done. The next QA record marks problems sent to fix as `still_failing` or `verified`. | `qa_problems.rs` |
| L7 | Done. A compact overview larger than 40,000 characters, asked without `fields`, keeps only phases, rounds and queued, plus a note. | `mcp.rs` `fit_overview` |
