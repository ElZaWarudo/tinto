// Per-repo project panel: a level-1 dockview tab laid out like a mini IDE — the
// repo's own file explorer on the left, and on the right a NESTED dockview that
// holds the open file panels. Because files are real dockview panels they can be
// dragged into splits, so two files can sit on screen at once. A single click in
// the explorer previews a file (italic tab); a double click pins it. When no
// files are open the project's overview ("Resumen") is shown instead.

import { useEffect, useState } from "react";
import { DockviewReact, themeVisualStudio } from "dockview-react";
import type { DockviewReadyEvent, IDockviewPanelProps } from "dockview-react";
import { getCommitLog, retryRepo } from "../bus/client";
import type { CommitInfo, RepoDelta } from "../bus/contract";
import {
  busStore,
  commitDate,
  getFsEvents,
  getPathSignals,
  getRepoMetrics,
  getRepoSignals,
  sortSignals,
  useBusState,
} from "../bus/store";
import { filterFsEvents, filterStatusFiles } from "../qol/filters";
import { useQualityState } from "../qol/state";
import { useWorkspaceActions } from "../workspace/actions";
import { fileDock, useRepoDock } from "../workspace/fileDock";
import { updateRepoFsWatch } from "../workbench/operations";
import { MetricsPill, SignalBadges } from "./SignalBadges";
import { WatchedFilesSection } from "./WatchedFilesSection";
import { FileView } from "./file/FileView";
import { FileTab } from "./FileTab";
import { ProjectExplorer } from "./tree/ProjectExplorer";
import { useExplorerCollapsed } from "./tree/explorerCollapseState";

const COMMIT_LOG_LIMIT = 30;

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

function FilePanel(props: IDockviewPanelProps<{ repo: string; path: string }>) {
  return <FileView repo={props.params.repo} path={props.params.path} />;
}

const fileComponents = { file: FilePanel };
const fileTabComponents = { fileTab: FileTab };

export function RepoPanel(props: IDockviewPanelProps<{ repo: string }>) {
  const repo = props.params.repo;
  const dock = useRepoDock(repo);
  const empty = dock.open.length === 0;
  const [explorerCollapsed, toggleExplorer] = useExplorerCollapsed(repo);

  // Drop the nested dock binding when the project tab unmounts.
  useEffect(() => () => fileDock.unregister(repo), [repo]);

  const onReady = (event: DockviewReadyEvent) => fileDock.register(repo, event.api);

  return (
    <div className="repo-panel" data-testid={`repo-panel-${repo}`}>
      <ProjectExplorer
        repo={repo}
        collapsed={explorerCollapsed}
        onToggleCollapse={toggleExplorer}
      />

      <div className="repo-panel__main">
        {/* The nested dock stays mounted (api stays registered) even when empty;
            hidden behind the overview until a file is opened. */}
        <div className="repo-panel__files" style={empty ? { display: "none" } : undefined}>
          <DockviewReact
            components={fileComponents}
            tabComponents={fileTabComponents}
            theme={themeVisualStudio}
            onReady={onReady}
          />
        </div>
        {empty && (
          <div className="repo-panel__overview-wrap" data-testid={`repo-overview-wrap-${repo}`}>
            <RepoOverview repo={repo} />
          </div>
        )}
      </div>
    </div>
  );
}

function RepoOverview({ repo }: { repo: string }) {
  const state = useBusState();
  const { filters } = useQualityState();
  const { repos } = state;
  const { removeRepo, openFile } = useWorkspaceActions();
  const nowMs = useNow(30_000);
  const delta = repos[repo];
  const [commits, setCommits] = useState<CommitInfo[] | null>(null);
  const [logError, setLogError] = useState(false);

  // Refetch the log when the repo changes or its revision advances (new commit).
  const revision = delta?.revision ?? -1;
  useEffect(() => {
    let active = true;
    // Refetch on repo/revision change; keep the prior list visible until the
    // new one arrives (avoids a synchronous setState in the effect body).
    getCommitLog(repo, 0, COMMIT_LOG_LIMIT)
      .then((c) => {
        if (active) {
          setCommits(c);
          setLogError(false);
        }
      })
      .catch(() => {
        if (active) setLogError(true); // distinct from an empty repo
      });
    return () => {
      active = false;
    };
  }, [repo, revision]);

  if (!delta) {
    return (
      <div className="repo-overview repo-overview--missing" data-testid="repo-overview-missing">
        This repo is no longer in the active workbench.
      </div>
    );
  }

  const { status, error } = delta;
  const metrics = getRepoMetrics(delta);
  const signals = getRepoSignals(delta);
  const filteredModified = filterStatusFiles(status.modified, filters, signals);
  const filteredStaged = filterStatusFiles(status.staged, filters, signals);
  const filteredUntracked = filterStatusFiles(status.untracked, filters, signals);
  const filteredEvents = filterFsEvents(
    repo,
    getFsEvents(state, repo),
    filters,
    busStore.displayName(repo),
    nowMs,
  );
  const activeWorkbench = state.config?.active ?? null;
  const activeConfig = (state.config?.workbenches ?? []).find((w) => w.name === activeWorkbench);
  const repoEntry = activeConfig?.repos.find((r) => r.path === repo);
  return (
    <div className="repo-overview" data-testid={`repo-overview-${repo}`}>
      <header className="repo-panel__head">
        <h2>{busStore.displayName(repo)}</h2>
        <span className="repo-panel__path">{repo}</span>
        <button
          className="repo-panel__remove"
          data-testid="repo-panel-remove"
          title="Remove from workbench"
          onClick={() => removeRepo(repo)}
        >
          Remove
        </button>
      </header>

      {error && (
        <div className="repo-panel__error" data-testid="repo-panel-error">
          <span>
            {error.class}: {error.message}
          </span>
          {error.class === "terminal" && (
            <button data-testid="repo-panel-retry" onClick={() => void retryRepo(repo)}>
              Retry
            </button>
          )}
        </div>
      )}

      <section className="repo-panel__signals" data-testid="repo-signals">
        <h3>Passive signals</h3>
        <MetricsPill metrics={metrics} />
        {signals.length === 0 ? (
          <p className="repo-panel__muted">No passive signals.</p>
        ) : (
          <ul className="signal-list">
            {sortSignals(signals).map((signal, index) => (
              <li
                key={`${signal.kind}:${signal.path ?? "repo"}:${index}`}
                className={`signal-list__item signal-list__item--${signal.severity}`}
              >
                <span className="signal-list__kind">{signal.kind.replace(/_/g, " ")}</span>
                <span className="signal-list__message">{signal.message}</span>
                {signal.path && <span className="signal-list__path">{signal.path}</span>}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="repo-panel__status" data-testid="status-lists">
        <StatusList
          label="Modified"
          files={filteredModified}
          delta={delta}
          onOpen={(f, pin) => openFile(repo, f, pin)}
        />
        <StatusList
          label="Staged"
          files={filteredStaged}
          delta={delta}
          onOpen={(f, pin) => openFile(repo, f, pin)}
        />
        <StatusList
          label="Untracked"
          files={filteredUntracked}
          delta={delta}
          onOpen={(f, pin) => openFile(repo, f, pin)}
        />
        {status.modified.length + status.staged.length + status.untracked.length > 0 &&
          filteredModified.length + filteredStaged.length + filteredUntracked.length === 0 && (
            <p className="repo-panel__muted" data-testid="status-no-matches">
              No status files match the current filters.
            </p>
          )}
      </section>

      <WatchedFilesSection
        key={`${repo}:${(repoEntry?.fs_watch ?? []).join("\0")}`}
        repo={repo}
        activeWorkbench={activeWorkbench}
        patterns={repoEntry?.fs_watch ?? []}
        events={filteredEvents}
        filtersActive={filteredEvents.length !== getFsEvents(state, repo).length}
        watching={state.watching}
        onSave={(patterns) =>
          activeWorkbench
            ? updateRepoFsWatch(activeWorkbench, repo, patterns)
            : Promise.reject(new Error("No active workbench."))
        }
      />

      <section className="repo-panel__log" data-testid="commit-log">
        <h3>Commits</h3>
        {logError ? (
          <p className="repo-panel__muted">Could not load commits.</p>
        ) : commits === null ? (
          <p className="repo-panel__muted">Loading…</p>
        ) : commits.length === 0 ? (
          <p className="repo-panel__muted">No commits yet</p>
        ) : (
          <ul>
            {commits.map((c) => (
              <li key={c.id} title={c.id}>
                <span className="repo-panel__commit-summary">{c.summary}</span>
                <span className="repo-panel__muted">
                  {" · "}
                  {c.author}
                  {" · "}
                  {commitDate(c.timestamp).toLocaleDateString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function StatusList({
  label,
  files,
  delta,
  onOpen,
}: {
  label: string;
  files: string[];
  delta: RepoDelta;
  onOpen: (file: string, pin: boolean) => void;
}) {
  if (files.length === 0) return null;
  return (
    <div className="status-list">
      <h4>
        {label} ({files.length})
      </h4>
      <ul>
        {files.map((f) => (
          <li
            key={f}
            role="button"
            tabIndex={0}
            title={`Preview (click) / open (double-click): ${f}`}
            data-testid={`status-file-${f}`}
            onClick={() => onOpen(f, false)}
            onDoubleClick={() => onOpen(f, true)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                onOpen(f, true);
              } else if (e.key === " ") {
                e.preventDefault();
                onOpen(f, false);
              }
            }}
          >
            {f}
            <SignalBadges signals={getPathSignals(delta, f)} limit={2} compact />
          </li>
        ))}
      </ul>
    </div>
  );
}
