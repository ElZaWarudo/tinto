import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const ops = vi.hoisted(() => ({
  switchWorkbench: vi.fn(),
  addRepoFlow: vi.fn(),
  autodetectFlow: vi.fn(),
  createAndActivate: vi.fn(),
  removeRepoFlow: vi.fn(),
}));
vi.mock("./operations", () => ops);

import { TopBar } from "./TopBar";
import { FirstRun } from "./firstRun";
import { busStore } from "../bus/store";
import type { WorkbenchConfig } from "../bus/contract";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";

const config: WorkbenchConfig = {
  version: 1,
  active: "Work",
  workbenches: [
    { name: "Work", repos: [] },
    { name: "Side", repos: [] },
  ],
};

describe("TopBar", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    busStore.resetAll();
  });

  // Covers AE5 (switch trigger) + R7
  it("lists workbenches, marks the active one, and switches on change", () => {
    act(() => {
      busStore.setConfig(config);
      busStore.setWatching({ available: true });
    });
    render(<TopBar />);
    const switcher = screen.getByTestId("wb-switcher") as HTMLSelectElement;
    expect(switcher.value).toBe("Work");
    fireEvent.change(switcher, { target: { value: "Side" } });
    expect(ops.switchWorkbench).toHaveBeenCalledWith("Side", "Work");
  });

  it("triggers the add and autodetect flows for the active workbench", () => {
    act(() => busStore.setConfig(config));
    render(<TopBar />);
    fireEvent.click(screen.getByTestId("add-repo"));
    expect(ops.addRepoFlow).toHaveBeenCalledWith("Work");
    fireEvent.click(screen.getByTestId("autodetect"));
    expect(ops.autodetectFlow).toHaveBeenCalledWith("Work");
  });

  it("shows the degraded watch indicator", () => {
    act(() => {
      busStore.setConfig(config);
      busStore.setWatching({ available: false, reason: "inotify" });
    });
    render(<TopBar />);
    expect(screen.getByTestId("watch-indicator")).toHaveTextContent("degraded");
  });

  it("does not crash when config is missing workbenches", () => {
    act(() => {
      busStore.setConfig({ version: 1, active: "Work" } as WorkbenchConfig);
      busStore.setWatching({ available: true });
    });

    render(<TopBar />);

    expect(screen.getByText("Tinto")).toBeInTheDocument();
    expect(screen.getByTestId("wb-switcher")).toBeInTheDocument();
  });

  it("opens the timeline from the top bar", () => {
    const openTimeline = vi.fn();
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openDiff: vi.fn(),
      openTimeline,
    };
    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <TopBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("open-timeline"));
    expect(openTimeline).toHaveBeenCalledOnce();
  });
});

describe("FirstRun", () => {
  beforeEach(() => vi.clearAllMocks());

  // Covers AE1 (create step) + R8
  it("creates a workbench from the entered name", async () => {
    ops.createAndActivate.mockResolvedValue(undefined);
    render(<FirstRun />);
    fireEvent.change(screen.getByTestId("wb-name"), { target: { value: "My Work" } });
    fireEvent.click(screen.getByTestId("create-wb"));
    expect(ops.createAndActivate).toHaveBeenCalledWith("My Work");
  });

  it("does not create on a blank name", () => {
    render(<FirstRun />);
    fireEvent.click(screen.getByTestId("create-wb"));
    expect(ops.createAndActivate).not.toHaveBeenCalled();
  });
});
