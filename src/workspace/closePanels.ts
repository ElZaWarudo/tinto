import type { DockviewApi } from "dockview-react";
import { diffPanelId, repoPanelId } from "./panels";

export function closePanelsForRemovedRepo(api: DockviewApi, path: string): void {
  api.getPanel(repoPanelId(path))?.api.close();
  const prefix = diffPanelId(path, "");
  for (const panel of api.panels) {
    if (panel.id.startsWith(prefix)) panel.api.close();
  }
}
