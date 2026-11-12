# Delivery coordinator — acceptance criteria

Date: 2026-11-07. Applies to `develop` at `bef0288` plus the changes in this branch.

The coordinator is the agent conversation (Claude Code, Codex…) that runs a
Delivery batch over the `tinto-delivery` MCP server
(`src-tauri/src/delivery/mcp.rs`). Tinto owns the state, the queue, the checks
and every publishing step; the coordinator decides what to dispatch and talks
to the user. "Works" means both halves hold: Tinto never lets the coordinator
do what is the user's, and the coordinator never has to do what Tinto can.

## How to verify

1. **Automated** (every criterion marked with a test): from `src-tauri`,
   `cargo test --lib delivery::` and, from the repo root,
   `npx vitest run src/panels/delivery`. All must pass on Windows and Linux.
2. **Live** (criteria marked *live*): the scenario at the end, run once per
   release that touches `delivery/`, with its measured numbers written into
   the release notes.

A criterion is met only when its proof passes. A criterion without a proof is
listed under [Gaps](#gaps) until it gets one.

## A. Connection and safety

| ID | Criterion | Proof |
|---|---|---|
| A1 | The server listens on 127.0.0.1 only and answers only `POST /mcp` with the right bearer token: 401 without or with a wrong token, 404 elsewhere, 405 for other methods, 413 above 4 MiB. A refusal reaches the client (no connection reset). | `the_coordinator_endpoint_answers_only_authorized_mcp_posts` |
| A2 | `initialize` returns the server name and the coordinator rules; every tool call returns an object in `structuredContent` and the same value as compact JSON in its text. | `every_tool_result_is_an_object_for_mcp_clients`, `the_coordinator_api_speaks_mcp_and_fences_writes` |
| A3 | The coordinator cannot approve a step, answer a permission, or give a job full access; those stay the user's. | `a_coordinator_accepts_the_recommended_decisions_but_never_a_permission`, `a_coordinator_runs_qa_with_full_access_only_when_the_user_allowed_it`, `approvals_climb_one_rung_at_a_time_and_tinto_runs_commit_and_push` |
| A4 | A step the coordinator reports as done must say what the user approved (a PR must carry the approved text). | `a_pr_reported_by_the_coordinator_must_say_what_was_approved` |
| A5 | Commands the coordinator proposes cannot run files agents can edit, nor paths outside the repo. | `a_permission_cannot_run_files_agents_can_edit`, `a_coordinator_proposes_settings_and_the_next_job_runs_what_the_user_approved` |

## B. Ownership of a batch

| ID | Criterion | Proof |
|---|---|---|
| B1 | Every write carries the run's owner and generation; a coordinator that was replaced is refused. | `a_replaced_coordinator_is_fenced`, `the_coordinator_api_speaks_mcp_and_fences_writes` |
| B2 | The user can hand a batch to another conversation from Tinto; the new one gets one handover message, the old one is told. | `a_coordinator_the_user_asks_takes_the_batch_and_the_old_one_is_told`, `the_handover_names_every_batch_and_one_wait_for_all`, UI: *starts a coordinator here and hands it the batch once it waits* |
| B3 | One conversation can hold several batches; one `wait_run` covers them all and each batch is fenced on its own. | `one_wait_run_covers_every_batch_its_conversation_coordinates`, `a_wait_on_several_batches_is_fenced_per_batch` |
| B4 | Blockers and pending work survive a takeover; the new coordinator gets them in `pending`. | `blockers_survive_a_takeover_and_the_user_can_override_them`, `the_overview_says_what_a_coordinator_that_is_not_waiting_has_not_read` |

## C. Waiting and waking

| ID | Criterion | Proof |
|---|---|---|
| C1 | `wait_run` returns as soon as anything needs the coordinator, including a user decision with no job running, and not at the timeout. | `wait_run_wakes_on_the_users_decision_even_with_no_job_running` |
| C2 | Changes that follow each other within moments come in one reply. | same test, "changes moments apart come in one reply" |
| C3 | The coordinator is not told about its own actions, nor about other batches it does not hold. | `wait_run_leaves_out_what_the_coordinator_asked_and_other_batches`, `the_coordinator_is_not_told_of_its_own_task_edits_but_is_of_the_users` |
| C4 | Nothing is lost between calls: passing the cursor acknowledges up to it; an interrupted coordinator gets unacknowledged changes again, marked `redelivered`. | `changes_handed_to_an_interrupted_coordinator_come_again` |
| C5 | A coordinator whose turn ended is written to in its Agents conversation, once, with what it has not seen and the cursor to read from; one inside `wait_run` is not. | `the_coordinators_conversation_is_woken_once_with_what_it_has_not_seen`, `a_coordinator_inside_wait_run_is_not_woken`, `a_conversation_with_several_batches_is_woken_once_for_all_of_them` |
| C6 | Waiting never leaves the batch idle: while a task can run and there is capacity, `wait_run`/`wait_job` return at once naming the idle tasks. | `waiting_on_one_issue_never_leaves_the_rest_of_the_batch_idle`, `waiting_on_a_batch_returns_the_first_job_that_ends` |
| C7 | A task whose blocker cleared waits on the coordinator (phase `unblocked`) until it acts. | `a_blocker_the_user_clears_waits_on_the_coordinator_until_it_acts`, `a_blocker_that_resolves_waits_on_the_coordinator_until_it_dispatches` |

## D. Jobs and capacity

| ID | Criterion | Proof |
|---|---|---|
| D1 | Capacity and one-job-per-task are respected; a queued job says why it waits. | `capacity_and_one_job_per_task_are_respected`, `capacity_is_automatic_fixed_up_to_eight_or_unlimited`, `a_queued_job_says_what_holds_it` |
| D2 | A dispatch without an agent uses the stage's, else the implementation stage's. | `the_coordinator_api_speaks_mcp_and_fences_writes` ("a stage left unset runs on the implementation's agent") |
| D3 | The coordinator never runs checks itself: `run_checks` queues the task's checks once, on its code as it is. | `run_checks_queues_the_task_checks_once_on_its_code_as_it_is` |
| D4 | A Tinto restart loses no job: running ones are adopted and end the normal way, ones that died are interrupted and retried when safe, and the coordinator is told. | `a_job_still_running_after_a_restart_is_adopted_and_ends_the_normal_way`, `a_check_adopted_after_a_restart_ends_with_the_code_it_left_or_runs_again`, `retry_on_restart_queues_again_once_the_jobs_safe_to_repeat_and_tells_the_coordinator` |
| D5 | Usage limits hold jobs: from 95 % of an account's window no new job of that agent starts until it renews, and a job its limit stopped runs again later. | `queue::tests::an_agent_near_its_limit_starts_no_new_job_until_it_renews`, `a_job_its_usage_limit_stopped_runs_again_later` |

## E. Decisions

| ID | Criterion | Proof |
|---|---|---|
| E1 | Pending blocking decisions hold their task's stages; answers reach the next jobs. | `pending_decisions_block_stages_and_answers_reach_the_jobs`, `a_choice_answered_in_the_users_own_words_reaches_the_jobs` |
| E2 | Invalid decisions are refused whole, naming which. | `decisions_are_validated`, `a_batch_with_an_invalid_decision_says_which_and_creates_none` |
| E3 | A question from the user holds the decision until the coordinator replies. | `nothing_answers_a_decision_the_user_asked_about_until_the_coordinator_replies` |
| E4 | Past the brainstorm, decisions settle as asked (never permissions). | `after_the_brainstorm_decisions_settle_as_they_are_asked` |

## F. Publishing ladder

| ID | Criterion | Proof |
|---|---|---|
| F1 | Steps climb one rung at a time (commit → push → PR → merge → Jira); Tinto runs commit and push itself on approval. | `approvals_climb_one_rung_at_a_time_and_tinto_runs_commit_and_push` |
| F2 | Past the brainstorm, Tinto asks the commit, push, restack and Jira steps itself; the coordinator does not ask them again. | `past_the_brainstorm_tinto_asks_the_push_and_jira_steps_itself`, `past_the_brainstorm_the_commit_plan_the_stages_proposed_is_committed` |
| F3 | A merge needs green CI on the PR head and every chosen reviewer's approval; it uses the repo's merge method. | `the_merge_needs_green_ci_on_the_head_and_a_base_that_has_not_moved`, `a_pr_merges_only_once_every_chosen_reviewer_approved_or_was_removed`, `the_merge_method_proposed_follows_how_the_base_merges_its_prs` |
| F4 | A step that drifted from git/GitHub is reported and asked again; a branch rewritten outside Tinto with the same content is adopted. | `a_push_drifts_when_the_remote_branch_is_gone_or_moved_and_is_asked_again`, `a_branch_rewritten_outside_tinto_is_adopted_when_the_worktree_has_it` |

## G. Rounds (work sent back)

| ID | Criterion | Proof |
|---|---|---|
| G1 | Every send-back (findings, QA problems, failing checks, PR changes, conflicts) opens one round naming cause and items; it closes once its work is committed. | `each_send_back_opens_a_round_that_closes_once_its_work_is_committed`, `changes_asked_on_the_pr_open_a_round_that_repeats_commit_and_push`, `a_sync_with_conflicts_opens_a_round_once` |
| G2 | Past the brainstorm, the first two failing-checks rounds get their fix stage from Tinto, on the model of the stage's last attempt (the stage's setting over it), and none when no model is fixed anywhere. | `past_the_brainstorm_a_failing_checks_round_gets_its_fix_stage_from_tinto` |
| G3 | Rounds survive a restart. | `rounds_survive_a_restart` |

## H. Context budget

| ID | Criterion | Proof |
|---|---|---|
| H1 | The overview the coordinator reads fits one tool result: compact by default, and past 40,000 characters it keeps phases, rounds and the queue with a note. The limit is measured in the same compact JSON the client receives. | `the_coordinator_reads_a_compact_overview_and_jobs_without_their_instructions`, `an_overview_too_large_to_read_keeps_where_each_task_is`, `every_tool_result_is_an_object_for_mcp_clients` |
| H2 | Jobs come without their instructions unless asked (`full`). | `the_coordinator_reads_a_compact_overview_and_jobs_without_their_instructions` |
| H3 | The rules sent on `initialize` are grouped by situation (loop, ownership, user, packages, Tinto-only work, rounds, what Tinto does alone, checks, push/sync/merge) and contain every rule of the previous single-paragraph text. | `the_coordinator_endpoint_answers_only_authorized_mcp_posts` (rules present); wording kept verbatim at the time of the change |

## I. In Tinto (what the user sees)

| ID | Criterion | Proof |
|---|---|---|
| I1 | The user starts a coordinator for the ticked batches on a chosen model, sees why a start failed, and sees one conversation per coordinator with each batch's state. | `DeliveryCoordinator.test.tsx` (all cases) |
| I2 | Each task says what it waits for, in the user's terms; a need notifies once, only with notifications on. | `taskStatus.test.ts`, `DeliveryNotifications.test.tsx` |

## Live acceptance scenario

Run on a throwaway GitHub repo with CI (one lint job, one test job) and three
small issues, one of which needs a user decision and one of which is split
into two packages. Batch unattended after the brainstorm; Jira optional.

Pass when, from batch creation to the last merge:

1. **No coordinator shell work:** the coordinator's transcript has zero test,
   build, install, git-push or merge commands (search the transcript for
   `npm`, `cargo`, `pytest`, `git push`, `gh pr merge`). *(A, D3, live)*
2. **Every PR merged with green CI** on its head, packages in order, no
   stacked PR. *(F3, live)*
3. **Nothing asked twice:** no `request_approval` for a step Tinto already
   asked (`approval_requested` by `auto`). *(F2, live)*
4. **Waiting works:** at least 60 % of `wait_run` replies have
   `waited: true` (the 2026-10-30 audit measured 28 %). *(C1–C6, live)*
5. **Restart survives:** quit and reopen Tinto while a stage runs; the
   stage ends normally or is retried and the coordinator continues without
   help. *(D4, live)*
6. **Takeover works:** hand the batch to a new conversation mid-run; the old
   one stops writing (fenced) and the new one finishes. *(B1–B4, live)*
7. **No truncated results:** no tool result is cut by the client; the
   overview never needs a file read-back. *(H1, live)*

Record: total coordinator turns, turns that only waited or bookkept, cached
tokens per turn, and user wait per step, for comparison with the audit.

## Gaps

- **Steps refused for a transient reason are not queued** (audit Q6): a merge
  asked while CI still runs is refused and the coordinator must ask again
  later. Needs a "waits for its precondition" state for approvals in the model
  and the panel. Tracked; not in this change.
- **No automated end-to-end run with a real agent:** the live scenario above
  is manual.
