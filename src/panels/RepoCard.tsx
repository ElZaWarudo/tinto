// A single repo "bento" tile: at-a-glance health, always visible (no expand).
// Identity + activity, branch/upstream, change counts, passive metrics/signals,
// and the latest commit. Important repos (errors, critical signals, active work)
// get a wider tile. A single click opens the project; files are drilled into via
// the project's own explorer, not here.

import { memo } from "react";
import type { BranchInfo, RepoDelta } from "../bus/contract";
import { commitDate, getRepoMetrics, getRepoSignals, signalCounts } from "../bus/store";
import { ACTIVITY_WINDOW_MS } from "./constants";
import { MetricsPill, SignalBadges } from "./SignalBadges";

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
  const { status, branch, head, error } = delta;
  const active = nowMs - activityMs < ACTIVITY_WINDOW_MS;
  const upstream = upstreamLabel(branch);
  const metrics = getRepoMetrics(delta);
  const signals = getRepoSignals(delta);
  const counts = signalCounts(signals);
  const changes = status.modified.length + status.staged.length + status.untracked.length;

  // Bento emphasis: feature the repos that warrant attention with a wider tile.
  const feature = !!error || counts.critical > 0 || (active && changes > 0);
  const cls = [
    "repo-card",
    feature ? "repo-card--feature" : "",
    error ? "repo-card--error" : active ? "repo-card--active" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      className={cls}
      data-testid={`card-${delta.repo}`}
      tabIndex={0}
      role="button"
      onClick={onOpen}
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
      </header>

      <div className="repo-card__branch" data-testid="branch">
        <span className="repo-card__branch-name">{branchLabel(branch, head)}</span>
        {upstream && <span className="repo-card__upstream">{upstream}</span>}
      </div>

      <div className="repo-card__counts" data-testid="counts">
        <span className="count count--modified">{status.modified.length}M</span>
        <span className="count count--staged">{status.staged.length}S</span>
        <span className="count count--untracked">{status.untracked.length}U</span>
        {signals.length > 0 && (
          <span className="repo-card__signal-count" data-testid="signal-count">
            {counts.critical > 0 ? counts.critical : signals.length} signal
            {(counts.critical > 0 ? counts.critical : signals.length) === 1 ? "" : "s"}
          </span>
        )}
      </div>

      <div className="repo-card__metrics">
        <MetricsPill metrics={metrics} />
        {signals.length > 0 && <SignalBadges signals={signals} limit={feature ? 4 : 2} />}
      </div>

      <footer className="repo-card__foot">
        {head ? (
          <span className="repo-card__commit" title={`${head.summary} · ${head.id}`}>
            <span className="repo-card__commit-summary">{head.summary}</span>
            <span className="repo-card__commit-time">
              {commitDate(head.timestamp).toLocaleDateString()}
            </span>
          </span>
        ) : (
          <span className="repo-card__commit repo-card__commit--none">no commits yet</span>
        )}
      </footer>

      {error && (
        <div className="repo-card__error" data-testid="error-detail">
          <span className="repo-card__error-msg">{error.message}</span>
          {error.class === "terminal" && (
            <button
              className="repo-card__retry"
              data-testid="retry"
              onClick={(e) => {
                e.stopPropagation();
                onRetry();
              }}
            >
              Retry
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export const RepoCard = memo(RepoCardImpl);
