// How a Delivery task reads in the view. Where a task is (its phase, who it
// waits on, the stage due next) is decided once, by Tinto's backend
// (src-tauri/src/delivery/phase.rs); these functions only word it, and word
// each job and delivery step.

import type {
  DeliveryApproval,
  DeliveryBlocker,
  DeliveryDecision,
  DeliveryJob,
  DeliveryJobVerdict,
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

/** One thing a task waits on the user for. */
export interface TaskAsk {
  /** What «te necesita» goes to: the suffix of its card's id. */
  id: string;
  label: string;
}

/** What a task waits on the user for, in the order to attend it: the first
 *  is the task's one primary action, the rest follow it. */
export function taskAsks(pending: {
  decisions: number;
  /** The steps waiting for approval, replies to reviewers aside. */
  approvals: DeliveryApproval[];
  replies: number;
  qaProblems: number;
  settingsProposals: number;
  broken: boolean;
  pr: DeliveryPr | null | undefined;
}): TaskAsk[] {
  const count = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);
  return [
    pending.decisions > 0 && {
      id: "decisions",
      label: count(pending.decisions, "decisión", "decisiones"),
    },
    ...pending.approvals.map((approval) => ({
      id: approval.id,
      label: `aprobar ${stepName(stepOf(approval), pending.pr)}`,
    })),
    pending.replies > 0 && {
      id: "replies",
      label: count(pending.replies, "respuesta al revisor", "respuestas a los revisores"),
    },
    pending.qaProblems > 0 && {
      id: "qa",
      label: count(pending.qaProblems, "problema de la QA", "problemas de la QA"),
    },
    pending.settingsProposals > 0 && {
      id: "settings",
      label: count(pending.settingsProposals, "cambio de ajustes", "cambios de ajustes"),
    },
    pending.broken && { id: "broken", label: "una etapa rota" },
  ].filter((ask): ask is TaskAsk => !!ask);
}

/** The task's latest stage, when it broke and nothing runs again: what the
 *  task's detail shows to retry. */
export function brokenJobOf(overview: DeliveryOverview, taskId: string): DeliveryJob | null {
  const jobs = jobsOf(overview, taskId);
  const latest = [...jobs]
    .reverse()
    .find((job) => job.agent !== "shell" && job.result_state !== "stale" && !job.undone_at_ms);
  return latest && !jobs.some(isOpen) && verdictsOf(overview, taskId)[latest.id]?.broken
    ? latest
    : null;
}

/** What the task waits on the user for, from the overview: the asks its
 *  detail shows and the queue lists. */
export function taskAsksOf(overview: DeliveryOverview, task: DeliveryTask): TaskAsk[] {
  const pending = overview.approvals.filter(
    (approval) => approval.task_id === task.id && approval.status === "pending",
  );
  return taskAsks({
    decisions: pendingDecisions(overview, task.id),
    approvals: pending.filter((approval) => approval.rung !== "pr_reply"),
    replies: pending.filter((approval) => approval.rung === "pr_reply").length,
    qaProblems: qaProblemsToDecide(overview, task.id),
    settingsProposals: pendingSettingsProposals(overview, task).length,
    broken: brokenJobOf(overview, task.id) !== null,
    pr: (overview.prs ?? []).find((pr) => pr.task_id === task.id),
  });
}

export function files(count: number): string {
  return count === 1 ? "1 archivo" : `${count} archivos`;
}

/** What an open step's line says of it: its commits, or its title. */
export function approvalSummary(approval: DeliveryApproval): string {
  const commits = approval.commits ?? [];
  return commits.length > 0
    ? `${commits.length === 1 ? "1 commit" : `${commits.length} commits`} · ${files(commits.reduce((sum, commit) => sum + commit.paths.length, 0))}`
    : approval.title;
}

/** What the button that approves a step says it does. */
export function approveLabel(
  approval: DeliveryApproval,
  pr: DeliveryPr | null | undefined,
): string {
  switch (approval.rung) {
    case "commit":
      return "Aprobar y hacer commit";
    case "sync":
      return "Aprobar y sincronizar";
    case "push":
      return "Aprobar y publicar";
    case "merge":
      return "Aprobar y fusionar";
    case "pr":
      return pr?.state === "open"
        ? `Aprobar y actualizar el PR #${pr.number}`
        : "Aprobar y abrir el PR";
    default:
      return "Aprobar";
  }
}

/** A step approved from its line, with nothing to choose or to read first:
 *  the PR asks for reviewers, the merge shows its checks, a step Tinto noticed
 *  something about says it, and a text the user `edited` is approved where
 *  they edited it. */
export function approvesInOneClick(approval: DeliveryApproval, edited: boolean): boolean {
  return (
    approval.status === "pending" &&
    ["commit", "sync", "push", "jira"].includes(approval.rung) &&
    !approval.notice &&
    !edited
  );
}

/** A key that tells the two Jira steps apart. */
export function stepKey(step: DeliveryStep): string {
  return step.stage ? `${step.rung}-${step.stage}` : step.rung;
}

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

/** The list's groups: what is under way, whoever it waits for (each row says
 *  who), and what was delivered. What needs the user is the queue above them. */
export type ListGroup = "ongoing" | "delivered";

export const GROUPS: { group: ListGroup; label: string }[] = [
  { group: "ongoing", label: "En curso" },
  { group: "delivered", label: "Entregadas" },
];

/** A status's order in the list: within "En curso", what needs the user comes
 *  first, then what runs. */
const STATUS_ORDER: StatusGroup[] = ["attention", "running", "waiting", "idle", "delivered"];

export function listGroup(group: StatusGroup): ListGroup {
  return group === "delivered" ? group : "ongoing";
}

/** The tasks that came to need the user since the previous groups: only a
 *  task that was there in another group, never one seen for the first time. */
export function arrivedInAttention(
  before: ReadonlyMap<string, StatusGroup>,
  after: ReadonlyMap<string, StatusGroup>,
): string[] {
  return [...after]
    .filter(([id, group]) => {
      const was = before.get(id);
      return group === "attention" && was !== undefined && was !== "attention";
    })
    .map(([id]) => id);
}

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

/** What broke on a stage, said one way wherever it shows (the list, the queue,
 *  the detail's "Ahora" and its stop box): "Bloqueada en QA · intento 2". */
export function brokePhrase(blocked: boolean, role: string, attempt: number): string {
  return `${blocked ? "Bloqueada en" : "Falló"} ${lower(stageLabel(role))} · intento ${attempt}`;
}

/** A broken job's phrase: one that finished and broke was blocked, as Tinto
 *  derives it; one that failed or stopped, failed. */
export function jobBrokePhrase(job: DeliveryJob): string {
  return brokePhrase(job.status === "finished", job.role, job.attempt);
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

/** Not superseded: neither stale nor undone. */
function isLive(job: DeliveryJob): boolean {
  return job.result_state !== "stale" && !job.undone_at_ms;
}

function isCurrent(job: DeliveryJob): boolean {
  return isLive(job) && job.result_state !== "invalid";
}

function stageOrder(role: string): number {
  return STAGES.findIndex((stage) => stage.role === role);
}

/** The task's jobs, oldest first (the overview keeps them in creation order). */
export function jobsOf(overview: DeliveryOverview, taskId: string): DeliveryJob[] {
  return overview.jobs.filter((job) => job.task_id === taskId);
}

/** What Tinto says of each of the task's jobs, by id (broken, passed, answered, superseded). */
export function verdictsOf(
  overview: DeliveryOverview,
  taskId: string,
): Record<string, DeliveryJobVerdict> {
  return phaseOf(overview, taskId)?.verdicts ?? {};
}

/** Millionths of a dollar as the view shows them: `$0.42`. */
export function dollars(microUsd: number): string {
  return `$${(microUsd / 1_000_000).toFixed(2)}`;
}

/** What the jobs cost together, from those whose agent reported it: null when
 *  none did; `partial` when an agent job that started has no cost (Codex never
 *  reports one, a running job not yet). Checks (shell) cost nothing. */
export function costOf(jobs: DeliveryJob[]): { label: string; partial: boolean } | null {
  let total: number | null = null;
  let partial = false;
  for (const job of jobs) {
    const cost = job.usage?.cost_micro_usd;
    if (cost != null) total = (total ?? 0) + cost;
    else if (job.agent !== "shell" && job.started_at_ms !== null) partial = true;
  }
  if (total === null) return null;
  return { label: `${partial ? "≥ " : ""}${dollars(total)}`, partial };
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
 *  `jobs` a resumed pause says so, and with Tinto's `verdicts` a pass later
 *  work left behind, or findings a later stage answered. */
export function attemptOutcome(
  job: DeliveryJob,
  jobs: DeliveryJob[] = [],
  verdicts: Record<string, DeliveryJobVerdict> = {},
): { tone: Tone; label: string } {
  const verdict = verdicts[job.id] ?? {};
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
  if (job.resolved_at_ms && verdict.passed) {
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
  // Tinto's commands end without a result: finished is passed (a failure is `failed`).
  if (!result) return { tone: "ok", label: job.role === "checks" ? "Pasa" : "Terminado" };
  // Alcance's findings are its questions for the user: a normal outcome.
  if (job.role === SCOPE_ROLE && result.status !== "blocked") {
    const count = result.status === "findings" ? result.findings.length : 0;
    return { tone: "ok", label: count > 0 ? `Listo · ${questions(count)}` : "Listo" };
  }
  if (result.status === "pass") {
    return verdict.superseded
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
    // Once a later stage answered them they are settled, not a warning.
    return verdict.answered
      ? { tone: "ok", label: `${label} · atendidos` }
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

/** The first step of its ladder not done, as Tinto says; null once every
 *  step is, or until the overview has its phase. */
export function nextStepOf(overview: DeliveryOverview, taskId: string): DeliveryStep | null {
  const next = phaseOf(overview, taskId)?.next_step;
  return next ? phaseStep(next.rung, next.stage) : null;
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

/** A row of the task list: a task, or a split issue holding its packages in order. */
export type TaskListEntry =
  | { kind: "task"; task: DeliveryTask }
  | { kind: "issue"; id: string; key: string; packages: DeliveryTask[] };

/** What selects a split issue's row in its repo's list. */
export function issueRowId(task: DeliveryTask): string {
  return `issue:${task.key.toLowerCase()}`;
}

/** The task list by group. A split issue is one entry, placed by its package
 *  that needs the most (the first status among theirs). */
export function taskListGroups(
  tasks: DeliveryTask[],
  groupOf: (task: DeliveryTask) => StatusGroup,
): { group: ListGroup; label: string; entries: TaskListEntry[] }[] {
  const rank = (group: StatusGroup) => STATUS_ORDER.indexOf(group);
  const issues = new Map<string, DeliveryTask[]>();
  for (const task of tasks) {
    if (!task.package) continue;
    issues.set(issueRowId(task), [...(issues.get(issueRowId(task)) ?? []), task]);
  }
  const entryRank = (task: DeliveryTask) =>
    task.package
      ? Math.min(...issues.get(issueRowId(task))!.map((one) => rank(groupOf(one))))
      : rank(groupOf(task));
  const byRank = [...tasks].sort((a, b) => entryRank(a) - entryRank(b));
  return GROUPS.map((group) => {
    const entries: TaskListEntry[] = [];
    for (const task of byRank) {
      if (listGroup(STATUS_ORDER[entryRank(task)]) !== group.group) continue;
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

/** The tasks a search finds, by issue key or title, or by PR number (`#412`
 *  or `412`, which also finds the issue whose key ends in it): the first
 *  `limit`, in the order given. */
export function searchTasks(
  tasks: DeliveryTask[],
  prs: DeliveryPr[],
  query: string,
  limit = 8,
): DeliveryTask[] {
  const wanted = query.trim().toLowerCase();
  if (!wanted) return [];
  const number = /^#?\d+$/.test(wanted) ? wanted.replace("#", "") : null;
  const prOf = (task: DeliveryTask) => prs.find((pr) => pr.task_id === task.id);
  return tasks
    .filter((task) => {
      if (number && String(prOf(task)?.number ?? "").startsWith(number)) return true;
      if (wanted.startsWith("#")) return false;
      const title = task.package?.title || task.title;
      return task.key.toLowerCase().includes(wanted) || title.toLowerCase().includes(wanted);
    })
    .slice(0, limit);
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

/** How a stage reads in the track while the task is in a round, for an
 *  attempt from before it: the tests still stand, implementation and what
 *  follows it run again. null when its own outcome applies (an attempt of
 *  this round, no round, another kind of stage). */
export function roundStage(
  role: string,
  job: DeliveryJob | undefined,
  verdicts: Record<string, DeliveryJobVerdict>,
  round: DeliveryCurrentRound | null,
): { tone: Tone | "todo"; label: string } | null {
  if (!round || (job && job.created_at_ms >= round.opened_at_ms)) return null;
  // Alcance reads as it went; the tests still stand; what follows runs again.
  const order = stageOrder(role);
  const tests = stageOrder("tests");
  if (order > tests) return { tone: "todo", label: "Se repite en esta ronda" };
  const verdict = job ? verdicts[job.id] : undefined;
  if (order === tests && (verdict?.passed || verdict?.answered) && !verdict?.superseded) {
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

/** How a delivery step reads from its latest approval. */
export function rungStep(approval: DeliveryApproval | undefined): {
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
      if (approval.conflicts?.length)
        return {
          tone: "attention",
          icon: "alert",
          label: `Conflictos en ${approval.conflicts.join(", ")}`,
          title: approval.outcome ?? undefined,
        };
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

/** Where a stage stands on the task's line, said by the form of its station:
 *  passed, running, never done yet, done before and to be done again in this
 *  round, waiting on the user, broken, reached with a problem nobody has to
 *  answer here (conflicts, changes asked, findings still open), or skipped:
 *  never run and not to run, the task being past it. */
export type Station =
  "passed" | "running" | "pending" | "repeats" | "yours" | "broke" | "attention" | "skipped";

/** A stage of the track (Agentes): its latest attempt and how it reads. */
export interface AgentStop {
  role: string;
  job: DeliveryJob | undefined;
  /** It reads as the open round says, not as its own attempt went. */
  inRound: boolean;
  tone: Tone | "todo";
  label: string;
  title?: string;
  station: Station;
  /** The rail from the station before to this one is done. */
  joined: boolean;
}

/** A step of the track (Entrega): its latest approval and how it reads. */
export interface StepStop {
  step: DeliveryStep;
  approval: DeliveryApproval | undefined;
  tone: Tone | "todo";
  icon: StatusIcon;
  label: string;
  title?: string;
  station: Station;
  /** The rail from the station before to this one is done. */
  joined: boolean;
}

export interface TaskTrack {
  agents: AgentStop[];
  steps: StepStop[];
  /** Tinto's own command running between stages (its checks, a setup), as
   *  the "Ahora" line names it: the stage it runs after. */
  between: { after: string; label: string } | null;
}

/** A group's stations as its line reads them, given when each was reached
 *  (`at`, null if never): a problem a later passed station got past (unless it
 *  is the one due next), or any on a delivered task, is passed; a station never reached, not the one due next,
 *  that the task is past (or delivered) is skipped; and whether the rail into
 *  each one is done: passed at both ends, skipped ones in between. */
function settleLine<
  T extends {
    station: Station;
    tone: Tone | "todo";
    icon?: StatusIcon;
    label: string;
    title?: string;
  },
>(
  stops: T[],
  at: (stop: T) => number | null,
  isNext: (stop: T) => boolean,
  delivered: boolean,
): (T & { joined: boolean })[] {
  // A problem left behind is passed, drawn as passed in every theme (tone and
  // icon), and its words stop naming the problem: what it was stays in its title.
  const past = (stop: T): T =>
    stop.station === "attention" || stop.station === "broke"
      ? {
          ...stop,
          station: "passed",
          tone: "ok",
          ...(stop.icon && { icon: "check" }),
          label: "Resuelto",
          title: stop.title ?? stop.label,
        }
      : { ...stop, station: "passed" };
  const settled = stops.map((stop, index): T => {
    if (stop.station === "skipped") return stop;
    const reached = at(stop);
    const later = stops.slice(index + 1);
    const neverRan = stop.station === "pending" && reached === null;
    if (neverRan && !isNext(stop) && (delivered || later.some((next) => at(next) !== null))) {
      return { ...stop, station: "skipped" };
    }
    if (delivered) return past(stop);
    // The stop due next is still owed, whatever passed after it: a stage
    // Tinto counts as answered says so in its own outcome (findings answered).
    const gotPast =
      !isNext(stop) &&
      reached !== null &&
      later.some((next) => next.station === "passed" && (at(next) ?? -Infinity) > reached);
    return (stop.station === "attention" || stop.station === "broke") && gotPast
      ? past(stop)
      : stop;
  });
  const passedAround = (from: number, step: 1 | -1) => {
    for (let index = from; index >= 0 && index < settled.length; index += step) {
      if (settled[index].station !== "skipped") return settled[index].station === "passed";
    }
    return false;
  };
  return settled.map((stop, index) => ({
    ...stop,
    joined: index > 0 && passedAround(index - 1, -1) && passedAround(index, 1),
  }));
}

/** The station a tone draws. A fine note (answered findings, a wait) says
 *  nothing of progress: `done` says whether its stage was reached. */
function stationOf(tone: Tone | "todo", done: boolean): Station {
  switch (tone) {
    case "ok":
      return "passed";
    case "live":
      return "running";
    case "step":
      return "yours";
    case "danger":
      return "broke";
    default:
      return done ? "passed" : "pending";
  }
}

/** The task's stages and delivery steps as its track shows them, in order:
 *  the one reading both the stage strip and the list row's line draw. */
export function taskTrack(overview: DeliveryOverview, task: DeliveryTask): TaskTrack {
  const jobs = jobsOf(overview, task.id);
  const verdicts = verdictsOf(overview, task.id);
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const decisions = (overview.decisions ?? []).filter((decision) => decision.task_id === task.id);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  const next = nextStageOf(overview, task.id);
  const round = roundOf(overview, task.id);
  const agents = rolesOf(jobs).map((role): AgentStop => {
    const job = latestOfRole(jobs, role);
    const inRound = roundStage(role, job, verdicts, round);
    const scope = role === SCOPE_ROLE ? scopeOutcome(task, job, jobs, decisions) : null;
    const outcome: { tone: Tone | "todo"; label: string; title?: string } = inRound
      ? { ...inRound, label: role === next ? "Siguiente" : inRound.label }
      : (scope ??
        (job
          ? attemptOutcome(job, jobs, verdicts)
          : { tone: "todo", label: role === next ? "Siguiente" : "—" }));
    // Reached, under a fine note: findings answered.
    const done = !!job && !!verdicts[job.id]?.answered;
    return {
      role,
      job,
      inRound: !!inRound,
      ...outcome,
      station:
        inRound?.tone === "todo"
          ? "repeats"
          : role === SCOPE_ROLE && task.scope_skip && !(job && isOpen(job))
            ? "skipped"
            : // Findings still open: reached, with a problem.
              outcome.tone === "attention"
              ? "attention"
              : stationOf(outcome.tone, done),
      joined: false,
    };
  });
  const steps = trackSteps(stepsOf(overview, task.id), approvals).map((step): StepStop => {
    const live = step.rung === "pr" && pr ? prSummary(pr) : null;
    const approval = latestStep(approvals, step);
    const unlinked = step.rung === "pr" && !pr && approval?.status === "executed";
    const before = rungStep(approval);
    const repeats = roundRung(step, approval, round);
    const shown = repeats
      ? {
          tone: "todo" as const,
          icon: "ring" as const,
          label: "Se repetirá",
          // What it was before the round, for who asks.
          title: `Se repetirá en esta ronda${approval ? `. Antes: ${before.title ?? before.label}` : ""}`,
        }
      : live
        ? { ...live, title: `${pr?.repo}#${pr?.number}` }
        : unlinked
          ? {
              // Done, and nothing the user has to give: neutral.
              tone: "waiting" as const,
              icon: "alert" as const,
              label: "Hecho · sin PR vinculado",
              title: approval.outcome ?? undefined,
            }
          : before;
    // A followed PR is a step taken: broken once closed or failing its checks,
    // with a problem while it has conflicts, changes asked or a CI that did
    // not run, fine otherwise (news to read included). Done without a link is
    // a step taken too; executed with conflicts, taken with a problem;
    // approved, the coordinator is taking it.
    const station: Station = repeats
      ? "repeats"
      : pr && live
        ? pr.state === "closed" || pr.checks.failing > 0
          ? "broke"
          : pr.state !== "merged" &&
              (pr.conflicts || pr.review_decision === "changes_requested" || pr.ci === "not_run")
            ? "attention"
            : "passed"
        : approval?.status === "approved"
          ? "running"
          : shown.tone === "attention"
            ? "attention"
            : stationOf(shown.tone, unlinked);
    return { step, approval, ...shown, station, joined: false };
  });
  const phase = phaseOf(overview, task.id);
  const delivered = phase?.kind === "delivered";
  const nextStep = nextStepOf(overview, task.id);
  // A followed PR's problem, or its merge, is now: nothing before gets past it.
  const stepAt = (stop: StepStop) =>
    stop.step.rung === "pr" && pr && (stop.station !== "passed" || pr.state === "merged")
      ? Infinity
      : (stop.approval?.requested_at_ms ?? null);
  const line = {
    agents: settleLine(
      agents,
      (stop) => stop.job?.created_at_ms ?? null,
      (stop) => stop.role === next,
      delivered,
    ),
    steps: settleLine(
      steps,
      stepAt,
      (stop) => !!nextStep && stepKey(stop.step) === stepKey(nextStep),
      delivered,
    ),
  };
  // Tinto's own command running (its checks, a setup) is no station: it runs
  // after the stage that ran last before it, unless a station is running.
  const shell =
    phase?.kind === "running"
      ? jobs.find((candidate) => candidate.id === phase.job_id && candidate.agent === "shell")
      : undefined;
  const busy = [...line.agents, ...line.steps].some((stop) => stop.station === "running");
  const after =
    shell && !busy
      ? line.agents
          .filter((stop) => stop.job && stop.job.created_at_ms <= shell.created_at_ms)
          .reduce<AgentStop | null>(
            (last, stop) =>
              !last || stop.job!.created_at_ms > last.job!.created_at_ms ? stop : last,
            null,
          )
      : null;
  return {
    ...line,
    between:
      shell && after ? { after: after.role, label: `${stageLabel(shell.role)} en marcha` } : null,
  };
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
      // Blocked by a stage that broke: said as that stage's break is.
      const broke = brokenJobOf(overview, task.id);
      return {
        group: "waiting",
        tone: "blocked",
        icon: "lock",
        label: broke
          ? jobBrokePhrase(broke)
          : open.length > 1
            ? `Bloqueada · ${open.length} bloqueos`
            : "Bloqueada",
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
      return danger(brokePhrase(phase.blocked, phase.role, phase.attempt), "x");
    case "delivered":
      return { group: "delivered", tone: "ok", icon: "check", label: "Entregada" };
    case "waits_merge_commit":
      return {
        ...waiting("Espera el commit del merge"),
        detail:
          phase.notice ??
          (phase.waiting_round === null
            ? "Hay un merge de la base en curso: se confirma antes de cualquier otra etapa."
            : `La ronda ${phase.waiting_round} corre encima del commit del merge, en commits aparte.`),
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
      const next = nextStepOf(overview, task.id);
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

/** "te espera", "espera al coordinador" (`a` + `el` contract); `null` when
 *  the row's `label` already says it. */
export function waitsOnPhrase(who: string, label = ""): string | null {
  const phrase =
    who === "ti"
      ? "te espera"
      : who === "su turno"
        ? "espera su turno"
        : who.startsWith("el ")
          ? `espera al ${who.slice(3)}`
          : `espera a ${who}`;
  return label.includes(phrase) ? null : phrase;
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
