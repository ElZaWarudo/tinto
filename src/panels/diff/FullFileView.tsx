// Full-file view (RDM-008, R4): the file's CURRENT working-tree content with
// changed lines highlighted in context. Degrades per R4: base64/binary → guard
// placeholder; truncated → content up to the 1 MiB cut + a notice (no highlight
// past the cut). Best-effort under mid-edit content/diff skew (no correctness
// guarantee on the exact highlighted line during an active write).

import { useEffect, useState } from "react";
import { getFileContent } from "../../bus/client";
import type { FileContent } from "../../bus/contract";
import { FileOverviewRuler, type FileOverviewMarker } from "../file/FileOverviewRuler";
import { MAX_HIGHLIGHT_BYTES, languageFromPath } from "./highlight";
import { useLineHighlighter } from "./lineHighlighter";

export function FullFileView({
  repo,
  path,
  changedLines,
  overviewMarkers = [],
}: {
  repo: string;
  path: string;
  changedLines: Set<number>;
  overviewMarkers?: FileOverviewMarker[];
}) {
  const [content, setContent] = useState<FileContent | undefined>(undefined);
  const [error, setError] = useState(false);

  // Highlighting layers on after the text paints; disabled for binary content,
  // oversized files, and unknown languages (the hook falls back to plain).
  const lang = languageFromPath(path);
  const highlightable =
    content?.encoding === "utf8" && content.content.length <= MAX_HIGHLIGHT_BYTES;
  const renderLine = useLineHighlighter(lang, !!highlightable);

  // A diff panel is one (repo, path) target, so this fetches once on mount; the
  // initial `undefined` state IS the loading state (no synchronous reset needed).
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

  const raw = content.content.split("\n");
  // Drop the spurious trailing empty line from a file ending in a newline.
  const lines = raw.length > 1 && raw[raw.length - 1] === "" ? raw.slice(0, -1) : raw;
  const markedLines = new Set(overviewMarkers.map((marker) => marker.line));
  return (
    <div className="full-file" data-testid="full-file">
      <FileOverviewRuler
        markers={overviewMarkers}
        totalLines={lines.length}
        targetAttribute="data-line"
      />
      <pre className="full-file__code">
        {lines.map((line, i) => {
          const lineno = i + 1;
          const changed = changedLines.has(lineno);
          const marked = markedLines.has(lineno);
          const classes = ["full-file__line"];
          if (changed) classes.push("full-file__line--changed");
          if (marked) classes.push("full-file__line--signal-critical");
          return (
            <div key={i} className={classes.join(" ")} data-line={lineno}>
              <span className="diff-gutter">{lineno}</span>
              <code className="diff-content">{renderLine(line)}</code>
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
