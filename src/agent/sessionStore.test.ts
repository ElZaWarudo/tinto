import { act } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { AgentSession } from "../bus/contract";
import { AgentSessionStore } from "./sessionStore";

const fixture = (subagents?: AgentSession["subagents"]): AgentSession => ({
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
  subagents,
});

describe("AgentSessionStore", () => {
  it("normalizes legacy roots and bounds restored child payloads", () => {
    const store = new AgentSessionStore();
    const child = {
      id: "child",
      source_kind: "subAgent",
      depth: 1,
      thread_status: "completed",
      turn_status: "waiting",
      capabilities: {
        inspect: true,
        direct_input: true,
        steer: false,
        interrupt: false,
        wait: false,
        close: false,
      },
      updated_at_ms: 1,
      timeline: Array.from({ length: 2005 }, (_, index) => ({
        session_id: "child",
        id: `child:${index}`,
        kind: "agent_message" as const,
        text: String(index),
        timestamp_ms: index,
      })),
    };
    act(() => store.setSessions([fixture([child])]));
    const saved = store.getState().sessions.root;
    expect(saved?.subagents?.[0].parent_id).toBeNull();
    expect(saved?.subagents?.[0].timeline).toHaveLength(2000);
    expect(saved?.subagents?.[0].timeline?.[0].text).toBe("5");
  });
});
