// App-level actions panels invoke (open a repo panel, start the add-repo flow).
// Provided above the dockview host; dockview renders panels through React
// portals, which preserve context, so panel components can consume this.

import { createContext, useContext } from "react";

export interface WorkspaceActions {
  openRepo: (canonicalPath: string) => void;
  addRepo: () => void;
  removeRepo: (canonicalPath: string) => void;
  /** Open (or focus) the diff panel for a file within a repo (RDM-008). */
  openDiff: (canonicalPath: string, filePath: string) => void;
  /** Open the cross-repo Timeline / history panel (RDM-010). */
  openTimeline: () => void;
}

const noop: WorkspaceActions = {
  openRepo: () => {},
  addRepo: () => {},
  removeRepo: () => {},
  openDiff: () => {},
  openTimeline: () => {},
};

export const WorkspaceActionsContext = createContext<WorkspaceActions>(noop);

export function useWorkspaceActions(): WorkspaceActions {
  return useContext(WorkspaceActionsContext);
}
