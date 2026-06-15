import { describe, it, expect, beforeEach } from "vitest";
import { BusStore, basename, getDiff, hasComputedDiffs } from "./store";
import type { FileDiff, RepoDelta, RepoStatus, WorkbenchConfig } from "./contract";

const fileDiff = (path: string): FileDiff => ({
  path,
  old_path: null,
  is_binary: false,
  hunks: [{ old_start: 1, new_start: 1, lines: [] }],
});

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

  it("loadSnapshot keeps a newer in-flight delta instead of clobbering it (revision rule)", () => {
    // Delta N+1 arrived while the snapshot (N) was in flight.
    store.applyDelta(
      delta("/r/a", 6, { status: { modified: ["fresh"], staged: [], untracked: [] } }),
    );
    store.loadSnapshot([delta("/r/a", 5)], { available: true });
    const s = store.getState();
    expect(s.repos["/r/a"].revision).toBe(6); // newer delta preserved
    expect(s.repos["/r/a"].status.modified).toEqual(["fresh"]);
    // An older-or-equal snapshot for a repo not yet known is taken as-is.
    store.loadSnapshot([delta("/r/a", 5), delta("/r/b", 2)], { available: true });
    expect(store.getState().repos["/r/a"].revision).toBe(6); // still kept
    expect(store.getState().repos["/r/b"].revision).toBe(2);
  });

  it("loadSnapshot preserves fs-event activity newer than the snapshot's", () => {
    store.applyDelta(delta("/r/a", 1)); // last_activity_ms = 1000
    store.applyFsEvents({
      repo: "/r/a",
      events: [{ path: "x", kind: "modified", timestamp_ms: 9000, size: 1, size_delta: 0 }],
    });
    store.loadSnapshot([delta("/r/a", 2)], { available: true }); // snapshot activity = 2000
    expect(store.getState().activity["/r/a"]).toBe(9000); // fs activity preserved
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

describe("BusStore diff slice (RDM-008)", () => {
  let store: BusStore;
  beforeEach(() => {
    store = new BusStore();
  });

  it("fills the slice from a non-null subscribed_diffs (AE5 fill)", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("src/a.ts")] }));
    expect(getDiff(store.getState(), "/r/a", "src/a.ts")?.path).toBe("src/a.ts");
    expect(hasComputedDiffs(store.getState(), "/r/a")).toBe(true);
  });

  it("retains the open diff across a status-only delta with null diffs (AE5 no-blank)", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("src/a.ts")] }));
    // A later status-only / transient delta carries subscribed_diffs == null.
    store.applyDelta(
      delta("/r/a", 2, {
        status: { modified: ["src/a.ts"], staged: [], untracked: [] },
        subscribed_diffs: null,
      }),
    );
    expect(getDiff(store.getState(), "/r/a", "src/a.ts")?.path).toBe("src/a.ts"); // NOT blanked
  });

  it("clears a reverted file by omission from a non-null array (clean-clear)", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("a.ts"), fileDiff("b.ts")] }));
    expect(getDiff(store.getState(), "/r/a", "b.ts")).toBeDefined();
    // b.ts reverted → omitted from a fresh non-null array; a.ts still changed.
    store.applyDelta(delta("/r/a", 2, { subscribed_diffs: [fileDiff("a.ts")] }));
    expect(getDiff(store.getState(), "/r/a", "a.ts")).toBeDefined();
    expect(getDiff(store.getState(), "/r/a", "b.ts")).toBeUndefined(); // gone
  });

  it("an empty array marks computed with no diffs (loading→clean signal)", () => {
    expect(hasComputedDiffs(store.getState(), "/r/a")).toBe(false); // loading
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [] }));
    expect(hasComputedDiffs(store.getState(), "/r/a")).toBe(true); // computed, clean
    expect(getDiff(store.getState(), "/r/a", "x")).toBeUndefined();
  });

  it("a stale delta does not apply its diffs", () => {
    store.applyDelta(delta("/r/a", 5, { subscribed_diffs: [fileDiff("a.ts")] }));
    store.applyDelta(delta("/r/a", 4, { subscribed_diffs: [fileDiff("b.ts")] })); // stale
    expect(getDiff(store.getState(), "/r/a", "a.ts")).toBeDefined();
    expect(getDiff(store.getState(), "/r/a", "b.ts")).toBeUndefined();
  });

  it("dropDiff removes a single target", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("a.ts"), fileDiff("b.ts")] }));
    store.dropDiff("/r/a", "a.ts");
    expect(getDiff(store.getState(), "/r/a", "a.ts")).toBeUndefined();
    expect(getDiff(store.getState(), "/r/a", "b.ts")).toBeDefined();
  });

  it("loadSnapshot drops diffs for repos that leave membership; retains in-flight", () => {
    store.applyDelta(delta("/r/gone", 1, { subscribed_diffs: [fileDiff("x")] }));
    store.applyDelta(delta("/r/keep", 1, { subscribed_diffs: [fileDiff("y")] }));
    // Snapshot omits /r/gone; /r/keep present without diffs → in-flight retained.
    store.loadSnapshot([delta("/r/keep", 2)], { available: true });
    const s = store.getState();
    expect(s.diffs["/r/gone"]).toBeUndefined();
    expect(getDiff(s, "/r/keep", "y")).toBeDefined();
  });

  it("reset clears the diff slice", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("a.ts")] }));
    store.reset();
    expect(store.getState().diffs).toEqual({});
  });
});
