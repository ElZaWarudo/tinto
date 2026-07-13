import type { CSSProperties, ReactNode } from "react";
import type { DiffHunk, DiffLine, FileDiff } from "../../bus/contract";
import { FileOverviewRuler, type FileOverviewMarker } from "../file/FileOverviewRuler";
import { useOverviewScrollSync } from "../file/useOverviewScrollSync";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { MAX_RENDERED_DIFF_LINE_CHARS, MAX_RENDERED_DIFF_LINES } from "./limits";
import { useLineHighlighter, type RenderLine } from "./lineHighlighter";

export type DiffMode = "inline" | "side-by-side";

function diffStats(diff: FileDiff): { bytes: number; lines: number; longLines: number } {
  let bytes = 0;
  let lines = 0;
  let longLines = 0;
  for (const h of diff.hunks) {
    lines += h.lines.length;
    for (const l of h.lines) {
      bytes += l.content.length;
      if (l.content.length > MAX_RENDERED_DIFF_LINE_CHARS) longLines += 1;
    }
  }
  return { bytes, lines, longLines };
}

function visibleHunks(
  hunks: DiffHunk[],
  maxLines: number,
): { hunks: DiffHunk[]; renderedLines: number; hiddenLines: number } {
  let remaining = maxLines;
  let totalLines = 0;
  const visible: DiffHunk[] = [];

  for (const hunk of hunks) {
    totalLines += hunk.lines.length;
    if (remaining <= 0) continue;
    if (hunk.lines.length <= remaining) {
      visible.push(hunk);
      remaining -= hunk.lines.length;
      continue;
    }
    visible.push({ ...hunk, lines: hunk.lines.slice(0, remaining) });
    remaining = 0;
  }

  return {
    hunks: visible,
    renderedLines: Math.min(totalLines, maxLines),
    hiddenLines: Math.max(0, totalLines - maxLines),
  };
}

function overviewLinesFor(diff: FileDiff, totalLines: number): string[] {
  if (totalLines > MAX_RENDERED_DIFF_LINES) return [];
  return diff.hunks.flatMap((hunk) =>
    hunk.lines.map((line) => `${SIGN[line.kind]}${linePreview(line.content)}`),
  );
}

function linePreview(content: string): string {
  if (content.length <= MAX_RENDERED_DIFF_LINE_CHARS) return content;
  return content.slice(0, MAX_RENDERED_DIFF_LINE_CHARS);
}

function renderContent(content: string, render: RenderLine): ReactNode {
  if (content.length <= MAX_RENDERED_DIFF_LINE_CHARS) return render(content);
  const hidden = content.length - MAX_RENDERED_DIFF_LINE_CHARS;
  return (
    <>
      {render(content.slice(0, MAX_RENDERED_DIFF_LINE_CHARS))}
      <span className="diff-content__truncated" data-testid="diff-line-truncated">
        {" "}
        … {hidden.toLocaleString()} caracteres ocultos
      </span>
    </>
  );
}

const SIGN: Record<DiffLine["kind"], string> = { Added: "+", Removed: "-", Context: " " };

export function DiffView({
  diff,
  mode,
  overviewMarkers = [],
  overviewTotalLines = 0,
  bodyRef,
}: {
  diff: FileDiff;
  mode: DiffMode;
  overviewMarkers?: FileOverviewMarker[];
  overviewTotalLines?: number;
  bodyRef?: React.RefObject<HTMLElement | null>;
}) {
  const stats = diffStats(diff);
  const oversized = stats.bytes > MAX_HIGHLIGHT_BYTES;
  const visible = visibleHunks(diff.hunks, MAX_RENDERED_DIFF_LINES);
  const lang = languageFromPath(diff.path);
  const render = useLineHighlighter(lang, !diff.is_binary && !oversized);
  const markerByLine = new Map<number, FileOverviewMarker>();
  for (const marker of overviewMarkers) {
    if (!markerByLine.has(marker.line)) markerByLine.set(marker.line, marker);
  }
  const markedLines = new Set(markerByLine.keys());
  const { topLine, visibleLineCount, viewportHeight, scrollProgress } = useOverviewScrollSync(
    bodyRef,
    overviewTotalLines,
  );
  const overviewLines = overviewLinesFor(diff, stats.lines);
  const scrollPastEndStyle =
    viewportHeight > 0
      ? ({
          "--file-scroll-past-end": `${Math.max(0, viewportHeight - 18)}px`,
        } as CSSProperties)
      : undefined;

  if (diff.is_binary) {
    return (
      <div className="diff-view diff-view--binary" data-testid="diff-binary">
        Archivo binario: no hay una comparación de texto disponible.
      </div>
    );
  }

  return (
    <div
      className={`diff-view diff-view--${mode}`}
      data-testid="diff-view"
      style={scrollPastEndStyle}
    >
      <div className="diff-view__content">
        {oversized && (
          <div className="diff-view__notice" data-testid="diff-large">
            Comparación extensa: se desactivó el resaltado de sintaxis.
          </div>
        )}
        {stats.longLines > 0 && (
          <div className="diff-view__notice" data-testid="diff-long-lines">
            {stats.longLines.toLocaleString()}{" "}
            {stats.longLines === 1 ? "línea extensa" : "líneas extensas"} de la comparación se{" "}
            {stats.longLines === 1 ? "acortó" : "acortaron"} para mantener la vista fluida.
          </div>
        )}
        {visible.hiddenLines > 0 && (
          <div className="diff-view__notice" data-testid="diff-render-capped">
            Comparación extensa: se muestran las primeras {visible.renderedLines.toLocaleString()}{" "}
            de {stats.lines.toLocaleString()} líneas para mantener la vista fluida.
          </div>
        )}
        {diff.hunks.length === 0 ? (
          <div className="diff-view__notice" data-testid="diff-no-hunks">
            No hay cambios de texto.
          </div>
        ) : mode === "inline" ? (
          visible.hunks.map((h, i) => (
            <InlineHunk
              key={i}
              hunk={h}
              render={render}
              markedLines={markedLines}
              markerByLine={markerByLine}
            />
          ))
        ) : (
          visible.hunks.map((h, i) => (
            <SplitHunk
              key={i}
              hunk={h}
              render={render}
              markedLines={markedLines}
              markerByLine={markerByLine}
            />
          ))
        )}
      </div>
      <FileOverviewRuler
        markers={overviewMarkers}
        totalLines={overviewTotalLines}
        topLine={topLine}
        visibleLineCount={visibleLineCount}
        scrollProgress={scrollProgress}
        overviewLines={overviewLines}
        bodyRef={bodyRef}
        targetAttribute="data-new-line"
      />
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
  markerByLine,
}: {
  hunk: DiffHunk;
  render: RenderLine;
  markedLines: Set<number>;
  markerByLine: Map<number, FileOverviewMarker>;
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
          <code className="diff-content">{renderContent(l.content, render)}</code>
          {l.new_lineno != null && shouldShowMarkerLabel(markerByLine.get(l.new_lineno)) && (
            <LineMarkerLabel marker={markerByLine.get(l.new_lineno)!} />
          )}
        </div>
      ))}
    </div>
  );
}

function SplitHunk({
  hunk,
  render,
  markedLines,
  markerByLine,
}: {
  hunk: DiffHunk;
  render: RenderLine;
  markedLines: Set<number>;
  markerByLine: Map<number, FileOverviewMarker>;
}) {
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
              <code className="diff-content">{renderContent(l.content, render)}</code>
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
              <code className="diff-content">{renderContent(l.content, render)}</code>
              {l.new_lineno != null && shouldShowMarkerLabel(markerByLine.get(l.new_lineno)) && (
                <LineMarkerLabel marker={markerByLine.get(l.new_lineno)!} />
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function LineMarkerLabel({ marker }: { marker: FileOverviewMarker }) {
  const source = marker.source ?? "alert";
  return (
    <span
      className={`line-marker-label line-marker-label--${source} line-marker-label--${marker.severity}`}
      title={`${marker.label} · línea ${marker.line}`}
    >
      <span className="line-marker-label__compact" aria-hidden="true">
        {source === "hunk" ? "~" : source === "search" ? "?" : "!"}
      </span>
      <span className="line-marker-label__text">{marker.label}</span>
    </span>
  );
}

function shouldShowMarkerLabel(marker: FileOverviewMarker | undefined): boolean {
  return !!marker && marker.showLabel !== false;
}
