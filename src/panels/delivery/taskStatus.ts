// How a Delivery task reads in the view. Where a task is (its phase, who it
// waits on, the stage due next) is decided once, by Tinto's backend
// (src-tauri/src/delivery/phase.rs); these functions only word it, and word
// each job and delivery step.

import type {
  DeliveryApproval,
  DeliveryBlocker,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPr,
  DeliveryRung,
  DeliverySettingsProposal,
  DeliveryTask,
  DeliveryTaskPhase,
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

/** `step`: a delivery step (commit, push, PR, merge, Jira) the user acts on,
 *  told apart from blockers (`attention`, `danger`). */
export type Tone = "attention" | "step" | "danger" | "live" | "waiting" | "idle" | "ok";
export type StatusGroup = "attention" | "running" | "waiting" | "idle" | "delivered";
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
  { group: "delivered", label: "Entregadas" },
];

export function stageLabel(role: string): string {
  if (role === "bootstrap") return "Preparación";
  if (role === "checks") return "Comprobaciones";
  return STAGES.find((stage) => stage.role === role)?.label ?? role;
}

/** The history row a job belongs to: its stage, or each of Tinto's checks
 *  by its command, since they share a role. */
export function historyKey(job: DeliveryJob): string {
  return job.role === "checks" ? `checks ${job.prompt}` : job.role;
}

/** "Implementación" → "implementación", but keep "QA", "PR" and "Jira…" as written. */
export function lower(label: string): string {
  return label === label.toUpperCase() || label.startsWith("Jira") ? label : label.toLowerCase();
}

export function isOpen(job: DeliveryJob): boolean {
  return job.status === "queued" || job.status === "pending" || job.status === "started";
}

/** The paused job no later job resumes: it holds its task. */
export function pausedJob(jobs: DeliveryJob[]): DeliveryJob | undefined {
  return jobs.find(
    (job) => job.status === "paused" && !jobs.some((other) => other.resumes === job.id),
  );
}

/** How long the agent has been quiet, once it is long enough to say. A
 *  command it waits on is not quiet. */
export function quietFor(job: DeliveryJob, now: number): string | null {
  const since = job.activity_at_ms ?? job.started_at_ms;
  if (job.status !== "started" || !since || job.current_command) return null;
  const minutes = Math.floor((now - since) / 60_000);
  return minutes >= 5 ? `sin actividad hace ${minutes} min` : null;
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
  if (job.status === "paused") {
    return jobs.some((other) => other.resumes === job.id)
      ? { tone: "idle", label: "Pausado · reanudado" }
      : { tone: "waiting", label: "En pausa" };
  }
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
    const label = count === 1 ? "1 hallazgo" : count > 1 ? `${count} hallazgos` : "Con hallazgos";
    // Once a later stage answered them they are history, not a warning.
    return isAnswered(job, jobs)
      ? { tone: "idle", label: `${label} · atendidos` }
      : { tone: "attention", label };
  }
  return { tone: "idle", label: result.status };
}

/** Findings a later stage answered: one after it passed (the tests' failing
 *  cases, made to pass by the implementation). */
export function isAnswered(job: DeliveryJob, jobs: DeliveryJob[]): boolean {
  return (
    job.status === "finished" &&
    job.result?.status === "findings" &&
    jobs.some(
      (later) =>
        later.created_at_ms > job.created_at_ms &&
        stageOrder(later.role) > stageOrder(job.role) &&
        isPassed(later),
    )
  );
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

const MILESTONE_LABEL: Record<string, string> = {
  pr_opened: "PR abierto",
  merged: "fusionado",
  delivered: "entregada",
  closed: "cerrado",
};

/** The PR's repo and number from its URL: "owner/repo#305". */
export function prRef(url: string): string | null {
  const match = /github\.com\/([^/]+\/[^/]+)\/pull\/(\d+)/i.exec(url);
  return match ? `${match[1]}#${match[2]}` : null;
}

/** A blocker in a few words: "AGOS-580 (necesita: fusionado)", "PR owner/repo#305". */
export function blockerLabel(blocker: DeliveryBlocker): string {
  switch (blocker.type) {
    case "task":
      return `${blocker.key} (necesita: ${MILESTONE_LABEL[blocker.until] ?? blocker.until})`;
    case "pr": {
      const ref = prRef(blocker.url) ?? blocker.url;
      return blocker.until === "closed" ? `PR ${ref} (necesita: cerrado)` : `PR ${ref}`;
    }
    case "decision":
      return `una decisión de ${blocker.key}`;
    case "note":
      return blocker.text;
  }
}

/** The blockers the task still waits for. */
export function openBlockers(task: DeliveryTask): DeliveryBlocker[] {
  return (task.blockers ?? []).filter((blocker) => !blocker.cleared_at_ms);
}

/** The ladder step a phase names. */
function phaseStep(rung: DeliveryRung, stage: string | null): DeliveryStep {
  return (
    STEPS.find((step) => step.rung === rung && (step.stage ?? null) === (stage ?? null)) ?? {
      rung,
      label: rung,
    }
  );
}

/** The task's phase, as Tinto derives it; `null` until the overview has it. */
export function phaseOf(overview: DeliveryOverview, taskId: string): DeliveryTaskPhase | null {
  return overview.phases?.[taskId] ?? null;
}

/** The stage due next, as Tinto decides it. */
export function nextStageOf(overview: DeliveryOverview, taskId: string): string | null {
  return phaseOf(overview, taskId)?.next_stage ?? null;
}

/** How the task's phase reads in the list: Tinto decides where the task is;
 *  this only words it and adds what changes by the second (a clock). */
export function taskStatus(
  task: DeliveryTask,
  overview: DeliveryOverview,
  now: number,
): TaskStatus {
  const phase = phaseOf(overview, task.id);
  const attention = (label: string, icon: StatusIcon = "flag"): TaskStatus => ({
    group: "attention",
    tone: "attention",
    icon,
    label,
  });
  const step = (label: string, icon: StatusIcon = "flag"): TaskStatus => ({
    group: "attention",
    tone: "step",
    icon,
    label,
  });
  const danger = (label: string, icon: StatusIcon = "alert"): TaskStatus => ({
    group: "attention",
    tone: "danger",
    icon,
    label,
  });
  const waiting = (label: string): TaskStatus => ({
    group: "waiting",
    tone: "waiting",
    icon: "clock",
    label,
  });
  const idle = (label: string): TaskStatus => ({
    group: "idle",
    tone: "idle",
    icon: "ring",
    label,
  });
  if (!phase) return idle("Sin actividad");
  switch (phase.kind) {
    case "decisions":
      return attention(
        phase.count === 1 ? "1 decisión pendiente" : `${phase.count} decisiones pendientes`,
      );
    case "approval":
      return step(`Aprobar ${lower(phaseStep(phase.rung, phase.stage).label)}`);
    case "record":
      // Approved: running it and reporting it is the coordinator's turn.
      return waiting(
        `Esperando al coordinador: ${lower(phaseStep(phase.rung, phase.stage).label)}`,
      );
    case "drifted":
      return step(`${phaseStep(phase.rung, phase.stage).label}: ya no coincide`, "alert");
    case "running": {
      const job = overview.jobs.find((candidate) => candidate.id === phase.job_id);
      const verb =
        STAGES.find((stage) => stage.role === phase.role)?.verb ?? stageLabel(phase.role);
      const time = job ? clock(job, now) : "";
      const quiet = job ? quietFor(job, now) : null;
      return {
        group: "running",
        tone: "live",
        icon: "live",
        label: [verb, time, quiet].filter(Boolean).join(" · "),
      };
    }
    case "paused":
      return waiting(`${stageLabel(phase.role)} en pausa`);
    case "batch_paused":
      return waiting("Lote en pausa");
    case "waits_quarantine":
      return danger(`Espera ${phase.lease.toUpperCase()}, en cuarentena`);
    case "queued":
      return waiting(
        phase.lease
          ? `Esperando ${phase.lease.toUpperCase()} · ${ordinal(phase.position)} en la cola`
          : phase.position > 0
            ? `En cola · ${ordinal(phase.position)}`
            : "En cola",
      );
    case "quarantined":
      return danger(`${phase.lease.toUpperCase()} en cuarentena`);
    case "settings_proposal":
      return attention("Aprobar cambio de ajustes");
    case "blocked": {
      const open = openBlockers(task);
      return waiting(
        open.length === 1
          ? `Bloqueada por ${blockerLabel(open[0])}`
          : `Bloqueada · ${phase.open} bloqueos`,
      );
    }
    case "broken":
      return danger(
        `${phase.blocked ? "Bloqueada en" : "Falló"} ${lower(stageLabel(phase.role))} · intento ${phase.attempt}`,
        "x",
      );
    case "delivered":
      return { group: "delivered", tone: "ok", icon: "check", label: "Entregada" };
    case "pr": {
      const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
      if (!pr) return waiting("PR");
      const next = nextStep(overview.approvals.filter((approval) => approval.task_id === task.id));
      const then = next ? ` · siguiente: ${lower(next.label)}` : "";
      if (pr.state === "merged") return idle(`PR fusionado${then}`);
      const summary = prSummary(pr);
      const needsYou = summary.tone === "attention" || summary.tone === "danger";
      const ready = summary.label === "Aprobado";
      return {
        group: needsYou ? "attention" : ready ? "idle" : "waiting",
        tone: summary.tone,
        icon: summary.icon,
        label: ready ? `PR aprobado${then}` : `PR: ${lower(summary.label)}`,
      };
    }
    case "next_stage":
      return idle(`Siguiente: ${lower(stageLabel(phase.role))}`);
    case "next_step":
      return idle(`Siguiente: ${lower(phaseStep(phase.rung, phase.stage).label)}`);
  }
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
