import { useEffect, useState } from "react";
import { busStore, getRepoSignals, signalCounts, statusSummary, useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";
import { filterRepoPaths } from "./filters";
import { useQualityState } from "./state";

function dirty(delta: ReturnType<typeof busStore.getState>["repos"][string]): boolean {
  const { modified, staged, untracked } = delta.status;
  return modified.length + staged.length + untracked.length > 0;
}

function latestActivityAge(activityMs: number, nowMs: number): string {
  if (!activityMs) return "sin actividad";
  const seconds = Math.max(0, Math.round((nowMs - activityMs) / 1000));
  if (seconds < 60) return `hace ${seconds}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `hace ${minutes} min`;
  return `hace ${Math.round(minutes / 60)} h`;
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
  const { openRepo } = useWorkspaceActions();
  const nowMs = useNow(30_000);
  const paths = filterRepoPaths(state, Object.keys(state.repos), filters, (repo) =>
    busStore.displayName(repo),
  );
  const deltas = paths.map((repo) => state.repos[repo]);
  const signals = deltas.flatMap((delta) => getRepoSignals(delta));
  const counts = signalCounts(signals);
  const latest = paths.reduce((max, repo) => Math.max(max, state.activity[repo] ?? 0), 0);
  const dirtyCount = deltas.filter(dirty).length;
  const orderedPaths = [...paths].sort((left, right) => {
    const leftCounts = signalCounts(getRepoSignals(state.repos[left]));
    const rightCounts = signalCounts(getRepoSignals(state.repos[right]));
    const byCritical = rightCounts.critical - leftCounts.critical;
    if (byCritical !== 0) return byCritical;
    const byDirty = Number(dirty(state.repos[right])) - Number(dirty(state.repos[left]));
    if (byDirty !== 0) return byDirty;
    return (state.activity[right] ?? 0) - (state.activity[left] ?? 0);
  });

  return (
    <main className="glance-mode" data-testid="glance-mode">
      <header className="glance-mode__status-band">
        <div className="glance-mode__title">
          <span className="glance-mode__eyebrow">Vista rápida</span>
          <h1>Estado de la workbench</h1>
          <p>Última actividad {latestActivityAge(latest, nowMs)}</p>
        </div>
        <dl className="glance-mode__summary" aria-label="Resumen de la workbench">
          <Summary label="Repos" value={paths.length} />
          <Summary label="Con cambios" value={dirtyCount} tone={dirtyCount ? "warn" : "ok"} />
          <Summary
            label="Críticas"
            value={counts.critical}
            tone={counts.critical ? "danger" : "ok"}
          />
          <Summary label="Avisos" value={counts.warning} tone={counts.warning ? "warn" : "ok"} />
          <Summary
            label="Supervisión"
            value={state.watching.available ? "En vivo" : "Degradado"}
            tone={state.watching.available ? "ok" : "warn"}
          />
        </dl>
      </header>

      <section className="glance-mode__queue" aria-labelledby="glance-queue-title">
        <header className="glance-mode__queue-head">
          <h2 id="glance-queue-title">Repositorios</h2>
          <span>{orderedPaths.length} visibles</span>
        </header>
        {orderedPaths.length === 0 ? (
          <p className="empty-state" data-testid="glance-no-matches">
            Ningún repositorio coincide con los filtros actuales.
          </p>
        ) : (
          <ul className="glance-mode__repos">
            {orderedPaths.map((repo) => {
              const delta = state.repos[repo];
              const repoSignals = signalCounts(getRepoSignals(delta));
              const hasChanges = dirty(delta);
              return (
                <li className="glance-repo" key={repo}>
                  <button type="button" onClick={() => openRepo(repo)}>
                    <span
                      className={`glance-repo__mark ${
                        repoSignals.critical
                          ? "glance-repo__mark--danger"
                          : hasChanges
                            ? "glance-repo__mark--warn"
                            : "glance-repo__mark--ok"
                      }`}
                      aria-hidden="true"
                    />
                    <span className="glance-repo__identity">
                      <strong>{busStore.displayName(repo)}</strong>
                      <small>{delta.branch?.name ?? "Sin rama"}</small>
                    </span>
                    <span className="glance-repo__git">{statusSummary(delta.status)}</span>
                    <span className="glance-repo__signals">
                      {repoSignals.critical
                        ? `${repoSignals.critical} críticas`
                        : repoSignals.warning
                          ? `${repoSignals.warning} avisos`
                          : "Sin señales"}
                    </span>
                    <span className="glance-repo__activity">
                      {latestActivityAge(state.activity[repo] ?? 0, nowMs)}
                    </span>
                    <span className="glance-repo__state">
                      {hasChanges ? "Con cambios" : "Limpio"}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}

function Summary({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string | number;
  tone?: "neutral" | "ok" | "warn" | "danger";
}) {
  return (
    <div className={`glance-summary glance-summary--${tone}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
