// One node in a file tree: a collapsible folder (single click toggles) or a
// file. File interaction follows VS Code: a single click PREVIEWS (onOpen with
// pin=false), a double click PINS (pin=true); Enter pins, Space previews. The
// active file row is highlighted. Shared by the in-project explorer.

import { useState, type MouseEvent } from "react";
import type { RepoDelta } from "../../bus/contract";
import { getPathSignals } from "../../bus/store";
import { SignalBadges } from "../SignalBadges";
import type { TreeNode } from "./fileTree";

const MARK: Record<string, string> = { staged: "S", modified: "M", untracked: "U" };

export function FileTreeNode({
  node,
  delta,
  depth,
  activePath,
  expandedDirs,
  onToggleDir,
  onOpen,
  onContextMenu,
}: {
  node: TreeNode;
  delta: RepoDelta;
  depth: number;
  activePath: string | null;
  expandedDirs?: Set<string>;
  onToggleDir?: (path: string) => void;
  onOpen: (path: string, pin: boolean) => void;
  onContextMenu?: (event: MouseEvent, node: TreeNode) => void;
}) {
  const [localOpen, setLocalOpen] = useState(false); // fallback for standalone use
  const open = expandedDirs ? expandedDirs.has(node.path) : localOpen;
  const pad = { paddingLeft: `${depth * 12 + 8}px` };
  const toggleDir = () => {
    if (onToggleDir) onToggleDir(node.path);
    else setLocalOpen((o) => !o);
  };

  if (node.isDir) {
    const dirClass = node.hasChanges ? "tree-dir__row tree-dir__row--changed" : "tree-dir__row";
    return (
      <div className="tree-dir">
        <button
          className={dirClass}
          style={pad}
          onClick={toggleDir}
          onContextMenu={(event) => onContextMenu?.(event, node)}
        >
          <span className="tree-dir__caret">{open ? "▾" : "▸"}</span>
          <span className="tree-dir__name">{node.name}</span>
          {node.hasChanges && (
            <span
              className="tree-dir__dot"
              data-testid={`tree-dir-changed-${node.path}`}
              title="Contiene archivos con cambios"
            >
              ●
            </span>
          )}
        </button>
        {open &&
          node.children.map((c) => (
            <FileTreeNode
              key={c.path}
              node={c}
              delta={delta}
              depth={depth + 1}
              activePath={activePath}
              expandedDirs={expandedDirs}
              onToggleDir={onToggleDir}
              onOpen={onOpen}
              onContextMenu={onContextMenu}
            />
          ))}
      </div>
    );
  }

  const classes = ["tree-file"];
  if (node.changed) classes.push("tree-file--changed");
  if (node.path === activePath) classes.push("tree-file--active");

  return (
    <div
      className={classes.join(" ")}
      style={pad}
      role="button"
      tabIndex={0}
      data-testid={`tree-file-${node.path}`}
      onClick={() => onOpen(node.path, false)}
      onDoubleClick={() => onOpen(node.path, true)}
      onContextMenu={(event) => onContextMenu?.(event, node)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen(node.path, true);
        } else if (e.key === " ") {
          e.preventDefault();
          onOpen(node.path, false);
        }
      }}
    >
      <span className="tree-file__name">{node.name}</span>
      {node.changed && (
        <span className={`tree-file__mark tree-file__mark--${node.changed}`}>
          {MARK[node.changed]}
        </span>
      )}
      <SignalBadges signals={getPathSignals(delta, node.path)} limit={1} compact />
    </div>
  );
}
