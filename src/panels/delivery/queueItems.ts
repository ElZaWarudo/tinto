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
  lower,
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
  /** The ask of the task's detail it goes to; null: the task's first. */
  ask: string | null;
  /** null: it is answered in the task (or, for a PR, on GitHub). */
  inPlace: QueueAnswer | null;
}

/** Phases that wait on the user for the batch, not for the task. */
const BATCH_PHASES = new Set(["batch_paused", "settings_proposal"]);

const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** What broke on the task, once: its broken stage (retried from the queue
 *  when its detail would offer it), else a failed setup, failing checks. */
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
      inPlace: { kind: "retry", jobId: job.id },
    };
  }
  const phase = phaseOf(overview, task.id);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  const need =
    phase?.kind === "broken"
      ? `${phase.blocked ? "Bloqueada en" : "Falló"} ${lower(stageLabel(phase.role))} · intento ${phase.attempt}`
      : phase?.kind === "setup_failed"
        ? `Falló la preparación: ${phase.command}`
        : phase?.kind === "merged_failing_ci"
          ? `PR #${phase.number} fusionado con checks fallando`
          : phase?.failing_checks?.length
            ? `Fallan las comprobaciones: ${phase.failing_checks.join(", ")}`
            : pr?.state === "open" && pr.checks.failing > 0
              ? `PR #${pr.number}: checks fallando`
              : null;
  return need ? { need, ask: null, inPlace: null } : null;
}

/** The PRs to review that belong to `repo`: by the GitHub name its tasks' PRs
 *  have, else by its folder's name. */
function reviewsOf(
  overview: DeliveryOverview,
  repo: string,
  tasks: DeliveryTask[],
  reviews: DeliveryReviewRequest[],
): DeliveryReviewRequest[] {
  const ids = new Set(tasks.map((task) => task.id));
  const known = new Set(
    (overview.prs ?? []).filter((pr) => ids.has(pr.task_id)).map((pr) => pr.repo.toLowerCase()),
  );
  const folder = repo.split(/[\\/]/).filter(Boolean).at(-1)?.toLowerCase();
  return reviews.filter((review) => {
    const name = review.repo.toLowerCase();
    return known.size > 0 ? known.has(name) : name.split("/")[1] === folder;
  });
}

/** The queue of `repo`, given its tasks in the list's order and every PR
 *  waiting for the user's review. `edited` says which approvals have a text
 *  the user edited in the task: those are approved there, never from here. */
export function deliveryQueue(
  overview: DeliveryOverview,
  repo: string,
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
      const oneClick = approval && approvesInOneClick(approval, edited(approval.id));
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
    // that no longer matches, news on its PR…): as its row says it.
    const status = taskStatus(task, overview, now);
    const phase = phaseOf(overview, task.id);
    if (asks.length === 0 && status.group === "attention" && !BATCH_PHASES.has(phase?.kind ?? "")) {
      holds.push({
        ...of,
        key: `${task.id}:status`,
        kind: "holds",
        need: status.label,
        ask: null,
        inPlace: null,
      });
    }
  }
  const toReview: QueueItem[] = reviewsOf(overview, repo, tasks, reviews).map((review) => ({
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
