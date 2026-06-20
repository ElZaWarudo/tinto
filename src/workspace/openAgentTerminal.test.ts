import { describe, expect, it, vi } from "vitest";
import { openAgentTerminalPanel, terminalLayoutPosition } from "./openAgentTerminal";
import {
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "./panels";

function fakeApi() {
  const panels: Record<string, { id: string; api: { setActive: ReturnType<typeof vi.fn> } }> = {};
  const panelList: Array<{ id: string; api: { setActive: ReturnType<typeof vi.fn> } }> = [];
  return {
    get panels() {
      return panelList;
    },
    get activePanel() {
      return panelList[panelList.length - 1];
    },
    addPanel: vi.fn((opts: { id: string }) => {
      panels[opts.id] = { id: opts.id, api: { setActive: vi.fn() } };
      panelList.push(panels[opts.id]);
      return panels[opts.id];
    }),
    getPanel: vi.fn((id: string) => panels[id]),
    _panels: panels,
  };
}

describe("agent terminal panel helpers", () => {
  it("derives stable ids from session ids", () => {
    expect(agentTerminalPanelId("sess-1")).toBe("agent-terminal:sess-1");
    expect(sessionIdFromAgentTerminalPanelId("agent-terminal:sess-1")).toBe("sess-1");
    expect(sessionIdFromAgentTerminalPanelId("repo:/r/api")).toBeNull();
  });

  it("adds one terminal panel and focuses it on later opens", () => {
    const api = fakeApi();
    openAgentTerminalPanel(api as never, {
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });
    expect(api.addPanel).toHaveBeenCalledWith({
      id: "agent-terminal:sess-123456789",
      component: PANEL_AGENT_TERMINAL,
      title: "codex sess-123",
      params: {
        sessionId: "sess-123456789",
        repo: "/r/api",
        agentType: "codex",
      },
      position: undefined,
    });
    const created = api._panels["agent-terminal:sess-123456789"];

    openAgentTerminalPanel(api as never, {
      sessionId: "sess-123456789",
      repo: "/r/api",
      agentType: "codex",
    });

    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });

  it("places the first terminal right of the active panel, then below/right for more", () => {
    const api = fakeApi();
    api.addPanel({ id: "dashboard" });
    expect(terminalLayoutPosition(api as never)).toEqual({
      direction: "right",
      referencePanel: "dashboard",
    });

    api.addPanel({ id: agentTerminalPanelId("sess-1") });
    expect(terminalLayoutPosition(api as never)).toEqual({
      direction: "below",
      referencePanel: agentTerminalPanelId("sess-1"),
    });

    api.addPanel({ id: agentTerminalPanelId("sess-2") });
    expect(terminalLayoutPosition(api as never)).toEqual({
      direction: "right",
      referencePanel: agentTerminalPanelId("sess-2"),
    });
  });
});
