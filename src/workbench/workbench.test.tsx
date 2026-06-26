import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

const ops = vi.hoisted(() => ({
  switchWorkbench: vi.fn(),
  addRepoFlow: vi.fn(),
  addWslRepoFlow: vi.fn(),
  autodetectFlow: vi.fn(),
  listWslDirectoryFlow: vi.fn(),
  listWslDistrosFlow: vi.fn(),
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
import { AddRepoDialog } from "./AddRepoDialog";
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
    ops.listWslDistrosFlow.mockResolvedValue(["Ubuntu-24.04", "Debian"]);
    ops.listWslDirectoryFlow.mockResolvedValue({
      path: "/home/me",
      is_git_repo: false,
      entries: [{ name: "repo", path: "/home/me/repo" }],
    });
  });

  it("renders the Tinto brand asset", () => {
    act(() => busStore.setConfig(config));
    render(<MenuBar />);
    expect(screen.getByAltText("Tinto")).toHaveAttribute("src", expect.stringContaining(".png"));
  });

  // Covers AE5 (switch trigger) + R7
  it("lists recent workbenches, marks the active one, and switches on click", () => {
    // Seed an MRU order so we can verify the menu respects it.
    localStorage.setItem("tinto:recent-workbenches:v1", JSON.stringify(["Side", "Work"]));
    act(() => {
      busStore.setConfig(config);
      busStore.setWatching({ available: true });
    });
    render(<MenuBar />);

    fireEvent.click(screen.getByTestId("menu-workbench"));
    // Side is the most recent and is listed first.
    const recentList = screen.getByTestId("workbench-recent-Side");
    expect(recentList).toBeInTheDocument();
    // The active workbench is marked via the menu__check ✓ (no testid, but
    // the active one is in the same menu and renders the checkmark).
    const activeItem = screen.getByTestId("workbench-recent-Work");
    expect(activeItem).toBeInTheDocument();
    expect(activeItem.querySelector(".menu__check")?.textContent).toBe("✓");

    fireEvent.click(screen.getByTestId("workbench-recent-Side"));
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

  it("does not expose WSL as a separate Repos menu action", () => {
    setWindowsHostOverrideForTests(true);
    act(() => busStore.setConfig(config));
    render(<MenuBar />);
    fireEvent.click(screen.getByTestId("menu-repos"));
    expect(screen.queryByRole("menuitem", { name: /add wsl repo/i })).not.toBeInTheDocument();
  });

  it("submits a selected Ubuntu Linux path from the WSL add dialog", async () => {
    ops.addWslRepoFlow.mockResolvedValue("/home/me/repo");
    render(<AddRepoDialog activeWorkbench="Work" onClose={vi.fn()} />);

    expect(screen.getByTestId("add-repo-dialog")).toBeInTheDocument();

    await act(async () => {
      await Promise.resolve();
    });

    fireEvent.change(screen.getByTestId("wsl-distro"), {
      target: { value: "Ubuntu-24.04" },
    });
    fireEvent.change(screen.getByTestId("wsl-path"), { target: { value: "/home/me/repo/" } });
    fireEvent.change(screen.getByTestId("wsl-alias"), { target: { value: "API WSL" } });
    fireEvent.click(screen.getByTestId("add-repo-submit"));

    await act(async () => {
      await Promise.resolve();
    });

    expect(ops.addWslRepoFlow).toHaveBeenCalledWith("Work", {
      distro: "Ubuntu-24.04",
      path: "/home/me/repo",
      alias: "API WSL",
    });
  });

  it("shows backend errors when adding a WSL repo fails", async () => {
    ops.addWslRepoFlow.mockRejectedValue(new Error("distro WSL no soportada: Ubuntu-24.04"));
    render(<AddRepoDialog activeWorkbench="Work" onClose={vi.fn()} />);

    await act(async () => {
      await Promise.resolve();
    });

    fireEvent.change(screen.getByTestId("wsl-path"), { target: { value: "/home/me/repo" } });
    fireEvent.click(screen.getByTestId("add-repo-submit"));

    expect(await screen.findByTestId("add-repo-error")).toHaveTextContent(
      "distro WSL no soportada: Ubuntu-24.04",
    );
  });

  it("can render the add repo dialog as one source-neutral flow", async () => {
    const addLocal = vi.fn();
    render(<AddRepoDialog activeWorkbench="Work" onClose={vi.fn()} onAddLocal={addLocal} />);

    await act(async () => {
      await Promise.resolve();
    });

    expect(screen.getByRole("dialog", { name: "Agregar repo" })).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("add-local-repo"));
    expect(addLocal).toHaveBeenCalledOnce();
    expect(screen.getByText("Linux en WSL")).toBeInTheDocument();
    expect(screen.getByTestId("wsl-path")).toBeInTheDocument();
  });

  it("lets Windows users browse detected WSL distro directories", async () => {
    ops.listWslDistrosFlow.mockResolvedValue(["Ubuntu-24.04"]);
    ops.listWslDirectoryFlow
      .mockResolvedValueOnce({
        path: "/home/me",
        is_git_repo: false,
        entries: [{ name: "repo", path: "/home/me/repo" }],
      })
      .mockResolvedValueOnce({
        path: "/home/me/repo",
        is_git_repo: true,
        entries: [],
      });
    render(<AddRepoDialog activeWorkbench="Work" onClose={vi.fn()} />);

    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByTestId("wsl-distro")).toHaveValue("Ubuntu-24.04");

    await act(async () => {
      fireEvent.click(screen.getByTestId("wsl-dir-/home/me/repo"));
      await Promise.resolve();
    });

    expect(screen.getByTestId("wsl-path")).toHaveValue("/home/me/repo");
    expect(screen.getByText("Git repo")).toBeInTheDocument();
  });

  it("lists loaded WSL repos as regular projects instead of a separate configured list", () => {
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
    act(() => {
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
      });
      busStore.loadSnapshot(
        [
          {
            repo: "/home/me/repo",
            revision: 1,
            status: { modified: [], staged: [], untracked: [] },
            branch: null,
            head: null,
            last_activity_ms: 1,
            error: null,
            metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
            gitleaks_configured: false,
            signals: [],
            secret_findings: [],
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

    fireEvent.click(screen.getByTestId("menu-repos"));
    expect(screen.queryByTestId("configured-wsl-Ubuntu-/home/me/repo")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("menu-projects"));
    fireEvent.click(screen.getByTestId("open-project-/home/me/repo"));
    expect(openRepo).toHaveBeenCalledWith("/home/me/repo");
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
    localStorage.setItem("tinto:recent-workbenches:v1", JSON.stringify(["Side", "Work"]));
    act(() => {
      busStore.setConfig({ version: 1, active: "Work" } as WorkbenchConfig);
      busStore.setWatching({ available: true });
    });

    render(<MenuBar />);

    expect(screen.getByAltText("Tinto")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("menu-workbench"));
    expect(screen.queryByTestId("workbench-empty")).not.toBeInTheDocument();
    expect(screen.getByTestId("workbench-recent-Work")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-recent-Side")).toBeInTheDocument();
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

  it("opens the manage-workbenches modal from the Workbench menu", () => {
    act(() => busStore.setConfig(config));
    render(<MenuBar />);

    expect(screen.queryByTestId("manage-workbenches-modal")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("menu-workbench"));
    expect(screen.queryByTestId("workbench-create")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("workbench-manage"));
    expect(screen.getByTestId("manage-workbenches-modal")).toBeInTheDocument();
  });

  it("creates from the manage modal, closes it, opens dashboard, and keeps prior workbenches switchable", async () => {
    const openDashboard = vi.fn();
    const actions: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile: vi.fn(),
      openTimeline: vi.fn(),
      openDashboard,
      openAgentTerminal: vi.fn(),
    };
    ops.createAndActivate.mockResolvedValue(undefined);
    act(() => busStore.setConfig(config));
    render(
      <WorkspaceActionsContext.Provider value={actions}>
        <MenuBar />
      </WorkspaceActionsContext.Provider>,
    );

    fireEvent.click(screen.getByTestId("menu-workbench"));
    fireEvent.click(screen.getByTestId("workbench-manage"));
    fireEvent.change(screen.getByTestId("manage-workbench-new-input"), {
      target: { value: "  Sandbox  " },
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId("manage-workbench-new-submit"));
      await Promise.resolve();
    });

    expect(ops.createAndActivate).toHaveBeenCalledWith("Sandbox");
    expect(openDashboard).toHaveBeenCalledWith({ closeAll: true });
    expect(screen.queryByTestId("manage-workbenches-modal")).not.toBeInTheDocument();

    act(() =>
      busStore.setConfig({
        ...config,
        active: "Sandbox",
        workbenches: [...config.workbenches, { name: "Sandbox", repos: [] }],
      }),
    );
    fireEvent.click(screen.getByTestId("menu-workbench"));

    expect(screen.getByTestId("workbench-recent-Work")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-recent-Side")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-recent-Sandbox")).toBeInTheDocument();
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
