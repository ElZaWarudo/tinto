import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent, within } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";
import type { CommitInfo, FileDiff, RepoDelta } from "../../bus/contract";

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
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

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

    expect(screen.getByTestId("timeline-feed")).toHaveTextContent("Árbol de trabajo modificado");
    await waitFor(() => expect(screen.getByText(/ship parser/)).toBeInTheDocument());
    const feedRows = within(screen.getByTestId("timeline-feed")).getAllByRole("listitem");
    expect(feedRows[0]).toHaveTextContent("ship parser");
    expect(feedRows[1]).toHaveTextContent("Árbol de trabajo modificado");

    const commitButton = screen.getByTestId("timeline-commit-abc123456789");
    fireEvent.click(commitButton);
    expect(commitButton).toHaveAttribute("aria-current", "true");
    await waitFor(() => expect(getCommitDiffMock).toHaveBeenCalledWith("/r/api", "abc123456789"));
    expect(await screen.findByTestId("timeline-files")).toHaveTextContent("src/a.ts");
    expect(screen.getByTestId("diff-view")).toHaveTextContent("new line");
    expect(screen.getByRole("heading", { name: "ship parser" })).toHaveFocus();

    fireEvent.click(screen.getByRole("button", { name: "Volver a la cronología" }));
    await waitFor(() => expect(commitButton).toHaveFocus());
    expect(commitButton).not.toHaveAttribute("aria-current");
  });

  it("groups entries by local day and exposes complete machine-readable timestamps", async () => {
    const newer = new Date(2026, 6, 28, 18, 30, 0);
    const older = new Date(2026, 6, 27, 9, 15, 0);
    getCommitLogMock.mockResolvedValue([
      {
        id: "newer-day",
        summary: "newer",
        author: "me",
        timestamp: Math.floor(newer.getTime() / 1000),
      },
      {
        id: "older-day",
        summary: "older",
        author: "me",
        timestamp: Math.floor(older.getTime() / 1000),
      },
    ]);
    act(() =>
      busStore.loadSnapshot(
        [delta("/r/api", { status: { modified: [], staged: [], untracked: [] } })],
        { available: true },
      ),
    );

    render(<TimelinePanel {...panelProps} />);

    await screen.findByTestId("timeline-commit-newer-day");
    const expectedNewerLabel = newer.toLocaleDateString(undefined, {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: "numeric",
    });
    const expectedOlderLabel = older.toLocaleDateString(undefined, {
      weekday: "long",
      day: "numeric",
      month: "long",
      year: "numeric",
    });
    expect(screen.getByRole("heading", { level: 3, name: expectedNewerLabel })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: expectedOlderLabel })).toBeInTheDocument();

    const newerRow = screen.getByTestId("timeline-commit-newer-day").closest("li");
    expect(newerRow?.querySelector("time")).toHaveAttribute("datetime", newer.toISOString());
    expect(newerRow?.querySelector("time")).toHaveAccessibleName(
      newer.toLocaleString(undefined, { dateStyle: "long", timeStyle: "medium" }),
    );
  });

  it("searches commit metadata even when the current repo delta does not match", async () => {
    getCommitLogMock.mockResolvedValue([
      {
        id: "unique-sha",
        summary: "prepare quantum migration",
        author: "Ada Lovelace",
        timestamp: 1_700_000_100,
      },
    ]);
    act(() => {
      busStore.loadSnapshot([delta("/r/api")], { available: true });
      qualityStore.setFilters({ search: "quantum migration" });
    });

    render(<TimelinePanel {...panelProps} />);

    expect(await screen.findByTestId("timeline-commit-unique-sha")).toHaveTextContent(
      "prepare quantum migration",
    );
    expect(getCommitLogMock).toHaveBeenCalledWith("/r/api", 0, expect.any(Number));
  });

  it("ignores a stale commit diff response after a newer selection", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "commit-a", summary: "commit A", author: "me", timestamp: 1_700_000_200 },
      { id: "commit-b", summary: "commit B", author: "me", timestamp: 1_700_000_100 },
    ]);
    const requestA = deferred<FileDiff[]>();
    const requestB = deferred<FileDiff[]>();
    getCommitDiffMock.mockImplementation((_repo: string, commitId: string) =>
      commitId === "commit-a" ? requestA.promise : requestB.promise,
    );
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    render(<TimelinePanel {...panelProps} />);

    await screen.findByTestId("timeline-commit-commit-a");
    fireEvent.click(screen.getByTestId("timeline-commit-commit-a"));
    fireEvent.click(screen.getByTestId("timeline-commit-commit-b"));

    await act(async () => requestB.resolve([diff("src/b.ts")]));
    expect(await screen.findByTestId("timeline-files")).toHaveTextContent("src/b.ts");

    await act(async () => requestA.resolve([diff("src/a.ts")]));
    expect(screen.getByTestId("timeline-files")).toHaveTextContent("src/b.ts");
    expect(screen.getByTestId("timeline-files")).not.toHaveTextContent("src/a.ts");
  });

  it("keeps commit loading distinct from an empty timeline", async () => {
    let resolveLog: ((items: unknown[]) => void) | null = null;
    getCommitLogMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveLog = resolve;
        }),
    );
    act(() => {
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            status: { modified: [], staged: [], untracked: [] },
          }),
        ],
        { available: true },
      );
    });

    render(<TimelinePanel {...panelProps} />);

    expect(await screen.findByTestId("timeline-commits-loading")).toHaveTextContent(
      "Cargando historial de commits",
    );
    expect(screen.getByRole("region", { name: "Entradas de la cronología" })).toHaveAttribute(
      "aria-busy",
      "true",
    );
    expect(screen.queryByTestId("timeline-no-matches")).not.toBeInTheDocument();
    expect(screen.queryByTestId("timeline-log-error")).not.toBeInTheDocument();

    await act(async () => resolveLog?.([]));
    expect(await screen.findByTestId("timeline-no-matches")).toHaveTextContent(
      "Ninguna entrada de la cronología",
    );
    expect(screen.queryByTestId("timeline-commits-loading")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Entradas de la cronología" })).toHaveAttribute(
      "aria-busy",
      "false",
    );
  });

  it("keeps commit-log failures distinct and retryable", async () => {
    getCommitLogMock.mockRejectedValueOnce(new Error("history unavailable"));
    getCommitLogMock.mockResolvedValueOnce([
      { id: "retry123", summary: "recovered history", author: "me", timestamp: 1_700_000_100 },
    ]);
    act(() => {
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            status: { modified: [], staged: [], untracked: [] },
          }),
        ],
        { available: true },
      );
    });

    render(<TimelinePanel {...panelProps} />);

    expect(await screen.findByTestId("timeline-log-error")).toHaveTextContent(
      "history unavailable",
    );
    expect(screen.queryByTestId("timeline-no-matches")).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Reintentar la carga del historial de commits" }),
    );
    expect(await screen.findByText(/recovered history/)).toBeInTheDocument();
    expect(getCommitLogMock).toHaveBeenCalledTimes(2);
    expect(screen.queryByTestId("timeline-log-error")).not.toBeInTheDocument();
  });

  it("shows degraded and empty states", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([], { available: false, reason: "watcher failed" }));

    render(<TimelinePanel {...panelProps} />);

    expect(screen.getByTestId("timeline-degraded")).toHaveTextContent("degradado");
    expect(screen.getByTestId("timeline-empty")).toHaveTextContent("No hay repositorios");
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
    fireEvent.click(screen.getByText("Reintentar"));
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

  it("publishes each repo history as it resolves while aggregate loading remains pending", async () => {
    const apiLog = deferred<CommitInfo[]>();
    const webLog = deferred<CommitInfo[]>();
    getCommitLogMock.mockImplementation((repo: string) =>
      repo === "/r/api" ? apiLog.promise : webLog.promise,
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
            head: { id: "api-head", summary: "api head", author: "me", timestamp: 1_700_000_100 },
          }),
          delta("/r/web", {
            head: { id: "web-head", summary: "web head", author: "me", timestamp: 1_700_000_100 },
          }),
        ],
        { available: true },
      );
    });

    render(<TimelinePanel {...panelProps} />);
    await waitFor(() => expect(getCommitLogMock).toHaveBeenCalledTimes(2));

    await act(async () =>
      apiLog.resolve([
        {
          id: "api-fast",
          summary: "api history arrived",
          author: "me",
          timestamp: 1_700_000_200,
        },
      ]),
    );

    expect(await screen.findByTestId("timeline-commit-api-fast")).toBeInTheDocument();
    expect(screen.queryByTestId("timeline-commit-web-slow")).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Entradas de la cronología" })).toHaveAttribute(
      "aria-busy",
      "true",
    );

    await act(async () =>
      webLog.resolve([
        {
          id: "web-slow",
          summary: "web history arrived",
          author: "me",
          timestamp: 1_700_000_150,
        },
      ]),
    );

    expect(await screen.findByTestId("timeline-commit-web-slow")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Entradas de la cronología" })).toHaveAttribute(
      "aria-busy",
      "false",
    );
  });

  it("keeps prior commit rows while refreshing and after a refresh failure", async () => {
    const refresh = deferred<CommitInfo[]>();
    getCommitLogMock.mockResolvedValueOnce([
      { id: "old-commit", summary: "old history", author: "me", timestamp: 1_700_000_100 },
    ]);
    act(() =>
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            head: { id: "old-head", summary: "old", author: "me", timestamp: 1_700_000_100 },
          }),
        ],
        { available: true },
      ),
    );

    render(<TimelinePanel {...panelProps} />);
    expect(await screen.findByTestId("timeline-commit-old-commit")).toBeInTheDocument();

    getCommitLogMock.mockReturnValueOnce(refresh.promise);
    act(() =>
      busStore.applyDelta(
        delta("/r/api", {
          revision: 2,
          head: { id: "new-head", summary: "new", author: "me", timestamp: 1_700_000_200 },
        }),
      ),
    );

    await waitFor(() => expect(getCommitLogMock).toHaveBeenCalledTimes(2));
    expect(screen.getByTestId("timeline-commit-old-commit")).toBeInTheDocument();
    expect(screen.getByTestId("timeline-commits-refreshing")).toHaveTextContent("Actualizando");

    await act(async () =>
      refresh.resolve([
        {
          id: "new-commit",
          summary: "new history",
          author: "me",
          timestamp: 1_700_000_200,
        },
      ]),
    );
    expect(await screen.findByTestId("timeline-commit-new-commit")).toBeInTheDocument();
    expect(screen.queryByTestId("timeline-commit-old-commit")).not.toBeInTheDocument();
    expect(screen.queryByTestId("timeline-commits-refreshing")).not.toBeInTheDocument();

    getCommitLogMock.mockRejectedValueOnce(new Error("refresh offline"));
    act(() =>
      busStore.applyDelta(
        delta("/r/api", {
          revision: 3,
          head: {
            id: "failed-head",
            summary: "failed",
            author: "me",
            timestamp: 1_700_000_300,
          },
        }),
      ),
    );

    expect(await screen.findByTestId("timeline-log-error")).toHaveTextContent("refresh offline");
    expect(screen.getByTestId("timeline-commit-new-commit")).toBeInTheDocument();
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

    expect(await screen.findByTestId("timeline-no-matches")).toHaveTextContent(
      "Ninguna entrada de la cronología",
    );
    expect(screen.queryByText(/old commit/)).not.toBeInTheDocument();
  });
});
