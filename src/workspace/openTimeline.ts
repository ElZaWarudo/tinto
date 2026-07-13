import type { DockviewApi } from "dockview-react";
import { PANEL_TIMELINE } from "./panels";

export function openTimelinePanel(api: DockviewApi): void {
  const existing = api.getPanel(PANEL_TIMELINE);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    api.addPanel({ id: PANEL_TIMELINE, component: PANEL_TIMELINE, title: "Cronología" });
  } catch {
    api.getPanel(PANEL_TIMELINE)?.api.setActive();
  }
}
