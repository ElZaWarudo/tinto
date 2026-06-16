// Repo tree (RDM-008): the active workbench's repos, each expandable to its
// files (list_repo_tree). Files are marked from the live status and open a diff
// on activation (double-click / Enter / Space). Folders are collapsed by
// default. The repo name still opens the repo detail panel.

import { useMemo, useState } from "react";
import { listRepoTree } from "../bus/client";
import type { RepoDelta, RepoStatus, RepoTree } from "../bus/contract";
import {
  busStore,
  getPathSignals,
  sortedRepoPaths,
  statusSummary,
  useBusState,
} from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";
import { SignalBadges } from "./SignalBadges";
import { buildFileTree, type TreeNode } from "./tree/fileTree";

const MARK: Record<string, string> = { staged: "S", modified: "M", untracked: "U" };

export function RepoTreePanel() {
  const state = useBusState();
  const paths = sortedRepoPaths(busStore, state);

  if (paths.length === 0) {
    return <div className="repo-tree repo-tree--empty">No repos.</div>;
  }

  return (
    <div className="repo-tree" data-testid="repo-tree">
      {paths.map((p) => (
        <RepoTreeNode key={p} repo={p} delta={state.repos[p]} status={state.repos[p].status} />
      ))}
    </div>
  );
}

function RepoTreeNode({
  repo,
  delta,
  status,
}: {
  repo: string;
  delta: RepoDelta;
  status: RepoStatus;
}) {
  const { openRepo, openDiff } = useWorkspaceActions();
  const [expanded, setExpanded] = useState(false);
  const [tree, setTree] = useState<RepoTree | undefined>(undefined); // undefined=not loaded
  const [error, setError] = useState(false);

  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    if (next && tree === undefined && !error) {
      listRepoTree(repo)
        .then(setTree)
        .catch(() => setError(true));
    }
  };

  // Markers update live: rebuild when entries or status change.
  const nodes = useMemo(() => (tree ? buildFileTree(tree.entries, status) : []), [tree, status]);

  return (
    <div className="tree-repo">
      <div className="tree-repo__row">
        <button
          className="tree-repo__caret"
          aria-label={expanded ? "collapse" : "expand"}
          data-testid={`tree-expand-${repo}`}
          onClick={toggle}
        >
          {expanded ? "▾" : "▸"}
        </button>
        <button
          className="tree-node"
          data-testid={`tree-node-${repo}`}
          title={repo}
          onClick={() => openRepo(repo)}
        >
          <span className="tree-node__name">{busStore.displayName(repo)}</span>
          <span className="tree-node__status">{statusSummary(status)}</span>
        </button>
      </div>
      {expanded && (
        <div className="tree-repo__files" data-testid={`tree-files-${repo}`}>
          {error ? (
            <div className="tree-files__msg">Could not load files.</div>
          ) : tree === undefined ? (
            <div className="tree-files__msg">Loading…</div>
          ) : (
            <>
              {nodes.map((n) => (
                <FileTreeNode
                  key={n.path}
                  node={n}
                  delta={delta}
                  depth={0}
                  onOpen={(path) => openDiff(repo, path)}
                />
              ))}
              {tree.truncated && (
                <div className="tree-files__msg" data-testid={`tree-truncated-${repo}`}>
                  Tree truncated (too many files).
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function FileTreeNode({
  node,
  delta,
  depth,
  onOpen,
}: {
  node: TreeNode;
  delta: RepoDelta;
  depth: number;
  onOpen: (path: string) => void;
}) {
  const [open, setOpen] = useState(false); // folders collapsed by default
  const pad = { paddingLeft: `${depth * 12 + 8}px` };

  if (node.isDir) {
    return (
      <div className="tree-dir">
        <button className="tree-dir__row" style={pad} onClick={() => setOpen((o) => !o)}>
          <span className="tree-dir__caret">{open ? "▾" : "▸"}</span>
          <span className="tree-dir__name">{node.name}</span>
        </button>
        {open &&
          node.children.map((c) => (
            <FileTreeNode key={c.path} node={c} delta={delta} depth={depth + 1} onOpen={onOpen} />
          ))}
      </div>
    );
  }

  return (
    <div
      className={node.changed ? "tree-file tree-file--changed" : "tree-file"}
      style={pad}
      role="button"
      tabIndex={0}
      data-testid={`tree-file-${node.path}`}
      onDoubleClick={() => onOpen(node.path)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen(node.path);
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
