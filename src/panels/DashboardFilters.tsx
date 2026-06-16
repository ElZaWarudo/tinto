// Quality filter bar for the Dashboard (search / extension / time window).
// These are the global filters (they also scope the in-project explorers); the
// Dashboard is their natural home now that it is the project launcher.

import { qualityStore, useQualityState } from "../qol/state";

export function DashboardFilters() {
  const { filters } = useQualityState();
  return (
    <div className="dashboard__filters" aria-label="Quality filters">
      <input
        className="dashboard__search"
        data-testid="qol-search"
        value={filters.search}
        placeholder="Search projects, files, signals"
        onChange={(event) => qualityStore.setFilters({ search: event.target.value })}
      />
      <input
        className="dashboard__ext"
        data-testid="qol-extension"
        value={filters.extension}
        placeholder=".ts"
        aria-label="extension filter"
        onChange={(event) => qualityStore.setFilters({ extension: event.target.value })}
      />
      <select
        data-testid="qol-time"
        value={filters.timeWindow}
        onChange={(event) =>
          qualityStore.setFilters({ timeWindow: event.target.value as typeof filters.timeWindow })
        }
      >
        <option value="all">All time</option>
        <option value="15m">15 min</option>
        <option value="1h">1 hour</option>
        <option value="today">Today</option>
      </select>
      <button type="button" data-testid="qol-reset" onClick={() => qualityStore.resetFilters()}>
        Reset
      </button>
    </div>
  );
}
