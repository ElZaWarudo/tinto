import type { PassiveSignal, RepoMetrics } from "../bus/contract";
import { sortSignals } from "../bus/store";

function signalKindLabel(kind: PassiveSignal["kind"]): string {
  switch (kind) {
    case "sensitive_path":
      return "Archivo sensible";
    case "possible_secret":
      return "Posible secreto";
    case "large_delete":
      return "Borrado grande";
    case "config_change":
      return "Configuración";
    case "test_change":
      return "Pruebas";
  }
}

function severityLabel(severity: PassiveSignal["severity"]): string {
  if (severity === "critical") return "Crítica";
  if (severity === "warning") return "Advertencia";
  return "Información";
}

export function SignalBadges({
  signals,
  limit = 3,
  compact = false,
}: {
  signals: PassiveSignal[];
  limit?: number;
  compact?: boolean;
}) {
  const shown = sortSignals(signals).slice(0, limit);
  if (shown.length === 0) return null;
  const hidden = signals.length - shown.length;
  return (
    <span className={compact ? "signal-badges signal-badges--compact" : "signal-badges"}>
      {shown.map((signal, index) => (
        <span
          key={`${signal.kind}:${signal.path ?? "repo"}:${index}`}
          className={`signal-chip signal-chip--${signal.severity}`}
          title={[severityLabel(signal.severity), signal.message, signal.path]
            .filter(Boolean)
            .join(": ")}
          aria-label={`${severityLabel(signal.severity)}: ${signalKindLabel(signal.kind)}`}
          data-testid={`signal-${signal.kind}`}
        >
          <span className="signal-chip__severity">{severityLabel(signal.severity)}</span>
          <span>{signalKindLabel(signal.kind)}</span>
        </span>
      ))}
      {hidden > 0 && (
        <span className="signal-chip signal-chip--more" title={`${hidden} señales más`}>
          +{hidden}
        </span>
      )}
    </span>
  );
}

export function MetricsPill({ metrics }: { metrics: RepoMetrics }) {
  return (
    <span className="metrics-pill" data-testid="repo-metrics">
      {metrics.changed_files} archivos · +{metrics.lines_added} -{metrics.lines_removed}
    </span>
  );
}
