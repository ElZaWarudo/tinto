import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, beforeEach } from "vitest";
import { busStore } from "../bus/store";
import type { RepoDelta } from "../bus/contract";
import { qualityStore } from "./state";
import { GlanceMode } from "./GlanceMode";

const delta = (repo: string, over: Partial<RepoDelta> = {}): RepoDelta => ({
  repo,
  revision: 1,
  status: { modified: [], staged: [], untracked: [] },
  branch: null,
  head: null,
  last_activity_ms: Date.now(),
  error: null,
  ...over,
});

describe("GlanceMode", () => {
  beforeEach(() => {
    busStore.resetAll();
  });

  it("summarizes visible repos, dirty repos, signals, and watcher state", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: "/r/api", alias: "API", fs_watch: [] }] }],
      });
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            status: { modified: ["src/a.ts"], staged: [], untracked: [] },
            signals: [
              {
                kind: "possible_secret",
                severity: "critical",
                path: "src/a.ts",
                message: "Possible secret",
              },
            ],
          }),
        ],
        { available: false, reason: "watcher failed" },
      );
    });

    render(<GlanceMode />);

    expect(screen.getByTestId("glance-mode")).toHaveTextContent("1 dirty repos");
    expect(screen.getByText("Critical").parentElement).toHaveTextContent("1");
    expect(screen.getByText("Watcher").parentElement).toHaveTextContent("degraded");
  });

  it("shows a no-match state for active filters", () => {
    act(() => {
      busStore.loadSnapshot([delta("/r/api")], { available: true });
      qualityStore.setFilters({ search: "missing" });
    });

    render(<GlanceMode />);

    expect(screen.getByTestId("glance-no-matches")).toHaveTextContent("No repos match");
  });
});
