// In-project file explorer: the left pane of a project (level-1) tab. Shows the
// repo's own files (always loaded via repoTreeStore — cached + preloaded, so no
// spinner on re-open), with the quality filters applied and the active file
// highlighted. Single click previews a file, double click pins it (VS Code).

import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
} from "react";
import type { PassiveSignal } from "../../bus/contract";
import { busStore, getRepoSignals, useBusState } from "../../bus/store";
import { filterTreeNodes, hasActiveFilters } from "../../qol/filters";
import { qualityStore, useQualityState } from "../../qol/state";
import { repoTreeStore, useRepoTree } from "../../workspace/repoTreeStore";
import { fileDock, useRepoDock } from "../../workspace/fileDock";
import { buildFileTree, type TreeNode } from "./fileTree";
import { FileTreeNode } from "./FileTreeNode";
import { useExplorerExpanded } from "./explorerCollapseState";
import { treeClipboard } from "./treeClipboard";
import { OverwriteConfirmModal } from "../file/OverwriteConfirmModal";
import {
  sendFromOs,
  sendWithinRepo,
  deleteWithinRepo,
  needsConfirmation,
  type FileOpReport,
} from "../file/fileOps";
import { deleteUndoManager } from "../file/deleteUndo";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface ContextMenuState {
  node: TreeNode;
  x: number;
  y: number;
  showSignals: boolean;
}

const MENU_VIEWPORT_MARGIN = 8;
const EXPLORER_MIN_WIDTH = 160;
const EXPLORER_DEFAULT_WIDTH = 240;
const EXPLORER_MAX_WIDTH = 520;

function explorerWidthStorageKey(repo: string): string {
  return `tinto:explorer-width:${repo}`;
}

function clampExplorerWidth(width: number): number {
  const viewportMax = typeof window === "undefined" ? EXPLORER_MAX_WIDTH : window.innerWidth * 0.55;
  return Math.round(
    Math.min(Math.max(width, EXPLORER_MIN_WIDTH), Math.min(EXPLORER_MAX_WIDTH, viewportMax)),
  );
}

function loadExplorerWidth(repo: string): number {
  try {
    const stored = localStorage.getItem(explorerWidthStorageKey(repo));
    return stored ? clampExplorerWidth(Number(stored)) : EXPLORER_DEFAULT_WIDTH;
  } catch {
    return EXPLORER_DEFAULT_WIDTH;
  }
}

function absolutePath(repo: string, path: string): string {
  return `${repo.replace(/[\\/]+$/, "")}/${path}`;
}

function copyText(value: string): void {
  void navigator.clipboard?.writeText(value).catch(() => {
    /* clipboard unavailable */
  });
}

function descendantDirs(node: TreeNode): string[] {
  if (!node.isDir) return [];
  return [node.path, ...node.children.flatMap(descendantDirs)];
}

function changedFiles(node: TreeNode): string[] {
  if (!node.isDir) return node.changed ? [node.path] : [];
  return node.children.flatMap(changedFiles);
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return !!target.closest("input, textarea, select, [contenteditable='true']");
}

function signalMatches(repoSignals: PassiveSignal[], node: TreeNode): PassiveSignal[] {
  if (!node.isDir) return repoSignals.filter((signal) => signal.path === node.path);
  const prefix = `${node.path}/`;
  return repoSignals.filter(
    (signal) => signal.path === node.path || signal.path?.startsWith(prefix),
  );
}

export function ProjectExplorer({
  repo,
  collapsed = false,
  onToggleCollapse,
}: {
  repo: string;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
}) {
  const state = useBusState();
  const { filters } = useQualityState();
  const { tree, loading, error } = useRepoTree(repo);
  const { active } = useRepoDock(repo);
  const delta = state.repos[repo];
  const [expandedDirs, setExpandedDirs] = useExplorerExpanded(repo);
  const [menu, setMenu] = useState<ContextMenuState | null>(null);
  // Drag/drop + paste state
  const [draggingNode, setDraggingNode] = useState<TreeNode | null>(null);
  const [osDraggingOver, setOsDraggingOver] = useState(false);
  const [treeDropTarget, setTreeDropTarget] = useState<string | null>(null);
  const [explorerWidth, setExplorerWidth] = useState(() => loadExplorerWidth(repo));
  const [pendingOp, setPendingOp] = useState<{
    retry: () => Promise<void> | void;
    report: FileOpReport;
  } | null>(null);
  const [osDraggedFiles, setOsDraggedFiles] = useState<string[] | null>(null);
  const [fileOpError, setFileOpError] = useState<string | null>(null);
  const explorerRef = useRef<HTMLDivElement | null>(null);
  // Refs para mantener referencias actualizadas en closures de useEffect
  const handleOsDropRef = useRef<((paths: string[], destDir: string) => Promise<void>) | null>(
    null,
  );
  const handleTreeDropRef = useRef<((targetPath: string) => Promise<void>) | null>(null);
  const handlePasteRef = useRef<((destDir: string) => Promise<void>) | null>(null);

  // Load on mount; the store keeps it cached (stale-while-revalidate) thereafter.
  useEffect(() => {
    repoTreeStore.ensureLoaded(repo);
  }, [repo]);

  useEffect(() => {
    const width = loadExplorerWidth(repo);
    setExplorerWidth(width);
  }, [repo]);

  useEffect(() => {
    try {
      localStorage.setItem(explorerWidthStorageKey(repo), String(explorerWidth));
    } catch {
      /* ignore storage unavailable */
    }
  }, [repo, explorerWidth]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  // OS-level drag/drop: listen to Tauri's native `onDragDropEvent`.
  useEffect(() => {
    if (collapsed) return;
    let active = true;
    let unlisten: UnlistenFn | undefined;
    // En entornos sin runtime Tauri (tests jsdom), getCurrentWebview lanza.
    // Guardamos el accceso con try/catch para no romper el mount.
    let webview: ReturnType<typeof getCurrentWebview> | null = null;
    try {
      webview = getCurrentWebview();
    } catch {
      webview = null;
    }
    if (!webview) return;
    webview
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setOsDraggingOver(true);
          if (payload.type === "enter") {
            setOsDraggedFiles(payload.paths);
          }
        } else if (payload.type === "drop") {
          setOsDraggingOver(false);
          const paths = payload.paths;
          setOsDraggedFiles(paths);
          handleOsDropRef.current?.(paths, "");
        } else if (payload.type === "leave") {
          setOsDraggingOver(false);
          setOsDraggedFiles(null);
        }
      })
      .then((un) => {
        if (!active) {
          void un();
          return;
        }
        unlisten = un;
      })
      .catch(() => {
        // ignore: webview event unavailable (older Tauri build)
      });
    return () => {
      active = false;
      if (unlisten) unlisten();
    };
  }, [collapsed, repo]);

  const nodes = useMemo(() => {
    if (!tree || !delta) return [];
    const signals = getRepoSignals(delta);
    return filterTreeNodes(buildFileTree(tree.entries, delta.status), filters, signals);
  }, [tree, delta, filters]);
  const repoSignals = delta ? getRepoSignals(delta) : [];

  const toggleDir = (path: string) => {
    setExpandedDirs((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const expandAll = (node: TreeNode) => {
    setExpandedDirs((current) => {
      const next = new Set(current);
      descendantDirs(node).forEach((path) => next.add(path));
      return next;
    });
  };

  const collapseAll = (node: TreeNode) => {
    setExpandedDirs((current) => {
      const next = new Set(current);
      descendantDirs(node).forEach((path) => next.delete(path));
      return next;
    });
  };

  const openContextMenu = (event: MouseEvent, node: TreeNode) => {
    event.preventDefault();
    event.stopPropagation();
    setMenu({ node, x: event.clientX, y: event.clientY, showSignals: false });
  };

  const closeMenu = () => setMenu(null);
  const runMenuAction = (action: () => void) => {
    action();
    closeMenu();
  };

  const startResize = (event: PointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const measuredWidth = explorerRef.current?.getBoundingClientRect().width ?? 0;
    const startWidth = measuredWidth > 0 ? measuredWidth : explorerWidth;

    const onMove = (moveEvent: globalThis.PointerEvent) => {
      setExplorerWidth(clampExplorerWidth(startWidth + moveEvent.clientX - startX));
    };
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
  };

  /** Refresca el árbol para reflejar los cambios recién escritos. */
  const refreshTree = () => repoTreeStore.refresh(repo);

  /** Procesa el reporte de una operación: si hay conflictos, abre el modal
   * para confirmar overwrite; si hay error fatal lo muestra; si todo OK,
   * refresca el árbol. */
  const processReport = (report: FileOpReport, retry: () => Promise<void> | void) => {
    if (report.fatalError) {
      setFileOpError(report.fatalError);
      return;
    }
    if (needsConfirmation(report)) {
      setPendingOp({ retry, report });
      return;
    }
    refreshTree();
  };

  /**
   * Copia archivos del OS a una carpeta del repo. `destDir` es path relativo
   * al repo ("" = raíz).
   */
  const handleOsDrop = async (paths: string[], destDir: string) => {
    if (!paths.length) return;
    const report = await sendFromOs({
      repo,
      destDir,
      sources: paths,
      strategy: "copy",
      overwrite: false,
    });
    processReport(report, () => handleOsDrop(paths, destDir));
  };

  /**
   * Copia (drag con Ctrl) o mueve (drag sin Ctrl) archivos dentro del repo
   * desde el nodo que se está arrastrando a `targetPath` (carpeta destino).
   */
  const handleTreeDrop = async (targetPath: string) => {
    if (!draggingNode) return;
    if (targetPath === draggingNode.path || targetPath.startsWith(`${draggingNode.path}/`)) {
      // No mover un directorio dentro de sí mismo.
      setDraggingNode(null);
      setTreeDropTarget(null);
      return;
    }
    const report = await sendWithinRepo({
      repo,
      sources: [draggingNode.path],
      destDir: targetPath,
      strategy: "move",
      overwrite: false,
    });
    setDraggingNode(null);
    setTreeDropTarget(null);
    processReport(report, () => {
      void sendWithinRepo({
        repo,
        sources: [draggingNode.path],
        destDir: targetPath,
        strategy: "move",
        overwrite: true,
      }).then((r) => processReport(r, () => Promise.resolve()));
    });
  };

  /** Pega archivos del clipboard interno (Ctrl+C en cualquier nodo) a una
   * carpeta destino. Soporta copia y corte. */
  const handlePaste = async (destDir: string) => {
    const clip = treeClipboard.get();
    if (!clip) return;
    const report = await sendWithinRepo({
      repo,
      sources: clip.paths,
      destDir,
      strategy: clip.mode === "cut" ? "move" : "copy",
      overwrite: false,
    });
    processReport(report, () => {
      void sendWithinRepo({
        repo,
        sources: clip.paths,
        destDir,
        strategy: clip.mode === "cut" ? "move" : "copy",
        overwrite: true,
      }).then((r) => {
        processReport(r, () => Promise.resolve());
        if (clip.mode === "cut") treeClipboard.clear();
      });
    });
  };

  const handleDelete = async (node: TreeNode) => {
    const label = node.isDir ? "carpeta" : "archivo";
    if (
      !window.confirm(
        `Eliminar ${label} "${node.path}" del disco?\n\nPuedes restaurarlo con Ctrl+Z mientras Tinto siga abierto.`,
      )
    ) {
      return;
    }
    const report = await deleteWithinRepo({ repo, sources: [node.path] });
    if (report.fatalError) {
      setFileOpError(report.fatalError);
      return;
    }
    if (report.deleteResult) {
      deleteUndoManager.recordDelete(repo, report.deleteResult);
    }
    refreshTree();
  };

  const undoDelete = async () => {
    const report = await deleteUndoManager.undo();
    if (report?.fatalError) setFileOpError(report.fatalError);
  };

  const redoDelete = async () => {
    const report = await deleteUndoManager.redo();
    if (report?.fatalError) setFileOpError(report.fatalError);
  };

  const deleteActiveFile = () => {
    if (!active) return;
    void handleDelete({
      path: active,
      name: active.split("/").pop() ?? active,
      isDir: false,
      children: [],
      changed: null,
      hasChanges: false,
    });
  };

  // Sincronizar refs con las funciones para evitar stale closures en useEffect.
  useEffect(() => {
    handleOsDropRef.current = handleOsDrop;
  }, [handleOsDrop]);
  useEffect(() => {
    handleTreeDropRef.current = handleTreeDrop;
  }, [handleTreeDrop]);
  useEffect(() => {
    handlePasteRef.current = handlePaste;
  }, [handlePaste]);

  const confirmOverwrite = async () => {
    if (!pendingOp) return;
    setPendingOp(null);
    await pendingOp.retry();
  };
  const cancelOverwrite = () => setPendingOp(null);

  if (collapsed) {
    return (
      <div
        className="project-explorer project-explorer--collapsed"
        data-testid={`project-explorer-${repo}`}
      >
        <button
          className="project-explorer__toggle project-explorer__toggle--rail"
          type="button"
          title="Show file tree"
          aria-label="Show file tree"
          data-testid={`project-explorer-expand-${repo}`}
          onClick={onToggleCollapse}
        >
          &rsaquo;
        </button>
      </div>
    );
  }

  return (
    <div
      ref={explorerRef}
      className="project-explorer"
      data-testid={`project-explorer-${repo}`}
      style={{ width: explorerWidth }}
      onKeyDown={(event) => {
        if (event.key === "Delete") {
          if (
            isEditableTarget(event.target) ||
            (event.target as HTMLElement | null)?.closest(".tree-menu")
          ) {
            return;
          }
          if (active) {
            event.preventDefault();
            deleteActiveFile();
          }
          return;
        }
        const ctrl = event.ctrlKey || event.metaKey;
        if (!ctrl) return;
        const key = event.key.toLowerCase();
        if (key === "z" && event.shiftKey) {
          event.preventDefault();
          void redoDelete();
        } else if (key === "z") {
          event.preventDefault();
          void undoDelete();
        } else if (key === "c" && active) {
          event.preventDefault();
          treeClipboard.copy(repo, [active]);
        } else if (key === "x" && active) {
          event.preventDefault();
          treeClipboard.cut(repo, [active]);
        } else if (key === "v") {
          event.preventDefault();
          // Pegar al raíz del repositorio.
          void handlePaste("");
        }
      }}
      onDragOver={(event) => {
        // Solo previene default si NO viene del OS (el OS se maneja via
        // onDragDropEvent). Para HTML5 drag dentro del árbol, es necesario
        // para que onDrop dispare.
        if (draggingNode) event.preventDefault();
      }}
      onDrop={(event) => {
        if (draggingNode) {
          event.preventDefault();
          // Drop al raíz del repo (sin target folder específico).
          void handleTreeDrop("");
        }
      }}
      tabIndex={0}
    >
      <div className="project-explorer__head">
        <span className="project-explorer__title">{busStore.displayName(repo)}</span>
        {onToggleCollapse && (
          <button
            className="project-explorer__toggle"
            type="button"
            title="Hide file tree"
            aria-label="Hide file tree"
            data-testid={`project-explorer-collapse-${repo}`}
            onClick={onToggleCollapse}
          >
            &lsaquo;
          </button>
        )}
      </div>
      <div
        className={`project-explorer__body${osDraggingOver ? " project-explorer--dragging-active" : ""}`}
        data-testid={`project-explorer-body-${repo}`}
      >
        {error && !tree ? (
          <div className="tree-files__msg">Could not load files.</div>
        ) : !tree && loading ? (
          <div className="tree-files__msg" data-testid="explorer-loading">
            Loading…
          </div>
        ) : nodes.length === 0 && hasActiveFilters(filters) ? (
          <div className="tree-files__msg" data-testid="explorer-no-matches">
            No files match the current filters.
          </div>
        ) : nodes.length === 0 ? (
          <div className="tree-files__msg">No files.</div>
        ) : (
          <>
            {delta &&
              nodes.map((n) => (
                <FileTreeNode
                  key={n.path}
                  node={n}
                  delta={delta}
                  depth={0}
                  activePath={active}
                  expandedDirs={expandedDirs}
                  onToggleDir={toggleDir}
                  onOpen={(path, pin) => fileDock.openFile(repo, path, pin)}
                  onContextMenu={openContextMenu}
                  onTreeDragStart={(node) => setDraggingNode(node)}
                  onTreeDragEnd={() => {
                    setDraggingNode(null);
                    setTreeDropTarget(null);
                  }}
                  onTreeDrop={(targetPath) => void handleTreeDrop(targetPath)}
                  dropTargetPath={treeDropTarget}
                  onPasteInto={(destDir) => void handlePaste(destDir)}
                  onDelete={(node) => void handleDelete(node)}
                />
              ))}
            {tree?.truncated && (
              <div className="tree-files__msg" data-testid="explorer-truncated">
                Tree truncated (too many files).
              </div>
            )}
          </>
        )}
        {osDraggingOver && (
          <div
            className="project-explorer__drop-overlay"
            data-testid={`project-explorer-drop-overlay-${repo}`}
          >
            <span className="project-explorer__drop-text">
              Soltar para copiar al repo
              {osDraggedFiles && osDraggedFiles.length > 0
                ? ` (${osDraggedFiles.length} archivos)`
                : ""}
            </span>
          </div>
        )}
      </div>
      {fileOpError && (
        <div
          className="tree-files__msg tree-files__msg--error"
          role="alert"
          data-testid="file-op-error"
        >
          {fileOpError}
          <button
            type="button"
            className="tree-files__msg-close"
            onClick={() => setFileOpError(null)}
            aria-label="Cerrar"
          >
            ×
          </button>
        </div>
      )}
      {pendingOp && (
        <OverwriteConfirmModal
          report={pendingOp.report}
          onConfirm={() => void confirmOverwrite()}
          onCancel={cancelOverwrite}
        />
      )}
      {menu && delta && (
        <TreeContextMenu
          repo={repo}
          menu={menu}
          expanded={expandedDirs.has(menu.node.path)}
          signals={signalMatches(repoSignals, menu.node)}
          changedFiles={changedFiles(menu.node)}
          onMouseDown={(event) => event.stopPropagation()}
          onPreview={() => runMenuAction(() => fileDock.openFile(repo, menu.node.path, false))}
          onPin={() => runMenuAction(() => fileDock.openFile(repo, menu.node.path, true))}
          onOpenDiff={() => runMenuAction(() => fileDock.openFile(repo, menu.node.path, false))}
          onToggleDir={() => runMenuAction(() => toggleDir(menu.node.path))}
          onExpandAll={() => runMenuAction(() => expandAll(menu.node))}
          onCollapseAll={() => runMenuAction(() => collapseAll(menu.node))}
          onCopyRelative={() => runMenuAction(() => copyText(menu.node.path))}
          onCopyAbsolute={() => runMenuAction(() => copyText(absolutePath(repo, menu.node.path)))}
          onCopyName={() => runMenuAction(() => copyText(menu.node.name))}
          onShowSignals={() => setMenu({ ...menu, showSignals: true })}
          onDelete={() => runMenuAction(() => void handleDelete(menu.node))}
          onOpenChanged={() =>
            runMenuAction(() =>
              changedFiles(menu.node).forEach((path) => fileDock.openFile(repo, path, true)),
            )
          }
          onFilterFolder={() =>
            runMenuAction(() => qualityStore.setFilters({ repo, search: menu.node.path }))
          }
        />
      )}
      <div
        className="project-explorer__resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Redimensionar árbol de archivos"
        title="Redimensionar árbol de archivos"
        onPointerDown={startResize}
      />
    </div>
  );
}

function TreeContextMenu({
  repo,
  menu,
  expanded,
  signals,
  changedFiles,
  onMouseDown,
  onPreview,
  onPin,
  onOpenDiff,
  onToggleDir,
  onExpandAll,
  onCollapseAll,
  onCopyRelative,
  onCopyAbsolute,
  onCopyName,
  onShowSignals,
  onDelete,
  onOpenChanged,
  onFilterFolder,
}: {
  repo: string;
  menu: ContextMenuState;
  expanded: boolean;
  signals: PassiveSignal[];
  changedFiles: string[];
  onMouseDown: (event: MouseEvent) => void;
  onPreview: () => void;
  onPin: () => void;
  onOpenDiff: () => void;
  onToggleDir: () => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  onCopyRelative: () => void;
  onCopyAbsolute: () => void;
  onCopyName: () => void;
  onShowSignals: () => void;
  onDelete: () => void;
  onOpenChanged: () => void;
  onFilterFolder: () => void;
}) {
  const { node } = menu;
  const isChangedFile = !node.isDir && node.changed !== null;
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [position, setPosition] = useState(() => ({ left: menu.x, top: menu.y }));

  useLayoutEffect(() => {
    const rect = menuRef.current?.getBoundingClientRect();
    if (!rect) return;
    const maxLeft = window.innerWidth - rect.width - MENU_VIEWPORT_MARGIN;
    const maxTop = window.innerHeight - rect.height - MENU_VIEWPORT_MARGIN;
    setPosition({
      left: Math.max(MENU_VIEWPORT_MARGIN, Math.min(menu.x, maxLeft)),
      top: Math.max(MENU_VIEWPORT_MARGIN, Math.min(menu.y, maxTop)),
    });
  }, [menu.x, menu.y, menu.showSignals, node.path, signals.length, changedFiles.length]);

  return (
    <div
      ref={menuRef}
      className="tree-menu"
      role="menu"
      data-testid="tree-context-menu"
      style={{ left: position.left, top: position.top }}
      onMouseDown={onMouseDown}
    >
      <div className="tree-menu__title" title={`${repo} / ${node.path}`}>
        {node.path}
      </div>

      {node.isDir ? (
        <>
          <MenuButton onClick={onToggleDir}>{expanded ? "Contraer" : "Expandir"}</MenuButton>
          <MenuButton onClick={onExpandAll}>Expandir todo dentro</MenuButton>
          <MenuButton onClick={onCollapseAll}>Contraer todo dentro</MenuButton>
          <div className="tree-menu__sep" />
          <MenuButton onClick={onOpenChanged} disabled={changedFiles.length === 0}>
            Abrir archivos cambiados
          </MenuButton>
          <MenuButton onClick={onFilterFolder}>Filtrar por esta carpeta</MenuButton>
        </>
      ) : (
        <>
          <MenuButton onClick={onPreview}>Abrir preview</MenuButton>
          <MenuButton onClick={onPin}>Abrir fijo</MenuButton>
          <MenuButton onClick={onOpenDiff} disabled={!isChangedFile}>
            Abrir diff
          </MenuButton>
          <MenuButton onClick={onShowSignals} disabled={signals.length === 0}>
            Mostrar señales
          </MenuButton>
        </>
      )}

      <div className="tree-menu__sep" />
      <MenuButton onClick={onCopyRelative}>Copiar ruta relativa</MenuButton>
      <MenuButton onClick={onCopyAbsolute}>Copiar ruta absoluta</MenuButton>
      {!node.isDir && <MenuButton onClick={onCopyName}>Copiar nombre</MenuButton>}
      <div className="tree-menu__sep" />
      <MenuButton onClick={onDelete} danger>
        Eliminar
      </MenuButton>

      {menu.showSignals && (
        <>
          <div className="tree-menu__sep" />
          <div className="tree-menu__signals" data-testid="tree-context-signals">
            {signals.map((signal, index) => (
              <div key={`${signal.kind}:${signal.path ?? "repo"}:${index}`}>
                <strong>{signal.kind.replace(/_/g, " ")}</strong>
                <span>{signal.message}</span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function MenuButton({
  children,
  disabled = false,
  danger = false,
  onClick,
}: {
  children: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={danger ? "tree-menu__item tree-menu__item--danger" : "tree-menu__item"}
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
