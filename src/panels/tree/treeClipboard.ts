// Simple cross-repo clipboard for tree copy/cut: holds the (repo, paths[])
// selected by Ctrl+C. Pasting copies them into the target repo folder using
// the file_ops backend command. We store repo-relative paths (not
// OS-absolute) so the payload survives workbench switches and the sender's
// repo stays explicit; the backend still validates everything.

import { useSyncExternalStore } from "react";

export interface TreeClipboardPayload {
  repo: string;
  paths: string[]; // repo-relative
  mode: "copy" | "cut";
}

let current: TreeClipboardPayload | null = null;
const listeners = new Set<() => void>();

const subscribe = (listener: () => void): (() => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

const emit = () => listeners.forEach((l) => l());

export const treeClipboard = {
  get: (): TreeClipboardPayload | null => current,
  set: (value: TreeClipboardPayload | null) => {
    current = value;
    emit();
  },
  copy: (repo: string, paths: string[]) => {
    current = { repo, paths, mode: "copy" };
    emit();
  },
  cut: (repo: string, paths: string[]) => {
    current = { repo, paths, mode: "cut" };
    emit();
  },
  clear: () => {
    current = null;
    emit();
  },
};

const noopSnapshot = (): TreeClipboardPayload | null => current;

export function useTreeClipboard(): TreeClipboardPayload | null {
  return useSyncExternalStore(subscribe, noopSnapshot, noopSnapshot);
}