// Custom dockview tab for project (level-1) panels: the default tab plus a
// change dot when the repo has working-tree changes. Subscribes to the live bus
// state, so a project tab that starts detecting changes lights up even while
// it is NOT the active tab.

import { DockviewDefaultTab, type IDockviewPanelHeaderProps } from "dockview-react";
import type { RepoDelta } from "../bus/contract";
import { useBusState } from "../bus/store";

function repoHasChanges(delta: RepoDelta | undefined): boolean {
  const s = delta?.status;
  return !!s && s.modified.length + s.staged.length + s.untracked.length > 0;
}

export function RepoTab(props: IDockviewPanelHeaderProps) {
  const repo = (props.params as { repo?: string }).repo;
  const state = useBusState();
  const changed = repoHasChanges(repo ? state.repos[repo] : undefined);

  return (
    <div className="repo-tab">
      {/* The dot slot is always present (fixed width) so the title never shifts
          when the change indicator toggles on/off. */}
      <span
        className={changed ? "repo-tab__dot repo-tab__dot--on" : "repo-tab__dot"}
        data-testid={changed ? `repo-tab-changed-${repo}` : undefined}
        title={changed ? "Cambios sin ver" : undefined}
      />
      <DockviewDefaultTab {...props} />
    </div>
  );
}
