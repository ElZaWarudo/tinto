// Level-2 file view: the content shown when a file tab is active inside a repo
// project tab. Picks the right surface for the file:
//   - Markdown (.md/.markdown) → rendered (formatted) by default, toggle to source.
//   - A file WITH changes → diff (Hunks/Full toggle, inline/side-by-side).
//   - A file WITHOUT changes → the normal highlighted full-file view by default.
// Owns the diff subscription lifecycle (reconciler add/remove + one-shot load +
// dropDiff on unmount), lifted verbatim from the former standalone DiffPanel.

import { useCallback, useEffect, useMemo, useState } from "react";
import { getWorktreeDiff } from "../../bus/client";
import type { FileDiff, RepoDelta } from "../../bus/contract";
import { busStore, getDiff, getPathSignals, hasComputedDiffs, useBusState } from "../../bus/store";
import { reconciler, useIsLive } from "../../workspace/subscriptions";
import { SignalBadges } from "../SignalBadges";
import { DiffView, type DiffMode } from "../diff/DiffView";
import { FullFileView } from "../diff/FullFileView";
import { MediaView } from "./MediaView";
import { mediaKind } from "./mediaTypes";
import { MarkdownView } from "./MarkdownView";

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

/** True if the file appears in the repo's working-tree status (a change exists
 * independent of whether its diff has loaded yet). */
function fileInStatus(delta: RepoDelta | undefined, path: string): boolean {
  const s = delta?.status;
  return (
    !!s && (s.modified.includes(path) || s.staged.includes(path) || s.untracked.includes(path))
  );
}

function isMarkdown(path: string): boolean {
  const dot = path.lastIndexOf(".");
  if (dot < 0) return false;
  const ext = path.slice(dot + 1).toLowerCase();
  return ext === "md" || ext === "markdown";
}

export function FileView({ repo, path }: { repo: string; path: string }) {
  const state = useBusState();
  const live = getDiff(state, repo, path);
  const pathSignals = getPathSignals(state.repos[repo], path);
  const isLive = useIsLive(repo, path);
  const markdown = isMarkdown(path);
  const media = mediaKind(path);

  const [oneShot, setOneShot] = useState<FileDiff | null | undefined>(undefined);
  const [loadError, setLoadError] = useState<CmdError | null>(null);
  const [mode, setMode] = useState<DiffMode>("inline");
  // null = follow the auto rule (changed→hunks, clean→full); set by the toggle.
  const [manualKind, setManualKind] = useState<"hunks" | "full" | null>(null);
  // Markdown sub-view: rendered (formatted) by default, or raw source.
  const [mdView, setMdView] = useState<"rendered" | "source">("rendered");

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
    if (media) return;
    reconciler.add(repo, path);
    const cancel = loadOneShot();
    return () => {
      cancel();
      reconciler.remove(repo, path);
      busStore.dropDiff(repo, path);
    };
  }, [repo, path, loadOneShot, media]);

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

  // Auto rule: a file with a known change shows the diff; an unchanged file
  // shows the normal full-file view. The user can override with the toggle.
  const inStatus = fileInStatus(state.repos[repo], path);
  const hasChange = !!diff || inStatus;
  const viewKind = manualKind ?? (hasChange ? "hunks" : "full");
  // The initial one-shot load hasn't settled yet (undefined = not loaded). Used
  // to show a spinner instead of a premature "no changes" while the diff loads.
  const settling = oneShot === undefined;

  return (
    <div className="file-view" data-testid={`file-view-${repo}::${path}`}>
      <div className="file-view__toolbar">
        {media ? (
          <span className="file-view__mode" data-testid="media-mode">
            {media === "pdf" ? "PDF preview" : "Image preview"}
          </span>
        ) : markdown ? (
          <div className="seg" role="group" aria-label="markdown view">
            <button
              className={mdView === "rendered" ? "seg__btn seg__btn--on" : "seg__btn"}
              onClick={() => setMdView("rendered")}
            >
              Formateado
            </button>
            <button
              className={mdView === "source" ? "seg__btn seg__btn--on" : "seg__btn"}
              onClick={() => setMdView("source")}
            >
              Fuente
            </button>
          </div>
        ) : (
          <>
            <div className="seg" role="group" aria-label="file view">
              <button
                className={viewKind === "hunks" ? "seg__btn seg__btn--on" : "seg__btn"}
                onClick={() => setManualKind("hunks")}
              >
                Hunks
              </button>
              <button
                className={viewKind === "full" ? "seg__btn seg__btn--on" : "seg__btn"}
                onClick={() => setManualKind("full")}
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
          </>
        )}
        <span className="file-view__path" title={`${repo} — ${path}`}>
          {path}
        </span>
        <SignalBadges signals={pathSignals} limit={3} compact />
      </div>

      {!media && !isLive && (
        <div className="file-view__paused" data-testid="diff-paused">
          <span>Live updates paused (subscription limit reached).</span>
          <button onClick={loadOneShot} data-testid="diff-paused-reload">
            Reload
          </button>
        </div>
      )}

      <div className="file-view__body">
        {media ? (
          <MediaView repo={repo} path={path} kind={media} />
        ) : markdown ? (
          mdView === "rendered" ? (
            <MarkdownView repo={repo} path={path} />
          ) : (
            <FullFileView repo={repo} path={path} changedLines={changed} />
          )
        ) : viewKind === "full" ? (
          <FullFileView repo={repo} path={path} changedLines={changed} />
        ) : loadError ? (
          <div className="file-view__error" data-testid="diff-error">
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
          <div className="file-view__empty" data-testid="diff-renamed">
            <span>This file was renamed to “{renamedTo}”. Reopen it from the tree.</span>
          </div>
        ) : !isLive ? (
          <div className="file-view__empty" data-testid="diff-paused-body">
            <span>Live updates paused (subscription limit). Reload to load this diff.</span>
            <button onClick={loadOneShot} data-testid="diff-paused-body-reload">
              Reload
            </button>
          </div>
        ) : inStatus || settling ? (
          // The file has changes (per status) but its diff hasn't arrived yet,
          // or the initial load is still in flight: show a spinner instead of a
          // premature "no changes" flash.
          <div className="file-view__loading" data-testid="diff-loading">
            Loading…
          </div>
        ) : (
          // Settled with no diff and nothing in status: the file is clean, so
          // show its normal content rather than an empty "no changes" box.
          <FullFileView repo={repo} path={path} changedLines={changed} />
        )}
      </div>
    </div>
  );
}
