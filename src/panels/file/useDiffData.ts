import { useCallback, useEffect, useRef, useState } from "react";
import { getWorktreeDiff } from "../../bus/client";
import type { FileDiff, RepoDelta } from "../../bus/contract";
import { busStore, getDiff, hasComputedDiffs, useBusState } from "../../bus/store";
import { reconciler, useIsLive } from "../../workspace/subscriptions";

export interface CmdError {
  category: string;
  message: string;
}

const LIVE_DIFF_FALLBACK_DELAY_MS = 250;

function asCmdError(e: unknown): CmdError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as Record<string, unknown>;
    return { category: String(o.category ?? "error"), message: String(o.message ?? e) };
  }
  return { category: "error", message: String(e) };
}

/** True if the file appears in the repo's working-tree status (a change exists
 * independent of whether its diff has loaded yet). */
function fileInStatus(delta: RepoDelta | undefined, path: string): boolean {
  const s = delta?.status;
  return (
    !!s && (s.modified.includes(path) || s.staged.includes(path) || s.untracked.includes(path))
  );
}

export function useDiffData({
  repo,
  path,
  enabled,
}: {
  repo: string;
  path: string;
  enabled: boolean;
}) {
  const state = useBusState();
  const live = getDiff(state, repo, path);
  const repoDelta = state.repos[repo];
  const repoReady = !!repoDelta;
  const isLive = useIsLive(repo, path);
  const diffKey = `${repo}\0${path}`;
  const [oneShotResult, setOneShotResult] = useState<
    { key: string; diff: FileDiff | null } | undefined
  >(undefined);
  const [loadErrorResult, setLoadErrorResult] = useState<{
    key: string;
    error: CmdError;
  } | null>(null);
  const oneShotRequestRef = useRef(0);
  const inStatus = enabled && fileInStatus(repoDelta, path);
  const oneShot =
    inStatus && oneShotResult?.key === diffKey ? oneShotResult.diff : inStatus ? undefined : null;
  const loadError = inStatus && loadErrorResult?.key === diffKey ? loadErrorResult.error : null;

  const loadOneShot = useCallback(() => {
    let active = true;
    const requestId = oneShotRequestRef.current + 1;
    oneShotRequestRef.current = requestId;
    getWorktreeDiff(repo)
      .then((diffs) => {
        if (active && oneShotRequestRef.current === requestId) {
          setOneShotResult({ key: diffKey, diff: diffs.find((d) => d.path === path) ?? null });
          setLoadErrorResult(null);
        }
      })
      .catch((e) => {
        if (active && oneShotRequestRef.current === requestId) {
          setLoadErrorResult({ key: diffKey, error: asCmdError(e) });
        }
      });
    return () => {
      active = false;
    };
  }, [repo, path, diffKey]);

  // Subscribe for live updates + run the one-shot initial load. On unmount drop
  // both the subscription and the cached diff (the single reconciled set - R6).
  useEffect(() => {
    if (!enabled || !repoReady) return;
    reconciler.add(repo, path);
    return () => {
      reconciler.remove(repo, path);
      busStore.dropDiff(repo, path);
    };
  }, [enabled, repo, path, repoReady]);

  useEffect(() => {
    if (!enabled || !repoReady) return;
    if (!inStatus) return;

    if (reconciler.isLive(repo, path)) {
      let cleanup: (() => void) | undefined;
      const timer = window.setTimeout(() => {
        const latest = busStore.getState();
        if (getDiff(latest, repo, path) || hasComputedDiffs(latest, repo)) return;
        cleanup = loadOneShot();
      }, LIVE_DIFF_FALLBACK_DELAY_MS);
      return () => {
        window.clearTimeout(timer);
        cleanup?.();
      };
    }

    return loadOneShot();
  }, [enabled, repoReady, inStatus, loadOneShot, repo, path]);

  // Once a diff computation has occurred for the repo, the live slice is
  // authoritative (KTD2/R7): the one-shot must NOT resurface a diff the slice
  // cleared (e.g. a reverted file omitted by clean-clear-by-omission).
  const computed = hasComputedDiffs(state, repo);
  const diff = computed ? live : (live ?? oneShot);
  const renamedTo = state.diffs[repo]
    ? Object.values(state.diffs[repo]).find((d) => d.old_path === path)?.path
    : undefined;

  return {
    diff,
    inStatus,
    isLive,
    loadError,
    loadOneShot,
    renamedTo,
    repoDelta,
    stateLoaded: state.loaded,
    settling: oneShot === undefined,
  };
}
