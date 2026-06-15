// Thin client over the frozen bus contract: invoke wrappers + event listeners.
// invoke strings are the exact registered snake_case command names (KTD9). All
// RDM-007 command args are single-word, so no camelCase/snake_case conversion
// concerns. RDM-007 does NOT use set_subscriptions / list_repo_tree / diffs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  EVENT_FS_EVENTS,
  EVENT_WATCHING_STATE,
  EVENT_WORKBENCH_DELTA,
  type CommitInfo,
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

export const addRepo = (workbench: string, path: string, alias?: string) =>
  invoke("add_repo", { workbench, path, alias: alias ?? null });

export const removeRepo = (workbench: string, path: string) =>
  invoke("remove_repo", { workbench, path });

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

export const retryRepo = (repo: string) => invoke("retry_repo", { repo });

// ---- RDM-008: diff / tree / content / subscriptions ----
export const getWorktreeDiff = (repo: string) => invoke<FileDiff[]>("get_worktree_diff", { repo });

export const getFileContent = (repo: string, path: string) =>
  invoke<FileContent>("get_file_content", { repo, path });

export const listRepoTree = (repo: string) => invoke<RepoTree>("list_repo_tree", { repo });

export const setSubscriptions = (targets: SubscriptionTarget[]) =>
  invoke("set_subscriptions", { targets });

// ---- Event listeners (StrictMode-safe; see KTD6) ----
// Each returns a promise resolving to an unlisten fn. Callers attach in an
// effect with an `active` guard and call the unlisten on cleanup.
export const onWorkbenchDelta = (cb: (d: RepoDelta) => void): Promise<UnlistenFn> =>
  listen<RepoDelta>(EVENT_WORKBENCH_DELTA, (e) => cb(e.payload));

export const onFsEvents = (cb: (b: FsEventBatch) => void): Promise<UnlistenFn> =>
  listen<FsEventBatch>(EVENT_FS_EVENTS, (e) => cb(e.payload));

export const onWatchingState = (cb: (w: WatchingState) => void): Promise<UnlistenFn> =>
  listen<WatchingState>(EVENT_WATCHING_STATE, (e) => cb(e.payload));
