// A system notification when a Delivery task starts needing the user: a
// decision, an approval, a failed stage or a quarantined resource. It works
// with the Delivery view closed and follows the same switch as Tinto's other
// notifications.

import { useEffect, useRef } from "react";
import { getDeliveryOverview, onDeliveryChanged } from "../../delivery/client";
import type { DeliveryOverview } from "../../delivery/types";
import { tauriNotificationAdapter, type NotificationAdapter } from "../../qol/notifications";
import { useQualityState } from "../../qol/state";
import { taskStatus } from "./taskStatus";

export interface DeliveryNeed {
  /** What the task is waiting for. It is announced again only when one is new. */
  keys: string[];
  title: string;
  body: string;
}

/** The tasks that need the user, each with what it needs. */
export function deliveryNeeds(overview: DeliveryOverview, now: number): DeliveryNeed[] {
  return overview.tasks.flatMap((task) => {
    const status = taskStatus(task, overview, now);
    if (status.group !== "attention") return [];
    // The label counts pending decisions, so it changes as they are answered;
    // their ids do not.
    const decisions = (overview.decisions ?? [])
      .filter((decision) => decision.task_id === task.id && decision.status === "pending")
      .map((decision) => decision.id);
    const approvals = overview.approvals
      .filter(
        (approval) =>
          approval.task_id === task.id &&
          (approval.status === "pending" || approval.status === "approved"),
      )
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
    void onDeliveryChanged(() => {
      if (pending) return;
      pending = setTimeout(() => {
        pending = null;
        void check();
      }, 150);
    }).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    });
    return () => {
      active = false;
      unlisten?.();
      if (pending) clearTimeout(pending);
    };
  }, [adapter, notificationsEnabled]);

  return null;
}
