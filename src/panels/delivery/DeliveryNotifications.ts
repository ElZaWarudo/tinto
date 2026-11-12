// A system notification when a Delivery task starts needing the user: a
// decision, an approval, a failed stage or a quarantined resource; also a
// stopped coordinator and guide rules to approve. It works with the Delivery
// view closed and follows the same switch as Tinto's other notifications.

import { useEffect, useRef } from "react";
import { busStore } from "../../bus/store";
import {
  getDeliveryGuide,
  getDeliveryOverview,
  getDeliveryReviewRequests,
  onDeliveryChanged,
  REVIEW_REQUESTS_EVERY_MS,
} from "../../delivery/client";
import type { DeliveryOverview, DeliveryRun } from "../../delivery/types";
import { tauriNotificationAdapter, type NotificationAdapter } from "../../qol/notifications";
import { useQualityState } from "../../qol/state";
import { taskLabel, taskStatus } from "./taskStatus";

export interface DeliveryNeed {
  /** What the task is waiting for. It is announced again only when one is new. */
  keys: string[];
  title: string;
  body: string;
}

/** How long a coordinator may leave changes unread before Tinto says so:
 *  long enough for one that is busy between two waits. */
export const COORDINATOR_STALL_MS = 3 * 60_000;

export interface CoordinatorStall {
  run: DeliveryRun;
  unread: number;
  since: number;
}

/** Batches whose coordinator stopped waiting with changes it has not read:
 *  nothing moves until someone tells it to go on. */
export function coordinatorStalls(overview: DeliveryOverview, now: number): CoordinatorStall[] {
  return (overview.coordinators ?? []).flatMap((state) => {
    const run = overview.runs.find((candidate) => candidate.id === state.run_id);
    const since = state.unread_since_ms;
    if (!run || run.paused || state.waiting || state.unread === 0 || since === null) return [];
    return now - since >= COORDINATOR_STALL_MS ? [{ run, unread: state.unread, since }] : [];
  });
}

/** A repo whose guide rules a coordinator proposed and the user has not decided. */
export interface GuideProposalWait {
  repo: string;
  /** When it was proposed: a new proposal replaces the one waiting. */
  at: number;
}

/** The repos of the batches with guide rules waiting for the user: only a
 *  batch's coordinator proposes them. A repo whose guide cannot be read is left out. */
export async function guideProposals(overview: DeliveryOverview): Promise<GuideProposalWait[]> {
  const repos = [...new Set(overview.runs.map((run) => run.repo))];
  const found = await Promise.all(
    repos.map((repo) =>
      getDeliveryGuide(repo).then(
        (guide) => (guide.proposal ? [{ repo, at: guide.proposal.requested_at_ms }] : []),
        () => [],
      ),
    ),
  );
  return found.flat();
}

/** The tasks that need the user, each with what it needs, the batches whose
 *  coordinator stopped and the repos with guide rules to decide. */
export function deliveryNeeds(
  overview: DeliveryOverview,
  now: number,
  guides: GuideProposalWait[] = [],
): DeliveryNeed[] {
  const stalls = coordinatorStalls(overview, now).map(({ run, unread, since }) => ({
    keys: [`${run.id}:coordinator:${since}`],
    title: `Tinto: ${run.title || run.id}`,
    body: `El coordinador no está esperando y tiene ${unread === 1 ? "1 cambio" : `${unread} cambios`} sin leer`,
  }));
  const rules = guides.map(({ repo, at }) => ({
    keys: [`${repo}:guide:${at}`],
    title: `Tinto: ${busStore.displayName(repo)}`,
    body: "Reglas propuestas de las guías por aprobar",
  }));
  return [...stalls, ...taskNeeds(overview, now), ...rules];
}

function taskNeeds(overview: DeliveryOverview, now: number): DeliveryNeed[] {
  return overview.tasks.flatMap((task) => {
    const status = taskStatus(task, overview, now);
    if (status.group !== "attention") return [];
    // The label counts pending decisions and QA problems, so it changes as
    // the user settles them; their ids do not.
    const decisions = (overview.decisions ?? [])
      .filter((decision) => decision.task_id === task.id && decision.status === "pending")
      .map((decision) => decision.id);
    const approvals = overview.approvals
      .filter((approval) => approval.task_id === task.id && approval.status === "pending")
      .map((approval) => `${approval.id}:${approval.status}`);
    const problems = (overview.qa_problems ?? [])
      .filter((problem) => problem.task_id === task.id && problem.status === "open")
      .map((problem) => problem.id);
    const ids = [...(decisions.length > 0 ? decisions : approvals), ...problems];
    const keys = ids.length > 0 ? ids : [`${task.id}:${status.label}`];
    return [{ keys, title: `Tinto: ${taskLabel(task)}`, body: status.label }];
  });
}

export function DeliveryNotificationWatcher({
  adapter = tauriNotificationAdapter,
}: {
  adapter?: NotificationAdapter;
}) {
  const { notificationsEnabled } = useQualityState();
  const seen = useRef(new Set<string>());

  useEffect(() => {
    if (!notificationsEnabled) return;
    let active = true;
    let unlisten: (() => void) | null = null;
    let pending: ReturnType<typeof setTimeout> | null = null;
    const check = () =>
      getDeliveryOverview().then(
        async (overview) => {
          const guides = await guideProposals(overview);
          if (!active) return;
          for (const need of deliveryNeeds(overview, Date.now(), guides)) {
            const fresh = need.keys.some((key) => !seen.current.has(key));
            for (const key of need.keys) seen.current.add(key);
            if (!fresh) continue;
            try {
              void Promise.resolve(
                adapter.send({ key: need.keys[0], title: need.title, body: need.body }),
              ).catch(() => {});
            } catch {
              // The other notifications report an unavailable system.
            }
          }
        },
        () => {},
      );
    void check();
    // A coordinator stalls by time passing, with no change to announce it.
    const clock = setInterval(() => void check(), 60_000);
    void onDeliveryChanged(() => {
      if (pending) return;
      // At most once a second: running agents change things all the time.
      pending = setTimeout(() => {
        pending = null;
        void check();
      }, 1000);
    }).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    });
    return () => {
      active = false;
      clearInterval(clock);
      unlisten?.();
      if (pending) clearTimeout(pending);
    };
  }, [adapter, notificationsEnabled]);

  // A PR someone asks the user to review, in any repo. The ones waiting when
  // Tinto opens are not news: only those that come afterwards are announced.
  const reviews = useRef<Set<string> | null>(null);
  useEffect(() => {
    if (!notificationsEnabled) return;
    let active = true;
    const check = () =>
      getDeliveryReviewRequests().then(
        (requests) => {
          if (!active) return;
          const known = reviews.current;
          reviews.current = new Set(requests.map((request) => request.url));
          for (const request of known ? requests : []) {
            if (known!.has(request.url)) continue;
            try {
              void Promise.resolve(
                adapter.send({
                  key: request.url,
                  title: `Tinto: ${request.repo} #${request.number}`,
                  body: `${request.author} te pide revisar: ${request.title}`,
                }),
              ).catch(() => {});
            } catch {
              // The other notifications report an unavailable system.
            }
          }
        },
        () => {},
      );
    void check();
    const clock = setInterval(() => void check(), REVIEW_REQUESTS_EVERY_MS);
    return () => {
      active = false;
      clearInterval(clock);
    };
  }, [adapter, notificationsEnabled]);

  return null;
}
