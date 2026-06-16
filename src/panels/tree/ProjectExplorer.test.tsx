import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

let tree: unknown = { entries: [], truncated: false };
const listRepoTreeMock = vi.fn(() => Promise.resolve(tree));
vi.mock("../../bus/client", () => ({
  listRepoTree: () => listRepoTreeMock(),
}));

import { ProjectExplorer } from "./ProjectExplorer";
import { busStore } from "../../bus/store";
import { fileDock } from "../../workspace/fileDock";
import { repoTreeStore } from "../../workspace/repoTreeStore";
import type { RepoDelta } from "../../bus/contract";

const REPO = "/r/api";

function delta(over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: REPO,
    revision: 1,
    status: { modified: ["src/a.ts"], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

describe("ProjectExplorer", () => {
  beforeEach(() => {
    busStore.resetAll();
    fileDock.drop(REPO);
    repoTreeStore.reset();
    listRepoTreeMock.mockClear();
    tree = {
      entries: [
        { path: "src", is_dir: true },
        { path: "src/a.ts", is_dir: false },
        { path: "README.md", is_dir: false },
      ],
      truncated: false,
    };
  });

  it("loads and renders the repo's files (changed files marked)", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    await waitFor(() => expect(listRepoTreeMock).toHaveBeenCalled());
    // README is at the root; the src folder is collapsed by default.
    const readme = await screen.findByTestId("tree-file-README.md");
    expect(readme).toBeInTheDocument();
    fireEvent.click(await screen.findByText("src")); // expand folder
    const aFile = await screen.findByTestId("tree-file-src/a.ts");
    expect(aFile).toHaveClass("tree-file--changed");
  });

  it("marks a collapsed folder that contains changed files", async () => {
    // delta() marks src/a.ts modified; the src folder is collapsed by default.
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    expect(await screen.findByTestId("tree-dir-changed-src")).toBeInTheDocument();
  });

  it("single click previews a file (pin=false), double click pins it (pin=true)", async () => {
    const openSpy = vi.spyOn(fileDock, "openFile").mockImplementation(() => {});
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const readme = await screen.findByTestId("tree-file-README.md");

    fireEvent.click(readme); // preview
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", false);

    fireEvent.doubleClick(readme); // pin
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", true);
    openSpy.mockRestore();
  });

  it("serves a cached tree without reloading (no spinner on re-open)", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    const first = render(<ProjectExplorer repo={REPO} />);
    await screen.findByTestId("tree-file-README.md");
    expect(listRepoTreeMock).toHaveBeenCalledTimes(1);
    first.unmount();

    render(<ProjectExplorer repo={REPO} />);
    // Cached: the file is there immediately and no second fetch fires.
    expect(screen.getByTestId("tree-file-README.md")).toBeInTheDocument();
    expect(listRepoTreeMock).toHaveBeenCalledTimes(1);
  });
});
