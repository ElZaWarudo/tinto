// Delivery mode: tasks in their own worktree and branch, advanced by
// background agent stages. A separate way to work from Agents; the two only
// meet through "Abrir en Agents".

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { busStore, useBusState } from "../../bus/store";
import { useWorkspaceActions } from "../../workspace/actions";
import { isWslRepoSource } from "../repoSource";
import { confirm } from "../../workbench/confirmDialog";
import { lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import {
  cancelDeliveryJob,
  closeDeliveryRun,
  completeDeliveryApproval,
  createDeliveryRun,
  createDeliveryTask,
  decideDeliveryApproval,
  dispatchDeliveryJob,
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
  undoDeliveryJob,
  updateDeliverySettings,
  updateDeliveryTask,
} from "../../delivery/client";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryOverview,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryRung,
  DeliveryTask,
} from "../../delivery/types";
import {
  DispatchDialog,
  NewTaskDialog,
  ReleaseLeaseDialog,
  SettingsDialog,
  type RepoChoice,
} from "./DeliveryDialogs";
import { DecisionLog, PendingDecisions } from "./DeliveryDecisions";
import {
  GROUPS,
  RUNGS,
  TASK_STATE_LABEL,
  attemptOutcome,
  batchName,
  isBroken,
  isOpen,
  jobsOf,
  lastHandoff,
  latestApproval,
  latestOfRole,
  lower,
  nextRole,
  nextRung,
  rolesOf,
  rungLabel,
  stageLabel,
  taskStatus,
  type StatusIcon,
  type TaskStatus,
  type Tone,
} from "./taskStatus";
import "./delivery.css";

type Dialog =
  | { kind: "new-task" }
  | { kind: "dispatch"; task: DeliveryTask; role: string; prompt: string; source: string | null }
  | { kind: "release"; name: string; note: string | null }
  | { kind: "settings" }
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

function changeSymbol(kind: string): string {
  if (kind === "created") return "+";
  if (kind === "removed") return "×";
  return "−";
}

function files(count: number): string {
  return count === 1 ? "1 archivo" : `${count} archivos`;
}

function agentName(agent: string): string {
  return agent === "codex" ? "Codex" : agent === "claude" ? "Claude Code" : "Shell";
}

export function DeliveryPanel() {
  const bus = useBusState();
  const { openAgentTerminal } = useWorkspaceActions();
  const [overview, setOverview] = useState<DeliveryOverview | null>(null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [busy, setBusy] = useState(false);
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
    void onDeliveryChanged(() => {
      if (pending) return;
      pending = setTimeout(() => {
        pending = null;
        if (active) void refresh();
      }, 150);
    }).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    });
    const clock = setInterval(() => setNow(Date.now()), 1000);
    const fallback = setInterval(() => void refresh(), 5000);
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
  const capacity = overview?.settings.capacity ?? 3;
  const qa = overview?.leases.find((lease) => lease.name === "qa");
  const qaHolder = tasks.find((task) => task.id === qa?.holder_task_id);
  const activeRuns = (overview?.runs ?? []).filter((candidate) => candidate.status === "active");

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await refresh();
      return true;
    } catch (cause) {
      setError(errorText(cause));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const takeover = async (target: DeliveryRun) => {
    const ok = await confirm(
      `${target.owner} dejará de coordinar "${target.title}". Lo que envíe a partir de ahora se descarta.`,
      { title: "Tomar el control", kind: "warning", okLabel: "Tomar el control" },
    );
    if (ok) await run(() => takeoverDeliveryRun(target.id));
  };

  return (
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
            {activeRuns[0].owner && activeRuns[0].owner !== "user" && (
              <button type="button" disabled={busy} onClick={() => void takeover(activeRuns[0])}>
                Tomar el control
              </button>
            )}
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
            {needYou.length === 1 ? "1 tarea te necesita" : `${needYou.length} tareas te necesitan`}
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
        {qa && (
          <span
            className={`delivery-lease delivery-lease--${qa.state}`}
            title={qa.note ?? undefined}
          >
            {qa.state === "quarantined"
              ? "QA en cuarentena"
              : qa.state === "active"
                ? `QA: ${qaHolder?.key ?? "ocupado"}`
                : "QA libre"}
            {qa.queue.length > 0 && (
              <span className="delivery-muted">· {qa.queue.length} esperando</span>
            )}
          </span>
        )}
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

      {qa?.state === "quarantined" && (
        <div className="delivery-panel__warning" role="status">
          <span>
            El recurso QA está en cuarentena{qaHolder ? ` tras ${qaHolder.key}` : ""}. {qa.note}
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
              busy={busy}
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
                await run(async () => {
                  const conversation = await openDeliveryJobInAgents(job.id);
                  openAgentTerminal({
                    sessionId: conversation.session_id,
                    repo: conversation.repo,
                    agentType: conversation.agent_type,
                  });
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
          busy={busy}
          onCancel={() => setDialog(null)}
          onSubmit={(values) =>
            void run(async () => {
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
            })
          }
        />
      )}
      {dialog?.kind === "dispatch" && (
        <DispatchDialog
          task={dialog.task}
          settings={repoSettings}
          initialRole={dialog.role}
          initialPrompt={dialog.prompt}
          promptSource={dialog.source}
          busy={busy}
          onCancel={() => setDialog(null)}
          onSubmit={(values) =>
            void (async () => {
              if (values.access === "full") {
                const ok = await confirm(
                  `El agente podrá ejecutar comandos y modificar archivos fuera del worktree sin pedir aprobación.\n\nWorktree: ${dialog.task.worktree}`,
                  {
                    title: "Dar acceso completo",
                    kind: "warning",
                    okLabel: "Dar acceso completo",
                  },
                );
                if (!ok) return;
              }
              const done = await run(async () => {
                if (values.settings) {
                  const current = await getDeliveryRepoSettings(dialog.task.repo);
                  await setDeliveryRepoSettings(dialog.task.repo, {
                    ...current,
                    ...values.settings,
                  });
                }
                return dispatchDeliveryJob({
                  taskId: dialog.task.id,
                  role: values.role,
                  agent: values.agent,
                  model: values.model,
                  access: values.access,
                  prompt: values.prompt,
                });
              });
              if (done) setDialog(null);
            })()
          }
        />
      )}
      {dialog?.kind === "release" && (
        <ReleaseLeaseDialog
          name={dialog.name}
          note={dialog.note}
          busy={busy}
          onCancel={() => setDialog(null)}
          onSubmit={(confirmation) =>
            void run(async () => {
              await releaseDeliveryLease(dialog.name, confirmation);
              setDialog(null);
            })
          }
        />
      )}
      {dialog?.kind === "settings" && (
        <SettingsDialog
          capacity={capacity}
          endpoint={overview?.coordinator ?? null}
          runs={overview?.runs ?? []}
          repos={repos}
          busy={busy}
          onCancel={() => setDialog(null)}
          onCapacity={(value) => void run(() => updateDeliverySettings(value))}
          onCreateRun={(repo, title, qaJiraComment) =>
            void run(() => createDeliveryRun(repo, title, qaJiraComment))
          }
          onQaJiraComment={(target, post) =>
            void run(() => setDeliveryRunQaJiraComment(target.id, post))
          }
          onTakeover={(target) => void takeover(target)}
          onClose={(target) =>
            void (async () => {
              const ok = await confirm(
                `Se cerrará el lote "${target.title}". Sus tareas siguen disponibles.`,
                { title: "Cerrar lote", okLabel: "Cerrar" },
              );
              if (ok) await run(() => closeDeliveryRun(target.id));
            })()
          }
        />
      )}
    </div>
  );
}

function TaskDetail({
  task,
  overview,
  now,
  busy,
  run,
  onDispatch,
  onOpenConversation,
  onContinueJob,
  onRemoved,
}: {
  task: DeliveryTask;
  overview: DeliveryOverview;
  now: number;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onDispatch: (role: string, prompt: string, source: string | null) => void;
  onOpenConversation: (agentType: string) => Promise<void>;
  onContinueJob: (job: DeliveryJob) => Promise<void>;
  onRemoved: () => void;
}) {
  const jobs = jobsOf(overview, task.id);
  const newestFirst = [...jobs].reverse();
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const taskRun = overview.runs.find((candidate) => candidate.id === task.run_id);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(newestFirst[0]?.id ?? null);
  const latestUndoable = newestFirst.find(
    (job) => job.agent !== "shell" && !job.undone_at_ms && job.changes.length > 0,
  );
  const hasOpenJob = jobs.some(isOpen);
  const next = nextRole(jobs);
  const handoff = lastHandoff(jobs);
  const latest = newestFirst.find(
    (job) => job.agent !== "shell" && job.result_state !== "stale" && !job.undone_at_ms,
  );
  const broken = latest && !hasOpenJob && isBroken(latest) ? latest : null;
  const openApprovals = approvals.filter(
    (approval) => approval.status === "pending" || approval.status === "approved",
  );
  const rung = openApprovals.length === 0 ? nextRung(approvals) : null;
  const [requesting, setRequesting] = useState(false);
  const taskDecisions = (overview.decisions ?? []).filter(
    (decision) => decision.task_id === task.id,
  );
  const pendingDecisions = taskDecisions.filter((decision) => decision.status === "pending");
  const blocked = pendingDecisions.length > 0;
  const needsYou = blocked || openApprovals.length > 0 || broken !== null;
  const stateLabel = TASK_STATE_LABEL[task.state] ?? task.state;

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
    if (ok) await run(() => undoDeliveryJob(job.id));
  };

  const remove = async () => {
    const ok = await confirm(
      `Se eliminará el worktree ${task.worktree}. La rama ${task.branch} se conserva.`,
      { title: "Eliminar tarea", kind: "warning", okLabel: "Eliminar" },
    );
    if (!ok) return;
    const removed = await run(async () => {
      try {
        await removeDeliveryTask(task.id, false);
      } catch (cause) {
        const category =
          cause && typeof cause === "object" && "category" in cause
            ? String((cause as { category: unknown }).category)
            : "";
        if (category !== "task_unsaved") throw cause;
        const force = await confirm(
          `${errorText(cause)}. Si eliminas el worktree, esos cambios se pierden; los commits siguen en la rama ${task.branch}.`,
          { title: "Hay trabajo sin guardar", kind: "warning", okLabel: "Eliminar igualmente" },
        );
        if (!force) return;
        await removeDeliveryTask(task.id, true);
      }
    });
    if (removed) onRemoved();
  };

  const bumpContract = async () => {
    const version = task.contract_version + 1;
    const ok = await confirm(
      `La tarea pasa a la versión ${version} del contrato. Los resultados que están en curso quedarán obsoletos y no harán avanzar la tarea.`,
      { title: "Nueva versión del contrato", kind: "warning", okLabel: `Subir a v${version}` },
    );
    if (ok) await run(() => updateDeliveryTask(task.id, { contractVersion: version }));
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
          </h2>
          <p className="delivery-task__meta">
            <span>
              <code title={task.worktree}>{task.branch}</code> desde {task.base_ref} (
              {shortCommit(task.base_commit)})
            </span>
            {taskRun && <span>{batchName(taskRun.title || taskRun.id)}</span>}
            <span title="Lo fija quien coordina la tarea">Estado: {stateLabel}</span>
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
                disabled={busy}
                onClick={() => {
                  close();
                  void bumpContract();
                }}
              >
                Nueva versión del contrato… <small>v{task.contract_version}</small>
              </button>
              <hr />
              <button
                type="button"
                role="menuitem"
                className="delivery-menu__danger"
                disabled={busy || hasOpenJob}
                title={hasOpenJob ? "Cancela o espera a que terminen sus etapas." : undefined}
                onClick={() => {
                  close();
                  void remove();
                }}
              >
                Eliminar tarea…
              </button>
            </>
          )}
        </Menu>
      </header>

      {blocked && (
        <PendingDecisions taskId={task.id} decisions={pendingDecisions} busy={busy} run={run} />
      )}

      {openApprovals.map((approval) => (
        <ApprovalBanner key={approval.id} task={task} approval={approval} busy={busy} run={run} />
      ))}

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
          <div className="delivery-actions">
            <button
              type="button"
              className="delivery-button--primary"
              disabled={busy}
              onClick={() => void run(() => retryDeliveryJob(broken.id))}
            >
              Reintentar
            </button>
            {broken.agent === "codex" && broken.provider_session_id && (
              <button type="button" disabled={busy} onClick={() => void onContinueJob(broken)}>
                Continuar en Agents
              </button>
            )}
            {latestUndoable?.id === broken.id && (
              <button type="button" disabled={busy} onClick={() => void undo(broken)}>
                Deshacer cambios
              </button>
            )}
          </div>
        </div>
      )}

      <StageTrack jobs={jobs} approvals={approvals} next={next} onFocus={focusStage} />

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
          disabled={busy}
          trigger={
            <>
              Abrir en Agents <Icon name="down" />
            </>
          }
        >
          {(close) =>
            (["codex", "claude"] as const).map((agent) => (
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
        {rung && !requesting && (
          <button type="button" onClick={() => setRequesting(true)}>
            Preparar {lower(rungLabel(rung))}…
          </button>
        )}
        {blocked && (
          <span className="delivery-muted">
            Contesta las decisiones pendientes para lanzar etapas.
          </span>
        )}
        {!next && !hasOpenJob && !needsYou && (
          <span className="delivery-muted">
            {rung ? "Todas las etapas pasaron. Lo siguiente es la entrega." : "Tarea entregada."}
          </span>
        )}
      </div>

      {rung && requesting && (
        <DeliveryRequest
          key={rung}
          task={task}
          rung={rung}
          busy={busy}
          run={run}
          onClose={() => setRequesting(false)}
        />
      )}

      <DecisionLog decisions={taskDecisions.filter((decision) => decision.status === "answered")} />

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
                renderJob={(job) => (
                  <JobRow
                    key={job.id}
                    job={job}
                    now={now}
                    busy={busy}
                    expanded={expandedJobId === job.id}
                    undoable={latestUndoable?.id === job.id && !hasOpenJob}
                    onToggle={() => setExpandedJobId(expandedJobId === job.id ? null : job.id)}
                    run={run}
                    onUndo={() => void undo(job)}
                    onContinue={() => void onContinueJob(job)}
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

function StageTrack({
  jobs,
  approvals,
  next,
  onFocus,
}: {
  jobs: DeliveryJob[];
  approvals: DeliveryApproval[];
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
              ? attemptOutcome(job)
              : { tone: "todo" as const, label: role === next ? "Siguiente" : "—" };
            const tone = outcome.tone;
            const content = (
              <>
                <span className="delivery-node__name">
                  <StatusMark icon={tone === "todo" ? "ring" : TONE_ICON[tone]} />
                  {stageLabel(role)}
                </span>
                <small>{outcome.label}</small>
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
                    title={`${stageLabel(role)} · intento ${job.attempt}`}
                    onClick={() => onFocus(job)}
                  >
                    {content}
                  </button>
                ) : (
                  <div>{content}</div>
                )}
              </li>
            );
          })}
        </ol>
      </div>
      <div className="delivery-track__part delivery-track__part--rungs">
        <h3 id="delivery-track-rungs">Entrega</h3>
        <ol aria-labelledby="delivery-track-rungs">
          {RUNGS.map(({ rung, label }) => {
            const step = rungStep(latestApproval(approvals, rung));
            return (
              <li key={rung} className={`delivery-node delivery-node--${step.tone}`}>
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
      return { tone: "attention", icon: "flag", label: "Tu aprobación" };
    case "approved":
      return { tone: "attention", icon: "flag", label: "Falta registrarlo" };
    case "executed":
      return {
        tone: "ok",
        icon: "check",
        label: approval.outcome || "Hecho",
        title: approval.outcome ?? undefined,
      };
    case "rejected":
      return { tone: "danger", icon: "x", label: "Rechazado" };
    default:
      return { tone: "danger", icon: "x", label: "Falló", title: approval.outcome ?? undefined };
  }
}

function ApprovalBanner({
  task,
  approval,
  busy,
  run,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
}) {
  const [outcome, setOutcome] = useState("");
  const name = lower(rungLabel(approval.rung));
  const pending = approval.status === "pending";
  const consequence =
    approval.rung === "commit"
      ? `Al aprobar, Tinto hace el commit en ${task.branch} con este texto exacto. No publica nada: el push se aprueba aparte.`
      : approval.rung === "push"
        ? `Al aprobar, Tinto publica la rama ${task.branch}.`
        : approval.rung === "pr"
          ? "Al aprobar, el coordinador abre el PR con este título y cuerpo. Después registras aquí su URL."
          : "Al aprobar, el coordinador hace estas operaciones en Jira. Después registras aquí qué se hizo.";
  return (
    <div className="delivery-banner" role="group" aria-label={`Aprobación ${approval.rung}`}>
      <p className="delivery-banner__title">
        <Icon name="flag" />
        <strong>{pending ? `Aprobar ${name}` : `Registrar ${name}`}</strong>
        <span className="delivery-muted">
          {pending
            ? `pedido por ${approval.requested_by === "user" ? "ti" : approval.requested_by}`
            : "aprobado; falta anotar el resultado"}
        </span>
      </p>
      <pre className="delivery-banner__text">
        {approval.title}
        {approval.body ? `\n\n${approval.body}` : ""}
      </pre>
      {pending && <p className="delivery-muted">{consequence}</p>}
      {pending ? (
        <div className="delivery-actions">
          <button
            type="button"
            className="delivery-button--primary"
            disabled={busy}
            onClick={() => void run(() => decideDeliveryApproval(approval.id, true))}
          >
            {approval.rung === "commit"
              ? "Aprobar y hacer commit"
              : approval.rung === "push"
                ? "Aprobar y publicar"
                : "Aprobar"}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void run(() => decideDeliveryApproval(approval.id, false))}
          >
            Rechazar
          </button>
        </div>
      ) : (
        <div className="delivery-actions">
          <input
            aria-label="Resultado del paso"
            placeholder={approval.rung === "pr" ? "URL del PR" : "Qué se hizo"}
            value={outcome}
            onChange={(event) => setOutcome(event.target.value)}
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => void run(() => completeDeliveryApproval(approval.id, true, outcome))}
          >
            Marcar como hecho
          </button>
        </div>
      )}
    </div>
  );
}

function DeliveryRequest({
  task,
  rung,
  busy,
  run,
  onClose,
}: {
  task: DeliveryTask;
  rung: DeliveryRung;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onClose: () => void;
}) {
  const named = task.title ? `${task.key}: ${task.title}` : task.key;
  const [title, setTitle] = useState(rung === "push" ? `Publicar ${task.branch}` : named);
  const [body, setBody] = useState("");
  const name = lower(rungLabel(rung));
  return (
    <form
      className="delivery-request"
      aria-label={`Pedir aprobación de ${name}`}
      onSubmit={(event) => {
        event.preventDefault();
        void run(() => requestDeliveryApproval(task.id, rung, title, body)).then((ok) => {
          if (ok) onClose();
        });
      }}
    >
      <label className="delivery-request__field">
        <span>{rung === "commit" ? "Mensaje del commit" : "Texto exacto a aprobar"}</span>
        <input value={title} onChange={(event) => setTitle(event.target.value)} required />
      </label>
      <label className="delivery-request__field">
        <span>{rung === "jira" ? "Operaciones exactas en Jira" : "Detalle (opcional)"}</span>
        <textarea value={body} onChange={(event) => setBody(event.target.value)} rows={3} />
      </label>
      <div className="delivery-actions">
        <button type="submit" className="delivery-button--primary" disabled={busy}>
          Pedir aprobación
        </button>
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
  renderJob,
}: {
  id: string;
  current: DeliveryJob;
  older: DeliveryJob[];
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
              · {older.map((job) => lower(attemptOutcome(job).label)).join(", ")}
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
  now,
  busy,
  expanded,
  undoable,
  onToggle,
  run,
  onUndo,
  onContinue,
}: {
  job: DeliveryJob;
  now: number;
  busy: boolean;
  expanded: boolean;
  undoable: boolean;
  onToggle: () => void;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onUndo: () => void;
  onContinue: () => void;
}) {
  const outcome = attemptOutcome(job);
  const scope =
    (job.access === "full" ? " · acceso completo" : job.writes ? "" : " · solo lectura") +
    (job.qa_browser ? " · navegador" : "");
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
          {job.model ? ` · ${job.model}` : ""}
          {scope}
        </span>
        <span className={`delivery-outcome delivery-tone--${outcome.tone}`}>
          <StatusMark icon={TONE_ICON[outcome.tone]} />
          {outcome.label}
        </span>
        <span className="delivery-job__changes">
          {job.changes.length > 0 ? files(job.changes.length) : ""}
        </span>
        <span className="delivery-job__time">{duration(job, now)}</span>
      </button>
      {expanded && (
        <div className="delivery-job__detail">
          {job.error && <p className="delivery-job__error">{job.error}</p>}
          {job.result_note && <p className="delivery-job__note">{job.result_note}</p>}
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
          <div className="delivery-actions">
            {isOpen(job) && (
              <button
                type="button"
                disabled={busy}
                onClick={() => void run(() => cancelDeliveryJob(job.id))}
              >
                Cancelar
              </button>
            )}
            {!isOpen(job) && job.agent !== "shell" && (
              <button
                type="button"
                disabled={busy}
                onClick={() => void run(() => retryDeliveryJob(job.id))}
              >
                Nuevo intento
              </button>
            )}
            {undoable && (
              <button type="button" disabled={busy} onClick={onUndo}>
                Deshacer estos cambios
              </button>
            )}
            {!isOpen(job) && job.agent === "codex" && job.provider_session_id && (
              <button type="button" disabled={busy} onClick={onContinue}>
                Continuar en Agents
              </button>
            )}
          </div>
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
