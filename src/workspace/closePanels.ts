import type { DockviewApi } from "dockview-react";
import { repoPanelId } from "./panels";

// Close a removed repo's project tab. Its nested file tabs live in tabsStore
// (cleared by the caller via tabsStore.closeRepo), not as dockview panels.
export function closePanelsForRemovedRepo(api: DockviewApi, path: string): void {
  api.getPanel(repoPanelId(path))?.api.close();
}
