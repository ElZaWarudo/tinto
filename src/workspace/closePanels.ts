import type { DockviewApi } from "dockview-react";
import { repoPanelId } from "./panels";

// Close a removed repo's project tab. Its nested file panels live in the
// project's own dockview (dropped by the caller via fileDock.drop).
export function closePanelsForRemovedRepo(api: DockviewApi, path: string): void {
  api.getPanel(repoPanelId(path))?.api.close();
}
