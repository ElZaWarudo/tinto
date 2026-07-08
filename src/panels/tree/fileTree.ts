// Pure file-tree builder (RDM-008, R8): turns the backend's flat list_repo_tree
// entries into a nested folder tree, and marks changed leaves from the repo's
// status. No I/O — unit-tested directly.

import type { RepoStatus, TreeEntry } from "../../bus/contract";
import { changeKindForPath, type RepoChangeKind } from "../statusMarks";

export type ChangeKind = RepoChangeKind;

export interface TreeNode {
  name: string;
  path: string; // repo-relative
  isDir: boolean;
  changed: ChangeKind | null;
  /** This node, or any descendant, has a change — so collapsed folders that
   * contain changed files still show an indicator. */
  hasChanges: boolean;
  children: TreeNode[];
}

function dirname(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}
function basename(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? path : path.slice(i + 1);
}

function normalizeTreePath(path: string): string {
  return path.replace(/\\/g, "/");
}

function sortChildren(node: TreeNode): void {
  node.children.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1; // dirs first
    return a.name.localeCompare(b.name);
  });
  for (const c of node.children) if (c.isDir) sortChildren(c);
}

/** Post-order: a node has changes if it is a changed file or any child does. */
function computeHasChanges(node: TreeNode): boolean {
  let has = node.changed !== null;
  for (const c of node.children) has = computeHasChanges(c) || has;
  node.hasChanges = has;
  return has;
}

/** Build the nested tree (top-level nodes) from flat entries + status. */
export function buildFileTree(entries: TreeEntry[], status: RepoStatus): TreeNode[] {
  const root: TreeNode = {
    name: "",
    path: "",
    isDir: true,
    changed: null,
    hasChanges: false,
    children: [],
  };
  const dirs = new Map<string, TreeNode>([["", root]]);

  const ensureDir = (path: string): TreeNode => {
    const found = dirs.get(path);
    if (found) return found;
    const parent = ensureDir(dirname(path));
    const node: TreeNode = {
      name: basename(path),
      path,
      isDir: true,
      changed: null,
      hasChanges: false,
      children: [],
    };
    parent.children.push(node);
    dirs.set(path, node);
    return node;
  };

  for (const e of entries) {
    const path = normalizeTreePath(e.path);
    if (e.is_dir) {
      ensureDir(path);
    } else {
      const parent = ensureDir(dirname(path));
      parent.children.push({
        name: basename(path),
        path,
        isDir: false,
        changed: changeKindForPath(status, path),
        hasChanges: false,
        children: [],
      });
    }
  }

  sortChildren(root);
  for (const c of root.children) computeHasChanges(c);
  return root.children;
}
