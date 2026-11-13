// The queue of what needs the user in one repo: what holds a batch or a task,
// what broke, and the PRs others wait for them to review. It is the one list
// of what waits on the user: the header's count is every repo's queue. A
// task's asks come from taskAsksOf, the same answer its detail shows; its
// phrase from taskStatus, the same its row shows.

import type {
  DeliveryApproval,
  DeliveryOverview,
  DeliveryReviewRequest,
  DeliveryTask,
} from "../../delivery/types";
import {
  repoKey,
  stallKey,
  type CoordinatorStall,
  type GuideProposalWait,
} from "./DeliveryNotifications";
import {
  approvalSummary,
  approveLabel,
  approvesInOneClick,
  batchName,
  blockerLabel,
  brokenJobOf,
  isOpen,
  jobBrokePhrase,
  jobsOf,
  openBlockers,
  phaseOf,
  taskAsksOf,
  taskLabel,
  taskStatus,
} from "./taskStatus";

/** What holds a batch or a task, what broke, a PR to review: the queue's order. */
export type QueueKind = "holds" | "broke" | "review";

/** What answers an item from the queue itself. */
export type QueueAnswer =
  | { kind: "approve"; approval: DeliveryApproval; label: string }
  | { kind: "retry"; jobId: string }
  | { kind: "resume"; runId: string };

/** What a batch's item goes to, instead of a task. */
export type QueueBatch =
  | { kind: "stall"; runId: string }
  | { kind: "guides"; repo: string }
  | { kind: "paused"; runId: string };

export interface QueueItem {
  key: string;
  kind: QueueKind;
  /** The task's label, the batch's name, or "PR #9 · ana". */
  title: string;
  /** What it needs, in a short line. */
  need: string;
  task: DeliveryTask | null;
  review: DeliveryReviewRequest | null;
  batch: QueueBatch | null;
  /** The ask of the task's detail it goes to, or `job:<id>` for a job's row
   *  in its history (a paused stage, resumed there); null: the task's first. */
  ask: string | null;
  /** null: it is answered where it goes (or, for a PR, on GitHub). */
  inPlace: QueueAnswer | null;
}

/** What waits on the user for a repo's batches, not for one task. */
export interface BatchWaits {
  repo: string;
  /** Coordinators that stopped with changes unread, in any repo. */
  stalls: CoordinatorStall[];
  /** The stops the user hid (stallKey). */
  hiddenStalls: readonly string[];
  /** The repos with guide rules proposed and not decided. */
  guides: GuideProposalWait[];
}

/** Phases that wait on the user for the batch, not for the task: listed once
 *  per batch or proposal, not under each task they hold. */
const BATCH_PHASES = new Set(["batch_paused", "settings_proposal"]);

/** Phases whose row says what broke, word for word as the queue does. */
const BROKE_PHASES = new Set(["broken", "setup_failed", "merged_failing_ci"]);

const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

const item = (
  of: Pick<QueueItem, "key" | "kind" | "title" | "need"> & Partial<QueueItem>,
): QueueItem => ({ task: null, review: null, batch: null, ask: null, inPlace: null, ...of });

/** Somebody works on the task: a job open for it, or a phase that waits on
 *  an agent or the queue. What broke or holds it is the user's only when not. */
function attended(overview: DeliveryOverview, task: DeliveryTask): boolean {
  const phase = phaseOf(overview, task.id);
  return (
    jobsOf(overview, task.id).some(isOpen) ||
    phase?.waits_on === "agent" ||
    phase?.waits_on === "queue"
  );
}

/** What broke on the task, once, as its row says it: its broken stage
 *  (retried from the queue when its detail would offer it, never while a
 *  blocker holds the task: the retry would be refused), else a failed setup,
 *  failing checks. */
function broke(
  overview: DeliveryOverview,
  task: DeliveryTask,
  now: number,
): Pick<QueueItem, "need" | "ask" | "inPlace"> | null {
  const phase = phaseOf(overview, task.id);
  const said = () => taskStatus(task, overview, now).label;
  const job = brokenJobOf(overview, task.id);
  if (job) {
    return {
      need: phase?.kind === "broken" || phase?.kind === "blocked" ? said() : jobBrokePhrase(job),
      ask: "broken",
      inPlace: openBlockers(task).length > 0 ? null : { kind: "retry", jobId: job.id },
    };
  }
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  // Something that broke is the user's only while nobody works on it.
  const idle = !attended(overview, task);
  const need = BROKE_PHASES.has(phase?.kind ?? "")
    ? said()
    : idle && phase?.failing_checks?.length
      ? `Fallan las comprobaciones: ${phase.failing_checks.join(", ")}`
      : idle && pr?.state === "open" && pr.checks.failing > 0
        ? `PR #${pr.number}: checks fallando`
        : null;
  return need ? { need, ask: null, inPlace: null } : null;
}

/** The PRs to review that belong to the repo of `tasks`: only those of the
 *  GitHub repository one of its tasks' PRs is in (a folder's name says nothing
 *  of whose repository it is; the header's menu lists every request). */
function reviewsOf(
  overview: DeliveryOverview,
  tasks: DeliveryTask[],
  reviews: DeliveryReviewRequest[],
): DeliveryReviewRequest[] {
  const ids = new Set(tasks.map((task) => task.id));
  const known = new Set(
    (overview.prs ?? []).filter((pr) => ids.has(pr.task_id)).map((pr) => pr.repo.toLowerCase()),
  );
  return reviews.filter((review) => known.has(review.repo.toLowerCase()));
}

/** A step approved from the queue itself: one approved in one click that
 *  publishes no text of its own, a push. A commit's messages, a sync's merge
 *  message and a Jira step's text are read in the task before they go out. */
function approvesFromQueue(approval: DeliveryApproval, edited: boolean): boolean {
  return approval.rung === "push" && approvesInOneClick(approval, edited);
}

/** What holds the repo's batches: a stopped coordinator the user has not
 *  hidden, its proposed guide rules, a paused batch, a settings proposal. */
function batchHolds(
  overview: DeliveryOverview,
  tasks: DeliveryTask[],
  batch: BatchWaits | null,
): QueueItem[] {
  const held = (kind: string) => tasks.filter((task) => phaseOf(overview, task.id)?.kind === kind);
  const key = batch && repoKey(batch.repo);
  const stalls = (batch?.stalls ?? []).filter(
    (stall) => repoKey(stall.run.repo) === key && !batch?.hiddenStalls.includes(stallKey(stall)),
  );
  const guides = (batch?.guides ?? []).filter((guide) => repoKey(guide.repo) === key);
  // One per batch, however many of its tasks it holds.
  const paused = [...new Set(held("batch_paused").map((task) => task.run_id))].flatMap(
    (runId) => overview.runs.find((run) => run.id === runId) ?? [],
  );
  const proposing = new Set(held("settings_proposal").map((task) => task.id));
  const proposals = (overview.settings_proposals ?? []).filter(
    (proposal) => proposal.status === "pending" && proposing.has(proposal.task_id),
  );
  return [
    ...stalls.map((stall) =>
      item({
        key: `stall:${stall.run.id}`,
        kind: "holds",
        title: batchName(stall.run.title || stall.run.id),
        need: `Coordinador parado · ${stall.unread === 1 ? "1 cambio" : `${stall.unread} cambios`} sin leer`,
        batch: { kind: "stall", runId: stall.run.id },
      }),
    ),
    ...guides.map((guide) =>
      item({
        key: `guides:${repoKey(guide.repo)}`,
        kind: "holds",
        title: "Guías",
        need: "Reglas propuestas de las guías",
        batch: { kind: "guides", repo: guide.repo },
      }),
    ),
    ...paused.map((run) =>
      item({
        key: `paused:${run.id}`,
        kind: "holds",
        title: batchName(run.title || run.id),
        need: "Lote en pausa",
        batch: { kind: "paused", runId: run.id },
        inPlace: { kind: "resume", runId: run.id },
      }),
    ),
    ...proposals.map((proposal) => {
      const task = tasks.find((candidate) => candidate.id === proposal.task_id)!;
      return item({
        key: `proposal:${proposal.id}`,
        kind: "holds",
        title: taskLabel(task),
        need: "Aprobar cambio de ajustes",
        task,
        ask: "settings",
      });
    }),
  ];
}

/** The queue of a repo, given its tasks in the list's order, every PR
 *  waiting for the user's review and what waits on the user for its batches
 *  (null: only the tasks'). `edited` says which approvals have a text the
 *  user edited in the task: those are approved there, never from here. */
export function deliveryQueue(
  overview: DeliveryOverview,
  tasks: DeliveryTask[],
  reviews: DeliveryReviewRequest[],
  now: number,
  edited: (approvalId: string) => boolean,
  batch: BatchWaits | null,
): QueueItem[] {
  const holds: QueueItem[] = batchHolds(overview, tasks, batch);
  const broken: QueueItem[] = [];
  const listed = new Set(holds.flatMap((held) => (held.task ? [held.task.id] : [])));
  for (const task of tasks) {
    const of = { task, title: taskLabel(task) };
    const before = holds.length;
    const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
    const asks = taskAsksOf(overview, task).filter(
      (ask) => ask.id !== "settings" && ask.id !== "broken",
    );
    for (const ask of asks) {
      const approval = overview.approvals.find((candidate) => candidate.id === ask.id);
      const oneClick = approval && approvesFromQueue(approval, edited(approval.id));
      holds.push(
        item({
          ...of,
          key: `${task.id}:${ask.id}`,
          kind: "holds",
          // What the click approves, as the task's line says it.
          need: oneClick
            ? `${capital(ask.label)} · ${approvalSummary(approval)}`
            : capital(ask.label),
          ask: ask.id,
          inPlace: oneClick
            ? { kind: "approve", approval, label: approveLabel(approval, pr) }
            : null,
        }),
      );
    }
    const failed = broke(overview, task, now);
    if (failed) {
      broken.push(item({ ...of, ...failed, key: `${task.id}:broke`, kind: "broke" }));
      continue;
    }
    // Waiting on the user for something without an ask of its own (a step
    // that no longer matches, news on its PR, a paused stage…): as its row
    // says it.
    const status = taskStatus(task, overview, now);
    const phase = phaseOf(overview, task.id);
    const waitsOnUser = status.group === "attention" || phase?.waits_on === "user";
    if (asks.length === 0 && waitsOnUser && !BATCH_PHASES.has(phase?.kind ?? "")) {
      holds.push(
        item({
          ...of,
          key: `${task.id}:status`,
          kind: "holds",
          need: status.label,
          // A paused stage is resumed on its job's row; a PR is moved in its section.
          ask:
            phase?.kind === "paused" ? `job:${phase.job_id}` : phase?.kind === "pr" ? "pr" : null,
        }),
      );
    }
    // Held by blockers nobody works on, and in the queue for nothing else:
    // lifting one is the user's, in the block that lists them.
    // Only a note: another task or a PR it waits for clears by itself.
    const blocker = openBlockers(task).find((one) => one.type === "note");
    if (blocker && holds.length === before && !listed.has(task.id) && !attended(overview, task)) {
      holds.push(
        item({
          ...of,
          key: `${task.id}:blocked`,
          kind: "holds",
          need: `Bloqueada: ${blockerLabel(blocker)}`,
          ask: "broken",
        }),
      );
    }
  }
  const toReview: QueueItem[] = reviewsOf(overview, tasks, reviews).map((review) =>
    item({
      key: `review:${review.url}`,
      kind: "review",
      title: `PR #${review.number} · ${review.author}`,
      need: review.draft ? `${review.title} · borrador` : review.title,
      review,
    }),
  );
  return [...holds, ...broken, ...toReview];
}
