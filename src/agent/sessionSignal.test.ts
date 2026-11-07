import { describe, expect, it } from "vitest";
import type { AgentSession, AgentSubagentThread } from "../bus/contract";
import { repoSignal, sessionSignal } from "./sessionSignal";

const agentSession = (over: Partial<AgentSession> = {}): AgentSession => ({
  id: "s-1",
  repo: "/repo",
  agent_type: "claude",
  status: "running",
  pid: 1,
  started_at_ms: 1,
  exit_code: null,
  error: null,
  turn_status: "waiting",
  turn_interrupt_supported: true,
  reports_turns: true,
  active_sessions: 1,
  age_ms: 1,
  ...over,
});

const permission = (state: "pending" | "allowed") => ({
  id: `p-${state}`,
  generation: 1,
  provider_session_id: "x",
  turn_id: "t",
  tool_call_id: "c",
  title: "Bash",
  options: [],
  state,
  expires_at_ms: 0,
});

const approvingChild = { id: "child", approval_request_id: "a-1" } as AgentSubagentThread;

describe("sessionSignal", () => {
  it.each([
    ["a pending permission", { acp_permissions: [permission("pending")] }, "needs_you"],
    ["a sub-agent approval", { subagents: [approvingChild] }, "needs_you"],
    ["a permission already answered", { acp_permissions: [permission("allowed")] }, "done"],
    ["a turn in progress", { turn_status: "working" as const }, "working"],
    ["a starting session", { status: "starting" as const }, "working"],
    ["a finished turn", {}, "done"],
    ["a plain terminal", { reports_turns: false }, null],
    ["an ended session", { status: "exited" as const }, null],
  ])("%s", (_, over, expected) => {
    expect(sessionSignal(agentSession(over))).toBe(expected);
  });

  it("gives a repo its most urgent signal across its sessions only", () => {
    const sessions = [
      agentSession({ id: "a", turn_status: "working" }),
      agentSession({ id: "b", acp_permissions: [permission("pending")] }),
      agentSession({ id: "c", turn_status: "working" }),
      agentSession({ id: "d", repo: "/other", acp_permissions: [permission("pending")] }),
    ];
    expect(repoSignal(sessions, "/repo")).toEqual({ signal: "needs_you", count: 1 });
    expect(repoSignal(sessions.slice(0, 1), "/repo")).toEqual({ signal: "working", count: 1 });
    expect(repoSignal(sessions, "/none")).toBeNull();
  });
});
