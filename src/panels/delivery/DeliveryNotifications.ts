// A system notification when a Delivery task starts needing the user: a
// decision, an approval, a failed stage or a quarantined resource. It works
// with the Delivery view closed and follows the same switch as Tinto's other
// notifications.

import { useEffect, useRef } from "react";
import { getDeliveryOverview, onDeliveryChanged } from "../../delivery/client";
import type { DeliveryOverview, DeliveryRun } from "../../delivery/types";
import { tauriNotificationAdapter, type NotificationAdapter } from "../../qol/notifications";
import { useQualityState } from "../../qol/state";
import { taskStatus } from "./taskStatus";

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

/** The tasks that need the user, each with what it needs, and the batches
 *  whose coordinator stopped. */
export function deliveryNeeds(overview: DeliveryOverview, now: number): DeliveryNeed[] {
  const stalls = coordinatorStalls(overview, now).map(({ run, unread, since }) => ({
    keys: [`${run.id}:coordinator:${since}`],
    title: `Tinto: ${run.title || run.id}`,
    body: `El coordinador no está esperando y tiene ${unread === 1 ? "1 cambio" : `${unread} cambios`} sin leer`,
  }));
  return [...stalls, ...taskNeeds(overview, now)];
}

function taskNeeds(overview: DeliveryOverview, now: number): DeliveryNeed[] {
  return overview.tasks.flatMap((task) => {
    const status = taskStatus(task, overview, now);
    if (status.group !== "attention") return [];
    // The label counts pending decisions, so it changes as they are answered;
    // their ids do not.
    const decisions = (overview.decisions ?? [])
      .filter((decision) => decision.task_id === task.id && decision.status === "pending")
      .map((decision) => decision.id);
    const approvals = overview.approvals
      .filter((approval) => approval.task_id === task.id && approval.status === "pending")
      .map((approval) => `${approval.id}:${approval.status}`);
    const keys =
      decisions.length > 0
        ? decisions
        : approvals.length > 0
          ? approvals
          : [`${task.id}:${status.label}`];
    return [{ keys, title: `Tinto: ${task.key}`, body: status.label }];
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
        (overview) => {
          if (!active) return;
          for (const need of deliveryNeeds(overview, Date.now())) {
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

  return null;
}
