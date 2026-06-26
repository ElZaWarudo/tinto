import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";
import type { FileDiff, RepoDelta } from "../../bus/contract";

const getCommitLogMock = vi.fn();
const getCommitDiffMock = vi.fn();
vi.mock("../../bus/client", () => ({
  getCommitLog: (...a: unknown[]) => getCommitLogMock(...a),
  getCommitDiff: (...a: unknown[]) => getCommitDiffMock(...a),
}));

import { busStore } from "../../bus/store";
import { qualityStore } from "../../qol/state";
import { TimelinePanel } from "./TimelinePanel";

function delta(repo: string, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision: 1,
    status: { modified: ["src/a.ts"], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1_700_000_000_000,
    error: null,
    ...over,
  };
}

const diff = (path: string): FileDiff => ({
  path,
  old_path: null,
  is_binary: false,
  hunks: [
    {
      old_start: 1,
      new_start: 1,
      lines: [
        { kind: "Context", content: "same", old_lineno: 1, new_lineno: 1 },
        { kind: "Added", content: "new line", old_lineno: null, new_lineno: 2 },
      ],
    },
  ],
});

const panelProps = {} as IDockviewPanelProps;

describe("TimelinePanel", () => {
  beforeEach(() => {
    busStore.resetAll();
    qualityStore.reset();
    getCommitLogMock.mockReset();
    getCommitDiffMock.mockReset();
  });

  it("renders activity, commits, and selected commit diffs", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "abc123456789", summary: "ship parser", author: "me", timestamp: 1_700_000_100 },
    ]);
    getCommitDiffMock.mockResolvedValue([diff("src/a.ts"), diff("src/b.ts")]);
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: "/r/api", alias: "API", fs_watch: [] }] }],
      });
      busStore.loadSnapshot([delta("/r/api")], { available: true });
    });

    render(<TimelinePanel {...panelProps} />);

    expect(screen.getByTestId("timeline-feed")).toHaveTextContent("Working tree changed");
    await waitFor(() => expect(screen.getByText(/ship parser/)).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("timeline-commit-abc123456789"));
    await waitFor(() => expect(getCommitDiffMock).toHaveBeenCalledWith("/r/api", "abc123456789"));
    expect(await screen.findByTestId("timeline-files")).toHaveTextContent("src/a.ts");
    expect(screen.getByTestId("diff-view")).toHaveTextContent("new line");
  });

  it("shows degraded and empty states", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([], { available: false, reason: "watcher failed" }));

    render(<TimelinePanel {...panelProps} />);

    expect(screen.getByTestId("timeline-degraded")).toHaveTextContent("degraded");
    expect(screen.getByTestId("timeline-empty")).toHaveTextContent("No repos");
  });

  it("keeps commit diff failures retryable", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "badc0de", summary: "broken", author: "me", timestamp: 1_700_000_100 },
    ]);
    getCommitDiffMock.mockRejectedValueOnce(new Error("missing commit"));
    getCommitDiffMock.mockResolvedValueOnce([diff("src/a.ts")]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));

    render(<TimelinePanel {...panelProps} />);
    await screen.findByText(/broken/);
    fireEvent.click(screen.getByTestId("timeline-commit-badc0de"));
    expect(await screen.findByTestId("timeline-diff-error")).toHaveTextContent("missing commit");
    fireEvent.click(screen.getByText("Retry"));
    expect(await screen.findByTestId("timeline-files")).toHaveTextContent("src/a.ts");
  });

  it("does not reload all commit logs for status-only updates", async () => {
    getCommitLogMock.mockImplementation((repo: string) =>
      Promise.resolve([
        {
          id: repo === "/r/api" ? "api-head" : "web-head",
          summary: repo === "/r/api" ? "api commit" : "web commit",
          author: "me",
          timestamp: 1_700_000_100,
        },
      ]),
    );
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              { path: "/r/api", alias: "API", fs_watch: [] },
              { path: "/r/web", alias: "WEB", fs_watch: [] },
            ],
          },
        ],
      });
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            head: { id: "api-head", summary: "api commit", author: "me", timestamp: 1_700_000_100 },
          }),
          delta("/r/web", {
            head: { id: "web-head", summary: "web commit", author: "me", timestamp: 1_700_000_100 },
          }),
        ],
        { available: true },
      );
    });

    render(<TimelinePanel {...panelProps} />);
    await waitFor(() => expect(getCommitLogMock).toHaveBeenCalledTimes(2));

    act(() =>
      busStore.applyDelta(
        delta("/r/api", {
          revision: 2,
          status: { modified: ["src/a.ts", "src/b.ts"], staged: [], untracked: [] },
          head: { id: "api-head", summary: "api commit", author: "me", timestamp: 1_700_000_100 },
        }),
      ),
    );
    await waitFor(() => expect(screen.getByTestId("timeline-feed")).toHaveTextContent("2M 0S 0U"));
    expect(getCommitLogMock).toHaveBeenCalledTimes(2);

    act(() =>
      busStore.applyDelta(
        delta("/r/api", {
          revision: 3,
          head: { id: "api-next", summary: "api next", author: "me", timestamp: 1_700_000_200 },
        }),
      ),
    );

    await waitFor(() => expect(getCommitLogMock).toHaveBeenCalledTimes(3));
    expect(getCommitLogMock).toHaveBeenLastCalledWith("/r/api", 0, 8);
  });

  it("applies the time filter to commit entries", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "oldc0de", summary: "old commit", author: "me", timestamp: 1_700_000_100 },
    ]);
    act(() => {
      qualityStore.setFilters({ timeWindow: "15m" });
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            status: { modified: [], staged: [], untracked: [] },
            last_activity_ms: 1_700_000_000_000,
          }),
        ],
        { available: true },
      );
    });

    render(<TimelinePanel {...panelProps} />);

    expect(await screen.findByTestId("timeline-no-matches")).toHaveTextContent("No timeline");
    expect(screen.queryByText(/old commit/)).not.toBeInTheDocument();
  });
});
