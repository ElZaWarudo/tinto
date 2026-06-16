import { describe, it, expect } from "vitest";
import { BusStore } from "../../bus/store";
import type { RepoDelta } from "../../bus/contract";
import { buildTimelineEntries, ORPHAN_QUIET_MS } from "./model";

function delta(repo: string, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision: 1,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

describe("timeline model", () => {
  it("sorts repo activity newest-first and uses display names", () => {
    const store = new BusStore();
    store.setConfig({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            { path: "/r/a", alias: "API", fs_watch: [] },
            { path: "/r/b", alias: "Web", fs_watch: [] },
          ],
        },
      ],
    });
    store.loadSnapshot(
      [
        delta("/r/a", {
          status: { modified: ["a.ts"], staged: [], untracked: [] },
          last_activity_ms: 2000,
        }),
        delta("/r/b", {
          status: { modified: ["b.ts"], staged: [], untracked: [] },
          last_activity_ms: 5000,
        }),
      ],
      { available: true },
    );

    const entries = buildTimelineEntries(store.getState(), (repo) => store.displayName(repo), 6000);

    expect(entries.filter((e) => e.kind === "activity").map((e) => e.repoName)).toEqual([
      "Web",
      "API",
    ]);
  });

  it("includes passive signal count in current activity detail", () => {
    const store = new BusStore();
    store.loadSnapshot(
      [
        delta("/r/api", {
          status: { modified: ["src/a.ts"], staged: [], untracked: [] },
          signals: [
            {
              kind: "possible_secret",
              severity: "critical",
              path: "src/a.ts",
              message: "Possible secret marker added",
            },
          ],
        }),
      ],
      { available: true },
    );

    const entries = buildTimelineEntries(store.getState(), () => "api", 5000);

    expect(entries.find((e) => e.kind === "activity")?.detail).toContain("1 passive signal");
  });

  it("includes Plane 2 file events without turning them into diffs", () => {
    const store = new BusStore();
    store.loadSnapshot([delta("/r/api")], { available: true });
    store.applyFsEvents({
      repo: "/r/api",
      events: [
        {
          path: ".env",
          kind: "modified",
          timestamp_ms: 4000,
          size: 20,
          size_delta: 4,
        },
      ],
    });

    const entries = buildTimelineEntries(store.getState(), () => "api", 5000);

    expect(entries).toContainEqual(
      expect.objectContaining({
        kind: "fs-event",
        repoName: "api",
        path: ".env",
        fsKind: "modified",
      }),
    );
  });

  it("flags only dirty repos that have exceeded the quiet threshold", () => {
    const store = new BusStore();
    store.loadSnapshot(
      [
        delta("/r/old", {
          status: { modified: ["old.ts"], staged: [], untracked: [] },
          last_activity_ms: 1000,
        }),
        delta("/r/new", {
          status: { modified: ["new.ts"], staged: [], untracked: [] },
          last_activity_ms: 1000 + ORPHAN_QUIET_MS - 1,
        }),
        delta("/r/clean", { last_activity_ms: 1000 }),
      ],
      { available: true },
    );

    const entries = buildTimelineEntries(store.getState(), (repo) => repo, 1000 + ORPHAN_QUIET_MS);

    expect(entries.filter((e) => e.kind === "orphan").map((e) => e.repo)).toEqual(["/r/old"]);
  });

  it("adds degraded and error entries distinctly", () => {
    const store = new BusStore();
    store.loadSnapshot(
      [
        delta("/r/api", {
          error: { class: "terminal", category: "repo-removed", message: "gone" },
          last_activity_ms: 3000,
        }),
      ],
      { available: false, reason: "watcher failed" },
    );

    const entries = buildTimelineEntries(store.getState(), () => "api", 5000);

    expect(entries).toContainEqual(
      expect.objectContaining({ kind: "degraded", detail: "watcher failed" }),
    );
    expect(entries).toContainEqual(expect.objectContaining({ kind: "error", detail: "gone" }));
  });
});
