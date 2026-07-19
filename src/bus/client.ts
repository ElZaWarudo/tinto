// Thin client over the frozen bus contract: invoke wrappers + event listeners.
// invoke strings are the exact registered snake_case command names (KTD9). All
// RDM-007 command args are single-word, so no camelCase/snake_case conversion
// concerns. RDM-007 does NOT use set_subscriptions / list_repo_tree / diffs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  EVENT_AGENT_SESSIONS_CHANGED,
  EVENT_AGENT_SESSION_OUTPUT,
  EVENT_AGENT_SESSION_CHANGE_LOG,
  EVENT_AGENT_SESSION_TIMELINE,
  EVENT_FS_EVENTS,
  EVENT_WATCHING_STATE,
  EVENT_WORKBENCH_DELTA,
  type CommitInfo,
  type AgentSessionChangeLog,
  type AgentJournalSessionSummary,
  type AgentSessionOutput,
  type AgentSessionTimelineItem,
  type AgentSession,
  type AgentSessionResumeResult,
  type AgentRuntimeCatalog,
  type AgentSessionRuntimeOptions,
  type AgentHostCommandResult,
  type AgentProviderReadiness,
  type GitleaksSetupStatus,
  type GitleaksInstallResult,
  type CopyResult,
  type DeleteResult,
  type FileOpOutcome,
  type FileContent,
  type FileDiff,
  type FsEventBatch,
  type RepoDelta,
  type RepoFetchPreview,
  type RepoFetchResult,
  type RepoTree,
  type SubscriptionTarget,
  type WatchingState,
  type WorkbenchConfig,
  type WorkbenchSnapshot,
} from "./contract";

// ---- Commands ----
export const getWorkbenchSnapshot = () => invoke<WorkbenchSnapshot>("get_workbench_snapshot");

export const listWorkbenches = () => invoke<WorkbenchConfig>("list_workbenches");

export const setActiveWorkbench = (name: string) => invoke("set_active_workbench", { name });

export const createWorkbench = (name: string) => invoke("create_workbench", { name });

export const renameWorkbench = (from: string, to: string) =>
  invoke("rename_workbench", { from, to });

export const deleteWorkbench = (name: string) => invoke("delete_workbench", { name });

/** Adds a repo and resolves to the stored canonical path (the key the bus
 * reports it under), so the caller can open its project tab. */
export const addRepo = (workbench: string, path: string, alias?: string) =>
  invoke<string>("add_repo", { workbench, path, alias: alias ?? null });

export const addWslRepo = (workbench: string, distro: string, path: string, alias?: string) =>
  invoke<string>("add_wsl_repo", { workbench, distro, path, alias: alias ?? null });

export const removeRepo = (workbench: string, path: string) =>
  invoke("remove_repo", { workbench, path });

export const removeRepoEntry = (workbench: string, path: string) =>
  invoke<boolean>("remove_repo_entry", { workbench, path });

export const removeWslRepo = (workbench: string, distro: string, path: string) =>
  invoke("remove_wsl_repo", { workbench, distro, path });

export interface WslDirectoryEntry {
  name: string;
  path: string;
}

export interface WslDirectoryListing {
  path: string;
  is_git_repo: boolean;
  entries: WslDirectoryEntry[];
}

export const listWslDistros = () => invoke<string[]>("list_wsl_distros");

export const listWslDirectory = (distro: string, path?: string | null) =>
  invoke<WslDirectoryListing>("list_wsl_directory", { distro, path: path ?? null });

export const updateRepo = (
  workbench: string,
  path: string,
  options: { alias?: string; clearAlias?: boolean; fsWatch?: string[] },
) =>
  invoke("update_repo", {
    workbench,
    path,
    alias: options.alias,
    clearAlias: options.clearAlias,
    fsWatch: options.fsWatch,
  });

export const autodetectReposUnder = (root: string) =>
  invoke<string[]>("autodetect_repos_under", { root });

export const getCommitLog = (repo: string, offset: number, limit: number) =>
  invoke<CommitInfo[]>("get_commit_log", { repo, offset, limit });

export const getCommitDiff = (repo: string, commitId: string) =>
  invoke<FileDiff[]>("get_commit_diff", { repo, commitId });

export const getBlob = (repo: string, commitId: string, path: string) =>
  invoke<FileContent>("get_blob", { repo, commitId, path });

export const retryRepo = (repo: string) => invoke("retry_repo", { repo });

/** Drop a repo from the live bus state when it is no longer in the active
 *  workbench config (orphan panel / stale snapshot). */
export const forgetRepo = (repo: string) => invoke("forget_repo", { repo });

export const startAgentSession = (repo: string, agentType: string) =>
  invoke<string>("start_agent_session", { repo, agentType });

export const stopAgentSession = (sessionId: string) => invoke("stop_agent_session", { sessionId });

export const retryAgentSessionAcp = (sessionId: string, confirmed: boolean) =>
  invoke("retry_agent_session_acp", { sessionId, confirmed });

export const respondAgentSessionAcpPermission = (
  sessionId: string,
  permissionId: string,
  optionId?: string,
  deny = false,
) => invoke("respond_agent_session_acp_permission", { sessionId, permissionId, optionId, deny });

export const setAgentSessionAcpConfigOption = (
  sessionId: string,
  configId: string,
  valueId: string,
) => invoke("set_agent_session_acp_config_option", { sessionId, configId, valueId });

export const listAgentSessions = () => invoke<AgentSession[]>("list_agent_sessions");

export const getAgentRuntimeCatalog = (sessionId: string, refresh = false) =>
  invoke<AgentRuntimeCatalog | null>("get_agent_runtime_catalog", { sessionId, refresh });

export const listAgentJournalSessions = (limit?: number) =>
  invoke<AgentJournalSessionSummary[]>("list_agent_journal_sessions", { limit });

export const getAgentJournalSession = (sessionId: string) =>
  invoke<AgentSession | null>("get_agent_journal_session", { sessionId });

export const resumeAgentJournalSession = (sessionId: string) =>
  invoke<AgentSessionResumeResult>("resume_agent_journal_session", { sessionId });

export const branchAgentSessionFromMessage = (sessionId: string, messageId: string) =>
  invoke<AgentSessionResumeResult>("branch_agent_session_from_message", { sessionId, messageId });

export const deleteAgentJournalSession = (sessionId: string, userConsent: boolean) =>
  invoke<boolean>("delete_agent_journal_session", { sessionId, userConsent });

export const getGitleaksSetupStatus = () =>
  invoke<GitleaksSetupStatus>("get_gitleaks_setup_status");
export const installGitleaks = () => invoke<GitleaksInstallResult>("install_gitleaks");
export const getRepoGitleaksSetupStatus = (repo: string) =>
  invoke<GitleaksSetupStatus>("get_repo_gitleaks_setup_status", { repo });
export const installRepoGitleaks = (repo: string) =>
  invoke<GitleaksInstallResult>("install_repo_gitleaks", { repo });
export const createRepoGitleaksConfig = (repo: string) =>
  invoke("create_repo_gitleaks_config", { repo });
export const createRepoAgentsMdConfig = (repo: string) =>
  invoke("create_repo_agents_md_config", { repo });
export const getRepoFetchPreview = (repo: string) =>
  invoke<RepoFetchPreview>("get_repo_fetch_preview", { repo });
export const fetchRepo = (
  repo: string,
  remote: string,
  confirmedHost: string,
  userConsent: boolean,
) =>
  invoke<RepoFetchResult>("fetch_repo", {
    repo,
    remote,
    confirmedHost,
    userConsent,
  });

export const revertSession = (sessionId: string, userConsent: boolean) =>
  invoke<AgentSession>("revert_session", { sessionId, userConsent });

export const revertSessionTurnFile = (
  sessionId: string,
  turnCheckpointId: string,
  path: string,
  userConsent: boolean,
) =>
  invoke<AgentSession>("revert_session_turn_file", {
    sessionId,
    turnCheckpointId,
    path,
    userConsent,
  });

export const restoreSessionTurn = (
  sessionId: string,
  turnCheckpointId: string,
  userConsent: boolean,
) =>
  invoke<AgentSession>("restore_session_turn", {
    sessionId,
    turnCheckpointId,
    userConsent,
  });

export const agentBinaryAvailable = (agentType: string) =>
  invoke<boolean>("agent_binary_available", { agentType });

export const agentBinaryAvailableForRepo = (repo: string, agentType: string) =>
  invoke<boolean>("agent_binary_available_for_repo", { repo, agentType });

export const agentProviderReadinessForRepo = (repo: string, agentType: string) =>
  invoke<AgentProviderReadiness>("agent_provider_readiness_for_repo", { repo, agentType });

export const writeAgentSessionInput = (
  sessionId: string,
  input: string | Uint8Array,
  options?: AgentSessionRuntimeOptions,
) =>
  invoke("write_agent_session_input", {
    sessionId,
    inputBase64: encodeAgentInput(input),
    options: options ?? null,
  });

export const writeAgentSessionTurn = (
  sessionId: string,
  text: string,
  attachmentPaths: string[],
  options?: AgentSessionRuntimeOptions,
) =>
  invoke("write_agent_session_turn", {
    sessionId,
    text,
    attachmentPaths,
    options: options ?? null,
  });

export const steerAgentSessionTurn = (sessionId: string, text: string, attachmentPaths: string[]) =>
  invoke("steer_agent_session_turn", {
    sessionId,
    text,
    attachmentPaths,
  });

export const getAgentImagePreview = (path: string) =>
  invoke<string | null>("get_agent_image_preview", { path });

export const runAgentHostCommand = (sessionId: string, command: string, argument?: string) =>
  invoke<AgentHostCommandResult>("run_agent_host_command", {
    sessionId,
    command,
    argument: argument ?? null,
  });

export const resizeAgentSession = (sessionId: string, cols: number, rows: number) =>
  invoke("resize_agent_session", { sessionId, cols, rows });

// ---- RDM-008: diff / tree / content / subscriptions ----
export const getWorktreeDiff = (repo: string) => invoke<FileDiff[]>("get_worktree_diff", { repo });

export const getFileContent = (repo: string, path: string) =>
  invoke<FileContent>("get_file_content", { repo, path });

export const getMediaContent = (repo: string, path: string) =>
  invoke<FileContent>("get_media_content", { repo, path });

export const listRepoTree = (repo: string) => invoke<RepoTree>("list_repo_tree", { repo });

export const setSubscriptions = (targets: SubscriptionTarget[]) =>
  invoke("set_subscriptions", { targets });

// ---- File operations (drag/drop/paste/move) ----
export const copyToRepo = (repo: string, destDir: string, sources: string[], overwrite: boolean) =>
  invoke<CopyResult>("copy_to_repo", {
    repo,
    destDir,
    sources,
    overwrite,
  });

export const copyWithinRepo = (
  repo: string,
  sources: string[],
  destDir: string,
  overwrite: boolean,
) =>
  invoke<CopyResult>("copy_within_repo", {
    repo,
    sources,
    destDir,
    overwrite,
  });

export const moveWithinRepo = (
  repo: string,
  sources: string[],
  destDir: string,
  overwrite: boolean,
) =>
  invoke<CopyResult>("move_within_repo", {
    repo,
    sources,
    destDir,
    overwrite,
  });

export const exportFromRepo = (repo: string, sources: string[], destDir: string) =>
  invoke<FileOpOutcome>("export_from_repo", { repo, sources, destDir });

export const deleteFromRepo = (repo: string, sources: string[], userConsent: boolean) =>
  invoke<DeleteResult>("delete_from_repo", { repo, sources, userConsent });

export const restoreDeletedFromRepo = (repo: string, token: string) =>
  invoke<FileOpOutcome>("restore_deleted_from_repo", { repo, token });

export const redoDeletedFromRepo = (repo: string, token: string) =>
  invoke<FileOpOutcome>("redo_deleted_from_repo", { repo, token });

// ---- Event listeners (StrictMode-safe; see KTD6) ----
// Each returns a promise resolving to an unlisten fn. Callers attach in an
// effect with an `active` guard and call the unlisten on cleanup.
const noopUnlisten: UnlistenFn = () => {};

function safeListen<T>(
  event: string,
  handler: Parameters<typeof listen<T>>[1],
): Promise<UnlistenFn> {
  try {
    return listen<T>(event, handler);
  } catch (error) {
    if (!isUnavailableTauriEventBridgeError(error)) {
      return Promise.reject(error);
    }
    return Promise.resolve(noopUnlisten);
  }
}

function isUnavailableTauriEventBridgeError(error: unknown): boolean {
  return error instanceof TypeError && error.message.includes("transformCallback");
}

export const onWorkbenchDelta = (cb: (d: RepoDelta) => void): Promise<UnlistenFn> =>
  safeListen<RepoDelta>(EVENT_WORKBENCH_DELTA, (e) => cb(e.payload));

export const onFsEvents = (cb: (b: FsEventBatch) => void): Promise<UnlistenFn> =>
  safeListen<FsEventBatch>(EVENT_FS_EVENTS, (e) => cb(e.payload));

export const onWatchingState = (cb: (w: WatchingState) => void): Promise<UnlistenFn> =>
  safeListen<WatchingState>(EVENT_WATCHING_STATE, (e) => cb(e.payload));

export const onAgentSessionsChanged = (
  cb: (sessions: AgentSession[]) => void,
): Promise<UnlistenFn> =>
  safeListen<AgentSession[]>(EVENT_AGENT_SESSIONS_CHANGED, (e) => cb(e.payload));

export const onAgentSessionOutput = (
  cb: (output: AgentSessionOutput) => void,
): Promise<UnlistenFn> =>
  safeListen<AgentSessionOutput>(EVENT_AGENT_SESSION_OUTPUT, (e) => cb(e.payload));

export const onAgentSessionChangeLog = (
  cb: (changeLog: AgentSessionChangeLog) => void,
): Promise<UnlistenFn> =>
  safeListen<AgentSessionChangeLog>(EVENT_AGENT_SESSION_CHANGE_LOG, (e) => cb(e.payload));

export const onAgentSessionTimeline = (
  cb: (item: AgentSessionTimelineItem) => void,
): Promise<UnlistenFn> =>
  safeListen<AgentSessionTimelineItem>(EVENT_AGENT_SESSION_TIMELINE, (e) => cb(e.payload));

const encodeAgentInput = (input: string | Uint8Array) => {
  const bytes = typeof input === "string" ? new TextEncoder().encode(input) : input;
  let binary = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const chunk = bytes.subarray(offset, offset + chunkSize);
    binary += String.fromCharCode(...chunk);
  }
  return btoa(binary);
};
