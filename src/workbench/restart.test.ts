import { describe, it, expect, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { openRestoredConversations } from "./restart";

describe("openRestoredConversations", () => {
  it("opens a tab for each conversation the restart resumed but its coordinators", async () => {
    invoke.mockResolvedValue([
      { session_id: "new-1", repo: "/repos/tinto", agent_type: "claude", coordinator: false },
      { session_id: "new-3", repo: "/repos/tinto", agent_type: "codex", coordinator: true },
      { session_id: "new-2", repo: "/home/u/api", agent_type: "codex", coordinator: false },
    ]);
    const open = vi.fn();
    await openRestoredConversations(open);
    expect(invoke).toHaveBeenCalledWith("restore_live_agent_sessions");
    expect(open.mock.calls).toEqual([
      [{ sessionId: "new-1", repo: "/repos/tinto", agentType: "claude" }],
      [{ sessionId: "new-2", repo: "/home/u/api", agentType: "codex" }],
    ]);
  });
});
