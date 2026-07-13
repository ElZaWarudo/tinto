import type { DockviewApi } from "dockview-react";
import { PANEL_AGENT_CONSOLES } from "./panels";
import type { TerminalPanelParams } from "../panels/terminal/TerminalPanel";
import { consoleDock } from "./consoleDock";
import { sendTerminalToDetachedConsoles } from "../panels/terminal/detachTerminalWindow";

export type AgentTerminalOpenParams = TerminalPanelParams;

// Below the same breakpoint used by the agent surface, two independently
// docked workspace groups no longer leave enough room for both the session
// navigator and the terminal. Compact mode temporarily expands the Agents
// group; wider windows keep the user's split layout intact.
const COMPACT_AGENT_WORKSPACE_MAX_WIDTH = 900;

/**
 * Keep the Agents surface usable when it has been docked into a narrow split.
 * Maximizing is reversible and preserves the user's split geometry; moving the
 * panel between two populated groups would still leave it at roughly half the
 * compact window width.
 */
export function ensureCompactAgentWorkspace(api: DockviewApi): boolean {
  const workspaceWidth = api.width;
  if (
    !Number.isFinite(workspaceWidth) ||
    workspaceWidth <= 0 ||
    workspaceWidth > COMPACT_AGENT_WORKSPACE_MAX_WIDTH
  ) {
    return false;
  }

  const agents = api.getPanel(PANEL_AGENT_CONSOLES);
  if (!agents || agents.api.location.type !== "grid") return false;

  const hasSiblingGridGroup = api.panels.some(
    (panel) =>
      panel.group.id !== agents.group.id &&
      panel.api.location.type === "grid" &&
      panel.group.api.location.type === "grid",
  );
  if (!hasSiblingGridGroup) return agents.api.isMaximized();

  if (!agents.api.isMaximized()) {
    agents.api.maximize();
  }
  return agents.api.isMaximized();
}

export function openAgentTerminalPanel(api: DockviewApi, params: AgentTerminalOpenParams): void {
  void sendTerminalToDetachedConsoles(params).then((routed) => {
    if (routed) return;
    openAgentConsolesPanel(api);
    consoleDock.openTerminal(params);
  });
}

export function openAgentConsolesPanel(api: DockviewApi): void {
  const existing = api.getPanel(PANEL_AGENT_CONSOLES);
  if (existing) {
    ensureCompactAgentWorkspace(api);
    existing.api.setActive();
    return;
  }

  const referencePanel = api.activePanel;
  try {
    const panel = api.addPanel({
      id: PANEL_AGENT_CONSOLES,
      component: PANEL_AGENT_CONSOLES,
      title: "Agents",
      ...(referencePanel ? { position: { referencePanel, direction: "within" as const } } : {}),
    });
    ensureCompactAgentWorkspace(api);
    panel.api.setActive();
  } catch {
    api.getPanel(PANEL_AGENT_CONSOLES)?.api.setActive();
  }
}
