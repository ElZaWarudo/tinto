import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

let tree: unknown = { entries: [], truncated: false };
const listRepoTreeMock = vi.fn(() => Promise.resolve(tree));
vi.mock("../../bus/client", () => ({
  listRepoTree: () => listRepoTreeMock(),
}));

import { ProjectExplorer } from "./ProjectExplorer";
import { busStore } from "../../bus/store";
import { qualityStore } from "../../qol/state";
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
    qualityStore.reset();
    fileDock.drop(REPO);
    repoTreeStore.reset();
    listRepoTreeMock.mockClear();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn(() => Promise.resolve()) },
    });
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

  it("opens the file context menu actions", async () => {
    const openSpy = vi.spyOn(fileDock, "openFile").mockImplementation(() => {});
    act(() =>
      busStore.loadSnapshot(
        [
          delta({
            signals: [
              {
                kind: "config_change",
                severity: "warning",
                path: "README.md",
                message: "Documentation changed",
              },
            ],
          }),
        ],
        { available: true },
      ),
    );
    render(<ProjectExplorer repo={REPO} />);

    const readme = await screen.findByTestId("tree-file-README.md");
    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    expect(screen.getByTestId("tree-context-menu")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Mostrar señales"));
    expect(screen.getByTestId("tree-context-signals")).toHaveTextContent("Documentation changed");
    fireEvent.click(screen.getByText("Copiar nombre"));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("README.md");

    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Abrir preview"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", false);

    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Abrir fijo"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", true);
    openSpy.mockRestore();
  });

  it("enables diff for changed files and opens folder changed files", async () => {
    const openSpy = vi.spyOn(fileDock, "openFile").mockImplementation(() => {});
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    fireEvent.click(await screen.findByText("src"));
    const aFile = await screen.findByTestId("tree-file-src/a.ts");
    fireEvent.contextMenu(aFile, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Abrir diff"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "src/a.ts", false);

    fireEvent.contextMenu(await screen.findByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Abrir archivos cambiados"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "src/a.ts", true);
    openSpy.mockRestore();
  });

  it("expands, collapses, copies, and filters from the folder context menu", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    fireEvent.contextMenu(await screen.findByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Expandir todo dentro"));
    expect(await screen.findByTestId("tree-file-src/a.ts")).toBeInTheDocument();

    fireEvent.contextMenu(screen.getByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Copiar ruta absoluta"));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(`${REPO}/src`);

    fireEvent.contextMenu(screen.getByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Contraer todo dentro"));
    expect(screen.queryByTestId("tree-file-src/a.ts")).not.toBeInTheDocument();

    fireEvent.contextMenu(screen.getByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Filtrar por esta carpeta"));
    expect(screen.queryByTestId("tree-file-README.md")).not.toBeInTheDocument();
  });

  it("keeps the context menu inside the viewport near the bottom edge", async () => {
    const rectSpy = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 220,
      bottom: 180,
      width: 220,
      height: 180,
      toJSON: () => ({}),
    });
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 800 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 600 });

    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    fireEvent.contextMenu(await screen.findByTestId("tree-file-README.md"), {
      clientX: 790,
      clientY: 590,
    });

    await waitFor(() => {
      const menu = screen.getByTestId("tree-context-menu");
      expect(menu).toHaveStyle({ left: "572px", top: "412px" });
    });
    rectSpy.mockRestore();
  });
});
