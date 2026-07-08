import type { DockviewApi } from "dockview-react";
import type { WorkbenchConfig } from "../bus/contract";
import { repoPanelId, repoPathFromPanelId } from "./panels";

// Close a removed repo's project tab. Its nested file panels live in the
// project's own dockview (dropped by the caller via fileDock.drop).
export function closePanelsForRemovedRepo(api: DockviewApi, path: string): void {
  api.getPanel(repoPanelId(path))?.api.close();
}

function activeWorkbenchRepoPaths(config: WorkbenchConfig | null | undefined): Set<string> | null {
  if (!config?.active) return null;
  const active = (config.workbenches ?? []).find((workbench) => workbench.name === config.active);
  if (!active) return null;
  return new Set(active.repos.map((repo) => repo.path));
}

// Close repo project tabs that no longer belong to the active workbench. This
// is intentionally config-based so workbench switches do not leave stale repo
// file/diff tabs open while still preserving repos shared by both workbenches.
export function closePanelsOutsideActiveWorkbench(
  api: DockviewApi,
  config: WorkbenchConfig | null | undefined,
): string[] {
  const activeRepos = activeWorkbenchRepoPaths(config);
  if (!activeRepos) return [];

  const closed: string[] = [];
  for (const panel of [...api.panels]) {
    const repo = repoPathFromPanelId(panel.id);
    if (!repo || activeRepos.has(repo)) continue;
    panel.api.close();
    closed.push(repo);
  }
  return closed;
}
