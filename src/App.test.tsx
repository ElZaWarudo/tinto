import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act } from "@testing-library/react";

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

vi.mock("./panels/terminal/detachTerminalWindow", () => ({
  consumeTerminalDetachedMarker: vi.fn(() => false),
  markTerminalDetached: detachedWindowMocks.markTerminalDetached,
  onDetachedConsolesReattach: detachedWindowMocks.onDetachedConsolesReattach,
  openDetachedConsolesWindow: detachedWindowMocks.openDetachedConsolesWindow,
  openDetachedTerminalWindow: detachedWindowMocks.openDetachedTerminalWindow,
}));

vi.mock("./bus/connection", () => ({
  useBusConnection: () => {},
  reloadActiveWorkbench: vi.fn(),
}));
vi.mock("./panels/terminal/TerminalPanel", () => ({
  TerminalPanel: () => null,
}));

const captured = vi.hoisted(() => ({
  components: undefined as Record<string, unknown> | undefined,
}));
vi.mock("./workspace/DockWorkspace", () => ({
  DockWorkspace: (props: { components: Record<string, unknown> }) => {
    captured.components = props.components;
    return <div data-testid="workspace-stub" />;
  },
}));

import App, { detachConsolesPanel, detachConsolesPanelFromWorkspaceDrop } from "./App";
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

describe("App", () => {
  beforeEach(() => {
    busStore.resetAll();
    captured.components = undefined;
    detachedWindowMocks.markTerminalDetached.mockClear();
    detachedWindowMocks.onDetachedConsolesReattach.mockClear();
    detachedWindowMocks.onDetachedConsolesReattach.mockResolvedValue(() => {});
    detachedWindowMocks.openDetachedConsolesWindow.mockClear();
    detachedWindowMocks.openDetachedConsolesWindow.mockResolvedValue(true);
  });

  it("shows the workspace shell before the snapshot loads", () => {
    render(<App />);
    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.getByAltText("Tinto")).toBeInTheDocument(); // top bar brand
  });

  // Covers AE1 (first-run gate) + R8
  it("shows first-run when loaded with no active workbench", () => {
    act(() => busStore.loadSnapshot([], { available: true })); // loaded, no config.active
    render(<App />);
    expect(screen.getByTestId("first-run")).toBeInTheDocument();
    expect(screen.queryByTestId("workspace-stub")).not.toBeInTheDocument();
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
