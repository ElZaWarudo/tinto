// A detached second-level tab: one panel filling its own window, fed by the
// same bus as the main window.

import { useBusConnection } from "../bus/connection";
import { FileView } from "../panels/file/FileView";
import { DeliveryPanel } from "../panels/delivery/DeliveryPanel";
import type { DetachedPanel } from "./detachPanelWindow";

export function DetachedPanelApp({ panel }: { panel: DetachedPanel }) {
  useBusConnection();
  return (
    <div className="detached-panel-window">
      {panel.kind === "file" ? (
        <FileView repo={panel.repo} path={panel.path} />
      ) : (
        <DeliveryPanel only={panel.repo} />
      )}
    </div>
  );
}
