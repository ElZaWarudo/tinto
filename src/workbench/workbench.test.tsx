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

import { MenuBar } from "./MenuBar";
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

describe("MenuBar", () => {
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
    render(<MenuBar />);
    const switcher = screen.getByTestId("wb-switcher") as HTMLSelectElement;
    expect(switcher.value).toBe("Work");
    fireEvent.change(switcher, { target: { value: "Side" } });
    expect(ops.switchWorkbench).toHaveBeenCalledWith("Side", "Work");
  });

  it("triggers add (via the workspace action) and autodetect from the Repos menu", () => {
    // Add repo goes through the workspace action so it can open the new tab;
    // autodetect still calls the operations flow directly.
    const addRepo = vi.fn();
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo,
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
    };
    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("menu-repos"));
    fireEvent.click(screen.getByTestId("add-repo"));
    expect(addRepo).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByTestId("menu-repos"));
    fireEvent.click(screen.getByTestId("autodetect"));
    expect(ops.autodetectFlow).toHaveBeenCalledWith("Work");
  });

  it("shows the degraded watch indicator", () => {
    act(() => {
      busStore.setConfig(config);
      busStore.setWatching({ available: false, reason: "inotify" });
    });
    render(<MenuBar />);
    expect(screen.getByTestId("watch-indicator")).toHaveTextContent("degraded");
  });

  it("does not crash when config is missing workbenches", () => {
    act(() => {
      busStore.setConfig({ version: 1, active: "Work" } as WorkbenchConfig);
      busStore.setWatching({ available: true });
    });

    render(<MenuBar />);

    expect(screen.getByText("Tinto")).toBeInTheDocument();
    expect(screen.getByTestId("wb-switcher")).toBeInTheDocument();
  });

  it("opens the dashboard and timeline from the Ver menu", () => {
    const openTimeline = vi.fn();
    const openDashboard = vi.fn();
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline,
      openDashboard,
    };
    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("menu-view"));
    fireEvent.click(screen.getByTestId("open-dashboard"));
    expect(openDashboard).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByTestId("menu-view"));
    fireEvent.click(screen.getByTestId("open-timeline"));
    expect(openTimeline).toHaveBeenCalledOnce();
  });

  it("opens a project from the Proyectos menu", () => {
    const openRepo = vi.fn();
    const actions: WorkspaceActions = {
      openRepo,
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
    };
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: "/r/api", alias: null, fs_watch: [] }] }],
      });
      // The menu lists projects from the live snapshot (same as the Dashboard).
      busStore.loadSnapshot(
        [
          {
            repo: "/r/api",
            revision: 1,
            status: { modified: [], staged: [], untracked: [] },
            branch: null,
            head: null,
            last_activity_ms: 0,
            error: null,
          },
        ],
        { available: true },
      );
    });
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("menu-projects"));
    fireEvent.click(screen.getByTestId("open-project-/r/api"));
    expect(openRepo).toHaveBeenCalledWith("/r/api");
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
