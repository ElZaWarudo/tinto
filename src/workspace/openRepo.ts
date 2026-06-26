// Opening a repo panel: dedup by canonical path. If a panel for the repo is
// already open, focus it; otherwise add a new one. Panel id encodes the path so
// a restored layout reopens the same repo, and a repo can never open twice.

import type { DockviewApi } from "dockview-react";
import { PANEL_REPO, TAB_REPO, repoPanelId } from "./panels";

function splitReferencePanelId(api: DockviewApi): string | undefined {
  if (api.activePanel?.id.startsWith("repo:")) return api.activePanel.id;
  return api.panels.find((panel) => panel.id.startsWith("repo:"))?.id;
}

export function openRepoPanel(api: DockviewApi, repo: string, title: string): void {
  const id = repoPanelId(repo);
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  const referencePanel = splitReferencePanelId(api);
  const position = referencePanel ? { direction: "right" as const, referencePanel } : undefined;
  try {
    api.addPanel({
      id,
      component: PANEL_REPO,
      tabComponent: TAB_REPO,
      title,
      params: { repo },
      position,
    });
  } catch {
    // Lost a race with a restored layout that already holds this id — focus it.
    api.getPanel(id)?.api.setActive();
  }
}
