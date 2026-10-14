// Typed wrappers for the Delivery commands (src-tauri/src/delivery/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryApproval,
  DeliveryCommit,
  DeliveryCommitDraft,
  DeliveryConversation,
  DeliveryDecision,
  DeliveryPr,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryLease,
  DeliveryOverview,
  DeliveryPlatform,
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
  reasoning?: string | null;
  access: DeliveryAccess;
  prompt: string;
  /** Claude jobs on a Windows repo: where it runs; Tinto chooses when null. */
  platform?: DeliveryPlatform | null;
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
    reasoning: job.reasoning || null,
    platform: job.platform ?? null,
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
  plan?: { commits: DeliveryCommit[]; leaveOut: string[] },
  mergeMethod?: string,
  /** `jira` only: after_pr or after_merge; the next one missing when omitted. */
  stage?: string,
) =>
  invoke<DeliveryApproval>("delivery_request_approval", {
    taskId,
    rung,
    title,
    body,
    commits: plan?.commits ?? null,
    leaveOut: plan?.leaveOut ?? null,
    mergeMethod: mergeMethod ?? null,
    stage: stage ?? null,
  });

/** Reads the task's PR from GitHub, unless it was read moments ago. */
export const refreshDeliveryPr = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_refresh_pr", { taskId });

/** What opening a task checks: its PR is linked (found by its branch) and read. */
export const checkDeliveryTask = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_check_task", { taskId });

/** Links the PR GitHub has for the task's branch. */
export const findDeliveryPr = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_find_pr", { taskId });

/** What is new on the task's PR stops being new. */
export const markDeliveryPrSeen = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_pr_seen", { taskId });

/** The merge method the base branch's history shows: "merge" or "squash". */
export const getDeliveryMergeMethod = (taskId: string) =>
  invoke<string>("delivery_merge_method", { taskId });

/** The changed files and recent commit subjects a commit plan starts from. */
export const getDeliveryCommitDraft = (taskId: string) =>
  invoke<DeliveryCommitDraft>("delivery_commit_draft", { taskId });

/** Takes back the commits of an unpushed commit step; the changes stay in the worktree. */
export const undoDeliveryCommit = (approvalId: string) =>
  invoke<DeliveryApproval>("delivery_undo_commit", { approvalId });

export const decideDeliveryApproval = (approvalId: string, approve: boolean, note?: string) =>
  invoke<DeliveryApproval>("delivery_decide_approval", {
    approvalId,
    approve,
    note: note ?? null,
  });

export const completeDeliveryApproval = (approvalId: string, success: boolean, outcome: string) =>
  invoke<DeliveryApproval>("delivery_complete_approval", { approvalId, success, outcome });

/** Corrects a done step's result by hand; the previous one is kept. */
export const correctDeliveryApproval = (approvalId: string, outcome: string, reason: string) =>
  invoke<DeliveryApproval>("delivery_correct_approval", { approvalId, outcome, reason });

export const answerDeliveryDecision = (decisionId: string, answer: string) =>
  invoke<DeliveryDecision>("delivery_answer_decision", { decisionId, answer });

/** Allows again, for the worktree as it is now, the task's permissions that run its code. */
export const renewDeliveryPermissions = (taskId: string) =>
  invoke<DeliveryDecision[]>("delivery_renew_permissions", { taskId });

/** Takes back a decision nobody answered yet. */
export const withdrawDeliveryDecision = (decisionId: string) =>
  invoke<void>("delivery_withdraw_decision", { decisionId });

export const acceptRecommendedDecisions = (taskId: string) =>
  invoke<DeliveryDecision[]>("delivery_accept_recommended", { taskId });

export const createDeliveryRun = (repo: string, title: string, qaJiraComment: boolean) =>
  invoke<DeliveryRun>("delivery_create_run", { repo, title, qaJiraComment });

export const setDeliveryRunQaJiraComment = (runId: string, post: boolean) =>
  invoke<DeliveryRun>("delivery_set_run_qa_jira_comment", { runId, post });

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
