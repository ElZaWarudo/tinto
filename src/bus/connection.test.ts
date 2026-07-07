import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, act } from "@testing-library/react";
import { createElement } from "react";

const h = vi.hoisted(() => ({
  deltaCb: null as ((d: unknown) => void) | null,
  fsCb: null as ((b: unknown) => void) | null,
  watchCb: null as ((w: unknown) => void) | null,
  sessionsCb: null as ((sessions: unknown) => void) | null,
  changeLogCb: null as ((log: unknown) => void) | null,
  outputCb: null as ((output: unknown) => void) | null,
  timelineCb: null as ((item: unknown) => void) | null,
  unlistenDelta: vi.fn(),
  unlistenFs: vi.fn(),
  unlistenWatch: vi.fn(),
  unlistenSessions: vi.fn(),
  unlistenChangeLog: vi.fn(),
  unlistenOutput: vi.fn(),
  unlistenTimeline: vi.fn(),
  getSnapshot: vi.fn(),
  listWb: vi.fn(),
  listSessions: vi.fn(),
  ensureTreeLoaded: vi.fn(),
  refreshTree: vi.fn(),
}));

vi.mock("./client", () => ({
  onWorkbenchDelta: vi.fn((cb) => {
    h.deltaCb = cb;
    return Promise.resolve(h.unlistenDelta);
  }),
  onFsEvents: vi.fn((cb) => {
    h.fsCb = cb;
    return Promise.resolve(h.unlistenFs);
  }),
  onWatchingState: vi.fn((cb) => {
    h.watchCb = cb;
    return Promise.resolve(h.unlistenWatch);
  }),
  onAgentSessionsChanged: vi.fn((cb) => {
    h.sessionsCb = cb;
    return Promise.resolve(h.unlistenSessions);
  }),
  onAgentSessionChangeLog: vi.fn((cb) => {
    h.changeLogCb = cb;
    return Promise.resolve(h.unlistenChangeLog);
  }),
  onAgentSessionOutput: vi.fn((cb) => {
    h.outputCb = cb;
    return Promise.resolve(h.unlistenOutput);
  }),
  onAgentSessionTimeline: vi.fn((cb) => {
    h.timelineCb = cb;
    return Promise.resolve(h.unlistenTimeline);
  }),
  getWorkbenchSnapshot: () => h.getSnapshot(),
  listWorkbenches: () => h.listWb(),
  listAgentSessions: () => h.listSessions(),
}));

vi.mock("../workspace/repoTreeStore", () => ({
  repoTreeStore: {
    ensureLoaded: h.ensureTreeLoaded,
    refresh: h.refreshTree,
  },
}));

import { useBusConnection } from "./connection";
import { agentSessionStore } from "../agent/sessionStore";
import { busStore } from "./store";
import type { RepoDelta } from "./contract";

function makeDelta(repo: string, revision = 1): RepoDelta {
  return {
    repo,
    revision,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1,
    error: null,
  };
}

const Probe = () => {
  useBusConnection();
  return null;
};

describe("useBusConnection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    busStore.resetAll();
    agentSessionStore.reset();
    h.getSnapshot.mockResolvedValue({ watching: { available: true }, repos: [] });
    h.listWb.mockResolvedValue({ version: 1, active: "Work", workbenches: [] });
    h.listSessions.mockResolvedValue([]);
  });

  it("attaches listeners and loads config + snapshot", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalled());
    expect(h.deltaCb).toBeTypeOf("function");
    expect(h.fsCb).toBeTypeOf("function");
    expect(h.watchCb).toBeTypeOf("function");
    expect(h.sessionsCb).toBeTypeOf("function");
    expect(h.changeLogCb).toBeTypeOf("function");
    expect(h.outputCb).toBeTypeOf("function");
    expect(h.timelineCb).toBeTypeOf("function");
    expect(busStore.getState().config?.active).toBe("Work");
  });

  it("marks the initial snapshot degraded when the Tauri bridge is unavailable", async () => {
    h.getSnapshot.mockRejectedValueOnce(
      new TypeError("Cannot read properties of undefined (reading 'transformCallback')"),
    );

    render(createElement(Probe));

    await waitFor(() => expect(busStore.getState().loaded).toBe(true));
    expect(busStore.getState().watching).toEqual({
      available: false,
      reason: "Tauri bridge unavailable; run inside the Tauri shell for live repo data.",
    });
  });

  it("applies a delta received while mounted without preloading the file tree", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.deltaCb).toBeTypeOf("function"));
    act(() => h.deltaCb!(makeDelta("/r/a")));
    expect(busStore.getState().repos["/r/a"]).toBeDefined();
    expect(h.ensureTreeLoaded).not.toHaveBeenCalled();
    expect(h.refreshTree).not.toHaveBeenCalled();
  });

  it("does not reload the tree for stale deltas", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.deltaCb).toBeTypeOf("function"));
    act(() => busStore.loadSnapshot([makeDelta("/r/a", 3)], { available: true }));

    act(() => h.deltaCb!(makeDelta("/r/a", 2)));

    expect(h.ensureTreeLoaded).not.toHaveBeenCalled();
    expect(h.refreshTree).not.toHaveBeenCalled();
  });

  it("refreshes the tree for filesystem-only event batches", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.fsCb).toBeTypeOf("function"));
    act(() => busStore.loadSnapshot([makeDelta("/r/a")], { available: true }));
    act(() =>
      h.fsCb!({
        repo: "/r/a",
        events: [{ path: ".env", kind: "created", timestamp_ms: 2, size: 4, size_delta: null }],
      }),
    );
    expect(h.refreshTree).toHaveBeenCalledWith("/r/a");
  });

  it("unlistens on unmount and drops callbacks fired after (active guard)", async () => {
    const { unmount } = render(createElement(Probe));
    await waitFor(() => expect(h.deltaCb).toBeTypeOf("function"));
    unmount();
    await waitFor(() => expect(h.unlistenDelta).toHaveBeenCalled());
    expect(h.unlistenFs).toHaveBeenCalled();
    expect(h.unlistenWatch).toHaveBeenCalled();
    expect(h.unlistenSessions).toHaveBeenCalled();
    expect(h.unlistenChangeLog).toHaveBeenCalled();
    expect(h.unlistenOutput).toHaveBeenCalled();
    expect(h.unlistenTimeline).toHaveBeenCalled();

    // A late event after unmount must be ignored (the `active` guard).
    const before = Object.keys(busStore.getState().repos).length;
    act(() => h.deltaCb!(makeDelta("/r/late")));
    expect(Object.keys(busStore.getState().repos)).toHaveLength(before);
  });

  it("buffers agent output received before a terminal panel mounts", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.outputCb).toBeTypeOf("function"));

    act(() =>
      h.outputCb!({
        session_id: "sess-1",
        chunk_base64: "aGk=",
        timestamp_ms: 2,
      }),
    );

    expect(agentSessionStore.getState().output["sess-1"]).toHaveLength(1);
  });

  it("buffers native agent timeline received before a panel mounts", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.timelineCb).toBeTypeOf("function"));

    act(() =>
      h.timelineCb!({
        session_id: "sess-1",
        id: "sess-1:1",
        kind: "agent_message",
        text: "Listo",
        timestamp_ms: 2,
      }),
    );

    expect(agentSessionStore.getState().timeline["sess-1"]).toHaveLength(1);
  });

  it("applies pushed agent session snapshots", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.sessionsCb).toBeTypeOf("function"));

    act(() =>
      h.sessionsCb!([
        {
          id: "sess-1",
          repo: "/r/a",
          agent_type: "codex",
          status: "running",
          pid: 123,
          started_at_ms: 1,
          exit_code: null,
          error: null,
          checkpoint: null,
          change_log: [],
          turn_status: "working",
          turn_checkpoints: [],
          timeline: [
            {
              session_id: "sess-1",
              id: "sess-1:timeline:1",
              kind: "agent_message",
              text: "Snapshot restored",
              timestamp_ms: 11,
            },
          ],
          active_sessions: 1,
          age_ms: 10,
        },
      ]),
    );

    expect(agentSessionStore.getState().sessions["sess-1"]?.status).toBe("running");
    expect(agentSessionStore.getState().timeline["sess-1"]?.[0]?.text).toBe("Snapshot restored");
  });

  it("swallows a failed config load without throwing", async () => {
    h.listWb.mockRejectedValueOnce(new Error("boom"));
    render(createElement(Probe));
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalled());
    expect(busStore.getState().config).toBeNull(); // not set, no crash
  });
});
