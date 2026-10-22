// Typed wrappers for the Delivery commands (src-tauri/src/delivery/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryApproval,
  DeliveryQaProblem,
  DeliveryArchivedTask,
  DeliveryCommit,
  DeliveryCommitDraft,
  DeliveryIssue,
  DeliveryConversation,
  DeliveryTaskChanges,
  DeliveryDecision,
  DeliveryPr,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryLease,
  DeliveryOverview,
  DeliveryPlatform,
  DeliveryEnvironmentCheck,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryRung,
  DeliverySettings,
  DeliverySettingsChanges,
  DeliverySettingsProposal,
  DeliveryTask,
  NewDeliveryBlocker,
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

/** Archives a delivered task: its worktree and local branch go. */
export const archiveDeliveryTask = (taskId: string) =>
  invoke<void>("delivery_archive_task", { taskId });

/** Archived tasks by issue key or PR number ("#279" or "279"); all when empty. */
export const getArchivedDeliveryTasks = (query: string) =>
  invoke<DeliveryArchivedTask[]>("delivery_archived_tasks", { query });

/** Brings an archived task back, with its worktree. */
export const unarchiveDeliveryTask = (taskId: string) =>
  invoke<DeliveryTask>("delivery_unarchive_task", { taskId });

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

/** Adds a blocker to the task; Tinto clears it when it resolves. */
export const addDeliveryBlocker = (taskId: string, blocker: NewDeliveryBlocker) =>
  invoke<DeliveryTask>("delivery_add_blocker", { taskId, blocker });

/** Clears the given blockers, or every open one: the user's override. */
export const clearDeliveryBlockers = (taskId: string, blockerIds?: string[]) =>
  invoke<DeliveryTask>("delivery_clear_blockers", { taskId, blockerIds: blockerIds ?? null });

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

/** What the task changed since its worktree was created; registers the worktree as a repo. */
export const getDeliveryTaskChanges = (taskId: string) =>
  invoke<DeliveryTaskChanges>("delivery_task_changes", { taskId });

/** The task's Jira issue page, from the repo's Jira in Tinto or the skill's jira.env. */
export const getDeliveryIssueUrl = (taskId: string) =>
  invoke<string | null>("delivery_issue_url", { taskId });

/** The task's Jira issue, read from the repo's Jira in Tinto. */
export const getDeliveryIssue = (taskId: string) =>
  invoke<DeliveryIssue>("delivery_issue", { taskId });

/** Opens a web page (Jira issue, PR) in the system's browser. */
export const openDeliveryUrl = (url: string) => invoke<void>("delivery_open_url", { url });

/** Opens a QA job's visual changes page in the system's browser. */
export const openDeliveryVisualRecord = (jobId: string) =>
  invoke<void>("delivery_open_visual_record", { jobId });

export const pauseDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_pause_job", { jobId });

export const resumeDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_resume_job", { jobId });

export const setDeliveryRunPaused = (runId: string, paused: boolean) =>
  invoke<DeliveryRun>("delivery_set_run_paused", { runId, paused });

export const retryDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_retry_job", { jobId });

export const undoDeliveryJob = (jobId: string) =>
  invoke<DeliveryJob>("delivery_undo_job", { jobId });

export const resolveDeliveryJob = (jobId: string, note: string) =>
  invoke<DeliveryJob>("delivery_resolve_job", { jobId, note });

export const getDeliveryJobLog = (jobId: string, fromLine = 0) =>
  invoke<DeliveryJobLog>("delivery_job_log", { jobId, fromLine });

export const updateDeliverySettings = (capacity: number) =>
  invoke<DeliverySettings>("delivery_update_settings", { capacity });

/** The account's model catalog of Codex or Muse Code, as Agents last loaded it: the models
 *  stages and coordinators may name, and the default Codex jobs that name none use. */
export const setDeliveryModelCatalog = (
  agent: "codex" | "muse",
  defaultModel: string | null,
  models: string[],
) => invoke<void>("delivery_set_model_catalog", { agent, defaultModel, models });

export const getDeliveryRepoSettings = (repo: string) =>
  invoke<DeliveryRepoSettings>("delivery_repo_settings", { repo });

export const setDeliveryRepoSettings = (repo: string, settings: DeliveryRepoSettings) =>
  invoke<DeliveryRepoSettings>("delivery_set_repo_settings", { repo, settings });

/** Saves the repo's Jira token apart from its settings; empty forgets it. */
export const setDeliveryJiraToken = (repo: string, token: string) =>
  invoke<void>("delivery_set_jira_token", { repo, token });

/** "Entorno": whether Docker (when the repo uses it) and its probes answer now. */
export const checkDeliveryEnvironment = (repo: string, distro: string | null) =>
  invoke<DeliveryEnvironmentCheck[]>("delivery_check_environment", { repo, distro });

/** "Comprobar Jira": the token owner's name, as Jira sees it. */
export const checkDeliveryJira = (repo: string) => invoke<string>("delivery_check_jira", { repo });

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
  /** `jira` only: statuses for Tinto to walk the issue through; the repo's when omitted. */
  jiraStatuses?: string[],
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
    jiraStatuses: jiraStatuses ?? null,
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

/** A problem QA missed, as the user describes it. */
export const reportDeliveryQaProblem = (taskId: string, text: string, place: string) =>
  invoke<DeliveryQaProblem>("delivery_report_qa_problem", { taskId, text, place });

/** The user decides a QA problem is not one. */
export const dismissDeliveryQaProblem = (problemId: string, reason: string) =>
  invoke<DeliveryQaProblem>("delivery_dismiss_qa_problem", { problemId, reason });

/** The user sends QA problems to fix, each with its solution. */
export const sendDeliveryQaProblems = (
  taskId: string,
  choices: { problem_id: string; solution: string }[],
) => invoke<DeliveryQaProblem[]>("delivery_send_qa_problems", { taskId, choices });

/** Stops or allows new tasks in a batch: create_task refuses them while stopped. */
export const setDeliveryRunNoNewTasks = (runId: string, noNewTasks: boolean) =>
  invoke<DeliveryRun>("delivery_set_run_no_new_tasks", { runId, noNewTasks });

/** Asks the Agents conversation `sessionId` to take the batches over and coordinate them all. */
export const handOverDeliveryRun = (runIds: string[], sessionId: string) =>
  invoke<void>("hand_over_delivery_run", { runIds, sessionId });

/** Opens an approved PR step's PR on GitHub with its approved text. */
export const openDeliveryPr = (approvalId: string) =>
  invoke<DeliveryApproval>("delivery_open_pr", { approvalId });

/** Closes the task's PR on GitHub without merging it. */
export const closeDeliveryPr = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_close_pr", { taskId });

/** Turns the task's PR back into a draft on GitHub. */
export const deliveryPrToDraft = (taskId: string) =>
  invoke<DeliveryPr | null>("delivery_pr_to_draft", { taskId });

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

/** `reviewers`: for a PR step, who the user chose to review it. */
export const decideDeliveryApproval = (
  approvalId: string,
  approve: boolean,
  note?: string,
  reviewers?: string[],
) =>
  invoke<DeliveryApproval>("delivery_decide_approval", {
    approvalId,
    approve,
    note: note ?? null,
    reviewers: reviewers ?? null,
  });

/** Who can review the task's PR: the repo's assignable people, without the user. */
export const getDeliveryPrReviewers = (taskId: string) =>
  invoke<string[]>("delivery_pr_reviewers", { taskId });

export const completeDeliveryApproval = (approvalId: string, success: boolean, outcome: string) =>
  invoke<DeliveryApproval>("delivery_complete_approval", { approvalId, success, outcome });

/** Corrects a done step's result by hand; the previous one is kept. */
export const correctDeliveryApproval = (approvalId: string, outcome: string, reason: string) =>
  invoke<DeliveryApproval>("delivery_correct_approval", { approvalId, outcome, reason });

/** Changes what the coordinator decided on its own. */
export const changeDeliveryDecision = (decisionId: string, answer: string) =>
  invoke<DeliveryDecision>("delivery_change_decision", { decisionId, answer });

/** Approves a settings proposal, or the part of it in `accepted`. */
export const approveDeliverySettingsProposal = (
  proposalId: string,
  accepted: DeliverySettingsChanges,
) =>
  invoke<DeliverySettingsProposal>("delivery_approve_settings_proposal", {
    proposalId,
    accepted,
  });

export const rejectDeliverySettingsProposal = (proposalId: string) =>
  invoke<DeliverySettingsProposal>("delivery_reject_settings_proposal", { proposalId });

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
