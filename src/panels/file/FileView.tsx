// Level-2 file view: the content shown when a file tab is active inside a repo
// project tab. Picks the right surface for the file:
//   - Markdown (.md/.markdown) → rendered (formatted) by default, toggle to source.
//   - A file WITH changes → diff (Hunks/Full toggle, inline/side-by-side).
//   - A file WITHOUT changes → the normal highlighted full-file view by default.

import { type KeyboardEvent, useMemo, useRef, useState } from "react";
import type { FileDiff, SecretFinding } from "../../bus/contract";
import { getPathSecretFindings, getPathSignals } from "../../bus/store";
import { SignalBadges } from "../SignalBadges";
import { DiffView, type DiffMode } from "../diff/DiffView";
import { FullFileView } from "../diff/FullFileView";
import type { FileOverviewMarker } from "./FileOverviewRuler";
import { MediaView } from "./MediaView";
import { mediaKind } from "./mediaTypes";
import { MarkdownView } from "./MarkdownView";
import { useDiffData } from "./useDiffData";

/** new-side line numbers of added lines, for full-file change marking. */
function addedLines(diff: FileDiff | null | undefined): Set<number> {
  const s = new Set<number>();
  if (!diff) return s;
  for (const h of diff.hunks)
    for (const l of h.lines) if (l.kind === "Added" && l.new_lineno != null) s.add(l.new_lineno);
  return s;
}

function isMarkdown(path: string): boolean {
  const dot = path.lastIndexOf(".");
  if (dot < 0) return false;
  const ext = path.slice(dot + 1).toLowerCase();
  return ext === "md" || ext === "markdown";
}

function possibleSecretLineMarker(line: string): boolean {
  const lower = line.toLowerCase();
  return (
    (lower.includes("-----begin ") && lower.includes("private key")) ||
    lower.includes("api_key") ||
    lower.includes("apikey") ||
    lower.includes("access_token") ||
    lower.includes("auth_token") ||
    lower.includes("secret=") ||
    lower.includes("secret:") ||
    lower.includes("token=") ||
    lower.includes("token:") ||
    lower.includes("password=") ||
    lower.includes("password:") ||
    lower.includes("private_key")
  );
}

function secretOverviewMarkers(diff: FileDiff | null | undefined): FileOverviewMarker[] {
  if (!diff) return [];
  const markers: FileOverviewMarker[] = [];
  const seen = new Set<number>();
  for (const hunk of diff.hunks) {
    for (const line of hunk.lines) {
      if (
        line.kind === "Added" &&
        line.new_lineno != null &&
        possibleSecretLineMarker(line.content) &&
        !seen.has(line.new_lineno)
      ) {
        seen.add(line.new_lineno);
        markers.push({
          line: line.new_lineno,
          severity: "critical",
          label: "Possible secret",
        });
      }
    }
  }
  return markers;
}

function findingsOverviewMarkers(findings: SecretFinding[]): FileOverviewMarker[] {
  const markers: FileOverviewMarker[] = [];
  const seen = new Set<number>();
  for (const finding of findings) {
    if (seen.has(finding.line)) continue;
    seen.add(finding.line);
    markers.push({
      line: finding.line,
      severity: "critical",
      label: "Possible secret",
    });
  }
  return markers;
}

function maxNewLine(diff: FileDiff | null | undefined): number {
  if (!diff) return 0;
  let max = 0;
  for (const hunk of diff.hunks) {
    for (const line of hunk.lines) {
      if (line.new_lineno != null) max = Math.max(max, line.new_lineno);
    }
  }
  return max;
}

function hunkOverviewMarkers(lines: Set<number>): FileOverviewMarker[] {
  const sorted = [...lines].sort((a, b) => a - b);
  return sorted.map((line, index) => {
    const startsGroup = index === 0 || sorted[index - 1] !== line - 1;
    let groupLength = 1;
    if (startsGroup) {
      while (sorted[index + groupLength] === line + groupLength) groupLength += 1;
    }
    return {
      line,
      severity: "info",
      label: groupLength > 1 ? "Changed lines" : "Changed line",
      source: "hunk",
      showLabel: startsGroup,
    };
  });
}

export function FileView({ repo, path }: { repo: string; path: string }) {
  const bodyRef = useRef<HTMLDivElement>(null);
  const markdown = isMarkdown(path);
  const media = mediaKind(path);
  const {
    diff,
    inStatus,
    isLive,
    loadError,
    loadOneShot,
    renamedTo,
    repoDelta,
    stateLoaded,
    settling,
  } = useDiffData({ repo, path, enabled: !media });
  const pathSignals = getPathSignals(repoDelta, path);
  const pathSecretFindings = getPathSecretFindings(repoDelta, path);

  const [mode, setMode] = useState<DiffMode>("inline");
  // null = follow the auto rule (changed→hunks, clean→full); set by the toggle.
  const [manualKind, setManualKind] = useState<"hunks" | "full" | null>(null);
  // Markdown sub-view: rendered (formatted) by default, or raw source.
  const [mdView, setMdView] = useState<"rendered" | "source">("rendered");
  const handleBodyKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && event.target === event.currentTarget) {
      event.currentTarget.blur();
    }
  };
  const changed = useMemo(() => addedLines(diff), [diff]);
  const overviewMarkers = useMemo(() => {
    const alertMarkers =
      pathSecretFindings.length > 0
        ? findingsOverviewMarkers(pathSecretFindings)
        : secretOverviewMarkers(diff);
    return [...hunkOverviewMarkers(changed), ...alertMarkers];
  }, [changed, diff, pathSecretFindings]);
  const overviewTotalLines = useMemo(() => {
    const diffLines = maxNewLine(diff);
    const findingLines = pathSecretFindings.reduce(
      (max, finding) => Math.max(max, finding.line),
      0,
    );
    return Math.max(diffLines, findingLines);
  }, [diff, pathSecretFindings]);

  if (!repoDelta && !stateLoaded) {
    return (
      <div className="file-view" data-testid={`file-view-${repo}::${path}`}>
        <div className="file-view__toolbar">
          <span className="file-view__path" title={`${repo} - ${path}`}>
            {path}
          </span>
        </div>
        <div className="file-view__body">
          <div className="file-view__loading" data-testid="file-repo-loading">
            Loading...
          </div>
        </div>
      </div>
    );
  }

  if (!repoDelta) {
    return (
      <div className="file-view" data-testid={`file-view-${repo}::${path}`}>
        <div className="file-view__toolbar">
          <span className="file-view__path" title={`${repo} - ${path}`}>
            {path}
          </span>
        </div>
        <div className="file-view__body">
          <div className="file-view__error" data-testid="file-repo-missing">
            This repo is no longer available in the active workbench.
          </div>
        </div>
      </div>
    );
  }

  // Auto rule: a file with a known change shows the diff; an unchanged file
  // shows the normal full-file view. The user can override with the toggle.
  const hasChange = !!diff || inStatus;
  const viewKind = manualKind ?? (hasChange ? "hunks" : "full");

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

      <div
        className="file-view__body"
        ref={bodyRef}
        role="region"
        aria-label={`${path} file contents`}
        tabIndex={0}
        onKeyDown={handleBodyKeyDown}
      >
        {media ? (
          <MediaView repo={repo} path={path} kind={media} />
        ) : markdown ? (
          mdView === "rendered" ? (
            <MarkdownView repo={repo} path={path} />
          ) : (
            <FullFileView
              repo={repo}
              path={path}
              repoRevision={repoDelta.revision}
              changedLines={changed}
              overviewMarkers={overviewMarkers}
              bodyRef={bodyRef}
            />
          )
        ) : viewKind === "full" ? (
          <FullFileView
            repo={repo}
            path={path}
            repoRevision={repoDelta.revision}
            changedLines={changed}
            overviewMarkers={overviewMarkers}
            bodyRef={bodyRef}
          />
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
          <DiffView
            diff={diff}
            mode={mode}
            overviewMarkers={overviewMarkers}
            overviewTotalLines={overviewTotalLines}
            bodyRef={bodyRef}
          />
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
          <FullFileView
            repo={repo}
            path={path}
            repoRevision={repoDelta.revision}
            changedLines={changed}
            overviewMarkers={overviewMarkers}
            bodyRef={bodyRef}
          />
        )}
      </div>
    </div>
  );
}
