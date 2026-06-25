import { useMemo } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { useBusConnection } from "../../bus/connection";
import { TerminalPanel, type TerminalPanelParams } from "./TerminalPanel";

const detachedApi: IDockviewPanelProps<TerminalPanelParams>["api"] = {
  get isActive() {
    return true;
  },
  setActive: () => {},
  onDidActiveChange: () => ({ dispose: () => {} }),
} as unknown as IDockviewPanelProps<TerminalPanelParams>["api"];

export function DetachedTerminalApp({ params }: { params: TerminalPanelParams }) {
  useBusConnection();
  const panelProps = useMemo(
    () =>
      ({
        params,
        api: detachedApi,
      }) as IDockviewPanelProps<TerminalPanelParams>,
    [params],
  );

  return (
    <div className="detached-terminal-window">
      <TerminalPanel {...panelProps} />
    </div>
  );
}
