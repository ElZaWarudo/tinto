import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act } from "@testing-library/react";

// Stub the default tab so we don't need a live dockview context.
vi.mock("dockview-react", () => ({
  DockviewDefaultTab: (p: { params?: { repo?: string } }) => (
    <span data-testid="default-tab">{p.params?.repo}</span>
  ),
}));

import { RepoTab } from "./RepoTab";
import { busStore } from "../bus/store";
import type { RepoDelta } from "../bus/contract";

const REPO = "/r/api";

function delta(over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: REPO,
    revision: 1,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 0,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
    ...over,
  };
}

const props = { params: { repo: REPO } } as unknown as Parameters<typeof RepoTab>[0];

describe("RepoTab", () => {
  beforeEach(() => busStore.resetAll());

  it("shows no change dot when the repo is clean", () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<RepoTab {...props} />);
    expect(screen.getByTestId("default-tab")).toBeInTheDocument();
    expect(screen.queryByTestId(`repo-tab-changed-${REPO}`)).not.toBeInTheDocument();
  });

  it("lights up the change dot when the repo has working-tree changes", () => {
    act(() =>
      busStore.loadSnapshot(
        [delta({ status: { modified: ["src/a.ts"], staged: [], untracked: [] } })],
        { available: true },
      ),
    );
    render(<RepoTab {...props} />);
    expect(screen.getByTestId(`repo-tab-changed-${REPO}`)).toBeInTheDocument();
  });
});
