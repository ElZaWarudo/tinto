// How a Delivery task reads in the view. Where a task is (its phase, who it
// waits on, the stage due next) is decided once, by Tinto's backend
// (src-tauri/src/delivery/phase.rs); these functions only word it, and word
// each job and delivery step.

import type {
  DeliveryApproval,
  DeliveryBlocker,
  DeliveryDecision,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPr,
  DeliveryRung,
  DeliverySettingsProposal,
  DeliverySplitIssue,
  DeliveryTask,
  DeliveryTaskPhase,
} from "../../delivery/types";

/** Alcance: the read-only scoping stage, first in the row. */
export const SCOPE_ROLE = "explore";

export const STAGES = [
  { role: SCOPE_ROLE, label: "Alcance", verb: "Explorando el alcance" },
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

/** Merging the base into the branch: asked only when needed, and as often,
 *  so it is not a step every task climbs. */
export const SYNC_STEP: DeliveryStep = { rung: "sync", label: "Sync con la base" };

/** The ladder as the track shows it: with the sync after the commit once the
 *  task has one. */
export function trackSteps(steps: DeliveryStep[], approvals: DeliveryApproval[]): DeliveryStep[] {
  if (!approvals.some((approval) => approval.rung === "sync")) return steps;
  const at = steps.findIndex((step) => step.rung === "commit") + 1;
  return [...steps.slice(0, at), SYNC_STEP, ...steps.slice(at)];
}

function isStep(approval: DeliveryApproval, step: DeliveryStep): boolean {
  return (
    approval.rung === step.rung &&
    (step.stage === undefined || (approval.stage ?? "after_pr") === step.stage)
  );
}

/** A reply to a reviewer: asked per comment, outside the ladder. */
export const REPLY_STEP: DeliveryStep = { rung: "pr_reply", label: "Respuesta al revisor" };

/** The ladder step an approval is for. */
export function stepOf(approval: DeliveryApproval): DeliveryStep {
  if (approval.rung === "pr_reply") return REPLY_STEP;
  return (
    [...STEPS, SYNC_STEP].find((step) => isStep(approval, step)) ?? {
      rung: approval.rung,
      label: approval.rung,
    }
  );
}

/** How a step reads where it is asked: a PR step on a branch whose PR is
 *  already open sets its text on that PR instead of opening one. */
export function stepName(step: DeliveryStep, pr: DeliveryPr | null | undefined): string {
  return step.rung === "pr" && pr?.state === "open"
    ? `texto del PR #${pr.number}`
    : lower(step.label);
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
export type Tone = "attention" | "step" | "danger" | "live" | "waiting" | "blocked" | "idle" | "ok";
export type StatusGroup = "attention" | "running" | "waiting" | "idle" | "delivered";
export type StatusIcon = "flag" | "x" | "alert" | "live" | "clock" | "lock" | "ring" | "check";

export interface TaskStatus {
  group: StatusGroup;
  tone: Tone;
  icon: StatusIcon;
  label: string;
  /** What it waits for (a blocked or queued task), in a line of its own. */
  detail?: string;
}

export const GROUPS: { group: StatusGroup; label: string }[] = [
  { group: "attention", label: "Te necesitan" },
  { group: "running", label: "En marcha" },
  { group: "waiting", label: "En espera" },
  { group: "idle", label: "Sin actividad" },
  { group: "delivered", label: "Entregadas" },
];

export function stageLabel(role: string): string {
  if (role === "bootstrap" || role === "setup") return "Preparación";
  if (role === "checks") return "Comprobaciones";
  return STAGES.find((stage) => stage.role === role)?.label ?? role;
}

/** Tinto's own shell jobs that share a role: each is told by its command. */
export function byCommand(job: DeliveryJob): boolean {
  return job.role === "checks" || job.role === "setup";
}

/** The history row a job belongs to: its stage, or each of Tinto's checks
 *  and setup commands by its command, since they share a role. */
export function historyKey(job: DeliveryJob): string {
  return byCommand(job) ? `${job.role} ${job.prompt}` : job.role;
}

/** "Implementación" → "implementación", but keep "QA", "PR" and "Jira…" as written. */
export function lower(label: string): string {
  return label === label.toUpperCase() || label.startsWith("Jira") || label.startsWith("CI ")
    ? label
    : label.toLowerCase();
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

/** A job that ended without a usable result, and was not settled since. */
export function isBroken(job: DeliveryJob): boolean {
  return (
    !job.resolved_at_ms &&
    (job.status === "failed" ||
      job.status === "interrupted" ||
      job.result_state === "invalid" ||
      (job.status === "finished" && job.result?.status === "blocked"))
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
  return (
    (!!job.resolved_at_ms && isLive(job)) ||
    (job.status === "finished" && isCurrent(job) && job.result?.status === "pass")
  );
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
  // Nothing comes before the tests that changes the code (Alcance reads).
  if (order <= stageOrder("tests") || at < 0) return false;
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
  if (job.resolved_at_ms && isLive(job)) {
    // Findings accepted as they are, or a broken stage done another way.
    const findings = job.result?.status === "findings" ? job.result.findings.length : 0;
    if (findings > 0) {
      return {
        tone: "ok",
        label: `${findings === 1 ? "1 hallazgo" : `${findings} hallazgos`} · aceptados`,
      };
    }
    return { tone: "ok", label: "Resuelta aparte" };
  }
  if (job.status === "interrupted") return { tone: "danger", label: "Interrumpido" };
  if (job.status === "failed") return { tone: "danger", label: "Falló" };
  if (job.result_state === "stale") return { tone: "idle", label: "Obsoleto" };
  if (job.result_state === "invalid") return { tone: "danger", label: "Resultado inválido" };
  if (job.undone_at_ms) return { tone: "idle", label: "Deshecho" };
  const result = job.result;
  if (!result) return { tone: "idle", label: "Terminado" };
  // Alcance's findings are its questions for the user: a normal outcome.
  if (job.role === SCOPE_ROLE && result.status !== "blocked") {
    const count = result.status === "findings" ? result.findings.length : 0;
    return { tone: "ok", label: count > 0 ? `Listo · ${questions(count)}` : "Listo" };
  }
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

export function questions(count: number): string {
  return count === 1 ? "1 pregunta" : `${count} preguntas`;
}

/** How Alcance reads on the task row: skipped (with who and why), or its
 *  latest job, its questions counted from the task's decisions once asked. */
export function scopeOutcome(
  task: DeliveryTask,
  job: DeliveryJob | undefined,
  jobs: DeliveryJob[],
  decisions: DeliveryDecision[],
): { tone: Tone; label: string; title?: string } | null {
  if (task.scope_skip && (!job || !isOpen(job))) {
    const by = task.scope_skip.by === "user" ? "ti" : "el coordinador";
    return {
      tone: "idle",
      label: "Omitido",
      title: `Omitido por ${by}: ${task.scope_skip.reason}`,
    };
  }
  if (!job) return null;
  const outcome = attemptOutcome(job, jobs);
  if (outcome.tone !== "ok" || !outcome.label.startsWith("Listo")) return outcome;
  const asked = decisions.filter((decision) => decision.kind !== "permission");
  if (asked.length === 0) return outcome;
  const decided = asked.filter((decision) => decision.status === "answered").length;
  return {
    tone: "ok",
    label: `Listo · ${questions(asked.length)} · ${decided} ${decided === 1 ? "decidida" : "decididas"}`,
  };
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

/** Delivered once every step of its ladder is done: commit, push, PR, merge and its Jira. */
export function isDelivered(approvals: DeliveryApproval[], steps = STEPS): boolean {
  return nextStep(approvals, steps) === null;
}

/** First step not done; null once every step is. */
export function nextStep(approvals: DeliveryApproval[], steps = STEPS): DeliveryStep | null {
  return steps.find((step) => latestStep(approvals, step)?.status !== "executed") ?? null;
}

/** The ladder the task climbs, as Tinto says: a middle package of a split issue ends at merge. */
export function stepsOf(overview: DeliveryOverview, taskId: string): DeliveryStep[] {
  const steps = phaseOf(overview, taskId)?.steps;
  if (!steps?.length) return STEPS;
  return STEPS.filter((step) =>
    steps.some((wanted) => wanted.rung === step.rung && (wanted.stage ?? undefined) === step.stage),
  );
}

/** "AGOS-432 · 2/3" for a package of a split issue, the key otherwise. */
export function taskLabel(task: DeliveryTask): string {
  return task.package ? `${task.key} · ${task.package.index}/${task.package.of}` : task.key;
}

/** Where a package stands, in a word or two. */
function packageWhere(overview: DeliveryOverview, taskId: string): string {
  const phase = phaseOf(overview, taskId);
  switch (phase?.kind) {
    case "running":
      return `en ${lower(stageLabel(phase.role))}`;
    case "queued":
      return "en cola";
    case "blocked":
      return "bloqueado";
    case "decisions":
      return "con decisiones";
    case "qa_problems":
      return "con problemas de QA";
    case "pr":
      return "PR abierto";
    case "pr_waits_predecessor":
      return "PR en espera";
    default:
      return "en curso";
  }
}

/** The split issue a package belongs to, as Tinto sums it up. */
export function splitIssueOf(
  overview: DeliveryOverview,
  task: DeliveryTask,
): DeliverySplitIssue | null {
  return (
    overview.issues?.find((candidate) =>
      candidate.packages.some((pkg) => pkg.task_id === task.id),
    ) ?? null
  );
}

/** Where package `index` of a split issue stands: "fusionado", "en QA", "sin crear". */
export function packageState(
  overview: DeliveryOverview,
  issue: DeliverySplitIssue,
  index: number,
): string {
  const pkg = issue.packages.find((candidate) => candidate.index === index);
  if (!pkg) return "sin crear";
  if (pkg.delivered) return "entregado";
  if (pkg.merged) return "fusionado";
  return packageWhere(overview, pkg.task_id);
}

/** "1/2 fusionado · 2/2 en QA": where each package of the task's split
 *  issue stands; null for a task that is a whole issue. */
export function issueProgress(overview: DeliveryOverview, task: DeliveryTask): string | null {
  const issue = splitIssueOf(overview, task);
  if (!issue) return null;
  const parts = [];
  for (let index = 1; index <= issue.of; index++) {
    parts.push(`${index}/${issue.of} ${packageState(overview, issue, index)}`);
  }
  return parts.join(" · ");
}

/** A row of the task list: a task, or a split issue holding its packages in order. */
export type TaskListEntry =
  | { kind: "task"; task: DeliveryTask }
  | { kind: "issue"; id: string; key: string; packages: DeliveryTask[] };

/** What selects a split issue's row in its repo's list. */
export function issueRowId(task: DeliveryTask): string {
  return `issue:${task.key.toLowerCase()}`;
}

/** The task list by status group. A split issue is one entry, in the group
 *  of its package that needs the most (the first group among theirs). */
export function taskListGroups(
  tasks: DeliveryTask[],
  groupOf: (task: DeliveryTask) => StatusGroup,
): { group: StatusGroup; label: string; entries: TaskListEntry[] }[] {
  const rank = (group: StatusGroup) => GROUPS.findIndex((candidate) => candidate.group === group);
  const issues = new Map<string, DeliveryTask[]>();
  for (const task of tasks) {
    if (!task.package) continue;
    issues.set(issueRowId(task), [...(issues.get(issueRowId(task)) ?? []), task]);
  }
  const entryGroup = (task: DeliveryTask) =>
    task.package
      ? GROUPS[Math.min(...issues.get(issueRowId(task))!.map((one) => rank(groupOf(one))))].group
      : groupOf(task);
  return GROUPS.map((group) => {
    const entries: TaskListEntry[] = [];
    for (const task of tasks) {
      if (entryGroup(task) !== group.group) continue;
      if (!task.package) {
        entries.push({ kind: "task", task });
        continue;
      }
      const id = issueRowId(task);
      if (entries.some((entry) => entry.kind === "issue" && entry.id === id)) continue;
      const packages = [...issues.get(id)!].sort((a, b) => a.package!.index - b.package!.index);
      entries.push({ kind: "issue", id, key: task.key, packages });
    }
    return { ...group, entries };
  }).filter((group) => group.entries.length > 0);
}

/** The tasks of the list's entries, packages in order. */
export function entryTasks(entries: TaskListEntry[]): DeliveryTask[] {
  return entries.flatMap((entry) => (entry.kind === "task" ? [entry.task] : entry.packages));
}

/** How long since `since` (ms), as a clock: "1:05". */
export function elapsed(since: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - since) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** What the merge needs of the PR, each with whether it holds as Tinto last
 *  read GitHub: the CI of its last commit, its approvals, no conflicts, not a
 *  draft. Tinto checks them again when the merge is approved. */
export function mergeChecks(pr: DeliveryPr): { ok: boolean; label: string }[] {
  const missing = missingApprovals(pr);
  const approvedBy = pr.approved_by ?? [];
  const base = pr.base ?? "la base";
  return [
    {
      ok: pr.ci === "passing" || pr.ci == null,
      label: pr.ci == null ? "Sin CI en este repositorio" : (ciLine(pr) ?? "CI"),
    },
    missing.length > 0
      ? { ok: false, label: `Falta la aprobación de ${missing.join(", ")}` }
      : pr.review_decision === "changes_requested"
        ? { ok: false, label: "Hay cambios pedidos" }
        : pr.review_decision === "review_required"
          ? { ok: false, label: "GitHub aún pide una revisión aprobada" }
          : approvedBy.length > 0
            ? { ok: true, label: `Aprobado por ${approvedBy.join(", ")}` }
            : { ok: pr.review_decision !== "review_required", label: "Sin revisión aprobada" },
    pr.conflicts
      ? { ok: false, label: `Conflictos con ${base}` }
      : { ok: true, label: `Sin conflictos con ${base}` },
    ...(pr.draft ? [{ ok: false, label: "Es un borrador" }] : []),
  ];
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

/** The task's QA problems that wait on the user: the new ones. One that
 *  still fails has nothing for the user: QA reports it again as a new one. */
export function qaProblemsToDecide(overview: DeliveryOverview, taskId: string): number {
  return (overview.qa_problems ?? []).filter(
    (problem) => problem.task_id === taskId && problem.status === "open",
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

/** What a blocked task waits for, as its list row says it: "Espera a
 *  AGOS-580 · fusionado", "Espera al PR acme/api#305", or the note. */
export function blockerWait(blocker: DeliveryBlocker): string {
  switch (blocker.type) {
    case "task":
      return `Espera a ${blocker.key} · ${MILESTONE_LABEL[blocker.until] ?? blocker.until}`;
    case "pr": {
      const ref = prRef(blocker.url) ?? blocker.url;
      return blocker.until === "closed"
        ? `Espera a que se cierre el PR ${ref}`
        : `Espera al PR ${ref}`;
    }
    case "decision":
      return `Espera una decisión de ${blocker.key}`;
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

/** The task's open round, as Tinto says it; null outside one. */
export function roundOf(overview: DeliveryOverview, taskId: string): DeliveryCurrentRound | null {
  return phaseOf(overview, taskId)?.round ?? null;
}

export type DeliveryCurrentRound = NonNullable<DeliveryTaskPhase["round"]>;

/** "Implementando · 2:10" → "implementando · 2:10"; "QA en pausa" stays. */
function lowerFirst(text: string): string {
  return /^\p{Lu}\p{Lu}/u.test(text) ? text : text.charAt(0).toLowerCase() + text.slice(1);
}

/** How the task's phase reads in the list: Tinto decides where the task is;
 *  this only words it and adds what changes by the second (a clock). In a
 *  round it says which and why first: "↺ Ronda 3 · PR: implementando"; when
 *  what one of its queued jobs waits for looks stuck, it says so. */
export function taskStatus(
  task: DeliveryTask,
  overview: DeliveryOverview,
  now: number,
): TaskStatus {
  const phased = phaseStatus(task, overview, now);
  // QA problems wait on the user without holding the task: the list shows
  // them unless something there already needs the user.
  const problems = qaProblemsToDecide(overview, task.id);
  const problemsLabel = `Decidir ${problems} problema${problems === 1 ? "" : "s"} de QA`;
  const status: TaskStatus =
    problems === 0
      ? phased
      : phased.group === "attention"
        ? { ...phased, detail: [phased.detail, problemsLabel].filter(Boolean).join(" · ") }
        : {
            group: "attention",
            tone: "attention",
            icon: "flag",
            label: problemsLabel,
            detail: [phased.label, phased.detail].filter(Boolean).join(" · "),
          };
  const warning = phaseOf(overview, task.id)?.warning;
  const warned = warning
    ? { ...status, detail: [status.detail, warning].filter(Boolean).join(" · ") }
    : status;
  const round = roundOf(overview, task.id);
  if (!round) return warned;
  const joint = round.kind === "pr" ? ":" : " ·";
  return {
    ...warned,
    label: `↺ Ronda ${round.number} · ${round.short}${joint} ${lowerFirst(status.label)}`,
  };
}

/** What the task is doing now, naming its stage: "Implementación en cola",
 *  "Comprobaciones en marcha · 6:35"; otherwise as the list says it. */
export function nowLabel(task: DeliveryTask, overview: DeliveryOverview, now: number): string {
  const phase = phaseOf(overview, task.id);
  if (phase?.kind === "running") {
    const job = overview.jobs.find((candidate) => candidate.id === phase.job_id);
    const quiet = job ? quietFor(job, now) : null;
    return [`${stageLabel(phase.role)} en marcha`, job ? clock(job, now) : "", quiet]
      .filter(Boolean)
      .join(" · ");
  }
  if (phase?.kind === "queued") {
    return phase.lease
      ? `${stageLabel(phase.role)} espera ${phase.lease.toUpperCase()} · ${ordinal(phase.position)} en la cola`
      : `${stageLabel(phase.role)} en cola`;
  }
  return phaseStatus(task, overview, now).label;
}

/** The header's "Ahora" line: the round and its cause first, when in one. */
export function nowLine(task: DeliveryTask, overview: DeliveryOverview, now: number): string {
  const round = roundOf(overview, task.id);
  const doing = nowLabel(task, overview, now);
  return round ? `Ronda ${round.number} · ${round.cause} · ${doing}` : doing;
}

/** The task's rounds that ended, newest first, with round 1 (the initial
 *  delivery) last: "Ronda 2 · 5 hallazgos de QA → corregidos". Empty while
 *  the task never went back. */
export function roundHistory(overview: DeliveryOverview, taskId: string): string[] {
  const rounds = (overview.rounds ?? []).filter((round) => round.task_id === taskId);
  if (rounds.length === 0) return [];
  return [
    ...rounds
      .filter((round) => round.closed_at_ms)
      .reverse()
      .map((round) => `Ronda ${round.number} · ${round.cause} → ${round.outcome ?? "cerrada"}`),
    "Ronda 1 · entrega inicial",
  ];
}

/** How a stage reads in the track while the task is in a round, for an
 *  attempt from before it: the tests still stand, implementation and what
 *  follows it run again. null when its own outcome applies (an attempt of
 *  this round, no round, another kind of stage). */
export function roundStage(
  role: string,
  job: DeliveryJob | undefined,
  jobs: DeliveryJob[],
  round: DeliveryCurrentRound | null,
): { tone: Tone | "todo"; label: string } | null {
  if (!round || (job && job.created_at_ms >= round.opened_at_ms)) return null;
  // Alcance reads as it went; the tests still stand; what follows runs again.
  const order = stageOrder(role);
  const tests = stageOrder("tests");
  if (order > tests) return { tone: "todo", label: "Se repite en esta ronda" };
  if (
    order === tests &&
    job &&
    (isPassed(job) || isAnswered(job, jobs)) &&
    !isSuperseded(job, jobs)
  ) {
    return { tone: "ok", label: "Vale de la ronda anterior" };
  }
  return null;
}

/** How a commit or push step reads while a round has to do it again, until
 *  it is asked for in the round: what it was before goes to its title. */
export function roundRung(
  step: DeliveryStep,
  approval: DeliveryApproval | undefined,
  round: DeliveryCurrentRound | null,
): boolean {
  if (!round || !(step.rung === "commit" || (step.rung === "push" && round.after_push))) {
    return false;
  }
  return !approval || approval.requested_at_ms < round.opened_at_ms;
}

function phaseStatus(task: DeliveryTask, overview: DeliveryOverview, now: number): TaskStatus {
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
    case "qa_problems":
      return attention(
        phase.count === 1
          ? "1 problema de QA por decidir"
          : `${phase.count} problemas de QA por decidir`,
      );
    case "approval": {
      const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
      return step(`Aprobar ${stepName(phaseStep(phase.rung, phase.stage), pr)}`);
    }
    case "step_running":
      return {
        group: "running",
        tone: "live",
        icon: "live",
        label: `${phase.step} · ${elapsed(phase.since_ms, now)}`,
      };
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
      // Only the user can release it while the batch asks them things.
      return phase.waits_on === "user"
        ? danger(`${phase.lease.toUpperCase()} bloqueada · libera el recurso`, "lock")
        : waiting(`Espera ${phase.lease.toUpperCase()}, en cuarentena`);
    case "setup_failed":
      return danger(`Falló la preparación: ${phase.command}`, "x");
    case "waits_environment":
      // Starting Docker or a database is the user's; Tinto checks again by itself.
      return attention(`${stageLabel(phase.role)} espera al entorno`);
    case "queued":
      return {
        ...waiting(
          phase.lease
            ? `Esperando ${phase.lease.toUpperCase()} · ${ordinal(phase.position)} en la cola`
            : phase.position > 0
              ? `En cola · ${ordinal(phase.position)}`
              : "En cola",
        ),
        // Why it has not started, as the scheduler says.
        detail: phase.reason ?? undefined,
      };
    case "quarantined":
      return danger(`${phase.lease.toUpperCase()} en cuarentena`);
    case "settings_proposal":
      return attention("Aprobar cambio de ajustes");
    case "blocked": {
      const open = openBlockers(task);
      return {
        group: "waiting",
        tone: "blocked",
        icon: "lock",
        label: open.length > 1 ? `Bloqueada · ${open.length} bloqueos` : "Bloqueada",
        detail: open.map(blockerWait).join(" · ") || undefined,
      };
    }
    case "unblocked":
      // Cleared, but the coordinator has not taken it up again yet.
      return {
        ...idle("Desbloqueada · espera al coordinador"),
        detail: `${phase.by === "user" ? "Quitaste" : "Se resolvió"}: ${phase.cleared}`,
      };
    case "broken":
      return danger(
        `${phase.blocked ? "Bloqueada en" : "Falló"} ${lower(stageLabel(phase.role))} · intento ${phase.attempt}`,
        "x",
      );
    case "delivered":
      return { group: "delivered", tone: "ok", icon: "check", label: "Entregada" };
    case "waits_merge_commit":
      return {
        ...waiting("Espera el commit del merge"),
        detail:
          phase.waiting_round === null
            ? "Hay un merge de la base en curso: se confirma antes de cualquier otra etapa."
            : `La ronda ${phase.waiting_round} corre encima del commit del merge, en commits aparte.`,
      };
    case "merge_mixed":
      return {
        ...attention("Merge mezclado con otros cambios", "alert"),
        detail: `Cambios de ${phase.mixed_rounds.length === 1 ? "la ronda" : "las rondas"} ${phase.mixed_rounds.join(", ")} quedaron dentro del merge en curso: decide cómo confirmarlos.`,
      };
    case "pr_waits_predecessor":
      return {
        ...waiting(`PR tras fusionar ${phase.package}`),
        detail:
          "Sus etapas, commit y push siguen; cuando se fusione, Tinto la rebasa sobre la base y el PR va allí.",
      };
    case "merged_failing_ci":
      return {
        ...danger(`PR #${phase.number} fusionado con checks fallando`, "x"),
        detail: phase.why,
      };
    case "pr": {
      const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
      if (!pr) return waiting("PR");
      const next = nextStep(
        overview.approvals.filter((approval) => approval.task_id === task.id),
        stepsOf(overview, task.id),
      );
      const then = next ? ` · siguiente: ${lower(next.label)}` : "";
      // Tinto says what the PR waits on and who acts; this only words it,
      // and only what waits on the user asks for them.
      const wait = phase.wait;
      const ci = ciLine(pr);
      const line = (...parts: (string | null)[]) => parts.filter(Boolean).join(" · ") || undefined;
      const group = phaseOf(overview, task.id)?.waits_on === "user" ? "attention" : "waiting";
      switch (wait.kind) {
        case "reply":
          return {
            ...step(
              wait.reviewers.length > 0
                ? `Aprobar respuesta a ${wait.reviewers.join(", ")}`
                : `Aprobar ${lower(REPLY_STEP.label)}`,
            ),
            detail: line(ci),
          };
        case "trouble": {
          const summary = prSummary(pr);
          return {
            group,
            tone: summary.tone,
            icon: summary.icon,
            label: `PR: ${lower(summary.label)}`,
            detail: line(ci),
          };
        }
        case "answer":
          return {
            ...waiting(`PR: responder a ${wait.reviewers.join(", ")}`),
            group,
            detail: line(ci),
          };
        case "review":
          return {
            ...waiting("PR: espera aprobación"),
            group,
            detail: line(
              wait.reviewers.length > 0
                ? `Revisión pedida a ${wait.reviewers.join(", ")}`
                : "GitHub espera una revisión",
              ci,
            ),
          };
        case "checks":
          return { ...waiting("PR aprobado · checks en curso"), group, detail: line(ci) };
        case "draft":
          return {
            ...attention("PR aprobado · borrador: márcalo como listo"),
            group,
            detail: line(ci),
          };
        case "ready":
          return { ...idle(`PR aprobado${then}`), detail: line(ci) };
        case "nobody":
          return {
            ...attention("PR: nadie tiene pedida la revisión"),
            group,
            detail: line(ci),
          };
      }
      return waiting("PR");
    }
    case "next_stage":
      return idle(`Siguiente: ${lower(stageLabel(phase.role))}`);
    case "next_step": {
      const label = lower(phaseStep(phase.rung, phase.stage).label);
      // Merged is not finished while a step of the ladder is left.
      const merged = (overview.prs ?? []).some(
        (pr) => pr.task_id === task.id && pr.state === "merged",
      );
      return idle(merged ? `PR fusionado · falta ${label}` : `Siguiente: ${label}`);
    }
  }
}

/** The CI of a PR's head in words, with its commit: "CI pasa en abc1234". */
export function ciLine(pr: DeliveryPr): string | null {
  if (!pr.ci) return null;
  const at = pr.head ? ` en ${pr.head.slice(0, 7)}` : "";
  const words = {
    passing: "CI pasa",
    failing: "CI falla",
    pending: "CI en curso",
    not_run: "CI sin ejecutar",
  } as const;
  return `${words[pr.ci]}${at}`;
}

/** The reviewers who must approve the PR and have not yet. */
export function missingApprovals(pr: DeliveryPr): string[] {
  const approved = (pr.approved_by ?? []).map((login) => login.toLowerCase());
  return (pr.reviewers ?? []).filter((login) => !approved.includes(login.toLowerCase()));
}

/** Who the task waits on, as its row says it after its state: "ti", "el
 *  coordinador", the reviewers by name… Nothing while an agent works on it or
 *  nobody has to act. */
export function waitsOnLabel(overview: DeliveryOverview, taskId: string): string | null {
  const phase = phaseOf(overview, taskId);
  switch (phase?.waits_on) {
    case "user":
      return "ti";
    case "coordinator":
      return "el coordinador";
    case "queue":
      return "su turno";
    case "reviewers": {
      const asked = phase.kind === "pr" && phase.wait.kind === "review" ? phase.wait.reviewers : [];
      return asked.length > 0 ? asked.join(", ") : "los revisores";
    }
    case "dependency":
      return "otra tarea";
    case "external":
      return phase.kind === "pr" ? "la CI" : "algo fuera de Tinto";
    default:
      return null;
  }
}

/** "espera a ti", "espera al coordinador": `a` + `el` contract. */
export function waitsOnPhrase(who: string): string {
  return who.startsWith("el ") ? `espera al ${who.slice(3)}` : `espera a ${who}`;
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
  if (pr.ci === "not_run") return { tone: "attention", icon: "alert", label: "CI sin ejecutar" };
  if (pr.conflicts) return { tone: "attention", icon: "alert", label: "Conflictos" };
  if (pr.review_decision === "changes_requested") {
    return { tone: "attention", icon: "alert", label: "Cambios pedidos" };
  }
  if (pr.checks.pending > 0) return { tone: "waiting", icon: "clock", label: "Checks en curso" };
  if (pr.draft) return { tone: "waiting", icon: "clock", label: "Borrador" };
  // Without required reviews GitHub gives no decision; an approval still counts.
  // And every reviewer chosen for it, until removed.
  const approved =
    (pr.review_decision === "approved" ||
      (!pr.review_decision && (pr.approved_by?.length ?? 0) > 0)) &&
    missingApprovals(pr).length === 0;
  if (approved) return { tone: "ok", icon: "check", label: "Aprobado" };
  return { tone: "waiting", icon: "clock", label: "Espera aprobación" };
}

/** The colour the PR's link takes in the task header, from what it needs:
 *  merged, failing (checks or closed), attention (news, conflicts, changes
 *  asked, checks running), approved, or neutral while it waits on review. */
export type PrMood = "merged" | "failing" | "attention" | "approved" | "neutral";

export function prMood(pr: DeliveryPr): PrMood {
  if (pr.state === "merged") return "merged";
  const { tone } = prSummary(pr);
  if (tone === "danger") return "failing";
  if (tone === "attention" || pr.checks.pending > 0) return "attention";
  if (tone === "ok") return "approved";
  return "neutral";
}

/** "En ejecución, In Review" (or with → between them) → the statuses in order. */
export function parseStatuses(text: string): string[] {
  return text
    .split(/[,→\n]/)
    .map((status) => status.trim())
    .filter(Boolean);
}

/** What an Alcance job has done so far, in plain language, from its log:
 *  the files it read grouped by service or package, its searches, and the
 *  commands it ran with their outcome. */
export interface ScopeActivity {
  files: { group: string; paths: string[] }[];
  searches: string[];
  commands: { command: string; outcome: string | null }[];
}

/** Folders whose children are each a service or package of their own. */
const CONTAINERS = new Set(["services", "packages", "apps", "crates", "libs", "modules"]);

function relative(path: string, worktree: string): string {
  const slashes = (text: string) => text.replace(/\\/g, "/");
  const root = slashes(worktree).replace(/\/+$/, "");
  const file = slashes(path);
  return file.toLowerCase().startsWith(`${root.toLowerCase()}/`)
    ? file.slice(root.length + 1)
    : file;
}

function groupOf(path: string): string {
  const parts = path.split("/");
  if (parts.length === 1) return "raíz";
  return CONTAINERS.has(parts[0]) && parts.length > 2 ? `${parts[0]}/${parts[1]}` : parts[0];
}

export function scopeActivity(
  entries: { kind: string; text: string }[],
  worktree: string,
): ScopeActivity {
  const groups = new Map<string, string[]>();
  const searches: string[] = [];
  const commands: ScopeActivity["commands"] = [];
  for (const entry of entries) {
    if (entry.kind !== "command") continue;
    const tool = /^(\w+): ([\s\S]*)$/.exec(entry.text.trim());
    const name = tool?.[1];
    const detail = tool?.[2].trim() ?? "";
    if (name === "Read" && detail) {
      const path = relative(detail, worktree);
      const group = groupOf(path);
      const paths = groups.get(group) ?? [];
      if (!paths.includes(path)) paths.push(path);
      groups.set(group, paths);
    } else if ((name === "Grep" || name === "Glob") && detail) {
      const search =
        name === "Grep"
          ? `Buscando «${detail}»`
          : `Buscando archivos «${relative(detail, worktree)}»`;
      if (!searches.includes(search)) searches.push(search);
    } else if (name !== "TodoWrite") {
      // A Bash call, or a command as Codex reports it: "cmd  → code".
      const text = name === "Bash" ? detail : entry.text.trim();
      const exit = /^([\s\S]*?)\s+→ (-?\d+)$/.exec(text);
      commands.push(
        exit
          ? {
              command: exit[1],
              outcome: exit[2] === "0" ? "terminó bien" : `falló (código ${exit[2]})`,
            }
          : { command: text, outcome: null },
      );
    }
  }
  return {
    files: [...groups].map(([group, paths]) => ({ group, paths })),
    searches,
    commands,
  };
}

/** Why a PR step cannot be approved yet, or `null` when it can. It goes
 *  without a reviewer only when the repo allows it ("PR sin revisor") or
 *  GitHub itself said nobody else can review: never while either is still
 *  being read, nor on a list it could not read or only remembered. */
export function reviewerBlock(state: {
  choosesReviewers: boolean;
  chosen: number;
  settingsRead: boolean;
  reviewerOptional: boolean;
  candidates: string[] | null;
  fresh: boolean;
  failed: boolean;
}): string | null {
  if (!state.choosesReviewers || state.chosen > 0) return null;
  if (!state.settingsRead) return "Leyendo los ajustes del repositorio…";
  if (state.reviewerOptional) return null;
  if (state.candidates !== null && state.candidates.length > 0) return "Elige al menos un revisor.";
  if (state.fresh) return null;
  return state.failed
    ? "No se pudo leer quién puede revisar: reinténtalo."
    : "Leyendo quién puede revisar en GitHub…";
}
