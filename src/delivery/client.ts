// Typed wrappers for the Delivery commands (src-tauri/src/delivery/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryApproval,
  DeliveryConversation,
  DeliveryDecision,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryLease,
  DeliveryOverview,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryRung,
  DeliverySettings,
  DeliveryTask,
} from "./types";

export const EVENT_DELIVERY_CHANGED = "tinto://delivery-changed";

export const getDeliveryOverview = () => invoke<DeliveryOverview>("delivery_overview");

export interface NewDeliveryTask {
  repo: string;
  distro: string | null;
  key: string;
  title: string;
  base?: string | null;
  branch?: string | null;
  runId?: string | null;
}

export const createDeliveryTask = (task: NewDeliveryTask) =>
  invoke<DeliveryTask>("delivery_create_task", {
    repo: task.repo,
    distro: task.distro,
    key: task.key,
    title: task.title,
    base: task.base || null,
    branch: task.branch || null,
    runId: task.runId || null,
  });

export const removeDeliveryTask = (taskId: string, force: boolean) =>
  invoke<void>("delivery_remove_task", { taskId, force });

export const updateDeliveryTask = (
  taskId: string,
  changes: { state?: string; contractVersion?: number; title?: string },
) =>
  invoke<DeliveryTask>("delivery_update_task", {
    taskId,
    state: changes.state ?? null,
    contractVersion: changes.contractVersion ?? null,
    title: changes.title ?? null,
  });

export interface NewDeliveryJob {
  taskId: string;
  role: string;
  agent: DeliveryAgent;
  model?: string | null;
  access: DeliveryAccess;
  prompt: string;
}

export const dispatchDeliveryJob = (job: NewDeliveryJob) =>
  invoke<DeliveryJob>("delivery_dispatch_job", {
    taskId: job.taskId,
    role: job.role,
    agent: job.agent,
    model: job.model || null,
    access: job.access,
    prompt: job.prompt,
    writes: null,
    lease: null,
    timeoutMinutes: null,
  });

export const cancelDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_cancel_job", { jobId });

export const retryDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_retry_job", { jobId });

export const undoDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_undo_job", { jobId });

export const getDeliveryJobLog = (jobId: string, fromLine = 0) =>
  invoke<DeliveryJobLog>("delivery_job_log", { jobId, fromLine });

export const updateDeliverySettings = (capacity: number) =>
  invoke<DeliverySettings>("delivery_update_settings", { capacity });

/** The model Codex jobs use when they name none (the catalog's default). */
export const setDeliveryCodexModel = (model: string | null) =>
  invoke<void>("delivery_set_codex_model", { model });

export const getDeliveryRepoSettings = (repo: string) =>
  invoke<DeliveryRepoSettings>("delivery_repo_settings", { repo });

export const setDeliveryRepoSettings = (repo: string, settings: DeliveryRepoSettings) =>
  invoke<DeliveryRepoSettings>("delivery_set_repo_settings", { repo, settings });

export const releaseDeliveryLease = (name: string, note: string) =>
  invoke<DeliveryLease>("delivery_release_lease", { name, note });

export const requestDeliveryApproval = (
  taskId: string,
  rung: DeliveryRung,
  title: string,
  body: string,
) => invoke<DeliveryApproval>("delivery_request_approval", { taskId, rung, title, body });

export const decideDeliveryApproval = (approvalId: string, approve: boolean, note?: string) =>
  invoke<DeliveryApproval>("delivery_decide_approval", {
    approvalId,
    approve,
    note: note ?? null,
  });

export const completeDeliveryApproval = (approvalId: string, success: boolean, outcome: string) =>
  invoke<DeliveryApproval>("delivery_complete_approval", { approvalId, success, outcome });

export const answerDeliveryDecision = (decisionId: string, answer: string) =>
  invoke<DeliveryDecision>("delivery_answer_decision", { decisionId, answer });

export const acceptRecommendedDecisions = (taskId: string) =>
  invoke<DeliveryDecision[]>("delivery_accept_recommended", { taskId });

export const createDeliveryRun = (repo: string, title: string) =>
  invoke<DeliveryRun>("delivery_create_run", { repo, title });

export const takeoverDeliveryRun = (runId: string) =>
  invoke<DeliveryRun>("delivery_takeover_run", { runId });

export const closeDeliveryRun = (runId: string) =>
  invoke<DeliveryRun>("delivery_close_run", { runId });

export const openDeliveryTaskInAgents = (taskId: string, agentType: string) =>
  invoke<DeliveryConversation>("delivery_open_task_in_agents", { taskId, agentType });

export const openDeliveryJobInAgents = (jobId: string) =>
  invoke<DeliveryConversation>("delivery_open_job_in_agents", { jobId });

export function onDeliveryChanged(callback: () => void): Promise<UnlistenFn> {
  try {
    return listen(EVENT_DELIVERY_CHANGED, () => callback());
  } catch {
    return Promise.resolve(() => {});
  }
}
