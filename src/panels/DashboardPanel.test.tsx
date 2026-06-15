import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { DashboardPanel } from "./DashboardPanel";
import { busStore } from "../bus/store";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import type { RepoDelta } from "../bus/contract";

function delta(repo: string, revision: number, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision,
    status: { modified: [], staged: [], untracked: [] },
    branch: { name: "main", detached: false, unborn: false, ahead: 0, behind: 0 },
    head: null,
    last_activity_ms: revision * 1000,
    error: null,
    ...over,
  };
}

function renderDash(actions: Partial<WorkspaceActions> = {}) {
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
      <DashboardPanel />
    </WorkspaceActionsContext.Provider>,
  );
  return value;
}

describe("DashboardPanel", () => {
  beforeEach(() => busStore.resetAll());

  // Covers AE12: loading skeletons before the snapshot
  it("shows skeletons until the snapshot is loaded", () => {
    renderDash();
    expect(screen.getByTestId("skeletons")).toBeInTheDocument();
  });

  // Covers AE12: zero-repos state with an Add action
  it("shows the zero-repos state with a working Add button", () => {
    act(() => busStore.loadSnapshot([], { available: true }));
    const { addRepo } = renderDash();
    expect(screen.getByTestId("zero-repos")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Add repo"));
    expect(addRepo).toHaveBeenCalledOnce();
  });

  it("renders a card per repo", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1), delta("/r/b", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("card-/r/a")).toBeInTheDocument();
    expect(screen.getByTestId("card-/r/b")).toBeInTheDocument();
  });

  // Covers AE2: a status change updates that card live
  it("updates a card's counts when a newer delta arrives", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("card-/r/a").querySelector(".count--modified")).toHaveTextContent(
      "0M",
    );
    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, { status: { modified: ["x", "y"], staged: [], untracked: [] } }),
      ),
    );
    expect(screen.getByTestId("card-/r/a").querySelector(".count--modified")).toHaveTextContent(
      "2M",
    );
  });

  // Covers AE3: a new commit / branch update reflects live
  it("reflects a branch update from a newer delta", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("branch")).toHaveTextContent("main");
    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, {
          branch: { name: "feature/x", detached: false, unborn: false, ahead: 2, behind: 0 },
        }),
      ),
    );
    expect(screen.getByTestId("branch")).toHaveTextContent("feature/x");
  });

  // Covers AE8: degraded watching banner
  it("shows the degraded banner when watching is unavailable", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: false, reason: "inotify" }));
    renderDash();
    expect(screen.getByTestId("degraded-banner")).toHaveTextContent("inotify");
  });
});
