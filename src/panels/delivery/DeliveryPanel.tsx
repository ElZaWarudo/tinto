// Delivery mode: tasks in their own worktree and branch, advanced by
// background agent stages. A separate way to work from Agents; the two only
// meet through "Abrir en Agents".

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { busStore, useBusState } from "../../bus/store";
import { useAgentSessionState } from "../../agent/sessionStore";
import { useWorkspaceActions } from "../../workspace/actions";
import { setBaseView } from "../../workspace/baseView";
import { isWslRepoSource } from "../repoSource";
import { confirm } from "../../workbench/confirmDialog";
import { codexReasoningLabel, lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import {
  archiveDeliveryTask,
  cancelDeliveryJob,
  getDeliveryIssueUrl,
  getDeliveryTaskChanges,
  openDeliveryVisualRecord,
  pauseDeliveryJob,
  resumeDeliveryJob,
  setDeliveryRunPaused,
  checkDeliveryTask,
  closeDeliveryRun,
  closeDeliveryPr,
  handOverDeliveryRun,
  completeDeliveryApproval,
  deliveryPrToDraft,
  openDeliveryPr,
  correctDeliveryApproval,
  createDeliveryRun,
  createDeliveryTask,
  decideDeliveryApproval,
  dispatchDeliveryJob,
  findDeliveryPr,
  getDeliveryMergeMethod,
  getDeliveryPrReviewers,
  getDeliveryJobLog,
  getDeliveryOverview,
  getDeliveryRepoSettings,
  onDeliveryChanged,
  openDeliveryJobInAgents,
  openDeliveryTaskInAgents,
  releaseDeliveryLease,
  removeDeliveryTask,
  requestDeliveryApproval,
  retryDeliveryJob,
  setDeliveryCodexModel,
  setDeliveryRepoSettings,
  setDeliveryRunQaJiraComment,
  takeoverDeliveryRun,
  undoDeliveryCommit,
  unarchiveDeliveryTask,
  undoDeliveryJob,
  updateDeliverySettings,
  updateDeliveryTask,
} from "../../delivery/client";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryOverview,
  DeliveryPr,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryStageAgent,
  DeliveryTask,
  DeliveryUsage,
} from "../../delivery/types";
import {
  DispatchDialog,
  ArchivedTasksDialog,
  CorrectResultDialog,
  NewTaskDialog,
  ReleaseLeaseDialog,
  SettingsDialog,
  StageAgentRow,
  type CorrectableStep,
  type RepoChoice,
} from "./DeliveryDialogs";
import { CommitPlanForm, CommitPlanView } from "./DeliveryCommitPlan";
import { QaProblems } from "./DeliveryQaProblems";
import { coordinatorStalls } from "./DeliveryNotifications";
import { CoordinatorDecisions, DecisionLog, PendingDecisions } from "./DeliveryDecisions";
import { PendingSettingsProposals } from "./DeliverySettingsProposals";
import { PrSection } from "./DeliveryPr";
import {
  GROUPS,
  STEPS,
  TASK_STATE_LABEL,
  attemptOutcome,
  batchName,
  isBroken,
  isDelivered,
  isOpen,
  isSuperseded,
  jobsOf,
  lastHandoff,
  latestApproval,
  latestOfRole,
  lower,
  nextStageOf,
  latestStep,
  nextStep,
  parseStatuses,
  pausedJob,
  pendingSettingsProposals,
  prSummary,
  quietFor,
  rolesOf,
  stepKey,
  stepOf,
  type DeliveryStep,
  stageLabel,
  taskStatus,
  type StatusIcon,
  type TaskStatus,
  type Tone,
} from "./taskStatus";
import "./delivery.css";
import { ExternalLink } from "./ExternalLink";
import { ActionButton, ActionGroup } from "./ActionButton";
import { DeliveryRefreshContext } from "./deliveryRefresh";

type Dialog =
  | { kind: "new-task" }
  | { kind: "dispatch"; task: DeliveryTask; role: string; prompt: string; source: string | null }
  | { kind: "release"; name: string; note: string | null }
  | { kind: "settings" }
  | { kind: "archived" }
  | null;

type IconName = Exclude<StatusIcon, "live"> | "more" | "down" | "right" | "gear" | "play";

const ICONS: Record<IconName, ReactNode> = {
  flag: <path d="M4 14V2.5M4 3h7.5l-1.8 2.6L11.5 8.2H4" />,
  x: <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />,
  alert: <path d="M8 2.5l6 11H2zM8 6.6v3.1M8 11.7v.1" />,
  clock: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v3.2l2.2 1.4" />
    </>
  ),
  ring: <circle cx="8" cy="8" r="4" />,
  check: <path d="M3.5 8.5l3 3 6-7" />,
  more: <path d="M3.5 8h.01M8 8h.01M12.5 8h.01" strokeWidth="2.6" />,
  down: <path d="M4 6l4 4 4-4" />,
  right: <path d="M6 4l4 4-4 4" />,
  gear: (
    <>
      <circle cx="8" cy="8" r="2.3" />
      <path d="M8 1.8v2M8 12.2v2M1.8 8h2M12.2 8h2M3.6 3.6L5 5M11 11l1.4 1.4M3.6 12.4L5 11M11 5l1.4-1.4" />
    </>
  ),
  play: <path d="M5 3.5l7 4.5-7 4.5z" />,
};

function Icon({ name }: { name: IconName }) {
  return (
    <svg className="delivery-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      {ICONS[name]}
    </svg>
  );
}

function StatusMark({ icon }: { icon: StatusIcon }) {
  return icon === "live" ? (
    <span className="delivery-dot delivery-dot--live" aria-hidden />
  ) : (
    <Icon name={icon} />
  );
}

const TONE_ICON: Record<Tone, StatusIcon> = {
  attention: "alert",
  step: "flag",
  danger: "x",
  live: "live",
  waiting: "clock",
  idle: "ring",
  ok: "check",
};

function StatusLine({ status }: { status: TaskStatus }) {
  return (
    <span className={`delivery-status-line delivery-tone--${status.tone}`}>
      <StatusMark icon={status.icon} />
      {status.label}
    </span>
  );
}

/** A button that opens a short list of actions; closes on outside click or Escape. */
function Menu({
  label,
  trigger,
  disabled,
  children,
}: {
  label?: string;
  trigger: ReactNode;
  disabled?: boolean;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    ref.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
    const onPointer = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <span className="delivery-menu" ref={ref}>
      <button
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen(!open)}
      >
        {trigger}
      </button>
      {open && (
        <div className="delivery-menu__list" role="menu">
          {children(() => setOpen(false))}
        </div>
      )}
    </span>
  );
}

function errorText(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function shortCommit(value: string | null | undefined): string {
  return value ? value.slice(0, 8) : "";
}

function duration(job: DeliveryJob, now: number): string {
  if (!job.started_at_ms) return "";
  const end = job.ended_at_ms ?? now;
  const seconds = Math.max(0, Math.round((end - job.started_at_ms) / 1000));
  if (seconds < 60) return `${seconds} s`;
  return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
}

/** Two paths of the same folder, whatever their slashes and case. */
function samePath(a: string, b: string): boolean {
  const clean = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  return clean(a) === clean(b);
}

/** "hace 12 s", "hace 3 min": when the agent last did something. */
function ago(ms: number | null | undefined, now: number): string {
  if (!ms) return "";
  const seconds = Math.max(0, Math.round((now - ms) / 1000));
  return seconds < 60 ? `hace ${seconds} s` : `hace ${Math.floor(seconds / 60)} min`;
}

function tokens(count: number): string {
  return count >= 1000 ? `${(count / 1000).toFixed(1)}k` : String(count);
}

/** What a job used: tokens in, read from cache and out, and the cost when known. */
function usageLabel(usage: DeliveryUsage): string {
  const cost =
    usage.cost_micro_usd === null ? "" : ` · $${(usage.cost_micro_usd / 1_000_000).toFixed(2)}`;
  return `Consumo: ${tokens(usage.input_tokens)} de entrada · ${tokens(usage.cached_tokens)} en caché · ${tokens(usage.output_tokens)} de salida${cost}`;
}

function changeSymbol(kind: string): string {
  if (kind === "created") return "+";
  if (kind === "removed") return "×";
  return "−";
}

function files(count: number): string {
  return count === 1 ? "1 archivo" : `${count} archivos`;
}

function agentName(agent: string): string {
  return agent === "codex"
    ? "Codex"
    : agent === "claude"
      ? "Claude Code"
      : agent === "muse"
        ? "Muse Code"
        : "Shell";
}

/** The model and reasoning a stage runs with, named even when they are the defaults. */
function runtimeLabel(job: DeliveryJob): string {
  if (job.agent === "shell") return "";
  const model = job.model ?? job.session_model ?? "modelo predeterminado";
  if (job.reasoning) return `${model} · ${codexReasoningLabel(job.reasoning).toLowerCase()}`;
  // Codex and Muse Code fall back to the model's default effort, as the last catalog has it.
  const fallback =
    job.agent === "codex" || job.agent === "muse"
      ? lastRuntimeCatalog(job.agent)?.models?.find((entry) => entry.model === job.model)
          ?.default_reasoning_effort
      : undefined;
  return fallback
    ? `${model} · ${codexReasoningLabel(fallback).toLowerCase()} (predeterminado)`
    : `${model} · razonamiento predeterminado`;
}

/** Who ran the task's latest stage, for one with no agent of its own to start from. */
function lastStageAgent(jobs: DeliveryJob[], taskId: string): DeliveryStageAgent | null {
  const last = [...jobs].reverse().find((job) => job.task_id === taskId && job.agent !== "shell");
  return last ? { agent: last.agent, model: last.model, reasoning: last.reasoning ?? null } : null;
}

export function DeliveryPanel() {
  const bus = useBusState();
  const { openAgentTerminal } = useWorkspaceActions();
  const [overview, setOverview] = useState<DeliveryOverview | null>(null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [repoSettings, setRepoSettings] = useState<DeliveryRepoSettings | null>(null);

  const refresh = useCallback(
    () =>
      getDeliveryOverview().then(
        (next) => setOverview(next),
        (cause: unknown) => setError(errorText(cause)),
      ),
    [],
  );

  useEffect(() => {
    // Codex jobs that name no model (e.g. from a coordinator) use the
    // account's default from the last model catalog Tinto loaded.
    const model = lastRuntimeCatalog()?.default_model;
    if (model) setDeliveryCodexModel(model).catch(() => {});
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    let pending: ReturnType<typeof setTimeout> | null = null;
    getDeliveryOverview().then(
      (next) => {
        if (active) setOverview(next);
      },
      (cause: unknown) => {
        if (active) setError(errorText(cause));
      },
    );
    // Running agents change things several times a second: read the
    // overview at most once a second, one read at a time, so the panel
    // keeps answering clicks.
    let reading = false;
    let again = false;
    const reload = () => {
      if (reading) {
        again = true;
        return;
      }
      reading = true;
      void refresh().finally(() => {
        reading = false;
        if (again && active) {
          again = false;
          schedule();
        }
      });
    };
    const schedule = () => {
      if (pending) return;
      pending = setTimeout(() => {
        pending = null;
        if (active) reload();
      }, 1000);
    };
    void onDeliveryChanged(schedule).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    });
    const clock = setInterval(() => setNow(Date.now()), 1000);
    const fallback = setInterval(reload, 5000);
    return () => {
      active = false;
      unlisten?.();
      if (pending) clearTimeout(pending);
      clearInterval(clock);
      clearInterval(fallback);
    };
  }, [refresh]);

  const repos: RepoChoice[] = useMemo(() => {
    const config = bus.config;
    const active = config?.workbenches.find((workbench) => workbench.name === config.active);
    return (active?.repos ?? []).map((entry) => ({
      path: entry.path,
      distro: isWslRepoSource(entry.path, entry.source, entry.distro)
        ? (entry.distro ?? null)
        : null,
      label: busStore.displayName(entry.path),
    }));
  }, [bus.config]);

  const tasks = useMemo(() => overview?.tasks ?? [], [overview]);
  const statuses = new Map(
    overview ? tasks.map((task) => [task.id, taskStatus(task, overview, now)] as const) : [],
  );
  const groups = GROUPS.map((group) => ({
    ...group,
    tasks: tasks.filter((task) => statuses.get(task.id)?.group === group.group),
  })).filter((group) => group.tasks.length > 0);
  const ordered = groups.flatMap((group) => group.tasks);
  const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? ordered[0] ?? null;
  const shownTaskId = selectedTask?.id ?? null;
  // Keep showing the same task when its group changes (e.g. after answering
  // its decisions) instead of jumping to whatever is first in the list.
  if (shownTaskId && shownTaskId !== selectedTaskId) setSelectedTaskId(shownTaskId);
  const needYou = groups.find((group) => group.group === "attention")?.tasks ?? [];
  const multiRepo = new Set(tasks.map((task) => task.repo)).size > 1;

  const jobs = overview?.jobs ?? [];
  const running = jobs.filter((job) => job.status === "pending" || job.status === "started");
  const queued = jobs.filter((job) => job.status === "queued");
  // The setting, and the agents it allows now (automatic: one per active task).
  const capacitySetting = overview?.settings.capacity ?? 0;
  const capacity = overview?.capacity ?? Math.max(1, capacitySetting);
  const qa = overview?.leases.find((lease) => lease.name === "qa");
  const qaHolder = tasks.find((task) => task.id === qa?.holder_task_id);
  const activeRuns = (overview?.runs ?? []).filter((candidate) => candidate.status === "active");
  const stalls = overview ? coordinatorStalls(overview, now) : [];

  // For what is not a button (a select, a menu item): its error goes to the
  // panel's banner. Buttons are ActionButtons and say it themselves.
  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      await refresh();
      return true;
    } catch (cause) {
      setError(errorText(cause));
      return false;
    }
  };

  // Running Agents conversations of the batch's repo, to hand it to one.
  const agentSessions = Object.values(useAgentSessionState().sessions);
  const handOverTargets = (target: DeliveryRun) =>
    agentSessions.filter(
      (session) =>
        session.status === "running" &&
        session.id !== target.coordinator_session &&
        samePath(session.repo, target.repo),
    );

  const takeover = async (target: DeliveryRun) => {
    const ok = await confirm(
      `${target.owner} dejará de coordinar "${target.title}". Lo que envíe a partir de ahora se descarta.`,
      { title: "Tomar el control", kind: "warning", okLabel: "Tomar el control" },
    );
    if (!ok) return false;
    await takeoverDeliveryRun(target.id);
  };

  return (
    <DeliveryRefreshContext.Provider value={refresh}>
      <div className="delivery-panel" data-testid="delivery-panel">
        <header className="delivery-panel__header">
          <h1>Delivery</h1>
          {activeRuns.length === 1 ? (
            <span className="delivery-run" title={activeRuns[0].repo}>
              <strong>{batchName(activeRuns[0].title || activeRuns[0].id)}</strong>
              <span className="delivery-muted">
                {busStore.displayName(activeRuns[0].repo)} ·{" "}
                {activeRuns[0].owner === "user"
                  ? "lo diriges tú"
                  : activeRuns[0].owner
                    ? `coordina ${activeRuns[0].owner}`
                    : "sin coordinador"}
              </span>
              {activeRuns[0].notified_at_ms && (
                <span
                  className="delivery-muted"
                  title="Tinto le escribe en Agents lo que espera de él"
                >
                  · coordinador avisado {ago(activeRuns[0].notified_at_ms, now)}
                </span>
              )}
              {activeRuns[0].owner && activeRuns[0].owner !== "user" && (
                <ActionButton onAction={() => takeover(activeRuns[0])}>
                  Tomar el control
                </ActionButton>
              )}
              <Menu
                trigger={
                  <>
                    Cambiar coordinador <Icon name="down" />
                  </>
                }
              >
                {(close) => {
                  const targets = handOverTargets(activeRuns[0]);
                  return targets.length === 0 ? (
                    <span className="delivery-muted" role="menuitem" aria-disabled="true">
                      Abre en Agents una conversación de este repositorio
                    </span>
                  ) : (
                    targets.map((session) => (
                      <button
                        key={session.id}
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          close();
                          void run(() => handOverDeliveryRun(activeRuns[0].id, session.id));
                        }}
                      >
                        {agentName(session.agent_type)} · {session.id.slice(0, 8)}
                        {session.turn_status === "working" ? " (trabajando)" : ""}
                      </button>
                    ))
                  );
                }}
              </Menu>
              <ActionButton
                title={
                  activeRuns[0].paused
                    ? "Lo que está en cola vuelve a arrancar"
                    : "Lo que está en cola espera; lo que corre sigue (puedes pausar cada etapa)"
                }
                onAction={() => setDeliveryRunPaused(activeRuns[0].id, !activeRuns[0].paused)}
              >
                {activeRuns[0].paused ? "Reanudar lote" : "Pausar lote"}
              </ActionButton>
            </span>
          ) : (
            activeRuns.length > 1 && (
              <button
                type="button"
                className="delivery-run"
                title="Ver, tomar el control o cerrar los lotes"
                onClick={() => setDialog({ kind: "settings" })}
              >
                {activeRuns.length} lotes activos
              </button>
            )
          )}
          <span className="delivery-panel__spacer" />
          {needYou.length > 0 && (
            <button
              type="button"
              className="delivery-attention"
              onClick={() => setSelectedTaskId(needYou[0].id)}
            >
              <Icon name="flag" />
              {needYou.length === 1
                ? "1 tarea te necesita"
                : `${needYou.length} tareas te necesitan`}
            </button>
          )}
          <span className="delivery-load">
            Agentes
            <span className="delivery-meter" aria-hidden>
              {Array.from({ length: capacity }, (_, index) => (
                <i key={index} className={index < running.length ? "on" : undefined} />
              ))}
            </span>
            {running.length} de {capacity}
            {queued.length > 0 && <span className="delivery-muted">· {queued.length} en cola</span>}
          </span>
          {qa?.state === "quarantined" ? (
            <button
              type="button"
              className="delivery-lease delivery-lease--quarantined"
              title={qa.note ?? undefined}
              onClick={() => setDialog({ kind: "release", name: "qa", note: qa.note })}
            >
              QA en cuarentena
              {qa.queue.length > 0 && (
                <span className="delivery-muted">· {qa.queue.length} esperando</span>
              )}
            </button>
          ) : (
            qa && (
              <span
                className={`delivery-lease delivery-lease--${qa.state}`}
                title={qa.note ?? undefined}
              >
                {qa.state === "active" ? `QA: ${qaHolder?.key ?? "ocupado"}` : "QA libre"}
                {qa.queue.length > 0 && (
                  <span className="delivery-muted">· {qa.queue.length} esperando</span>
                )}
              </span>
            )
          )}
          <button type="button" onClick={() => setDialog({ kind: "archived" })}>
            Archivadas…
          </button>
          <button
            type="button"
            className="delivery-icon-button"
            aria-label="Ajustes de Delivery"
            title="Ajustes de Delivery"
            onClick={() => setDialog({ kind: "settings" })}
          >
            <Icon name="gear" />
          </button>
          <button
            type="button"
            className="delivery-button--primary"
            disabled={repos.length === 0}
            onClick={() => setDialog({ kind: "new-task" })}
          >
            Nueva tarea
          </button>
        </header>

        {error && (
          <div className="delivery-panel__error" role="alert">
            <span>{error}</span>
            <button type="button" aria-label="Descartar error" onClick={() => setError(null)}>
              ×
            </button>
          </div>
        )}

        {/* A coordinator Tinto could not wake: nothing moves until it goes on. */}
        {stalls.map(({ run, unread, since }) => (
          <div key={run.id} className="delivery-panel__warning" role="status">
            <span>
              {batchName(run.title || run.id)}: el coordinador
              {run.owner && run.owner !== "user" ? ` (${run.owner})` : ""} no está esperando y tiene{" "}
              {unread === 1 ? "1 cambio" : `${unread} cambios`} sin leer desde las{" "}
              {new Date(since).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}.
              Pídele que siga con wait_run; si sus herramientas de Delivery fallan, que las
              reconecte.
            </span>
          </div>
        ))}

        {/* Only when it holds work back; otherwise the header and the task say it. */}
        {qa?.state === "quarantined" && qa.queue.length > 0 && (
          <div className="delivery-panel__warning" role="status">
            <span title={qa.note ?? undefined}>
              {qa.queue.length === 1 ? "Una QA espera" : `${qa.queue.length} QA esperan`} a que
              liberes el recurso QA{qaHolder ? `, en cuarentena tras ${qaHolder.key}` : ""}.
            </span>
            <button
              type="button"
              onClick={() => setDialog({ kind: "release", name: "qa", note: qa.note })}
            >
              Liberar…
            </button>
          </div>
        )}

        <div className="delivery-panel__body">
          <nav className="delivery-panel__tasks" aria-label="Tareas de Delivery">
            {tasks.length === 0 && (
              <p className="delivery-panel__empty">
                Crea una tarea para trabajar un issue en su propio worktree y rama.
              </p>
            )}
            {groups.map((group) => (
              <section key={group.group} className="delivery-task-group" aria-label={group.label}>
                <h2>
                  {group.label} <span>{group.tasks.length}</span>
                </h2>
                {group.tasks.map((task) => (
                  <button
                    key={task.id}
                    type="button"
                    className="delivery-task-row"
                    aria-current={selectedTask?.id === task.id ? "true" : undefined}
                    onClick={() => setSelectedTaskId(task.id)}
                  >
                    <span className="delivery-task-row__key">
                      {task.key}
                      {multiRepo && (
                        <span className="delivery-task-row__repo" title={task.repo}>
                          {busStore.displayName(task.repo)}
                        </span>
                      )}
                    </span>
                    <span className="delivery-task-row__title">{task.title || task.branch}</span>
                    <StatusLine status={statuses.get(task.id)!} />
                  </button>
                ))}
              </section>
            ))}
          </nav>

          <main className="delivery-panel__detail">
            {selectedTask && overview ? (
              <TaskDetail
                key={selectedTask.id}
                task={selectedTask}
                overview={overview}
                now={now}
                run={run}
                onDispatch={(role, prompt, source) => {
                  setRepoSettings(null);
                  getDeliveryRepoSettings(selectedTask.repo).then(setRepoSettings, () =>
                    setRepoSettings({ worktree_root: null, bootstrap: null, default_base: null }),
                  );
                  setDialog({ kind: "dispatch", task: selectedTask, role, prompt, source });
                }}
                onOpenConversation={async (agentType) => {
                  await run(async () => {
                    const conversation = await openDeliveryTaskInAgents(selectedTask.id, agentType);
                    openAgentTerminal({
                      sessionId: conversation.session_id,
                      repo: conversation.repo,
                      agentType: conversation.agent_type,
                    });
                  });
                }}
                onContinueJob={async (job) => {
                  const conversation = await openDeliveryJobInAgents(job.id);
                  openAgentTerminal({
                    sessionId: conversation.session_id,
                    repo: conversation.repo,
                    agentType: conversation.agent_type,
                  });
                }}
                onRemoved={() => setSelectedTaskId(null)}
              />
            ) : (
              <p className="delivery-panel__empty">Sin tareas.</p>
            )}
          </main>
        </div>

        {dialog?.kind === "new-task" && (
          <NewTaskDialog
            repos={repos}
            runs={overview?.runs ?? []}
            onCancel={() => setDialog(null)}
            onSubmit={async (values) => {
              if (values.bootstrap.trim()) {
                const current = await getDeliveryRepoSettings(values.repo.path);
                await setDeliveryRepoSettings(values.repo.path, {
                  ...current,
                  bootstrap: values.bootstrap.trim(),
                });
              }
              const task = await createDeliveryTask({
                repo: values.repo.path,
                distro: values.repo.distro,
                key: values.key.trim(),
                title: values.title.trim(),
                base: values.base.trim(),
                branch: values.branch.trim(),
                runId: values.runId,
              });
              setSelectedTaskId(task.id);
              setDialog(null);
            }}
          />
        )}
        {dialog?.kind === "dispatch" && (
          <DispatchDialog
            task={dialog.task}
            settings={repoSettings}
            lastAgent={lastStageAgent(jobs, dialog.task.id)}
            initialRole={dialog.role}
            initialPrompt={dialog.prompt}
            promptSource={dialog.source}
            onCancel={() => setDialog(null)}
            onSubmit={async (values) => {
              if (values.access === "full") {
                const ok = await confirm(
                  `El agente podrá ejecutar comandos y modificar archivos fuera del worktree sin pedir aprobación.\n\nWorktree: ${dialog.task.worktree}`,
                  {
                    title: "Dar acceso completo",
                    kind: "warning",
                    okLabel: "Dar acceso completo",
                  },
                );
                if (!ok) return false;
              }
              {
                // The stage keeps the agent it was launched with.
                const stage = values.role.toLowerCase();
                const current = await getDeliveryRepoSettings(dialog.task.repo);
                const fixed = current.stages?.[stage];
                const same =
                  fixed?.agent === values.agent &&
                  (fixed.model ?? "") === values.model &&
                  (fixed.reasoning ?? "") === values.reasoning;
                if (values.settings || !same) {
                  await setDeliveryRepoSettings(dialog.task.repo, {
                    ...current,
                    ...values.settings,
                    stages: {
                      ...current.stages,
                      [stage]: {
                        agent: values.agent,
                        model: values.model || null,
                        reasoning: values.reasoning || null,
                      },
                    },
                  });
                }
                await dispatchDeliveryJob({
                  taskId: dialog.task.id,
                  role: values.role,
                  agent: values.agent,
                  model: values.model,
                  reasoning: values.reasoning,
                  access: values.access,
                  prompt: values.prompt,
                  platform: values.platform,
                });
              }
              setDialog(null);
            }}
          />
        )}
        {dialog?.kind === "release" && (
          <ReleaseLeaseDialog
            name={dialog.name}
            note={dialog.note}
            onCancel={() => setDialog(null)}
            onSubmit={async (confirmation) => {
              await releaseDeliveryLease(dialog.name, confirmation);
              setDialog(null);
            }}
          />
        )}
        {dialog?.kind === "archived" && (
          <ArchivedTasksDialog
            onCancel={() => setDialog(null)}
            onUnarchive={async (taskId) => {
              const task = await unarchiveDeliveryTask(taskId);
              setDialog(null);
              setSelectedTaskId(task.id);
            }}
          />
        )}
        {dialog?.kind === "settings" && (
          <SettingsDialog
            capacity={capacitySetting}
            endpoint={overview?.coordinator ?? null}
            runs={overview?.runs ?? []}
            repos={repos}
            onCancel={() => setDialog(null)}
            onCapacity={(value) => void run(() => updateDeliverySettings(value))}
            onCreateRun={(repo, title, qaJiraComment) =>
              createDeliveryRun(repo, title, qaJiraComment)
            }
            onQaJiraComment={(target, post) =>
              void run(() => setDeliveryRunQaJiraComment(target.id, post))
            }
            onPause={(target, paused) => setDeliveryRunPaused(target.id, paused)}
            onTakeover={takeover}
            onClose={async (target) => {
              const ok = await confirm(
                `Se cerrará el lote "${target.title}". Sus tareas siguen disponibles.`,
                { title: "Cerrar lote", okLabel: "Cerrar" },
              );
              if (!ok) return false;
              await closeDeliveryRun(target.id);
            }}
          />
        )}
      </div>
    </DeliveryRefreshContext.Provider>
  );
}

function TaskDetail({
  task,
  overview,
  now,
  run,
  onDispatch,
  onOpenConversation,
  onContinueJob,
  onRemoved,
}: {
  task: DeliveryTask;
  overview: DeliveryOverview;
  now: number;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onDispatch: (role: string, prompt: string, source: string | null) => void;
  onOpenConversation: (agentType: string) => Promise<void>;
  onContinueJob: (job: DeliveryJob) => Promise<void>;
  onRemoved: () => void;
}) {
  const jobs = jobsOf(overview, task.id);
  // The latest QA's visual changes page: what the user reviews before the PR.
  const qaRecord = latestOfRole(jobs, "qa");
  const qaProblems = (overview.qa_problems ?? []).filter((problem) => problem.task_id === task.id);
  // The QA's problems that wait on the user: new ones, and fixes that did not hold.
  const qaProblemsToDecide = qaProblems.filter(
    (problem) => problem.status === "open" || problem.status === "still_failing",
  ).length;
  const newestFirst = [...jobs].reverse();
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const taskRun = overview.runs.find((candidate) => candidate.id === task.run_id);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(newestFirst[0]?.id ?? null);
  const latestUndoable = newestFirst.find(
    (job) => job.agent !== "shell" && !job.undone_at_ms && job.changes.length > 0,
  );
  const hasOpenJob = jobs.some(isOpen);
  const next = nextStageOf(overview, task.id);
  const handoff = lastHandoff(jobs);
  const latest = newestFirst.find(
    (job) => job.agent !== "shell" && job.result_state !== "stale" && !job.undone_at_ms,
  );
  const broken = latest && !hasOpenJob && isBroken(latest) ? latest : null;
  const openApprovals = approvals.filter(
    (approval) => approval.status === "pending" || approval.status === "approved",
  );
  const step = openApprovals.length === 0 ? nextStep(approvals) : null;
  const [requesting, setRequesting] = useState(false);
  // What opens on top of the task: its pending decisions or QA's problems.
  const [float, setFloat] = useState<"decisions" | "qa" | null>(null);
  // The worktree in the files view, compared with the commit it started from.
  const { openRepo } = useWorkspaceActions();
  const openChanges = async () => {
    const changes = await getDeliveryTaskChanges(task.id);
    setBaseView(changes.repo, {
      label: `${task.base_ref} (${shortCommit(changes.base_commit)})`,
      taskId: task.id,
      files: changes.files,
    });
    openRepo(changes.repo);
  };
  // The issue's page, when the repo's Jira is set in Tinto.
  const [issueUrl, setIssueUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    getDeliveryIssueUrl(task.id).then(
      (url) => {
        if (live) setIssueUrl(url);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [task.id]);
  // The approval whose result is being corrected.
  const [correcting, setCorrecting] = useState<string | null>(null);
  const correctable = correctableSteps(approvals);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  // Opening the task checks its steps against GitHub: the PR is found by its
  // branch if needed and read, unless it was read moments ago.
  useEffect(() => {
    void checkDeliveryTask(task.id).catch(() => {});
  }, [task.id]);
  const taskDecisions = (overview.decisions ?? []).filter(
    (decision) => decision.task_id === task.id,
  );
  const pendingDecisions = taskDecisions.filter((decision) => decision.status === "pending");
  const blocked = pendingDecisions.length > 0;
  // An approved step waits on the coordinator, not on the user.
  const needsYou =
    blocked || openApprovals.some((approval) => approval.status === "pending") || broken !== null;
  const rejected = rejectedStep(approvals);
  // The coordinator's label stops at the stages; the delivery is Tinto's to know.
  const delivered = isDelivered(approvals);
  const stateLabel = delivered ? "Entregada" : (TASK_STATE_LABEL[task.state] ?? task.state);
  // The task's code as its latest stage found or left it.
  const withCode = newestFirst.find((job) => job.end_candidate ?? job.start_candidate);
  const candidate = (withCode?.end_candidate ?? withCode?.start_candidate)?.split(":")[1] ?? null;

  const dispatch = (role: string, withHandoff: boolean) => {
    const use = withHandoff && handoff !== null;
    onDispatch(
      role,
      use ? handoff.text : "",
      use
        ? `«Para la siguiente etapa» de ${stageLabel(handoff.from.role)} · intento ${handoff.from.attempt}`
        : null,
    );
  };

  const focusStage = (job: DeliveryJob) => {
    setExpandedJobId(job.id);
    document
      .getElementById(`delivery-stage-${task.id}-${job.role}`)
      ?.scrollIntoView?.({ block: "nearest" });
  };

  const undo = async (job: DeliveryJob) => {
    const ok = await confirm(
      `Se restaurarán ${files(job.changes.length)} al estado en que estaban cuando empezó este intento.`,
      { title: "Deshacer intento", kind: "warning", okLabel: "Deshacer" },
    );
    if (!ok) return false;
    await undoDeliveryJob(job.id);
  };

  const remove = async () => {
    const ok = await confirm(
      `Se eliminarán el worktree ${task.worktree} y la rama local ${task.branch}. Lo publicado sigue en GitHub.`,
      { title: "Eliminar tarea", kind: "warning", okLabel: "Eliminar" },
    );
    if (!ok) return false;
    {
      try {
        await removeDeliveryTask(task.id, false);
      } catch (cause) {
        const category =
          cause && typeof cause === "object" && "category" in cause
            ? String((cause as { category: unknown }).category)
            : "";
        if (category !== "task_unsaved") throw cause;
        const force = await confirm(
          `${errorText(cause)}. Si eliminas la tarea, se pierden con el worktree y la rama local ${task.branch}.`,
          { title: "Hay trabajo sin guardar", kind: "warning", okLabel: "Eliminar igualmente" },
        );
        if (!force) return false;
        await removeDeliveryTask(task.id, true);
      }
    }
    onRemoved();
  };

  const archive = async () => {
    const ok = await confirm(
      `Se borran el worktree ${task.worktree} y la rama local ${task.branch}: todo está en GitHub. La tarea sale de la lista y puedes recuperarla en "Archivadas…" por ${task.key} o por el número de su PR.`,
      { title: "Archivar tarea", okLabel: "Archivar" },
    );
    if (!ok) return false;
    await archiveDeliveryTask(task.id);
    onRemoved();
  };

  const bumpContract = async () => {
    const version = task.contract_version + 1;
    const ok = await confirm(
      `La tarea pasa a la versión ${version} del contrato. Los resultados que están en curso quedarán obsoletos y no harán avanzar la tarea.`,
      { title: "Nueva versión del contrato", kind: "warning", okLabel: `Subir a v${version}` },
    );
    if (!ok) return false;
    await updateDeliveryTask(task.id, { contractVersion: version });
  };

  // Stages with the most recent activity first; older attempts fold under the latest.
  const historyRoles = [...new Set(newestFirst.map((job) => job.role))];

  return (
    <article className="delivery-task" aria-label={`Tarea ${task.key}`}>
      <header className="delivery-task__header">
        <div className="delivery-task__heading">
          <h2>
            {task.key}
            {task.title && <span> {task.title}</span>}
            {issueUrl && (
              <ExternalLink
                className="delivery-task__issue"
                href={issueUrl}
                label={`Abrir ${task.key} en Jira`}
              >
                Abrir en Jira ↗
              </ExternalLink>
            )}
            {pr && (
              <ExternalLink
                className="delivery-task__pr"
                href={pr.url}
                label={`Abrir el PR #${pr.number} en GitHub`}
              >
                Abrir PR #{pr.number} en GitHub ↗
              </ExternalLink>
            )}
            {(qaRecord?.visual_record || qaProblems.length > 0) && (
              // The QA's record and its problems, as one split pill.
              <span className="delivery-task__qa">
                {qaRecord?.visual_record && (
                  <ActionButton
                    className="delivery-task__record"
                    aria-label="Abrir registro visual de QA"
                    title={`Los cambios visuales que revisó la QA (intento ${qaRecord.attempt}), con capturas de escritorio y móvil. Se abre en tu navegador.`}
                    onAction={() => openDeliveryVisualRecord(qaRecord.id)}
                  >
                    Registro visual QA ↗
                  </ActionButton>
                )}
                <button
                  type="button"
                  className="delivery-task__record delivery-task__record--problems"
                  aria-expanded={float === "qa"}
                  aria-label={
                    qaProblemsToDecide > 0
                      ? `Problemas de la QA, ${qaProblemsToDecide} por decidir`
                      : "Problemas de la QA"
                  }
                  onClick={() => setFloat(float === "qa" ? null : "qa")}
                >
                  Problemas
                  {qaProblemsToDecide > 0 && (
                    <span className="delivery-task__record-count">{qaProblemsToDecide} !</span>
                  )}{" "}
                  ▾
                </button>
              </span>
            )}
          </h2>
          <p className="delivery-task__meta">
            <span>
              <code title={task.worktree}>{task.branch}</code> desde {task.base_ref} (
              {shortCommit(task.base_commit)})
            </span>
            {taskRun && <span>{batchName(taskRun.title || taskRun.id)}</span>}
            <span title={delivered ? undefined : "Lo fija quien coordina la tarea"}>
              Estado: {stateLabel}
            </span>
            {!task.distro && (
              <ActionButton
                className="delivery-link-button"
                title={`Abre el worktree en Archivos con solo lo que cambió desde ${task.base_ref}`}
                onAction={openChanges}
              >
                Ver cambios
              </ActionButton>
            )}
          </p>
        </div>
        <Menu label="Más acciones de la tarea" trigger={<Icon name="more" />}>
          {(close) => (
            <>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void navigator.clipboard?.writeText(task.worktree).catch(() => {});
                }}
              >
                Copiar ruta del worktree
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void run(bumpContract);
                }}
              >
                Nueva versión del contrato… <small>v{task.contract_version}</small>
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={correctable.length === 0}
                title={
                  correctable.length === 0
                    ? "Aún no hay un PR, merge o Jira hecho que corregir."
                    : undefined
                }
                onClick={() => {
                  close();
                  setCorrecting(correctable[0].id);
                }}
              >
                Corregir resultado…
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!delivered || hasOpenJob}
                title={delivered ? undefined : "Se archiva una vez entregada."}
                onClick={() => {
                  close();
                  void run(archive);
                }}
              >
                Archivar tarea…
              </button>
              <hr />
              <button
                type="button"
                role="menuitem"
                className="delivery-menu__danger"
                disabled={hasOpenJob}
                title={hasOpenJob ? "Cancela o espera a que terminen sus etapas." : undefined}
                onClick={() => {
                  close();
                  void run(remove);
                }}
              >
                Eliminar tarea…
              </button>
            </>
          )}
        </Menu>
      </header>

      {/* One line in the task; the cards open on top only when asked for. */}
      {blocked && (
        <div className="delivery-pending-bar" role="status">
          <span>
            <strong>
              {pendingDecisions.length === 1
                ? "1 decisión pendiente"
                : `${pendingDecisions.length} decisiones pendientes`}
            </strong>{" "}
            <span className="delivery-muted">· las etapas de esta tarea esperan tu respuesta</span>
          </span>
          <button
            type="button"
            className="delivery-button--primary"
            onClick={() => setFloat("decisions")}
          >
            Responder
          </button>
        </div>
      )}
      {blocked && float === "decisions" && (
        <TaskFloat
          label="Responder decisiones"
          closeLabel="Cerrar decisiones"
          onClose={() => setFloat(null)}
        >
          <PendingDecisions taskId={task.id} decisions={pendingDecisions} />
        </TaskFloat>
      )}
      {float === "qa" && (
        <TaskFloat
          label="Problemas de la QA"
          closeLabel="Cerrar problemas de la QA"
          onClose={() => setFloat(null)}
        >
          <QaProblems taskId={task.id} problems={qaProblems} />
        </TaskFloat>
      )}

      <PendingSettingsProposals proposals={pendingSettingsProposals(overview, task)} />

      {openApprovals.map((approval) => (
        <ApprovalBanner key={approval.id} task={task} approval={approval} pr={pr} />
      ))}

      {rejected && openApprovals.length === 0 && (
        <RejectedStep task={task} approval={rejected} pr={pr} />
      )}

      {broken && (
        <div
          className="delivery-banner delivery-banner--danger"
          role="group"
          aria-label="Etapa fallida"
        >
          <p className="delivery-banner__title">
            <Icon name="x" />
            <strong>
              {stageLabel(broken.role)} {brokenVerb(broken)}
            </strong>
            <span className="delivery-muted">intento {broken.attempt}</span>
          </p>
          {broken.error && <p>{broken.error}</p>}
          {broken.result_note && <p>{broken.result_note}</p>}
          {broken.result?.status === "blocked" && <p>{broken.result.summary}</p>}
          {broken.changes.length > 0 && !broken.undone_at_ms && (
            <p className="delivery-muted">Dejó {files(broken.changes.length)} cambiados.</p>
          )}
          <ActionGroup>
            <div className="delivery-actions">
              <ActionButton
                className="delivery-button--primary"
                onAction={() => retryDeliveryJob(broken.id)}
              >
                Reintentar
              </ActionButton>
              {broken.agent !== "shell" && broken.provider_session_id && (
                <ActionButton onAction={() => onContinueJob(broken)}>
                  Continuar en Agents
                </ActionButton>
              )}
              {latestUndoable?.id === broken.id && (
                <ActionButton onAction={() => undo(broken)}>Deshacer cambios</ActionButton>
              )}
            </div>
          </ActionGroup>
        </div>
      )}

      {qaRecord?.visual_record_error && !qaRecord.visual_record && (
        <div
          className="delivery-banner delivery-banner--danger"
          role="group"
          aria-label="Registro visual de QA"
        >
          <p className="delivery-banner__title">
            <Icon name="x" />
            <strong>No se pudo generar el registro visual de la QA</strong>
          </p>
          <p>{qaRecord.visual_record_error}</p>
        </div>
      )}

      <StageTrack
        repo={task.repo}
        jobs={jobs}
        approvals={approvals}
        pr={pr}
        next={next}
        onFocus={focusStage}
      />

      <div className="delivery-actions">
        {next && !broken && (
          <button
            type="button"
            className={needsYou ? undefined : "delivery-button--primary"}
            disabled={blocked}
            onClick={() => dispatch(next, true)}
          >
            <Icon name="play" />
            Lanzar {lower(stageLabel(next))}
          </button>
        )}
        <button type="button" disabled={blocked} onClick={() => dispatch(next ?? "tests", false)}>
          {next && !broken ? "Otra etapa…" : "Lanzar etapa…"}
        </button>
        <Menu
          trigger={
            <>
              Abrir en Agents <Icon name="down" />
            </>
          }
        >
          {(close) =>
            (["codex", "claude", "muse"] as const).map((agent) => (
              <button
                key={agent}
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void onOpenConversation(agent);
                }}
              >
                Nueva conversación con {agentName(agent)}
              </button>
            ))
          }
        </Menu>
        {step && !requesting && (
          <button type="button" onClick={() => setRequesting(true)}>
            {latestStep(approvals, step)?.status === "drifted"
              ? `Volver a pedir el ${lower(step.label)}…`
              : `Preparar ${lower(step.label)}…`}
          </button>
        )}
        {blocked && (
          <span className="delivery-muted">
            Contesta las decisiones pendientes para lanzar etapas.
          </span>
        )}
        {!next && !hasOpenJob && !needsYou && (
          <span className="delivery-muted">
            {step
              ? `Todas las etapas pasaron. Lo siguiente es ${nextStepName(step)}.`
              : "Tarea entregada."}
          </span>
        )}
        {delivered && !hasOpenJob && (
          <ActionButton onAction={archive}>Archivar tarea…</ActionButton>
        )}
      </div>

      <StepChecks task={task} approvals={approvals} pr={pr} />

      <CommitStep approvals={approvals} />

      {step?.rung === "commit" && requesting && (
        <CommitPlanForm task={task} onClose={() => setRequesting(false)} />
      )}
      {step && step.rung !== "commit" && requesting && (
        <DeliveryRequest
          key={stepKey(step)}
          task={task}
          step={step}
          pr={pr}
          onClose={() => setRequesting(false)}
        />
      )}

      {pr && <PrSection pr={pr} />}

      <DeliveryResults approvals={approvals} onCorrect={setCorrecting} />
      {correcting && (
        <CorrectResultDialog
          steps={correctable}
          initial={correcting}
          onCancel={() => setCorrecting(null)}
          onSubmit={async (approvalId, outcome, reason) => {
            await correctDeliveryApproval(approvalId, outcome, reason);
            setCorrecting(null);
          }}
        />
      )}

      <CoordinatorDecisions
        decisions={taskDecisions.filter((decision) => decision.blocking === false)}
      />

      <DecisionLog
        taskId={task.id}
        candidate={candidate}
        decisions={taskDecisions.filter(
          (decision) => decision.status === "answered" && decision.blocking !== false,
        )}
      />

      <section className="delivery-section" aria-label="Historial">
        <h3>Historial</h3>
        {jobs.length === 0 && (
          <p className="delivery-panel__empty">Todavía no se ha lanzado ninguna etapa.</p>
        )}
        <ul className="delivery-jobs">
          {historyRoles.map((role) => {
            const current = latestOfRole(jobs, role)!;
            const older = newestFirst.filter((job) => job.role === role && job !== current);
            return (
              <StageHistory
                key={role}
                id={`delivery-stage-${task.id}-${role}`}
                current={current}
                older={older}
                jobs={jobs}
                renderJob={(job) => (
                  <JobRow
                    key={job.id}
                    job={job}
                    jobs={jobs}
                    now={now}
                    expanded={expandedJobId === job.id}
                    undoable={latestUndoable?.id === job.id && !hasOpenJob}
                    onToggle={() => setExpandedJobId(expandedJobId === job.id ? null : job.id)}
                    onUndo={() => undo(job)}
                    onContinue={() => onContinueJob(job)}
                  />
                )}
              />
            );
          })}
        </ul>
      </section>
    </article>
  );
}

function brokenVerb(job: DeliveryJob): string {
  if (job.status === "interrupted") return "se interrumpió";
  if (job.status === "failed") return "falló";
  if (job.result_state === "invalid") return "terminó sin un resultado válido";
  return "está bloqueada";
}

/** Who runs a stage from now on in the task's repo: the same setting as
 *  Ajustes › Agente de cada etapa, saved as soon as it changes. */
function StageAgentEditor({ repo, role }: { repo: string; role: string }) {
  const [settings, setSettings] = useState<DeliveryRepoSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const stage = role.toLowerCase();
  useEffect(() => {
    let live = true;
    getDeliveryRepoSettings(repo).then(
      (loaded) => live && setSettings(loaded),
      (cause) => live && setError(errorText(cause)),
    );
    return () => {
      live = false;
    };
  }, [repo]);
  const save = async (next: DeliveryStageAgent | null) => {
    setError(null);
    try {
      // The latest settings, so nothing changed elsewhere is lost.
      const current = await getDeliveryRepoSettings(repo);
      const stages = { ...current.stages };
      if (next) stages[stage] = next;
      else delete stages[stage];
      await setDeliveryRepoSettings(repo, { ...current, stages });
      setSettings({ ...current, stages });
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  return (
    <div className="delivery-stage-agent">
      <p className="delivery-muted">
        Para los próximos lanzamientos de {lower(stageLabel(role))} en este repositorio, tuyos o del
        coordinador. Lo que ya corre no cambia.
      </p>
      {settings ? (
        <StageAgentRow
          label={stageLabel(role)}
          value={settings.stages?.[stage]}
          onChange={(next) => void save(next)}
        />
      ) : (
        !error && <p className="delivery-muted">Leyendo los ajustes…</p>
      )}
      {error && <p className="delivery-tone--danger">{error}</p>}
    </div>
  );
}

function StageTrack({
  repo,
  jobs,
  approvals,
  pr,
  next,
  onFocus,
}: {
  /** The task's repo, whose settings say who runs each stage. */
  repo: string;
  jobs: DeliveryJob[];
  approvals: DeliveryApproval[];
  /** The PR step shows the followed PR's live state once it exists. */
  pr: DeliveryPr | undefined;
  next: string | null;
  onFocus: (job: DeliveryJob) => void;
}) {
  return (
    <div className="delivery-track">
      <div className="delivery-track__part">
        <h3 id="delivery-track-agents">Agentes</h3>
        <ol aria-labelledby="delivery-track-agents">
          {rolesOf(jobs).map((role) => {
            const job = latestOfRole(jobs, role);
            const outcome = job
              ? attemptOutcome(job, jobs)
              : { tone: "todo" as const, label: role === next ? "Siguiente" : "—" };
            const tone = outcome.tone;
            const content = (
              <>
                <span className="delivery-node__name">
                  <StatusMark icon={tone === "todo" ? "ring" : TONE_ICON[tone]} />
                  {stageLabel(role)}
                </span>
                <small>{outcome.label}</small>
                {job?.visual_record && <small>Con registro visual ↓</small>}
                {job && job.agent !== "shell" && (
                  <small className="delivery-node__runtime">{runtimeLabel(job)}</small>
                )}
              </>
            );
            return (
              <li
                key={role}
                className={`delivery-node delivery-node--${tone}`}
                aria-current={role === next ? "step" : undefined}
              >
                {job ? (
                  <button
                    type="button"
                    title={`${stageLabel(role)} · intento ${job.attempt} · ${agentName(job.agent)} · ${runtimeLabel(job)}`}
                    onClick={() => onFocus(job)}
                  >
                    {content}
                  </button>
                ) : (
                  <div>{content}</div>
                )}
                <Menu
                  label={`Agente, modelo y razonamiento de ${stageLabel(role)}`}
                  trigger={<Icon name="down" />}
                >
                  {() => <StageAgentEditor repo={repo} role={role} />}
                </Menu>
              </li>
            );
          })}
        </ol>
      </div>
      <div className="delivery-track__part delivery-track__part--rungs">
        <h3 id="delivery-track-rungs">Entrega</h3>
        <ol aria-labelledby="delivery-track-rungs">
          {STEPS.map((ladder) => {
            const { rung, label } = ladder;
            const live = rung === "pr" && pr ? prSummary(pr) : null;
            const approval = latestStep(approvals, ladder);
            const unlinked = rung === "pr" && !pr && approval?.status === "executed";
            const step = live
              ? { ...live, title: `${pr?.repo}#${pr?.number}` }
              : unlinked
                ? {
                    tone: "step" as const,
                    icon: "alert" as const,
                    label: "Hecho · sin PR vinculado",
                    title: approval.outcome ?? undefined,
                  }
                : rungStep(approval);
            return (
              <li key={stepKey(ladder)} className={`delivery-node delivery-node--${step.tone}`}>
                <div title={step.title}>
                  <span className="delivery-node__name">
                    <StatusMark icon={step.icon} />
                    {label}
                  </span>
                  <small>{step.label}</small>
                </div>
              </li>
            );
          })}
        </ol>
      </div>
    </div>
  );
}

function rungStep(approval: DeliveryApproval | undefined): {
  tone: Tone | "todo";
  icon: StatusIcon;
  label: string;
  title?: string;
} {
  if (!approval) return { tone: "todo", icon: "ring", label: "—" };
  switch (approval.status) {
    case "pending":
      return { tone: "step", icon: "flag", label: "Tu aprobación" };
    case "approved":
      return { tone: "waiting", icon: "clock", label: "Lo hace el coordinador" };
    case "executed":
      return {
        tone: "ok",
        icon: "check",
        label: approval.outcome || "Hecho",
        title: approval.outcome ?? undefined,
      };
    case "rejected":
      return { tone: "danger", icon: "x", label: "Rechazado" };
    case "undone":
      return {
        tone: "todo",
        icon: "ring",
        label: "Deshecho",
        title: approval.outcome ?? undefined,
      };
    case "drifted":
      return {
        tone: "step",
        icon: "alert",
        label: "Ya no coincide",
        title: approval.notice ?? undefined,
      };
    default:
      return { tone: "danger", icon: "x", label: "Falló", title: approval.outcome ?? undefined };
  }
}

/** Who prepared a request. One made in Tinto's own view may come from the
 *  user or from whoever drives the view for them; the API names its caller. */
function preparedBy(approval: DeliveryApproval): string {
  return approval.requested_by === "user"
    ? "preparado desde la interfaz de Tinto"
    : `preparado por el coordinador ${approval.requested_by}`;
}

/** What Tinto found when it checked the delivery steps against git and
 *  GitHub, with the way to put each one right. */
function StepChecks({
  task,
  approvals,
  pr,
}: {
  task: DeliveryTask;
  approvals: DeliveryApproval[];
  pr: DeliveryPr | undefined;
}) {
  type Check = {
    text: string;
    action: { label: string; run: () => Promise<unknown> } | null;
  };
  const checks = STEPS.flatMap((step): Check[] => {
    const { rung, label } = step;
    const approval = latestStep(approvals, step);
    if (!approval) return [];
    if (rung === "pr" && approval.status === "executed" && !pr) {
      return [
        {
          text: "PR: el paso está hecho, pero la tarea no tiene un PR vinculado.",
          action: { label: "Buscar el PR de la rama", run: () => findDeliveryPr(task.id) },
        },
      ];
    }
    if (!approval.notice) return [];
    const drifted = approval.status === "drifted";
    return [
      {
        text: `${label}: ${drifted ? "ya no coincide. " : ""}${approval.notice}.`,
        action:
          drifted && rung === "pr"
            ? { label: "Buscar otro PR de la rama", run: () => findDeliveryPr(task.id) }
            : null,
      },
    ];
  });
  if (checks.length === 0) return null;
  return (
    <div className="delivery-step-checks" role="group" aria-label="Comprobaciones de la entrega">
      {checks.map(({ text, action }) => (
        <p key={text}>
          <span className="delivery-job__note">{text}</span>
          {action && <ActionButton onAction={action.run}>{action.label}</ActionButton>}
        </p>
      ))}
    </div>
  );
}

/** A panel on top of the task, opened by the user: it takes the focus once
 *  (later refreshes must not take it from a card) and closes on Escape. */
function TaskFloat({
  label,
  closeLabel,
  onClose,
  children,
}: {
  label: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    panel.current?.focus();
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
    // Once, when it opens; onClose only closes it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <div className="delivery-float" role="dialog" aria-label={label} tabIndex={-1} ref={panel}>
      <button
        type="button"
        className="delivery-float__close"
        aria-label={closeLabel}
        onClick={onClose}
      >
        ×
      </button>
      {children}
    </div>
  );
}

/** "el merge", "el PR", "Jira (merge)": the step as a sentence names it. */
function nextStepName(step: DeliveryStep): string {
  return step.rung === "jira" ? step.label : `el ${lower(step.label)}`;
}

function when(ms: number | null | undefined): string {
  return ms ? new Date(ms).toLocaleString([], { dateStyle: "short", timeStyle: "short" }) : "";
}

const isDone = (approval: DeliveryApproval | undefined): approval is DeliveryApproval =>
  approval?.status === "executed" || approval?.status === "drifted";

/** Done steps whose result Tinto cannot check against git: the user's to
 *  correct. Commit and push are checked, so they are not here. */
function correctableSteps(approvals: DeliveryApproval[]): CorrectableStep[] {
  return STEPS.filter(({ rung }) => rung !== "commit" && rung !== "push").flatMap((step) => {
    const approval = latestStep(approvals, step);
    return isDone(approval)
      ? [{ id: approval.id, label: step.label, outcome: approval.outcome ?? "" }]
      : [];
  });
}

/** Each done step's result, with the corrections the user made and when. */
function DeliveryResults({
  approvals,
  onCorrect,
}: {
  approvals: DeliveryApproval[];
  onCorrect: (approvalId: string) => void;
}) {
  const done = STEPS.flatMap((step) => {
    const approval = latestStep(approvals, step);
    return isDone(approval) ? [{ step, approval }] : [];
  });
  if (done.length === 0) return null;
  return (
    <section className="delivery-section" aria-label="Resultados de la entrega">
      <h3>Resultados de la entrega</h3>
      <ul className="delivery-results">
        {done.map(({ step: { rung, label, stage }, approval }) => {
          const corrections = approval.corrections ?? [];
          // Oldest first: what was recorded, then each correction.
          const history = [
            {
              text: corrections[0]?.previous ?? approval.outcome ?? "—",
              at: approval.executed_at_ms,
              reason: null as string | null,
            },
            ...corrections.map((c) => ({ text: c.outcome, at: c.at_ms, reason: c.reason })),
          ];
          return (
            <li key={stage ?? rung} aria-label={label}>
              <strong>{label}</strong>
              <ol className="delivery-results__history">
                {history.map((entry, index) => (
                  <li
                    key={index}
                    className={index < history.length - 1 ? "delivery-muted" : undefined}
                  >
                    <span>{entry.text}</span>{" "}
                    <small className="delivery-muted">
                      {index === 0 ? when(entry.at) : `Corregido el ${when(entry.at)}`}
                      {entry.reason && ` · Por qué: ${entry.reason}`}
                    </small>
                  </li>
                ))}
              </ol>
              {rung !== "commit" && rung !== "push" && (
                <button
                  type="button"
                  className="delivery-link-button"
                  onClick={() => onCorrect(approval.id)}
                >
                  Corregir resultado…
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** What happened to the task's last commit step when it is not simply
 *  done, and the way to take back commits that are not pushed yet. */
function CommitStep({ approvals }: { approvals: DeliveryApproval[] }) {
  const commit = latestApproval(approvals, "commit");
  if (!commit) return null;
  const pushed = latestApproval(approvals, "push")?.status === "executed";
  // Steps recorded before plans existed only have "Commit <hash>" as outcome.
  const made =
    commit.created?.length ||
    (commit.status === "executed" && commit.outcome?.startsWith("Commit ") ? 1 : 0);
  const undoable =
    !pushed && made > 0 && (commit.status === "executed" || commit.status === "failed");
  const notice = commit.status === "undone" || commit.status === "failed";
  if (!undoable && !notice) return null;
  return (
    <div className="delivery-commit-step" role="group" aria-label="Paso de commit">
      {notice && commit.outcome && <p className="delivery-job__note">{commit.outcome}</p>}
      {undoable && (
        <ActionButton
          title="Los cambios vuelven al worktree, sin confirmar; no se pierde nada."
          onAction={async () => {
            const ok = await confirm(
              `La rama volverá a donde estaba antes de ${made === 1 ? "el commit" : `los ${made} commits`} y sus cambios quedarán en el worktree, sin confirmar. No se pierde nada.`,
              { title: "Deshacer el commit", okLabel: "Deshacer" },
            );
            if (!ok) return false;
            await undoDeliveryCommit(commit.id);
          }}
        >
          {made === 1 ? "Deshacer el commit" : `Deshacer los ${made} commits`}
        </ActionButton>
      )}
    </div>
  );
}

/** The latest step, when the user rejected it and nothing was asked since. */
function rejectedStep(approvals: DeliveryApproval[]): DeliveryApproval | null {
  const latest = [...approvals].sort((a, b) => b.requested_at_ms - a.requested_at_ms)[0];
  return latest?.status === "rejected" ? latest : null;
}

/** What stays as it is when the user rejects a step: a rejection only stops
 *  that step, it undoes none before it. */
function rejectionScope(
  approval: DeliveryApproval,
  task: DeliveryTask,
  pr: DeliveryPr | undefined,
) {
  const openPr = pr && pr.state === "open" ? `El PR #${pr.number} sigue abierto en GitHub` : null;
  switch (approval.rung) {
    case "commit":
      return `Rechazar no toca los cambios: siguen sin commit en ${task.branch}.`;
    case "push":
      return `Rechazar no deshace los commits: siguen en ${task.branch}, sin publicar.`;
    case "pr":
      return `Rechazar no borra nada: la rama ${task.branch} sigue publicada en GitHub, sin PR.`;
    case "merge":
      return openPr
        ? `${openPr}; rechazar este paso solo evita fusionarlo.`
        : "Rechazar este paso solo evita fusionar el PR.";
    default:
      if (approval.stage === "after_merge")
        return "El PR ya está fusionado; rechazar este paso solo evita los cambios en Jira.";
      return openPr
        ? `${openPr}; rechazar este paso solo evita los cambios en Jira.`
        : "Rechazar este paso solo evita los cambios en Jira.";
  }
}

/** A rejected step: what still stands, and how to undo the PR when it is open. */
function RejectedStep({
  task,
  approval,
  pr,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  pr: DeliveryPr | undefined;
}) {
  const openPr = pr?.state === "open";
  return (
    <div className="delivery-banner delivery-banner--step" role="group" aria-label="Paso rechazado">
      <p className="delivery-banner__title">
        <Icon name="x" />
        <strong>Rechazaste {lower(stepOf(approval).label)}</strong>
      </p>
      <p className="delivery-muted">{rejectionScope(approval, task, pr)}</p>
      {openPr && (
        <ActionGroup>
          <div className="delivery-actions">
            {!pr.draft && (
              <ActionButton onAction={() => deliveryPrToDraft(task.id)}>
                Pasar el PR a borrador
              </ActionButton>
            )}
            <ActionButton onAction={() => closeDeliveryPr(task.id)}>
              Cerrar el PR #{pr.number}
            </ActionButton>
          </div>
        </ActionGroup>
      )}
    </div>
  );
}

function ApprovalBanner({
  task,
  approval,
  pr,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  pr: DeliveryPr | undefined;
}) {
  const [outcome, setOutcome] = useState("");
  const name = lower(stepOf(approval).label);
  const pending = approval.status === "pending";
  const choosesReviewers = pending && approval.rung === "pr";
  const [reviewers, setReviewers] = useState<string[]>(approval.reviewers ?? []);
  // The repo's people who can review, or why they could not be read.
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [candidatesError, setCandidatesError] = useState<string | null>(null);
  useEffect(() => {
    if (!choosesReviewers) return;
    let live = true;
    getDeliveryPrReviewers(task.id).then(
      (logins) => live && setCandidates(logins),
      (cause) => live && setCandidatesError(errorText(cause)),
    );
    return () => {
      live = false;
    };
  }, [choosesReviewers, task.id]);
  // Without a reviewer nobody can approve the PR, and it cannot be merged.
  const needsReviewer = choosesReviewers && (candidates?.length ?? 0) > 0 && reviewers.length === 0;
  const consequence =
    approval.rung === "commit"
      ? `Al aprobar, Tinto crea ${(approval.commits?.length ?? 1) > 1 ? "estos commits, en este orden," : "el commit"} en ${task.branch} con estos textos exactos y sin añadir firmas. Si uno falla, se detiene. No publica nada: el push se aprueba aparte, y hasta entonces puedes deshacerlo.`
      : approval.rung === "push"
        ? `Al aprobar, Tinto publica la rama ${task.branch}.`
        : approval.rung === "pr"
          ? `Al aprobar, Tinto abre en GitHub el PR de ${task.branch} con este título y cuerpo exactos (o se los pone al PR abierto de la rama). Desde entonces lo sigue y pide la revisión a quienes elijas. Sin una revisión aprobada no se podrá fusionar.`
          : approval.rung === "merge"
            ? `Al aprobar, Tinto fusiona el PR en GitHub (${approval.merge_method ?? "squash"}) con este mensaje y borra allí la rama ${task.branch}. La rama local sigue hasta que elimines la tarea.`
            : approval.jira_statuses?.length
              ? `Al aprobar, Tinto ${approval.stage === "after_merge" ? "" : "enlaza el PR en la issue y "}lleva ${task.key} a ${approval.jira_statuses.join(" → ")} en Jira, y comprueba el estado final.`
              : "Al aprobar, el coordinador hace estas operaciones en Jira y las informa; no tienes que hacer nada más.";
  return (
    <div
      className="delivery-banner delivery-banner--step"
      role="group"
      aria-label={`Aprobación ${approval.rung}`}
    >
      <p className="delivery-banner__title">
        <Icon name="flag" />
        <strong>{pending ? `Aprobar ${name}` : `Esperando al coordinador: ${name}`}</strong>
        <span className="delivery-muted">
          {pending ? preparedBy(approval) : "aprobado; el coordinador lo hace y lo informa"}
        </span>
      </p>
      {!pending && approval.notice && <p className="delivery-job__note">{approval.notice}</p>}
      {!pending && approval.rung === "pr" && (
        <div className="delivery-actions">
          <ActionButton
            className="delivery-button--primary"
            onAction={() => openDeliveryPr(approval.id)}
          >
            Abrir el PR en GitHub
          </ActionButton>
          <span className="delivery-muted">
            Tinto lo abre desde {task.branch} con el título y el cuerpo aprobados.
          </span>
        </div>
      )}
      {approval.rung === "commit" ? (
        <>
          <CommitPlanView approval={approval} />
          {approval.single_commit_reason && (
            <p className="delivery-job__note">
              Un solo commit porque: {approval.single_commit_reason}
            </p>
          )}
        </>
      ) : (
        <pre className="delivery-banner__text">
          {approval.title}
          {approval.body ? `\n\n${approval.body}` : ""}
        </pre>
      )}
      {choosesReviewers && (
        <fieldset className="delivery-reviewers">
          <legend>Revisores</legend>
          {candidates === null && !candidatesError && (
            <p className="delivery-muted">Leyendo quién puede revisar en GitHub…</p>
          )}
          {candidatesError && (
            <p className="delivery-job__note">
              No se pudo leer quién puede revisar: {candidatesError}. Pídelo en GitHub.
            </p>
          )}
          {candidates?.length === 0 && (
            <p className="delivery-job__note">
              El repositorio no tiene a nadie más que pueda revisar.
            </p>
          )}
          {candidates?.map((login) => (
            <label key={login}>
              <input
                type="checkbox"
                checked={reviewers.includes(login)}
                onChange={(event) =>
                  setReviewers(
                    event.target.checked
                      ? [...reviewers, login]
                      : reviewers.filter((chosen) => chosen !== login),
                  )
                }
              />
              {login}
            </label>
          ))}
        </fieldset>
      )}
      {pending && <p className="delivery-muted">{consequence}</p>}
      {pending ? (
        <ActionGroup>
          <div className="delivery-actions">
            <ActionButton
              className="delivery-button--primary"
              disabled={needsReviewer}
              title={needsReviewer ? "Elige al menos un revisor." : undefined}
              onAction={() =>
                choosesReviewers
                  ? decideDeliveryApproval(approval.id, true, undefined, reviewers)
                  : decideDeliveryApproval(approval.id, true)
              }
            >
              {approval.rung === "commit"
                ? "Aprobar y hacer commit"
                : approval.rung === "push"
                  ? "Aprobar y publicar"
                  : approval.rung === "merge"
                    ? "Aprobar y fusionar"
                    : approval.rung === "pr"
                      ? "Aprobar y abrir el PR"
                      : "Aprobar"}
            </ActionButton>
            <ActionButton
              title={rejectionScope(approval, task, pr)}
              onAction={() => decideDeliveryApproval(approval.id, false)}
            >
              Rechazar
            </ActionButton>
            <span className="delivery-muted">{rejectionScope(approval, task, pr)}</span>
          </div>
        </ActionGroup>
      ) : (
        // Only a correction: the coordinator reports the step. Nothing here
        // waits on it.
        <details className="delivery-manual-record">
          <summary>Lo hice fuera de Tinto: registrar el paso hecho</summary>
          <div className="delivery-actions">
            <input
              aria-label="Resultado del paso"
              placeholder={
                approval.rung === "pr"
                  ? "URL del PR (si no, Tinto lo busca por la rama)"
                  : "Qué se hizo"
              }
              value={outcome}
              onChange={(event) => setOutcome(event.target.value)}
            />
            <ActionButton
              disabled={approval.rung === "jira" && !outcome.trim()}
              title={
                approval.rung === "jira" && !outcome.trim()
                  ? "Escribe qué se hizo en Jira: transiciones y estado final."
                  : undefined
              }
              onAction={() => completeDeliveryApproval(approval.id, true, outcome)}
            >
              Marcar como hecho
            </ActionButton>
          </div>
        </details>
      )}
    </div>
  );
}

function DeliveryRequest({
  task,
  step,
  pr,
  onClose,
}: {
  task: DeliveryTask;
  step: DeliveryStep;
  /** The followed PR, for GitHub's own merge commit message. */
  pr: DeliveryPr | undefined;
  onClose: () => void;
}) {
  const named = task.title ? `${task.key}: ${task.title}` : task.key;
  const { rung } = step;
  // With a merge commit, GitHub's own message; otherwise the task's name.
  const mergeTitle = (method: string) =>
    method === "merge" && pr
      ? `Merge pull request #${pr.number} from ${pr.repo.split("/")[0]}/${task.branch}`
      : named;
  const [title, setTitle] = useState(rung === "push" ? `Publicar ${task.branch}` : named);
  // The commit step has its own form (CommitPlanForm).
  const [body, setBody] = useState("");
  const [method, setMethod] = useState("squash");
  // Once the user picks a method or writes the message, the defaults stop.
  const methodEdited = useRef(false);
  const titleEdited = useRef(false);
  const merge = rung === "merge";
  const name = lower(step.label);
  const chooseMethod = (next: string) => {
    setMethod(next);
    if (!titleEdited.current) setTitle(mergeTitle(next));
  };
  // The method the base branch's latest PRs were merged with.
  useEffect(() => {
    if (!merge) return;
    let live = true;
    getDeliveryMergeMethod(task.id).then(
      (proposed) => {
        if (live && !methodEdited.current) chooseMethod(proposed);
      },
      () => {},
    );
    return () => {
      live = false;
    };
    // Proposed once, when the form opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // A Jira step Tinto runs itself when the repo's Jira is set in Tinto.
  const [jiraByTinto, setJiraByTinto] = useState(false);
  const [statuses, setStatuses] = useState("");
  useEffect(() => {
    if (rung !== "jira") return;
    let live = true;
    getDeliveryRepoSettings(task.repo).then(
      (settings) => {
        const jira = settings.jira;
        if (!live || !jira?.has_token) return;
        setJiraByTinto(true);
        setStatuses((step.stage === "after_merge" ? jira.after_merge : jira.after_pr).join(", "));
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [rung, step.stage, task.repo]);
  return (
    <form
      className="delivery-request"
      aria-label={`Pedir aprobación de ${name}`}
      // The submit button sends it, also on Enter.
      onSubmit={(event) => event.preventDefault()}
    >
      {merge && (
        <label className="delivery-request__field">
          <span>Cómo fusionar</span>
          <select
            value={method}
            onChange={(event) => {
              methodEdited.current = true;
              chooseMethod(event.target.value);
            }}
          >
            <option value="squash">Squash: un solo commit</option>
            <option value="merge">Merge commit</option>
            <option value="rebase">Rebase</option>
          </select>
        </label>
      )}
      <label className="delivery-request__field">
        <span>
          {merge
            ? method === "rebase"
              ? "Descripción (rebase no crea un commit propio)"
              : "Mensaje del commit de fusión"
            : "Texto exacto a aprobar"}
        </span>
        <input
          value={title}
          onChange={(event) => {
            titleEdited.current = true;
            setTitle(event.target.value);
          }}
          required
        />
      </label>
      {jiraByTinto && (
        <label className="delivery-request__field">
          <span>Estados de {task.key} en Jira, en orden</span>
          <input
            value={statuses}
            onChange={(event) => setStatuses(event.target.value)}
            placeholder="En ejecución, In Review"
            required
          />
        </label>
      )}
      <label className="delivery-request__field">
        <span>
          {rung === "jira" && !jiraByTinto ? "Operaciones exactas en Jira" : "Detalle (opcional)"}
        </span>
        <textarea value={body} onChange={(event) => setBody(event.target.value)} rows={3} />
      </label>
      {jiraByTinto && (
        <p className="delivery-muted">
          Al aprobarlo, Tinto
          {step.stage === "after_pr" ? " enlaza el PR en la issue y la" : " la"} lleva por estos
          estados en Jira, y comprueba el estado final.
        </p>
      )}
      <div className="delivery-actions">
        <ActionButton
          type="submit"
          className="delivery-button--primary"
          onAction={async () => {
            await (merge
              ? requestDeliveryApproval(task.id, rung, title, body, undefined, method)
              : step.stage
                ? requestDeliveryApproval(
                    task.id,
                    rung,
                    title,
                    body,
                    undefined,
                    undefined,
                    step.stage,
                    jiraByTinto ? parseStatuses(statuses) : undefined,
                  )
                : requestDeliveryApproval(task.id, rung, title, body));
            onClose();
          }}
        >
          Pedir aprobación
        </ActionButton>
        <button type="button" onClick={onClose}>
          Cancelar
        </button>
      </div>
    </form>
  );
}

function StageHistory({
  id,
  current,
  older,
  jobs,
  renderJob,
}: {
  id: string;
  current: DeliveryJob;
  older: DeliveryJob[];
  /** Every job of the task, oldest first. */
  jobs: DeliveryJob[];
  renderJob: (job: DeliveryJob) => ReactNode;
}) {
  const [showOlder, setShowOlder] = useState(false);
  return (
    <>
      <li id={id} className="delivery-stage">
        <ul>{renderJob(current)}</ul>
      </li>
      {older.length > 0 && (
        <li className="delivery-older">
          <button type="button" aria-expanded={showOlder} onClick={() => setShowOlder(!showOlder)}>
            <Icon name={showOlder ? "down" : "right"} />
            {older.length === 1 ? "1 intento anterior" : `${older.length} intentos anteriores`}
            <span className="delivery-muted">
              · {older.map((job) => lower(attemptOutcome(job, jobs).label)).join(", ")}
            </span>
          </button>
          {showOlder && <ul>{older.map(renderJob)}</ul>}
        </li>
      )}
    </>
  );
}

function JobRow({
  job,
  jobs,
  now,
  expanded,
  undoable,
  onToggle,
  onUndo,
  onContinue,
}: {
  job: DeliveryJob;
  /** Every job of the task, oldest first. */
  jobs: DeliveryJob[];
  now: number;
  expanded: boolean;
  undoable: boolean;
  onToggle: () => void;
  onUndo: () => Promise<unknown>;
  onContinue: () => Promise<unknown>;
}) {
  const outcome = attemptOutcome(job, jobs);
  const quiet = quietFor(job, now);
  const held = pausedJob(jobs)?.id === job.id;
  const scope =
    (job.access === "full" ? " · acceso completo" : job.writes ? "" : " · solo lectura") +
    (job.qa_browser ? " · navegador" : "") +
    (job.ran_on ? ` · en ${job.ran_on}` : "");
  return (
    <li className="delivery-job">
      <button
        type="button"
        className="delivery-job__summary"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <Icon name={expanded ? "down" : "right"} />
        <span className="delivery-job__role">
          {stageLabel(job.role)} <small>· intento {job.attempt}</small>
        </span>
        <span className="delivery-job__agent">
          {agentName(job.agent)}
          {job.agent === "shell" ? "" : ` · ${runtimeLabel(job)}`}
          {scope}
        </span>
        <span className={`delivery-outcome delivery-tone--${outcome.tone}`}>
          <StatusMark icon={TONE_ICON[outcome.tone]} />
          {outcome.label}
        </span>
        <span className="delivery-job__changes">
          {job.changes.length > 0 ? files(job.changes.length) : ""}
        </span>
        <span className="delivery-job__time">
          {duration(job, now)}
          {job.status === "started" && (
            <small className="delivery-muted"> de {job.timeout_minutes} min</small>
          )}
        </span>
      </button>
      {job.status === "started" && job.activity && (
        <p className="delivery-job__activity" aria-label={`Actividad de ${stageLabel(job.role)}`}>
          <span>{job.activity}</span>
          <small className={quiet ? "delivery-tone--attention" : "delivery-muted"}>
            {quiet ?? ago(job.activity_at_ms, now)}
          </small>
        </p>
      )}
      {expanded && (
        <div className="delivery-job__detail">
          {job.error && <p className="delivery-job__error">{job.error}</p>}
          {job.result_note && <p className="delivery-job__note">{job.result_note}</p>}
          {job.substituted && <p className="delivery-muted">{job.substituted}</p>}
          {job.visual_record_error && (
            <p className="delivery-job__note">
              No se pudo generar el registro visual: {job.visual_record_error}
            </p>
          )}
          {isSuperseded(job, jobs) && (
            <p className="delivery-job__note">
              Una etapa anterior cambió el código después de este resultado, así que ya no cuenta:
              hay que repetir esta etapa.
            </p>
          )}
          {job.result && (
            <div className="delivery-result">
              <p>{job.result.summary}</p>
              {job.result.findings.length > 0 && (
                <>
                  <h4>Hallazgos</h4>
                  <ul>
                    {job.result.findings.map((finding, index) => (
                      <li key={index}>{finding}</li>
                    ))}
                  </ul>
                </>
              )}
              {job.result.checks.length > 0 && (
                <>
                  <h4>Comprobaciones</h4>
                  <ul>
                    {job.result.checks.map((check, index) => (
                      <li key={index}>
                        <code>{check.command}</code> — {check.result}
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {job.result.handoff && (
                <>
                  <h4>Para la siguiente etapa</h4>
                  <p>{job.result.handoff}</p>
                </>
              )}
            </div>
          )}
          {(job.plan?.length ?? 0) > 0 && (
            <>
              <h4 className="delivery-job__subtitle">Plan del agente</h4>
              <ul className="delivery-plan" aria-label="Plan del agente">
                {job.plan?.map((item, index) => (
                  <li key={index} data-status={item.status}>
                    <span aria-hidden>
                      {item.status === "completed"
                        ? "✓"
                        : item.status === "in_progress"
                          ? "▸"
                          : "○"}
                    </span>
                    {item.text}
                  </li>
                ))}
              </ul>
            </>
          )}
          {job.usage && <p className="delivery-job__usage">{usageLabel(job.usage)}</p>}
          {job.changes.length > 0 && (
            <>
              <h4 className="delivery-job__subtitle">
                Archivos{job.undone_at_ms ? " (deshechos)" : ""}
              </h4>
              <ul className="delivery-changes" aria-label="Archivos cambiados">
                {job.changes.map((change) => (
                  <li key={change.path}>
                    <span
                      className={`agent-panel__chat-turn-file-kind`}
                      data-change-kind={change.kind}
                      aria-hidden
                    >
                      {changeSymbol(change.kind)}
                    </span>
                    <code>{change.path}</code>
                  </li>
                ))}
              </ul>
            </>
          )}
          <JobLog key={job.id} job={job} />
          <details className="delivery-job__fold">
            <summary>Instrucciones</summary>
            <pre>{job.prompt}</pre>
          </details>
          <details className="delivery-job__fold">
            <summary>Detalles técnicos</summary>
            <dl className="delivery-job__facts">
              {(job.start_candidate || job.end_candidate) && (
                <>
                  <dt>Instantánea</dt>
                  <dd>
                    <code>
                      {shortCommit(job.start_candidate?.split(":")[1])}
                      {job.end_candidate
                        ? ` → ${shortCommit(job.end_candidate.split(":")[1])}`
                        : ""}
                    </code>
                  </dd>
                </>
              )}
              <dt>Contrato</dt>
              <dd>v{job.contract_version}</dd>
              <dt>Tiempo máximo</dt>
              <dd>{job.timeout_minutes} min</dd>
              {job.exit_code !== null && (
                <>
                  <dt>Código de salida</dt>
                  <dd>{job.exit_code}</dd>
                </>
              )}
              {job.provider_session_id && (
                <>
                  <dt>Sesión</dt>
                  <dd>
                    <code>{job.provider_session_id}</code>
                  </dd>
                </>
              )}
            </dl>
          </details>
          <ActionGroup>
            <div className="delivery-actions">
              {job.visual_record && (
                <ActionButton
                  className="delivery-button--primary"
                  title="Los cambios visuales que revisó la QA, con sus capturas de escritorio y móvil"
                  onAction={() => openDeliveryVisualRecord(job.id)}
                >
                  Abrir registro visual
                </ActionButton>
              )}
              {job.status === "started" && job.agent !== "shell" && (
                <ActionButton
                  title="Detiene el agente; al reanudar sigue en la misma sesión, con lo que ya hizo"
                  onAction={() => pauseDeliveryJob(job.id)}
                >
                  Pausar
                </ActionButton>
              )}
              {held && (
                <ActionButton
                  className="delivery-button--primary"
                  onAction={() => resumeDeliveryJob(job.id)}
                >
                  Reanudar
                </ActionButton>
              )}
              {(isOpen(job) || held) && (
                <ActionButton onAction={() => cancelDeliveryJob(job.id)}>Cancelar</ActionButton>
              )}
              {!isOpen(job) && job.status !== "paused" && job.agent !== "shell" && (
                <ActionButton onAction={() => retryDeliveryJob(job.id)}>Nuevo intento</ActionButton>
              )}
              {undoable && <ActionButton onAction={onUndo}>Deshacer estos cambios</ActionButton>}
              {!isOpen(job) && !held && job.agent !== "shell" && job.provider_session_id && (
                <ActionButton onAction={onContinue}>Continuar en Agents</ActionButton>
              )}
            </div>
          </ActionGroup>
        </div>
      )}
    </li>
  );
}

function JobLog({ job }: { job: DeliveryJob }) {
  const [log, setLog] = useState<DeliveryJobLog | null>(null);
  const live = isOpen(job);
  const nextLine = useRef(0);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const chunk = await getDeliveryJobLog(job.id, nextLine.current);
        if (!active) return;
        nextLine.current = chunk.next_line;
        setLog((previous) => ({
          entries: [...(previous?.entries ?? []), ...chunk.entries],
          next_line: chunk.next_line,
          stderr_tail: chunk.stderr_tail,
        }));
      } catch {
        // The log appears once the job has started.
      }
    };
    void load();
    const timer = live ? setInterval(() => void load(), 1500) : null;
    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  }, [job.id, live]);
  if (!log || (log.entries.length === 0 && !log.stderr_tail.trim())) return null;
  return (
    <details className="delivery-log" open={live}>
      <summary>Registro</summary>
      <ol>
        {log.entries.map((entry, index) => (
          <li key={index} className={`delivery-log__entry delivery-log__entry--${entry.kind}`}>
            {entry.text}
          </li>
        ))}
      </ol>
      {log.stderr_tail.trim() && !live && job.status !== "finished" && (
        <pre className="delivery-log__stderr">{log.stderr_tail.trim()}</pre>
      )}
    </details>
  );
}
