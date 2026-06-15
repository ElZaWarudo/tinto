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
});
