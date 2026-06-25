import { useEffect, useRef } from "react";
import { DockviewReact, themeVisualStudio } from "dockview-react";
import type { DockviewReadyEvent } from "dockview-react";
import { PANEL_AGENT_TERMINAL } from "../../workspace/panels";
import { consoleDock } from "../../workspace/consoleDock";
import { TerminalPanel } from "./TerminalPanel";

const consoleComponents = {
  [PANEL_AGENT_TERMINAL]: TerminalPanel,
};

export function ConsoleDockPanel() {
  const apiRef = useRef<DockviewReadyEvent["api"] | null>(null);

  useEffect(() => {
    return () => {
      if (apiRef.current) {
        consoleDock.unregister(apiRef.current);
      }
    };
  }, []);

  const onReady = (event: DockviewReadyEvent) => {
    apiRef.current = event.api;
    consoleDock.register(event.api);
  };

  return (
    <div className="console-dock-panel" data-testid="console-dock-panel">
      <DockviewReact
        components={consoleComponents}
        dndStrategy="pointer"
        theme={themeVisualStudio}
        onReady={onReady}
      />
    </div>
  );
}
