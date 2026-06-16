import { useEffect, useState } from "react";
import { busStore, getRepoSignals, signalCounts, useBusState } from "../bus/store";
import { useQualityState } from "./state";
import { filterRepoPaths } from "./filters";

function dirty(delta: ReturnType<typeof busStore.getState>["repos"][string]): boolean {
  const { modified, staged, untracked } = delta.status;
  return modified.length + staged.length + untracked.length > 0;
}

function latestActivityAge(activityMs: number, nowMs: number): string {
  if (!activityMs) return "none";
  const seconds = Math.max(0, Math.round((nowMs - activityMs) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.round(minutes / 60)}h ago`;
}

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

export function GlanceMode() {
  const state = useBusState();
  const { filters } = useQualityState();
  const nowMs = useNow(30_000);
  const paths = filterRepoPaths(state, Object.keys(state.repos), filters, (repo) =>
    busStore.displayName(repo),
  );
  const deltas = paths.map((repo) => state.repos[repo]);
  const signals = deltas.flatMap((delta) => getRepoSignals(delta));
  const counts = signalCounts(signals);
  const latest = paths.reduce((max, repo) => Math.max(max, state.activity[repo] ?? 0), 0);

  return (
    <main className="glance-mode" data-testid="glance-mode">
      <section className="glance-mode__hero">
        <span className="glance-mode__eyebrow">Tinto glance</span>
        <h1>{deltas.filter(dirty).length} dirty repos</h1>
        <p>
          {paths.length} repos visible · latest activity {latestActivityAge(latest, nowMs)}
        </p>
      </section>

      <section className="glance-mode__cards" aria-label="Workbench summary">
        <Metric label="Repos" value={paths.length} />
        <Metric label="Dirty" value={deltas.filter(dirty).length} tone="warn" />
        <Metric label="Critical" value={counts.critical} tone="danger" />
        <Metric label="Warnings" value={counts.warning} tone="warn" />
        <Metric
          label="Watcher"
          value={state.watching.available ? "live" : "degraded"}
          tone={state.watching.available ? "ok" : "warn"}
        />
      </section>

      <section className="glance-mode__repos">
        {paths.length === 0 ? (
          <p className="empty-state" data-testid="glance-no-matches">
            No repos match the current filters.
          </p>
        ) : (
          paths.map((repo) => (
            <article className="glance-repo" key={repo}>
              <strong>{busStore.displayName(repo)}</strong>
              <span>{dirty(state.repos[repo]) ? "dirty" : "clean"}</span>
            </article>
          ))
        )}
      </section>
    </main>
  );
}

function Metric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string | number;
  tone?: "neutral" | "ok" | "warn" | "danger";
}) {
  return (
    <div className={`glance-metric glance-metric--${tone}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
