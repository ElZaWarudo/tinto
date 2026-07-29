// Nested dockview tab for file panels: the default tab, italicized while the
// panel is the shared "preview" slot (VS Code style). Double-clicking a preview
// tab pins it (promotes it to a permanent, split-able panel).

import { DockviewDefaultTab, type IDockviewPanelHeaderProps } from "dockview-react";
import { useAccessibleDockTab } from "../workspace/useAccessibleDockTab";
import { FILE_PREVIEW_ID, fileDock } from "../workspace/fileDock";

export function FileTab(props: IDockviewPanelHeaderProps) {
  const isPreview = props.api.id === FILE_PREVIEW_ID;
  const { repo, path } = props.params as { repo?: string; path?: string };
  const ref = useAccessibleDockTab(props.api);

  return (
    <div
      className={isPreview ? "file-dock-tab file-dock-tab--preview" : "file-dock-tab"}
      onDoubleClick={() => isPreview && repo && path && fileDock.openFile(repo, path, true)}
      ref={ref}
    >
      <DockviewDefaultTab {...props} />
    </div>
  );
}
