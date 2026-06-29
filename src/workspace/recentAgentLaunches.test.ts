import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  clearRecentAgentLaunchesForTests,
  forgetRecentAgentLaunch,
  markRecentAgentLaunch,
  readRecentAgentLaunches,
} from "./recentAgentLaunches";

describe("recentAgentLaunches", () => {
  beforeEach(() => {
    clearRecentAgentLaunchesForTests();
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
  });

  it("stores repo and agent launch combinations newest first", () => {
    markRecentAgentLaunch({ repo: "/r/a", agentType: "codex" });
    vi.setSystemTime(2_000);
    markRecentAgentLaunch({ repo: "/r/b", agentType: "claude" });

    expect(readRecentAgentLaunches()).toEqual([
      { repo: "/r/b", agentType: "claude", count: 1, lastUsedAt: 2_000 },
      { repo: "/r/a", agentType: "codex", count: 1, lastUsedAt: 1_000 },
    ]);
  });

  it("deduplicates repeated combinations and tracks usage count", () => {
    markRecentAgentLaunch({ repo: "/r/a", agentType: "codex" });
    vi.setSystemTime(2_000);
    markRecentAgentLaunch({ repo: "/r/a", agentType: "codex" });

    expect(readRecentAgentLaunches()).toEqual([
      { repo: "/r/a", agentType: "codex", count: 2, lastUsedAt: 2_000 },
    ]);
  });

  it("ignores incomplete launches", () => {
    markRecentAgentLaunch({ repo: "/r/a" });
    markRecentAgentLaunch({ agentType: "codex" });

    expect(readRecentAgentLaunches()).toEqual([]);
  });

  it("removes one stored launch combination", () => {
    markRecentAgentLaunch({ repo: "/r/a", agentType: "codex" });
    markRecentAgentLaunch({ repo: "/r/a", agentType: "claude" });

    forgetRecentAgentLaunch({ repo: "/r/a", agentType: "codex" });

    expect(readRecentAgentLaunches()).toEqual([
      { repo: "/r/a", agentType: "claude", count: 1, lastUsedAt: 1_000 },
    ]);
  });
});
