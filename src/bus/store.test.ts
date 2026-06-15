import { describe, it, expect, beforeEach } from "vitest";
import { BusStore, basename } from "./store";
import type { RepoDelta, RepoStatus, WorkbenchConfig } from "./contract";

const emptyStatus = (): RepoStatus => ({ modified: [], staged: [], untracked: [] });

function delta(repo: string, revision: number, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision,
    status: emptyStatus(),
    branch: null,
    head: null,
    last_activity_ms: revision * 1000,
    error: null,
    ...over,
  };
}

describe("BusStore", () => {
  let store: BusStore;
  beforeEach(() => {
    store = new BusStore();
  });

  it("applies a newer-revision delta and ignores stale/equal ones", () => {
    store.applyDelta(delta("/r/a", 5, { status: { modified: ["x"], staged: [], untracked: [] } }));
    expect(store.getState().repos["/r/a"].revision).toBe(5);

    store.applyDelta(delta("/r/a", 4)); // stale
    expect(store.getState().repos["/r/a"].revision).toBe(5);
    expect(store.getState().repos["/r/a"].status.modified).toEqual(["x"]);

    store.applyDelta(delta("/r/a", 5)); // equal — also ignored
    expect(store.getState().repos["/r/a"].status.modified).toEqual(["x"]);

    store.applyDelta(delta("/r/a", 6, { status: { modified: ["y"], staged: [], untracked: [] } }));
    expect(store.getState().repos["/r/a"].status.modified).toEqual(["y"]);
  });

  it("preserves untouched repo delta references for memoization", () => {
    store.applyDelta(delta("/r/a", 1));
    store.applyDelta(delta("/r/b", 1));
    const aBefore = store.getState().repos["/r/a"];
    store.applyDelta(delta("/r/b", 2));
    expect(store.getState().repos["/r/a"]).toBe(aBefore); // same ref
  });

  it("loadSnapshot seeds repos + watching and is authoritative", () => {
    store.applyDelta(delta("/r/old", 9));
    store.loadSnapshot([delta("/r/a", 2), delta("/r/b", 3)], {
      available: false,
      reason: "x",
    });
    const s = store.getState();
    expect(Object.keys(s.repos).sort()).toEqual(["/r/a", "/r/b"]);
    expect(s.repos["/r/old"]).toBeUndefined();
    expect(s.watching.available).toBe(false);
  });

  it("fs-events bump activity without changing the delta; ignored for unknown repos", () => {
    store.applyDelta(delta("/r/a", 1)); // last_activity_ms = 1000
    const before = store.getState().repos["/r/a"];
    store.applyFsEvents({
      repo: "/r/a",
      events: [{ path: ".env", kind: "modified", timestamp_ms: 5000, size: 10, size_delta: 1 }],
    });
    expect(store.getState().activity["/r/a"]).toBe(5000);
    expect(store.getState().repos["/r/a"]).toBe(before); // delta untouched

    store.applyFsEvents({
      repo: "/r/unknown",
      events: [{ path: "x", kind: "created", timestamp_ms: 9, size: null, size_delta: null }],
    });
    expect(store.getState().activity["/r/unknown"]).toBeUndefined();
  });

  it("exposes error state from a delta", () => {
    store.applyDelta(
      delta("/r/a", 1, {
        error: { class: "terminal", category: "repo-removed", message: "gone" },
      }),
    );
    expect(store.getState().repos["/r/a"].error?.class).toBe("terminal");
  });

  it("displayName uses the alias, then the basename", () => {
    const config: WorkbenchConfig = {
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            { path: "/home/me/code/api", alias: "API", fs_watch: [] },
            { path: "/home/me/code/web", alias: null, fs_watch: [] },
          ],
        },
      ],
    };
    store.setConfig(config);
    expect(store.displayName("/home/me/code/api")).toBe("API");
    expect(store.displayName("/home/me/code/web")).toBe("web");
    expect(store.displayName("/unknown/path/repo")).toBe("repo");
  });

  it("reset clears live repos but notifies subscribers", () => {
    let notified = 0;
    store.subscribe(() => notified++);
    store.applyDelta(delta("/r/a", 1));
    store.reset();
    expect(Object.keys(store.getState().repos)).toHaveLength(0);
    expect(notified).toBeGreaterThan(0);
  });

  it("basename handles posix and windows separators", () => {
    expect(basename("/a/b/c")).toBe("c");
    expect(basename("C:\\x\\y")).toBe("y");
    expect(basename("solo")).toBe("solo");
  });
});
