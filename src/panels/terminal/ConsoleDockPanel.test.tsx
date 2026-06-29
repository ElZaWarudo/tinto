import type { DockviewWillDropEvent, IDockviewPanel } from "dockview-react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { consoleDock } from "../../workspace/consoleDock";
import { agentTerminalPanelId } from "../../workspace/panels";
import {
  clearRecentAgentLaunchesForTests,
  markRecentAgentLaunch,
  readRecentAgentLaunches,
} from "../../workspace/recentAgentLaunches";

const detachMocks = vi.hoisted(() => ({
  markTerminalDetached: vi.fn(),
  openDetachedTerminalWindow: vi.fn<() => Promise<boolean>>(() => Promise.resolve(true)),
}));

const dockviewMocks = vi.hoisted(() => ({
  api: null as null | ReturnType<typeof nestedDockApi>,
}));

const busMocks = vi.hoisted(() => ({
  repos: { "/r/api": {} },
  displayName: vi.fn((path: string) => (path === "/r/api" ? "api" : path)),
}));

const clientMocks = vi.hoisted(() => ({
  startAgentSession: vi.fn(() => Promise.resolve("sess-new")),
  listAgentSessions: vi.fn(() => Promise.resolve([])),
}));

vi.mock("dockview-react", async () => {
  const React = await import("react");
  return {
    DockviewReact: (props: {
      onReady?: (event: { api: ReturnType<typeof nestedDockApi> }) => void;
    }) => {
      React.useEffect(() => {
        if (dockviewMocks.api) props.onReady?.({ api: dockviewMocks.api });
      }, []);
      return <div data-testid="nested-console-dock" />;
    },
    themeVisualStudio: {},
  };
});

vi.mock("../../bus/store", () => ({
  useBusState: () => ({ repos: busMocks.repos }),
  busStore: { displayName: busMocks.displayName },
}));

vi.mock("../../bus/client", () => ({
  startAgentSession: clientMocks.startAgentSession,
  listAgentSessions: clientMocks.listAgentSessions,
}));

vi.mock("../../agent/sessionStore", () => ({
  agentSessionStore: { setSessions: vi.fn() },
}));

vi.mock("./detachTerminalWindow", () => ({
  markTerminalDetached: detachMocks.markTerminalDetached,
  openDetachedTerminalWindow: detachMocks.openDetachedTerminalWindow,
}));

import {
  ConsoleDockPanel,
  detachTerminalFromConsoleDrop,
  detachTerminalPanel,
} from "./ConsoleDockPanel";

function nestedDockApi() {
  const panels: Record<string, IDockviewPanel> = {};
  return {
    panels: [],
    addPanel: vi.fn((opts: { id: string; params?: unknown }) => {
      const panel = {
        id: opts.id,
        params: opts.params,
        api: { setActive: vi.fn(), location: { type: "grid" } },
      } as unknown as IDockviewPanel;
      panels[opts.id] = panel;
      return panel;
    }),
    getPanel: vi.fn((id: string) => panels[id]),
    removePanel: vi.fn(),
    fromJSON: vi.fn(),
    toJSON: vi.fn(() => ({ panels: {} })),
    onDidRemovePanel: vi.fn(() => ({ dispose: vi.fn() })),
    onDidLayoutChange: vi.fn(() => ({ dispose: vi.fn() })),
    onDidMovePanel: vi.fn(() => ({ dispose: vi.fn() })),
    onWillDragPanel: vi.fn(() => ({ dispose: vi.fn() })),
  };
}

function dropEvent(panelId: string | null, kind = "edge") {
  return {
    kind,
    getData: vi.fn(() => ({ panelId })),
    preventDefault: vi.fn(),
  } as unknown as DockviewWillDropEvent;
}

function dockApi(panel?: IDockviewPanel) {
  return {
    getPanel: vi.fn((id: string) => (panel?.id === id ? panel : undefined)),
    removePanel: vi.fn(),
  };
}

describe("ConsoleDockPanel detach drop", () => {
  beforeEach(() => {
    consoleDock.resetForTests();
    clearRecentAgentLaunchesForTests();
    dockviewMocks.api = nestedDockApi();
    busMocks.repos = { "/r/api": {} };
    busMocks.displayName.mockClear();
    clientMocks.startAgentSession.mockClear();
    clientMocks.startAgentSession.mockResolvedValue("sess-new");
    clientMocks.listAgentSessions.mockClear();
    clientMocks.listAgentSessions.mockResolvedValue([]);
    detachMocks.markTerminalDetached.mockClear();
    detachMocks.openDetachedTerminalWindow.mockClear();
    detachMocks.openDetachedTerminalWindow.mockResolvedValue(true);
  });

  it("opens a detached terminal window and removes the embedded tab on edge drop", async () => {
    const panelId = agentTerminalPanelId("sess-123456789");
    const panel = {
      id: panelId,
      params: { sessionId: "sess-123456789", repo: "/r/api", agentType: "codex" },
    } as unknown as IDockviewPanel;
    const api = dockApi(panel);
    const event = dropEvent(panelId);

    const detached = await detachTerminalFromConsoleDrop(event, api as never);

    expect(detached).toBe(true);
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(detachMocks.openDetachedTerminalWindow).toHaveBeenCalledWith({
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });
    expect(detachMocks.markTerminalDetached).toHaveBeenCalledWith("sess-123456789");
    expect(api.removePanel).toHaveBeenCalledWith(panel);
  });

  it("ignores non-terminal drops", async () => {
    const api = dockApi();
    const event = dropEvent("repo:/r/api");

    const detached = await detachTerminalFromConsoleDrop(event, api as never);

    expect(detached).toBe(false);
    expect(event.preventDefault).not.toHaveBeenCalled();
    expect(detachMocks.openDetachedTerminalWindow).not.toHaveBeenCalled();
    expect(api.removePanel).not.toHaveBeenCalled();
  });

  it("detaches a terminal panel after Dockview moves it to floating", async () => {
    const panelId = agentTerminalPanelId("sess-123456789");
    const panel = {
      id: panelId,
      params: { sessionId: "sess-123456789", repo: "/r/api", agentType: "codex" },
    } as unknown as IDockviewPanel;
    const api = dockApi(panel);

    const detached = await detachTerminalPanel(api as never, panelId, panel);

    expect(detached).toBe(true);
    expect(detachMocks.openDetachedTerminalWindow).toHaveBeenCalledWith({
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });
    expect(detachMocks.markTerminalDetached).toHaveBeenCalledWith("sess-123456789");
    expect(api.removePanel).toHaveBeenCalledWith(panel);
  });

  it("shows recent agent launch shortcuts in the empty console state", () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "codex" });

    const { unmount } = render(<ConsoleDockPanel />);

    expect(screen.getByTestId("console-empty")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /launch api with codex/i })).toBeInTheDocument();
    unmount();
  });

  it("starts a new session from a recent launch shortcut", async () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "codex" });
    const openSpy = vi.spyOn(consoleDock, "openTerminal");

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(screen.getByRole("button", { name: /launch api with codex/i }));

    await waitFor(() =>
      expect(clientMocks.startAgentSession).toHaveBeenCalledWith("/r/api", "codex"),
    );
    await waitFor(() =>
      expect(openSpy).toHaveBeenCalledWith({
        sessionId: "sess-new",
        repo: "/r/api",
        agentType: "codex",
      }),
    );
    expect(dockviewMocks.api?.addPanel).toHaveBeenCalledWith(
      expect.objectContaining({
        id: agentTerminalPanelId("sess-new"),
        component: "agent-terminal",
      }),
    );

    openSpy.mockRestore();
    unmount();
  });

  it("opens the new terminal even if the session list refresh fails", async () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "codex" });
    clientMocks.listAgentSessions.mockRejectedValueOnce(new Error("refresh failed"));
    const openSpy = vi.spyOn(consoleDock, "openTerminal");

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(screen.getByRole("button", { name: /launch api with codex/i }));

    await waitFor(() =>
      expect(openSpy).toHaveBeenCalledWith({
        sessionId: "sess-new",
        repo: "/r/api",
        agentType: "codex",
      }),
    );

    openSpy.mockRestore();
    unmount();
  });

  it("removes a recent launch shortcut without starting a session", () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "codex" });

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(
      screen.getByRole("button", { name: /remove api with codex from quick launch/i }),
    );

    expect(clientMocks.startAgentSession).not.toHaveBeenCalled();
    expect(readRecentAgentLaunches()).toEqual([]);
    expect(screen.queryByRole("button", { name: /launch api with codex/i })).toBeNull();
    unmount();
  });
});
