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
  metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
  gitleaks_configured: false,
  agents_md_configured: false,
  secret_scan_status: { state: "not_run" },
  ...over,
});

describe("GlanceMode", () => {
  beforeEach(() => {
    busStore.resetAll();
    qualityStore.resetFilters();
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

    expect(screen.getByTestId("glance-mode")).toHaveTextContent("Con cambios1");
    expect(screen.getByText("Críticas").parentElement).toHaveTextContent("1");
    expect(screen.getByText("Supervisión").parentElement).toHaveTextContent("Degradado");
  });

  it("shows a no-match state for active filters", () => {
    act(() => {
      busStore.loadSnapshot([delta("/r/api")], { available: true });
      qualityStore.setFilters({ search: "missing" });
    });

    render(<GlanceMode />);

    expect(screen.getByTestId("glance-no-matches")).toHaveTextContent(
      "Ningún repositorio coincide",
    );
  });
});
