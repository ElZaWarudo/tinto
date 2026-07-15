import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

let tree: unknown = { entries: [], truncated: false };
const listRepoTreeMock = vi.fn(() => Promise.resolve(tree));
const deleteFromRepoMock = vi.fn<(_repo: string, _sources: string[]) => Promise<unknown>>(() =>
  Promise.resolve({ token: "11111111-1111-4111-8111-111111111111", entries: [] }),
);
const restoreDeletedFromRepoMock = vi.fn<(_repo: string, _token: string) => Promise<void>>(() =>
  Promise.resolve(),
);
const redoDeletedFromRepoMock = vi.fn<(_repo: string, _token: string) => Promise<void>>(() =>
  Promise.resolve(),
);
const copyToRepoMock = vi.fn<
  (_repo: string, _destDir: string, _sources: string[], _overwrite: boolean) => Promise<unknown>
>(() => Promise.resolve({ copied: [], conflicts: [] }));
const copyWithinRepoMock = vi.fn<
  (_repo: string, _sources: string[], _destDir: string, _overwrite: boolean) => Promise<unknown>
>(() => Promise.resolve({ copied: [], conflicts: [] }));
const moveWithinRepoMock = vi.fn<
  (_repo: string, _sources: string[], _destDir: string, _overwrite: boolean) => Promise<unknown>
>(() => Promise.resolve({ copied: [], conflicts: [] }));
const tauriDrag = vi.hoisted(() => ({
  handler: null as
    | ((event: {
        payload: {
          type: string;
          paths?: string[];
          position?: { x: number; y: number };
        };
      }) => void)
    | null,
  onDragDropEvent: vi.fn(),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: typeof tauriDrag.handler) => {
      tauriDrag.handler = handler;
      tauriDrag.onDragDropEvent(handler);
      return Promise.resolve(() => {});
    },
  }),
}));
vi.mock("../../bus/client", () => ({
  listRepoTree: () => listRepoTreeMock(),
  copyToRepo: (repo: string, destDir: string, sources: string[], overwrite: boolean) =>
    copyToRepoMock(repo, destDir, sources, overwrite),
  copyWithinRepo: (repo: string, sources: string[], destDir: string, overwrite: boolean) =>
    copyWithinRepoMock(repo, sources, destDir, overwrite),
  moveWithinRepo: (repo: string, sources: string[], destDir: string, overwrite: boolean) =>
    moveWithinRepoMock(repo, sources, destDir, overwrite),
  exportFromRepo: vi.fn(),
  deleteFromRepo: (repo: string, sources: string[]) => deleteFromRepoMock(repo, sources),
  restoreDeletedFromRepo: (repo: string, token: string) => restoreDeletedFromRepoMock(repo, token),
  redoDeletedFromRepo: (repo: string, token: string) => redoDeletedFromRepoMock(repo, token),
}));

import { ProjectExplorer } from "./ProjectExplorer";
import { busStore } from "../../bus/store";
import { qualityStore } from "../../qol/state";
import { fileDock } from "../../workspace/fileDock";
import { repoTreeStore } from "../../workspace/repoTreeStore";
import { deleteUndoManager } from "../file/deleteUndo";
import { treeClipboard } from "./treeClipboard";
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
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
    ...over,
  };
}

describe("ProjectExplorer", () => {
  beforeEach(() => {
    localStorage.clear();
    busStore.resetAll();
    qualityStore.reset();
    fileDock.drop(REPO);
    repoTreeStore.reset();
    deleteUndoManager.reset();
    listRepoTreeMock.mockClear();
    deleteFromRepoMock.mockClear();
    restoreDeletedFromRepoMock.mockClear();
    redoDeletedFromRepoMock.mockClear();
    copyToRepoMock.mockReset();
    copyToRepoMock.mockResolvedValue({ copied: [], conflicts: [] });
    copyWithinRepoMock.mockClear();
    moveWithinRepoMock.mockReset();
    moveWithinRepoMock.mockResolvedValue({ copied: [], conflicts: [] });
    tauriDrag.handler = null;
    tauriDrag.onDragDropEvent.mockClear();
    treeClipboard.clear();
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 1024 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 768 });
    Object.defineProperty(window, "PointerEvent", {
      configurable: true,
      value: MouseEvent,
    });
    Object.defineProperty(window, "devicePixelRatio", { configurable: true, value: 2 });
    Object.defineProperty(document, "elementFromPoint", {
      configurable: true,
      value: vi.fn(() => null),
    });
    Object.defineProperty(window, "confirm", {
      configurable: true,
      value: vi.fn(() => true),
    });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn(() => Promise.resolve()) },
    });
    tree = {
      entries: [
        { path: "src", is_dir: true },
        { path: "src/a.ts", is_dir: false },
        { path: "src/App.tsx", is_dir: false },
        { path: "src/App.css", is_dir: false },
        { path: ".env", is_dir: false },
        { path: "package.json", is_dir: false },
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
    expect(readme.querySelector(".tree-icon--markdown")).toBeInTheDocument();
    expect(
      (await screen.findByTestId("tree-file-package.json")).querySelector(".tree-icon--npm"),
    ).toBeInTheDocument();
    expect(
      (await screen.findByTestId("tree-file-.env")).querySelector(".tree-icon--env"),
    ).toBeInTheDocument();
    fireEvent.click(await screen.findByText("src")); // expand folder
    expect(
      screen.getByText("src").parentElement?.querySelector(".tree-icon--folder"),
    ).toBeInTheDocument();
    const aFile = await screen.findByTestId("tree-file-src/a.ts");
    expect(aFile).toHaveClass("tree-file--changed");
    expect(aFile.querySelector(".tree-icon--typescript")).toBeInTheDocument();
    expect(
      (await screen.findByTestId("tree-file-src/App.tsx")).querySelector(".tree-icon--react"),
    ).toBeInTheDocument();
    expect(
      (await screen.findByTestId("tree-file-src/App.css")).querySelector(".tree-icon--css"),
    ).toBeInTheDocument();
  });

  it("keeps the worktree collapse action inside the normalized active-tab header", async () => {
    const onToggleCollapse = vi.fn();
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} onToggleCollapse={onToggleCollapse} />);

    const title = await screen.findByText("api");
    expect(title).toHaveClass("project-explorer__title");
    const toggle = screen.getByTestId(`project-explorer-collapse-${REPO}`);
    fireEvent.click(toggle);
    expect(onToggleCollapse).toHaveBeenCalledOnce();
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

  it("restores expanded folders when the explorer remounts", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    const first = render(<ProjectExplorer repo={REPO} />);

    fireEvent.click(await screen.findByText("src"));
    expect(await screen.findByTestId("tree-file-src/a.ts")).toBeInTheDocument();
    expect(localStorage.getItem("tinto:explorer-expanded:/r/api")).toBe('["src"]');
    first.unmount();

    render(<ProjectExplorer repo={REPO} />);

    expect(await screen.findByTestId("tree-file-src/a.ts")).toBeInTheDocument();
  });

  it("keeps expanded folders in sync between mounted explorer views", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(
      <>
        <ProjectExplorer repo={REPO} />
        <ProjectExplorer repo={REPO} />
      </>,
    );

    fireEvent.click((await screen.findAllByText("src"))[0]);

    await waitFor(() => expect(screen.getAllByTestId("tree-file-src/a.ts")).toHaveLength(2));
  });

  it("resizes the file tree with the drag handle", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const explorer = await screen.findByTestId(`project-explorer-${REPO}`);
    const handle = screen.getByRole("separator", { name: "Redimensionar árbol de archivos" });

    fireEvent.pointerDown(handle, { clientX: 240 });
    fireEvent.pointerMove(window, { clientX: 340 });
    fireEvent.pointerUp(window);

    expect(explorer).toHaveStyle({ width: "340px" });
  });

  it("exposes an adjustable splitter and resizes it with the keyboard", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const explorer = await screen.findByTestId(`project-explorer-${REPO}`);
    const handle = screen.getByRole("separator", { name: "Redimensionar árbol de archivos" });
    const maxWidth = Math.round(Math.max(160, Math.min(520, window.innerWidth * 0.55)));

    expect(handle).toHaveAttribute("tabindex", "0");
    expect(handle).toHaveAttribute("aria-valuemin", "160");
    expect(handle).toHaveAttribute("aria-valuemax", String(maxWidth));
    expect(handle).toHaveAttribute("aria-valuenow", "240");

    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(explorer).toHaveStyle({ width: "256px" });
    expect(handle).toHaveAttribute("aria-valuenow", "256");

    fireEvent.keyDown(handle, { key: "ArrowRight", shiftKey: true });
    expect(explorer).toHaveStyle({ width: "304px" });
    fireEvent.keyDown(handle, { key: "Home" });
    expect(explorer).toHaveStyle({ width: "160px" });
    fireEvent.keyDown(handle, { key: "End" });
    expect(explorer).toHaveStyle({ width: `${maxWidth}px` });
    expect(handle).toHaveAttribute("aria-valuetext", `${maxWidth} píxeles`);
  });

  it("uses tree semantics, roving tabindex, and hierarchical arrow navigation", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const treeWidget = await screen.findByRole("tree", { name: "Archivos de api" });
    const src = screen.getByRole("treeitem", { name: "src" });
    expect(treeWidget).toContainElement(src);
    expect(src).toHaveAttribute("aria-level", "1");
    expect(src).toHaveAttribute("aria-expanded", "false");
    expect(treeWidget.querySelectorAll('[role="treeitem"][tabindex="0"]')).toHaveLength(1);

    src.focus();
    fireEvent.keyDown(src, { key: "ArrowRight" });
    expect(src).toHaveAttribute("aria-expanded", "true");

    fireEvent.keyDown(src, { key: "ArrowRight" });
    const focusedChild = document.activeElement as HTMLElement;
    expect(focusedChild).toHaveAttribute("role", "treeitem");
    expect(focusedChild).toHaveAttribute("aria-level", "2");
    expect(treeWidget.querySelectorAll('[role="treeitem"][tabindex="0"]')).toHaveLength(1);

    fireEvent.keyDown(focusedChild, { key: "ArrowLeft" });
    expect(src).toHaveFocus();
    fireEvent.keyDown(src, { key: "End" });
    expect(screen.getByRole("treeitem", { name: "README.md" })).toHaveFocus();
    fireEvent.keyDown(document.activeElement as HTMLElement, { key: "Home" });
    expect(src).toHaveFocus();
  });

  it("pastes the internal clipboard into the focused subfolder", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const src = await screen.findByRole("treeitem", { name: "src" });
    treeClipboard.copy(REPO, ["README.md"]);

    src.focus();
    fireEvent.keyDown(src, { key: "v", ctrlKey: true });

    await waitFor(() =>
      expect(copyWithinRepoMock).toHaveBeenCalledWith(REPO, ["README.md"], "src", false),
    );
  });

  it("blocks an internal clipboard paste from a different repository", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const src = await screen.findByRole("treeitem", { name: "src" });
    treeClipboard.cut("/r/other", ["README.md"]);

    src.focus();
    fireEvent.keyDown(src, { key: "v", ctrlKey: true });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se puede pegar entre repositorios todavía",
    );
    expect(copyWithinRepoMock).not.toHaveBeenCalled();
    expect(moveWithinRepoMock).not.toHaveBeenCalled();
    expect(treeClipboard.get()?.repo).toBe("/r/other");
  });

  it("clears a cut clipboard after the initial move succeeds", async () => {
    moveWithinRepoMock.mockResolvedValueOnce({ copied: ["src/README.md"], conflicts: [] });
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const src = await screen.findByRole("treeitem", { name: "src" });
    treeClipboard.cut(REPO, ["README.md"]);

    src.focus();
    fireEvent.keyDown(src, { key: "v", ctrlKey: true });

    await waitFor(() =>
      expect(moveWithinRepoMock).toHaveBeenCalledWith(REPO, ["README.md"], "src", false),
    );
    await waitFor(() => expect(treeClipboard.get()).toBeNull());
  });

  it("surfaces unresolved paste conflicts and keeps the cut clipboard", async () => {
    moveWithinRepoMock.mockResolvedValueOnce({
      copied: [],
      conflicts: [{ dest_rel: "README.md", kind: "source_missing" }],
    });
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const src = await screen.findByRole("treeitem", { name: "src" });
    treeClipboard.cut(REPO, ["README.md"]);

    src.focus();
    fireEvent.keyDown(src, { key: "v", ctrlKey: true });

    expect(await screen.findByRole("alert")).toHaveTextContent("No se encuentra README.md");
    expect(treeClipboard.get()).toEqual({ repo: REPO, paths: ["README.md"], mode: "cut" });
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
    fireEvent.click(screen.getByText("Abrir vista previa"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", false);

    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Abrir fijo"));
    expect(openSpy).toHaveBeenLastCalledWith(REPO, "README.md", true);
    openSpy.mockRestore();
  });

  it("moves focus into the context menu, navigates it, and restores the tree item on Escape", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const readme = await screen.findByRole("treeitem", { name: "README.md" });
    readme.focus();
    fireEvent.keyDown(readme, { key: "F10", shiftKey: true });

    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: "Abrir vista previa" })).toHaveFocus(),
    );
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Abrir vista previa" }), {
      key: "ArrowDown",
    });
    expect(screen.getByRole("menuitem", { name: "Abrir fijo" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Abrir fijo" }), { key: "End" });
    expect(screen.getByRole("menuitem", { name: "Eliminar" })).toHaveFocus();

    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Eliminar" }), { key: "Escape" });
    expect(screen.queryByTestId("tree-context-menu")).not.toBeInTheDocument();
    expect(readme).toHaveFocus();
  });

  it("deletes a file from the context menu after confirmation", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const readme = await screen.findByTestId("tree-file-README.md");
    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Eliminar"));

    await waitFor(() => expect(deleteFromRepoMock).toHaveBeenCalledWith(REPO, ["README.md"]));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('archivo "README.md"'));
    await waitFor(() => expect(listRepoTreeMock).toHaveBeenCalledTimes(2));
  });

  it("restores and redoes a deleted file with Ctrl+Z and Ctrl+Shift+Z", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const explorer = await screen.findByTestId(`project-explorer-${REPO}`);
    const readme = await screen.findByTestId("tree-file-README.md");
    fireEvent.contextMenu(readme, { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Eliminar"));
    await waitFor(() => expect(deleteFromRepoMock).toHaveBeenCalledWith(REPO, ["README.md"]));

    fireEvent.keyDown(explorer, { key: "z", ctrlKey: true });
    await waitFor(() =>
      expect(restoreDeletedFromRepoMock).toHaveBeenCalledWith(
        REPO,
        "11111111-1111-4111-8111-111111111111",
      ),
    );

    fireEvent.keyDown(explorer, { key: "z", ctrlKey: true, shiftKey: true });
    await waitFor(() =>
      expect(redoDeletedFromRepoMock).toHaveBeenCalledWith(
        REPO,
        "11111111-1111-4111-8111-111111111111",
      ),
    );
  });

  it("deletes the focused file with the Delete key", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    const readme = await screen.findByTestId("tree-file-README.md");
    fireEvent.keyDown(readme, { key: "Delete" });

    await waitFor(() => expect(deleteFromRepoMock).toHaveBeenCalledWith(REPO, ["README.md"]));
  });

  it("deletes a folder from the context menu after confirmation", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);

    fireEvent.contextMenu(await screen.findByText("src"), { clientX: 20, clientY: 30 });
    fireEvent.click(screen.getByText("Eliminar"));

    await waitFor(() => expect(deleteFromRepoMock).toHaveBeenCalledWith(REPO, ["src"]));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('carpeta "src"'));
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

  it("retries an OS drop with overwrite enabled after confirmation", async () => {
    copyToRepoMock
      .mockResolvedValueOnce({
        copied: [],
        conflicts: [{ dest_rel: "README.md", kind: "file_exists" }],
      })
      .mockResolvedValueOnce({ copied: ["README.md"], conflicts: [] });
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    await screen.findByTestId(`project-explorer-${REPO}`);
    await waitFor(() => expect(tauriDrag.handler).not.toBeNull());

    act(() => {
      tauriDrag.handler?.({ payload: { type: "drop", paths: ["C:/tmp/README.md"] } });
    });
    fireEvent.click(await screen.findByTestId("overwrite-confirm-ok"));

    await waitFor(() => expect(copyToRepoMock).toHaveBeenCalledTimes(2));
    expect(copyToRepoMock).toHaveBeenNthCalledWith(1, REPO, "", ["C:/tmp/README.md"], false);
    expect(copyToRepoMock).toHaveBeenNthCalledWith(2, REPO, "", ["C:/tmp/README.md"], true);
    await waitFor(() =>
      expect(screen.queryByTestId("overwrite-confirm-modal")).not.toBeInTheDocument(),
    );
  });

  it("drops OS files into the folder under the physical pointer and refreshes immediately", async () => {
    copyToRepoMock.mockImplementationOnce(async () => {
      const current = tree as { entries: Array<{ path: string; is_dir: boolean }> };
      tree = {
        ...current,
        entries: [...current.entries, { path: "src/dropped.txt", is_dir: false }],
      };
      return { copied: ["src/dropped.txt"], conflicts: [] };
    });
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const src = await screen.findByRole("treeitem", { name: "src" });
    const elementFromPoint = vi.mocked(document.elementFromPoint);
    elementFromPoint.mockReturnValue(src.querySelector(".tree-dir__row"));
    await waitFor(() => expect(tauriDrag.handler).not.toBeNull());

    act(() => {
      tauriDrag.handler?.({
        payload: { type: "enter", paths: ["C:/tmp/dropped.txt"], position: { x: 80, y: 40 } },
      });
    });
    expect(src.querySelector(".tree-dir__row")).toHaveClass("tree-dir__row--drop-target");

    act(() => {
      tauriDrag.handler?.({
        payload: { type: "drop", paths: ["C:/tmp/dropped.txt"], position: { x: 80, y: 40 } },
      });
    });

    await waitFor(() =>
      expect(copyToRepoMock).toHaveBeenCalledWith(REPO, "src", ["C:/tmp/dropped.txt"], false),
    );
    expect(elementFromPoint).toHaveBeenCalledWith(40, 20);
    await waitFor(() => expect(listRepoTreeMock).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(repoTreeStore.get(REPO).tree?.entries).toContainEqual({
        path: "src/dropped.txt",
        is_dir: false,
      }),
    );
    const refreshedSrc = screen.getByRole("treeitem", { name: "src" });
    fireEvent.click(refreshedSrc.querySelector(".tree-dir__row")!);
    expect(await screen.findByTestId("tree-file-src/dropped.txt")).toBeInTheDocument();
  });

  it("moves an existing file into a folder through pointer drag without HTML5", async () => {
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const readme = await screen.findByRole("treeitem", { name: "README.md" });
    const src = screen.getByRole("treeitem", { name: "src" });
    vi.mocked(document.elementFromPoint).mockReturnValue(src.querySelector(".tree-dir__row"));

    expect(readme).not.toHaveAttribute("draggable");
    fireEvent.pointerDown(readme, {
      button: 0,
      pointerId: 7,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerMove(document, {
      pointerId: 7,
      clientX: 30,
      clientY: 30,
    });
    await waitFor(() => expect(readme).toHaveClass("tree-file--dragging"));
    await waitFor(() =>
      expect(src.querySelector(".tree-dir__row")).toHaveClass("tree-dir__row--drop-target"),
    );
    fireEvent.pointerUp(document, {
      pointerId: 7,
      clientX: 30,
      clientY: 30,
    });

    await waitFor(() =>
      expect(moveWithinRepoMock).toHaveBeenCalledWith(REPO, ["README.md"], "src", false),
    );
  });

  it("keeps a short pointer gesture as a normal file click", async () => {
    const openSpy = vi.spyOn(fileDock, "openFile").mockImplementation(() => {});
    act(() => busStore.loadSnapshot([delta()], { available: true }));
    render(<ProjectExplorer repo={REPO} />);
    const readme = await screen.findByRole("treeitem", { name: "README.md" });

    fireEvent.pointerDown(readme, {
      button: 0,
      pointerId: 8,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerMove(document, {
      pointerId: 8,
      clientX: 12,
      clientY: 12,
    });
    fireEvent.pointerUp(document, {
      pointerId: 8,
      clientX: 12,
      clientY: 12,
    });
    fireEvent.click(readme);

    expect(moveWithinRepoMock).not.toHaveBeenCalled();
    expect(openSpy).toHaveBeenCalledWith(REPO, "README.md", false);
    openSpy.mockRestore();
  });
});
