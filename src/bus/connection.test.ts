import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, act } from "@testing-library/react";
import { createElement } from "react";

const h = vi.hoisted(() => ({
  deltaCb: null as ((d: unknown) => void) | null,
  fsCb: null as ((b: unknown) => void) | null,
  watchCb: null as ((w: unknown) => void) | null,
  unlistenDelta: vi.fn(),
  unlistenFs: vi.fn(),
  unlistenWatch: vi.fn(),
  getSnapshot: vi.fn(),
  listWb: vi.fn(),
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
  getWorkbenchSnapshot: () => h.getSnapshot(),
  listWorkbenches: () => h.listWb(),
}));

import { useBusConnection } from "./connection";
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
    h.getSnapshot.mockResolvedValue({ watching: { available: true }, repos: [] });
    h.listWb.mockResolvedValue({ version: 1, active: "Work", workbenches: [] });
  });

  it("attaches all three listeners and loads config + snapshot", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalled());
    expect(h.deltaCb).toBeTypeOf("function");
    expect(h.fsCb).toBeTypeOf("function");
    expect(h.watchCb).toBeTypeOf("function");
    expect(busStore.getState().config?.active).toBe("Work");
  });

  it("applies a delta received while mounted", async () => {
    render(createElement(Probe));
    await waitFor(() => expect(h.deltaCb).toBeTypeOf("function"));
    act(() => h.deltaCb!(makeDelta("/r/a")));
    expect(busStore.getState().repos["/r/a"]).toBeDefined();
  });

  it("unlistens on unmount and drops callbacks fired after (active guard)", async () => {
    const { unmount } = render(createElement(Probe));
    await waitFor(() => expect(h.deltaCb).toBeTypeOf("function"));
    unmount();
    await waitFor(() => expect(h.unlistenDelta).toHaveBeenCalled());
    expect(h.unlistenFs).toHaveBeenCalled();
    expect(h.unlistenWatch).toHaveBeenCalled();

    // A late event after unmount must be ignored (the `active` guard).
    const before = Object.keys(busStore.getState().repos).length;
    act(() => h.deltaCb!(makeDelta("/r/late")));
    expect(Object.keys(busStore.getState().repos)).toHaveLength(before);
  });

  it("swallows a failed config load without throwing", async () => {
    h.listWb.mockRejectedValueOnce(new Error("boom"));
    render(createElement(Probe));
    await waitFor(() => expect(h.getSnapshot).toHaveBeenCalled());
    expect(busStore.getState().config).toBeNull(); // not set, no crash
  });
});
