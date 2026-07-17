import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
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
  listenerFailures: {
    delta: null,
    fs: null,
    watching: null,
    sessions: null,
    changeLog: null,
    output: null,
    timeline: null,
  } as Record<string, unknown | null>,
}));

vi.mock("./client", () => ({
  onWorkbenchDelta: vi.fn((cb) => {
    if (h.listenerFailures.delta) return Promise.reject(h.listenerFailures.delta);
    h.deltaCb = cb;
    return Promise.resolve(h.unlistenDelta);
  }),
  onFsEvents: vi.fn((cb) => {
    if (h.listenerFailures.fs) return Promise.reject(h.listenerFailures.fs);
    h.fsCb = cb;
    return Promise.resolve(h.unlistenFs);
  }),
  onWatchingState: vi.fn((cb) => {
    if (h.listenerFailures.watching) return Promise.reject(h.listenerFailures.watching);
    h.watchCb = cb;
    return Promise.resolve(h.unlistenWatch);
  }),
  onAgentSessionsChanged: vi.fn((cb) => {
    if (h.listenerFailures.sessions) return Promise.reject(h.listenerFailures.sessions);
    h.sessionsCb = cb;
    return Promise.resolve(h.unlistenSessions);
  }),
  onAgentSessionChangeLog: vi.fn((cb) => {
    if (h.listenerFailures.changeLog) return Promise.reject(h.listenerFailures.changeLog);
    h.changeLogCb = cb;
    return Promise.resolve(h.unlistenChangeLog);
  }),
  onAgentSessionOutput: vi.fn((cb) => {
    if (h.listenerFailures.output) return Promise.reject(h.listenerFailures.output);
    h.outputCb = cb;
    return Promise.resolve(h.unlistenOutput);
  }),
  onAgentSessionTimeline: vi.fn((cb) => {
    if (h.listenerFailures.timeline) return Promise.reject(h.listenerFailures.timeline);
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

import { reloadActiveWorkbench, useBusConnection } from "./connection";
import { agentSessionStore } from "../agent/sessionStore";
import { busStore } from "./store";
import type { AgentSession, RepoDelta } from "./contract";

function makeDelta(repo: string, revision = 1): RepoDelta {
  return {
    repo,
    revision,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
  };
}

function makeSession(id: string): AgentSession {
  return {
    id,
    repo: "/r/a",
    agent_type: "codex",
    status: "running",
    pid: 123,
    started_at_ms: 1,
    exit_code: null,
    error: null,
    turn_status: "working",
    active_sessions: 1,
    age_ms: 10,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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
    h.deltaCb = null;
    h.fsCb = null;
    h.watchCb = null;
    h.sessionsCb = null;
    h.changeLogCb = null;
    h.outputCb = null;
    h.timelineCb = null;
    h.getSnapshot.mockResolvedValue({ watching: { available: true }, repos: [] });
    h.listWb.mockResolvedValue({ version: 1, active: "Work", workbenches: [] });
    h.listSessions.mockResolvedValue([]);
    Object.keys(h.listenerFailures).forEach((key) => {
      h.listenerFailures[key] = null;
    });
  });

  afterEach(() => {
    vi.useRealTimers();
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

  it("preserves agent sessions, identifies list failures, and clears the error after reload", async () => {
    agentSessionStore.setSessions([makeSession("existing")]);
    h.listSessions.mockRejectedValueOnce(new Error("agent backend offline"));

    await reloadActiveWorkbench();

    expect(agentSessionStore.getState().sessions.existing).toBeDefined();
    expect(busStore.getState().connectionErrors["agent-session-list"]).toContain(
      "Listado de sesiones Agent: agent backend offline",
    );

    h.listSessions.mockResolvedValueOnce([makeSession("recovered")]);
    await reloadActiveWorkbench();

    expect(agentSessionStore.getState().sessions.existing).toBeUndefined();
    expect(agentSessionStore.getState().sessions.recovered).toBeDefined();
    expect(busStore.getState().connectionErrors["agent-session-list"]).toBeUndefined();
  });

  it("retries a failed Agent session list without clearing the previous sessions", async () => {
    vi.useFakeTimers();
    agentSessionStore.setSessions([makeSession("existing")]);
    h.listSessions
      .mockRejectedValueOnce(new Error("agent backend offline"))
      .mockResolvedValueOnce([makeSession("recovered")]);

    render(createElement(Probe));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(agentSessionStore.getState().sessions.existing).toBeDefined();
    expect(busStore.getState().connectionErrors["agent-session-list"]).toContain(
      "agent backend offline",
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });

    expect(h.listSessions).toHaveBeenCalledTimes(2);
    expect(agentSessionStore.getState().sessions.existing).toBeUndefined();
    expect(agentSessionStore.getState().sessions.recovered).toBeDefined();
    expect(busStore.getState().connectionErrors["agent-session-list"]).toBeUndefined();
  });

  it("publishes listener failures and reconnects without clearing the current snapshot", async () => {
    vi.useFakeTimers();
    act(() => busStore.loadSnapshot([makeDelta("/r/existing", 3)], { available: true }));
    h.getSnapshot.mockResolvedValueOnce({
      watching: { available: true },
      repos: [makeDelta("/r/existing", 3)],
    });
    h.listenerFailures.delta = new Error("delta listener offline");

    const view = render(createElement(Probe));
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(busStore.getState().repos["/r/existing"]?.revision).toBe(3);
    expect(busStore.getState().connectionErrors["repo-deltas"]).toContain(
      "Canal de cambios de repositorios: delta listener offline",
    );

    h.listenerFailures.delta = null;
    await act(async () => {
      vi.advanceTimersByTime(1_000);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(h.deltaCb).toBeTypeOf("function");
    expect(busStore.getState().connectionErrors["repo-deltas"]).toBeUndefined();
    view.unmount();
  });

  it("marks the initial snapshot degraded when the Tauri bridge is unavailable", async () => {
    h.getSnapshot.mockRejectedValueOnce(
      new TypeError("Cannot read properties of undefined (reading 'transformCallback')"),
    );

    render(createElement(Probe));

    await waitFor(() => expect(busStore.getState().loaded).toBe(true));
    expect(busStore.getState().watching).toEqual({
      available: false,
      reason:
        "El puente de Tauri no está disponible; abre Tinto como aplicación de escritorio para cargar los datos de los repositorios.",
    });
    expect(busStore.getState()).toMatchObject({
      snapshotStatus: "error",
      snapshotError:
        "El puente de Tauri no está disponible; abre Tinto como aplicación de escritorio para cargar los datos de los repositorios.",
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

  it("publishes a retryable error when the config load fails", async () => {
    h.listWb.mockRejectedValueOnce(new Error("boom"));
    render(createElement(Probe));
    await waitFor(() => expect(busStore.getState().configStatus).toBe("error"));
    expect(busStore.getState()).toMatchObject({
      config: null,
      configStatus: "error",
      configError: "boom",
      snapshotStatus: "error",
    });
    expect(h.getSnapshot).not.toHaveBeenCalled();
  });

  it("publishes initial config while the first repo snapshot is still loading", async () => {
    const pendingSnapshot = deferred<{
      watching: { available: boolean };
      repos: RepoDelta[];
    }>();
    h.listWb.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [{ path: "/r/work", alias: null, fs_watch: [] }],
        },
      ],
    });
    h.getSnapshot.mockReturnValueOnce(pendingSnapshot.promise);

    const reload = reloadActiveWorkbench();
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalledOnce());

    expect(busStore.getState()).toMatchObject({
      config: { active: "Work" },
      configStatus: "ready",
      loaded: false,
      snapshotStatus: "loading",
    });

    pendingSnapshot.resolve({ watching: { available: true }, repos: [makeDelta("/r/work", 1)] });
    await reload;
    expect(busStore.getState()).toMatchObject({ loaded: true, snapshotStatus: "ready" });
  });

  it("does not combine an old config with a new snapshot when a later config reload fails", async () => {
    act(() => {
      busStore.setConfig({ version: 1, active: "Work", workbenches: [] });
      busStore.loadSnapshot([makeDelta("/r/work", 2)], { available: true });
    });
    h.listWb.mockRejectedValueOnce(new Error("config offline"));

    await reloadActiveWorkbench();

    expect(busStore.getState().config?.active).toBe("Work");
    expect(Object.keys(busStore.getState().repos)).toEqual(["/r/work"]);
    expect(busStore.getState()).toMatchObject({
      configStatus: "error",
      configError: "config offline",
      snapshotStatus: "error",
    });
    expect(h.getSnapshot).not.toHaveBeenCalled();
  });

  it("preserves a usable snapshot and publishes an error when a later snapshot reload fails", async () => {
    act(() => {
      busStore.setConfig({ version: 1, active: "Original", workbenches: [] });
      busStore.loadSnapshot([makeDelta("/r/existing", 3)], { available: true });
    });
    h.getSnapshot.mockRejectedValueOnce(new Error("snapshot offline"));

    await reloadActiveWorkbench();

    expect(busStore.getState().repos["/r/existing"]?.revision).toBe(3);
    expect(busStore.getState().config?.active).toBe("Original");
    expect(busStore.getState()).toMatchObject({
      loaded: true,
      snapshotStatus: "error",
      snapshotError: "snapshot offline",
    });
  });

  it("never publishes a staged config when a newer reload fails", async () => {
    const staleSnapshot = deferred<{
      watching: { available: boolean };
      repos: RepoDelta[];
    }>();
    act(() => {
      busStore.setConfig({ version: 1, active: "Original", workbenches: [] });
      busStore.loadSnapshot([makeDelta("/r/original", 3)], { available: true });
    });
    h.listWb
      .mockResolvedValueOnce({ version: 1, active: "Staged", workbenches: [] })
      .mockRejectedValueOnce(new Error("newer config failed"));
    h.getSnapshot.mockReturnValueOnce(staleSnapshot.promise);

    const firstReload = reloadActiveWorkbench();
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalledOnce());
    await reloadActiveWorkbench();

    expect(busStore.getState().config?.active).toBe("Original");
    expect(Object.keys(busStore.getState().repos)).toEqual(["/r/original"]);

    staleSnapshot.resolve({ watching: { available: true }, repos: [makeDelta("/r/staged", 4)] });
    await firstReload;
    expect(busStore.getState().config?.active).toBe("Original");
    expect(Object.keys(busStore.getState().repos)).toEqual(["/r/original"]);
  });

  it("ignores a stale workbench reload snapshot after a newer reload starts", async () => {
    const staleSnapshot = deferred<{
      watching: { available: boolean };
      repos: RepoDelta[];
    }>();
    h.listWb
      .mockResolvedValueOnce({ version: 1, active: "Work", workbenches: [] })
      .mockResolvedValueOnce({ version: 1, active: "Side", workbenches: [] });
    h.getSnapshot
      .mockReturnValueOnce(staleSnapshot.promise)
      .mockResolvedValueOnce({ watching: { available: true }, repos: [makeDelta("/r/side", 2)] });

    const staleReload = reloadActiveWorkbench();
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalledTimes(1));

    await reloadActiveWorkbench();
    expect(busStore.getState().config?.active).toBe("Side");
    expect(Object.keys(busStore.getState().repos)).toEqual(["/r/side"]);

    staleSnapshot.resolve({ watching: { available: true }, repos: [makeDelta("/r/work", 1)] });
    await staleReload;

    expect(busStore.getState().config?.active).toBe("Side");
    expect(Object.keys(busStore.getState().repos)).toEqual(["/r/side"]);
  });
});
