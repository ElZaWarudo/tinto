// Diff renderer (RDM-008, D-008-1): renders a structured FileDiff as either an
// inline (stacked removed→added) or side-by-side (old left / new right) view,
// from one data source. Syntax highlighting (Shiki) layers on after the
// structure paints and is bounded by a size cap; binary and oversized files
// fall back to a placeholder / plain monospace.

import type { DiffHunk, DiffLine, FileDiff } from "../../bus/contract";
import { FileOverviewRuler, type FileOverviewMarker } from "../file/FileOverviewRuler";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { useLineHighlighter, type RenderLine } from "./lineHighlighter";

export type DiffMode = "inline" | "side-by-side";

function diffSize(diff: FileDiff): number {
  let n = 0;
  for (const h of diff.hunks) for (const l of h.lines) n += l.content.length;
  return n;
}

const SIGN: Record<DiffLine["kind"], string> = { Added: "+", Removed: "-", Context: " " };

export function DiffView({
  diff,
  mode,
  overviewMarkers = [],
  overviewTotalLines = 0,
}: {
  diff: FileDiff;
  mode: DiffMode;
  overviewMarkers?: FileOverviewMarker[];
  overviewTotalLines?: number;
}) {
  const oversized = diffSize(diff) > MAX_HIGHLIGHT_BYTES;
  const lang = languageFromPath(diff.path);
  const render = useLineHighlighter(lang, !diff.is_binary && !oversized);
  const markedLines = new Set(overviewMarkers.map((marker) => marker.line));

  if (diff.is_binary) {
    return (
      <div className="diff-view diff-view--binary" data-testid="diff-binary">
        Binary file — no text diff.
      </div>
    );
  }

  return (
    <div className={`diff-view diff-view--${mode}`} data-testid="diff-view">
      <FileOverviewRuler
        markers={overviewMarkers}
        totalLines={overviewTotalLines}
        targetAttribute="data-new-line"
      />
      {oversized && (
        <div className="diff-view__notice" data-testid="diff-large">
          Large file — syntax highlighting disabled.
        </div>
      )}
      {diff.hunks.length === 0 ? (
        <div className="diff-view__notice" data-testid="diff-no-hunks">
          No textual changes.
        </div>
      ) : mode === "inline" ? (
        diff.hunks.map((h, i) => (
          <InlineHunk key={i} hunk={h} render={render} markedLines={markedLines} />
        ))
      ) : (
        diff.hunks.map((h, i) => (
          <SplitHunk key={i} hunk={h} render={render} markedLines={markedLines} />
        ))
      )}
    </div>
  );
}

function HunkHeader({ hunk }: { hunk: DiffHunk }) {
  return (
    <div className="diff-hunk-header" data-testid="hunk-header">
      @@ -{hunk.old_start} +{hunk.new_start} @@
    </div>
  );
}

function lineClass(line: DiffLine, markedLines: Set<number>): string {
  const classes = [`diff-line`, `diff-line--${line.kind.toLowerCase()}`];
  if (line.new_lineno != null && markedLines.has(line.new_lineno)) {
    classes.push("diff-line--signal-critical");
  }
  return classes.join(" ");
}

function InlineHunk({
  hunk,
  render,
  markedLines,
}: {
  hunk: DiffHunk;
  render: RenderLine;
  markedLines: Set<number>;
}) {
  return (
    <div className="diff-hunk">
      <HunkHeader hunk={hunk} />
      {hunk.lines.map((l, i) => (
        <div
          key={i}
          className={lineClass(l, markedLines)}
          data-new-line={l.new_lineno ?? undefined}
        >
          <span className="diff-gutter">{l.old_lineno ?? ""}</span>
          <span className="diff-gutter">{l.new_lineno ?? ""}</span>
          <span className="diff-sign">{SIGN[l.kind]}</span>
          <code className="diff-content">{render(l.content)}</code>
        </div>
      ))}
    </div>
  );
}

function SplitHunk({
  hunk,
  render,
  markedLines,
}: {
  hunk: DiffHunk;
  render: RenderLine;
  markedLines: Set<number>;
}) {
  // Left = old file slice (context + removed); right = new (context + added).
  const left = hunk.lines.filter((l) => l.kind !== "Added");
  const right = hunk.lines.filter((l) => l.kind !== "Removed");
  return (
    <div className="diff-hunk">
      <HunkHeader hunk={hunk} />
      <div className="diff-split">
        <div className="diff-side diff-side--old" data-testid="diff-old">
          {left.map((l, i) => (
            <div
              key={i}
              className={lineClass(l, markedLines)}
              data-new-line={l.new_lineno ?? undefined}
            >
              <span className="diff-gutter">{l.old_lineno ?? ""}</span>
              <code className="diff-content">{render(l.content)}</code>
            </div>
          ))}
        </div>
        <div className="diff-side diff-side--new" data-testid="diff-new">
          {right.map((l, i) => (
            <div
              key={i}
              className={lineClass(l, markedLines)}
              data-new-line={l.new_lineno ?? undefined}
            >
              <span className="diff-gutter">{l.new_lineno ?? ""}</span>
              <code className="diff-content">{render(l.content)}</code>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
