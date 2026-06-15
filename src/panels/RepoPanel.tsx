// Per-repo detail panel. Differentiated from the card (R4): the full status
// file lists + a commit log (get_commit_log), plus error detail with retry.
// The repo path comes from the panel params so a restored layout reopens it.

import { useEffect, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { getCommitLog, retryRepo } from "../bus/client";
import type { CommitInfo } from "../bus/contract";
import { busStore, commitDate, useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";

const COMMIT_LOG_LIMIT = 30;

export function RepoPanel(props: IDockviewPanelProps<{ repo: string }>) {
  const repo = props.params.repo;
  const { repos } = useBusState();
  const { removeRepo, openDiff } = useWorkspaceActions();
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
      <div className="repo-panel repo-panel--missing">
        This repo is no longer in the active workbench.
      </div>
    );
  }

  const { status, error } = delta;
  return (
    <div className="repo-panel" data-testid={`repo-panel-${repo}`}>
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

      <section className="repo-panel__status" data-testid="status-lists">
        <StatusList label="Modified" files={status.modified} onOpen={(f) => openDiff(repo, f)} />
        <StatusList label="Staged" files={status.staged} onOpen={(f) => openDiff(repo, f)} />
        <StatusList label="Untracked" files={status.untracked} onOpen={(f) => openDiff(repo, f)} />
      </section>

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
  onOpen,
}: {
  label: string;
  files: string[];
  onOpen: (file: string) => void;
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
            title={`Open diff: ${f}`}
            data-testid={`status-file-${f}`}
            onDoubleClick={() => onOpen(f)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onOpen(f);
              }
            }}
          >
            {f}
          </li>
        ))}
      </ul>
    </div>
  );
}
