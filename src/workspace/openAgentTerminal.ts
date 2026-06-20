import type { DockviewApi } from "dockview-react";
import { PANEL_AGENT_TERMINAL, agentTerminalPanelId } from "./panels";
import type { TerminalPanelParams } from "../panels/terminal/TerminalPanel";

export type AgentTerminalOpenParams = TerminalPanelParams;

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
    });
  } catch {
    api.getPanel(id)?.api.setActive();
  }
}

function terminalTitle(params: AgentTerminalOpenParams): string {
  const agent = params.agentType ?? "agent";
  const short = params.sessionId.length > 8 ? params.sessionId.slice(0, 8) : params.sessionId;
  return `${agent} ${short}`;
}
