// In-project file explorer: the left pane of a project (level-1) tab. Shows the
// repo's own files (always loaded via repoTreeStore — cached + preloaded, so no
// spinner on re-open), with the quality filters applied and the active file
// highlighted. Single click previews a file, double click pins it (VS Code).

import { useEffect, useMemo } from "react";
import { busStore, getRepoSignals, useBusState } from "../../bus/store";
import { filterTreeNodes, hasActiveFilters } from "../../qol/filters";
import { useQualityState } from "../../qol/state";
import { repoTreeStore, useRepoTree } from "../../workspace/repoTreeStore";
import { fileDock, useRepoDock } from "../../workspace/fileDock";
import { buildFileTree } from "./fileTree";
import { FileTreeNode } from "./FileTreeNode";

export function ProjectExplorer({ repo }: { repo: string }) {
  const state = useBusState();
  const { filters } = useQualityState();
  const { tree, loading, error } = useRepoTree(repo);
  const { active } = useRepoDock(repo);
  const delta = state.repos[repo];

  // Load on mount; the store keeps it cached (stale-while-revalidate) thereafter.
  useEffect(() => {
    repoTreeStore.ensureLoaded(repo);
  }, [repo]);

  const nodes = useMemo(() => {
    if (!tree || !delta) return [];
    const signals = getRepoSignals(delta);
    return filterTreeNodes(buildFileTree(tree.entries, delta.status), filters, signals);
  }, [tree, delta, filters]);

  return (
    <div className="project-explorer" data-testid={`project-explorer-${repo}`}>
      <div className="project-explorer__head">
        <span className="project-explorer__title">{busStore.displayName(repo)}</span>
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
                  onOpen={(path, pin) => fileDock.openFile(repo, path, pin)}
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
    </div>
  );
}
