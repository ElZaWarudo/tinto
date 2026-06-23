import { useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { useOverviewScrollSync } from "./useOverviewScrollSync";

export interface FileOverviewMarker {
  line: number;
  severity: "critical" | "warning" | "info";
  label: string;
  source?: "alert" | "hunk" | "search";
}

const MAX_MINI_ROWS = 600;

function sourceLabel(source: FileOverviewMarker["source"]): string {
  switch (source ?? "alert") {
    case "hunk":
      return "Change";
    case "search":
      return "Search";
    case "alert":
      return "Alert";
  }
}

function markerGlyph(source: FileOverviewMarker["source"]): string {
  switch (source ?? "alert") {
    case "hunk":
      return "+";
    case "search":
      return "?";
    case "alert":
      return "!";
  }
}

function lineToPercent(line: number, totalLines: number): number {
  if (totalLines <= 1) return 0;
  return ((Math.max(1, Math.min(line, totalLines)) - 1) / (totalLines - 1)) * 100;
}

function lineFromClientY(clientY: number, rect: DOMRect, totalLines: number): number {
  if (totalLines <= 1 || rect.height <= 0) return 1;
  const progress = Math.max(0, Math.min(1, (clientY - rect.top) / rect.height));
  return Math.max(1, Math.min(totalLines, Math.round(progress * (totalLines - 1)) + 1));
}

function findLineElement(
  root: ParentNode,
  targetAttribute: "data-line" | "data-new-line",
  line: number,
): HTMLElement | null {
  const candidates = root.querySelectorAll<HTMLElement>(`[${targetAttribute}="${line}"]`);
  return [...candidates].find((candidate) => !candidate.closest(".file-overview-ruler")) ?? null;
}

export function FileOverviewRuler({
  markers,
  totalLines,
  targetAttribute,
}: {
  markers: FileOverviewMarker[];
  totalLines: number;
  targetAttribute: "data-line" | "data-new-line";
}) {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [activeLine, setActiveLine] = useState<number | null>(null);
  const { topLine, viewportPercent, viewportSizePercent } = useOverviewScrollSync(
    rootRef,
    totalLines,
    targetAttribute,
  );

  const sorted = useMemo(
    () =>
      [...markers]
        .filter((marker) => marker.line > 0)
        .sort(
          (a, b) => a.line - b.line || sourceLabel(a.source).localeCompare(sourceLabel(b.source)),
        ),
    [markers],
  );
  const alertCount = sorted.filter((marker) => (marker.source ?? "alert") === "alert").length;
  const sampledRows = useMemo(() => {
    if (totalLines <= 0) return [];
    const count = Math.min(totalLines, MAX_MINI_ROWS);
    return Array.from({ length: count }, (_, index) =>
      Math.max(1, Math.round((index / Math.max(1, count - 1)) * (totalLines - 1)) + 1),
    );
  }, [totalLines]);

  if (totalLines <= 0) return null;

  const jumpToLine = (line: number) => {
    const root = rootRef.current?.closest(".file-view__body") ?? document;
    const lineEl = findLineElement(root, targetAttribute, line);
    lineEl?.scrollIntoView({ block: "center", inline: "nearest" });
    setActiveLine(line);
  };

  const handleTrackClick = (event: MouseEvent<HTMLDivElement>) => {
    const line = lineFromClientY(
      event.clientY,
      event.currentTarget.getBoundingClientRect(),
      totalLines,
    );
    jumpToLine(line);
  };

  const handleTrackKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    let nextLine: number | null = null;
    if (event.key === "ArrowDown") nextLine = Math.min(totalLines, topLine + 1);
    if (event.key === "ArrowUp") nextLine = Math.max(1, topLine - 1);
    if (event.key === "Home") nextLine = 1;
    if (event.key === "End") nextLine = totalLines;
    if (nextLine == null) return;
    event.preventDefault();
    jumpToLine(nextLine);
  };

  return (
    <div
      ref={rootRef}
      className={`file-overview-ruler${sorted.length === 0 ? " file-overview-ruler--empty" : ""}`}
      aria-label="File overview"
      data-testid="file-overview-ruler"
    >
      <div className="file-overview-ruler__legend" data-testid="overview-summary">
        {alertCount > 0 ? `${alertCount}` : "0"}
        <span className="sr-only">
          {alertCount === 1 ? "1 file alert" : `${alertCount} file alerts`}
        </span>
      </div>
      <div
        tabIndex={0}
        className="file-overview-ruler__track"
        aria-label={`File overview, top line ${topLine} of ${totalLines}`}
        aria-valuemin={1}
        aria-valuemax={totalLines}
        aria-valuenow={topLine}
        aria-valuetext={`Line ${topLine} of ${totalLines}`}
        role="slider"
        onClick={handleTrackClick}
        onKeyDown={handleTrackKeyDown}
      >
        <span className="file-overview-ruler__mini" aria-hidden="true">
          {sampledRows.map((line, index) => (
            <span
              key={`${line}:${index}`}
              className="file-overview-ruler__mini-line"
              style={{ top: `${lineToPercent(line, totalLines)}%` }}
            />
          ))}
        </span>
        <span
          className="file-overview-ruler__viewport"
          style={{
            height: `${viewportSizePercent}%`,
            transform: `translate3d(0, ${viewportPercent}%, 0)`,
          }}
          aria-hidden="true"
        />
        {sorted.map((marker, index) => {
          const source = marker.source ?? "alert";
          const top = lineToPercent(marker.line, totalLines);
          const active = activeLine === marker.line;
          return (
            <span
              key={`${source}:${marker.severity}:${marker.line}:${index}`}
              role="button"
              tabIndex={0}
              className={`file-overview-ruler__mark file-overview-ruler__mark--${marker.severity} file-overview-ruler__mark--${source}${
                active ? " file-overview-ruler__mark--active" : ""
              }`}
              style={{ top: `${top}%` }}
              title={`${marker.label} - line ${marker.line}`}
              aria-label={`${marker.label}, line ${marker.line}`}
              data-marker-line={marker.line}
              data-testid={`overview-marker-${marker.line}-${index}`}
              onClick={(event) => {
                event.stopPropagation();
                jumpToLine(marker.line);
              }}
              onKeyDown={(event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                event.stopPropagation();
                jumpToLine(marker.line);
              }}
            >
              <span className="file-overview-ruler__mark-icon" aria-hidden="true">
                {markerGlyph(source)}
              </span>
              <span className="file-overview-ruler__mark-label">
                {sourceLabel(source)} L{marker.line}
              </span>
            </span>
          );
        })}
      </div>
    </div>
  );
}
