// What a Delivery task needs and where it is in its flow, derived from the
// overview the view already loads. Pure functions so every surface (list,
// header, track, banners) reads the same answer.

import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPr,
  DeliveryRung,
  DeliverySettingsProposal,
  DeliveryTask,
} from "../../delivery/types";

export const STAGES = [
  { role: "tests", label: "Tests", verb: "Escribiendo tests" },
  { role: "implementation", label: "Implementación", verb: "Implementando" },
  { role: "review", label: "Revisión", verb: "Revisando" },
  { role: "qa", label: "QA", verb: "En QA" },
];

export type JiraStage = "after_pr" | "after_merge";

/** One step of the delivery ladder. Jira comes twice: once the PR is open
 *  (the issue goes to review) and once it is merged (the issue is closed). */
export interface DeliveryStep {
  rung: DeliveryRung;
  stage?: JiraStage;
  label: string;
}

export const STEPS: DeliveryStep[] = [
  { rung: "commit", label: "Commit" },
  { rung: "push", label: "Push" },
  { rung: "pr", label: "PR" },
  { rung: "jira", stage: "after_pr", label: "Jira (PR)" },
  { rung: "merge", label: "Merge" },
  { rung: "jira", stage: "after_merge", label: "Jira (merge)" },
];

function isStep(approval: DeliveryApproval, step: DeliveryStep): boolean {
  return (
    approval.rung === step.rung &&
    (step.stage === undefined || (approval.stage ?? "after_pr") === step.stage)
  );
}

/** The ladder step an approval is for. */
export function stepOf(approval: DeliveryApproval): DeliveryStep {
  return (
    STEPS.find((step) => isStep(approval, step)) ?? { rung: approval.rung, label: approval.rung }
  );
}

/** A key that tells the two Jira steps apart. */
export function stepKey(step: DeliveryStep): string {
  return step.stage ? `${step.rung}-${step.stage}` : step.rung;
}

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

/** "Implementación" → "implementación", but keep "QA", "PR" and "Jira…" as written. */
export function lower(label: string): string {
  return label === label.toUpperCase() || label.startsWith("Jira") ? label : label.toLowerCase();
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

function stageOrder(role: string): number {
  return STAGES.findIndex((stage) => stage.role === role);
}

/**
 * A result left behind by later work on an earlier stage: new tests after the
 * implementation, or new code after its review or QA. `jobs` are the task's,
 * oldest first.
 */
export function isSuperseded(job: DeliveryJob, jobs: DeliveryJob[]): boolean {
  const order = stageOrder(job.role);
  const at = jobs.findIndex((other) => other.id === job.id);
  if (order < 1 || at < 0) return false;
  return jobs.slice(at + 1).some((later) => {
    const laterOrder = stageOrder(later.role);
    return laterOrder >= 0 && laterOrder < order && !later.undone_at_ms && later.changes.length > 0;
  });
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

/** One outcome per attempt: the strongest fact about it. With the task's
 *  `jobs`, a pass that later work left behind says so. */
export function attemptOutcome(
  job: DeliveryJob,
  jobs: DeliveryJob[] = [],
): { tone: Tone; label: string } {
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
  if (result.status === "pass") {
    return isSuperseded(job, jobs)
      ? { tone: "idle", label: "Superado" }
      : { tone: "ok", label: "Sin hallazgos" };
  }
  if (result.status === "blocked") {
    const by = { sandbox: " por el sandbox", environment: " por el entorno" }[
      result.blocked_by ?? ""
    ];
    return { tone: "danger", label: `Bloqueado${by ?? ""}` };
  }
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
 * findings, else the first built-in stage without a passing attempt that
 * still stands.
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
    if (!job || !isPassed(job) || isSuperseded(job, agentJobs)) {
      return job && isOpen(job) ? null : stage.role;
    }
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

/** The latest approval for a ladder step (each Jira step on its own). */
export function latestStep(
  approvals: DeliveryApproval[],
  step: DeliveryStep,
): DeliveryApproval | undefined {
  return [...approvals].reverse().find((approval) => isStep(approval, step));
}

/** Delivered once every step is done: commit, push, PR, merge and both Jira. */
export function isDelivered(approvals: DeliveryApproval[]): boolean {
  return nextStep(approvals) === null;
}

/** First step not done; null once every step is. */
export function nextStep(approvals: DeliveryApproval[]): DeliveryStep | null {
  return STEPS.find((step) => latestStep(approvals, step)?.status !== "executed") ?? null;
}

function clock(job: DeliveryJob, now: number): string {
  if (!job.started_at_ms) return "";
  const seconds = Math.max(0, Math.round(((job.ended_at_ms ?? now) - job.started_at_ms) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function ordinal(position: number): string {
  return `${position}.º`;
}

export function pendingDecisions(overview: DeliveryOverview, taskId: string): number {
  return (overview.decisions ?? []).filter(
    (decision) => decision.task_id === taskId && decision.status === "pending",
  ).length;
}

/** The settings changes waiting for the user on the task's repo. */
export function pendingSettingsProposals(
  overview: DeliveryOverview,
  task: DeliveryTask,
): DeliverySettingsProposal[] {
  return (overview.settings_proposals ?? []).filter(
    (proposal) => proposal.repo === task.repo && proposal.status === "pending",
  );
}

export function taskStatus(
  task: DeliveryTask,
  overview: DeliveryOverview,
  now: number,
): TaskStatus {
  const decisions = pendingDecisions(overview, task.id);
  if (decisions > 0) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: decisions === 1 ? "1 decisión pendiente" : `${decisions} decisiones pendientes`,
    };
  }
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const pending = approvals.find((approval) => approval.status === "pending");
  if (pending) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: `Aprobar ${lower(stepOf(pending).label)}`,
    };
  }
  const approved = approvals.find((approval) => approval.status === "approved");
  if (approved) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: `Registrar ${lower(stepOf(approved).label)}`,
    };
  }
  const drifted = approvals.find(
    (approval) =>
      approval.status === "drifted" && latestStep(approvals, stepOf(approval)) === approval,
  );
  if (drifted) {
    return {
      group: "attention",
      tone: "attention",
      icon: "alert",
      label: `${stepOf(drifted).label}: ya no coincide`,
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
  // A proposal is the repo's, but only the task that asked for it waits on
  // it, and only once its agents stop: it holds no job already dispatched.
  if (pendingSettingsProposals(overview, task).some((proposal) => proposal.task_id === task.id)) {
    return {
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: "Aprobar cambio de ajustes",
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

  if (isDelivered(approvals)) {
    return { group: "idle", tone: "ok", icon: "check", label: "Entregada" };
  }
  const next = nextStep(approvals);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  if (pr) {
    const summary = prSummary(pr);
    const then = next ? ` · siguiente: ${lower(next.label)}` : "";
    if (pr.state === "merged") {
      return { group: "idle", tone: "idle", icon: "ring", label: `PR fusionado${then}` };
    }
    const needsYou = summary.tone === "attention" || summary.tone === "danger";
    const ready = summary.label === "Aprobado";
    return {
      group: needsYou ? "attention" : ready ? "idle" : "waiting",
      tone: summary.tone,
      icon: summary.icon,
      label: ready ? `PR aprobado${then}` : `PR: ${lower(summary.label)}`,
    };
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
  return {
    group: "idle",
    tone: "idle",
    icon: "ring",
    label: next ? `Siguiente: ${lower(next.label)}` : "Sin actividad",
  };
}

/** One line for the state of a followed PR, from most to least urgent. */
export function prSummary(pr: DeliveryPr): { tone: Tone; icon: StatusIcon; label: string } {
  if (pr.state === "merged") return { tone: "ok", icon: "check", label: "Fusionado" };
  if (pr.state === "closed") return { tone: "danger", icon: "x", label: "Cerrado sin fusionar" };
  if (pr.state === "unknown") return { tone: "waiting", icon: "clock", label: "Leyendo GitHub" };
  const fresh = pr.new_reviews + pr.new_comments;
  if (fresh > 0) {
    return {
      tone: "attention",
      icon: "flag",
      label: fresh === 1 ? "1 novedad" : `${fresh} novedades`,
    };
  }
  if (pr.checks.failing > 0) return { tone: "danger", icon: "x", label: "Checks fallando" };
  if (pr.conflicts) return { tone: "attention", icon: "alert", label: "Conflictos" };
  if (pr.review_decision === "changes_requested") {
    return { tone: "attention", icon: "alert", label: "Cambios pedidos" };
  }
  if (pr.checks.pending > 0) return { tone: "waiting", icon: "clock", label: "Checks en curso" };
  if (pr.draft) return { tone: "waiting", icon: "clock", label: "Borrador" };
  // Without required reviews GitHub gives no decision; an approval still counts.
  const approved =
    pr.review_decision === "approved" || (!pr.review_decision && (pr.approved_by?.length ?? 0) > 0);
  if (approved) return { tone: "ok", icon: "check", label: "Aprobado" };
  return { tone: "waiting", icon: "clock", label: "En revisión" };
}

/** "En ejecución, In Review" (or with → between them) → the statuses in order. */
export function parseStatuses(text: string): string[] {
  return text
    .split(/[,→\n]/)
    .map((status) => status.trim())
    .filter(Boolean);
}
