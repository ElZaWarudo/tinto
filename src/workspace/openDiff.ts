// Opening a diff panel: dedup by (repo, path). If a panel for the target is
// already open, focus it; otherwise add a new one. The id encodes the target so
// a restored layout reopens the same diff and a target never opens twice.
// Mirrors openRepo.ts.

import type { DockviewApi } from "dockview-react";
import { PANEL_DIFF, diffPanelId } from "./panels";

export function openDiffPanel(api: DockviewApi, repo: string, path: string, title: string): void {
  const id = diffPanelId(repo, path);
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    api.addPanel({ id, component: PANEL_DIFF, title, params: { repo, path } });
  } catch {
    // Lost a race with a restored layout that already holds this id — focus it.
    api.getPanel(id)?.api.setActive();
  }
}
