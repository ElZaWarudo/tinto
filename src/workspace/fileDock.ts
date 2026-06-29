// Per-project nested dock: each project (level-1 tab) hosts its own dockview
// for file panels, so files can be dragged into splits and two can sit on screen
// at once. This registry bridges the app to each project's nested DockviewApi:
//   - register/unregister on mount/unmount,
//   - openFile with VS Code preview/pin semantics layered on top of dockview
//     (a single reused "preview" panel; double-click/pin promotes to a permanent
//     panel),
//   - a subscribable mirror of each repo's open paths + active + preview so the
//     explorer can highlight the active file and the panel can show the overview
//     when no files are open.

import { useSyncExternalStore } from "react";
import type { DockviewApi, SerializedDockview } from "dockview-react";

export const FILE_PREVIEW_ID = "__preview__";
export const filePanelId = (path: string) => `file:${path}`;

const SAVE_DEBOUNCE_MS = 400;
const storageKey = (repo: string) => `tinto:filedock:${repo}`;

function basename(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i < 0 ? path : path.slice(i + 1);
}

/** A persisted layout is worth restoring only if it has at least one panel. */
function hasPanels(layout: SerializedDockview | null): boolean {
  const panels = (layout as { panels?: object } | null)?.panels;
  return !!panels && Object.keys(panels).length > 0;
}

function loadLayout(repo: string): SerializedDockview | null {
  try {
    const raw = localStorage.getItem(storageKey(repo));
    return raw ? (JSON.parse(raw) as SerializedDockview) : null;
  } catch {
    return null;
  }
}
function saveLayout(repo: string, layout: SerializedDockview): void {
  try {
    localStorage.setItem(storageKey(repo), JSON.stringify(layout));
  } catch {
    /* storage unavailable / quota — keep the in-memory layout */
  }
}
function clearLayout(repo: string): void {
  try {
    localStorage.removeItem(storageKey(repo));
  } catch {
    /* ignore */
  }
}

export interface RepoDockState {
  open: string[]; // open file paths (pinned + the preview)
  active: string | null;
  preview: string | null;
}
const EMPTY: RepoDockState = { open: [], active: null, preview: null };

interface Entry {
  api: DockviewApi | null;
  preview: string | null;
  pending: Array<{ path: string; pin: boolean }>;
  state: RepoDockState;
  disposers: Array<() => void>;
  saveTimer: ReturnType<typeof setTimeout> | null;
}

class FileDock {
  private entries = new Map<string, Entry>();
  private listeners = new Set<() => void>();

  subscribe = (l: () => void): (() => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };
  private emit() {
    this.listeners.forEach((l) => l());
  }

  private ensure(repo: string): Entry {
    let e = this.entries.get(repo);
    if (!e) {
      e = { api: null, preview: null, pending: [], state: EMPTY, disposers: [], saveTimer: null };
      this.entries.set(repo, e);
    }
    return e;
  }

  /** Debounced persist of the repo's nested layout to localStorage. */
  private scheduleSave(repo: string) {
    const e = this.entries.get(repo);
    if (!e || !e.api) return;
    if (e.saveTimer) clearTimeout(e.saveTimer);
    e.saveTimer = setTimeout(() => {
      if (e.api) saveLayout(repo, e.api.toJSON());
    }, SAVE_DEBOUNCE_MS);
  }

  getState(repo: string): RepoDockState {
    return this.entries.get(repo)?.state ?? EMPTY;
  }

  private pathOf(panel: { params?: unknown } | null | undefined): string | null {
    const p = (panel?.params as { path?: string } | undefined)?.path;
    return typeof p === "string" ? p : null;
  }

  private recompute(repo: string) {
    const e = this.entries.get(repo);
    if (!e || !e.api) return;
    const open: string[] = [];
    for (const panel of e.api.panels) {
      const p = this.pathOf(panel);
      if (p) open.push(p);
    }
    const active = this.pathOf(e.api.activePanel);
    e.state = { open, active, preview: e.preview };
    this.emit();
  }

  /** Bind a project's nested dockview api: restore the persisted layout, drain
   * anything opened before mount, and keep persisting on every layout change. */
  register(repo: string, api: DockviewApi) {
    const e = this.ensure(repo);
    e.disposers.forEach((d) => d());
    e.api = api;
    const d1 = api.onDidAddPanel(() => this.recompute(repo));
    const d2 = api.onDidRemovePanel((panel) => {
      // If the preview panel was closed, forget the preview marker.
      if (panel.id === FILE_PREVIEW_ID) e.preview = null;
      this.recompute(repo);
    });
    const d3 = api.onDidActivePanelChange(() => this.recompute(repo));
    const d4 = api.onDidLayoutChange(() => this.scheduleSave(repo));
    e.disposers = [() => d1.dispose(), () => d2.dispose(), () => d3.dispose(), () => d4.dispose()];

    // Restore the persisted file layout (open files + splits) from last session.
    const saved = loadLayout(repo);
    if (hasPanels(saved)) {
      try {
        api.fromJSON(saved as SerializedDockview);
      } catch {
        clearLayout(repo); // corrupt / incompatible — start fresh
      }
    }
    // The preview marker is derived from the restored preview panel, if any.
    e.preview = this.pathOf(api.getPanel(FILE_PREVIEW_ID));

    const queued = e.pending;
    e.pending = [];
    for (const { path, pin } of queued) this.openFile(repo, path, pin);
    this.recompute(repo);
  }

  /** The project tab unmounted: flush the layout, drop the live api but keep the
   * persisted layout so reopening the project restores its files. */
  unregister(repo: string) {
    const e = this.entries.get(repo);
    if (!e) return;
    if (e.saveTimer) clearTimeout(e.saveTimer);
    e.saveTimer = null;
    if (e.api) {
      try {
        saveLayout(repo, e.api.toJSON());
      } catch {
        /* api already torn down — the debounced saves captured the state */
      }
    }
    e.disposers.forEach((d) => d());
    e.disposers = [];
    e.api = null;
    e.preview = null;
    e.state = EMPTY;
    this.emit();
  }

  /** The repo left the workbench: forget it entirely (and its persisted layout). */
  drop(repo: string) {
    const e = this.entries.get(repo);
    if (e) {
      if (e.saveTimer) clearTimeout(e.saveTimer);
      e.disposers.forEach((d) => d());
      this.entries.delete(repo);
      this.emit();
    }
    clearLayout(repo);
  }

  private add(api: DockviewApi, id: string, repo: string, path: string, split = false) {
    const activePath = this.pathOf(api.activePanel);
    const position =
      split && activePath && api.activePanel
        ? { direction: "right" as const, referencePanel: api.activePanel.id }
        : undefined;
    try {
      api.addPanel({
        id,
        component: "file",
        tabComponent: "fileTab",
        title: basename(path),
        params: { repo, path },
        position,
      });
    } catch {
      api.getPanel(id)?.api.setActive();
    }
  }

  /** Open a file in the repo's nested dock. `pin` false previews it in the
   * single reused slot; `pin` true makes it a permanent split-able panel. */
  openFile(repo: string, path: string, pin: boolean) {
    const e = this.ensure(repo);
    if (!e.api) {
      e.pending.push({ path, pin });
      return;
    }
    const api = e.api;

    const pinned = api.getPanel(filePanelId(path));
    if (pinned) {
      pinned.api.setActive();
      if (e.preview === path) {
        e.preview = null;
        this.recompute(repo);
      }
      return;
    }

    if (pin) {
      if (e.preview === path) {
        // Promote the current preview to a permanent panel.
        api.getPanel(FILE_PREVIEW_ID)?.api.close();
        e.preview = null;
      }
      this.add(api, filePanelId(path), repo, path, true);
      return;
    }

    // Preview (single click): reuse the one preview slot.
    const prev = api.getPanel(FILE_PREVIEW_ID);
    if (prev) {
      if (e.preview === path) {
        prev.api.setActive();
      } else {
        prev.api.close();
        this.add(api, FILE_PREVIEW_ID, repo, path);
      }
    } else {
      this.add(api, FILE_PREVIEW_ID, repo, path);
    }
    e.preview = path;
    this.recompute(repo);
  }
}

export const fileDock = new FileDock();

/** Subscribe to a repo's nested-dock state (open paths, active, preview). */
export function useRepoDock(repo: string): RepoDockState {
  return useSyncExternalStore(fileDock.subscribe, () => fileDock.getState(repo));
}
