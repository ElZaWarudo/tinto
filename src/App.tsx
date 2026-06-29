import { useEffect, useMemo, useRef, useState } from "react";
import type {
  DockviewApi,
  DockviewWillDropEvent,
  IDockviewPanel,
  TabDragEvent,
} from "dockview-react";
import { DockWorkspace, type PanelComponents, type TabComponents } from "./workspace/DockWorkspace";
import {
  PANEL_AGENT_CONSOLES,
  PANEL_AGENT_TERMINAL,
  PANEL_DASHBOARD,
  PANEL_REPO,
  PANEL_TIMELINE,
  TAB_REPO,
} from "./workspace/panels";
import { WorkspaceActionsContext, type WorkspaceActions } from "./workspace/actions";
import { openRepoPanel } from "./workspace/openRepo";
import { openTimelinePanel } from "./workspace/openTimeline";
import { openDashboardPanel, resetToDashboardPanel } from "./workspace/openDashboard";
import { openAgentConsolesPanel, openAgentTerminalPanel } from "./workspace/openAgentTerminal";
import { closePanelsForRemovedRepo } from "./workspace/closePanels";
import { consoleDock } from "./workspace/consoleDock";
import { fileDock } from "./workspace/fileDock";
import { repoTreeStore } from "./workspace/repoTreeStore";
import { DashboardPanel } from "./panels/DashboardPanel";
import { RepoPanel } from "./panels/RepoPanel";
import { RepoTab } from "./panels/RepoTab";
import { TimelinePanel } from "./panels/timeline/TimelinePanel";
import { ConsoleDockPanel } from "./panels/terminal/ConsoleDockPanel";
import { TerminalPanel } from "./panels/terminal/TerminalPanel";
import {
  markTerminalDetached,
  onDetachedConsolesReattach,
  openDetachedConsolesWindow,
} from "./panels/terminal/detachTerminalWindow";
import { armExternalTabDetach } from "./workspace/externalTabDetach";
import { MenuBar } from "./workbench/MenuBar";
import { AddRepoDialog } from "./workbench/AddRepoDialog";
import { FirstRun } from "./workbench/firstRun";
import { addRepoFlow, removeRepoFlow } from "./workbench/operations";
import { isWindowsHost } from "./workbench/platform";
import { useBusConnection } from "./bus/connection";
import { busStore, useBusState } from "./bus/store";
import { GlanceMode } from "./qol/GlanceMode";
import { NotificationWatcher } from "./qol/notifications";
import { useQualityState } from "./qol/state";
import { installZoomKeybindings, zoomStore } from "./qol/zoom";
import { installShortcuts } from "./qol/shortcuts";
import "./App.css";

const components: PanelComponents = {
  [PANEL_DASHBOARD]: DashboardPanel,
  [PANEL_REPO]: RepoPanel,
  [PANEL_TIMELINE]: TimelinePanel,
  [PANEL_AGENT_CONSOLES]: ConsoleDockPanel,
  [PANEL_AGENT_TERMINAL]: TerminalPanel,
};

const tabComponents: TabComponents = {
  [TAB_REPO]: RepoTab,
};

const detachingConsolesPanels = new Set<string>();

export async function detachConsolesPanelFromWorkspaceDrop(
  event: DockviewWillDropEvent,
  api: DockviewApi,
): Promise<boolean> {
  if (event.kind !== "edge" || event.getData()?.panelId !== PANEL_AGENT_CONSOLES) {
    return false;
  }
  const panel = api.getPanel(PANEL_AGENT_CONSOLES);
  if (!panel) return false;

  event.preventDefault();
  return detachConsolesPanel(api, panel);
}

export async function detachConsolesPanel(
  api: DockviewApi,
  panel: IDockviewPanel | undefined = api.getPanel(PANEL_AGENT_CONSOLES),
): Promise<boolean> {
  if (!panel || detachingConsolesPanels.has(panel.id)) return false;

  detachingConsolesPanels.add(panel.id);
  const terminalParams = consoleDock.openTerminalParams();
  consoleDock.prepareDetachedTransfer();
  try {
    const opened = await openDetachedConsolesWindow(terminalParams);
    if (!opened) return false;

    for (const sessionId of consoleDock.openTerminalSessionIds()) {
      markTerminalDetached(sessionId);
    }
    const current = api.getPanel(panel.id);
    if (current) {
      api.removePanel(current);
    }
    return true;
  } finally {
    detachingConsolesPanels.delete(panel.id);
  }
}

export function armConsolesExternalDetach(event: TabDragEvent, api: DockviewApi): boolean {
  if (event.panel.id !== PANEL_AGENT_CONSOLES) return false;
  armExternalTabDetach(event.nativeEvent, () => detachConsolesPanel(api, event.panel));
  return true;
}

export default function App() {
  useBusConnection();
  const { config, loaded, repos } = useBusState();
  const { glanceMode } = useQualityState();
  const apiRef = useRef<DockviewApi | null>(null);
  const [showAddRepo, setShowAddRepo] = useState(false);

  const addLocalRepoFromPicker = () => {
    const active = busStore.getState().config?.active;
    if (!active) return;
    void addRepoFlow(active).then((path) => {
      if (!path) return;
      setShowAddRepo(false);
      if (apiRef.current) {
        openRepoPanel(apiRef.current, path, busStore.displayName(path));
      }
    });
  };

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
        if (isWindowsHost()) setShowAddRepo(true);
        else addLocalRepoFromPicker();
      },
      removeRepo: (path) => {
        const active = busStore.getState().config?.active;
        if (!active) return;
        void removeRepoFlow(active, path)
          .then((removed) => {
            if (!removed) return;
            fileDock.drop(path);
            repoTreeStore.drop(path);
            // If the backend already removed the repo (or never had it) but the
            // bus snapshot is stale, drop it from the frontend store so the
            // dashboard stops showing the orphan card immediately.
            busStore.dropRepo(path);
            const api = apiRef.current;
            if (!api) return;
            closePanelsForRemovedRepo(api, path);
          })
          .catch((e) => {
            console.warn("tinto: remove repo action failed", e);
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
      openDashboard: (options) => {
        if (apiRef.current) {
          if (options?.closeAll) {
            resetToDashboardPanel(apiRef.current);
          } else {
            openDashboardPanel(apiRef.current);
          }
        }
      },
      openAgents: () => {
        if (apiRef.current) {
          openAgentConsolesPanel(apiRef.current);
        }
      },
      openAgentTerminal: (params) => {
        if (apiRef.current) {
          openAgentTerminalPanel(apiRef.current, params);
        }
      },
    }),
    [],
  );

  // Install global keyboard shortcuts (navigation, view toggles, etc.)
  useEffect(() => {
    return installShortcuts(apiRef, {
      openDashboard: () => actions.openDashboard(),
      openTimeline: () => actions.openTimeline(),
      addRepo: () => actions.addRepo(),
    });
  }, [actions]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void onDetachedConsolesReattach((terminals) => {
      const api = apiRef.current;
      if (!api) return;
      openAgentConsolesPanel(api);
      terminals.forEach((params) => consoleDock.openTerminal(params));
    })
      .then((dispose) => {
        if (!active) {
          dispose();
          return;
        }
        unlisten = dispose;
      })
      .catch((error) => {
        console.error("Failed to listen for detached console reattach events", error);
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

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
        {showAddRepo && config?.active && (
          <AddRepoDialog
            activeWorkbench={config.active}
            onClose={() => setShowAddRepo(false)}
            onAddLocal={addLocalRepoFromPicker}
          />
        )}
        <div className="app-shell__body">
          {glanceMode ? (
            <GlanceMode />
          ) : (
            <DockWorkspace
              components={components}
              tabComponents={tabComponents}
              onWillDrop={(event, api) => {
                void detachConsolesPanelFromWorkspaceDrop(event, api);
              }}
              onDidMovePanel={(event, api) => {
                if (
                  event.panel.id !== PANEL_AGENT_CONSOLES ||
                  event.panel.api.location.type === "grid"
                ) {
                  return;
                }
                void detachConsolesPanel(api, event.panel);
              }}
              onWillDragPanel={(event, api) => {
                armConsolesExternalDetach(event, api);
              }}
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
