// Retrying a job from any place in this window (the queue, the task's
// broken stage, its history): one call per job at a time. Each button guards
// only itself, so two of them clicked together would create two attempts.
// Once the call settles the job can be retried again: Tinto refuses it while
// a newer attempt of the stage is open, and says so.

import { retryDeliveryJob } from "../../delivery/client";
import type { DeliveryJob } from "../../delivery/types";

const retrying = new Set<string>();

/** Retries `jobId` unless a retry of it from this window is still in flight:
 *  then it resolves to `false`, which ActionButton reads as "did not happen". */
export async function retryJobOnce(jobId: string): Promise<DeliveryJob | false> {
  if (retrying.has(jobId)) return false;
  retrying.add(jobId);
  try {
    return await retryDeliveryJob(jobId);
  } finally {
    retrying.delete(jobId);
  }
}
