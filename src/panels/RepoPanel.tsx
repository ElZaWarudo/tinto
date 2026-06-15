// Per-repo detail panel. Differentiated from the card (R4): the full status
// file lists + a commit log (get_commit_log), plus error detail with retry.
// The repo path comes from the panel params so a restored layout reopens it.

import { useEffect, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { getCommitLog, retryRepo } from "../bus/client";
import type { CommitInfo } from "../bus/contract";
import { busStore, useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";

const COMMIT_LOG_LIMIT = 30;

export function RepoPanel(props: IDockviewPanelProps<{ repo: string }>) {
  const repo = props.params.repo;
  const { repos } = useBusState();
  const { removeRepo } = useWorkspaceActions();
  const delta = repos[repo];
  const [commits, setCommits] = useState<CommitInfo[] | null>(null);

  // Refetch the log when the repo changes or its revision advances (new commit).
  const revision = delta?.revision ?? -1;
  useEffect(() => {
    let active = true;
    // Refetch on repo/revision change; keep the prior list visible until the
    // new one arrives (avoids a synchronous setState in the effect body).
    getCommitLog(repo, 0, COMMIT_LOG_LIMIT)
      .then((c) => active && setCommits(c))
      .catch(() => active && setCommits([]));
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
        <StatusList label="Modified" files={status.modified} />
        <StatusList label="Staged" files={status.staged} />
        <StatusList label="Untracked" files={status.untracked} />
      </section>

      <section className="repo-panel__log" data-testid="commit-log">
        <h3>Commits</h3>
        {commits === null ? (
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
                  {new Date(c.timestamp * 1000).toLocaleDateString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function StatusList({ label, files }: { label: string; files: string[] }) {
  if (files.length === 0) return null;
  return (
    <div className="status-list">
      <h4>
        {label} ({files.length})
      </h4>
      <ul>
        {files.map((f) => (
          <li key={f}>{f}</li>
        ))}
      </ul>
    </div>
  );
}
