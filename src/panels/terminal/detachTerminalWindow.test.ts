import { describe, expect, it } from "vitest";
import {
  detachedConsolesUrl,
  readDetachedConsolesFlag,
  readDetachedConsolesParams,
} from "./detachTerminalWindow";

describe("detached console windows", () => {
  it("round-trips detached console sessions through the window url", () => {
    const url = detachedConsolesUrl([
      { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
      { sessionId: "sess-2", repo: "/r/b", agentType: "claude" },
    ]);
    const search = url.slice(url.indexOf("?"));

    expect(readDetachedConsolesFlag(search)).toBe(true);
    expect(readDetachedConsolesParams(search)).toEqual([
      { sessionId: "sess-1", repo: "/r/a", agentType: "codex" },
      { sessionId: "sess-2", repo: "/r/b", agentType: "claude" },
    ]);
  });

  it("ignores malformed detached console session data", () => {
    const search = "?tintoDetachedConsoles=1&sessions=not-json";

    expect(readDetachedConsolesParams(search)).toEqual([]);
  });
});
