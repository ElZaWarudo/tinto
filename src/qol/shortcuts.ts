// Global keyboard shortcuts for navigation and commands. Centralizes all
// keybindings so they can be installed once on app mount and listed in the
// shortcuts modal. Uses the same pattern as zoom.ts: a store-like module with
// an install function that returns a cleanup.

import type { DockviewApi } from "dockview-react";
import { fileDock } from "../workspace/fileDock";
import { getExplorerCollapsed, setExplorerCollapsed } from "../panels/tree/explorerCollapseState";
import { deleteUndoManager } from "../panels/file/deleteUndo";
import { qualityStore } from "./state";
import { retryRepo } from "../bus/client";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform);
const mod = isMac ? "⌘" : "Ctrl";

export interface ShortcutDef {
  action: string;
  keys: string;
  group: string;
}

export const SHORTCUTS: ShortcutDef[] = [
  // Navegación
  { action: "Alternar árbol de archivos", keys: `${mod} B`, group: "Navegación" },
  { action: "Siguiente proyecto", keys: `${mod} PageDown`, group: "Navegación" },
  { action: "Proyecto anterior", keys: `${mod} PageUp`, group: "Navegación" },
  { action: "Siguiente archivo", keys: `${mod} Tab`, group: "Navegación" },
  { action: "Archivo anterior", keys: `${mod} Shift Tab`, group: "Navegación" },

  // Cerrar
  { action: "Cerrar archivo", keys: `${mod} W`, group: "Cerrar" },
  { action: "Cerrar proyecto", keys: `${mod} Shift W`, group: "Cerrar" },

  // Vista
  { action: "Glance mode", keys: `${mod} Shift G`, group: "Vista" },
  { action: "Abrir Dashboard", keys: `${mod} Shift D`, group: "Vista" },
  { action: "Abrir Timeline", keys: `${mod} Shift H`, group: "Vista" },

  // Proyecto
  { action: "Refrescar proyecto", keys: `${mod} R`, group: "Proyecto" },
  { action: "Añadir proyecto", keys: `${mod} Shift A`, group: "Proyecto" },
  { action: "Restaurar archivo eliminado", keys: `${mod} Z`, group: "Archivos" },
  { action: "Rehacer eliminación de archivo", keys: `${mod} Shift Z`, group: "Archivos" },
];

/** Get the currently active project repo path from the dockview api. */
function getActiveRepo(api: DockviewApi): string | null {
  const active = api.activePanel;
  if (!active) return null;
  const repo = (active.params as { repo?: string } | undefined)?.repo;
  return typeof repo === "string" ? repo : null;
}

/** Get all open project repo paths in order. */
function getOpenRepos(api: DockviewApi): string[] {
  return api.panels
    .map((p) => (p.params as { repo?: string } | undefined)?.repo)
    .filter((r): r is string => typeof r === "string");
}

/** Navigate to the next/previous project tab. */
function navigateProject(api: DockviewApi, direction: 1 | -1): void {
  const repos = getOpenRepos(api);
  if (repos.length === 0) return;

  const activeRepo = getActiveRepo(api);
  const currentIdx = activeRepo ? repos.indexOf(activeRepo) : -1;
  const nextIdx = (currentIdx + direction + repos.length) % repos.length;
  const targetRepo = repos[nextIdx];

  // Find and activate the panel for this repo
  for (const panel of api.panels) {
    const repo = (panel.params as { repo?: string } | undefined)?.repo;
    if (repo === targetRepo) {
      panel.api.setActive();
      return;
    }
  }
}

/** Navigate to the next/previous file in the active project. */
function navigateFile(api: DockviewApi, direction: 1 | -1): void {
  const repo = getActiveRepo(api);
  if (!repo) return;

  const state = fileDock.getState(repo);
  if (state.open.length === 0) return;

  const currentIdx = state.active ? state.open.indexOf(state.active) : -1;
  const nextIdx = (currentIdx + direction + state.open.length) % state.open.length;
  const targetPath = state.open[nextIdx];

  fileDock.openFile(repo, targetPath, true);
}

/** Close the active file in the active project. */
function closeActiveFile(api: DockviewApi): void {
  const repo = getActiveRepo(api);
  if (!repo) return;

  const state = fileDock.getState(repo);
  if (!state.active) return;

  // The file panel ID is file:<path>
  const panelId = `file:${state.active}`;
  const entry = (
    fileDock as unknown as { entries: Map<string, { api: DockviewApi | null }> }
  ).entries.get(repo);
  if (entry?.api) {
    const panel = entry.api.getPanel(panelId);
    if (panel) {
      panel.api.close();
    }
  }
}

/** Close the active project tab. */
function closeActiveProject(api: DockviewApi): void {
  const active = api.activePanel;
  if (!active) return;

  const repo = (active.params as { repo?: string } | undefined)?.repo;
  if (!repo) return;

  active.api.close();
}

/** Toggle the explorer tree for the active project. */
function toggleExplorer(api: DockviewApi): void {
  const repo = getActiveRepo(api);
  if (!repo) return;

  const current = getExplorerCollapsed(repo);
  setExplorerCollapsed(repo, !current);
}

/** Refresh the active project's git status. */
function refreshActiveRepo(api: DockviewApi): void {
  const repo = getActiveRepo(api);
  if (!repo) return;

  void retryRepo(repo);
}

export interface ShortcutActions {
  openDashboard: () => void;
  openTimeline: () => void;
  addRepo: () => void;
}

/** Install all global keyboard shortcuts. Returns a cleanup function. */
export function installShortcuts(
  apiRef: { current: DockviewApi | null },
  actions: ShortcutActions,
): () => void {
  const handler = (e: KeyboardEvent) => {
    if (e.defaultPrevented) return;
    // Ignore if user is typing in an input/textarea
    const target = e.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable) {
      return;
    }

    const api = apiRef.current;
    if (!api) return;

    const modPressed = e.ctrlKey || e.metaKey;
    if (!modPressed) return;

    // Undo/redo file deletion: Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z
    if (e.key === "z" || e.key === "Z") {
      e.preventDefault();
      void (e.shiftKey ? deleteUndoManager.redo() : deleteUndoManager.undo()).then((report) => {
        if (report?.fatalError) console.warn("tinto: file undo failed", report.fatalError);
      });
      return;
    }

    // Toggle explorer: Ctrl/Cmd+B
    if (e.key === "b" || e.key === "B") {
      e.preventDefault();
      toggleExplorer(api);
      return;
    }

    // Next project: Ctrl/Cmd+PageDown
    if (e.key === "PageDown") {
      e.preventDefault();
      navigateProject(api, 1);
      return;
    }

    // Previous project: Ctrl/Cmd+PageUp
    if (e.key === "PageUp") {
      e.preventDefault();
      navigateProject(api, -1);
      return;
    }

    // Next file: Ctrl/Cmd+Tab
    if (e.key === "Tab" && !e.shiftKey) {
      e.preventDefault();
      navigateFile(api, 1);
      return;
    }

    // Previous file: Ctrl/Cmd+Shift+Tab
    if (e.key === "Tab" && e.shiftKey) {
      e.preventDefault();
      navigateFile(api, -1);
      return;
    }

    // Close file: Ctrl/Cmd+W (without Shift)
    if ((e.key === "w" || e.key === "W") && !e.shiftKey) {
      e.preventDefault();
      closeActiveFile(api);
      return;
    }

    // Close project: Ctrl/Cmd+Shift+W
    if ((e.key === "w" || e.key === "W") && e.shiftKey) {
      e.preventDefault();
      closeActiveProject(api);
      return;
    }

    // Toggle Glance Mode: Ctrl/Cmd+Shift+G
    if ((e.key === "g" || e.key === "G") && e.shiftKey) {
      e.preventDefault();
      qualityStore.setGlanceMode(!qualityStore.getState().glanceMode);
      return;
    }

    // Open Dashboard: Ctrl/Cmd+Shift+D
    if ((e.key === "d" || e.key === "D") && e.shiftKey) {
      e.preventDefault();
      actions.openDashboard();
      return;
    }

    // Open Timeline: Ctrl/Cmd+Shift+H
    if ((e.key === "h" || e.key === "H") && e.shiftKey) {
      e.preventDefault();
      actions.openTimeline();
      return;
    }

    // Refresh repo: Ctrl/Cmd+R
    if (e.key === "r" || e.key === "R") {
      e.preventDefault();
      refreshActiveRepo(api);
      return;
    }

    // Add repo: Ctrl/Cmd+Shift+A
    if ((e.key === "a" || e.key === "A") && e.shiftKey) {
      e.preventDefault();
      actions.addRepo();
      return;
    }
  };

  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}
