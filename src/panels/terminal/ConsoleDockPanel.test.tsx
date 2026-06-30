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
  listAgentJournalSessions: vi.fn<() => Promise<unknown[]>>(() => Promise.resolve([])),
  getAgentJournalSession: vi.fn<() => Promise<unknown | null>>(() => Promise.resolve(null)),
}));

const sessionStoreMocks = vi.hoisted(() => ({
  setSessions: vi.fn(),
  upsertSession: vi.fn(),
  state: {
    sessions: {} as Record<string, unknown>,
    output: {},
    outputTotal: {},
    timeline: {} as Record<string, unknown[]>,
  },
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
  listAgentJournalSessions: clientMocks.listAgentJournalSessions,
  getAgentJournalSession: clientMocks.getAgentJournalSession,
}));

vi.mock("../../agent/sessionStore", () => ({
  agentSessionStore: {
    setSessions: sessionStoreMocks.setSessions,
    upsertSession: sessionStoreMocks.upsertSession,
  },
  useAgentSessionState: () => sessionStoreMocks.state,
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
  const panelList: IDockviewPanel[] = [];
  return {
    panels: panelList,
    addPanel: vi.fn((opts: { id: string; params?: unknown }) => {
      const panel = {
        id: opts.id,
        params: opts.params,
        api: { setActive: vi.fn(), location: { type: "grid" } },
      } as unknown as IDockviewPanel;
      panels[opts.id] = panel;
      panelList.push(panel);
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
    clientMocks.listAgentJournalSessions.mockClear();
    clientMocks.listAgentJournalSessions.mockResolvedValue([]);
    clientMocks.getAgentJournalSession.mockClear();
    clientMocks.getAgentJournalSession.mockResolvedValue(null);
    sessionStoreMocks.setSessions.mockClear();
    sessionStoreMocks.upsertSession.mockClear();
    sessionStoreMocks.state = { sessions: {}, output: {}, outputTotal: {}, timeline: {} };
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

  it("shows saved agent session transcripts in the empty agents home", async () => {
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-old",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 3,
        event_count: 2,
        last_event_kind: "agent_message",
        last_event_text: "Done with the refactor",
        last_event_at_ms: 3,
      },
    ]);

    const { unmount } = render(<ConsoleDockPanel />);

    expect(await screen.findByText("Recent sessions")).toBeInTheDocument();
    expect(screen.getByText("Done with the refactor")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open api codex transcript/i })).toBeInTheDocument();
    unmount();
  });

  it("opens a saved transcript as a read-only agent panel", async () => {
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-old",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        ended_at_ms: 4,
        updated_at_ms: 4,
        event_count: 2,
        last_event_kind: "agent_message",
        last_event_text: "Done",
        last_event_at_ms: 4,
      },
    ]);
    const session = {
      id: "sess-old",
      repo: "/r/api",
      agent_type: "codex",
      status: "completed",
      pid: null,
      started_at_ms: 1,
      ended_at_ms: 4,
      exit_code: null,
      error: null,
      checkpoint: null,
      change_log: [],
      turn_status: "waiting",
      turn_checkpoints: [],
      timeline: [],
      reverted_at_ms: null,
      active_sessions: 0,
      age_ms: 3,
      output_bytes_per_second: null,
    };
    clientMocks.getAgentJournalSession.mockResolvedValueOnce(session);
    const openSpy = vi.spyOn(consoleDock, "openTerminal");

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(await screen.findByRole("button", { name: /open api codex transcript/i }));

    await waitFor(() =>
      expect(clientMocks.getAgentJournalSession).toHaveBeenCalledWith("sess-old"),
    );
    expect(sessionStoreMocks.upsertSession).toHaveBeenCalledWith(session);
    expect(openSpy).toHaveBeenCalledWith({
      sessionId: "sess-old",
      repo: "/r/api",
      agentType: "codex",
      mode: "journal",
    });
    expect(dockviewMocks.api?.addPanel).toHaveBeenCalledWith(
      expect.objectContaining({
        id: agentTerminalPanelId("sess-old"),
        component: "agent-terminal",
        params: expect.objectContaining({ mode: "journal" }),
      }),
    );

    openSpy.mockRestore();
    unmount();
  });

  it("shows a side navigator for active agent sessions", async () => {
    sessionStoreMocks.state = {
      sessions: {
        "sess-live": {
          id: "sess-live",
          repo: "/r/api",
          agent_type: "codex",
          status: "running",
          pid: 1,
          started_at_ms: 1,
          exit_code: null,
          error: null,
          turn_status: "working",
          active_sessions: 1,
          age_ms: 4,
        },
      },
      output: {},
      outputTotal: {},
      timeline: {
        "sess-live": [
          {
            session_id: "sess-live",
            id: "evt-1",
            kind: "agent_message",
            text: "I am updating the parser",
            timestamp_ms: 2,
          },
        ],
      },
    };
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });

    const { unmount } = render(<ConsoleDockPanel />);

    expect(
      await screen.findByRole("complementary", { name: /agent sessions/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /focus api codex/i })).toBeInTheDocument();
    expect(screen.getByText(/codex \/ working/i)).toBeInTheDocument();
    expect(screen.getByText("I am updating the parser")).toBeInTheDocument();
    expect(screen.queryByTestId("console-empty")).toBeNull();
    unmount();
  });

  it("focuses an active session from the side navigator", async () => {
    const id = agentTerminalPanelId("sess-live");
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(await screen.findByRole("button", { name: /focus api codex/i }));

    const panel = dockviewMocks.api?.getPanel(id);
    expect(panel?.api.setActive).toHaveBeenCalled();
    unmount();
  });

  it("opens saved transcripts from the side navigator while an agent is active", async () => {
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-old",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        ended_at_ms: 4,
        updated_at_ms: 4,
        event_count: 2,
        last_event_kind: "agent_message",
        last_event_text: "Done",
        last_event_at_ms: 4,
      },
    ]);
    const session = {
      id: "sess-old",
      repo: "/r/api",
      agent_type: "codex",
      status: "completed",
      pid: null,
      started_at_ms: 1,
      ended_at_ms: 4,
      exit_code: null,
      error: null,
      checkpoint: null,
      change_log: [],
      turn_status: "waiting",
      turn_checkpoints: [],
      timeline: [],
      reverted_at_ms: null,
      active_sessions: 0,
      age_ms: 3,
      output_bytes_per_second: null,
    };
    clientMocks.getAgentJournalSession.mockResolvedValueOnce(session);

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(await screen.findByRole("button", { name: /open api codex transcript/i }));

    await waitFor(() =>
      expect(clientMocks.getAgentJournalSession).toHaveBeenCalledWith("sess-old"),
    );
    expect(dockviewMocks.api?.addPanel).toHaveBeenCalledWith(
      expect.objectContaining({
        id: agentTerminalPanelId("sess-old"),
        params: expect.objectContaining({ mode: "journal" }),
      }),
    );
    unmount();
  });
});
