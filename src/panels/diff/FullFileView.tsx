import { type CSSProperties, useEffect, useMemo, useState } from "react";
import { getFileContent } from "../../bus/client";
import type { FileContent } from "../../bus/contract";
import { FileOverviewRuler, type FileOverviewMarker } from "../file/FileOverviewRuler";
import { useOverviewScrollSync } from "../file/useOverviewScrollSync";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { useLineHighlighter } from "./lineHighlighter";

export function FullFileView({
  repo,
  path,
  changedLines,
  overviewMarkers = [],
  bodyRef,
}: {
  repo: string;
  path: string;
  changedLines: Set<number>;
  overviewMarkers?: FileOverviewMarker[];
  bodyRef?: React.RefObject<HTMLElement | null>;
}) {
  const [content, setContent] = useState<FileContent | undefined>(undefined);
  const [error, setError] = useState(false);

  const lang = languageFromPath(path);
  const highlightable =
    content?.encoding === "utf8" && content.content.length <= MAX_HIGHLIGHT_BYTES;
  const renderLine = useLineHighlighter(lang, !!highlightable);

  useEffect(() => {
    let active = true;
    getFileContent(repo, path)
      .then((c) => {
        if (active) {
          setContent(c);
          setError(false);
        }
      })
      .catch(() => active && setError(true));
    return () => {
      active = false;
    };
  }, [repo, path]);

  const lines = useMemo(() => {
    if (!content || content.encoding !== "utf8") return [];
    const raw = content.content.split("\n");
    return raw.length > 1 && raw[raw.length - 1] === "" ? raw.slice(0, -1) : raw;
  }, [content]);

  const { topLine, visibleLineCount, viewportHeight, scrollProgress } = useOverviewScrollSync(
    bodyRef,
    lines.length,
  );
  const markerByLine = new Map<number, FileOverviewMarker>();
  for (const marker of overviewMarkers) {
    if (!markerByLine.has(marker.line)) markerByLine.set(marker.line, marker);
  }
  const markedLines = new Set(markerByLine.keys());
  const scrollPastEndStyle =
    viewportHeight > 0
      ? ({
          "--file-scroll-past-end": `${Math.max(0, viewportHeight - 18)}px`,
        } as CSSProperties)
      : undefined;

  if (error) {
    return (
      <div className="full-file full-file--error" data-testid="full-error">
        Could not load file.
      </div>
    );
  }
  if (content === undefined) {
    return (
      <div className="full-file full-file--loading" data-testid="full-loading">
        Loading…
      </div>
    );
  }
  if (content.encoding === "base64") {
    return (
      <div className="full-file full-file--binary" data-testid="full-binary">
        Binary file — cannot show full content.
      </div>
    );
  }

  return (
    <div className="full-file" data-testid="full-file">
      <FileOverviewRuler
        markers={overviewMarkers}
        totalLines={lines.length}
        topLine={topLine}
        visibleLineCount={visibleLineCount}
        scrollProgress={scrollProgress}
        overviewLines={lines}
        bodyRef={bodyRef}
        targetAttribute="data-line"
      />
      <pre className="full-file__code" style={scrollPastEndStyle}>
        {lines.map((line, i) => {
          const lineno = i + 1;
          const changed = changedLines.has(lineno);
          const marked = markedLines.has(lineno);
          const marker = markerByLine.get(lineno);
          const classes = ["full-file__line"];
          if (changed) classes.push("full-file__line--changed");
          if (marked) classes.push("full-file__line--signal-critical");
          return (
            <div key={i} className={classes.join(" ")} data-line={lineno}>
              <span className="diff-gutter">{lineno}</span>
              <code className="diff-content">{renderLine(line)}</code>
              {marker && <LineMarkerLabel marker={marker} />}
            </div>
          );
        })}
      </pre>
      {content.truncated && (
        <div className="diff-view__notice" data-testid="full-truncated">
          File truncated at the read limit — content beyond this point is not shown.
        </div>
      )}
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
      {marker.label}
    </span>
  );
}
