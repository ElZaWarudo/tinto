import type { DockviewReadyEvent, DockviewWillDropEvent, IDockviewPanel } from "dockview-react";
import { sessionIdFromAgentTerminalPanelId } from "../../workspace/panels";
import type { TerminalPanelParams } from "./TerminalPanel";
import { markTerminalDetached, openDetachedTerminalWindow } from "./detachTerminalWindow";

const detachingTerminals = new Set<string>();

export async function detachTerminalFromConsoleDrop(
  event: DockviewWillDropEvent,
  api: DockviewReadyEvent["api"] | null,
): Promise<boolean> {
  if (!api || event.kind !== "edge") return false;
  const panelId = event.getData()?.panelId;
  const sessionId = panelId ? sessionIdFromAgentTerminalPanelId(panelId) : null;
  if (!panelId || !sessionId) return false;

  const panel = api.getPanel(panelId);
  event.preventDefault();
  return detachTerminalPanel(api, panelId, panel);
}

export async function detachTerminalPanel(
  api: DockviewReadyEvent["api"],
  panelId: string,
  panel = api.getPanel(panelId),
): Promise<boolean> {
  const sessionId = sessionIdFromAgentTerminalPanelId(panelId);
  if (!sessionId || detachingTerminals.has(panelId)) return false;

  detachingTerminals.add(panelId);
  try {
    const params = terminalParamsFromPanel(panel, sessionId);
    const opened = await openDetachedTerminalWindow(params);
    if (!opened) return false;

    markTerminalDetached(sessionId);
    const current = api.getPanel(panelId);
    if (current) {
      api.removePanel(current);
    }
    return true;
  } finally {
    detachingTerminals.delete(panelId);
  }
}

function terminalParamsFromPanel(
  panel: IDockviewPanel | undefined,
  sessionId: string,
): TerminalPanelParams {
  const params = panel?.params as Partial<TerminalPanelParams> | undefined;
  return {
    sessionId,
    repo: typeof params?.repo === "string" ? params.repo : undefined,
    agentType: typeof params?.agentType === "string" ? params.agentType : undefined,
  };
}
