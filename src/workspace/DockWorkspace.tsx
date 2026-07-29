// The dockable workspace host. Wraps dockview-react with: layout restore (or
// default) on ready, debounced persistence on layout change, a flush on quit,
// and an empty-workspace guard so the user can never end up with no panels.
// Panel components are injected so later items register new panel types without
// touching this shell.

import { DockviewReact } from "dockview-react";
import type {
  DockviewApi,
  DockviewReadyEvent,
  DockviewWillDropEvent,
  IDockviewPanelProps,
  IDockviewPanelHeaderProps,
  MovePanelEvent,
  TabDragEvent,
} from "dockview-react";
import { themeVisualStudio } from "dockview-react";
import "dockview-react/dist/styles/dockview.css";
import { useEffect, useRef } from "react";
import { AccessibleDockTab } from "./AccessibleDockTab";
import { applyLayout, loadUiState, saveUiState } from "./layout";
import { PANEL_DASHBOARD } from "./panels";

export type PanelComponents = Record<string, React.FunctionComponent<IDockviewPanelProps>>;
export type TabComponents = Record<string, React.FunctionComponent<IDockviewPanelHeaderProps>>;

const SAVE_DEBOUNCE_MS = 400;

export function DockWorkspace({
  components,
  tabComponents,
  onApi,
  onWillDrop,
  onDidMovePanel,
  onWillDragPanel,
}: {
  components: PanelComponents;
  tabComponents?: TabComponents;
  onApi?: (api: DockviewApi) => void;
  onWillDrop?: (event: DockviewWillDropEvent, api: DockviewApi) => void;
  onDidMovePanel?: (event: MovePanelEvent, api: DockviewApi) => void;
  onWillDragPanel?: (event: TabDragEvent, api: DockviewApi) => void;
}) {
  const apiRef = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cleanups = useRef<Array<() => void>>([]);

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    apiRef.current = api;
    onApi?.(api);
    const dragDisposable = api.onWillDragPanel((event) => onWillDragPanel?.(event, api));
    cleanups.current.push(() => dragDisposable.dispose());

    const flush = () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      void saveUiState(api.toJSON());
    };
    const scheduleSave = () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => void saveUiState(api.toJSON()), SAVE_DEBOUNCE_MS);
    };

    void (async () => {
      const layout = await loadUiState();
      applyLayout(api, layout);
      // Attach listeners AFTER the restore so the transient initial state never
      // overwrites the persisted layout before it is applied.
      const d1 = api.onDidLayoutChange(scheduleSave);
      const d0 = api.onDidMovePanel((event) => onDidMovePanel?.(event, api));
      const d2 = api.onDidRemovePanel(() => {
        if (api.panels.length === 0) {
          // Guard: never strand the user with an empty workspace.
          api.addPanel({
            id: PANEL_DASHBOARD,
            component: PANEL_DASHBOARD,
            title: "Resumen",
          });
        }
      });
      cleanups.current.push(
        () => d0.dispose(),
        () => d1.dispose(),
        () => d2.dispose(),
      );
    })();

    // Best-effort flush of the last arrangement on window close.
    window.addEventListener("beforeunload", flush);
    cleanups.current.push(() => window.removeEventListener("beforeunload", flush));
  };

  // Tear down listeners + any pending save on unmount (StrictMode remount, the
  // first-run→workspace transition) so stale handlers don't fire on a disposed api.
  useEffect(() => {
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      cleanups.current.forEach((c) => c());
      cleanups.current = [];
    };
  }, []);

  return (
    <DockviewReact
      components={components}
      tabComponents={tabComponents}
      defaultTabComponent={AccessibleDockTab}
      dndStrategy="pointer"
      theme={themeVisualStudio}
      onReady={onReady}
      onWillDrop={(event) => {
        if (apiRef.current) onWillDrop?.(event, apiRef.current);
      }}
    />
  );
}
