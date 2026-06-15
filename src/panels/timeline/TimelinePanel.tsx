import { useCallback, useEffect, useMemo, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { getCommitDiff, getCommitLog } from "../../bus/client";
import type { CommitInfo, FileDiff } from "../../bus/contract";
import { busStore, commitDate, sortedRepoPaths, useBusState } from "../../bus/store";
import { DiffView } from "../diff/DiffView";
import { buildTimelineEntries, TIMELINE_COMMIT_LIMIT } from "./model";

interface CmdError {
  message: string;
  category: string;
}

interface CommitEntry {
  repo: string;
  repoName: string;
  commit: CommitInfo;
}

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

export function TimelinePanel(props: IDockviewPanelProps) {
  void props;
  const state = useBusState();
  const nowMs = useNow(30_000);
  const repoPaths = useMemo(() => sortedRepoPaths(busStore, state), [state]);
  const repoKey = repoPaths.join("\0");
  const repoSet = useMemo(() => new Set(repoPaths), [repoPaths]);

  const [commits, setCommits] = useState<Record<string, CommitInfo[]>>({});
  const [logError, setLogError] = useState<string | null>(null);
  const [selected, setSelected] = useState<CommitEntry | null>(null);
  const [diffs, setDiffs] = useState<FileDiff[] | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [diffError, setDiffError] = useState<CmdError | null>(null);
  const [loadingDiff, setLoadingDiff] = useState(false);

  useEffect(() => {
    let active = true;
    void Promise.all(
      repoPaths.map((repo) =>
        getCommitLog(repo, 0, TIMELINE_COMMIT_LIMIT).then((items) => [repo, items] as const),
      ),
    )
      .then((pairs) => {
        if (!active) return;
        setCommits(Object.fromEntries(pairs));
        setLogError(null);
      })
      .catch((e) => {
        if (active) setLogError(asCmdError(e).message);
      });
    return () => {
      active = false;
    };
  }, [repoKey, repoPaths]);

  const activityEntries = useMemo(
    () => buildTimelineEntries(state, (repo) => busStore.displayName(repo), nowMs),
    [state, nowMs],
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
      .sort((a, b) => b.commit.timestamp - a.commit.timestamp);
  }, [commits, repoPaths]);

  const activeSelected = selected && repoSet.has(selected.repo) ? selected : null;

  const loadCommitDiff = useCallback((entry: CommitEntry) => {
    setSelected(entry);
    setDiffs(null);
    setSelectedPath(null);
    setDiffError(null);
    setLoadingDiff(true);
    getCommitDiff(entry.repo, entry.commit.id)
      .then((items) => {
        setDiffs(items);
        setSelectedPath(items[0]?.path ?? null);
      })
      .catch((e) => setDiffError(asCmdError(e)))
      .finally(() => setLoadingDiff(false));
  }, []);

  const selectedDiff = diffs?.find((d) => d.path === selectedPath) ?? null;

  return (
    <div className="timeline" data-testid="timeline-panel">
      <section className="timeline__feed">
        <header className="timeline__head">
          <h2>Timeline</h2>
          {state.watching.available ? (
            <span className="timeline__live">live</span>
          ) : (
            <span className="timeline__degraded" data-testid="timeline-degraded">
              degraded
            </span>
          )}
        </header>

        {repoPaths.length === 0 ? (
          <p className="repo-panel__muted" data-testid="timeline-empty">
            No repos in this workbench.
          </p>
        ) : (
          <ul className="timeline-list" data-testid="timeline-feed">
            {activityEntries.map((entry) => (
              <li className={`timeline-row timeline-row--${entry.kind}`} key={entry.id}>
                <span className="timeline-row__time">
                  {new Date(entry.timestampMs).toLocaleTimeString()}
                </span>
                <span className="timeline-row__main">
                  <strong>{entry.repoName}</strong> · {entry.title}
                  <small>{entry.detail}</small>
                </span>
              </li>
            ))}
            {commitEntries.map((entry) => (
              <li
                className="timeline-row timeline-row--commit"
                key={`${entry.repo}:${entry.commit.id}`}
              >
                <span className="timeline-row__time">
                  {commitDate(entry.commit.timestamp).toLocaleTimeString()}
                </span>
                <button
                  className="timeline-row__commit"
                  onClick={() => loadCommitDiff(entry)}
                  data-testid={`timeline-commit-${entry.commit.id}`}
                >
                  <strong>{entry.repoName}</strong> · commit {entry.commit.summary}
                  <small>
                    {entry.commit.author} · {entry.commit.id.slice(0, 8)}
                  </small>
                </button>
              </li>
            ))}
          </ul>
        )}

        {logError && (
          <p className="timeline__error" data-testid="timeline-log-error">
            Could not load commits: {logError}
          </p>
        )}
      </section>

      <section className="timeline__detail" data-testid="timeline-detail">
        {!activeSelected ? (
          <p className="repo-panel__muted">Select a commit to inspect its diff.</p>
        ) : (
          <>
            <header className="timeline__detail-head">
              <h3>{activeSelected.commit.summary}</h3>
              <span>
                {activeSelected.repoName} · {activeSelected.commit.id.slice(0, 8)}
              </span>
            </header>

            {loadingDiff ? (
              <p className="repo-panel__muted" data-testid="timeline-diff-loading">
                Loading commit diff…
              </p>
            ) : diffError ? (
              <div className="timeline__error" data-testid="timeline-diff-error">
                {diffError.category}: {diffError.message}
                <button onClick={() => loadCommitDiff(activeSelected)}>Retry</button>
              </div>
            ) : diffs && diffs.length === 0 ? (
              <p className="repo-panel__muted" data-testid="timeline-no-diffs">
                No file diffs for this commit.
              </p>
            ) : diffs ? (
              <div className="timeline-diff">
                <ul className="timeline-files" data-testid="timeline-files">
                  {diffs.map((diff) => (
                    <li key={diff.path}>
                      <button
                        className={diff.path === selectedPath ? "timeline-files__btn--on" : ""}
                        onClick={() => setSelectedPath(diff.path)}
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
  );
}
