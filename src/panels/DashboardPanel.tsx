// Dashboard: a card grid of the active workbench's repos. Handles loading
// (skeletons), the degraded watching banner, the zero-repos state, and live
// updates (memoized cards re-render only on their own repo's change).

import { useEffect, useState } from "react";
import { retryRepo } from "../bus/client";
import { busStore, sortedRepoPaths, useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";
import { RepoCard } from "./RepoCard";

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
  const { openRepo, addRepo, openDiff } = useWorkspaceActions();
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

  const paths = sortedRepoPaths(busStore, state);

  return (
    <div className="dashboard">
      {!watching.available && (
        <div className="banner banner--warn" data-testid="degraded-banner">
          Watching unavailable
          {watching.reason ? `: ${watching.reason}` : ""}. Data still loads on demand.
        </div>
      )}

      {paths.length === 0 ? (
        <div className="empty-state" data-testid="zero-repos">
          <p>No repos in this workbench.</p>
          <button onClick={addRepo}>Add repo</button>
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
              onOpenFile={(path) => openDiff(p, path)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
