import type { DockviewApi } from "dockview-react";
import { PANEL_AGENT_CONSOLES } from "./panels";
import type { TerminalPanelParams } from "../panels/terminal/TerminalPanel";
import { consoleDock } from "./consoleDock";

export type AgentTerminalOpenParams = TerminalPanelParams;

export function openAgentTerminalPanel(api: DockviewApi, params: AgentTerminalOpenParams): void {
  openAgentConsolesPanel(api);
  consoleDock.openTerminal(params);
}

export function openAgentConsolesPanel(api: DockviewApi): void {
  const existing = api.getPanel(PANEL_AGENT_CONSOLES);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    const panel = api.addPanel({
      id: PANEL_AGENT_CONSOLES,
      component: PANEL_AGENT_CONSOLES,
      title: "Consoles",
    });
    panel.api.setActive();
  } catch {
    api.getPanel(PANEL_AGENT_CONSOLES)?.api.setActive();
  }
}
