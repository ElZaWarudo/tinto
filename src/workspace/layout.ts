// Workspace layout: persistence glue + default layout + restore fallback.
// The pure helpers (safeParseLayout, isUsableLayout) are unit-tested; the
// dockview wiring (buildDefaultLayout, applyLayout) runs against a live api and
// is validated by the `tauri dev` smoke gate, since jsdom cannot measure
// dockview's DOM layout reliably.

import { invoke } from "@tauri-apps/api/core";
import type { DockviewApi, SerializedDockview } from "dockview-react";
import { PANEL_AGENT_CONSOLES, PANEL_AGENT_TERMINAL, PANEL_DASHBOARD, PANEL_TREE } from "./panels";

/** True if a persisted layout still references the legacy in-dock repo tree,
 * which is now a fixed left sidebar. Such layouts must be discarded: dockview
 * would fail to instantiate the unregistered `tree` component. */
export function layoutReferencesTree(layout: SerializedDockview | null): boolean {
  if (!layout) return false;
  const panels = (layout as { panels?: Record<string, { contentComponent?: string }> }).panels;
  if (!panels) return false;
  return Object.values(panels).some((p) => p?.contentComponent === PANEL_TREE);
}

export function layoutReferencesEphemeralConsoles(layout: SerializedDockview | null): boolean {
  if (!layout) return false;
  const panels = (layout as { panels?: Record<string, { contentComponent?: string }> }).panels;
  if (!panels) return false;
  return Object.entries(panels).some(
    ([id, panel]) =>
      id === PANEL_AGENT_CONSOLES ||
      id.startsWith(`${PANEL_AGENT_TERMINAL}:`) ||
      panel?.contentComponent === PANEL_AGENT_CONSOLES ||
      panel?.contentComponent === PANEL_AGENT_TERMINAL,
  );
}

/** Parse a persisted layout string; returns null on null/empty/corrupt input. */
export function safeParseLayout(json: string | null | undefined): SerializedDockview | null {
  if (!json) return null;
  try {
    const parsed: unknown = JSON.parse(json);
    return parsed && typeof parsed === "object" ? (parsed as SerializedDockview) : null;
  } catch {
    return null;
  }
}

/** A layout is usable only if it has at least one serialized panel. */
export function isUsableLayout(layout: SerializedDockview | null): boolean {
  if (!layout) return false;
  const panels = (layout as { panels?: unknown }).panels;
  return !!panels && typeof panels === "object" && Object.keys(panels).length > 0;
}

/** Read the persisted UI layout from the backend, tolerant of absence/errors. */
export async function loadUiState(): Promise<SerializedDockview | null> {
  try {
    const raw = await invoke<string | null>("get_ui_state");
    return safeParseLayout(raw ?? null);
  } catch {
    return null;
  }
}

/** Persist the UI layout; write failures are logged and swallowed (prototype). */
export async function saveUiState(layout: SerializedDockview): Promise<void> {
  try {
    await invoke("set_ui_state", { state: JSON.stringify(layout) });
  } catch (e) {
    console.warn("tinto: failed to persist UI layout", e);
  }
}

/** Build the first-run default layout: the Dashboard as the single open tab.
 * The repo/file explorer now lives in the fixed left sidebar, outside the dock. */
export function buildDefaultLayout(api: DockviewApi): void {
  api.clear();
  api.addPanel({ id: PANEL_DASHBOARD, component: PANEL_DASHBOARD, title: "Dashboard" });
}

/** Restore a persisted layout, falling back to the default if unusable or if it
 * still references the legacy in-dock repo tree (now a fixed sidebar). */
export function applyLayout(api: DockviewApi, layout: SerializedDockview | null): void {
  if (
    layout &&
    isUsableLayout(layout) &&
    !layoutReferencesTree(layout) &&
    !layoutReferencesEphemeralConsoles(layout)
  ) {
    try {
      api.fromJSON(layout);
      return;
    } catch (e) {
      console.warn("tinto: corrupt saved layout, using default", e);
    }
  }
  buildDefaultLayout(api);
}
