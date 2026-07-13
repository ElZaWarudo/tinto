import { describe, expect, it } from "vitest";
import type { FsEvent, RepoDelta } from "../bus/contract";
import type { BusState } from "../bus/store";
import type { TimelineEntry } from "../panels/timeline/model";
import { buildFileTree } from "../panels/tree/fileTree";
import {
  filterFsEvents,
  filterRepoPaths,
  filterStatusFiles,
  filterTimelineEntries,
  filterTreeNodes,
  matchesTimeWindow,
  normalizedExtension,
  pathExtension,
} from "./filters";
import type { QualityFilters } from "./state";

const filters = (over: Partial<QualityFilters> = {}): QualityFilters => ({
  search: "",
  repo: "all",
  extension: "",
  timeWindow: "all",
  ...over,
});

const delta = (repo: string, over: Partial<RepoDelta> = {}): RepoDelta => ({
  repo,
  revision: 1,
  status: { modified: [], staged: [], untracked: [] },
  branch: null,
  head: null,
  last_activity_ms: 1_700_000_000_000,
  error: null,
  metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
  gitleaks_configured: false,
  agents_md_configured: false,
  ...over,
});

describe("quality filters", () => {
  it("normalizes extensions and extracts path extensions", () => {
    expect(normalizedExtension("ts")).toBe(".ts");
    expect(normalizedExtension(".RS")).toBe(".rs");
    expect(pathExtension("src/App.test.tsx")).toBe(".tsx");
  });

  it("filters repos by repo, search text, and extension", () => {
    const state: BusState = {
      repos: {
        "/r/api": delta("/r/api", {
          status: { modified: ["src/server.rs"], staged: [], untracked: [] },
        }),
        "/r/web": delta("/r/web", {
          status: { modified: ["src/App.tsx"], staged: [], untracked: [] },
          signals: [
            {
              kind: "config_change",
              severity: "warning",
              path: "vite.config.ts",
              message: "Config changed",
            },
          ],
        }),
      },
      activity: {},
      diffs: {},
      fsEventsByRepo: {},
      watching: { available: true },
      config: null,
      configStatus: "ready",
      configError: null,
      snapshotStatus: "ready",
      snapshotError: null,
      loaded: true,
    };

    expect(
      filterRepoPaths(state, ["/r/api", "/r/web"], filters({ search: "config" }), (repo) =>
        repo === "/r/web" ? "Web" : "API",
      ),
    ).toEqual(["/r/web"]);
    expect(
      filterRepoPaths(state, ["/r/api", "/r/web"], filters({ extension: ".rs" }), (repo) => repo),
    ).toEqual(["/r/api"]);
    expect(
      filterRepoPaths(state, ["/r/api", "/r/web"], filters({ repo: "/r/web" }), (repo) => repo),
    ).toEqual(["/r/web"]);
  });

  it("filters status files, watched events, tree nodes, and timeline entries", () => {
    const q = filters({ search: "env", extension: ".env", timeWindow: "15m" });
    const now = 1_700_000_060_000;
    const event: FsEvent = {
      path: ".env",
      kind: "modified",
      timestamp_ms: 1_700_000_030_000,
      size: 10,
      size_delta: 1,
    };
    const oldEvent = { ...event, timestamp_ms: 1_699_000_000_000 };
    const timeline: TimelineEntry[] = [
      {
        id: "1",
        kind: "fs-event",
        repo: "/r/api",
        repoName: "API",
        timestampMs: event.timestamp_ms,
        title: "Watched file modified",
        detail: ".env",
        path: ".env",
      },
      {
        id: "2",
        kind: "activity",
        repo: "/r/api",
        repoName: "API",
        timestampMs: oldEvent.timestamp_ms,
        title: "Old",
        detail: "src/a.ts",
      },
    ];

    expect(filterStatusFiles([".env", "src/a.ts"], q)).toEqual([".env"]);
    expect(filterFsEvents("/r/api", [event, oldEvent], q, "API", now)).toEqual([event]);
    expect(filterTimelineEntries(timeline, q, now).map((entry) => entry.id)).toEqual(["1"]);
    expect(matchesTimeWindow(event.timestamp_ms, q, now)).toBe(true);

    const nodes = buildFileTree(
      [
        { path: "src", is_dir: true },
        { path: "src/a.ts", is_dir: false },
        { path: ".env", is_dir: false },
      ],
      { modified: [], staged: [], untracked: [] },
    );
    expect(filterTreeNodes(nodes, q).map((node) => node.path)).toEqual([".env"]);
  });
});
