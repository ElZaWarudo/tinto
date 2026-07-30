// Dashboard: a control-table ledger of the active workbench's repos. Handles loading
// (skeletons), the degraded watching banner, the zero-repos state, and live
// updates (memoized rows re-render only on their own repo's change).

import { useEffect, useRef, useState } from "react";
import { listAgentSessions, retryRepo, startAgentSession } from "../bus/client";
import { agentSessionStore } from "../agent/sessionStore";
import {
  busStore,
  getRepoMetrics,
  getRepoSignals,
  sortedRepoPaths,
  useBusState,
} from "../bus/store";
import { filterRepoPaths, hasActiveFilters } from "../qol/filters";
import { useQualityState } from "../qol/state";
import { useWorkspaceActions } from "../workspace/actions";
import { fetchRepoFlow } from "../workbench/operations";
import { agentAvailabilityKey } from "./agentAvailability";
import { ACTIVITY_WINDOW_MS } from "./constants";
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
const DELTA_ANNOUNCEMENT_DELAY_MS = 200;

interface LoadingRepoPreview {
  path: string;
  name: string;
}

function DashboardLoading({
  activeWorkbench,
  repos,
}: {
  activeWorkbench: string | null;
  repos: LoadingRepoPreview[];
}) {
  const previewRepos = repos.length
    ? repos.slice(0, 6)
    : Array.from({ length: SKELETON_COUNT }, (_, index) => ({
        path: `loading-${index}`,
        name: "Repositorio",
      }));

  return (
    <div className="dashboard dashboard--loading" aria-busy="true">
      <div
        className="dashboard__loading-preview"
        data-testid="dashboard-loading-preview"
        aria-hidden="true"
      >
        <div className="dashboard__status-band">
          <div className="dashboard__status-title">
            <span className="dashboard__status-label">Bitácora del workbench</span>
            <strong>{activeWorkbench ?? "Workbench activo"}</strong>
          </div>
          <dl className="dashboard__counters">
            {[
              ["Repos", repos.length],
              ["Activos", 0],
              ["Archivos", 0],
              ["Señales", 0],
              ["Bloqueados", 0],
            ].map(([label, value]) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
          <div className="dashboard__loading-actions">
            <span />
            <span />
          </div>
        </div>
        <div className="dashboard__loading-filters">
          <span />
          <span />
          <span />
          <span />
        </div>
        <div className="dashboard__ledger-ruler">
          <span>Repo / rama</span>
          <span>Estado de Git</span>
          <span>Volumen de cambios</span>
          <span>Señales</span>
          <span>Inicio de Agent</span>
        </div>
        <div className="repo-ledger repo-ledger--loading" data-testid="skeletons">
          {previewRepos.map((repo) => (
            <div className="dashboard__loading-repo" key={repo.path}>
              <span className="dashboard__loading-state" />
              <strong>{repo.name}</strong>
              <span />
              <span />
              <span />
              <span />
            </div>
          ))}
        </div>
      </div>
      <div
        className="dashboard__loading-overlay"
        data-testid="dashboard-loading-overlay"
        role="status"
        aria-live="polite"
      >
        <div className="dashboard__loading-indicator">
          <span className="dashboard__loading-spinner" aria-hidden="true" />
          <strong>Cargando repos</strong>
        </div>
      </div>
    </div>
  );
}

function pendingRepoDelta(repo: string): RepoDelta {
  return {
    repo,
    revision: 0,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 0,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
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

function materialDeltaFingerprint(delta: RepoDelta): string {
  return JSON.stringify({
    status: delta.status,
    branch: delta.branch,
    head: delta.head?.id ?? null,
    error: delta.error,
    metrics: delta.metrics,
    signals: delta.signals,
  });
}

function repoDeltaAnnouncement(delta: RepoDelta): string {
  const name = busStore.displayName(delta.repo);
  if (delta.error) {
    return `${name} requiere atención por ${delta.error.category}.`;
  }
  const changedFiles = new Set([
    ...delta.status.modified,
    ...delta.status.staged,
    ...delta.status.untracked,
  ]).size;
  const signalCount = getRepoSignals(delta).length;
  const fileLabel = changedFiles === 1 ? "archivo con cambios" : "archivos con cambios";
  const signalLabel = signalCount === 1 ? "señal" : "señales";
  return `${name} actualizado: ${changedFiles} ${fileLabel} y ${signalCount} ${signalLabel}.`;
}

interface DeltaAnnouncement {
  sequence: number;
  text: string;
}

function useDeltaAnnouncement(
  repos: Record<string, RepoDelta>,
  activeWorkbench: string | null,
  snapshotReady: boolean,
): DeltaAnnouncement | null {
  const [announcement, setAnnouncement] = useState<DeltaAnnouncement | null>(null);
  const previousFingerprints = useRef<Map<string, string> | null>(null);
  const baselineWorkbench = useRef<string | null | undefined>(undefined);
  const awaitingSnapshotBaseline = useRef(true);
  const pending = useRef(new Map<string, RepoDelta>());
  const flushTimer = useRef<number | null>(null);
  const announcementSequence = useRef(0);

  useEffect(() => {
    const currentFingerprints = new Map(
      Object.entries(repos).map(([repo, delta]) => [repo, materialDeltaFingerprint(delta)]),
    );
    const workbenchChanged = baselineWorkbench.current !== activeWorkbench;
    baselineWorkbench.current = activeWorkbench;
    if (!snapshotReady) awaitingSnapshotBaseline.current = true;
    if (!snapshotReady || awaitingSnapshotBaseline.current || workbenchChanged) {
      if (snapshotReady) awaitingSnapshotBaseline.current = false;
      previousFingerprints.current = currentFingerprints;
      pending.current.clear();
      if (flushTimer.current !== null) {
        window.clearTimeout(flushTimer.current);
        flushTimer.current = null;
      }
      setAnnouncement(null);
      return;
    }
    const previous = previousFingerprints.current;
    previousFingerprints.current = currentFingerprints;
    if (previous === null) return;

    for (const repo of pending.current.keys()) {
      if (!currentFingerprints.has(repo)) pending.current.delete(repo);
    }
    let hasNewMaterialDelta = false;
    for (const [repo, delta] of Object.entries(repos)) {
      if (previous.get(repo) !== currentFingerprints.get(repo)) {
        pending.current.set(repo, delta);
        hasNewMaterialDelta = true;
      }
    }
    if (pending.current.size === 0) {
      if (flushTimer.current !== null) {
        window.clearTimeout(flushTimer.current);
        flushTimer.current = null;
      }
      return;
    }
    if (!hasNewMaterialDelta) return;

    if (flushTimer.current !== null) window.clearTimeout(flushTimer.current);
    flushTimer.current = window.setTimeout(() => {
      const summaries = Array.from(pending.current.values())
        .sort((a, b) => busStore.displayName(a.repo).localeCompare(busStore.displayName(b.repo)))
        .map(repoDeltaAnnouncement);
      pending.current.clear();
      flushTimer.current = null;
      announcementSequence.current += 1;
      setAnnouncement({
        sequence: announcementSequence.current,
        text: summaries.join(" "),
      });
    }, DELTA_ANNOUNCEMENT_DELAY_MS);
  }, [activeWorkbench, repos, snapshotReady]);

  useEffect(
    () => () => {
      if (flushTimer.current !== null) window.clearTimeout(flushTimer.current);
    },
    [],
  );

  return announcement;
}

export function DashboardPanel() {
  const state = useBusState();
  const { repos, activity, watching, loaded } = state;
  const { filters } = useQualityState();
  const { openRepo, addRepo, openAgents, openAgentTerminal, removeRepo } = useWorkspaceActions();
  const nowMs = useNow(1000);
  const deltaAnnouncement = useDeltaAnnouncement(
    repos,
    state.config?.active ?? null,
    state.snapshotStatus === "ready",
  );

  const activeConfig = (state.config?.workbenches ?? []).find(
    (workbench) => workbench.name === state.config?.active,
  );

  if (!loaded) {
    return (
      <DashboardLoading
        activeWorkbench={state.config?.active ?? null}
        repos={(activeConfig?.repos ?? []).map((repo) => ({
          path: repo.path,
          name: busStore.displayName(repo.path),
        }))}
      />
    );
  }

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
  const allDeltas = allPaths.map((path) => effectiveRepos[path]);
  const activeCount = allPaths.filter(
    (path) => nowMs - (activity[path] ?? 0) < ACTIVITY_WINDOW_MS,
  ).length;
  const changedFileCount = allDeltas.reduce(
    (sum, repo) => sum + getRepoMetrics(repo).changed_files,
    0,
  );
  const signalCount = allDeltas.reduce((sum, repo) => sum + getRepoSignals(repo).length, 0);
  const blockedCount = allDeltas.filter((repo) => repo.error).length;

  return (
    <div className="dashboard">
      <div className="dashboard__status-band" aria-label="Estado del workbench">
        <div className="dashboard__status-title">
          <span className="dashboard__status-label">Bitácora del workbench</span>
          <strong>{state.config?.active ?? "Workbench activo"}</strong>
          {!watching.available && <small>Supervisión local degradada</small>}
        </div>
        <dl className="dashboard__counters">
          <div>
            <dt>Repos</dt>
            <dd>{allPaths.length}</dd>
          </div>
          <div>
            <dt>Activos</dt>
            <dd>{activeCount}</dd>
          </div>
          <div>
            <dt>Archivos</dt>
            <dd>{changedFileCount}</dd>
          </div>
          <div>
            <dt>Señales</dt>
            <dd>{signalCount}</dd>
          </div>
          <div className={blockedCount > 0 ? "dashboard__counter--warn" : undefined}>
            <dt>Bloqueados</dt>
            <dd>{blockedCount}</dd>
          </div>
        </dl>
        <div className="dashboard__actions" aria-label="Acciones de repos">
          <button type="button" onClick={addRepo} data-testid="dashboard-add-repo">
            Añadir repo
          </button>
          <button type="button" onClick={openAgents} data-testid="dashboard-open-agents">
            Abrir Agents
          </button>
        </div>
      </div>

      <DashboardFilters />

      <p
        aria-atomic="true"
        aria-live="polite"
        className="sr-only"
        data-testid="dashboard-delta-announcement"
        role="status"
      >
        {deltaAnnouncement ? (
          <span data-testid="dashboard-delta-announcement-message" key={deltaAnnouncement.sequence}>
            {deltaAnnouncement.text}
          </span>
        ) : null}
      </p>

      {!watching.available && (
        <div
          className="banner banner--warn"
          data-testid="degraded-banner"
          role="status"
          aria-live="polite"
        >
          La supervisión en vivo no está disponible
          {watching.reason ? `: ${watching.reason}` : ""}. Los datos siguen disponibles bajo
          demanda.
        </div>
      )}

      {allPaths.length === 0 ? (
        <div className="empty-state" data-testid="zero-repos">
          <p>Este workbench todavía no tiene repos.</p>
          <div className="empty-state__actions">
            <button onClick={addRepo}>Añadir repo</button>
          </div>
        </div>
      ) : paths.length === 0 && hasActiveFilters(filters) ? (
        <div className="empty-state" data-testid="dashboard-no-matches">
          <p>Ningún repo coincide con los filtros actuales.</p>
        </div>
      ) : (
        <>
          <div className="dashboard__ledger-ruler" aria-hidden="true">
            <span>Repo / rama</span>
            <span>Estado de Git</span>
            <span>Volumen de cambios</span>
            <span>Señales</span>
            <span>Inicio de Agent</span>
          </div>
          <div className="repo-ledger">
            {paths.map((p) => {
              const entry = repoEntries.get(p) ?? configuredRepoEntries.get(p);
              return (
                <RepoCard
                  key={p}
                  delta={effectiveRepos[p]}
                  pending={!repos[p]}
                  name={busStore.displayName(p)}
                  source={entry?.source}
                  distro={entry?.distro ?? null}
                  activityMs={activity[p] ?? 0}
                  nowMs={nowMs}
                  availabilityKey={agentAvailabilityKey(entry?.source, entry?.distro)}
                  onOpen={() => openRepo(p)}
                  onRetry={() => void retryRepo(p)}
                  onRemove={() => removeRepo(p)}
                  onFetch={() => fetchRepoFlow(p)}
                  onLaunch={async (agentType, permissionMode) => {
                    const sessionId = await startAgentSession(p, agentType, permissionMode);
                    const sessions = await listAgentSessions();
                    agentSessionStore.setSessions(sessions);
                    openAgentTerminal({ sessionId, repo: p, agentType });
                  }}
                />
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
