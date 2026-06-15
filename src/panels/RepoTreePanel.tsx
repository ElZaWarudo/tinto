// Repo-node tree (RDM-007): the active workbench's repos as nodes with a status
// summary; clicking a node opens/focuses that repo's panel. File-level
// expansion is deferred to RDM-008 (where files open diffs).

import type { RepoStatus } from "../bus/contract";
import { busStore, useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";

function statusSummary(status: RepoStatus): string {
  const { modified, staged, untracked } = status;
  if (!modified.length && !staged.length && !untracked.length) return "clean";
  return `${modified.length}M ${staged.length}S ${untracked.length}U`;
}

export function RepoTreePanel() {
  const { repos } = useBusState();
  const { openRepo } = useWorkspaceActions();
  const paths = Object.keys(repos).sort((a, b) =>
    busStore.displayName(a).localeCompare(busStore.displayName(b)),
  );

  if (paths.length === 0) {
    return <div className="repo-tree repo-tree--empty">No repos.</div>;
  }

  return (
    <div className="repo-tree" data-testid="repo-tree">
      {paths.map((p) => (
        <button
          key={p}
          className="tree-node"
          data-testid={`tree-node-${p}`}
          title={p}
          onClick={() => openRepo(p)}
        >
          <span className="tree-node__name">{busStore.displayName(p)}</span>
          <span className="tree-node__status">{statusSummary(repos[p].status)}</span>
        </button>
      ))}
    </div>
  );
}
