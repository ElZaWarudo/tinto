import type { PassiveSignal, RepoMetrics } from "../bus/contract";
import { sortSignals } from "../bus/store";

function signalKindLabel(kind: PassiveSignal["kind"]): string {
  switch (kind) {
    case "sensitive_path":
      return "Sensitive file";
    case "possible_secret":
      return "Possible secret";
    case "large_delete":
      return "Large delete";
    case "config_change":
      return "Config";
    case "test_change":
      return "Tests";
  }
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
          title={[signal.message, signal.path].filter(Boolean).join(": ")}
          data-testid={`signal-${signal.kind}`}
        >
          {signalKindLabel(signal.kind)}
        </span>
      ))}
      {hidden > 0 && (
        <span className="signal-chip signal-chip--more" title={`${hidden} more signal(s)`}>
          +{hidden}
        </span>
      )}
    </span>
  );
}

export function MetricsPill({ metrics }: { metrics: RepoMetrics }) {
  return (
    <span className="metrics-pill" data-testid="repo-metrics">
      {metrics.changed_files} files · +{metrics.lines_added} -{metrics.lines_removed}
    </span>
  );
}
