// Level-2 (file) tab state per repo, with a VS Code-style preview slot:
//   - A single CLICK previews a file in one shared, italic preview tab; clicking
//     another file reuses that slot (the previous preview vanishes).
//   - A DOUBLE click (or editing, which we don't have — read-only) PINS the file
//     as a permanent tab.
// `active === null` means the repo's pinned "Resumen" (overview) tab is showing.
// Level-1 (Dashboard / Timeline / project) tabs are owned by dockview.

import { useSyncExternalStore } from "react";

export interface RepoTabs {
  /** Pinned file tabs, in order. */
  files: string[];
  /** The single preview tab (italic), or null. Not in `files`. */
  preview: string | null;
  /** Active path (a pinned file or the preview), or null for the overview. */
  active: string | null;
}

const EMPTY: RepoTabs = { files: [], preview: null, active: null };

/** Visible tab order: pinned files first, then the preview (if any, and not
 * already pinned). */
export function visibleTabs(t: RepoTabs): string[] {
  return t.preview && !t.files.includes(t.preview) ? [...t.files, t.preview] : t.files;
}

class TabsStore {
  private state: Record<string, RepoTabs> = {};
  private listeners = new Set<() => void>();

  getState = (): Record<string, RepoTabs> => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(next: Record<string, RepoTabs>) {
    this.state = next;
    this.listeners.forEach((l) => l());
  }

  private write(repo: string, tabs: RepoTabs) {
    this.set({ ...this.state, [repo]: tabs });
  }

  getRepo(repo: string): RepoTabs {
    return this.state[repo] ?? EMPTY;
  }

  /** Single-click: show a file in the shared preview slot (or just focus it if
   * it is already a pinned tab). */
  previewFile(repo: string, path: string) {
    const cur = this.getRepo(repo);
    if (cur.files.includes(path)) {
      this.write(repo, { ...cur, active: path });
      return;
    }
    this.write(repo, { ...cur, preview: path, active: path });
  }

  /** Double-click / pin: promote a file to a permanent tab. */
  pinFile(repo: string, path: string) {
    const cur = this.getRepo(repo);
    const files = cur.files.includes(path) ? cur.files : [...cur.files, path];
    const preview = cur.preview === path ? null : cur.preview;
    this.write(repo, { files, preview, active: path });
  }

  /** Open a file by intent: `pin` true pins it, otherwise previews it. */
  openFile(repo: string, path: string, pin = false) {
    if (pin) this.pinFile(repo, path);
    else this.previewFile(repo, path);
  }

  /** Activate a file tab, or the overview (null). Ignores unknown files. */
  setActive(repo: string, path: string | null) {
    const cur = this.getRepo(repo);
    if (path !== null && path !== cur.preview && !cur.files.includes(path)) return;
    this.write(repo, { ...cur, active: path });
  }

  /** Close a tab (pinned or the preview); active falls back to a neighbour in
   * the visible order, then the overview. */
  closeFile(repo: string, path: string) {
    const cur = this.getRepo(repo);
    const visible = visibleTabs(cur);
    const idx = visible.indexOf(path);
    if (idx < 0) return;
    const files = cur.preview === path ? cur.files : cur.files.filter((f) => f !== path);
    const preview = cur.preview === path ? null : cur.preview;
    let active = cur.active;
    if (cur.active === path) {
      const next = preview && !files.includes(preview) ? [...files, preview] : files;
      active = next[idx] ?? next[idx - 1] ?? null;
    }
    this.write(repo, { files, preview, active });
  }

  /** Drop all tab state for a repo (its project tab was closed / repo removed). */
  closeRepo(repo: string) {
    if (!(repo in this.state)) return;
    const next = { ...this.state };
    delete next[repo];
    this.set(next);
  }

  reset() {
    this.set({});
  }
}

export const tabsStore = new TabsStore();

/** Subscribe to one repo's file-tab state. */
export function useRepoTabs(repo: string): RepoTabs {
  return useSyncExternalStore(tabsStore.subscribe, () => tabsStore.getState()[repo] ?? EMPTY);
}
