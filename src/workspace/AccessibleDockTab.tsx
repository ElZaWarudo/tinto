import { DockviewDefaultTab, type IDockviewPanelHeaderProps } from "dockview-react";
import { useAccessibleDockTab } from "./useAccessibleDockTab";

export function AccessibleDockTab(props: IDockviewPanelHeaderProps) {
  const ref = useAccessibleDockTab(props.api);
  return (
    <div className="accessible-dock-tab" ref={ref}>
      <DockviewDefaultTab {...props} />
    </div>
  );
}
