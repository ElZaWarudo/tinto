// Pure file-tree builder (RDM-008, R8): turns the backend's flat list_repo_tree
// entries into a nested folder tree, and marks changed leaves from the repo's
// status. No I/O — unit-tested directly.

import type { RepoStatus, TreeEntry } from "../../bus/contract";

export type ChangeKind = "staged" | "modified" | "untracked";

export interface TreeNode {
  name: string;
  path: string; // repo-relative
  isDir: boolean;
  changed: ChangeKind | null;
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

/** Change kind for a file path; staged wins over modified wins over untracked. */
function changeKind(path: string, status: RepoStatus): ChangeKind | null {
  if (status.staged.includes(path)) return "staged";
  if (status.modified.includes(path)) return "modified";
  if (status.untracked.includes(path)) return "untracked";
  return null;
}

function sortChildren(node: TreeNode): void {
  node.children.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1; // dirs first
    return a.name.localeCompare(b.name);
  });
  for (const c of node.children) if (c.isDir) sortChildren(c);
}

/** Build the nested tree (top-level nodes) from flat entries + status. */
export function buildFileTree(entries: TreeEntry[], status: RepoStatus): TreeNode[] {
  const root: TreeNode = { name: "", path: "", isDir: true, changed: null, children: [] };
  const dirs = new Map<string, TreeNode>([["", root]]);

  const ensureDir = (path: string): TreeNode => {
    const found = dirs.get(path);
    if (found) return found;
    const parent = ensureDir(dirname(path));
    const node: TreeNode = { name: basename(path), path, isDir: true, changed: null, children: [] };
    parent.children.push(node);
    dirs.set(path, node);
    return node;
  };

  for (const e of entries) {
    if (e.is_dir) {
      ensureDir(e.path);
    } else {
      const parent = ensureDir(dirname(e.path));
      parent.children.push({
        name: basename(e.path),
        path: e.path,
        isDir: false,
        changed: changeKind(e.path, status),
        children: [],
      });
    }
  }

  sortChildren(root);
  return root.children;
}
