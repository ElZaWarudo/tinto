// A single repo card. Reading hierarchy (R3): activity + name, error badge,
// counts, branch, then ahead/behind + last commit (expanded). Handles git edge
// states (unborn / detached / no upstream / no head) without crashing.

import { memo, useState } from "react";
import type { BranchInfo, RepoDelta } from "../bus/contract";
import { commitDate } from "../bus/store";
import { ACTIVITY_WINDOW_MS } from "./constants";

export interface RepoCardProps {
  delta: RepoDelta;
  name: string;
  activityMs: number;
  nowMs: number;
  onOpen: () => void;
  onRetry: () => void;
}

function branchLabel(branch: BranchInfo | null, head: RepoDelta["head"]): string {
  if (!branch) return "…";
  if (branch.unborn) return "no commits yet";
  if (branch.detached) {
    const short = head ? head.id.slice(0, 7) : "";
    return short ? `(detached) ${short}` : "(detached)";
  }
  return branch.name ?? "(unknown)";
}

function upstreamLabel(branch: BranchInfo | null): string | null {
  if (!branch || branch.unborn) return null;
  if (branch.ahead == null || branch.behind == null) return "no upstream";
  return `↑${branch.ahead} ↓${branch.behind}`;
}

function RepoCardImpl({ delta, name, activityMs, nowMs, onOpen, onRetry }: RepoCardProps) {
  const [expanded, setExpanded] = useState(false);
  const { status, branch, head, error } = delta;
  const active = nowMs - activityMs < ACTIVITY_WINDOW_MS;
  const upstream = upstreamLabel(branch);

  return (
    <div
      className="repo-card"
      data-testid={`card-${delta.repo}`}
      tabIndex={0}
      role="button"
      onDoubleClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
    >
      <header className="repo-card__head">
        <span
          className={active ? "activity-dot activity-dot--active" : "activity-dot"}
          data-testid="activity"
          aria-label={active ? "active now" : "idle"}
        />
        <span className="repo-card__name" title={delta.repo}>
          {name}
        </span>
        {error && (
          <span className="error-badge" data-testid="error-badge">
            {error.class}
          </span>
        )}
        <button
          className="repo-card__expand"
          aria-label={expanded ? "collapse" : "expand"}
          onClick={() => setExpanded((e) => !e)}
        >
          {expanded ? "▾" : "▸"}
        </button>
      </header>

      <div className="repo-card__counts" data-testid="counts">
        <span className="count count--modified">{status.modified.length}M</span>
        <span className="count count--staged">{status.staged.length}S</span>
        <span className="count count--untracked">{status.untracked.length}U</span>
      </div>

      <div className="repo-card__branch" data-testid="branch">
        {branchLabel(branch, head)}
      </div>

      {expanded && (
        <div className="repo-card__details">
          {upstream && <div className="repo-card__upstream">{upstream}</div>}
          {head ? (
            <div className="repo-card__commit" title={head.id}>
              {head.summary}
              <span className="repo-card__commit-time">
                {" · "}
                {commitDate(head.timestamp).toLocaleString()}
              </span>
            </div>
          ) : (
            <div className="repo-card__commit repo-card__commit--none">no commits yet</div>
          )}
        </div>
      )}

      {error && (
        <div className="repo-card__error" data-testid="error-detail">
          <span className="repo-card__error-msg">{error.message}</span>
          {error.class === "terminal" && (
            <button className="repo-card__retry" data-testid="retry" onClick={onRetry}>
              Retry
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export const RepoCard = memo(RepoCardImpl);
