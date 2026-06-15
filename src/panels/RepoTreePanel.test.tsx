import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import { RepoTreePanel } from "./RepoTreePanel";
import { busStore } from "../bus/store";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import type { RepoDelta, RepoTree } from "../bus/contract";

function delta(repo: string, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision: 1,
    status: { modified: [], staged: [], untracked: [] },
    branch: { name: "main", detached: false, unborn: false, ahead: 0, behind: 0 },
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

function renderTree(actions: Partial<WorkspaceActions> = {}) {
  const value: WorkspaceActions = {
    openRepo: vi.fn(),
    addRepo: vi.fn(),
    removeRepo: vi.fn(),
    openDiff: vi.fn(),
    openTimeline: vi.fn(),
    ...actions,
  };
  render(
    <WorkspaceActionsContext.Provider value={value}>
      <RepoTreePanel />
    </WorkspaceActionsContext.Provider>,
  );
  return value;
}

describe("RepoTreePanel", () => {
  beforeEach(() => {
    busStore.resetAll();
    invokeMock.mockReset();
  });

  it("renders a node per repo with status; the name opens the repo", () => {
    act(() =>
      busStore.loadSnapshot(
        [delta("/r/api", { status: { modified: ["x"], staged: ["y"], untracked: ["z", "w"] } })],
        { available: true },
      ),
    );
    const { openRepo } = renderTree();
    expect(screen.getByTestId("tree-node-/r/api")).toHaveTextContent("1M 1S 2U");
    fireEvent.click(screen.getByTestId("tree-node-/r/api"));
    expect(openRepo).toHaveBeenCalledWith("/r/api");
  });

  it("expands to files, marks changed ones, and opens a diff on a file (AE8/AE9)", async () => {
    const tree: RepoTree = {
      entries: [
        { path: "src", is_dir: true },
        { path: "src/a.ts", is_dir: false },
        { path: "src/b.ts", is_dir: false },
      ],
      truncated: false,
    };
    invokeMock.mockResolvedValue(tree);
    act(() =>
      busStore.loadSnapshot(
        [delta("/r/api", { status: { modified: ["src/a.ts"], staged: [], untracked: [] } })],
        { available: true },
      ),
    );
    const { openDiff } = renderTree();

    fireEvent.click(screen.getByTestId("tree-expand-/r/api"));
    // Loading then files; expand the folder to reveal them.
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_repo_tree", { repo: "/r/api" }),
    );
    fireEvent.click(await screen.findByText("src")); // expand folder (collapsed by default)

    const aFile = await screen.findByTestId("tree-file-src/a.ts");
    expect(aFile).toHaveClass("tree-file--changed");
    fireEvent.doubleClick(aFile);
    expect(openDiff).toHaveBeenCalledWith("/r/api", "src/a.ts");
  });

  it("shows a truncated notice when the tree is capped", async () => {
    invokeMock.mockResolvedValue({ entries: [], truncated: true } as RepoTree);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    renderTree();
    fireEvent.click(screen.getByTestId("tree-expand-/r/api"));
    expect(await screen.findByTestId("tree-truncated-/r/api")).toBeInTheDocument();
  });

  it("shows an empty message with no repos", () => {
    act(() => busStore.loadSnapshot([], { available: true }));
    renderTree();
    expect(screen.getByText("No repos.")).toBeInTheDocument();
  });
});
