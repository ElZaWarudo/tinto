import type { DockviewApi } from "dockview-react";
import { PANEL_DASHBOARD } from "./panels";

// The Dashboard is the default, always-available tab: focus it if open, else
// (re)create it. Mirrors the empty-workspace guard so "Open Dashboard" always
// lands somewhere even after the user closed it.
export function openDashboardPanel(api: DockviewApi): void {
  const existing = api.getPanel(PANEL_DASHBOARD);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    api.addPanel({ id: PANEL_DASHBOARD, component: PANEL_DASHBOARD, title: "Dashboard" });
  } catch {
    api.getPanel(PANEL_DASHBOARD)?.api.setActive();
  }
}
