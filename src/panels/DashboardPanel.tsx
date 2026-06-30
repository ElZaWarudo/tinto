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
import { agentAvailabilityKey } from "./agentAvailability";
import { RepoCard } from "./RepoCard";
import { DashboardFilters } from "./DashboardFilters";
import type { RepoDelta } from "../bus/contract";

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

const SKELETON_COUNT = 3;

function pendingRepoDelta(repo: string): RepoDelta {
  return {
    repo,
    revision: 0,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 0,
    error: {
      class: "transient",
      category: "loading",
      message: "Waiting for repo snapshot...",
    },
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    signals: [],
    secret_findings: [],
    subscribed_diffs: null,
  };
}

function mergeActiveConfigPaths(livePaths: string[], configuredPaths: string[]): string[] {
  const paths = new Set(livePaths);
  for (const path of configuredPaths) paths.add(path);
  return Array.from(paths).sort((a, b) =>
    busStore.displayName(a).localeCompare(busStore.displayName(b)),
  );
}

export function DashboardPanel() {
  const state = useBusState();
  const { repos, activity, watching, loaded } = state;
  const { filters } = useQualityState();
  const { openRepo, addRepo, openAgents, openAgentTerminal, removeRepo } = useWorkspaceActions();
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

  const activeConfig = (state.config?.workbenches ?? []).find(
    (w) => w.name === state.config?.active,
  );
  const repoEntries = new Map((activeConfig?.repos ?? []).map((repo) => [repo.path, repo]));
  const configuredRepoEntries = new Map(
    (state.config?.workbenches ?? []).flatMap((workbench) =>
      workbench.repos.map((repo) => [repo.path, repo] as const),
    ),
  );
  const allPaths = mergeActiveConfigPaths(
    sortedRepoPaths(busStore, state),
    (activeConfig?.repos ?? []).map((repo) => repo.path),
  );
  const effectiveRepos: Record<string, RepoDelta> = { ...repos };
  for (const path of allPaths) {
    effectiveRepos[path] ??= pendingRepoDelta(path);
  }
  const effectiveState = { ...state, repos: effectiveRepos };
  const paths = filterRepoPaths(effectiveState, allPaths, filters, (repo) =>
    busStore.displayName(repo),
  );

  return (
    <div className="dashboard">
      <DashboardFilters />
      <div className="dashboard__actions" aria-label="repo actions">
        <button type="button" onClick={addRepo} data-testid="dashboard-add-repo">
          Add repo
        </button>
        <button type="button" onClick={openAgents} data-testid="dashboard-open-agents">
          Agents
        </button>
      </div>

      {!watching.available && (
        <div className="banner banner--warn" data-testid="degraded-banner">
          Watching unavailable
          {watching.reason ? `: ${watching.reason}` : ""}. Data still loads on demand.
        </div>
      )}

      {allPaths.length === 0 ? (
        <div className="empty-state" data-testid="zero-repos">
          <p>No repos in this workbench.</p>
          <div className="empty-state__actions">
            <button onClick={addRepo}>Add repo</button>
          </div>
        </div>
      ) : paths.length === 0 && hasActiveFilters(filters) ? (
        <div className="empty-state" data-testid="dashboard-no-matches">
          <p>No repos match the current filters.</p>
        </div>
      ) : (
        <div className="card-grid">
          {paths.map((p) => {
            const entry = repoEntries.get(p) ?? configuredRepoEntries.get(p);
            return (
              <RepoCard
                key={p}
                delta={effectiveRepos[p]}
                name={busStore.displayName(p)}
                source={entry?.source}
                distro={entry?.distro ?? null}
                activityMs={activity[p] ?? 0}
                nowMs={nowMs}
                availabilityKey={agentAvailabilityKey(entry?.source, entry?.distro)}
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
            );
          })}
        </div>
      )}
    </div>
  );
}
