// Fixed top bar (outside the dock area): brand, workbench switcher, add/auto-
// detect actions, and a watching indicator. Switching swaps data; the layout is
// untouched (handled by the workspace).

import { useBusState } from "../bus/store";
import { enableNotifications, disableNotifications } from "../qol/notifications";
import { ALL_REPOS, qualityStore, useQualityState } from "../qol/state";
import { useWorkspaceActions } from "../workspace/actions";
import { addRepoFlow, autodetectFlow, switchWorkbench } from "./operations";

export function TopBar() {
  const { config, watching } = useBusState();
  const quality = useQualityState();
  const { openTimeline } = useWorkspaceActions();
  const active = config?.active ?? "";
  const workbenches = config?.workbenches ?? [];
  const activeWorkbench = workbenches.find((workbench) => workbench.name === active);
  const repos = activeWorkbench?.repos ?? [];
  const { filters } = quality;

  return (
    <div className="top-bar">
      <span className="top-bar__brand">Tinto</span>

      <select
        className="top-bar__switcher"
        data-testid="wb-switcher"
        value={active}
        onChange={(e) => void switchWorkbench(e.target.value, active || null)}
      >
        {workbenches.map((w) => (
          <option key={w.name} value={w.name}>
            {w.name}
          </option>
        ))}
      </select>

      <div className="top-bar__filters" aria-label="Quality filters">
        <input
          className="top-bar__search"
          data-testid="qol-search"
          value={filters.search}
          placeholder="Search repos, files, signals"
          onChange={(event) => qualityStore.setFilters({ search: event.target.value })}
        />
        <select
          data-testid="qol-repo"
          value={filters.repo}
          onChange={(event) => qualityStore.setFilters({ repo: event.target.value })}
        >
          <option value={ALL_REPOS}>All repos</option>
          {repos.map((repo) => (
            <option key={repo.path} value={repo.path}>
              {repo.alias ?? repo.path.split(/[/\\]/).filter(Boolean).pop() ?? repo.path}
            </option>
          ))}
        </select>
        <input
          className="top-bar__ext"
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

      <span className="top-bar__spacer" />

      <button data-testid="add-repo" onClick={() => active && void addRepoFlow(active)}>
        Add repo
      </button>
      <button data-testid="autodetect" onClick={() => active && void autodetectFlow(active)}>
        Auto-detect
      </button>
      <button data-testid="open-timeline" onClick={openTimeline}>
        Timeline
      </button>
      <button
        data-testid="qol-glance"
        aria-pressed={quality.glanceMode}
        onClick={() => qualityStore.setGlanceMode(!quality.glanceMode)}
      >
        {quality.glanceMode ? "Workspace" : "Glance"}
      </button>
      <button
        data-testid="qol-notifications"
        aria-pressed={quality.notificationsEnabled}
        title={quality.notificationMessage ?? quality.notificationStatus}
        onClick={() =>
          quality.notificationsEnabled ? disableNotifications() : void enableNotifications()
        }
      >
        {quality.notificationsEnabled ? "Notify on" : "Notify"}
      </button>
      {quality.notificationStatus === "denied" || quality.notificationStatus === "unavailable" ? (
        <span className="top-bar__notice" data-testid="qol-notification-status">
          {quality.notificationStatus}
        </span>
      ) : null}

      <span
        className={watching.available ? "watch watch--ok" : "watch watch--bad"}
        title={watching.reason ?? "watching"}
        data-testid="watch-indicator"
      >
        {watching.available ? "● watching" : "○ degraded"}
      </span>
    </div>
  );
}
