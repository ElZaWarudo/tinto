import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";

const getCommitLogMock = vi.fn();
const retryRepoMock = vi.fn();
vi.mock("../bus/client", () => ({
  getCommitLog: (...a: unknown[]) => getCommitLogMock(...a),
  retryRepo: (...a: unknown[]) => retryRepoMock(...a),
}));

import { RepoPanel } from "./RepoPanel";
import { openRepoPanel } from "../workspace/openRepo";
import { busStore } from "../bus/store";
import type { RepoDelta } from "../bus/contract";

function delta(repo: string, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision: 1,
    status: { modified: ["src/a.rs"], staged: [], untracked: ["new.txt"] },
    branch: { name: "main", detached: false, unborn: false, ahead: 0, behind: 0 },
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

const panelProps = (repo: string) =>
  ({ params: { repo } }) as unknown as IDockviewPanelProps<{ repo: string }>;

describe("RepoPanel", () => {
  beforeEach(() => {
    busStore.resetAll();
    getCommitLogMock.mockReset();
    retryRepoMock.mockReset();
  });

  it("renders the full status lists and the commit log", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "abc123", summary: "fix parser", author: "me", timestamp: 1_699_999_000 },
    ]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    render(<RepoPanel {...panelProps("/r/api")} />);

    // Full file lists (the differentiator from the card), not just counts.
    expect(screen.getByTestId("status-lists")).toHaveTextContent("src/a.rs");
    expect(screen.getByTestId("status-lists")).toHaveTextContent("new.txt");
    await waitFor(() => expect(screen.getByTestId("commit-log")).toHaveTextContent("fix parser"));
    expect(getCommitLogMock).toHaveBeenCalledWith("/r/api", 0, 30);
  });

  it("shows a terminal error with a working retry", async () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() =>
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            error: { class: "terminal", category: "repo-removed", message: "gone" },
          }),
        ],
        { available: true },
      ),
    );
    render(<RepoPanel {...panelProps("/r/api")} />);
    expect(screen.getByTestId("repo-panel-error")).toHaveTextContent("gone");
    screen.getByTestId("repo-panel-retry").click();
    expect(retryRepoMock).toHaveBeenCalledWith("/r/api");
  });

  it("shows a graceful message when the repo left the workbench", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([], { available: true }));
    render(<RepoPanel {...panelProps("/r/gone")} />);
    expect(screen.getByText(/no longer in the active workbench/i)).toBeInTheDocument();
  });

  // Covers AE9: a status-list file opens its diff on double-click.
  it("opens a diff when a status file is activated", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    const openDiff = vi.fn();
    const value: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openDiff,
    };
    render(
      <WorkspaceActionsContext.Provider value={value}>
        <RepoPanel {...panelProps("/r/api")} />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.doubleClick(screen.getByTestId("status-file-src/a.rs"));
    expect(openDiff).toHaveBeenCalledWith("/r/api", "src/a.rs");
  });
});

describe("openRepoPanel (dedup)", () => {
  // Covers AE6: opening the same repo focuses the existing panel, no duplicate.
  function fakeApi() {
    const panels: Record<string, { api: { setActive: ReturnType<typeof vi.fn> } }> = {};
    return {
      addPanel: vi.fn((opts: { id: string }) => {
        panels[opts.id] = { api: { setActive: vi.fn() } };
        return panels[opts.id];
      }),
      getPanel: vi.fn((id: string) => panels[id]),
      _panels: panels,
    };
  }

  it("adds a panel the first time and focuses it the second time", () => {
    const api = fakeApi();
    // first open -> addPanel
    openRepoPanel(api as never, "/r/api", "api");
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    const created = api._panels["repo:/r/api"];
    // second open -> focus existing, no new panel
    openRepoPanel(api as never, "/r/api", "api");
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });
});
