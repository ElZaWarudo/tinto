// Opening a repo panel: dedup by canonical path. If a panel for the repo is
// already open, focus it; otherwise add a new one. Panel id encodes the path so
// a restored layout reopens the same repo, and a repo can never open twice.

import type { DockviewApi } from "dockview-react";
import { PANEL_REPO, repoPanelId } from "./panels";

export function openRepoPanel(api: DockviewApi, repo: string, title: string): void {
  const id = repoPanelId(repo);
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  api.addPanel({
    id,
    component: PANEL_REPO,
    title,
    params: { repo },
  });
}
