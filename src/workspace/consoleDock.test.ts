import { beforeEach, describe, expect, it, vi } from "vitest";
import { consoleDock } from "./consoleDock";
import { PANEL_AGENT_TERMINAL, agentTerminalPanelId } from "./panels";

function fakeApi() {
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
    onDidLayoutChange: vi.fn(() => ({ dispose: vi.fn() })),
    fromJSON: vi.fn(),
    toJSON: vi.fn(() => ({
      panels: Object.fromEntries(panelList.map((panel) => [panel.id, { params: panel.params }])),
    })),
    _panels: panels,
  };
}

describe("consoleDock", () => {
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
      title: "codex sess-123",
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

  it("restores a saved console layout when the nested dock registers", () => {
    localStorage.setItem("tinto:console-dock", JSON.stringify({ panels: { a: {} } }));
    const api = fakeApi();

    consoleDock.register(api as never);

    expect(api.fromJSON).toHaveBeenCalledWith({ panels: { a: {} } });
  });
});
