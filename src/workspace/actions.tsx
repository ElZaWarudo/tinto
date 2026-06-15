// App-level actions panels invoke (open a repo panel, start the add-repo flow).
// Provided above the dockview host; dockview renders panels through React
// portals, which preserve context, so panel components can consume this.

import { createContext, useContext } from "react";

export interface WorkspaceActions {
  openRepo: (canonicalPath: string) => void;
  addRepo: () => void;
  removeRepo: (canonicalPath: string) => void;
}

const noop: WorkspaceActions = {
  openRepo: () => {},
  addRepo: () => {},
  removeRepo: () => {},
};

export const WorkspaceActionsContext = createContext<WorkspaceActions>(noop);

export function useWorkspaceActions(): WorkspaceActions {
  return useContext(WorkspaceActionsContext);
}
