// App-level actions panels invoke (open a repo panel, start the add-repo flow).
// Provided above the dockview host; dockview renders panels through React
// portals, which preserve context, so panel components can consume this.

import { createContext, useContext } from "react";

export interface WorkspaceActions {
  openRepo: (canonicalPath: string) => void;
  addRepo: () => void;
  removeRepo: (canonicalPath: string) => void;
  /** Open a file as a level-2 tab inside its repo's project tab (opening the
   * project tab first if needed). `pin` false (default) previews it in the
   * shared italic slot; `pin` true makes it a permanent tab. The view (diff /
   * normal / markdown) is chosen by FileView from the file's type/change state. */
  openFile: (canonicalPath: string, filePath: string, pin?: boolean) => void;
  /** Open the cross-repo Timeline / history panel (RDM-010). */
  openTimeline: () => void;
  /** Open (or focus) the always-available Dashboard tab. */
  openDashboard: () => void;
}

const noop: WorkspaceActions = {
  openRepo: () => {},
  addRepo: () => {},
  removeRepo: () => {},
  openFile: () => {},
  openTimeline: () => {},
  openDashboard: () => {},
};

export const WorkspaceActionsContext = createContext<WorkspaceActions>(noop);

export function useWorkspaceActions(): WorkspaceActions {
  return useContext(WorkspaceActionsContext);
}
