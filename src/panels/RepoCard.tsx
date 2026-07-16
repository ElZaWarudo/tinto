// A single repo ledger row: at-a-glance health, always visible (no expand).
// Identity + activity, branch/upstream, change counts, passive metrics/signals,
// agent launch, and the latest commit. A single click opens the project; files
// are drilled into via the project's own explorer, not here.

import { memo, useEffect, useState } from "react";
import type { BranchInfo, RepoDelta } from "../bus/contract";
import { commitDate, getRepoMetrics, getRepoSignals, signalCounts } from "../bus/store";
import { checkAgentAvailabilityForRepo } from "./agentAvailability";
import { ACTIVITY_WINDOW_MS } from "./constants";
import { RepoSourceBadge } from "./RepoSourceBadge";
import { SecretScanIndicator } from "./SecretScanIndicator";
import { SignalBadges } from "./SignalBadges";
import { REPO_STATUS_MARKS } from "./statusMarks";

export interface RepoCardProps {
  delta: RepoDelta;
  name: string;
  pending?: boolean;
  source?: "local" | "wsl";
  distro?: string | null;
  activityMs: number;
  nowMs: number;
  onOpen: () => void;
  onRetry: () => void;
  onRemove: () => void;
  onFetch?: () => Promise<unknown> | unknown;
  onLaunch: (agentType: string) => Promise<void> | void;
  availabilityKey?: string;
}

const AGENT_OPTIONS = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code" },
  { id: "opencode", label: "OpenCode" },
];

export interface RepoAgentLauncherProps {
  repo: string;
  pending?: boolean;
  availabilityKey?: string;
  className?: string;
  onLaunch: (agentType: string) => Promise<void> | void;
}

export function RepoAgentLauncher({
  repo,
  pending = false,
  availabilityKey = `repo:${repo}`,
  className,
  onLaunch,
}: RepoAgentLauncherProps) {
  const [agentType, setAgentType] = useState("codex");
  const [available, setAvailable] = useState<boolean | null>(null);
  const [availabilityMessage, setAvailabilityMessage] = useState<string | null>(null);
  const [launching, setLaunching] = useState(false);
  const [launchMessage, setLaunchMessage] = useState<string | null>(null);
  const selectedAgent = AGENT_OPTIONS.find((agent) => agent.id === agentType) ?? AGENT_OPTIONS[0];

  useEffect(() => {
    if (pending) return;
    let alive = true;
    checkAgentAvailabilityForRepo(repo, availabilityKey, agentType)
      .then((ok) => {
        if (!alive) return;
        setAvailable(ok);
        setAvailabilityMessage(ok ? null : `No se encontró ${selectedAgent.label}`);
      })
      .catch((error) => {
        if (!alive) return;
        setAvailable(null);
        setAvailabilityMessage(commandMessage(error));
      });
    return () => {
      alive = false;
    };
  }, [agentType, availabilityKey, pending, repo, selectedAgent.label]);

  const launch = () => {
    if (pending || available === false || launching) return;
    setLaunching(true);
    setLaunchMessage(null);
    Promise.resolve(onLaunch(agentType))
      .catch((error) => setLaunchMessage(commandMessage(error)))
      .finally(() => setLaunching(false));
  };

  return (
    <div
      className={["repo-card__launcher", className].filter(Boolean).join(" ")}
      data-testid={`agent-launcher-${repo}`}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <span className="repo-card__launcher-label">Agent</span>
      <select
        className="repo-card__agent-select"
        aria-label="Tipo de Agent"
        value={agentType}
        onChange={(event) => {
          setAvailable(null);
          setAvailabilityMessage(null);
          setAgentType(event.target.value);
        }}
      >
        {AGENT_OPTIONS.map((agent) => (
          <option key={agent.id} value={agent.id}>
            {agent.label}
          </option>
        ))}
      </select>
      <button
        type="button"
        className="repo-card__launch"
        data-testid="agent-launch"
        disabled={
          pending || launching || (available == null && !availabilityMessage) || available === false
        }
        onClick={launch}
      >
        {launching ? "Iniciando…" : "Iniciar"}
      </button>
      <span className="repo-card__launch-msg" data-testid="agent-launch-message" aria-live="polite">
        {pending
          ? "Esperando la instantánea…"
          : (launchMessage ??
            availabilityMessage ??
            (available === null ? "Comprobando disponibilidad…" : "\u00a0"))}
      </span>
    </div>
  );
}

function branchLabel(branch: BranchInfo | null, head: RepoDelta["head"]): string {
  if (!branch) return "…";
  if (branch.unborn) return "sin commits";
  if (branch.detached) {
    const short = head ? head.id.slice(0, 7) : "";
    return short ? `(HEAD separado) ${short}` : "(HEAD separado)";
  }
  return branch.name ?? "(rama desconocida)";
}

function upstreamLabel(branch: BranchInfo | null): string | null {
  if (!branch || branch.unborn) return null;
  if (branch.ahead == null || branch.behind == null) return "sin rama remota";
  return `↑${branch.ahead} ↓${branch.behind}`;
}

function statusMarkLabel(kind: string, count: number): string {
  const plural = count !== 1;
  switch (kind) {
    case "modified":
      return plural ? "modificados" : "modificado";
    case "staged":
      return plural ? "preparados" : "preparado";
    case "untracked":
      return "sin seguimiento";
    default:
      return kind;
  }
}

function repoErrorLabel(errorClass: string): string {
  return errorClass === "terminal" ? "bloqueado" : "error temporal";
}

function RepoCardImpl({
  delta,
  name,
  pending = false,
  source,
  distro,
  activityMs,
  nowMs,
  onOpen,
  onRetry,
  onRemove,
  onFetch,
  onLaunch,
  availabilityKey = `repo:${delta.repo}`,
}: RepoCardProps) {
  const { status, branch, head, error } = delta;
  const [fetching, setFetching] = useState(false);
  const [fetchMessage, setFetchMessage] = useState<string | null>(null);
  const active = nowMs - activityMs < ACTIVITY_WINDOW_MS;
  const upstream = upstreamLabel(branch);
  const hasRemoteUpstream = branch?.ahead != null && branch?.behind != null;
  const metrics = getRepoMetrics(delta);
  const signals = getRepoSignals(delta);
  const counts = signalCounts(signals);

  const cls = ["repo-card", error ? "repo-card--error" : active ? "repo-card--active" : ""]
    .filter(Boolean)
    .join(" ");
  const stateLabel = pending
    ? "cargando"
    : error
      ? repoErrorLabel(error.class)
      : active
        ? "activo"
        : "en reposo";

  const fetchRemote = () => {
    if (!onFetch || fetching) return;
    setFetching(true);
    setFetchMessage(null);
    Promise.resolve(onFetch())
      .catch((e) => setFetchMessage(commandMessage(e)))
      .finally(() => setFetching(false));
  };

  return (
    <article
      className={cls}
      data-testid={`card-${delta.repo}`}
      aria-label={`Repo ${name}`}
      onClick={onOpen}
    >
      <div className="repo-card__state-mark" aria-label={`Estado del repo: ${stateLabel}`}>
        <span
          className={active ? "activity-dot activity-dot--active" : "activity-dot"}
          data-testid="activity"
          aria-label={active ? "activo ahora" : "en reposo"}
        />
        <span>{stateLabel}</span>
      </div>
      <header className="repo-card__head">
        <button
          className="repo-card__identity repo-card__open"
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onOpen();
          }}
          onKeyDown={(e) => e.stopPropagation()}
        >
          <RepoSourceBadge
            repo={delta.repo}
            source={source}
            distro={distro}
            className="repo-card__source-badge"
          />
          <span className="repo-card__name" title={delta.repo}>
            {name}
          </span>
        </button>
        {error && (
          <span className="error-badge" data-testid="error-badge" title={error.category}>
            {repoErrorLabel(error.class)}
          </span>
        )}
        <button
          className="repo-card__remove"
          data-testid="repo-card-remove"
          title="Quitar del workbench"
          aria-label={`Quitar ${name} del workbench`}
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          onKeyDown={(e) => e.stopPropagation()}
        >
          ✕
        </button>
        <div className="repo-card__branch" data-testid="branch">
          <span className="repo-card__branch-name">{branchLabel(branch, head)}</span>
          {upstream && <span className="repo-card__upstream">{upstream}</span>}
          {hasRemoteUpstream && onFetch && (
            <button
              type="button"
              className="repo-card__fetch"
              data-testid="repo-card-fetch"
              title="Actualizar las referencias remotas de este repo"
              disabled={fetching}
              onClick={(e) => {
                e.stopPropagation();
                fetchRemote();
              }}
              onKeyDown={(e) => e.stopPropagation()}
            >
              {fetching ? "Actualizando…" : "Actualizar"}
            </button>
          )}
        </div>
        {fetchMessage && (
          <span className="repo-card__fetch-error" role="alert">
            {fetchMessage}
          </span>
        )}
        <footer className="repo-card__foot">
          {head ? (
            <span className="repo-card__commit" title={`${head.summary} · ${head.id}`}>
              <span className="repo-card__commit-summary">{head.summary}</span>
              <span className="repo-card__commit-time">
                {commitDate(head.timestamp).toLocaleDateString()}
              </span>
            </span>
          ) : (
            <span className="repo-card__commit repo-card__commit--none">sin commits</span>
          )}
        </footer>
      </header>

      <div className="repo-card__counts" data-testid="counts">
        {REPO_STATUS_MARKS.map((mark) => {
          const count = mark.count(status);
          const label = statusMarkLabel(mark.kind, count);
          return (
            <span
              aria-label={`${count} ${count === 1 ? "archivo" : "archivos"} ${label}`}
              className={`count count--${mark.className}`}
              key={mark.kind}
              title={`${count === 1 ? "Archivo" : "Archivos"} ${label}: ${count}`}
            >
              <strong>{count}</strong>
              <span>{mark.short}</span>
            </span>
          );
        })}
        {signals.length > 0 && (
          <span className="repo-card__signal-count" data-testid="signal-count">
            {counts.critical > 0 ? counts.critical : signals.length}{" "}
            {counts.critical > 0
              ? counts.critical === 1
                ? "señal crítica"
                : "señales críticas"
              : signals.length === 1
                ? "señal"
                : "señales"}
          </span>
        )}
      </div>

      <div className="repo-card__metrics" data-testid="repo-metrics">
        <span>
          <strong>{metrics.changed_files}</strong>
          <small>archivos</small>
        </span>
        <span>
          <strong>+{metrics.lines_added}</strong>
          <small>añadidas</small>
        </span>
        <span>
          <strong>-{metrics.lines_removed}</strong>
          <small>eliminadas</small>
        </span>
      </div>

      <div className="repo-card__signals">
        {signals.length > 0 && <SignalBadges signals={signals} limit={2} />}
        <SecretScanIndicator
          status={delta.secret_scan_status}
          findings={delta.secret_findings?.length ?? 0}
          compact
        />
      </div>

      {pending && (
        <div
          className="repo-card__notice repo-card__pending"
          data-testid="repo-pending"
          role="status"
          aria-live="polite"
        >
          Esperando la primera instantánea del repo…
        </div>
      )}

      <RepoAgentLauncher
        repo={delta.repo}
        pending={pending}
        availabilityKey={availabilityKey}
        onLaunch={onLaunch}
      />

      {error && (
        <div className="repo-card__error" data-testid="error-detail" role="alert">
          <span className="repo-card__error-msg">
            <strong>Error: </strong>
            {error.message}
          </span>
          {error.class === "terminal" && (
            <button
              className="repo-card__retry"
              data-testid="retry"
              onClick={(e) => {
                e.stopPropagation();
                onRetry();
              }}
            >
              Reintentar
            </button>
          )}
        </div>
      )}
    </article>
  );
}

export const RepoCard = memo(RepoCardImpl);

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "No se pudo completar la acción");
  }
  return String(error || "No se pudo completar la acción");
}
