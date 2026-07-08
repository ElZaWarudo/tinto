import { type CSSProperties, useEffect, useMemo, useState } from "react";
import type { FileContent } from "../../bus/contract";
import { fileLoadErrorMessage, loadFileContentWithRetry } from "../file/fileContentLoader";
import { FileOverviewRuler, type FileOverviewMarker } from "../file/FileOverviewRuler";
import { useOverviewScrollSync } from "../file/useOverviewScrollSync";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { useLineHighlighter } from "./lineHighlighter";

interface LoadedFileContent {
  key: string;
  content: FileContent;
}

export function FullFileView({
  repo,
  path,
  repoRevision,
  changedLines,
  overviewMarkers = [],
  bodyRef,
}: {
  repo: string;
  path: string;
  repoRevision?: number;
  changedLines: Set<number>;
  overviewMarkers?: FileOverviewMarker[];
  bodyRef?: React.RefObject<HTMLElement | null>;
}) {
  const [loaded, setLoaded] = useState<LoadedFileContent | null>(null);
  const [error, setError] = useState<{ key: string; message: string } | null>(null);
  const [reloadToken, setReloadToken] = useState(0);
  const requestKey = `${repo}\0${path}\0${repoRevision ?? "unknown"}\0${reloadToken}`;
  const content = loaded?.key === requestKey ? loaded.content : undefined;
  const errorMessage = error?.key === requestKey ? error.message : null;

  const lang = languageFromPath(path);
  const highlightable =
    content?.encoding === "utf8" && content.content.length <= MAX_HIGHLIGHT_BYTES;
  const renderLine = useLineHighlighter(lang, !!highlightable);

  useEffect(() => {
    let active = true;
    const key = requestKey;
    loadFileContentWithRetry(repo, path)
      .then((c) => {
        if (active) {
          setLoaded({ key, content: c });
          setError(null);
        }
      })
      .catch((cause) => active && setError({ key, message: fileLoadErrorMessage(cause) }));
    return () => {
      active = false;
    };
  }, [repo, path, reloadToken, requestKey]);

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

  if (errorMessage) {
    return (
      <div className="full-file full-file--error" data-testid="full-error">
        <span>Could not load file: {errorMessage}</span>
        <button type="button" onClick={() => setReloadToken((token) => token + 1)}>
          Retry
        </button>
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
              {shouldShowMarkerLabel(marker) && <LineMarkerLabel marker={marker!} />}
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
