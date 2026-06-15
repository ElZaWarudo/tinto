// Diff panel (RDM-008): the dockview panel for one (repo, path) target. Owns the
// subscription lifecycle (reconciler add/remove on mount/unmount + dropDiff),
// the R7 initial load (one-shot get_worktree_diff for tracked files; live for
// untracked), and renders the structured diff (inline/side-by-side) or the
// full-file view, with all the R10 states. Live updates flow from the store's
// diff slice (KTD2); the one-shot only fills until a live diff arrives.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { getWorktreeDiff } from "../../bus/client";
import type { FileDiff } from "../../bus/contract";
import { busStore, getDiff, hasComputedDiffs, useBusState } from "../../bus/store";
import { reconciler, useIsLive } from "../../workspace/subscriptions";
import { DiffView, type DiffMode } from "./DiffView";
import { FullFileView } from "./FullFileView";

interface CmdError {
  category: string;
  message: string;
}

function asCmdError(e: unknown): CmdError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as Record<string, unknown>;
    return { category: String(o.category ?? "error"), message: String(o.message ?? e) };
  }
  return { category: "error", message: String(e) };
}

/** new-side line numbers of added lines, for full-file change marking. */
function addedLines(diff: FileDiff | null | undefined): Set<number> {
  const s = new Set<number>();
  if (!diff) return s;
  for (const h of diff.hunks)
    for (const l of h.lines) if (l.kind === "Added" && l.new_lineno != null) s.add(l.new_lineno);
  return s;
}

export function DiffPanel(props: IDockviewPanelProps<{ repo: string; path: string }>) {
  const { repo, path } = props.params;
  const state = useBusState();
  const live = getDiff(state, repo, path);
  const isLive = useIsLive(repo, path);

  const [oneShot, setOneShot] = useState<FileDiff | null | undefined>(undefined);
  const [loadError, setLoadError] = useState<CmdError | null>(null);
  const [mode, setMode] = useState<DiffMode>("inline");
  const [viewKind, setViewKind] = useState<"hunks" | "full">("hunks");

  const loadOneShot = useCallback(() => {
    let active = true;
    getWorktreeDiff(repo)
      .then((diffs) => {
        if (active) {
          setOneShot(diffs.find((d) => d.path === path) ?? null);
          setLoadError(null);
        }
      })
      .catch((e) => active && setLoadError(asCmdError(e)));
    return () => {
      active = false;
    };
  }, [repo, path]);

  // Subscribe for live updates + run the one-shot initial load. On unmount drop
  // both the subscription and the cached diff (the single reconciled set — R6).
  useEffect(() => {
    reconciler.add(repo, path);
    const cancel = loadOneShot();
    return () => {
      cancel();
      reconciler.remove(repo, path);
      busStore.dropDiff(repo, path);
    };
  }, [repo, path, loadOneShot]);

  // Once a diff computation has occurred for the repo, the live slice is
  // authoritative (KTD2/R7): the one-shot must NOT resurface a diff the slice
  // cleared (e.g. a reverted file omitted by clean-clear-by-omission).
  const computed = hasComputedDiffs(state, repo);
  const diff = computed ? live : (live ?? oneShot);
  const changed = useMemo(() => addedLines(diff), [diff]);

  // Rename detection (R11/AE10): a repo diff whose old_path is our path means
  // the file was renamed away — a distinct state from clean/reverted.
  const renamedTo = state.diffs[repo]
    ? Object.values(state.diffs[repo]).find((d) => d.old_path === path)?.path
    : undefined;

  return (
    <div className="diff-panel" data-testid={`diff-panel-${repo}::${path}`}>
      <div className="diff-panel__toolbar">
        <div className="seg" role="group" aria-label="diff view">
          <button
            className={viewKind === "hunks" ? "seg__btn seg__btn--on" : "seg__btn"}
            onClick={() => setViewKind("hunks")}
          >
            Hunks
          </button>
          <button
            className={viewKind === "full" ? "seg__btn seg__btn--on" : "seg__btn"}
            onClick={() => setViewKind("full")}
          >
            Full file
          </button>
        </div>
        <div className="seg" role="group" aria-label="diff layout">
          <button
            className={mode === "inline" ? "seg__btn seg__btn--on" : "seg__btn"}
            disabled={viewKind === "full"}
            onClick={() => setMode("inline")}
          >
            Inline
          </button>
          <button
            className={mode === "side-by-side" ? "seg__btn seg__btn--on" : "seg__btn"}
            disabled={viewKind === "full"}
            onClick={() => setMode("side-by-side")}
          >
            Side by side
          </button>
        </div>
        <span className="diff-panel__path" title={`${repo} — ${path}`}>
          {path}
        </span>
      </div>

      {!isLive && (
        <div className="diff-panel__paused" data-testid="diff-paused">
          <span>Live updates paused (subscription limit reached).</span>
          <button onClick={loadOneShot} data-testid="diff-paused-reload">
            Reload
          </button>
        </div>
      )}

      <div className="diff-panel__body">
        {viewKind === "full" ? (
          <FullFileView repo={repo} path={path} changedLines={changed} />
        ) : loadError ? (
          <div className="diff-panel__error" data-testid="diff-error">
            <span>
              {loadError.category}: {loadError.message}
            </span>
            <button onClick={loadOneShot} data-testid="diff-error-retry">
              Retry
            </button>
          </div>
        ) : diff ? (
          <DiffView diff={diff} mode={mode} />
        ) : renamedTo ? (
          <div className="diff-panel__empty" data-testid="diff-renamed">
            <span>This file was renamed to “{renamedTo}”. Reopen it from the tree.</span>
          </div>
        ) : !isLive ? (
          // Paused with no cached diff: don't fake a spinner or claim "no changes".
          <div className="diff-panel__empty" data-testid="diff-paused-body">
            <span>Live updates paused (subscription limit). Reload to load this diff.</span>
            <button onClick={loadOneShot} data-testid="diff-paused-body-reload">
              Reload
            </button>
          </div>
        ) : computed ? (
          <div className="diff-panel__empty" data-testid="diff-empty">
            <span>No changes for this file.</span>
            <button onClick={loadOneShot} data-testid="diff-empty-reload">
              Reload
            </button>
          </div>
        ) : (
          <div className="diff-panel__loading" data-testid="diff-loading">
            Loading…
          </div>
        )}
      </div>
    </div>
  );
}
