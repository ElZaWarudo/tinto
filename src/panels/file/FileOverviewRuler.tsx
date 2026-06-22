export interface FileOverviewMarker {
  line: number;
  severity: "critical" | "warning" | "info";
  label: string;
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
  if (markers.length === 0 || totalLines <= 0) return null;

  const sorted = [...markers].sort((a, b) => a.line - b.line);
  const criticalCount = sorted.filter((marker) => marker.severity === "critical").length;
  const summary =
    criticalCount === 1
      ? "1 posible secreto en este archivo"
      : `${criticalCount} posibles secretos en este archivo`;

  const jumpToLine = (line: number, target: EventTarget | null) => {
    const root = target instanceof HTMLElement ? target.closest(".file-view__body") : document;
    const lineEl = root?.querySelector(`[${targetAttribute}="${line}"]`);
    lineEl?.scrollIntoView({ block: "center", inline: "nearest" });
  };

  return (
    <div
      className={`file-overview-ruler${
        criticalCount > 0 ? " file-overview-ruler--has-summary" : ""
      }`}
      aria-label="Marcas del archivo"
    >
      {criticalCount > 0 && (
        <div
          className="file-overview-ruler__summary"
          title={summary}
          data-testid="overview-summary"
        >
          <span className="file-overview-ruler__summary-icon" aria-hidden="true">
            !
          </span>
          <span className="file-overview-ruler__summary-count">{criticalCount}</span>
          <span className="sr-only">{summary}</span>
        </div>
      )}
      <div className="file-overview-ruler__track">
        {sorted.map((marker, index) => {
          const top = totalLines <= 1 ? 0 : ((marker.line - 1) / (totalLines - 1)) * 100;
          const markerText = `${marker.label} · L${marker.line}`;
          return (
            <button
              key={`${marker.severity}:${marker.line}:${index}`}
              type="button"
              className={`file-overview-ruler__mark file-overview-ruler__mark--${marker.severity}`}
              style={{ top: `${top}%` }}
              title={`${marker.label} · línea ${marker.line}`}
              aria-label={`${marker.label}, línea ${marker.line}`}
              data-testid={`overview-marker-${marker.line}`}
              onClick={(event) => jumpToLine(marker.line, event.currentTarget)}
            >
              <span className="file-overview-ruler__mark-icon" aria-hidden="true">
                !
              </span>
              <span className="file-overview-ruler__mark-label">{markerText}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
