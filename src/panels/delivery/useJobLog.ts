import { useEffect, useRef, useState } from "react";
import { getDeliveryJobLog } from "../../delivery/client";
import type { DeliveryJob, DeliveryJobLog } from "../../delivery/types";
import { isOpen } from "./taskStatus";

/** The latest log entries a job shows: a check's whole output (a test suite
 * prints hundreds of thousands of lines) froze WebKit drawing it. */
export const MAX_LOG_ENTRIES = 500;

/** A job's log, read as it grows while the job runs; `skipped` counts the
 *  oldest entries dropped past {@link MAX_LOG_ENTRIES}. */
export function useJobLog(job: DeliveryJob): (DeliveryJobLog & { skipped: number }) | null {
  const [log, setLog] = useState<(DeliveryJobLog & { skipped: number }) | null>(null);
  const live = isOpen(job);
  const nextLine = useRef(0);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const from = nextLine.current;
        const chunk = await getDeliveryJobLog(job.id, from);
        // A slow read overtaken by another from the same line adds nothing.
        if (!active || from !== nextLine.current) return;
        nextLine.current = chunk.next_line;
        setLog((previous) => {
          const entries = [...(previous?.entries ?? []), ...chunk.entries];
          const over = Math.max(0, entries.length - MAX_LOG_ENTRIES);
          return {
            entries: entries.slice(over),
            skipped: (previous?.skipped ?? 0) + over,
            next_line: chunk.next_line,
            stderr_tail: chunk.stderr_tail,
          };
        });
      } catch {
        // The log appears once the job has started.
      }
    };
    void load();
    const timer = live ? setInterval(() => void load(), 1500) : null;
    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  }, [job.id, live]);
  return log;
}
