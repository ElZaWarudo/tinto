import { describe, expect, it, vi } from "vitest";
import { PANEL_DASHBOARD } from "./panels";
import { resetToDashboardPanel } from "./openDashboard";

describe("resetToDashboardPanel", () => {
  it("opens the dashboard, closes every other panel, and leaves dashboard active", () => {
    const dashboard = panel(PANEL_DASHBOARD);
    const repo = panel("repo:/r/api");
    const timeline = panel("timeline");
    const panels = [repo, dashboard, timeline];
    const api = {
      get panels() {
        return panels.filter((p) => !p.closed);
      },
      getPanel: vi.fn((id: string) => panels.find((p) => p.id === id && !p.closed)),
      addPanel: vi.fn(),
    };

    resetToDashboardPanel(api as never);

    expect(repo.api.close).toHaveBeenCalledOnce();
    expect(timeline.api.close).toHaveBeenCalledOnce();
    expect(dashboard.api.close).not.toHaveBeenCalled();
    expect(dashboard.api.setActive).toHaveBeenCalledTimes(2);
  });

  it("creates the dashboard when needed before closing the rest", () => {
    const repo = panel("repo:/r/api");
    const panels = [repo];
    const api = {
      get panels() {
        return panels.filter((p) => !p.closed);
      },
      getPanel: vi.fn((id: string) => panels.find((p) => p.id === id && !p.closed)),
      addPanel: vi.fn((opts: { id: string }) => {
        const next = panel(opts.id);
        panels.push(next);
        return next;
      }),
    };

    resetToDashboardPanel(api as never);

    expect(api.addPanel).toHaveBeenCalledWith({
      id: PANEL_DASHBOARD,
      component: PANEL_DASHBOARD,
      title: "Dashboard",
    });
    expect(repo.api.close).toHaveBeenCalledOnce();
    expect(api.getPanel(PANEL_DASHBOARD)?.api.setActive).toHaveBeenCalledOnce();
  });
});

function panel(id: string) {
  const state = {
    id,
    closed: false,
    api: {
      setActive: vi.fn(),
      close: vi.fn(() => {
        state.closed = true;
      }),
    },
  };
  return state;
}
