import { beforeEach, describe, expect, it, vi } from "vitest";

const detachedWindowMocks = vi.hoisted(() => ({
  sendTerminalToDetachedConsoles: vi.fn(() => Promise.resolve(false)),
}));

vi.mock("../panels/terminal/detachTerminalWindow", () => ({
  sendTerminalToDetachedConsoles: detachedWindowMocks.sendTerminalToDetachedConsoles,
}));

import { consoleDock } from "./consoleDock";
import { openAgentConsolesPanel, openAgentTerminalPanel } from "./openAgentTerminal";
import {
  PANEL_AGENT_CONSOLES,
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "./panels";

function fakeApi() {
  const panels: Record<
    string,
    { id: string; api: { setActive: ReturnType<typeof vi.fn> }; params?: unknown }
  > = {};
  const panelList: Array<{
    id: string;
    api: { setActive: ReturnType<typeof vi.fn> };
    params?: unknown;
  }> = [];
  return {
    get panels() {
      return panelList;
    },
    get activePanel() {
      return panelList[panelList.length - 1];
    },
    addPanel: vi.fn(
      (opts: { id: string; component?: string; title?: string; params?: unknown }) => {
        panels[opts.id] = { id: opts.id, api: { setActive: vi.fn() }, params: opts.params };
        panelList.push(panels[opts.id]);
        return panels[opts.id];
      },
    ),
    getPanel: vi.fn((id: string) => panels[id]),
    _panels: panels,
  };
}

describe("agent terminal panel helpers", () => {
  beforeEach(() => {
    detachedWindowMocks.sendTerminalToDetachedConsoles.mockReset();
    detachedWindowMocks.sendTerminalToDetachedConsoles.mockResolvedValue(false);
  });

  it("derives stable ids from session ids", () => {
    expect(agentTerminalPanelId("sess-1")).toBe("agent-terminal:sess-1");
    expect(sessionIdFromAgentTerminalPanelId("agent-terminal:sess-1")).toBe("sess-1");
    expect(sessionIdFromAgentTerminalPanelId("repo:/r/api")).toBeNull();
  });

  it("opens the level-1 Agents panel and delegates the session to the console dock", async () => {
    const api = fakeApi();
    const openSpy = vi.spyOn(consoleDock, "openTerminal").mockImplementation(() => {});

    openAgentTerminalPanel(api as never, {
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });

    await vi.waitFor(() => {
      expect(api.addPanel).toHaveBeenCalledWith({
        id: PANEL_AGENT_CONSOLES,
        component: PANEL_AGENT_CONSOLES,
        title: "Agents",
      });
    });
    expect(api._panels[PANEL_AGENT_CONSOLES].api.setActive).toHaveBeenCalledOnce();
    expect(openSpy).toHaveBeenCalledWith({
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });

    openSpy.mockRestore();
  });

  it("routes new terminal sessions to a detached consoles window when present", async () => {
    detachedWindowMocks.sendTerminalToDetachedConsoles.mockResolvedValue(true);
    const api = fakeApi();
    const openSpy = vi.spyOn(consoleDock, "openTerminal").mockImplementation(() => {});
    const params = {
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    };

    openAgentTerminalPanel(api as never, params);

    await vi.waitFor(() =>
      expect(detachedWindowMocks.sendTerminalToDetachedConsoles).toHaveBeenCalledWith(params),
    );
    expect(api.addPanel).not.toHaveBeenCalled();
    expect(openSpy).not.toHaveBeenCalled();

    openSpy.mockRestore();
  });

  it("focuses the existing Agents panel on later opens", () => {
    const api = fakeApi();
    openAgentConsolesPanel(api as never);
    const created = api._panels[PANEL_AGENT_CONSOLES];
    created.api.setActive.mockClear();

    openAgentConsolesPanel(api as never);

    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });

  it("maximizes a split Agents group in a compact workspace", () => {
    const agentsGroup = {
      id: "agents-group",
      api: { location: { type: "grid" } },
    };
    const repoGroup = { id: "repo-group", api: { location: { type: "grid" } } };
    let maximized = false;
    const existing = {
      id: PANEL_AGENT_CONSOLES,
      group: agentsGroup,
      api: {
        location: { type: "grid" },
        maximize: vi.fn(() => {
          maximized = true;
        }),
        isMaximized: vi.fn(() => maximized),
        setActive: vi.fn(),
      },
    };
    const repo = {
      id: "repo:/r/api",
      group: repoGroup,
      api: { location: { type: "grid" } },
    };
    const api = {
      width: 802,
      activeGroup: agentsGroup,
      panels: [existing, repo],
      getPanel: vi.fn(() => existing),
    };

    openAgentConsolesPanel(api as never);

    expect(existing.api.maximize).toHaveBeenCalledOnce();
    expect(existing.api.isMaximized()).toBe(true);
    expect(existing.api.setActive).toHaveBeenCalledOnce();
  });

  it("preserves an intentionally split Agents panel in a wide workspace", () => {
    const existing = {
      id: PANEL_AGENT_CONSOLES,
      group: { id: "agents-group" },
      api: {
        location: { type: "grid" },
        maximize: vi.fn(),
        isMaximized: vi.fn(() => false),
        setActive: vi.fn(),
      },
    };
    const api = {
      width: 1440,
      getPanel: vi.fn(() => existing),
    };

    openAgentConsolesPanel(api as never);

    expect(existing.api.maximize).not.toHaveBeenCalled();
    expect(existing.api.setActive).toHaveBeenCalledOnce();
  });

  it("does not maximize a compact workspace that already has a single grid group", () => {
    const agentsGroup = {
      id: "agents-group",
      api: { location: { type: "grid" } },
    };
    const existing = {
      id: PANEL_AGENT_CONSOLES,
      group: agentsGroup,
      api: {
        location: { type: "grid" },
        maximize: vi.fn(),
        isMaximized: vi.fn(() => false),
        setActive: vi.fn(),
      },
    };
    const api = {
      width: 802,
      activeGroup: agentsGroup,
      panels: [existing],
      getPanel: vi.fn(() => existing),
    };

    openAgentConsolesPanel(api as never);

    expect(existing.api.maximize).not.toHaveBeenCalled();
    expect(existing.api.setActive).toHaveBeenCalledOnce();
  });

  it("adds Agents as a tab in the current workspace group", () => {
    const api = fakeApi();
    api.addPanel({ id: "repo:/r/api" });
    const referencePanel = api._panels["repo:/r/api"];

    openAgentConsolesPanel(api as never);

    expect(api.addPanel).toHaveBeenLastCalledWith({
      id: PANEL_AGENT_CONSOLES,
      component: PANEL_AGENT_CONSOLES,
      title: "Agents",
      position: { referencePanel, direction: "within" },
    });
  });

  it("keeps terminal panel ids available for the nested console dock", () => {
    const api = fakeApi();
    api.addPanel({
      id: agentTerminalPanelId("sess-123456789"),
      component: PANEL_AGENT_TERMINAL,
      title: "codex sess-123",
      params: {
        sessionId: "sess-123456789",
        repo: "/r/api",
        agentType: "codex",
      },
    });

    expect(api._panels[agentTerminalPanelId("sess-123456789")].params).toEqual({
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });
  });
});
