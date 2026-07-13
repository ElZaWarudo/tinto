import { useEffect, useMemo, useRef, useState } from "react";
import type { DockviewApi } from "dockview-react";
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
import {
  closePanelsForRemovedRepo,
  closePanelsOutsideActiveWorkbench,
} from "./workspace/closePanels";
import { consoleDock } from "./workspace/consoleDock";
import { fileDock } from "./workspace/fileDock";
import { repoTreeStore } from "./workspace/repoTreeStore";
import { DashboardPanel } from "./panels/DashboardPanel";
import { RepoPanel } from "./panels/RepoPanel";
import { RepoTab } from "./panels/RepoTab";
import { TimelinePanel } from "./panels/timeline/TimelinePanel";
import { ConsoleDockPanel } from "./panels/terminal/ConsoleDockPanel";
import { TerminalPanel } from "./panels/terminal/TerminalPanel";
import { onDetachedConsolesReattach } from "./panels/terminal/detachTerminalWindow";
import {
  armConsolesExternalDetach,
  detachConsolesPanel,
  detachConsolesPanelFromWorkspaceDrop,
} from "./workspace/detachConsoles";
import { MenuBar } from "./workbench/MenuBar";
import { AddRepoDialog } from "./workbench/AddRepoDialog";
import { FirstRun, StartupFailure, StartupLoading } from "./workbench/firstRun";
import { addRepoFlow, removeRepoFlow } from "./workbench/operations";
import { isWindowsHost } from "./workbench/platform";
import { reloadActiveWorkbench, useBusConnection } from "./bus/connection";
import { busStore, useBusState } from "./bus/store";
import type { WorkbenchConfig } from "./bus/contract";
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

function dropRepoUiState(path: string): void {
  fileDock.drop(path);
  repoTreeStore.drop(path);
}

function closeInactiveRepoPanels(
  api: DockviewApi,
  config: WorkbenchConfig | null | undefined,
): void {
  for (const path of closePanelsOutsideActiveWorkbench(api, config)) {
    dropRepoUiState(path);
  }
}

export default function App() {
  useBusConnection();
  const { config, loaded, configError, snapshotError } = useBusState();
  const { glanceMode } = useQualityState();
  const apiRef = useRef<DockviewApi | null>(null);
  const [showAddRepo, setShowAddRepo] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const addLocalRepoFromPicker = async () => {
    const active = busStore.getState().config?.active;
    if (!active) return;
    const path = await addRepoFlow(active);
    if (!path) return;
    setShowAddRepo(false);
    if (apiRef.current) {
      openRepoPanel(apiRef.current, path, busStore.displayName(path));
    }
  };

  // Apply the persisted zoom and bind Ctrl/Cmd +/-/0 (browser-style text size).
  useEffect(() => {
    zoomStore.hydrate();
    return installZoomKeybindings();
  }, []);

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
        else {
          setActionError(null);
          void addLocalRepoFromPicker().catch((error) => {
            setActionError(commandErrorMessage(error, "No se pudo añadir el repositorio local."));
          });
        }
      },
      removeRepo: (path) => {
        const active = busStore.getState().config?.active;
        if (!active) return;
        setActionError(null);
        void removeRepoFlow(active, path)
          .then((removed) => {
            if (!removed) return;
            dropRepoUiState(path);
            // If the backend already removed the repo (or never had it) but the
            // bus snapshot is stale, drop it from the frontend store so the
            // dashboard stops showing the orphan card immediately.
            busStore.dropRepo(path);
            const api = apiRef.current;
            if (!api) return;
            closePanelsForRemovedRepo(api, path);
          })
          .catch((e) => {
            setActionError(commandErrorMessage(e, "No se pudo quitar el repositorio."));
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

  useEffect(() => {
    const api = apiRef.current;
    if (!api) return;
    closeInactiveRepoPanels(api, config);
  }, [config]);

  const hasUsableShell = config !== null && loaded;
  const startupError = !config ? configError : !loaded ? snapshotError : null;

  // Only the initial boot owns the full-screen status. Background reloads keep
  // the existing workspace mounted so dialogs, focus, and unsaved UI state survive.
  if (!hasUsableShell && startupError) {
    return (
      <div className="app-shell">
        <StartupFailure message={startupError} onRetry={() => void reloadActiveWorkbench()} />
      </div>
    );
  }

  if (!hasUsableShell) {
    return (
      <div className="app-shell">
        <StartupLoading />
      </div>
    );
  }

  if (!config?.active) {
    return (
      <div className="app-shell">
        <FirstRun />
      </div>
    );
  }

  const shellError = configError ?? snapshotError;

  return (
    <WorkspaceActionsContext.Provider value={actions}>
      <div className="app-shell">
        <NotificationWatcher />
        <MenuBar />
        {(actionError || shellError) && (
          <div className="app-shell__notice" role="alert" data-testid="app-shell-error">
            <span>{actionError ?? shellError}</span>
            {shellError && (
              <button type="button" onClick={() => void reloadActiveWorkbench()}>
                Reintentar
              </button>
            )}
            {actionError && (
              <button type="button" onClick={() => setActionError(null)}>
                Cerrar
              </button>
            )}
          </div>
        )}
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
                closeInactiveRepoPanels(api, busStore.getState().config);
              }}
            />
          )}
        </div>
      </div>
    </WorkspaceActionsContext.Provider>
  );
}

function commandErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
