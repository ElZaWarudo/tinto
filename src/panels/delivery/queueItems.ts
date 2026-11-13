// The queue of what needs the user in one repo: what holds a task, what broke,
// and the PRs others wait for them to review. A task's asks come from
// taskAsksOf, the same answer its detail shows; batch-level things (a stopped
// coordinator, proposed rules, settings proposals) stay where they are.

import type {
  DeliveryApproval,
  DeliveryOverview,
  DeliveryReviewRequest,
  DeliveryTask,
} from "../../delivery/types";
import {
  approvalSummary,
  approveLabel,
  approvesInOneClick,
  brokenJobOf,
  isOpen,
  jobsOf,
  lower,
  openBlockers,
  phaseOf,
  stageLabel,
  taskAsksOf,
  taskLabel,
  taskStatus,
} from "./taskStatus";

/** What holds a task, what broke, a PR to review: the queue's order. */
export type QueueKind = "holds" | "broke" | "review";

/** What answers an item from the queue itself. */
export type QueueAnswer =
  { kind: "approve"; approval: DeliveryApproval; label: string } | { kind: "retry"; jobId: string };

export interface QueueItem {
  key: string;
  kind: QueueKind;
  /** The task's label, or "PR #9 · ana". */
  title: string;
  /** What it needs, in a short line. */
  need: string;
  task: DeliveryTask | null;
  review: DeliveryReviewRequest | null;
  /** The ask of the task's detail it goes to, or `job:<id>` for a job's row
   *  in its history (a paused stage, resumed there); null: the task's first. */
  ask: string | null;
  /** null: it is answered in the task (or, for a PR, on GitHub). */
  inPlace: QueueAnswer | null;
}

/** Phases that wait on the user for the batch, not for the task. */
const BATCH_PHASES = new Set(["batch_paused", "settings_proposal"]);

const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** What broke on the task, once: its broken stage (retried from the queue
 *  when its detail would offer it, never while a blocker holds the task: the
 *  retry would be refused), else a failed setup, failing checks. */
function broke(
  overview: DeliveryOverview,
  task: DeliveryTask,
): Omit<QueueItem, "key" | "kind" | "title" | "task" | "review"> | null {
  const job = brokenJobOf(overview, task.id);
  if (job) {
    const verb = job.result?.status === "blocked" ? "Bloqueada en" : "Falló";
    return {
      need: `${verb} ${lower(stageLabel(job.role))} · intento ${job.attempt}`,
      ask: "broken",
      inPlace: openBlockers(task).length > 0 ? null : { kind: "retry", jobId: job.id },
    };
  }
  const phase = phaseOf(overview, task.id);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  // Something that broke is the user's only while nobody works on it: a job
  // open for the task, or a phase that waits on an agent or the queue, is on it.
  const attended =
    jobsOf(overview, task.id).some(isOpen) ||
    phase?.waits_on === "agent" ||
    phase?.waits_on === "queue";
  const need =
    phase?.kind === "broken"
      ? `${phase.blocked ? "Bloqueada en" : "Falló"} ${lower(stageLabel(phase.role))} · intento ${phase.attempt}`
      : phase?.kind === "setup_failed"
        ? `Falló la preparación: ${phase.command}`
        : phase?.kind === "merged_failing_ci"
          ? `PR #${phase.number} fusionado con checks fallando`
          : !attended && phase?.failing_checks?.length
            ? `Fallan las comprobaciones: ${phase.failing_checks.join(", ")}`
            : !attended && pr?.state === "open" && pr.checks.failing > 0
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

/** The queue of a repo, given its tasks in the list's order and every PR
 *  waiting for the user's review. `edited` says which approvals have a text
 *  the user edited in the task: those are approved there, never from here. */
export function deliveryQueue(
  overview: DeliveryOverview,
  tasks: DeliveryTask[],
  reviews: DeliveryReviewRequest[],
  now: number,
  edited: (approvalId: string) => boolean,
): QueueItem[] {
  const holds: QueueItem[] = [];
  const broken: QueueItem[] = [];
  for (const task of tasks) {
    const of = { task, review: null, title: taskLabel(task) };
    const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
    const asks = taskAsksOf(overview, task).filter(
      (ask) => ask.id !== "settings" && ask.id !== "broken",
    );
    for (const ask of asks) {
      const approval = overview.approvals.find((candidate) => candidate.id === ask.id);
      const oneClick = approval && approvesFromQueue(approval, edited(approval.id));
      holds.push({
        ...of,
        key: `${task.id}:${ask.id}`,
        kind: "holds",
        // What the click approves, as the task's line says it.
        need: oneClick
          ? `${capital(ask.label)} · ${approvalSummary(approval)}`
          : capital(ask.label),
        ask: ask.id,
        inPlace: oneClick ? { kind: "approve", approval, label: approveLabel(approval, pr) } : null,
      });
    }
    const failed = broke(overview, task);
    if (failed) {
      broken.push({ ...of, ...failed, key: `${task.id}:broke`, kind: "broke" });
      continue;
    }
    // Waiting on the user for something without an ask of its own (a step
    // that no longer matches, news on its PR, a paused stage…): as its row
    // says it.
    const status = taskStatus(task, overview, now);
    const phase = phaseOf(overview, task.id);
    const waitsOnUser = status.group === "attention" || phase?.waits_on === "user";
    if (asks.length === 0 && waitsOnUser && !BATCH_PHASES.has(phase?.kind ?? "")) {
      holds.push({
        ...of,
        key: `${task.id}:status`,
        kind: "holds",
        need: status.label,
        // A paused stage is resumed on its job's row; a PR is moved in its section.
        ask: phase?.kind === "paused" ? `job:${phase.job_id}` : phase?.kind === "pr" ? "pr" : null,
        inPlace: null,
      });
    }
  }
  const toReview: QueueItem[] = reviewsOf(overview, tasks, reviews).map((review) => ({
    key: `review:${review.url}`,
    kind: "review",
    title: `PR #${review.number} · ${review.author}`,
    need: review.draft ? `${review.title} · borrador` : review.title,
    task: null,
    review,
    ask: null,
    inPlace: null,
  }));
  return [...holds, ...broken, ...toReview];
}
