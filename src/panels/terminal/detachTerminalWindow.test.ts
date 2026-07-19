import { beforeEach, describe, expect, it, vi } from "vitest";

const eventMocks = vi.hoisted(() => ({
  callbacks: new Map<string, (event: { payload: unknown }) => void>(),
  emit: vi.fn<(event: string, payload: unknown) => Promise<void>>(() => Promise.resolve()),
  unlisten: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: eventMocks.emit,
  listen: vi.fn((event: string, callback: (event: { payload: unknown }) => void) => {
    eventMocks.callbacks.set(event, callback);
    return Promise.resolve(eventMocks.unlisten);
  }),
}));

import {
  detachedConsolesUrl,
  detachedTerminalWindowTitle,
  onDetachedConsolesReattach,
  readDetachedConsolesFlag,
  readDetachedConsolesParams,
  reattachDetachedConsoles,
} from "./detachTerminalWindow";

describe("detached console windows", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventMocks.callbacks.clear();
    eventMocks.emit.mockResolvedValue();
  });

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

  it("uses provider labels in detached terminal window titles", () => {
    expect(detachedTerminalWindowTitle({ sessionId: "sess-kimi-123", agentType: "kimi" })).toBe(
      "Kimi Code sess-kim",
    );
    expect(detachedTerminalWindowTitle({ sessionId: "sess-open-123", agentType: "opencode" })).toBe(
      "OpenCode sess-ope",
    );
  });

  it("ignores malformed detached console session data", () => {
    const search = "?tintoDetachedConsoles=1&sessions=not-json";

    expect(readDetachedConsolesParams(search)).toEqual([]);
  });

  it("waits for the main window acknowledgement before completing reattach", async () => {
    eventMocks.emit.mockImplementation(async (event, payload) => {
      if (event !== "tinto://detached-consoles-reattach") return;
      const requestId = (payload as { requestId: string }).requestId;
      eventMocks.callbacks.get("tinto://detached-consoles-reattach-ack")?.({
        payload: { requestId },
      });
    });

    await expect(reattachDetachedConsoles([{ sessionId: "sess-1" }])).resolves.toBe(true);
    expect(eventMocks.unlisten).toHaveBeenCalledOnce();
    expect(eventMocks.emit).toHaveBeenCalledWith(
      "tinto://detached-consoles-reattach",
      expect.objectContaining({ terminals: [{ sessionId: "sess-1" }] }),
    );
  });

  it("keeps reattach recoverable when no main window acknowledges it", async () => {
    vi.useFakeTimers();
    try {
      const result = reattachDetachedConsoles([{ sessionId: "sess-1" }]);
      await vi.advanceTimersByTimeAsync(3_000);

      await expect(result).resolves.toBe(false);
      expect(eventMocks.unlisten).toHaveBeenCalledOnce();
    } finally {
      vi.useRealTimers();
    }
  });

  it("acknowledges only after the main window opens the requested terminals", async () => {
    const opened: string[] = [];
    await onDetachedConsolesReattach(async (terminals) => {
      opened.push(...terminals.map((terminal) => terminal.sessionId));
    });

    eventMocks.callbacks.get("tinto://detached-consoles-reattach")?.({
      payload: { requestId: "request-1", terminals: [{ sessionId: "sess-1" }] },
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(opened).toEqual(["sess-1"]);
    expect(eventMocks.emit).toHaveBeenCalledWith("tinto://detached-consoles-reattach-ack", {
      requestId: "request-1",
    });
  });
});
