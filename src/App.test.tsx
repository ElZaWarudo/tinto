import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, fireEvent, waitFor } from "@testing-library/react";

// Avoid rendering dockview / hitting Tauri in jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const detachedWindowMocks = vi.hoisted(() => ({
  markTerminalDetached: vi.fn(),
  onDetachedConsolesReattach: vi.fn(() => Promise.resolve(() => {})),
  openDetachedConsolesWindow: vi.fn<() => Promise<boolean>>(() => Promise.resolve(true)),
  openDetachedTerminalWindow: vi.fn<() => Promise<boolean>>(() => Promise.resolve(true)),
}));

const connectionMocks = vi.hoisted(() => ({
  reloadActiveWorkbench: vi.fn(() => Promise.resolve()),
}));

vi.mock("./panels/terminal/detachTerminalWindow", () => ({
  consumeTerminalDetachedMarker: vi.fn(() => false),
  markTerminalDetached: detachedWindowMocks.markTerminalDetached,
  onDetachedConsolesReattach: detachedWindowMocks.onDetachedConsolesReattach,
  openDetachedConsolesWindow: detachedWindowMocks.openDetachedConsolesWindow,
  openDetachedTerminalWindow: detachedWindowMocks.openDetachedTerminalWindow,
}));

vi.mock("./bus/connection", () => ({
  useBusConnection: () => {},
  reloadActiveWorkbench: connectionMocks.reloadActiveWorkbench,
}));
vi.mock("./panels/terminal/TerminalPanel", () => ({
  TerminalPanel: () => null,
}));

const captured = vi.hoisted(() => ({
  components: undefined as Record<string, unknown> | undefined,
  props: undefined as { onApi?: (api: unknown) => void } | undefined,
}));
vi.mock("./workspace/DockWorkspace", () => ({
  DockWorkspace: (props: {
    components: Record<string, unknown>;
    onApi?: (api: unknown) => void;
  }) => {
    captured.components = props.components;
    captured.props = props;
    return <div data-testid="workspace-stub" />;
  },
}));

import App from "./App";
import {
  detachConsolesPanel,
  detachConsolesPanelFromWorkspaceDrop,
} from "./workspace/detachConsoles";
import { busStore } from "./bus/store";
import { closePanelsForRemovedRepo } from "./workspace/closePanels";
import { consoleDock } from "./workspace/consoleDock";
import {
  PANEL_AGENT_CONSOLES,
  PANEL_AGENT_TERMINAL,
  PANEL_DASHBOARD,
  PANEL_REPO,
  PANEL_TIMELINE,
  repoPanelId,
} from "./workspace/panels";
import type { WorkbenchConfig } from "./bus/contract";
import { open } from "@tauri-apps/plugin-dialog";
import { setWindowsHostOverrideForTests } from "./workbench/platform";

describe("App", () => {
  beforeEach(() => {
    busStore.resetAll();
    captured.components = undefined;
    captured.props = undefined;
    detachedWindowMocks.markTerminalDetached.mockClear();
    detachedWindowMocks.onDetachedConsolesReattach.mockClear();
    detachedWindowMocks.onDetachedConsolesReattach.mockResolvedValue(() => {});
    detachedWindowMocks.openDetachedConsolesWindow.mockClear();
    detachedWindowMocks.openDetachedConsolesWindow.mockResolvedValue(true);
    connectionMocks.reloadActiveWorkbench.mockClear();
    setWindowsHostOverrideForTests(null);
    vi.mocked(open).mockReset();
  });

  it("shows an explicit startup state before configuration and snapshot load", () => {
    render(<App />);
    expect(screen.getByTestId("startup-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("workspace-stub")).not.toBeInTheDocument();
    expect(screen.getByAltText("Tinto")).toBeInTheDocument();
  });

  it("mounts the workspace as soon as config is ready while repos are still loading", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      });
    });

    render(<App />);

    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.queryByTestId("startup-loading")).not.toBeInTheDocument();
  });

  // Covers AE1 (first-run gate) + R8
  it("shows first-run when loaded with no active workbench", () => {
    act(() => {
      busStore.setConfig({ version: 1, active: null, workbenches: [] });
      busStore.loadSnapshot([], { available: true });
    });
    render(<App />);
    expect(screen.getByTestId("first-run")).toBeInTheDocument();
    expect(screen.queryByTestId("workspace-stub")).not.toBeInTheDocument();
  });

  it("shows a recoverable startup failure instead of first-run when config loading fails", () => {
    act(() => {
      busStore.setConfigError("backend offline");
      busStore.loadSnapshot([], { available: false, reason: "backend offline" });
    });
    render(<App />);

    expect(screen.getByTestId("startup-failure")).toHaveTextContent("backend offline");
    expect(screen.queryByTestId("first-run")).not.toBeInTheDocument();

    screen.getByRole("button", { name: /reintentar conexi/i }).click();
    expect(connectionMocks.reloadActiveWorkbench).toHaveBeenCalledOnce();
  });

  it("keeps an already usable workspace mounted during background reloads", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      });
      busStore.loadSnapshot([], { available: true });
      busStore.beginConfigLoad();
      busStore.beginSnapshotLoad();
    });

    render(<App />);

    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.queryByTestId("startup-loading")).not.toBeInTheDocument();
  });

  it("surfaces a background snapshot failure without discarding the workspace", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      });
      busStore.loadSnapshot([], { available: true });
      busStore.setSnapshotError("snapshot offline");
    });

    render(<App />);

    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.getByTestId("app-shell-error")).toHaveTextContent("snapshot offline");
  });

  it("shows connection channel failures globally without discarding the workspace", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      });
      busStore.loadSnapshot([], { available: true });
      busStore.setConnectionError(
        "agent-session-list",
        "Listado de sesiones Agent: backend offline",
      );
    });

    render(<App />);

    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.getByTestId("connection-errors-banner")).toHaveTextContent(
      "Listado de sesiones Agent: backend offline",
    );
    expect(screen.getByTestId("connection-errors-banner")).toHaveTextContent(
      "Se conserva el último estado disponible mientras Tinto reconecta",
    );
  });

  it("surfaces local-picker failures from the non-Windows add-repo action", async () => {
    setWindowsHostOverrideForTests(false);
    vi.mocked(open).mockRejectedValueOnce(new Error("selector no disponible"));
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      });
      busStore.loadSnapshot([], { available: true });
    });
    render(<App />);

    fireEvent.click(screen.getByTestId("menu-repos"));
    fireEvent.click(screen.getByTestId("add-repo"));

    await waitFor(() =>
      expect(screen.getByTestId("app-shell-error")).toHaveTextContent("selector no disponible"),
    );
  });

  it("shows the workspace with all panel types registered when a workbench is active", () => {
    const config: WorkbenchConfig = {
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [] }],
    };
    act(() => {
      busStore.setConfig(config);
      busStore.loadSnapshot([], { available: true });
    });
    render(<App />);
    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(Object.keys(captured.components ?? {})).toEqual(
      expect.arrayContaining([
        PANEL_DASHBOARD,
        PANEL_REPO,
        PANEL_TIMELINE,
        PANEL_AGENT_CONSOLES,
        PANEL_AGENT_TERMINAL,
      ]),
    );
  });

  it("closes the project tab for a removed repo without touching other repos", () => {
    const closed: string[] = [];
    const panel = (id: string) => ({ id, api: { close: () => closed.push(id) } });
    const panels = [panel(repoPanelId("/r/a")), panel(repoPanelId("/r/b"))];
    const api = {
      panels,
      getPanel: (id: string) => panels.find((p) => p.id === id),
    };

    closePanelsForRemovedRepo(api as never, "/r/a");

    expect(closed).toEqual([repoPanelId("/r/a")]);
  });

  it("closes repo project tabs outside the active workbench after a workbench switch", () => {
    const workConfig: WorkbenchConfig = {
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            { path: "/r/a", alias: null, fs_watch: [] },
            { path: "/r/shared", alias: null, fs_watch: [] },
          ],
        },
        {
          name: "Side",
          repos: [
            { path: "/r/shared", alias: null, fs_watch: [] },
            { path: "/r/c", alias: null, fs_watch: [] },
          ],
        },
      ],
    };
    const sideConfig: WorkbenchConfig = { ...workConfig, active: "Side" };
    const closed: string[] = [];
    const panel = (id: string) => ({ id, api: { close: () => closed.push(id) } });
    const panels = [panel(repoPanelId("/r/a")), panel(repoPanelId("/r/shared"))];
    const api = {
      panels,
      getPanel: (id: string) => panels.find((p) => p.id === id),
    };

    act(() => {
      busStore.setConfig(workConfig);
      busStore.loadSnapshot([], { available: true });
    });
    render(<App />);
    act(() => captured.props?.onApi?.(api));
    expect(closed).toEqual([]);

    act(() => busStore.setConfig(sideConfig));

    expect(closed).toEqual([repoPanelId("/r/a")]);
  });

  it("detaches the top-level Agents panel when its tab is dropped on the workspace edge", async () => {
    const panel = { id: PANEL_AGENT_CONSOLES };
    const api = {
      getPanel: vi.fn((id: string) => (id === PANEL_AGENT_CONSOLES ? panel : undefined)),
      removePanel: vi.fn(),
    };
    const event = {
      kind: "edge",
      getData: vi.fn(() => ({ panelId: PANEL_AGENT_CONSOLES })),
      preventDefault: vi.fn(),
    };
    const saveNow = vi.spyOn(consoleDock, "saveNow").mockImplementation(() => {});
    const sessionIds = vi
      .spyOn(consoleDock, "openTerminalSessionIds")
      .mockReturnValue(["sess-1", "sess-2"]);
    const terminalParams = vi.spyOn(consoleDock, "openTerminalParams").mockReturnValue([
      { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
      { sessionId: "sess-2", repo: "/r/b", agentType: "codex" },
    ]);

    const detached = await detachConsolesPanelFromWorkspaceDrop(event as never, api as never);

    expect(detached).toBe(true);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(saveNow).toHaveBeenCalledOnce();
    expect(detachedWindowMocks.openDetachedConsolesWindow).toHaveBeenCalledWith([
      { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
      { sessionId: "sess-2", repo: "/r/b", agentType: "codex" },
    ]);
    expect(detachedWindowMocks.markTerminalDetached).toHaveBeenCalledWith("sess-1");
    expect(detachedWindowMocks.markTerminalDetached).toHaveBeenCalledWith("sess-2");
    expect(api.removePanel).toHaveBeenCalledWith(panel);

    saveNow.mockRestore();
    sessionIds.mockRestore();
    terminalParams.mockRestore();
  });

  it("detaches the top-level Agents panel after Dockview moves it to floating", async () => {
    const panel = { id: PANEL_AGENT_CONSOLES };
    const api = {
      getPanel: vi.fn((id: string) => (id === PANEL_AGENT_CONSOLES ? panel : undefined)),
      removePanel: vi.fn(),
    };
    const saveNow = vi.spyOn(consoleDock, "saveNow").mockImplementation(() => {});
    const sessionIds = vi.spyOn(consoleDock, "openTerminalSessionIds").mockReturnValue(["sess-1"]);
    const terminalParams = vi
      .spyOn(consoleDock, "openTerminalParams")
      .mockReturnValue([{ sessionId: "sess-1", repo: "/r/a", agentType: "codex" }]);

    const detached = await detachConsolesPanel(api as never, panel as never);

    expect(detached).toBe(true);
    expect(detachedWindowMocks.openDetachedConsolesWindow).toHaveBeenCalledWith([
      { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
    ]);
    expect(detachedWindowMocks.markTerminalDetached).toHaveBeenCalledWith("sess-1");
    expect(api.removePanel).toHaveBeenCalledWith(panel);

    saveNow.mockRestore();
    sessionIds.mockRestore();
    terminalParams.mockRestore();
  });
});
