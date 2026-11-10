// Quality filter bar for the Dashboard (search / extension / time window).
// These are the global filters (they also scope the in-project explorers); the
// Dashboard is their natural home now that it is the project launcher.

import { busStore } from "../bus/store";
import { ALL_REPOS, qualityStore, useQualityState } from "../qol/state";

export function DashboardFilters() {
  const { filters } = useQualityState();
  return (
    <div className="dashboard__filters" aria-label="Filtros del resumen">
      <label className="dashboard-filter dashboard-filter--search">
        <span>Buscar</span>
        <input
          className="dashboard__search"
          data-testid="qol-search"
          value={filters.search}
          placeholder="Proyecto, archivo o señal"
          onChange={(event) => qualityStore.setFilters({ search: event.target.value })}
        />
      </label>
      <label className="dashboard-filter dashboard-filter--extension">
        <span>Extensión</span>
        <input
          className="dashboard__ext"
          data-testid="qol-extension"
          value={filters.extension}
          placeholder=".ts"
          onChange={(event) => qualityStore.setFilters({ extension: event.target.value })}
        />
      </label>
      <label className="dashboard-filter dashboard-filter--time">
        <span>Periodo</span>
        <select
          data-testid="qol-time"
          value={filters.timeWindow}
          onChange={(event) =>
            qualityStore.setFilters({ timeWindow: event.target.value as typeof filters.timeWindow })
          }
        >
          <option value="all">Todo</option>
          <option value="15m">15 min</option>
          <option value="1h">1 hora</option>
          <option value="today">Hoy</option>
        </select>
      </label>
      <button type="button" data-testid="qol-reset" onClick={() => qualityStore.resetFilters()}>
        Restablecer
      </button>
      {/* Set from the explorer's "filter by folder"; without this it hides repos unseen. */}
      {filters.repo !== ALL_REPOS && (
        <div className="dashboard-filter dashboard-filter--repo">
          <span>Proyecto</span>
          <button
            type="button"
            data-testid="qol-repo"
            aria-label={`Quitar el filtro del proyecto ${busStore.displayName(filters.repo)}`}
            onClick={() => qualityStore.setFilters({ repo: ALL_REPOS })}
          >
            {busStore.displayName(filters.repo)} ✕
          </button>
        </div>
      )}
    </div>
  );
}
