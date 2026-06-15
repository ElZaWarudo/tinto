import { describe, it, expect, vi } from "vitest";
import { openTimelinePanel } from "./openTimeline";
import { PANEL_TIMELINE } from "./panels";

function fakeApi() {
  const panels: Record<string, { api: { setActive: ReturnType<typeof vi.fn> } }> = {};
  return {
    addPanel: vi.fn((opts: { id: string }) => {
      panels[opts.id] = { api: { setActive: vi.fn() } };
      return panels[opts.id];
    }),
    getPanel: vi.fn((id: string) => panels[id]),
    _panels: panels,
  };
}

describe("openTimelinePanel", () => {
  it("adds the timeline panel once and focuses it on later opens", () => {
    const api = fakeApi();
    openTimelinePanel(api as never);
    expect(api.addPanel).toHaveBeenCalledWith({
      id: PANEL_TIMELINE,
      component: PANEL_TIMELINE,
      title: "Timeline",
    });
    const created = api._panels[PANEL_TIMELINE];

    openTimelinePanel(api as never);
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });
});
