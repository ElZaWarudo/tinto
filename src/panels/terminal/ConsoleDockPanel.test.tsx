import type { DockviewWillDropEvent, IDockviewPanel } from "dockview-react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  deleteAgentJournalSession: vi.fn(() => Promise.resolve(true)),
}));

const sessionStoreMocks = vi.hoisted(() => ({
  setSessions: vi.fn(),
  upsertSession: vi.fn(),
  removeSession: vi.fn(),
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
    DockviewReact: ({
      onReady,
    }: {
      onReady?: (event: { api: ReturnType<typeof nestedDockApi> }) => void;
    }) => {
      const initialOnReady = React.useRef(onReady);
      React.useEffect(() => {
        if (dockviewMocks.api) initialOnReady.current?.({ api: dockviewMocks.api });
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
  deleteAgentJournalSession: clientMocks.deleteAgentJournalSession,
}));

vi.mock("../../agent/sessionStore", () => ({
  agentSessionStore: {
    setSessions: sessionStoreMocks.setSessions,
    upsertSession: sessionStoreMocks.upsertSession,
    removeSession: sessionStoreMocks.removeSession,
  },
  useAgentSessionState: () => sessionStoreMocks.state,
}));

vi.mock("./detachTerminalWindow", () => ({
  markTerminalDetached: detachMocks.markTerminalDetached,
  openDetachedTerminalWindow: detachMocks.openDetachedTerminalWindow,
}));

import { ConsoleDockPanel } from "./ConsoleDockPanel";
import { detachTerminalFromConsoleDrop, detachTerminalPanel } from "./detachTerminalPanel";

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
    layout: vi.fn(),
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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
    localStorage.removeItem("tinto:agents-navigator:active:collapsed");
    localStorage.removeItem("tinto:agents-navigator:saved:collapsed");
    localStorage.removeItem("tinto:agents-navigator:collapsed");
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
    clientMocks.deleteAgentJournalSession.mockClear();
    clientMocks.deleteAgentJournalSession.mockResolvedValue(true);
    sessionStoreMocks.setSessions.mockClear();
    sessionStoreMocks.upsertSession.mockClear();
    sessionStoreMocks.removeSession.mockClear();
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
    expect(screen.getByRole("button", { name: /iniciar api con codex/i })).toBeInTheDocument();
    unmount();
  });

  it("shows Kimi recent launches with a text fallback", () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "kimi" });

    const { unmount } = render(<ConsoleDockPanel />);

    expect(screen.getByRole("button", { name: /iniciar api con kimi code/i })).toBeInTheDocument();
    expect(screen.getByText("Ki")).toBeInTheDocument();
    unmount();
  });

  it("starts a new session from a recent launch shortcut", async () => {
    markRecentAgentLaunch({ repo: "/r/api", agentType: "codex" });
    const openSpy = vi.spyOn(consoleDock, "openTerminal");

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(screen.getByRole("button", { name: /iniciar api con codex/i }));

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
    fireEvent.click(screen.getByRole("button", { name: /iniciar api con codex/i }));

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
      screen.getByRole("button", { name: /quitar api con codex del inicio rápido/i }),
    );

    expect(clientMocks.startAgentSession).not.toHaveBeenCalled();
    expect(readRecentAgentLaunches()).toEqual([]);
    expect(screen.queryByRole("button", { name: /iniciar api con codex/i })).toBeNull();
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
        first_user_message: "Actualiza la autenticación y revisa las sesiones expiradas",
        last_event_kind: "agent_message",
        last_event_text: "Done with the refactor",
        last_event_at_ms: 3,
      },
    ]);

    const { unmount } = render(<ConsoleDockPanel />);

    expect(await screen.findByText("Sesiones recientes")).toBeInTheDocument();
    expect(
      screen.getByText("Actualiza la autenticación y revisa las sesiones expiradas"),
    ).toBeInTheDocument();
    expect(screen.getByText("Done with the refactor")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /abrir la transcripción de api con codex/i }),
    ).toBeInTheDocument();
    unmount();
  });

  it("groups saved conversations by project without repeating the project in each row", async () => {
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-api-1",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 5,
        event_count: 3,
        first_user_message: "Corrige la autenticación",
      },
      {
        id: "sess-web-1",
        repo: "/r/web",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 4,
        event_count: 2,
        first_user_message: "Ajusta la navegación",
      },
      {
        id: "sess-api-2",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 3,
        event_count: 1,
        first_user_message: "Revisa las sesiones",
      },
    ]);

    const { unmount } = render(<ConsoleDockPanel />);

    const apiGroup = await screen.findByRole("group", { name: "api, 2 conversaciones" });
    const webGroup = screen.getByRole("group", { name: "/r/web, 1 conversación" });
    expect(within(apiGroup).getByText("Corrige la autenticación")).toBeInTheDocument();
    expect(within(apiGroup).getByText("Revisa las sesiones")).toBeInTheDocument();
    expect(within(webGroup).getByText("Ajusta la navegación")).toBeInTheDocument();
    expect(within(apiGroup).getAllByText(/^Codex · completada/)).toHaveLength(2);
    expect(within(apiGroup).queryByText(/Codex · api ·/)).toBeNull();
    unmount();
  });

  it("keeps saved conversations visible while a journal refresh is pending or fails", async () => {
    const savedSessions = [
      {
        id: "sess-old",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 3,
        event_count: 2,
        first_user_message: "Conserva esta conversación",
        last_event_kind: "agent_message",
        last_event_text: "Lista para continuar",
        last_event_at_ms: 3,
      },
    ];
    const refresh = deferred<unknown[]>();
    clientMocks.listAgentJournalSessions
      .mockResolvedValueOnce(savedSessions)
      .mockReturnValueOnce(refresh.promise)
      .mockResolvedValueOnce(savedSessions);

    const { unmount } = render(<ConsoleDockPanel />);
    expect(
      await screen.findByRole("button", { name: /abrir la transcripción de api con codex/i }),
    ).toBeInTheDocument();

    act(() => {
      consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });
    });

    await waitFor(() => expect(clientMocks.listAgentJournalSessions).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("status")).toHaveTextContent("Actualizando historial");
    expect(
      screen.getByRole("button", { name: /abrir la transcripción de api con codex/i }),
    ).toBeInTheDocument();

    await act(async () => refresh.reject(new Error("offline")));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo actualizar el historial. Se conserva la última lista disponible.",
    );
    expect(
      screen.getByRole("button", { name: /abrir la transcripción de api con codex/i }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(clientMocks.listAgentJournalSessions).toHaveBeenCalledTimes(3));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(
      screen.getByRole("button", { name: /abrir la transcripción de api con codex/i }),
    ).toBeInTheDocument();
    unmount();
  });

  it("deletes a saved conversation from its right-click menu", async () => {
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
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const closeSpy = vi.spyOn(consoleDock, "closeTerminal");

    const { unmount } = render(<ConsoleDockPanel />);
    const savedConversation = await screen.findByRole("button", {
      name: /abrir la transcripción de api con codex/i,
    });
    fireEvent.contextMenu(savedConversation, { clientX: 120, clientY: 80 });
    const menu = screen.getByRole("menu", { name: /acciones para la conversación de api/i });
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Eliminar conversación" }));

    expect(confirmSpy).toHaveBeenCalledWith(
      expect.stringContaining("¿Eliminar la conversación guardada de api?"),
    );
    await waitFor(() =>
      expect(clientMocks.deleteAgentJournalSession).toHaveBeenCalledWith("sess-old", true),
    );
    await waitFor(() => expect(savedConversation).not.toBeInTheDocument());
    expect(closeSpy).toHaveBeenCalledWith("sess-old");
    expect(sessionStoreMocks.removeSession).toHaveBeenCalledWith("sess-old");

    closeSpy.mockRestore();
    confirmSpy.mockRestore();
    unmount();
  });

  it("opens the conversation menu with Shift+F10 and returns focus on Escape", async () => {
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-old",
        repo: "/r/api",
        agent_type: "codex",
        status: "completed",
        started_at_ms: 1,
        updated_at_ms: 4,
        event_count: 0,
      },
    ]);

    const { unmount } = render(<ConsoleDockPanel />);
    const savedConversation = await screen.findByRole("button", {
      name: /abrir la transcripción de api con codex/i,
    });
    savedConversation.focus();
    fireEvent.keyDown(savedConversation, { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu", { name: /acciones para la conversación de api/i });
    fireEvent.keyDown(menu, { key: "Escape" });

    expect(screen.queryByRole("menu")).toBeNull();
    await waitFor(() => expect(savedConversation).toHaveFocus());
    expect(clientMocks.deleteAgentJournalSession).not.toHaveBeenCalled();
    unmount();
  });

  it("keeps a saved conversation visible when deletion fails", async () => {
    clientMocks.listAgentJournalSessions.mockResolvedValue([
      {
        id: "sess-live",
        repo: "/r/api",
        agent_type: "codex",
        status: "running",
        started_at_ms: 1,
        updated_at_ms: 4,
        event_count: 1,
      },
    ]);
    clientMocks.deleteAgentJournalSession.mockRejectedValueOnce(
      new Error("No se puede eliminar una sesión activa"),
    );
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);

    const { unmount } = render(<ConsoleDockPanel />);
    const savedConversation = await screen.findByRole("button", {
      name: /abrir la transcripción de api con codex/i,
    });
    fireEvent.contextMenu(savedConversation, { clientX: 120, clientY: 80 });
    fireEvent.click(screen.getByRole("menuitem", { name: "Eliminar conversación" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se eliminó la conversación guardada. Sigue disponible; vuelve a intentarlo.",
    );
    expect(savedConversation).toBeInTheDocument();
    expect(sessionStoreMocks.removeSession).not.toHaveBeenCalled();

    confirmSpy.mockRestore();
    unmount();
  });

  it("shows a recoverable error when saved sessions cannot be loaded", async () => {
    clientMocks.listAgentJournalSessions.mockRejectedValueOnce(new Error("offline"));

    const { unmount } = render(<ConsoleDockPanel />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("No se pudo cargar el historial");
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));

    await waitFor(() => expect(clientMocks.listAgentJournalSessions).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
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
    fireEvent.click(
      await screen.findByRole("button", {
        name: /abrir la transcripción de api con codex/i,
      }),
    );

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
      await screen.findByRole("complementary", { name: /sesiones de agents/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /mostrar api codex/i })).toBeInTheDocument();
    expect(screen.getByText("En curso")).toBeInTheDocument();
    expect(screen.getByText(/codex · api · trabajando/i)).toBeInTheDocument();
    expect(screen.getByText("I am updating the parser")).toBeInTheDocument();
    expect(screen.queryByTestId("console-empty")).toBeNull();
    unmount();
  });

  it("collapses the conversations navigator horizontally and remembers the choice", async () => {
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });
    const { unmount } = render(<ConsoleDockPanel />);

    expect(screen.getByRole("button", { name: /mostrar api codex/i })).toBeInTheDocument();
    fireEvent.click(
      await screen.findByRole("button", { name: "Ocultar conversaciones de Agents" }),
    );

    expect(screen.queryByRole("button", { name: /mostrar api codex/i })).toBeNull();
    expect(
      screen.getByRole("complementary", { name: "Sesiones de Agents contraídas" }),
    ).toHaveClass("console-dock-panel__navigator--collapsed");
    expect(localStorage.getItem("tinto:agents-navigator:collapsed")).toBe("1");
    unmount();

    render(<ConsoleDockPanel />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Mostrar conversaciones de Agents" }),
    );
    expect(screen.getByText("Conversaciones")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /mostrar api codex/i })).toBeInTheDocument();
  });

  it("separates conversations in progress from recent history", async () => {
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
      {
        id: "sess-live",
        repo: "/r/api",
        agent_type: "codex",
        status: "running",
        started_at_ms: 1,
        ended_at_ms: null,
        updated_at_ms: 5,
        event_count: 3,
        last_event_kind: "agent_message",
        last_event_text: "Working",
        last_event_at_ms: 5,
      },
    ]);
    const { unmount } = render(<ConsoleDockPanel />);

    expect(
      await screen.findByRole("button", {
        name: /Abrir la transcripci.n de api con Codex/i,
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("Conversaciones").parentElement).toHaveTextContent("2");
    expect(screen.getByText("En curso")).toBeInTheDocument();
    expect(screen.getByText("Recientes")).toBeInTheDocument();
    unmount();
  });

  it("focuses an active session from the side navigator", async () => {
    const id = agentTerminalPanelId("sess-live");
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });

    const { unmount } = render(<ConsoleDockPanel />);
    fireEvent.click(await screen.findByRole("button", { name: /mostrar api codex/i }));

    const panel = dockviewMocks.api?.getPanel(id);
    expect(panel?.api.setActive).toHaveBeenCalled();
    unmount();
  });

  it("makes room for the terminal when its top-level Agents group is compact", async () => {
    const id = agentTerminalPanelId("sess-live");
    consoleDock.openTerminal({ sessionId: "sess-live", repo: "/r/api", agentType: "codex" });
    const agentsGroup = {
      id: "agents-group",
      api: { location: { type: "grid" }, width: 176 },
    };
    const repoGroup = {
      id: "repo-group",
      api: { location: { type: "grid" }, width: 626 },
    };
    let maximized = false;
    const agentsPanel = {
      id: "agent-consoles",
      group: agentsGroup,
      api: {
        location: { type: "grid" },
        maximize: vi.fn(() => {
          maximized = true;
        }),
        isMaximized: vi.fn(() => maximized),
        exitMaximized: vi.fn(() => {
          maximized = false;
        }),
        setActive: vi.fn(),
      },
    };
    const repoPanel = {
      id: "repo:/r/api",
      group: repoGroup,
      api: { location: { type: "grid" } },
    };
    const containerApi = {
      width: 802,
      activeGroup: agentsGroup,
      activePanel: agentsPanel,
      panels: [agentsPanel, repoPanel],
      getPanel: vi.fn((panelId: string) =>
        panelId === "agent-consoles" ? agentsPanel : undefined,
      ),
      onDidMaximizedGroupChange: vi.fn(() => ({ dispose: vi.fn() })),
    };

    const { unmount } = render(
      <ConsoleDockPanel api={agentsPanel.api as never} containerApi={containerApi as never} />,
    );
    const dockHost = screen.getByTestId("nested-console-dock").parentElement;
    expect(dockHost).not.toBeNull();
    vi.spyOn(dockHost!, "getBoundingClientRect").mockReturnValue({
      width: 626,
      height: 540,
      top: 0,
      right: 626,
      bottom: 540,
      left: 0,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    fireEvent.click(await screen.findByRole("button", { name: /mostrar api codex/i }));

    expect(agentsPanel.api.maximize).toHaveBeenCalledOnce();
    expect(agentsPanel.api.setActive).toHaveBeenCalled();
    expect(dockviewMocks.api?.getPanel(id)?.api.setActive).toHaveBeenCalled();
    await waitFor(() => expect(dockviewMocks.api?.layout).toHaveBeenCalledWith(626, 540, true));
    fireEvent.click(screen.getByRole("button", { name: "Restaurar" }));
    expect(agentsPanel.api.exitMaximized).toHaveBeenCalledOnce();
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
    fireEvent.click(
      await screen.findByRole("button", {
        name: /abrir la transcripción de api con codex/i,
      }),
    );

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
