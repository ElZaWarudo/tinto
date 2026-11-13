// Retrying a job from any place in this window (the queue, the task's
// broken stage, its history): once per job. Each button guards only itself,
// so two of them clicked before the overview refreshes would create two
// attempts. A retry makes a new job with its own id, so a job retried never
// needs it again from here; one whose retry failed may be tried again.

import { retryDeliveryJob } from "../../delivery/client";
import type { DeliveryJob } from "../../delivery/types";

const retried = new Set<string>();

/** Retries `jobId` unless this window already did (or is doing it): then it
 *  resolves to `false`, which ActionButton reads as "did not happen". */
export async function retryJobOnce(jobId: string): Promise<DeliveryJob | false> {
  if (retried.has(jobId)) return false;
  retried.add(jobId);
  try {
    return await retryDeliveryJob(jobId);
  } catch (cause) {
    retried.delete(jobId);
    throw cause;
  }
}
