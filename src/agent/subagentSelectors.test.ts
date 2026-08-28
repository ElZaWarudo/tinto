import { describe, expect, it } from "vitest";
import type { AgentSession } from "../bus/contract";
import { agentCapabilityReason, selectAgentTimeline, selectAgentTree } from "./subagentSelectors";

function session(): AgentSession {
  return {
    id: "root",
    repo: "/repo",
    agent_type: "codex",
    permission_mode: "workspace",
    permission_mode_change_supported: false,
    status: "running",
    pid: 1,
    started_at_ms: 1,
    ended_at_ms: null,
    exit_code: null,
    error: null,
    turn_status: "waiting",
    turn_interrupt_supported: true,
    active_sessions: 1,
    age_ms: 1,
    subagents: [
      {
        id: "child",
        parent_id: "root",
        source_kind: "subAgent",
        depth: 1,
        nickname: "Scout",
        role: "researcher",
        thread_status: "running",
        turn_status: "working",
        capabilities: {
          inspect: true,
          direct_input: true,
          steer: true,
          interrupt: true,
          wait: true,
          close: true,
        },
        timeline: [
          {
            session_id: "child",
            id: "child:1",
            kind: "agent_message",
            text: "child transcript",
            timestamp_ms: 2,
          },
        ],
        updated_at_ms: 2,
      },
      {
        id: "grandchild",
        parent_id: "child",
        source_kind: "subAgent",
        depth: 2,
        role: "checker",
        thread_status: "completed",
        turn_status: "waiting",
        capabilities: {
          inspect: true,
          direct_input: false,
          steer: false,
          interrupt: false,
          wait: false,
          close: false,
        },
        timeline: [],
        updated_at_ms: 3,
      },
    ],
  };
}

describe("subagent selectors", () => {
  it("keeps root, child, and nested descendants in deterministic tree order", () => {
    expect(selectAgentTree(session()).map((node) => [node.id, node.parentId, node.depth])).toEqual([
      ["root", null, 1],
      ["child", "root", 2],
      ["grandchild", "child", 3],
    ]);
  });

  it("selects only the requested child transcript", () => {
    expect(selectAgentTimeline(session(), "child").map((item) => item.text)).toEqual([
      "child transcript",
    ]);
    expect(selectAgentTimeline(session(), "root")).toEqual([]);
  });

  it("keeps generic provider progress filler out of the selected transcript", () => {
    const withFiller = session();
    withFiller.subagents?.[0].timeline?.unshift({
      session_id: "child",
      id: "child:filler",
      kind: "agent_progress",
      text: "Analizando el siguiente paso...",
      timestamp_ms: 1,
    });
    expect(selectAgentTimeline(withFiller, "child").map((item) => item.text)).toEqual([
      "child transcript",
    ]);
  });

  it("explains unavailable controls without guessing provider state", () => {
    const node = selectAgentTree(session())[2];
    expect(agentCapabilityReason(node, "direct_input", false)).toContain("did not report");
    expect(agentCapabilityReason(selectAgentTree(session())[1], "direct_input", true)).toContain(
      "read-only",
    );
  });

  it("keeps duplicate provider ids inspectable with unique projection ids", () => {
    const duplicate = session();
    duplicate.subagents?.push({ ...duplicate.subagents[0], nickname: "Second scout" });
    const nodes = selectAgentTree(duplicate).filter((node) => node.providerId === "child");
    expect(nodes).toHaveLength(2);
    expect(new Set(nodes.map((node) => node.id)).size).toBe(2);
  });
});
