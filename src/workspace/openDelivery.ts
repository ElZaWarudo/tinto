import type { DockviewApi } from "dockview-react";
import { PANEL_DELIVERY } from "./panels";

export function openDeliveryPanel(api: DockviewApi): void {
  const existing = api.getPanel(PANEL_DELIVERY);
  if (existing) {
    existing.api.setActive();
    return;
  }
  try {
    api.addPanel({ id: PANEL_DELIVERY, component: PANEL_DELIVERY, title: "Delivery" });
  } catch {
    api.getPanel(PANEL_DELIVERY)?.api.setActive();
  }
}
