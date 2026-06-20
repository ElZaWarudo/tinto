import type { DockviewApi } from "dockview-react";
import {
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "./panels";
import type { TerminalPanelParams } from "../panels/terminal/TerminalPanel";

export type AgentTerminalOpenParams = TerminalPanelParams;
type TerminalPanelPosition = {
  direction: "right" | "below";
  referencePanel: string;
};

export function openAgentTerminalPanel(api: DockviewApi, params: AgentTerminalOpenParams): void {
  const id = agentTerminalPanelId(params.sessionId);
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    api.addPanel({
      id,
      component: PANEL_AGENT_TERMINAL,
      title: terminalTitle(params),
      params,
      position: terminalLayoutPosition(api),
    });
  } catch {
    api.getPanel(id)?.api.setActive();
  }
}

export function terminalLayoutPosition(api: DockviewApi): TerminalPanelPosition | undefined {
  const panels = api.panels ?? [];
  const terminalPanels = panels.filter((panel) => sessionIdFromAgentTerminalPanelId(panel.id));
  if (terminalPanels.length === 0) {
    const reference = api.activePanel ?? panels[panels.length - 1];
    return reference ? { direction: "right", referencePanel: reference.id } : undefined;
  }
  if (terminalPanels.length === 1) {
    return { direction: "below", referencePanel: terminalPanels[0].id };
  }
  return {
    direction: "right",
    referencePanel: terminalPanels[terminalPanels.length - 1].id,
  };
}

function terminalTitle(params: AgentTerminalOpenParams): string {
  const agent = params.agentType ?? "agent";
  const short = params.sessionId.length > 8 ? params.sessionId.slice(0, 8) : params.sessionId;
  return `${agent} ${short}`;
}
