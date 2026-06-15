// The dockable workspace host. Wraps dockview-react with: layout restore (or
// default) on ready, debounced persistence on layout change, a flush on quit,
// and an empty-workspace guard so the user can never end up with no panels.
// Panel components are injected so later items register new panel types without
// touching this shell.

import { DockviewReact } from "dockview-react";
import type { DockviewApi, DockviewReadyEvent, IDockviewPanelProps } from "dockview-react";
import { themeVisualStudio } from "dockview-react";
import "dockview-react/dist/styles/dockview.css";
import { useRef } from "react";
import { applyLayout, loadUiState, saveUiState } from "./layout";
import { PANEL_DASHBOARD } from "./panels";

export type PanelComponents = Record<string, React.FunctionComponent<IDockviewPanelProps>>;

const SAVE_DEBOUNCE_MS = 400;

export function DockWorkspace({ components }: { components: PanelComponents }) {
  const apiRef = useRef<DockviewApi | null>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const onReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    apiRef.current = api;

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
      api.onDidLayoutChange(scheduleSave);
      api.onDidRemovePanel(() => {
        if (api.panels.length === 0) {
          // Guard: never strand the user with an empty workspace.
          api.addPanel({
            id: PANEL_DASHBOARD,
            component: PANEL_DASHBOARD,
            title: "Dashboard",
          });
        }
      });
    })();

    // Best-effort flush of the last arrangement on window close.
    window.addEventListener("beforeunload", flush);
  };

  return <DockviewReact components={components} theme={themeVisualStudio} onReady={onReady} />;
}
