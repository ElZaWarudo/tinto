import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const ops = vi.hoisted(() => ({
  switchWorkbench: vi.fn(),
  addRepoFlow: vi.fn(),
  addWslRepoFlow: vi.fn(),
  autodetectFlow: vi.fn(),
  createAndActivate: vi.fn(),
  removeRepoFlow: vi.fn(),
  normalizeWslLinuxPath: vi.fn((path: string) => {
    const trimmed = path.trim();
    if (!trimmed.startsWith("/") || trimmed.includes("\\") || trimmed.includes("..")) return null;
    return trimmed.replace(/\/+$/, "");
  }),
  getGitleaksSetupStatus: vi.fn(),
  installGitleaks: vi.fn(),
}));
vi.mock("./operations", () => ops);
vi.mock("../bus/client", async () => ({
  getGitleaksSetupStatus: ops.getGitleaksSetupStatus,
  installGitleaks: ops.installGitleaks,
}));
vi.mock("./platform", async () => {
  const actual = await vi.importActual<typeof import("./platform")>("./platform");
  return actual;
});

import { MenuBar } from "./MenuBar";
import { FirstRun } from "./firstRun";
import { setWindowsHostOverrideForTests } from "./platform";
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
    setWindowsHostOverrideForTests(null);
    busStore.resetAll();
  });

  it("renders the Tinto brand asset", () => {
    act(() => busStore.setConfig(config));
    render(<MenuBar />);
    expect(screen.getByAltText("Tinto")).toHaveAttribute("src", expect.stringContaining(".png"));
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
      openAgentTerminal: vi.fn(),
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

  it("hides WSL repo actions on non-Windows hosts", () => {
    setWindowsHostOverrideForTests(false);
    act(() => busStore.setConfig(config));
    render(<MenuBar />);
    fireEvent.click(screen.getByTestId("menu-repos"));
    expect(screen.queryByTestId("add-wsl-repo")).not.toBeInTheDocument();
  });

  it("opens the Windows-only WSL add dialog and submits a selected Ubuntu Linux path", async () => {
    setWindowsHostOverrideForTests(true);
    ops.addWslRepoFlow.mockResolvedValue("/home/me/repo");
    act(() => busStore.setConfig(config));
    render(<MenuBar />);

    fireEvent.click(screen.getByTestId("menu-repos"));
    fireEvent.click(screen.getByTestId("add-wsl-repo"));
    expect(screen.getByTestId("add-wsl-dialog")).toBeInTheDocument();

    fireEvent.change(screen.getByTestId("wsl-distro"), {
      target: { value: "Ubuntu-24.04" },
    });
    fireEvent.change(screen.getByTestId("wsl-path"), { target: { value: "/home/me/repo/" } });
    fireEvent.change(screen.getByTestId("wsl-alias"), { target: { value: "API WSL" } });
    fireEvent.click(screen.getByTestId("add-wsl-submit"));

    await act(async () => {
      await Promise.resolve();
    });

    expect(ops.addWslRepoFlow).toHaveBeenCalledWith("Work", {
      distro: "Ubuntu-24.04",
      path: "/home/me/repo",
      alias: "API WSL",
    });
  });

  it("shows configured WSL labels on Windows without listing them as projects", () => {
    setWindowsHostOverrideForTests(true);
    const openRepo = vi.fn();
    const actions: WorkspaceActions = {
      openRepo,
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
      openAgentTerminal: vi.fn(),
    };
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              {
                source: "wsl",
                path: "/home/me/repo",
                distro: "Ubuntu",
                alias: "API WSL",
                fs_watch: [],
              },
            ],
          },
        ],
      }),
    );
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );

    fireEvent.click(screen.getByTestId("menu-repos"));
    expect(screen.getByTestId("configured-wsl-/home/me/repo")).toHaveTextContent("API WSL");

    fireEvent.click(screen.getByTestId("menu-projects"));
    expect(screen.getByTestId("projects-empty")).toBeInTheDocument();
    expect(openRepo).not.toHaveBeenCalled();
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

    expect(screen.getByAltText("Tinto")).toBeInTheDocument();
    // Switcher is hidden when there's only one or no workbench
    expect(screen.queryByTestId("wb-switcher")).not.toBeInTheDocument();
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
      openAgentTerminal: vi.fn(),
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

  it("opens the addons manager from Complementos menu", async () => {
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
      openAgentTerminal: vi.fn(),
    };
    ops.getGitleaksSetupStatus.mockResolvedValue({
      installed: false,
      version: null,
      binary_path: null,
    });

    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("menu-addons"));
    fireEvent.click(screen.getByTestId("manage-addons"));

    expect(screen.getByRole("heading", { name: "Complementos" })).toBeInTheDocument();
    expect(ops.getGitleaksSetupStatus).toHaveBeenCalledOnce();
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
      openAgentTerminal: vi.fn(),
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

  it("installs Gitleaks only when requested from the addons modal", async () => {
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
      openAgentTerminal: vi.fn(),
    };
    ops.getGitleaksSetupStatus.mockResolvedValue({
      installed: false,
      version: null,
      binary_path: null,
    });
    ops.installGitleaks.mockResolvedValue({
      installed: true,
      version: "8.18.0",
      binary_path: "/usr/local/bin/gitleaks",
      method: "brew",
      message: "Gitleaks instalado con éxito",
    });

    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.click(screen.getByTestId("menu-addons"));
    fireEvent.click(screen.getByTestId("manage-addons"));

    await act(async () => {
      await Promise.resolve();
    });

    expect(ops.installGitleaks).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("gitleaks-install"));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(ops.installGitleaks).toHaveBeenCalled();
    expect(screen.getByText("Gitleaks instalado con éxito")).toBeInTheDocument();
  });
});

describe("FirstRun", () => {
  beforeEach(() => vi.clearAllMocks());

  // Covers AE1 (create step) + R8
  it("creates a workbench from the entered name", async () => {
    ops.createAndActivate.mockResolvedValue(undefined);
    render(<FirstRun />);
    expect(screen.getByAltText("Tinto")).toBeInTheDocument();
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
