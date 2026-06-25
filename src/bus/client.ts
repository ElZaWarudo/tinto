// Thin client over the frozen bus contract: invoke wrappers + event listeners.
// invoke strings are the exact registered snake_case command names (KTD9). All
// RDM-007 command args are single-word, so no camelCase/snake_case conversion
// concerns. RDM-007 does NOT use set_subscriptions / list_repo_tree / diffs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  EVENT_AGENT_SESSION_OUTPUT,
  EVENT_AGENT_SESSION_CHANGE_LOG,
  EVENT_FS_EVENTS,
  EVENT_WATCHING_STATE,
  EVENT_WORKBENCH_DELTA,
  type CommitInfo,
  type AgentSessionChangeLog,
  type AgentSessionOutput,
  type AgentSession,
  type GitleaksSetupStatus,
  type GitleaksInstallResult,
  type CopyResult,
  type DeleteResult,
  type FileContent,
  type FileDiff,
  type FsEventBatch,
  type RepoDelta,
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

/** Adds a repo and resolves to the stored canonical path (the key the bus
 * reports it under), so the caller can open its project tab. */
export const addRepo = (workbench: string, path: string, alias?: string) =>
  invoke<string>("add_repo", { workbench, path, alias: alias ?? null });

export const addWslRepo = (workbench: string, distro: string, path: string, alias?: string) =>
  invoke<string>("add_wsl_repo", { workbench, distro, path, alias: alias ?? null });

export const removeRepo = (workbench: string, path: string) =>
  invoke("remove_repo", { workbench, path });

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

export const listAgentSessions = () => invoke<AgentSession[]>("list_agent_sessions");

export const getGitleaksSetupStatus = () =>
  invoke<GitleaksSetupStatus>("get_gitleaks_setup_status");
export const installGitleaks = () => invoke<GitleaksInstallResult>("install_gitleaks");
export const getRepoGitleaksSetupStatus = (repo: string) =>
  invoke<GitleaksSetupStatus>("get_repo_gitleaks_setup_status", { repo });
export const installRepoGitleaks = (repo: string) =>
  invoke<GitleaksInstallResult>("install_repo_gitleaks", { repo });
export const createRepoGitleaksConfig = (repo: string) =>
  invoke("create_repo_gitleaks_config", { repo });

export const revertSession = (sessionId: string, userConsent: boolean) =>
  invoke<AgentSession>("revert_session", { sessionId, userConsent });

export const agentBinaryAvailable = (agentType: string) =>
  invoke<boolean>("agent_binary_available", { agentType });

export const agentBinaryAvailableForRepo = (repo: string, agentType: string) =>
  invoke<boolean>("agent_binary_available_for_repo", { repo, agentType });

export const writeAgentSessionInput = (sessionId: string, input: string | Uint8Array) =>
  invoke("write_agent_session_input", {
    sessionId,
    inputBase64: encodeAgentInput(input),
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
  invoke("export_from_repo", { repo, sources, destDir });

export const deleteFromRepo = (repo: string, sources: string[]) =>
  invoke<DeleteResult>("delete_from_repo", { repo, sources });

export const restoreDeletedFromRepo = (repo: string, token: string) =>
  invoke("restore_deleted_from_repo", { repo, token });

export const redoDeletedFromRepo = (repo: string, token: string) =>
  invoke("redo_deleted_from_repo", { repo, token });

// ---- Event listeners (StrictMode-safe; see KTD6) ----
// Each returns a promise resolving to an unlisten fn. Callers attach in an
// effect with an `active` guard and call the unlisten on cleanup.
export const onWorkbenchDelta = (cb: (d: RepoDelta) => void): Promise<UnlistenFn> =>
  listen<RepoDelta>(EVENT_WORKBENCH_DELTA, (e) => cb(e.payload));

export const onFsEvents = (cb: (b: FsEventBatch) => void): Promise<UnlistenFn> =>
  listen<FsEventBatch>(EVENT_FS_EVENTS, (e) => cb(e.payload));

export const onWatchingState = (cb: (w: WatchingState) => void): Promise<UnlistenFn> =>
  listen<WatchingState>(EVENT_WATCHING_STATE, (e) => cb(e.payload));

export const onAgentSessionOutput = (
  cb: (output: AgentSessionOutput) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionOutput>(EVENT_AGENT_SESSION_OUTPUT, (e) => cb(e.payload));

export const onAgentSessionChangeLog = (
  cb: (changeLog: AgentSessionChangeLog) => void,
): Promise<UnlistenFn> =>
  listen<AgentSessionChangeLog>(EVENT_AGENT_SESSION_CHANGE_LOG, (e) => cb(e.payload));

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
