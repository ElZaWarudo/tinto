// A repo's files view compared with a base commit instead of HEAD: Delivery
// opens a task's worktree this way, showing only what the task changed since
// its worktree was created (committed or not), each file with its diff
// against that commit. The comparison is remembered with the tab, so a tab
// restored when Tinto opens again reads its changes again instead of passing
// for a plain repo.

import { useSyncExternalStore } from "react";
import type { FileDiff } from "../bus/contract";
import { getDeliveryTaskChanges } from "../delivery/client";

export interface BaseView {
  /** What the comparison is against, for the user: `develop (0123abcd)`. */
  label: string;
  /** The Delivery task whose worktree this repo is; its changes come from it. */
  taskId: string;
  /** `null` while they are being read. */
  files: FileDiff[] | null;
  /** The explorer shows every file; the changed ones still open against the base. */
  showAll?: boolean;
}

const STORAGE_KEY = "tinto:delivery-base-views";
type Saved = Record<string, Pick<BaseView, "label" | "taskId" | "showAll">>;

const views = new Map<string, BaseView>();
const listeners = new Set<() => void>();
const reading = new Set<string>();

function saved(): Saved {
  try {
    return JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Saved;
  } catch {
    return {};
  }
}

function save(repo: string, view: BaseView | null) {
  try {
    const all = saved();
    if (view) all[repo] = { label: view.label, taskId: view.taskId, showAll: view.showAll };
    else delete all[repo];
    localStorage.setItem(STORAGE_KEY, JSON.stringify(all));
  } catch {
    // Without storage the comparison lasts until Tinto closes.
  }
}

function emit() {
  for (const listener of listeners) listener();
}

function put(repo: string, view: BaseView | null) {
  if (view) views.set(repo, view);
  else views.delete(repo);
  emit();
}

export function setBaseView(repo: string, view: BaseView | null) {
  put(repo, view);
  save(repo, view);
}

/** Switches the explorer between only the changed files and all of them. */
export function setShowAll(repo: string, showAll: boolean) {
  const view = views.get(repo);
  if (view) setBaseView(repo, { ...view, showAll });
}

/** Reads the task's changes again (its agents may still be working). One
 * read at a time per repo; a task that is gone ends the comparison. */
export async function refreshBaseView(repo: string): Promise<void> {
  const view = views.get(repo);
  if (!view || reading.has(repo)) return;
  reading.add(repo);
  try {
    const changes = await getDeliveryTaskChanges(view.taskId);
    const current = views.get(repo);
    if (current) put(repo, { ...current, files: changes.files });
  } catch {
    const current = views.get(repo);
    if (current?.files === null) setBaseView(repo, null);
  } finally {
    reading.delete(repo);
  }
}

/** A tab restored with the layout gets back the comparison it had. */
export function restoreBaseView(repo: string) {
  if (views.has(repo)) return;
  const entry = saved()[repo];
  if (!entry) return;
  put(repo, { ...entry, files: null });
  void refreshBaseView(repo);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useBaseView(repo: string): BaseView | null {
  return useSyncExternalStore(subscribe, () => views.get(repo) ?? null);
}

/** The file's diff against the base, when its repo is compared with one. */
export function useBaseDiff(repo: string, path: string): FileDiff | null {
  return useBaseView(repo)?.files?.find((file) => file.path === path) ?? null;
}

/** A file the task added (it was not in the base) or one it removed. */
export function baseChangeKind(file: FileDiff): "created" | "removed" | "modified" {
  const lines = file.hunks.flatMap((hunk) => hunk.lines);
  if (lines.length > 0 && lines.every((line) => line.kind === "Added")) {
    return file.hunks[0].old_start === 0 ? "created" : "modified";
  }
  if (lines.length > 0 && lines.every((line) => line.kind === "Removed")) {
    return file.hunks[0].new_start === 0 ? "removed" : "modified";
  }
  return "modified";
}
