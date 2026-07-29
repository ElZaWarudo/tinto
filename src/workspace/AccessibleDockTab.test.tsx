import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("dockview-react", () => ({
  DockviewDefaultTab: ({ api }: { api: { title?: string } }) => <span>{api.title}</span>,
}));

import { AccessibleDockTab } from "./AccessibleDockTab";

function makeApi(id: string, title: string, isActive: boolean) {
  let activeListener: ((event: { isActive: boolean }) => void) | undefined;
  const api = {
    id,
    title,
    isActive,
    setActive: vi.fn(() => {
      api.isActive = true;
      activeListener?.({ isActive: true });
    }),
    onDidActiveChange: vi.fn((listener: (event: { isActive: boolean }) => void) => {
      activeListener = listener;
      return { dispose: vi.fn() };
    }),
    onDidTitleChange: vi.fn(() => {
      return { dispose: vi.fn() };
    }),
  };
  return api;
}

function Tab({ api }: { api: ReturnType<typeof makeApi> }) {
  return (
    <div className="dv-tab">
      <AccessibleDockTab
        api={api as never}
        containerApi={{} as never}
        params={{}}
        tabLocation="header"
      />
    </div>
  );
}

describe("AccessibleDockTab", () => {
  it("exposes tab semantics and activates pointer and keyboard interactions", () => {
    const summary = makeApi("dashboard", "Resumen", true);
    const timeline = makeApi("timeline", "Cronología", false);
    render(
      <div className="dv-groupview">
        <div className="dv-tabs-container">
          <Tab api={summary} />
          <Tab api={timeline} />
        </div>
        <div className="dv-content-container">Contenido activo</div>
      </div>,
    );

    const tablist = screen.getByRole("tablist", { name: "Paneles" });
    const summaryTab = screen.getByRole("tab", { name: "Resumen" });
    const timelineTab = screen.getByRole("tab", { name: "Cronología" });
    expect(tablist).toContainElement(summaryTab);
    expect(summaryTab).toHaveAttribute("aria-selected", "true");
    expect(timelineTab).toHaveAttribute("aria-selected", "false");
    expect(summaryTab).toHaveAttribute("tabindex", "0");
    expect(timelineTab).toHaveAttribute("tabindex", "-1");
    const panel = screen.getByRole("tabpanel");
    expect(summaryTab).toHaveAttribute("aria-controls", panel.id);
    expect(timelineTab).toHaveAttribute("aria-controls", panel.id);
    expect(panel).toHaveAttribute("aria-labelledby", summaryTab.id);

    fireEvent.click(timelineTab);
    expect(timeline.setActive).toHaveBeenCalledOnce();
    expect(timelineTab).toHaveFocus();
    expect(panel).toHaveAttribute("aria-labelledby", timelineTab.id);

    fireEvent.keyDown(summaryTab, { key: "Enter" });
    expect(summary.setActive).toHaveBeenCalledOnce();

    fireEvent.keyDown(summaryTab, { key: "ArrowRight" });
    expect(timeline.setActive).toHaveBeenCalledTimes(2);
    expect(timelineTab).toHaveFocus();
  });
});
