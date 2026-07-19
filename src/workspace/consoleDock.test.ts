import { beforeEach, describe, expect, it, vi } from "vitest";
import { agentConversationTabTitle, consoleDock } from "./consoleDock";
import { PANEL_AGENT_TERMINAL, TAB_AGENT_CONVERSATION, agentTerminalPanelId } from "./panels";

function fakeApi() {
  const removePanelCallbacks: Array<(panel: (typeof panels)[string]) => void> = [];
  const layoutCallbacks: Array<() => void> = [];
  const panels: Record<
    string,
    {
      id: string;
      api: { setActive: ReturnType<typeof vi.fn> };
      title?: string;
      component?: string;
      params?: unknown;
    }
  > = {};
  const panelList: Array<(typeof panels)[string]> = [];
  return {
    get panels() {
      return panelList;
    },
    get activePanel() {
      return panelList[panelList.length - 1];
    },
    addPanel: vi.fn(
      (opts: { id: string; component?: string; title?: string; params?: unknown }) => {
        panels[opts.id] = {
          id: opts.id,
          api: { setActive: vi.fn() },
          title: opts.title,
          component: opts.component,
          params: opts.params,
        };
        panelList.push(panels[opts.id]);
        return panels[opts.id];
      },
    ),
    getPanel: vi.fn((id: string) => panels[id]),
    removePanel: vi.fn((panel: (typeof panels)[string]) => {
      delete panels[panel.id];
      const index = panelList.findIndex((candidate) => candidate.id === panel.id);
      if (index >= 0) panelList.splice(index, 1);
      removePanelCallbacks.forEach((callback) => callback(panel));
      layoutCallbacks.forEach((callback) => callback());
    }),
    onDidLayoutChange: vi.fn((callback: () => void) => {
      layoutCallbacks.push(callback);
      return { dispose: vi.fn() };
    }),
    onDidRemovePanel: vi.fn((callback: (panel: (typeof panels)[string]) => void) => {
      removePanelCallbacks.push(callback);
      return { dispose: vi.fn() };
    }),
    fromJSON: vi.fn(),
    toJSON: vi.fn(() => ({
      panels: Object.fromEntries(panelList.map((panel) => [panel.id, { params: panel.params }])),
    })),
    _panels: panels,
  };
}

describe("consoleDock", () => {
  it("identifies a conversation tab with its agent, project, and first-message title", () => {
    expect(
      agentConversationTabTitle(
        { sessionId: "sess-1", repo: "/work/api", agentType: "codex" },
        "Corrige la autenticación",
      ),
    ).toBe("Codex · api · Corrige la autenticación");
  });

  it("labels Kimi conversation tabs without requiring an image asset", () => {
    expect(
      agentConversationTabTitle(
        { sessionId: "sess-kimi", repo: "/work/api", agentType: "kimi" },
        "Revisa el parser",
      ),
    ).toBe("Kimi Code · api · Revisa el parser");
  });

  beforeEach(() => {
    consoleDock.resetForTests();
  });

  it("queues terminal opens until the nested dock registers", () => {
    const api = fakeApi();

    consoleDock.openTerminal({
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });
    expect(api.addPanel).not.toHaveBeenCalled();

    consoleDock.register(api as never);

    expect(api.addPanel).toHaveBeenCalledWith({
      id: agentTerminalPanelId("sess-123456789"),
      component: PANEL_AGENT_TERMINAL,
      tabComponent: TAB_AGENT_CONVERSATION,
      title: "Codex · api",
      params: {
        sessionId: "sess-123456789",
        repo: "/r/api",
        agentType: "codex",
      },
    });
  });

  it("focuses an existing terminal tab instead of adding it twice", () => {
    const api = fakeApi();
    consoleDock.register(api as never);

    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/api", agentType: "codex" });
    const panel = api._panels[agentTerminalPanelId("sess-1")];
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/api", agentType: "codex" });

    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(panel.api.setActive).toHaveBeenCalledOnce();
  });

  it("deduplicates queued opens for the same session id", () => {
    const api = fakeApi();

    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.register(api as never);

    expect(api.addPanel).toHaveBeenCalledTimes(1);
  });

  it("labels tabs by agent and project and only numbers repeated chats", () => {
    const api = fakeApi();
    consoleDock.register(api as never);

    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/api", agentType: "codex" });
    consoleDock.openTerminal({ sessionId: "sess-2", repo: "/r/api", agentType: "codex" });
    consoleDock.openTerminal({ sessionId: "sess-3", repo: "/r/web", agentType: "codex" });

    expect(api._panels[agentTerminalPanelId("sess-1")].title).toBe("Codex · api");
    expect(api._panels[agentTerminalPanelId("sess-2")].title).toBe("Codex · api · Chat 2");
    expect(api._panels[agentTerminalPanelId("sess-3")].title).toBe("Codex · web");
  });

  it("does not steal focus from existing terminal tabs while remounting", () => {
    const api = fakeApi();
    api.addPanel({
      id: agentTerminalPanelId("sess-1"),
      component: PANEL_AGENT_TERMINAL,
      title: "codex sess-1",
      params: { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
    });
    const panel = api._panels[agentTerminalPanelId("sess-1")];
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    panel.api.setActive.mockClear();

    consoleDock.register(api as never);

    expect(panel.api.setActive).not.toHaveBeenCalled();
  });

  it("notifies listeners when terminal state changes", () => {
    const api = fakeApi();
    const listener = vi.fn();
    const unsubscribe = consoleDock.subscribe(listener);

    consoleDock.register(api as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.unregister(api as never);
    unsubscribe();
    consoleDock.register(fakeApi() as never);

    expect(listener).toHaveBeenCalledTimes(4);
  });

  it("queues new terminal opens after the consoles dock is closed", () => {
    const firstApi = fakeApi();
    consoleDock.register(firstApi as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.unregister(firstApi as never);

    consoleDock.openTerminal({ sessionId: "sess-2", repo: "/r/b", agentType: "codex" });
    expect(firstApi.addPanel).toHaveBeenCalledTimes(1);

    const secondApi = fakeApi();
    consoleDock.register(secondApi as never);

    expect(secondApi.addPanel).toHaveBeenCalledWith({
      id: agentTerminalPanelId("sess-2"),
      component: PANEL_AGENT_TERMINAL,
      tabComponent: TAB_AGENT_CONVERSATION,
      title: "Codex · b",
      params: {
        sessionId: "sess-2",
        repo: "/r/b",
        agentType: "codex",
      },
    });
  });

  it("restores tracked terminal tabs when the nested dock remounts", () => {
    const firstApi = fakeApi();
    consoleDock.register(firstApi as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.unregister(firstApi as never);

    const secondApi = fakeApi();
    consoleDock.register(secondApi as never);

    expect(secondApi.addPanel).toHaveBeenCalledWith({
      id: agentTerminalPanelId("sess-1"),
      component: PANEL_AGENT_TERMINAL,
      tabComponent: TAB_AGENT_CONVERSATION,
      title: "Codex · a",
      params: {
        sessionId: "sess-1",
        repo: "/r/a",
        agentType: "codex",
      },
    });
  });

  it("does not restore a terminal tab after it is closed", () => {
    vi.useFakeTimers();
    const firstApi = fakeApi();
    consoleDock.register(firstApi as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    firstApi.removePanel(firstApi._panels[agentTerminalPanelId("sess-1")]);
    vi.advanceTimersByTime(250);
    consoleDock.unregister(firstApi as never);

    const secondApi = fakeApi();
    consoleDock.register(secondApi as never);

    expect(secondApi.addPanel).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("closes and forgets a terminal by session id", () => {
    const firstApi = fakeApi();
    consoleDock.register(firstApi as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    const panel = firstApi._panels[agentTerminalPanelId("sess-1")];

    consoleDock.closeTerminal("sess-1");

    expect(firstApi.removePanel).toHaveBeenCalledWith(panel);
    expect(consoleDock.openTerminalSessionIds()).toEqual([]);
    consoleDock.unregister(firstApi as never);
    const secondApi = fakeApi();
    consoleDock.register(secondApi as never);
    expect(secondApi.addPanel).not.toHaveBeenCalled();
  });

  it("does not restore stale console layouts from previous app runs", () => {
    localStorage.setItem("tinto:console-dock", JSON.stringify({ panels: { a: {} } }));
    const api = fakeApi();

    consoleDock.register(api as never);

    expect(api.fromJSON).not.toHaveBeenCalled();
    expect(localStorage.getItem("tinto:console-dock")).toBeNull();
  });

  it("restores a one-shot transfer layout for detached console windows", () => {
    const sourceApi = fakeApi();
    consoleDock.register(sourceApi as never);
    consoleDock.openTerminal({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" });
    consoleDock.prepareDetachedTransfer();
    consoleDock.unregister(sourceApi as never);

    const detachedApi = fakeApi();
    consoleDock.register(detachedApi as never, { restoreTransferLayout: true });

    expect(detachedApi.fromJSON).toHaveBeenCalledWith({
      panels: {
        [agentTerminalPanelId("sess-1")]: {
          params: {
            sessionId: "sess-1",
            repo: "/r/a",
            agentType: "codex",
          },
        },
      },
    });
    expect(localStorage.getItem("tinto:console-dock-transfer")).toBeNull();
  });
});
