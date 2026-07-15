import { describe, it, expect, beforeEach } from "vitest";
import {
  BusStore,
  MAX_FS_EVENTS_PER_REPO,
  basename,
  getDiff,
  getFsEvents,
  getPathSignals,
  getPathSecretFindings,
  getRepoMetrics,
  getRepoSignals,
  hasComputedDiffs,
  signalCounts,
  sortSignals,
} from "./store";
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
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
    ...over,
  };
}

describe("BusStore", () => {
  let store: BusStore;
  beforeEach(() => {
    store = new BusStore();
  });

  it("applies a newer-revision delta and ignores stale/equal ones", () => {
    expect(
      store.applyDelta(
        delta("/r/a", 5, { status: { modified: ["x"], staged: [], untracked: [] } }),
      ),
    ).toBe(true);
    expect(store.getState().repos["/r/a"].revision).toBe(5);

    expect(store.applyDelta(delta("/r/a", 4))).toBe(false); // stale
    expect(store.getState().repos["/r/a"].revision).toBe(5);
    expect(store.getState().repos["/r/a"].status.modified).toEqual(["x"]);

    expect(store.applyDelta(delta("/r/a", 5))).toBe(false); // equal — also ignored
    expect(store.getState().repos["/r/a"].status.modified).toEqual(["x"]);

    expect(
      store.applyDelta(
        delta("/r/a", 6, { status: { modified: ["y"], staged: [], untracked: [] } }),
      ),
    ).toBe(true);
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
    expect(getFsEvents(store.getState(), "/r/unknown")).toEqual([]);
  });

  it("stores recent fs-events newest-first and caps them per repo", () => {
    store.applyDelta(delta("/r/a", 1));
    store.applyFsEvents({
      repo: "/r/a",
      events: [
        { path: "old.env", kind: "modified", timestamp_ms: 2000, size: 1, size_delta: 0 },
        { path: "new.env", kind: "created", timestamp_ms: 3000, size: 2, size_delta: 2 },
      ],
    });
    expect(getFsEvents(store.getState(), "/r/a").map((e) => e.path)).toEqual([
      "new.env",
      "old.env",
    ]);

    for (let i = 0; i < MAX_FS_EVENTS_PER_REPO + 5; i++) {
      store.applyFsEvents({
        repo: "/r/a",
        events: [
          { path: `f${i}.env`, kind: "modified", timestamp_ms: 4000 + i, size: i, size_delta: 1 },
        ],
      });
    }
    const events = getFsEvents(store.getState(), "/r/a");
    expect(events).toHaveLength(MAX_FS_EVENTS_PER_REPO);
    expect(events[0].path).toBe(`f${MAX_FS_EVENTS_PER_REPO + 4}.env`);
    expect(events[events.length - 1].path).toBe("f5.env");
  });

  it("loadSnapshot and reset drop fs-events for repos that leave membership", () => {
    store.applyDelta(delta("/r/gone", 1));
    store.applyDelta(delta("/r/keep", 1));
    store.applyFsEvents({
      repo: "/r/gone",
      events: [{ path: ".env", kind: "modified", timestamp_ms: 5000, size: 10, size_delta: 1 }],
    });
    store.applyFsEvents({
      repo: "/r/keep",
      events: [{ path: "local.json", kind: "created", timestamp_ms: 6000, size: 4, size_delta: 4 }],
    });

    store.loadSnapshot([delta("/r/keep", 2)], { available: true });
    expect(getFsEvents(store.getState(), "/r/gone")).toEqual([]);
    expect(getFsEvents(store.getState(), "/r/keep").map((e) => e.path)).toEqual(["local.json"]);

    store.reset();
    expect(store.getState().fsEventsByRepo).toEqual({});
  });

  it("exposes error state from a delta", () => {
    store.applyDelta(
      delta("/r/a", 1, {
        error: { class: "terminal", category: "repo-removed", message: "gone" },
      }),
    );
    expect(store.getState().repos["/r/a"].error?.class).toBe("terminal");
  });

  it("tracks configuration loading, success, and recoverable failure explicitly", () => {
    expect(store.getState()).toMatchObject({
      configStatus: "loading",
      configError: null,
      snapshotStatus: "loading",
      snapshotError: null,
    });

    store.setConfigError("backend offline");
    expect(store.getState()).toMatchObject({
      configStatus: "error",
      configError: "backend offline",
      config: null,
    });

    store.beginConfigLoad();
    expect(store.getState()).toMatchObject({
      configStatus: "loading",
      configError: null,
    });

    store.setConfig({ version: 1, active: null, workbenches: [] });
    expect(store.getState()).toMatchObject({
      configStatus: "ready",
      configError: null,
    });

    store.loadSnapshot([], { available: true });
    expect(store.getState()).toMatchObject({
      snapshotStatus: "ready",
      snapshotError: null,
      loaded: true,
    });

    store.beginSnapshotLoad();
    store.setSnapshotError("snapshot offline");
    expect(store.getState()).toMatchObject({
      snapshotStatus: "error",
      snapshotError: "snapshot offline",
      loaded: true,
    });
  });

  it("exposes passive metrics and signals with additive fallbacks", () => {
    expect(getRepoMetrics(undefined)).toEqual({
      changed_files: 0,
      lines_added: 0,
      lines_removed: 0,
    });
    store.applyDelta(
      delta("/r/a", 1, {
        metrics: { changed_files: 2, lines_added: 9, lines_removed: 4 },
        signals: [
          {
            kind: "config_change",
            severity: "warning",
            path: "package.json",
            message: "Configuration file changed",
          },
          {
            kind: "possible_secret",
            severity: "critical",
            path: "src/config.ts",
            message: "Possible secret detected",
          },
        ],
        secret_findings: [
          {
            path: "src/config.ts",
            line: 18,
            rule_id: "generic-api-key",
            description: "Possible secret",
          },
        ],
      }),
    );
    const repo = store.getState().repos["/r/a"];
    expect(getRepoMetrics(repo).lines_added).toBe(9);
    expect(getRepoSignals(repo)).toHaveLength(2);
    expect(getPathSignals(repo, "src/config.ts").map((s) => s.kind)).toEqual(["possible_secret"]);
    expect(getPathSecretFindings(repo, "src/config.ts").map((f) => f.line)).toEqual([18]);
    expect(signalCounts(getRepoSignals(repo))).toEqual({ critical: 1, warning: 1, info: 0 });
    expect(sortSignals(getRepoSignals(repo)).map((s) => s.severity)).toEqual([
      "critical",
      "warning",
    ]);
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

  it("displayName prefers the active workbench alias for shared repo paths", () => {
    const config: WorkbenchConfig = {
      version: 1,
      active: "Client",
      workbenches: [
        {
          name: "Work",
          repos: [{ path: "/home/me/code/api", alias: "Work API", fs_watch: [] }],
        },
        {
          name: "Client",
          repos: [{ path: "/home/me/code/api", alias: "Client API", fs_watch: [] }],
        },
      ],
    };
    store.setConfig(config);
    expect(store.displayName("/home/me/code/api")).toBe("Client API");
  });

  it("displayName includes the distro for WSL repos without an alias", () => {
    const config: WorkbenchConfig = {
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            {
              path: "/home/me/code/api",
              alias: null,
              source: "wsl",
              distro: "Ubuntu-24.04",
              fs_watch: [],
            },
          ],
        },
      ],
    };
    store.setConfig(config);
    expect(store.displayName("/home/me/code/api")).toBe("Ubuntu-24.04:api");
  });

  it("reset clears live repos but notifies subscribers", () => {
    let notified = 0;
    store.subscribe(() => notified++);
    store.applyDelta(delta("/r/a", 1));
    store.reset();
    expect(Object.keys(store.getState().repos)).toHaveLength(0);
    expect(notified).toBeGreaterThan(0);
  });

  it("dropRepo removes a single repo and its activity/events/diffs", () => {
    store.applyDelta(delta("/r/a", 1, { subscribed_diffs: [fileDiff("a.ts")] }));
    store.applyDelta(delta("/r/b", 1));
    store.applyFsEvents({
      repo: "/r/a",
      events: [{ path: ".env", kind: "modified", timestamp_ms: 5000, size: 10, size_delta: 1 }],
    });
    store.dropRepo("/r/a");
    const s = store.getState();
    expect(s.repos["/r/a"]).toBeUndefined();
    expect(s.activity["/r/a"]).toBeUndefined();
    expect(s.diffs["/r/a"]).toBeUndefined();
    expect(s.fsEventsByRepo["/r/a"]).toBeUndefined();
    expect(s.repos["/r/b"]).toBeDefined();
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
