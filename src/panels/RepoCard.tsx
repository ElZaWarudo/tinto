// A single repo "bento" tile: at-a-glance health, always visible (no expand).
// Identity + activity, branch/upstream, change counts, passive metrics/signals,
// and the latest commit. A single click opens the project; files are drilled
// into via the project's own explorer, not here.

import { memo, useEffect, useState } from "react";
import type { BranchInfo, RepoDelta } from "../bus/contract";
import { commitDate, getRepoMetrics, getRepoSignals, signalCounts } from "../bus/store";
import { checkAgentAvailabilityForRepo } from "./agentAvailability";
import { ACTIVITY_WINDOW_MS } from "./constants";
import { GitleaksConfigNotice } from "./GitleaksConfigNotice";
import { RepoSourceBadge } from "./RepoSourceBadge";
import { SignalBadges } from "./SignalBadges";

export interface RepoCardProps {
  delta: RepoDelta;
  name: string;
  source?: "local" | "wsl";
  distro?: string | null;
  activityMs: number;
  nowMs: number;
  onOpen: () => void;
  onRetry: () => void;
  onRemove: () => void;
  onLaunch: (agentType: string) => Promise<void> | void;
  availabilityKey?: string;
}

const AGENT_OPTIONS = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code" },
  { id: "opencode", label: "OpenCode" },
];

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

function RepoCardImpl({
  delta,
  name,
  source,
  distro,
  activityMs,
  nowMs,
  onOpen,
  onRetry,
  onRemove,
  onLaunch,
  availabilityKey = `repo:${delta.repo}`,
}: RepoCardProps) {
  const { status, branch, head, error } = delta;
  const [agentType, setAgentType] = useState("codex");
  const [available, setAvailable] = useState<boolean | null>(null);
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [launching, setLaunching] = useState(false);
  const [launchMessage, setLaunchMessage] = useState<string | null>(null);
  const active = nowMs - activityMs < ACTIVITY_WINDOW_MS;
  const upstream = upstreamLabel(branch);
  const metrics = getRepoMetrics(delta);
  const signals = getRepoSignals(delta);
  const counts = signalCounts(signals);
  const selectedAgent = AGENT_OPTIONS.find((a) => a.id === agentType) ?? AGENT_OPTIONS[0];
  const missingGitleaksConfig = delta.gitleaks_configured === false;

  useEffect(() => {
    let alive = true;
    checkAgentAvailabilityForRepo(delta.repo, availabilityKey, agentType)
      .then((ok) => {
        if (!alive) return;
        setAvailable(ok);
        setAvailabilityMessage(ok ? null : `${selectedAgent.label} not found`);
      })
      .catch((e) => {
        if (!alive) return;
        setAvailable(null);
        setAvailabilityMessage(commandMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [agentType, selectedAgent.label, delta.repo, availabilityKey]);

  const cls = ["repo-card", error ? "repo-card--error" : active ? "repo-card--active" : ""]
    .filter(Boolean)
    .join(" ");

  const launch = () => {
    if (available === false || launching) return;
    setLaunching(true);
    setLaunchMessage(null);
    Promise.resolve(onLaunch(agentType))
      .catch((e) => setLaunchMessage(commandMessage(e)))
      .finally(() => setLaunching(false));
  };

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
        <div className="repo-card__identity">
          <span
            className={active ? "activity-dot activity-dot--active" : "activity-dot"}
            data-testid="activity"
            aria-label={active ? "active now" : "idle"}
          />
          <RepoSourceBadge
            repo={delta.repo}
            source={source}
            distro={distro}
            className="repo-card__source-badge"
          />
          <span className="repo-card__name" title={delta.repo}>
            {name}
          </span>
        </div>
        {error && (
          <span className="error-badge" data-testid="error-badge">
            {error.class}
          </span>
        )}
        <button
          className="repo-card__remove"
          data-testid="repo-card-remove"
          title="Remove from workbench"
          aria-label={`Remove ${name} from workbench`}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          onKeyDown={(e) => e.stopPropagation()}
        >
          ✕
        </button>
      </header>

      <div className="repo-card__branch" data-testid="branch">
        <span className="repo-card__branch-name">{branchLabel(branch, head)}</span>
        {upstream && <span className="repo-card__upstream">{upstream}</span>}
      </div>

      <div className="repo-card__counts" data-testid="counts">
        <span className="count count--modified">
          <strong>{status.modified.length}</strong>
          <span>M</span>
        </span>
        <span className="count count--staged">
          <strong>{status.staged.length}</strong>
          <span>S</span>
        </span>
        <span className="count count--untracked">
          <strong>{status.untracked.length}</strong>
          <span>U</span>
        </span>
        {signals.length > 0 && (
          <span className="repo-card__signal-count" data-testid="signal-count">
            {counts.critical > 0 ? counts.critical : signals.length} signal
            {(counts.critical > 0 ? counts.critical : signals.length) === 1 ? "" : "s"}
          </span>
        )}
      </div>

      <div className="repo-card__metrics" data-testid="repo-metrics">
        <span>
          <strong>{metrics.changed_files}</strong>
          <small>files</small>
        </span>
        <span>
          <strong>+{metrics.lines_added}</strong>
          <small>added</small>
        </span>
        <span>
          <strong>-{metrics.lines_removed}</strong>
          <small>removed</small>
        </span>
      </div>

      <div className="repo-card__signals">
        {signals.length > 0 && <SignalBadges signals={signals} limit={2} />}
      </div>

      {missingGitleaksConfig && (
        <div
          className="repo-card__notice"
          onClick={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
        >
          <GitleaksConfigNotice repo={delta.repo} compact />
        </div>
      )}

      <div
        className="repo-card__launcher"
        data-testid={`agent-launcher-${delta.repo}`}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
      >
        <span className="repo-card__launcher-label">Agent</span>
        <select
          className="repo-card__agent-select"
          aria-label="agent type"
          value={agentType}
          onChange={(e) => {
            setAvailable(null);
            setAvailabilityMessage(null);
            setAgentType(e.target.value);
          }}
        >
          {AGENT_OPTIONS.map((agent) => (
            <option key={agent.id} value={agent.id}>
              {agent.label}
            </option>
          ))}
        </select>
        <button
          className="repo-card__launch"
          data-testid="agent-launch"
          disabled={launching || (available == null && !availabilityMessage) || available === false}
          onClick={launch}
        >
          {launching ? "Starting" : "Launch"}
        </button>
        <span
          className="repo-card__launch-msg"
          data-testid="agent-launch-message"
          aria-live="polite"
        >
          {launchMessage ?? availabilityMessage ?? "\u00a0"}
        </span>
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

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Command failed");
  }
  return String(error || "Command failed");
}
