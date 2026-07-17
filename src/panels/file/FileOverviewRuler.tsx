import { type CSSProperties, useEffect, useState } from "react";

const MINIMAP_MIN_HEIGHT_PX = 96;
const MINIMAP_LINE_HEIGHT_PX = 4;
const MINIMAP_MAX_RENDERED_LINES = 600;
const MARKER_TARGET_SIZE_PX = 24;
const MARKER_SUMMARY_SPACE_PX = 20;

export interface FileOverviewMarker {
  line: number;
  severity: "critical" | "warning" | "info";
  label: string;
  source?: "alert" | "hunk" | "search";
  showLabel?: boolean;
}

export function FileOverviewRuler({
  markers,
  totalLines,
  topLine = 1,
  visibleLineCount = 1,
  scrollProgress = 0,
  overviewLines = [],
  bodyRef,
  targetAttribute = "data-line",
  onActiveLineChange,
}: {
  markers: FileOverviewMarker[];
  totalLines: number;
  topLine?: number;
  visibleLineCount?: number;
  scrollProgress?: number;
  overviewLines?: string[];
  bodyRef?: React.RefObject<HTMLElement | null>;
  targetAttribute?: "data-line" | "data-new-line";
  onActiveLineChange?: (line: number | null) => void;
}) {
  const hasLines = totalLines > 0;
  const sorted = [...markers].sort((a, b) => a.line - b.line);
  const markerGroups = buildMarkerGroups(sorted);
  const markerTargets = buildMarkerTargets(sorted);

  const [activeLine, setActiveLine] = useState<number | null>(null);
  const [rulerHeight, setRulerHeight] = useState<number | null>(null);

  useEffect(() => {
    if (activeLine == null) return;
    if (topLine <= activeLine) return;

    const clearTimer = window.setTimeout(() => {
      setActiveLine(null);
      onActiveLineChange?.(null);
    }, 0);

    return () => window.clearTimeout(clearTimer);
  }, [topLine, activeLine, onActiveLineChange]);

  useEffect(() => {
    const body = bodyRef?.current;
    if (!body) return;

    const updateHeight = () => {
      setRulerHeight(Math.max(96, body.clientHeight - 12));
    };

    updateHeight();
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(updateHeight);
    ro?.observe(body);
    window.addEventListener("resize", updateHeight);
    return () => {
      ro?.disconnect();
      window.removeEventListener("resize", updateHeight);
    };
  }, [bodyRef]);

  const jumpToLine = (line: number) => {
    const root = bodyRef?.current ?? document;
    const lineEl = findLineTarget(root, targetAttribute, line);
    lineEl?.scrollIntoView({ block: "center", inline: "nearest" });
    setActiveLine(line);
    onActiveLineChange?.(line);
  };

  const clearActiveLine = () => {
    setActiveLine(null);
    onActiveLineChange?.(null);
  };

  const onTrackClick = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!bodyRef?.current || !hasLines) return;
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.height <= 0) return;
    const ratio = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
    const line = Math.max(1, Math.min(totalLines, Math.round(ratio * (totalLines - 1)) + 1));
    jumpToLine(line);
  };

  const onTrackKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!hasLines) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      jumpToLine(Math.min(totalLines, (activeLine ?? topLine) + 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      jumpToLine(Math.max(1, (activeLine ?? topLine) - 1));
    } else if (event.key === "PageDown") {
      event.preventDefault();
      jumpToLine(Math.min(totalLines, (activeLine ?? topLine) + visibleLineCount));
    } else if (event.key === "PageUp") {
      event.preventDefault();
      jumpToLine(Math.max(1, (activeLine ?? topLine) - visibleLineCount));
    } else if (event.key === "Home") {
      event.preventDefault();
      jumpToLine(1);
    } else if (event.key === "End") {
      event.preventDefault();
      jumpToLine(totalLines);
    } else if (event.key === "Escape" && activeLine != null) {
      event.preventDefault();
      clearActiveLine();
    }
  };

  const viewportHeightPercent = hasLines
    ? Math.min(100, Math.max(8, (visibleLineCount / totalLines) * 100))
    : 0;
  const viewportTopPercent = hasLines
    ? Math.min(
        100 - viewportHeightPercent,
        Math.max(0, scrollProgress * (100 - viewportHeightPercent)),
      )
    : 0;
  const minimapLines = hasLines ? buildMinimapLines(overviewLines, totalLines) : [];
  const desiredTrackHeight = Math.max(
    MINIMAP_MIN_HEIGHT_PX,
    totalLines * MINIMAP_LINE_HEIGHT_PX,
    markerTargets.length * MARKER_TARGET_SIZE_PX,
  );
  const availableTrackHeight =
    rulerHeight == null
      ? desiredTrackHeight
      : Math.max(1, rulerHeight - (markerGroups.length > 0 ? MARKER_SUMMARY_SPACE_PX : 0));
  const trackHeight = Math.min(desiredTrackHeight, availableTrackHeight);
  const markerPlacements = buildMarkerPlacements(markerTargets, totalLines, trackHeight);
  const viewportTopPx = (trackHeight * viewportTopPercent) / 100;
  const viewportHeightPx = (trackHeight * viewportHeightPercent) / 100;
  const currentLine = hasLines ? (activeLine ?? topLine) : 1;
  const rulerStyle = {
    ...(rulerHeight == null ? {} : { "--file-overview-ruler-height": `${rulerHeight}px` }),
    "--file-overview-ruler-track-height": `${trackHeight}px`,
  } as CSSProperties;

  return (
    <div
      className={`file-overview-ruler${
        markerGroups.length > 0 ? " file-overview-ruler--has-summary" : ""
      }${hasLines ? " file-overview-ruler--has-track" : " file-overview-ruler--empty"}`}
      aria-label="Marcas del archivo"
      data-testid="file-overview-ruler"
      style={rulerStyle}
    >
      {markerGroups.length > 0 && (
        <div
          className="file-overview-ruler__summary"
          title={markerGroups.map((group) => group.summary).join(" · ")}
          data-testid="overview-summary"
        >
          {markerGroups.map((group) => (
            <span
              key={group.key}
              className={`file-overview-ruler__summary-item file-overview-ruler__summary-item--${group.source}`}
              aria-label={group.summary}
            >
              <span className="file-overview-ruler__summary-icon" aria-hidden="true">
                {group.icon}
              </span>
              <span className="file-overview-ruler__summary-count">{group.count}</span>
            </span>
          ))}
          <span className="sr-only">{markerGroups.map((group) => group.summary).join(", ")}</span>
        </div>
      )}
      <div
        className="file-overview-ruler__track"
        role="slider"
        tabIndex={hasLines ? 0 : -1}
        aria-label="Posición de desplazamiento"
        aria-valuemin={1}
        aria-valuemax={hasLines ? totalLines : 1}
        aria-valuenow={currentLine}
        aria-valuetext={hasLines ? `Línea ${currentLine} de ${totalLines}` : undefined}
        onClick={onTrackClick}
        onKeyDown={onTrackKeyDown}
        data-testid="file-overview-ruler-track"
      >
        {hasLines && (
          <>
            <div
              className="file-overview-ruler__minimap"
              aria-hidden="true"
              data-testid="file-overview-ruler-minimap"
              style={{ "--overview-mini-lines": minimapLines.length } as CSSProperties}
            >
              {minimapLines.map((line, index) => (
                <div
                  key={index}
                  className={`file-overview-ruler__mini-line ${miniLineClass(line ?? "")}`}
                >
                  {line ?? ""}
                </div>
              ))}
            </div>
            <div
              className="file-overview-ruler__caret"
              aria-hidden="true"
              style={
                {
                  height: `${viewportHeightPx}px`,
                  transform: `translate3d(0, ${viewportTopPx}px, 0)`,
                } as CSSProperties
              }
              data-testid="file-overview-ruler-caret"
            />
          </>
        )}
      </div>
      {markerPlacements.length > 0 && (
        <div className="file-overview-ruler__markers" aria-label="Marcas navegables">
          {markerPlacements.map(({ marker, top, lineOffset, targetSize }, index) => {
            const source = marker.source ?? "alert";
            const markerText = `${marker.label} · L${marker.line}`;
            const isActive = activeLine === marker.line;
            return (
              <button
                key={`line:${marker.line}`}
                type="button"
                className={`file-overview-ruler__mark file-overview-ruler__mark--${marker.severity} file-overview-ruler__mark--${source}${
                  isActive ? " file-overview-ruler__mark--active" : ""
                }`}
                style={
                  {
                    top: `${top}px`,
                    "--overview-marker-line-offset": `${lineOffset}px`,
                    "--overview-marker-target-size": `${targetSize}px`,
                  } as CSSProperties
                }
                title={`${marker.label} · línea ${marker.line}`}
                aria-label={`${marker.label}, línea ${marker.line}`}
                data-testid={`overview-marker-${marker.line}-${index}`}
                data-marker-line={marker.line}
                data-marker-count={marker.count}
                data-source={source}
                data-sources={marker.sources.join(" ")}
                onClick={() => jumpToLine(marker.line)}
              >
                <span className="file-overview-ruler__mark-icon" aria-hidden="true">
                  {sourceIcon(source)}
                </span>
                <span className="file-overview-ruler__mark-label">{markerText}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

type MarkerSource = NonNullable<FileOverviewMarker["source"]>;
type MarkerSeverity = FileOverviewMarker["severity"];

interface FileOverviewMarkerTarget {
  line: number;
  severity: MarkerSeverity;
  label: string;
  source: MarkerSource;
  sources: MarkerSource[];
  count: number;
}

interface FileOverviewMarkerPlacement {
  marker: FileOverviewMarkerTarget;
  top: number;
  lineOffset: number;
  targetSize: number;
}

function buildMarkerTargets(markers: FileOverviewMarker[]): FileOverviewMarkerTarget[] {
  const byLine = new Map<
    number,
    {
      severity: MarkerSeverity;
      labels: string[];
      sources: Set<MarkerSource>;
      count: number;
    }
  >();

  for (const marker of markers) {
    const source = marker.source ?? "alert";
    const current = byLine.get(marker.line);
    if (!current) {
      byLine.set(marker.line, {
        severity: marker.severity,
        labels: [marker.label],
        sources: new Set([source]),
        count: 1,
      });
      continue;
    }

    current.count += 1;
    current.sources.add(source);
    if (!current.labels.includes(marker.label)) current.labels.push(marker.label);
    if (severityRank(marker.severity) > severityRank(current.severity)) {
      current.severity = marker.severity;
    }
  }

  return Array.from(byLine.entries())
    .sort(([lineA], [lineB]) => lineA - lineB)
    .map(([line, group]) => {
      const sources = (["alert", "hunk", "search"] as const).filter((source) =>
        group.sources.has(source),
      );
      return {
        line,
        severity: group.severity,
        label: group.labels.join("; "),
        source: sources[0] ?? "alert",
        sources,
        count: group.count,
      };
    });
}

function buildMarkerPlacements(
  markers: FileOverviewMarkerTarget[],
  totalLines: number,
  trackHeight: number,
): FileOverviewMarkerPlacement[] {
  if (markers.length === 0 || trackHeight <= 0) return [];

  // Keep every hit target independent. When the physical track cannot fit
  // 24px per unique line, use the largest non-overlapping size available.
  const targetSize = Math.min(MARKER_TARGET_SIZE_PX, trackHeight / markers.length);
  const halfTarget = targetSize / 2;
  const maxCenter = Math.max(halfTarget, trackHeight - halfTarget);
  const lineCenters = markers.map((marker) => {
    const ratio = (marker.line - 1) / Math.max(totalLines - 1, 1);
    return ratio * trackHeight;
  });
  const targetCenters = lineCenters.map((center) =>
    Math.min(maxCenter, Math.max(halfTarget, center)),
  );

  for (let index = 1; index < targetCenters.length; index += 1) {
    targetCenters[index] = Math.max(targetCenters[index], targetCenters[index - 1] + targetSize);
  }
  if (targetCenters.at(-1)! > maxCenter) {
    targetCenters[targetCenters.length - 1] = maxCenter;
    for (let index = targetCenters.length - 2; index >= 0; index -= 1) {
      targetCenters[index] = Math.min(targetCenters[index], targetCenters[index + 1] - targetSize);
    }
  }

  return markers.map((marker, index) => ({
    marker,
    top: targetCenters[index],
    lineOffset: lineCenters[index] - targetCenters[index],
    targetSize,
  }));
}

function severityRank(severity: MarkerSeverity): number {
  switch (severity) {
    case "critical":
      return 3;
    case "warning":
      return 2;
    case "info":
      return 1;
  }
}

function sourceIcon(source: FileOverviewMarker["source"]): string {
  switch (source ?? "alert") {
    case "hunk":
      return "~";
    case "search":
      return "?";
    case "alert":
      return "!";
  }
}

function buildMarkerGroups(markers: FileOverviewMarker[]): Array<{
  key: string;
  source: "alert" | "hunk" | "search";
  count: number;
  icon: string;
  summary: string;
}> {
  const counts = new Map<"alert" | "hunk" | "search", number>();
  for (const marker of markers) {
    const source = marker.source ?? "alert";
    counts.set(source, (counts.get(source) ?? 0) + 1);
  }
  return (["alert", "hunk", "search"] as const)
    .map((source) => {
      const count = counts.get(source) ?? 0;
      if (count === 0) return null;
      const label =
        source === "alert"
          ? count === 1
            ? "1 posible secreto"
            : `${count} posibles secretos`
          : source === "hunk"
            ? count === 1
              ? "1 fragmento"
              : `${count} fragmentos`
            : count === 1
              ? "1 resultado de búsqueda"
              : `${count} resultados de búsqueda`;
      return {
        key: source,
        source,
        count,
        icon: sourceIcon(source),
        summary: label,
      };
    })
    .filter((group): group is NonNullable<typeof group> => group != null);
}

function buildMinimapLines(overviewLines: string[], totalLines: number): string[] {
  if (totalLines <= 0) return [];
  const renderedLineCount = Math.min(totalLines, MINIMAP_MAX_RENDERED_LINES);
  if (renderedLineCount === totalLines) {
    return Array.from({ length: totalLines }, (_, index) => overviewLines[index] ?? "");
  }
  return Array.from({ length: renderedLineCount }, (_, index) => {
    const sourceIndex =
      renderedLineCount === 1
        ? 0
        : Math.round((index / (renderedLineCount - 1)) * (totalLines - 1));
    return overviewLines[sourceIndex] ?? "";
  });
}

function miniLineClass(line: string): string {
  const trimmed = line.trim();
  if (trimmed.startsWith("//") || trimmed.startsWith("/*") || trimmed.startsWith("*")) {
    return "file-overview-ruler__mini-line--comment";
  }
  if (
    /\b(import|export|const|let|var|function|async|return|if|else|await|class|type|interface)\b/.test(
      trimmed,
    )
  ) {
    return "file-overview-ruler__mini-line--keyword";
  }
  if (/['"`]/.test(trimmed)) return "file-overview-ruler__mini-line--string";
  if (/^[}\]);,]+$/.test(trimmed)) return "file-overview-ruler__mini-line--punctuation";
  return "file-overview-ruler__mini-line--plain";
}

function findLineTarget(
  root: HTMLElement | Document,
  targetAttribute: "data-line" | "data-new-line",
  line: number,
): HTMLElement | null {
  const candidates = root.querySelectorAll<HTMLElement>(`[${targetAttribute}="${line}"]`);
  return (
    Array.from(candidates).find((candidate) => !candidate.closest(".file-overview-ruler")) ?? null
  );
}
