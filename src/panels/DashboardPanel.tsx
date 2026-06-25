// Dashboard: a card grid of the active workbench's repos. Handles loading
// (skeletons), the degraded watching banner, the zero-repos state, and live
// updates (memoized cards re-render only on their own repo's change).

import { useEffect, useState } from "react";
import { listAgentSessions, retryRepo, startAgentSession } from "../bus/client";
import { agentSessionStore } from "../agent/sessionStore";
import { busStore, sortedRepoPaths, useBusState } from "../bus/store";
import { filterRepoPaths, hasActiveFilters } from "../qol/filters";
import { useQualityState } from "../qol/state";
import { useWorkspaceActions } from "../workspace/actions";
import { RepoCard } from "./RepoCard";
import { DashboardFilters } from "./DashboardFilters";

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

const SKELETON_COUNT = 3;

export function DashboardPanel() {
  const state = useBusState();
  const { repos, activity, watching, loaded } = state;
  const { filters } = useQualityState();
  const { openRepo, addRepo, openAgentTerminal, removeRepo } = useWorkspaceActions();
  const nowMs = useNow(1000);

  if (!loaded) {
    return (
      <div className="dashboard">
        <div className="card-grid" data-testid="skeletons">
          {Array.from({ length: SKELETON_COUNT }).map((_, i) => (
            <div className="repo-card repo-card--skeleton" key={i} />
          ))}
        </div>
      </div>
    );
  }

  const allPaths = sortedRepoPaths(busStore, state);
  const paths = filterRepoPaths(state, allPaths, filters, (repo) => busStore.displayName(repo));

  return (
    <div className="dashboard">
      <DashboardFilters />

      {!watching.available && (
        <div className="banner banner--warn" data-testid="degraded-banner">
          Watching unavailable
          {watching.reason ? `: ${watching.reason}` : ""}. Data still loads on demand.
        </div>
      )}

      {allPaths.length === 0 ? (
        <div className="empty-state" data-testid="zero-repos">
          <p>No repos in this workbench.</p>
          <button onClick={addRepo}>Add repo</button>
        </div>
      ) : paths.length === 0 && hasActiveFilters(filters) ? (
        <div className="empty-state" data-testid="dashboard-no-matches">
          <p>No repos match the current filters.</p>
        </div>
      ) : (
        <div className="card-grid">
          {paths.map((p) => (
            <RepoCard
              key={p}
              delta={repos[p]}
              name={busStore.displayName(p)}
              activityMs={activity[p] ?? 0}
              nowMs={nowMs}
              onOpen={() => openRepo(p)}
              onRetry={() => void retryRepo(p)}
              onRemove={() => removeRepo(p)}
              onLaunch={async (agentType) => {
                const sessionId = await startAgentSession(p, agentType);
                const sessions = await listAgentSessions();
                agentSessionStore.setSessions(sessions);
                openAgentTerminal({ sessionId, repo: p, agentType });
              }}
            />
          ))}
        </div>
      )}
    </div>
  );
}
