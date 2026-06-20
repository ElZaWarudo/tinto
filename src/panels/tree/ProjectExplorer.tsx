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

interface ContextMenuState {
  node: TreeNode;
  x: number;
  y: number;
  showSignals: boolean;
}

const MENU_VIEWPORT_MARGIN = 8;

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

  // Load on mount; the store keeps it cached (stale-while-revalidate) thereafter.
  useEffect(() => {
    repoTreeStore.ensureLoaded(repo);
  }, [repo]);

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
    <div className="project-explorer" data-testid={`project-explorer-${repo}`}>
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
      <div className="project-explorer__body">
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
                />
              ))}
            {tree?.truncated && (
              <div className="tree-files__msg" data-testid="explorer-truncated">
                Tree truncated (too many files).
              </div>
            )}
          </>
        )}
      </div>
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
  onClick,
}: {
  children: ReactNode;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className="tree-menu__item"
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
