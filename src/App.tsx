import { useEffect, useMemo, useRef } from "react";
import type { DockviewApi } from "dockview-react";
import { DockWorkspace, type PanelComponents, type TabComponents } from "./workspace/DockWorkspace";
import { PANEL_DASHBOARD, PANEL_REPO, PANEL_TIMELINE, TAB_REPO } from "./workspace/panels";
import { WorkspaceActionsContext, type WorkspaceActions } from "./workspace/actions";
import { openRepoPanel } from "./workspace/openRepo";
import { openTimelinePanel } from "./workspace/openTimeline";
import { openDashboardPanel } from "./workspace/openDashboard";
import { closePanelsForRemovedRepo } from "./workspace/closePanels";
import { fileDock } from "./workspace/fileDock";
import { repoTreeStore } from "./workspace/repoTreeStore";
import { DashboardPanel } from "./panels/DashboardPanel";
import { RepoPanel } from "./panels/RepoPanel";
import { RepoTab } from "./panels/RepoTab";
import { TimelinePanel } from "./panels/timeline/TimelinePanel";
import { MenuBar } from "./workbench/MenuBar";
import { FirstRun } from "./workbench/firstRun";
import { addRepoFlow, removeRepoFlow } from "./workbench/operations";
import { useBusConnection } from "./bus/connection";
import { busStore, useBusState } from "./bus/store";
import { GlanceMode } from "./qol/GlanceMode";
import { NotificationWatcher } from "./qol/notifications";
import { useQualityState } from "./qol/state";
import { installZoomKeybindings, zoomStore } from "./qol/zoom";
import "./App.css";

const components: PanelComponents = {
  [PANEL_DASHBOARD]: DashboardPanel,
  [PANEL_REPO]: RepoPanel,
  [PANEL_TIMELINE]: TimelinePanel,
};

const tabComponents: TabComponents = {
  [TAB_REPO]: RepoTab,
};

export default function App() {
  useBusConnection();
  const { config, loaded, repos } = useBusState();
  const { glanceMode } = useQualityState();
  const apiRef = useRef<DockviewApi | null>(null);

  // Apply the persisted zoom and bind Ctrl/Cmd +/-/0 (browser-style text size).
  useEffect(() => {
    zoomStore.hydrate();
    return installZoomKeybindings();
  }, []);

  // Background-preload every repo's file tree so each project's explorer is
  // "always loaded" — no spinner when its tab opens. ensureLoaded is idempotent.
  const repoKeys = Object.keys(repos).sort().join("\n");
  useEffect(() => {
    if (repoKeys) repoTreeStore.preload(repoKeys.split("\n"));
  }, [repoKeys]);

  const actions = useMemo<WorkspaceActions>(
    () => ({
      openRepo: (path) => {
        if (apiRef.current) {
          openRepoPanel(apiRef.current, path, busStore.displayName(path));
        }
      },
      addRepo: () => {
        const active = busStore.getState().config?.active;
        if (!active) return;
        void addRepoFlow(active).then((path) => {
          // Open the newly added repo's project tab (bound to the canonical key).
          if (path && apiRef.current) {
            openRepoPanel(apiRef.current, path, busStore.displayName(path));
          }
        });
      },
      removeRepo: (path) => {
        const active = busStore.getState().config?.active;
        if (!active) return;
        void removeRepoFlow(active, path).then((removed) => {
          if (!removed) return;
          fileDock.drop(path);
          repoTreeStore.drop(path);
          const api = apiRef.current;
          if (!api) return;
          closePanelsForRemovedRepo(api, path);
        });
      },
      openFile: (path, filePath, pin = false) => {
        // Ensure the repo's project tab exists, then open the file in its nested
        // dock (queued there until the project's dockview is ready).
        if (apiRef.current) {
          openRepoPanel(apiRef.current, path, busStore.displayName(path));
        }
        fileDock.openFile(path, filePath, pin);
      },
      openTimeline: () => {
        if (apiRef.current) {
          openTimelinePanel(apiRef.current);
        }
      },
      openDashboard: () => {
        if (apiRef.current) {
          openDashboardPanel(apiRef.current);
        }
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
        <NotificationWatcher />
        <MenuBar />
        <div className="app-shell__body">
          {glanceMode ? (
            <GlanceMode />
          ) : (
            <DockWorkspace
              components={components}
              tabComponents={tabComponents}
              onApi={(api) => {
                apiRef.current = api;
              }}
            />
          )}
        </div>
      </div>
    </WorkspaceActionsContext.Provider>
  );
}
