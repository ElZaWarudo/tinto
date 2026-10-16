// A repo's files view compared with a base commit instead of HEAD: Delivery
// opens a task's worktree this way, showing only what the task changed since
// its worktree was created (committed or not), each file with its diff
// against that commit. Cleared, the repo shows its normal view again.

import { useSyncExternalStore } from "react";
import type { FileDiff } from "../bus/contract";

export interface BaseView {
  /** What the comparison is against, for the user: `develop (0123abcd)`. */
  label: string;
  files: FileDiff[];
  /** Reads the changes again (the task's agents may still be working). */
  refresh: () => Promise<FileDiff[]>;
}

const views = new Map<string, BaseView>();
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function setBaseView(repo: string, view: BaseView | null) {
  if (view) views.set(repo, view);
  else views.delete(repo);
  emit();
}

/** Replaces the files of a repo's comparison, if it still has one. */
export function updateBaseFiles(repo: string, files: FileDiff[]) {
  const view = views.get(repo);
  if (!view) return;
  views.set(repo, { ...view, files });
  emit();
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
  return useBaseView(repo)?.files.find((file) => file.path === path) ?? null;
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
