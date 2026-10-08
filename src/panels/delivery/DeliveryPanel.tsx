// Delivery mode: tasks in their own worktree and branch, advanced by
// background agent jobs. A separate way to work from Agents; the two only
// meet through "Abrir en Agents".

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  DeliveryRung,
  DeliveryTask,
} from "../../delivery/types";
import {
  CoordinatorDialog,
  DispatchDialog,
  NewTaskDialog,
  ReleaseLeaseDialog,
  type RepoChoice,
} from "./DeliveryDialogs";
import "./delivery.css";

const TASK_STATES = [
  "intake",
  "contract_ready",
  "tests",
  "implementation",
  "review",
  "qa_ready",
  "qa_active",
  "changes_requested",
  "review_ready",
  "blocked",
];

const RUNGS: { rung: DeliveryRung; label: string }[] = [
  { rung: "commit", label: "Commit" },
  { rung: "push", label: "Push" },
  { rung: "pr", label: "PR" },
  { rung: "jira", label: "Jira" },
];

const JOB_STATUS_LABEL: Record<string, string> = {
  queued: "en cola",
  pending: "iniciando",
  started: "en marcha",
  finished: "terminado",
  failed: "falló",
  cancelled: "cancelado",
  interrupted: "interrumpido",
};

const RESULT_STATE_LABEL: Record<string, string> = {
  accepted: "aceptado",
  stale: "obsoleto",
  invalid: "inválido",
};

const APPROVAL_STATUS_LABEL: Record<string, string> = {
  pending: "pendiente",
  approved: "aprobado",
  rejected: "rechazado",
  executed: "hecho",
  failed: "falló",
};

const ROLE_LABEL: Record<string, string> = {
  tests: "Tests",
  implementation: "Implementación",
  review: "Revisión",
  qa: "QA",
  bootstrap: "Preparación",
};

type Dialog =
  | { kind: "new-task" }
  | { kind: "dispatch"; task: DeliveryTask }
  | { kind: "release"; name: string; note: string | null }
  | { kind: "coordinator" }
  | null;

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

function isOpen(job: DeliveryJob): boolean {
  return job.status === "queued" || job.status === "pending" || job.status === "started";
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
  const [repoChecks, setRepoChecks] = useState<string[] | null>(null);

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
  const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? tasks[0] ?? null;
  const jobs = overview?.jobs ?? [];
  const running = jobs.filter((job) => job.status === "pending" || job.status === "started");
  const queued = jobs.filter((job) => job.status === "queued");
  const pendingApprovals = (overview?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );
  const qa = overview?.leases.find((lease) => lease.name === "qa");

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

  const tasksByRepo = useMemo(() => {
    const groups = new Map<string, DeliveryTask[]>();
    for (const task of tasks) {
      const list = groups.get(task.repo) ?? [];
      list.push(task);
      groups.set(task.repo, list);
    }
    return [...groups.entries()];
  }, [tasks]);

  return (
    <div className="delivery-panel" data-testid="delivery-panel">
      <header className="delivery-panel__header">
        <h1>Delivery</h1>
        <label className="delivery-panel__capacity">
          Capacidad
          <select
            aria-label="Capacidad de trabajos en paralelo"
            value={overview?.settings.capacity ?? 3}
            onChange={(event) => void run(() => updateDeliverySettings(Number(event.target.value)))}
          >
            {[1, 2, 3, 4, 5, 6, 7, 8].map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </label>
        <span className="delivery-chip">{running.length} en marcha</span>
        <span className="delivery-chip">{queued.length} en cola</span>
        <span
          className={`delivery-chip delivery-chip--${qa?.state ?? "free"}`}
          title={qa?.note ?? undefined}
        >
          QA{" "}
          {qa?.state === "active"
            ? "ocupado"
            : qa?.state === "quarantined"
              ? "en cuarentena"
              : "libre"}
          {qa && qa.queue.length > 0 ? ` · ${qa.queue.length} en cola` : ""}
        </span>
        {pendingApprovals.length > 0 && (
          <span className="delivery-chip delivery-chip--attention">
            {pendingApprovals.length} aprobaciones pendientes
          </span>
        )}
        <span className="delivery-panel__spacer" />
        <button type="button" onClick={() => setDialog({ kind: "coordinator" })}>
          Coordinador
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
          <span>El recurso QA está en cuarentena. {qa.note}</span>
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
          {tasksByRepo.map(([repo, list]) => (
            <section key={repo} className="delivery-task-group">
              <h2 title={repo}>{busStore.displayName(repo)}</h2>
              {list.map((task) => {
                const taskJobs = jobs.filter((job) => job.task_id === task.id);
                const active = taskJobs.some(
                  (job) => job.status === "pending" || job.status === "started",
                );
                const waiting = taskJobs.some((job) => job.status === "queued");
                return (
                  <button
                    key={task.id}
                    type="button"
                    className="delivery-task-row"
                    aria-current={selectedTask?.id === task.id ? "true" : undefined}
                    onClick={() => setSelectedTaskId(task.id)}
                  >
                    <span className="delivery-task-row__key">
                      {active && <span className="delivery-dot delivery-dot--live" aria-hidden />}
                      {!active && waiting && <span className="delivery-dot" aria-hidden />}
                      {task.key}
                    </span>
                    <span className="delivery-task-row__title">{task.title || task.branch}</span>
                    <span className="delivery-task-row__state">{task.state}</span>
                  </button>
                );
              })}
            </section>
          ))}
        </nav>

        <main className="delivery-panel__detail">
          {selectedTask ? (
            <TaskDetail
              key={selectedTask.id}
              task={selectedTask}
              overview={overview!}
              now={now}
              busy={busy}
              run={run}
              onDispatch={() => {
                setRepoChecks(null);
                getDeliveryRepoSettings(selectedTask.repo).then(
                  (settings) => setRepoChecks(settings.checks ?? []),
                  () => setRepoChecks([]),
                );
                setDialog({ kind: "dispatch", task: selectedTask });
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
          checks={repoChecks}
          busy={busy}
          onCancel={() => setDialog(null)}
          onSubmit={(values) =>
            void (async () => {
              if (values.access === "full") {
                const ok = await confirm(
                  `El trabajo podrá ejecutar comandos y modificar archivos fuera del worktree sin pedir aprobación.\n\nWorktree: ${dialog.task.worktree}`,
                  {
                    title: "Dar acceso completo",
                    kind: "warning",
                    okLabel: "Dar acceso completo",
                  },
                );
                if (!ok) return;
              }
              const done = await run(async () => {
                if (values.checks) {
                  const current = await getDeliveryRepoSettings(dialog.task.repo);
                  await setDeliveryRepoSettings(dialog.task.repo, {
                    ...current,
                    checks: values.checks,
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
      {dialog?.kind === "coordinator" && (
        <CoordinatorDialog
          endpoint={overview?.coordinator ?? null}
          runs={overview?.runs ?? []}
          repos={repos}
          busy={busy}
          onCancel={() => setDialog(null)}
          onCreateRun={(repo, title) => void run(() => createDeliveryRun(repo, title))}
          onTakeover={(target) =>
            void (async () => {
              const ok = await confirm(
                `${target.owner} dejará de coordinar "${target.title}". Lo que envíe a partir de ahora se descarta.`,
                { title: "Tomar el control", kind: "warning", okLabel: "Tomar el control" },
              );
              if (ok) await run(() => takeoverDeliveryRun(target.id));
            })()
          }
          onClose={(target) =>
            void (async () => {
              const ok = await confirm(
                `Se cerrará "${target.title}". Sus tareas siguen disponibles.`,
                { title: "Cerrar ejecución", okLabel: "Cerrar" },
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
  onDispatch: () => void;
  onOpenConversation: (agentType: string) => Promise<void>;
  onContinueJob: (job: DeliveryJob) => Promise<void>;
  onRemoved: () => void;
}) {
  const jobs = overview.jobs.filter((job) => job.task_id === task.id).reverse();
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const taskRun = overview.runs.find((candidate) => candidate.id === task.run_id);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(jobs[0]?.id ?? null);
  const [conversationAgent, setConversationAgent] = useState("codex");
  const latestUndoable = jobs.find(
    (job) => job.agent !== "shell" && !job.undone_at_ms && job.changes.length > 0,
  );
  const hasOpenJob = jobs.some(isOpen);

  return (
    <article className="delivery-task" aria-label={`Tarea ${task.key}`}>
      <header className="delivery-task__header">
        <div>
          <h2>
            {task.key}
            {task.title && <span> — {task.title}</span>}
          </h2>
          <p className="delivery-task__meta">
            <code>{task.branch}</code> desde {task.base_ref} ({shortCommit(task.base_commit)}) ·{" "}
            <span title={task.worktree}>{task.worktree}</span>
            {taskRun && (
              <>
                {" "}
                · ejecución {taskRun.title || taskRun.id}
                {taskRun.owner ? ` (coordina ${taskRun.owner})` : ""}
              </>
            )}
          </p>
        </div>
        <div className="delivery-task__controls">
          <label>
            Estado
            <select
              value={task.state}
              disabled={busy}
              onChange={(event) =>
                void run(() => updateDeliveryTask(task.id, { state: event.target.value }))
              }
            >
              {[...new Set([task.state, ...TASK_STATES])].map((state) => (
                <option key={state} value={state}>
                  {state}
                </option>
              ))}
            </select>
          </label>
          <span
            className="delivery-chip"
            title="Subir la versión del contrato vuelve obsoletos los resultados en curso."
          >
            contrato v{task.contract_version}
            <button
              type="button"
              aria-label="Nueva versión del contrato"
              disabled={busy}
              onClick={() =>
                void run(() =>
                  updateDeliveryTask(task.id, { contractVersion: task.contract_version + 1 }),
                )
              }
            >
              +
            </button>
          </span>
        </div>
      </header>

      <div className="delivery-task__actions">
        <button type="button" className="delivery-button--primary" onClick={onDispatch}>
          Nuevo trabajo
        </button>
        <span className="delivery-split-button">
          <button
            type="button"
            disabled={busy}
            onClick={() => void onOpenConversation(conversationAgent)}
          >
            Abrir en Agents
          </button>
          <select
            aria-label="Agente para la conversación"
            value={conversationAgent}
            onChange={(event) => setConversationAgent(event.target.value)}
          >
            <option value="codex">Codex</option>
            <option value="claude">Claude Code</option>
          </select>
        </span>
        <button
          type="button"
          disabled={busy || hasOpenJob}
          title={hasOpenJob ? "Cancela o espera a que terminen sus trabajos." : undefined}
          onClick={() =>
            void (async () => {
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
                    {
                      title: "Hay trabajo sin guardar",
                      kind: "warning",
                      okLabel: "Eliminar igualmente",
                    },
                  );
                  if (!force) return;
                  await removeDeliveryTask(task.id, true);
                }
              });
              if (removed) onRemoved();
            })()
          }
        >
          Eliminar
        </button>
      </div>

      <section className="delivery-section" aria-label="Trabajos">
        <h3>Trabajos</h3>
        {jobs.length === 0 && <p className="delivery-panel__empty">Todavía no hay trabajos.</p>}
        <ul className="delivery-jobs">
          {jobs.map((job) => (
            <JobRow
              key={job.id}
              job={job}
              now={now}
              busy={busy}
              expanded={expandedJobId === job.id}
              undoable={latestUndoable?.id === job.id && !hasOpenJob}
              onToggle={() => setExpandedJobId(expandedJobId === job.id ? null : job.id)}
              run={run}
              onContinue={() => void onContinueJob(job)}
            />
          ))}
        </ul>
      </section>

      <DeliveryLadder task={task} approvals={approvals} busy={busy} run={run} />
    </article>
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
  onContinue,
}: {
  job: DeliveryJob;
  now: number;
  busy: boolean;
  expanded: boolean;
  undoable: boolean;
  onToggle: () => void;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onContinue: () => void;
}) {
  const role = ROLE_LABEL[job.role] ?? job.role;
  const agent = job.agent === "codex" ? "Codex" : job.agent === "claude" ? "Claude Code" : "Shell";
  return (
    <li className={`delivery-job delivery-job--${job.status}`}>
      <button
        type="button"
        className="delivery-job__summary"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <span className="delivery-job__role">
          {role} <small>#{job.attempt}</small>
        </span>
        <span className="delivery-job__agent">
          {agent}
          {job.model ? ` · ${job.model}` : ""}
          {job.access === "full" ? " · acceso completo" : ""}
        </span>
        <span className={`delivery-status delivery-status--${job.status}`}>
          {JOB_STATUS_LABEL[job.status] ?? job.status}
        </span>
        {job.result && (
          <span className={`delivery-status delivery-status--result-${job.result.status}`}>
            {job.result.status}
          </span>
        )}
        {job.result_state && job.result_state !== "accepted" && (
          <span className={`delivery-status delivery-status--${job.result_state}`}>
            {RESULT_STATE_LABEL[job.result_state]}
          </span>
        )}
        {job.changes.length > 0 && (
          <span className="delivery-job__changes">
            {job.changes.length} {job.changes.length === 1 ? "archivo" : "archivos"}
            {job.undone_at_ms ? " · deshecho" : ""}
          </span>
        )}
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
          )}
          <details className="delivery-job__prompt">
            <summary>Encargo</summary>
            <pre>{job.prompt}</pre>
          </details>
          <JobLog key={job.id} job={job} />
          <div className="delivery-job__actions">
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
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void (async () => {
                    const ok = await confirm(
                      `Se restaurarán ${job.changes.length} archivos al estado en que estaban cuando empezó este trabajo.`,
                      { title: "Deshacer trabajo", kind: "warning", okLabel: "Deshacer" },
                    );
                    if (ok) await run(() => undoDeliveryJob(job.id));
                  })()
                }
              >
                Deshacer
              </button>
            )}
            {!isOpen(job) && job.agent === "codex" && job.provider_session_id && (
              <button type="button" disabled={busy} onClick={onContinue}>
                Continuar en Agents
              </button>
            )}
          </div>
          {(job.start_candidate || job.end_candidate) && (
            <p className="delivery-job__candidates">
              Candidato {shortCommit(job.start_candidate?.split(":")[1])}
              {job.end_candidate ? ` → ${shortCommit(job.end_candidate.split(":")[1])}` : ""}
            </p>
          )}
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

function DeliveryLadder({
  task,
  approvals,
  busy,
  run,
}: {
  task: DeliveryTask;
  approvals: DeliveryApproval[];
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
}) {
  const latest = (rung: DeliveryRung) =>
    [...approvals].reverse().find((approval) => approval.rung === rung);
  const nextRung = RUNGS.find(({ rung }) => latest(rung)?.status !== "executed")?.rung ?? "jira";
  const [rung, setRung] = useState<DeliveryRung>(nextRung);
  const defaultTitle = task.title ? `${task.key}: ${task.title}` : task.key;
  const [title, setTitle] = useState(defaultTitle);
  const [body, setBody] = useState("");
  const [outcomes, setOutcomes] = useState<Record<string, string>>({});
  return (
    <section className="delivery-section delivery-ladder" aria-label="Entrega">
      <h3>Entrega</h3>
      <ol className="delivery-ladder__rungs">
        {RUNGS.map(({ rung: step, label }) => {
          const approval = latest(step);
          return (
            <li
              key={step}
              className={`delivery-ladder__rung delivery-ladder__rung--${approval?.status ?? "none"}`}
            >
              <strong>{label}</strong>
              <span>{approval ? APPROVAL_STATUS_LABEL[approval.status] : "—"}</span>
              {approval?.outcome && <small title={approval.outcome}>{approval.outcome}</small>}
            </li>
          );
        })}
      </ol>
      {approvals
        .filter((approval) => approval.status === "pending" || approval.status === "approved")
        .map((approval) => (
          <div
            key={approval.id}
            className="delivery-approval"
            role="group"
            aria-label={`Aprobación ${approval.rung}`}
          >
            <p>
              <strong>{RUNGS.find((item) => item.rung === approval.rung)?.label}</strong> pedido por{" "}
              {approval.requested_by === "user" ? "ti" : approval.requested_by}
            </p>
            <pre className="delivery-approval__text">
              {approval.title}
              {approval.body ? `\n\n${approval.body}` : ""}
            </pre>
            {approval.status === "pending" ? (
              <div className="delivery-approval__actions">
                <button
                  type="button"
                  className="delivery-button--primary"
                  disabled={busy}
                  onClick={() => void run(() => decideDeliveryApproval(approval.id, true))}
                >
                  {approval.rung === "commit"
                    ? "Aprobar y confirmar"
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
              <div className="delivery-approval__actions">
                <input
                  aria-label="Resultado del paso"
                  placeholder={approval.rung === "pr" ? "URL del PR" : "Qué se hizo"}
                  value={outcomes[approval.id] ?? ""}
                  onChange={(event) =>
                    setOutcomes({ ...outcomes, [approval.id]: event.target.value })
                  }
                />
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      completeDeliveryApproval(approval.id, true, outcomes[approval.id] ?? ""),
                    )
                  }
                >
                  Marcar como hecho
                </button>
              </div>
            )}
          </div>
        ))}
      <form
        className="delivery-ladder__request"
        onSubmit={(event) => {
          event.preventDefault();
          void run(async () => {
            await requestDeliveryApproval(task.id, rung, title, body);
            setBody("");
          });
        }}
      >
        <select
          aria-label="Paso de entrega"
          value={rung}
          onChange={(event) => setRung(event.target.value as DeliveryRung)}
        >
          {RUNGS.map((item) => (
            <option key={item.rung} value={item.rung}>
              {item.label}
            </option>
          ))}
        </select>
        <input
          aria-label="Texto exacto a aprobar"
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder={rung === "commit" ? "Mensaje del commit" : "Título"}
          required
        />
        <textarea
          aria-label="Detalle"
          value={body}
          onChange={(event) => setBody(event.target.value)}
          rows={2}
          placeholder={rung === "jira" ? "Operaciones exactas en Jira" : "Cuerpo (opcional)"}
        />
        <button type="submit" disabled={busy}>
          Pedir aprobación
        </button>
      </form>
    </section>
  );
}
