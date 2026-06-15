import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import { RepoTreePanel } from "./RepoTreePanel";
import { busStore } from "../bus/store";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import type { RepoDelta } from "../bus/contract";

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
  beforeEach(() => busStore.resetAll());

  // Covers AE7: a node per repo with status; click opens the repo; no file nodes.
  it("renders a node per repo with a status summary and opens on click", () => {
    act(() =>
      busStore.loadSnapshot(
        [
          delta("/r/api", { status: { modified: ["x"], staged: ["y"], untracked: ["z", "w"] } }),
          delta("/r/web"),
        ],
        { available: true },
      ),
    );
    const { openRepo } = renderTree();

    expect(screen.getByTestId("tree-node-/r/api")).toHaveTextContent("1M 1S 2U");
    expect(screen.getByTestId("tree-node-/r/web")).toHaveTextContent("clean");
    // No file-level nodes in 007.
    expect(screen.queryByText("x")).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("tree-node-/r/api"));
    expect(openRepo).toHaveBeenCalledWith("/r/api");
  });

  it("updates a node's status live", () => {
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    renderTree();
    expect(screen.getByTestId("tree-node-/r/api")).toHaveTextContent("clean");
    act(() =>
      busStore.applyDelta(
        delta("/r/api", {
          revision: 2,
          status: { modified: ["a", "b"], staged: [], untracked: [] },
        }),
      ),
    );
    expect(screen.getByTestId("tree-node-/r/api")).toHaveTextContent("2M 0S 0U");
  });

  it("shows an empty message with no repos", () => {
    act(() => busStore.loadSnapshot([], { available: true }));
    renderTree();
    expect(screen.getByText("No repos.")).toBeInTheDocument();
  });
});
