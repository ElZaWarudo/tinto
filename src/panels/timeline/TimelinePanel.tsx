import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { getCommitDiff, getCommitLog } from "../../bus/client";
import type { CommitInfo, FileDiff } from "../../bus/contract";
import { busStore, commitDate, sortedRepoPaths, useBusState } from "../../bus/store";
import {
  filterRepoPaths,
  filterTimelineEntries,
  matchesExtension,
  matchesTimeWindow,
} from "../../qol/filters";
import { useQualityState } from "../../qol/state";
import { DiffView } from "../diff/DiffView";
import { buildTimelineEntries, TIMELINE_COMMIT_LIMIT, type TimelineEntry } from "./model";

interface CmdError {
  message: string;
  category: string;
}

interface CommitEntry {
  repo: string;
  repoName: string;
  commit: CommitInfo;
}

interface CommitLogTarget {
  repo: string;
  head: string;
}

type TimelineFeedEntry =
  | {
      id: string;
      type: "activity";
      timestampMs: number;
      activity: TimelineEntry;
    }
  | {
      id: string;
      type: "commit";
      timestampMs: number;
      commitEntry: CommitEntry;
    };

function asCmdError(e: unknown): CmdError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as Record<string, unknown>;
    return { category: String(o.category ?? "error"), message: String(o.message ?? e) };
  }
  return { category: "error", message: String(e) };
}

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

function encodeCommitLogTargets(targets: CommitLogTarget[]): string {
  return JSON.stringify(targets);
}

function decodeCommitLogTargets(key: string): CommitLogTarget[] {
  try {
    const parsed = JSON.parse(key) as CommitLogTarget[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function TimelinePanel(props: IDockviewPanelProps) {
  void props;
  const state = useBusState();
  const { filters } = useQualityState();
  const nowMs = useNow(30_000);
  const allRepoPaths = useMemo(() => sortedRepoPaths(busStore, state), [state]);
  const commitScopeFilters = useMemo(() => ({ ...filters, search: "" }), [filters]);
  const repoPaths = useMemo(
    () =>
      filterRepoPaths(state, allRepoPaths, commitScopeFilters, (repo) =>
        busStore.displayName(repo),
      ),
    [allRepoPaths, commitScopeFilters, state],
  );
  const repoSet = useMemo(() => new Set(repoPaths), [repoPaths]);
  const commitLogKey = encodeCommitLogTargets(
    repoPaths.map((repo) => ({
      repo,
      head: state.repos[repo]?.head?.id ?? "no-head",
    })),
  );

  const [commits, setCommits] = useState<Record<string, CommitInfo[]>>({});
  const [logError, setLogError] = useState<string | null>(null);
  const [loadingCommits, setLoadingCommits] = useState(false);
  const [logRetryRevision, setLogRetryRevision] = useState(0);
  const [resolvedCommitLogKey, setResolvedCommitLogKey] = useState<string | null>(null);
  const loadedCommitKeys = useRef(new Map<string, string>());
  const commitHistoryPending =
    loadingCommits || (!logError && repoPaths.length > 0 && resolvedCommitLogKey !== commitLogKey);
  const [selected, setSelected] = useState<CommitEntry | null>(null);
  const selectedCommitButtonRef = useRef<HTMLButtonElement | null>(null);
  const detailHeadingRef = useRef<HTMLHeadingElement | null>(null);
  const [diffs, setDiffs] = useState<FileDiff[] | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [diffError, setDiffError] = useState<CmdError | null>(null);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const diffRequestGeneration = useRef(0);

  useEffect(() => {
    let active = true;
    const targets = decodeCommitLogTargets(commitLogKey);
    const targetRepos = new Set(targets.map((target) => target.repo));
    for (const repo of Array.from(loadedCommitKeys.current.keys())) {
      if (!targetRepos.has(repo)) loadedCommitKeys.current.delete(repo);
    }
    const staleTargets = targets.filter(
      (target) => loadedCommitKeys.current.get(target.repo) !== target.head,
    );
    if (staleTargets.length === 0) {
      setCommits((current) => {
        const next = Object.fromEntries(
          Object.entries(current).filter(([repo]) => targetRepos.has(repo)),
        );
        return Object.keys(next).length === Object.keys(current).length ? current : next;
      });
      setLogError(null);
      setLoadingCommits(false);
      setResolvedCommitLogKey(commitLogKey);
      return () => {
        active = false;
      };
    }

    setCommits((current) =>
      Object.fromEntries(Object.entries(current).filter(([repo]) => targetRepos.has(repo))),
    );
    setLogError(null);
    setLoadingCommits(true);

    let pendingCount = staleTargets.length;
    const failures = Array<CmdError | null>(staleTargets.length).fill(null);
    const completeTarget = () => {
      pendingCount -= 1;
      if (!active || pendingCount > 0) return;
      const completedFailures = failures.filter((error): error is CmdError => error !== null);
      if (completedFailures.length === 0) {
        setLogError(null);
        setResolvedCommitLogKey(commitLogKey);
      } else {
        const suffix =
          completedFailures.length === 1 ? "" : ` (${completedFailures.length} repos fallaron)`;
        setLogError(`${completedFailures[0].message}${suffix}`);
      }
      setLoadingCommits(false);
    };

    staleTargets.forEach((target, targetIndex) => {
      void getCommitLog(target.repo, 0, TIMELINE_COMMIT_LIMIT).then(
        (items) => {
          if (!active) return;
          setCommits((current) => ({
            ...Object.fromEntries(
              Object.entries(current).filter(([repo]) => targetRepos.has(repo)),
            ),
            [target.repo]: items,
          }));
          loadedCommitKeys.current.set(target.repo, target.head);
          completeTarget();
        },
        (error) => {
          if (!active) return;
          failures[targetIndex] = asCmdError(error);
          completeTarget();
        },
      );
    });
    return () => {
      active = false;
    };
  }, [commitLogKey, logRetryRevision]);

  const activityEntries = useMemo(
    () =>
      filterTimelineEntries(
        buildTimelineEntries(state, (repo) => busStore.displayName(repo), nowMs),
        filters,
        nowMs,
      ),
    [filters, state, nowMs],
  );

  const commitEntries = useMemo<CommitEntry[]>(() => {
    return repoPaths
      .flatMap((repo) =>
        (commits[repo] ?? []).map((commit) => ({
          repo,
          repoName: busStore.displayName(repo),
          commit,
        })),
      )
      .filter((entry) => {
        if (!matchesTimeWindow(entry.commit.timestamp * 1000, filters, nowMs)) return false;
        const text = [
          entry.repoName,
          entry.repo,
          entry.commit.summary,
          entry.commit.author,
          entry.commit.id,
        ]
          .join(" ")
          .toLowerCase();
        const needle = filters.search.trim().toLowerCase();
        return !needle || text.includes(needle);
      })
      .sort((a, b) => b.commit.timestamp - a.commit.timestamp);
  }, [commits, filters, nowMs, repoPaths]);

  const feedEntries = useMemo<TimelineFeedEntry[]>(
    () =>
      [
        ...activityEntries.map((activity) => ({
          id: `activity:${activity.id}`,
          type: "activity" as const,
          timestampMs: activity.timestampMs,
          activity,
        })),
        ...commitEntries.map((commitEntry) => ({
          id: `commit:${commitEntry.repo}:${commitEntry.commit.id}`,
          type: "commit" as const,
          timestampMs: commitEntry.commit.timestamp * 1000,
          commitEntry,
        })),
      ].sort((a, b) => b.timestampMs - a.timestampMs || a.id.localeCompare(b.id)),
    [activityEntries, commitEntries],
  );

  const activeSelected = selected && repoSet.has(selected.repo) ? selected : null;

  useEffect(() => {
    if (activeSelected) detailHeadingRef.current?.focus();
  }, [activeSelected]);

  const loadCommitDiff = useCallback((entry: CommitEntry) => {
    const generation = ++diffRequestGeneration.current;
    setSelected(entry);
    setDiffs(null);
    setSelectedPath(null);
    setDiffError(null);
    setLoadingDiff(true);
    getCommitDiff(entry.repo, entry.commit.id)
      .then((items) => {
        if (generation !== diffRequestGeneration.current) return;
        setDiffs(items);
        setSelectedPath(items[0]?.path ?? null);
      })
      .catch((e) => {
        if (generation === diffRequestGeneration.current) setDiffError(asCmdError(e));
      })
      .finally(() => {
        if (generation === diffRequestGeneration.current) setLoadingDiff(false);
      });
  }, []);

  const filteredDiffs = useMemo(
    () => (diffs ?? []).filter((diff) => matchesExtension(diff.path, filters)),
    [diffs, filters],
  );
  const selectedDiff =
    filteredDiffs.find((d) => d.path === selectedPath) ?? filteredDiffs[0] ?? null;

  const closeCommitDetail = useCallback(() => {
    diffRequestGeneration.current += 1;
    const selectedButton = selectedCommitButtonRef.current;
    setSelected(null);
    setDiffs(null);
    setSelectedPath(null);
    setDiffError(null);
    setLoadingDiff(false);
    window.setTimeout(() => selectedButton?.focus(), 0);
  }, []);

  return (
    <div className="timeline" data-testid="timeline-panel">
      <div className={`timeline__layout${activeSelected ? " timeline__layout--detail" : ""}`}>
        <section
          aria-busy={commitHistoryPending}
          aria-label="Entradas de la cronología"
          className="timeline__feed"
        >
          <header className="timeline__head">
            <h2>Cronología</h2>
            {state.watching.available ? (
              <span className="timeline__live">en vivo</span>
            ) : (
              <span className="timeline__degraded" data-testid="timeline-degraded">
                degradado
              </span>
            )}
          </header>

          {allRepoPaths.length === 0 ? (
            <p className="repo-panel__muted" data-testid="timeline-empty">
              No hay repositorios en esta workbench.
            </p>
          ) : commitHistoryPending && feedEntries.length === 0 ? (
            <p
              aria-live="polite"
              className="timeline__status"
              data-testid="timeline-commits-loading"
              role="status"
            >
              Cargando historial de commits…
            </p>
          ) : !logError && feedEntries.length === 0 ? (
            <p className="repo-panel__muted" data-testid="timeline-no-matches">
              Ninguna entrada de la cronología coincide con los filtros actuales.
            </p>
          ) : feedEntries.length > 0 ? (
            <ul className="timeline-list" data-testid="timeline-feed">
              {feedEntries.map((feedEntry) => {
                if (feedEntry.type === "activity") {
                  const { activity } = feedEntry;
                  return (
                    <li
                      className={`timeline-row timeline-row--${activity.kind}`}
                      key={feedEntry.id}
                    >
                      <span className="timeline-row__time">
                        {new Date(activity.timestampMs).toLocaleTimeString()}
                      </span>
                      <span className="timeline-row__main">
                        <strong>{activity.repoName}</strong> · {activity.title}
                        <small>{activity.detail}</small>
                      </span>
                    </li>
                  );
                }

                const { commitEntry } = feedEntry;
                const isSelected =
                  activeSelected?.repo === commitEntry.repo &&
                  activeSelected.commit.id === commitEntry.commit.id;
                return (
                  <li
                    className={`timeline-row timeline-row--commit${
                      isSelected ? " timeline-row--selected" : ""
                    }`}
                    key={feedEntry.id}
                  >
                    <span className="timeline-row__time">
                      {commitDate(commitEntry.commit.timestamp).toLocaleTimeString()}
                    </span>
                    <button
                      aria-controls="timeline-detail"
                      aria-current={isSelected ? "true" : undefined}
                      className="timeline-row__commit"
                      onClick={(event) => {
                        selectedCommitButtonRef.current = event.currentTarget;
                        loadCommitDiff(commitEntry);
                      }}
                      data-testid={`timeline-commit-${commitEntry.commit.id}`}
                      type="button"
                    >
                      <strong>{commitEntry.repoName}</strong> · commit {commitEntry.commit.summary}
                      <small>
                        {commitEntry.commit.author} · {commitEntry.commit.id.slice(0, 8)}
                      </small>
                    </button>
                  </li>
                );
              })}
            </ul>
          ) : null}

          {commitHistoryPending && feedEntries.length > 0 && (
            <p
              aria-live="polite"
              className="timeline__status"
              data-testid="timeline-commits-refreshing"
              role="status"
            >
              Actualizando historial de commits…
            </p>
          )}

          {logError && (
            <div className="timeline__error" data-testid="timeline-log-error" role="alert">
              <span>No se pudieron cargar los commits: {logError}</span>
              <button
                aria-label="Reintentar la carga del historial de commits"
                onClick={() => {
                  setLogError(null);
                  setLoadingCommits(true);
                  setLogRetryRevision((revision) => revision + 1);
                }}
                type="button"
              >
                Reintentar
              </button>
            </div>
          )}
        </section>

        <section
          aria-busy={loadingDiff}
          aria-label={activeSelected ? undefined : "Detalle del commit"}
          aria-labelledby={activeSelected ? "timeline-detail-title" : undefined}
          className="timeline__detail"
          data-testid="timeline-detail"
          id="timeline-detail"
        >
          {!activeSelected ? (
            <p className="repo-panel__muted">Sin commit seleccionado.</p>
          ) : (
            <>
              <header className="timeline__detail-head">
                <button className="timeline__back" onClick={closeCommitDetail} type="button">
                  Volver a la cronología
                </button>
                <h3 id="timeline-detail-title" ref={detailHeadingRef} tabIndex={-1}>
                  {activeSelected.commit.summary}
                </h3>
                <span>
                  {activeSelected.repoName} · {activeSelected.commit.id.slice(0, 8)}
                </span>
              </header>

              {loadingDiff ? (
                <p
                  className="repo-panel__muted"
                  data-testid="timeline-diff-loading"
                  role="status"
                  aria-live="polite"
                >
                  Cargando diff del commit…
                </p>
              ) : diffError ? (
                <div className="timeline__error" data-testid="timeline-diff-error" role="alert">
                  {diffError.category}: {diffError.message}
                  <button onClick={() => loadCommitDiff(activeSelected)}>Reintentar</button>
                </div>
              ) : diffs && diffs.length === 0 ? (
                <p className="repo-panel__muted" data-testid="timeline-no-diffs">
                  Este commit no contiene diffs de archivos.
                </p>
              ) : diffs && filteredDiffs.length === 0 ? (
                <p className="repo-panel__muted" data-testid="timeline-no-diff-matches">
                  Ningún diff del commit coincide con los filtros actuales.
                </p>
              ) : diffs ? (
                <div className="timeline-diff">
                  <ul className="timeline-files" data-testid="timeline-files">
                    {filteredDiffs.map((diff) => (
                      <li key={diff.path}>
                        <button
                          aria-pressed={diff.path === selectedDiff?.path}
                          className={
                            diff.path === selectedDiff?.path ? "timeline-files__btn--on" : ""
                          }
                          onClick={() => setSelectedPath(diff.path)}
                          type="button"
                        >
                          {diff.path}
                        </button>
                      </li>
                    ))}
                  </ul>
                  {selectedDiff && <DiffView diff={selectedDiff} mode="inline" />}
                </div>
              ) : null}
            </>
          )}
        </section>
      </div>
    </div>
  );
}
