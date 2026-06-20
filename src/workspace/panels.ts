// Panel type registry for the dockview workspace. Panel ids are stable strings
// used both as dockview component keys and as serialized-layout identifiers, so
// later items (RDM-008+) can register new panel types without reworking the
// shell. Per-repo panels are addressed by `${PANEL_REPO}:${canonicalPath}` so
// opening the same repo twice can dedup to one panel (RDM-007 U5).

export const PANEL_DASHBOARD = "dashboard";
export const PANEL_TREE = "tree";
export const PANEL_REPO = "repo";
export const PANEL_TIMELINE = "timeline";
export const PANEL_AGENT_TERMINAL = "agent-terminal";

/** Custom dockview tab for project panels (shows a change indicator). */
export const TAB_REPO = "repoTab";

/** Panel id for a repo's detail panel, keyed by its canonical path. */
export function repoPanelId(canonicalPath: string): string {
  return `${PANEL_REPO}:${canonicalPath}`;
}

/** The repo path encoded in a repo panel id, or null if not a repo panel. */
export function repoPathFromPanelId(panelId: string): string | null {
  return panelId.startsWith(`${PANEL_REPO}:`) ? panelId.slice(PANEL_REPO.length + 1) : null;
}

export function agentTerminalPanelId(sessionId: string): string {
  return `${PANEL_AGENT_TERMINAL}:${sessionId}`;
}

export function sessionIdFromAgentTerminalPanelId(panelId: string): string | null {
  return panelId.startsWith(`${PANEL_AGENT_TERMINAL}:`)
    ? panelId.slice(PANEL_AGENT_TERMINAL.length + 1)
    : null;
}
