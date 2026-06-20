import { describe, expect, it, vi } from "vitest";
import { openAgentTerminalPanel } from "./openAgentTerminal";
import {
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "./panels";

function fakeApi() {
  const panels: Record<string, { api: { setActive: ReturnType<typeof vi.fn> } }> = {};
  return {
    addPanel: vi.fn((opts: { id: string }) => {
      panels[opts.id] = { api: { setActive: vi.fn() } };
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
});
