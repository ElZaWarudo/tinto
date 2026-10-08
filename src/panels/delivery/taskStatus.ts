// What a Delivery task needs and where it is in its flow, derived from the
// overview the view already loads. Pure functions so every surface (list,
// header, track, banners) reads the same answer.

import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryRung,
  DeliveryTask,
} from "../../delivery/types";

export const STAGES = [
  { role: "tests", label: "Tests", verb: "Escribiendo tests" },
  { role: "implementation", label: "Implementación", verb: "Implementando" },
  { role: "review", label: "Revisión", verb: "Revisando" },
  { role: "qa", label: "QA", verb: "En QA" },
];

export const RUNGS: { rung: DeliveryRung; label: string }[] = [
  { rung: "commit", label: "Commit" },
  { rung: "push", label: "Push" },
  { rung: "pr", label: "PR" },
  { rung: "jira", label: "Jira" },
];

/** Task states the coordinator sets, in flow order. */
export const TASK_STATE_LABEL: Record<string, string> = {
  intake: "Entrada",
  contract_ready: "Contrato listo",
  tests: "Tests",
  implementation: "Implementación",
  review: "Revisión",
  qa_ready: "Lista para QA",
  qa_active: "En QA",
  changes_requested: "Cambios pedidos",
  review_ready: "Lista para revisión",
  blocked: "Bloqueada",
};

export type Tone = "attention" | "danger" | "live" | "waiting" | "idle" | "ok";
export type StatusGroup = "attention" | "running" | "waiting" | "idle";
export type StatusIcon = "flag" | "x" | "alert" | "live" | "clock" | "ring" | "check";

export interface TaskStatus {
  group: StatusGroup;
  tone: Tone;
  icon: StatusIcon;
  label: string;
}

export const GROUPS: { group: StatusGroup; label: string }[] = [
  { group: "attention", label: "Te necesitan" },
  { group: "running", label: "En marcha" },
  { group: "waiting", label: "En espera" },
  { group: "idle", label: "Sin actividad" },
];

export function stageLabel(role: string): string {
  if (role === "bootstrap") return "Preparación";
  return STAGES.find((stage) => stage.role === role)?.label ?? role;
}

export function rungLabel(rung: string): string {
  return RUNGS.find((item) => item.rung === rung)?.label ?? rung;
}

/** "Implementación" → "implementación", but keep "QA", "PR" and "Jira" as written. */
export function lower(label: string): string {
  return label === label.toUpperCase() || label === "Jira" ? label : label.toLowerCase();
}

export function isOpen(job: DeliveryJob): boolean {
  return job.status === "queued" || job.status === "pending" || job.status === "started";
}

/** A job that ended without a usable result. */
export function isBroken(job: DeliveryJob): boolean {
  return (
    job.status === "failed" ||
    job.status === "interrupted" ||
    job.result_state === "invalid" ||
    (job.status === "finished" && job.result?.status === "blocked")
  );
}

/** Not superseded: neither stale nor undone. */
function isLive(job: DeliveryJob): boolean {
  return job.result_state !== "stale" && !job.undone_at_ms;
}

function isCurrent(job: DeliveryJob): boolean {
  return isLive(job) && job.result_state !== "invalid";
}

export function isPassed(job: DeliveryJob): boolean {
  return job.status === "finished" && isCurrent(job) && job.result?.status === "pass";
}

/** The task's jobs, oldest first (the overview keeps them in creation order). */
export function jobsOf(overview: DeliveryOverview, taskId: string): DeliveryJob[] {
  return overview.jobs.filter((job) => job.task_id === taskId);
}

/** The stage's latest attempt that still counts; stale or undone ones only as a fallback. */
export function latestOfRole(jobs: DeliveryJob[], role: string): DeliveryJob | undefined {
  const ofRole = jobs.filter((job) => job.role === role).reverse();
  return ofRole.find(isLive) ?? ofRole[0];
}

/** "Lote de octubre" stays as is; "Octubre" becomes "Lote Octubre". */
export function batchName(title: string): string {
  return /^lote(\s|$)/i.test(title) ? title : `Lote ${title}`;
}

/** Built-in stages first, then any other role this task has used. */
export function rolesOf(jobs: DeliveryJob[]): string[] {
  const roles = STAGES.map((stage) => stage.role);
  for (const job of jobs) {
    if (job.agent !== "shell" && !roles.includes(job.role)) roles.push(job.role);
  }
  return roles;
}

/** One outcome per attempt: the strongest fact about it. */
export function attemptOutcome(job: DeliveryJob): { tone: Tone; label: string } {
  if (job.status === "queued") {
    return {
      tone: "waiting",
      label: job.lease ? `Esperando ${job.lease.toUpperCase()}` : "En cola",
    };
  }
  if (job.status === "pending") return { tone: "live", label: "Iniciando" };
  if (job.status === "started") return { tone: "live", label: "En marcha" };
  if (job.status === "cancelled") return { tone: "idle", label: "Cancelado" };
  if (job.status === "interrupted") return { tone: "danger", label: "Interrumpido" };
  if (job.status === "failed") return { tone: "danger", label: "Falló" };
  if (job.result_state === "stale") return { tone: "idle", label: "Obsoleto" };
  if (job.result_state === "invalid") return { tone: "danger", label: "Resultado inválido" };
  if (job.undone_at_ms) return { tone: "idle", label: "Deshecho" };
  const result = job.result;
  if (!result) return { tone: "idle", label: "Terminado" };
  if (result.status === "pass") return { tone: "ok", label: "Sin hallazgos" };
  if (result.status === "blocked") return { tone: "danger", label: "Bloqueado" };
  if (result.status === "findings") {
    const count = result.findings.length;
    return {
      tone: "attention",
      label: count === 1 ? "1 hallazgo" : count > 1 ? `${count} hallazgos` : "Con hallazgos",
    };
  }
  return { tone: "idle", label: result.status };
}

/**
 * The stage to run next: retry a broken one, go back to implementation after
 * findings, else the first built-in stage without a passing attempt.
 */
export function nextRole(jobs: DeliveryJob[]): string | null {
  const agentJobs = jobs.filter((job) => job.agent !== "shell" && isLive(job));
  const latest = agentJobs[agentJobs.length - 1];
  if (latest && !isOpen(latest)) {
    if (isBroken(latest)) return latest.role;
    if (latest.status === "finished" && latest.result?.status === "findings") {
      return "implementation";
    }
  }
  for (const stage of STAGES) {
    const job = latestOfRole(agentJobs, stage.role);
    if (!job || !isPassed(job)) return job && isOpen(job) ? null : stage.role;
  }
  return null;
}

/** The latest accepted handoff, offered as the next stage's instructions. */
export function lastHandoff(jobs: DeliveryJob[]): { text: string; from: DeliveryJob } | null {
  const from = [...jobs]
    .reverse()
    .find((job) => job.status === "finished" && isCurrent(job) && job.result?.handoff?.trim());
  return from ? { text: from.result!.handoff.trim(), from } : null;
}

export function latestApproval(
  approvals: DeliveryApproval[],
  rung: DeliveryRung,
): DeliveryApproval | undefined {
  return [...approvals].reverse().find((approval) => approval.rung === rung);
}

/** First rung not yet executed; null once Jira is done. */
export function nextRung(approvals: DeliveryApproval[]): DeliveryRung | null {
  return (
    RUNGS.find(({ rung }) => latestApproval(approvals, rung)?.status !== "executed")?.rung ?? null
  );
}

function clock(job: DeliveryJob, now: number): string {
  if (!job.started_at_ms) return "";
  const seconds = Math.max(0, Math.round(((job.ended_at_ms ?? now) - job.started_at_ms) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function ordinal(position: number): string {
  return `${position}.º`;
}

export function taskStatus(
  task: DeliveryTask,
  overview: DeliveryOverview,
  now: number,
): TaskStatus {
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const pending = approvals.find((approval) => approval.status === "pending");
  if (pending) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: `Aprobar ${lower(rungLabel(pending.rung))}`,
    };
  }
  const approved = approvals.find((approval) => approval.status === "approved");
  if (approved) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: `Registrar ${lower(rungLabel(approved.rung))}`,
    };
  }
  const quarantined = overview.leases.find(
    (lease) => lease.state === "quarantined" && lease.holder_task_id === task.id,
  );
  if (quarantined) {
    return {
      group: "attention",
      tone: "danger",
      icon: "alert",
      label: `${quarantined.name.toUpperCase()} en cuarentena`,
    };
  }

  const jobs = jobsOf(overview, task.id);
  const running = jobs.find((job) => job.status === "pending" || job.status === "started");
  if (running) {
    const verb =
      STAGES.find((stage) => stage.role === running.role)?.verb ?? stageLabel(running.role);
    const time = clock(running, now);
    return {
      group: "running",
      tone: "live",
      icon: "live",
      label: time ? `${verb} · ${time}` : verb,
    };
  }
  const queued = jobs.find((job) => job.status === "queued");
  if (queued) {
    const lease = overview.leases.find((candidate) =>
      candidate.queue.some((waiter) => waiter.job_id === queued.id),
    );
    if (lease) {
      const position = lease.queue.findIndex((waiter) => waiter.job_id === queued.id) + 1;
      return {
        group: "waiting",
        tone: "waiting",
        icon: "clock",
        label: `Esperando ${lease.name.toUpperCase()} · ${ordinal(position)} en la cola`,
      };
    }
    const queue = overview.jobs
      .filter((job) => job.status === "queued")
      .filter((job) => !overview.leases.some((l) => l.queue.some((w) => w.job_id === job.id)))
      .sort((a, b) => a.created_at_ms - b.created_at_ms);
    const position = queue.findIndex((job) => job.id === queued.id) + 1;
    return {
      group: "waiting",
      tone: "waiting",
      icon: "clock",
      label: position > 0 ? `En cola · ${ordinal(position)}` : "En cola",
    };
  }

  const latest = [...jobs].reverse().find((job) => job.agent !== "shell" && isLive(job));
  if (latest && isBroken(latest)) {
    const what = latest.status === "finished" ? "Bloqueada en" : "Falló";
    return {
      group: "attention",
      tone: "danger",
      icon: "x",
      label: `${what} ${lower(stageLabel(latest.role))} · intento ${latest.attempt}`,
    };
  }

  if (latestApproval(approvals, "jira")?.status === "executed") {
    return { group: "idle", tone: "ok", icon: "check", label: "Entregada" };
  }
  const role = nextRole(jobs);
  if (role) {
    return {
      group: "idle",
      tone: "idle",
      icon: "ring",
      label: `Siguiente: ${lower(stageLabel(role))}`,
    };
  }
  const rung = nextRung(approvals);
  return {
    group: "idle",
    tone: "idle",
    icon: "ring",
    label: rung ? `Siguiente: ${lower(rungLabel(rung))}` : "Sin actividad",
  };
}
