import type {
  DockviewApi,
  DockviewWillDropEvent,
  IDockviewPanel,
  TabDragEvent,
} from "dockview-react";
import {
  markTerminalDetached,
  openDetachedConsolesWindow,
} from "../panels/terminal/detachTerminalWindow";
import { consoleDock } from "./consoleDock";
import { armExternalTabDetach } from "./externalTabDetach";
import { PANEL_AGENT_CONSOLES } from "./panels";

const detachingConsolesPanels = new Set<string>();

export async function detachConsolesPanelFromWorkspaceDrop(
  event: DockviewWillDropEvent,
  api: DockviewApi,
): Promise<boolean> {
  if (event.kind !== "edge" || event.getData()?.panelId !== PANEL_AGENT_CONSOLES) {
    return false;
  }
  const panel = api.getPanel(PANEL_AGENT_CONSOLES);
  if (!panel) return false;

  event.preventDefault();
  return detachConsolesPanel(api, panel);
}

export async function detachConsolesPanel(
  api: DockviewApi,
  panel: IDockviewPanel | undefined = api.getPanel(PANEL_AGENT_CONSOLES),
): Promise<boolean> {
  if (!panel || detachingConsolesPanels.has(panel.id)) return false;

  detachingConsolesPanels.add(panel.id);
  const terminalParams = consoleDock.openTerminalParams();
  consoleDock.prepareDetachedTransfer();
  try {
    const opened = await openDetachedConsolesWindow(terminalParams);
    if (!opened) return false;

    for (const sessionId of consoleDock.openTerminalSessionIds()) {
      markTerminalDetached(sessionId);
    }
    const current = api.getPanel(panel.id);
    if (current) {
      api.removePanel(current);
    }
    return true;
  } finally {
    detachingConsolesPanels.delete(panel.id);
  }
}

export function armConsolesExternalDetach(event: TabDragEvent, api: DockviewApi): boolean {
  if (event.panel.id !== PANEL_AGENT_CONSOLES) return false;
  armExternalTabDetach(event.nativeEvent, () => detachConsolesPanel(api, event.panel));
  return true;
}
