import { useMemo, useRef } from "react";
import type { DockviewApi } from "dockview-react";
import { DockWorkspace, type PanelComponents } from "./workspace/DockWorkspace";
import { PANEL_DASHBOARD, PANEL_REPO, PANEL_TREE, repoPanelId } from "./workspace/panels";
import { WorkspaceActionsContext, type WorkspaceActions } from "./workspace/actions";
import { openRepoPanel } from "./workspace/openRepo";
import { DashboardPanel } from "./panels/DashboardPanel";
import { RepoPanel } from "./panels/RepoPanel";
import { RepoTreePanel } from "./panels/RepoTreePanel";
import { TopBar } from "./workbench/TopBar";
import { FirstRun } from "./workbench/firstRun";
import { addRepoFlow, removeRepoFlow } from "./workbench/operations";
import { useBusConnection } from "./bus/connection";
import { busStore, useBusState } from "./bus/store";
import "./App.css";

const components: PanelComponents = {
  [PANEL_DASHBOARD]: DashboardPanel,
  [PANEL_TREE]: RepoTreePanel,
  [PANEL_REPO]: RepoPanel,
};

export default function App() {
  useBusConnection();
  const { config, loaded } = useBusState();
  const apiRef = useRef<DockviewApi | null>(null);

  const actions = useMemo<WorkspaceActions>(
    () => ({
      openRepo: (path) => {
        if (apiRef.current) {
          openRepoPanel(apiRef.current, path, busStore.displayName(path));
        }
      },
      addRepo: () => {
        const active = busStore.getState().config?.active;
        if (active) void addRepoFlow(active);
      },
      removeRepo: (path) => {
        const active = busStore.getState().config?.active;
        if (!active) return;
        void removeRepoFlow(active, path).then((removed) => {
          if (removed) apiRef.current?.getPanel(repoPanelId(path))?.api.close();
        });
      },
    }),
    [],
  );

  // First-run: once loaded, no active workbench → the create flow.
  if (loaded && !config?.active) {
    return (
      <div className="app-shell">
        <FirstRun />
      </div>
    );
  }

  return (
    <WorkspaceActionsContext.Provider value={actions}>
      <div className="app-shell">
        <TopBar />
        <div className="app-shell__body">
          <DockWorkspace
            components={components}
            onApi={(api) => {
              apiRef.current = api;
            }}
          />
        </div>
      </div>
    </WorkspaceActionsContext.Provider>
  );
}
