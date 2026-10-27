import { describe, it, expect } from "vitest";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryQaProblem,
  DeliveryPhase,
  DeliveryPr,
  DeliveryRound,
  DeliveryTask,
  DeliveryTaskPhase,
} from "../../delivery/types";
import {
  attemptOutcome,
  batchName,
  historyKey,
  isDelivered,
  entryTasks,
  issueProgress,
  issueRowId,
  isSuperseded,
  packageState,
  taskListGroups,
  lastHandoff,
  latestOfRole,
  nextStageOf,
  nextStep,
  nowLine,
  prSummary,
  waitsOnLabel,
  roundHistory,
  roundRung,
  roundStage,
  stageLabel,
  stepsOf,
  taskLabel,
  taskStatus,
  type TaskStatus,
} from "./taskStatus";

const task: DeliveryTask = {
  id: "t1",
  run_id: null,
  repo: "C:\\work\\agentos",
  distro: null,
  key: "AGOS-501",
  title: "",
  worktree: "C:\\work\\agentos-wt\\AGOS-501",
  branch: "delivery/agos-501",
  base_ref: "develop",
  base_commit: "0123456789",
  state: "tests",
  contract_version: 1,
  created_at_ms: 1,
  updated_at_ms: 1,
};

let created = 0;
function job(over: Partial<DeliveryJob> = {}): DeliveryJob {
  created += 1;
  return {
    id: `j${created}`,
    task_id: "t1",
    role: "tests",
    attempt: 1,
    agent: "codex",
    model: null,
    access: "workspace",
    prompt: "",
    contract_version: 1,
    writes: true,
    lease: null,
    timeout_minutes: 45,
    status: "finished",
    created_at_ms: created,
    started_at_ms: 1_000,
    ended_at_ms: 61_000,
    pid: null,
    provider_session_id: null,
    exit_code: 0,
    error: null,
    start_candidate: null,
    end_candidate: null,
    changes: [],
    result: {
      status: "pass",
      summary: "",
      changed_paths: [],
      checks: [],
      findings: [],
      handoff: "",
    },
    result_state: "accepted",
    result_note: null,
    undone_at_ms: null,
    ...over,
  };
}

function approval(over: Partial<DeliveryApproval> = {}): DeliveryApproval {
  return {
    id: "a1",
    task_id: "t1",
    rung: "commit",
    title: "msg",
    body: "",
    status: "pending",
    requested_by: "coord",
    requested_at_ms: 1,
    decided_at_ms: null,
    executed_at_ms: null,
    outcome: null,
    ...over,
  };
}

function overview(over: Partial<DeliveryOverview> = {}): DeliveryOverview {
  return {
    runs: [],
    tasks: [task],
    jobs: [],
    leases: [],
    approvals: [],
    decisions: [],
    settings: { capacity: 3 },
    coordinator: null,
    ...over,
  };
}

/** The status of task t1 when Tinto says it is in `phase`. */
const status = (
  phase: DeliveryPhase | null,
  over: Partial<DeliveryOverview> = {},
  now = 76_000,
): TaskStatus =>
  taskStatus(
    task,
    overview({
      phases: phase ? { t1: { ...phase, waits_on: "user", next_stage: null } } : {},
      ...over,
    }),
    now,
  );

// Where a task is, who it waits on and what comes next are decided by the
// backend (src-tauri/src/delivery/phase.rs, with its own tests); the view
// only words each phase.
describe("taskStatus words the phase Tinto derives", () => {
  it("asks the user for decisions, approvals, a changed step or a setting", () => {
    expect(status({ kind: "decisions", count: 1 })).toEqual({
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: "1 decisión pendiente",
    });
    expect(status({ kind: "decisions", count: 2 }).label).toBe("2 decisiones pendientes");
    // Delivery steps get their own colour, apart from blockers.
    expect(status({ kind: "approval", rung: "pr", stage: null })).toMatchObject({
      group: "attention",
      tone: "step",
      label: "Aprobar PR",
    });
    // An approved step is the coordinator's to run and report: the task waits.
    expect(status({ kind: "record", rung: "jira", stage: "after_pr" })).toMatchObject({
      group: "waiting",
      label: "Esperando al coordinador: Jira (PR)",
    });
    expect(status({ kind: "record", rung: "jira", stage: "after_merge" }).label).toBe(
      "Esperando al coordinador: Jira (merge)",
    );
    expect(status({ kind: "drifted", rung: "push", stage: null })).toMatchObject({
      group: "attention",
      label: "Push: ya no coincide",
    });
    expect(status({ kind: "settings_proposal" }).label).toBe("Aprobar cambio de ajustes");
  });

  it("shows the QA problems waiting on the user, without hiding what else does", () => {
    const problem = (status: DeliveryQaProblem["status"], number: number): DeliveryQaProblem => ({
      id: `p${number}`,
      task_id: "t1",
      job_id: "qa-1",
      number,
      label: `P${number}`,
      change: "",
      place: "",
      text: "The toast covers Retry.",
      suggestions: [],
      status,
      notes: [],
      created_at_ms: 1,
      updated_at_ms: 1,
    });
    const qa_problems = [problem("open", 1), problem("still_failing", 2), problem("sent", 3)];
    expect(status({ kind: "next_stage", role: "implementation" }, { qa_problems })).toEqual({
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: "Decidir 2 problemas de QA",
      detail: "Siguiente: implementación",
    });
    // Something else needing the user keeps its place; the problems join it.
    expect(status({ kind: "decisions", count: 1 }, { qa_problems })).toMatchObject({
      label: "1 decisión pendiente",
      detail: "Decidir 2 problemas de QA",
    });
    expect(
      status({ kind: "next_stage", role: "qa" }, { qa_problems: [problem("sent", 3)] }).label,
    ).toBe("Siguiente: QA");
  });

  it("names what a blocked task waits for, and waits instead of idling", () => {
    const blocked = {
      ...task,
      blockers: [
        {
          id: "b1",
          type: "pr" as const,
          url: "https://github.com/acme/api/pull/305",
          until: "merged" as const,
          status: "abierto",
          cleared_at_ms: null,
        },
      ],
    };
    const phases = {
      t1: { kind: "blocked" as const, open: 1, waits_on: "external" as const, next_stage: null },
    };
    expect(taskStatus(blocked, overview({ phases }), 0)).toMatchObject({
      group: "waiting",
      tone: "blocked",
      icon: "lock",
      label: "Bloqueada",
      detail: "Espera al PR acme/api#305",
    });
    const note = {
      ...blocked,
      blockers: [
        ...blocked.blockers,
        {
          id: "b2",
          type: "note" as const,
          text: "Docker Desktop está apagado",
          status: "",
          cleared_at_ms: null,
        },
      ],
    };
    expect(taskStatus(note, overview({ phases }), 0)).toMatchObject({
      label: "Bloqueada · 2 bloqueos",
      detail: "Espera al PR acme/api#305 · Docker Desktop está apagado",
    });
  });

  it("shows a running stage with its clock, and when its agent went quiet", () => {
    const running = job({
      id: "run",
      role: "implementation",
      status: "started",
      ended_at_ms: null,
    });
    const phase: DeliveryPhase = { kind: "running", job_id: "run", role: "implementation" };
    expect(status(phase, { jobs: [running] })).toEqual({
      group: "running",
      tone: "live",
      icon: "live",
      label: "Implementando · 1:15",
    });
    const quiet = { ...running, activity_at_ms: 1_000 };
    expect(status(phase, { jobs: [quiet] }).label).not.toContain("sin actividad");
    expect(status(phase, { jobs: [quiet] }, 421_000).label).toContain("sin actividad hace 7 min");
    // A long command is the agent working, not a quiet agent.
    const busy = { ...quiet, current_command: "npm run lint", current_command_at_ms: 1_000 };
    expect(status(phase, { jobs: [busy] }, 421_000).label).not.toContain("sin actividad");
  });

  it("says what a task waits for: a pause, the batch, a queue or a quarantine", () => {
    expect(status({ kind: "paused", job_id: "j", role: "tests" })).toMatchObject({
      group: "waiting",
      label: "Tests en pausa",
    });
    expect(status({ kind: "batch_paused", job_id: "j", role: "qa" }).label).toBe("Lote en pausa");
    expect(
      status({ kind: "queued", job_id: "j", role: "qa", lease: "qa", position: 1 }).label,
    ).toBe("Esperando QA · 1.º en la cola");
    expect(
      status({ kind: "queued", job_id: "j", role: "tests", lease: null, position: 2 }).label,
    ).toBe("En cola · 2.º");
    // Only the user can release it: it is theirs to act on.
    expect(status({ kind: "waits_quarantine", job_id: "j", lease: "qa" })).toMatchObject({
      group: "attention",
      tone: "danger",
      label: "QA bloqueada · libera el recurso",
    });
    expect(status({ kind: "quarantined", lease: "qa" }).label).toBe("QA en cuarentena");
  });

  it("says why a queued job has not started, and when what it waits for looks stuck", () => {
    const reason = "espera a que terminen las comprobaciones (trabajo c1) de esta tarea";
    const queued: DeliveryPhase = {
      kind: "queued",
      job_id: "j",
      role: "implementation",
      lease: null,
      position: 0,
      reason,
      since_ms: 1,
    };
    expect(status(queued)).toMatchObject({ label: "En cola", detail: reason });
    const warning = "el trabajo c1 lleva 50 min, más que su límite de 30 min";
    const stuck = taskStatus(
      task,
      overview({
        phases: { t1: { ...queued, waits_on: "queue", next_stage: null, warning } },
      }),
      0,
    );
    expect(stuck.detail).toBe(`${reason} · ${warning}`);
    // The checks run: the warning still shows on the task.
    const running = taskStatus(
      task,
      overview({
        phases: {
          t1: {
            kind: "running",
            job_id: "c1",
            role: "checks",
            waits_on: "agent",
            next_stage: null,
            warning,
          },
        },
      }),
      0,
    );
    expect(running.detail).toBe(warning);
  });

  it("keeps a task whose blocker cleared on the coordinator, naming who cleared it", () => {
    const unblocked = (by: string): DeliveryPhase => ({
      kind: "unblocked",
      cleared: "las bases de pruebas",
      by,
      at_ms: 1,
    });
    expect(status(unblocked("user"))).toMatchObject({
      group: "idle",
      label: "Desbloqueada · espera al coordinador",
      detail: "Quitaste: las bases de pruebas",
    });
    expect(status(unblocked("tinto")).detail).toBe("Se resolvió: las bases de pruebas");
  });

  it("flags a failed setup command and a job waiting on the environment", () => {
    expect(status({ kind: "setup_failed", job_id: "j", command: "npm ci" })).toEqual({
      group: "attention",
      tone: "danger",
      icon: "x",
      label: "Falló la preparación: npm ci",
    });
    expect(
      status({ kind: "waits_environment", job_id: "j", role: "qa", problem: "Docker: no" }),
    ).toMatchObject({ group: "attention", label: "QA espera al entorno" });
  });

  it("flags a broken stage and names what comes next", () => {
    expect(
      status({ kind: "broken", job_id: "j", role: "implementation", attempt: 2, blocked: false }),
    ).toEqual({
      group: "attention",
      tone: "danger",
      icon: "x",
      label: "Falló implementación · intento 2",
    });
    expect(
      status({ kind: "broken", job_id: "j", role: "qa", attempt: 1, blocked: true }).label,
    ).toBe("Bloqueada en QA · intento 1");
    expect(status({ kind: "next_stage", role: "implementation" }).label).toBe(
      "Siguiente: implementación",
    );
    expect(status({ kind: "next_step", rung: "jira", stage: "after_merge" }).label).toBe(
      "Siguiente: Jira (merge)",
    );
    expect(status({ kind: "delivered" })).toMatchObject({
      group: "delivered",
      tone: "ok",
      label: "Entregada",
    });
    expect(status(null).label).toBe("Sin actividad");
  });

  it("takes the next stage from the phase", () => {
    const view = overview({
      phases: {
        t1: { kind: "running", job_id: "j", role: "review", waits_on: "agent", next_stage: null },
      },
    });
    expect(nextStageOf(view, "t1")).toBeNull();
    const idle = overview({
      phases: { t1: { kind: "next_stage", role: "qa", waits_on: "coordinator", next_stage: "qa" } },
    });
    expect(nextStageOf(idle, "t1")).toBe("qa");
  });
});

describe("a followed PR", () => {
  const pr = (over: Partial<DeliveryPr> = {}): DeliveryPr => ({
    ...{
      task_id: "t1",
      url: "https://github.com/o/r/pull/5",
      repo: "o/r",
      number: 5,
      state: "open",
      draft: false,
      conflicts: false,
      review_decision: "review_required",
      checks: { passing: 2, failing: 0, pending: 0 },
      new_reviews: 0,
      new_comments: 0,
      new_authors: [],
      latest_activity: null,
      seen_until: null,
      checked_at_ms: 1,
      error: null,
    },
    ...over,
  });
  // Done up to the PR and its Jira step: the merge is next.
  const upToJira = (["commit", "push", "pr", "jira"] as const).map((rung) =>
    approval({ id: rung, rung, status: "executed" }),
  );
  const withPr = (over: Partial<DeliveryPr>, approvals = upToJira) =>
    status({ kind: "pr", number: 5 }, { prs: [pr(over)], approvals });

  it("brings the task back when the PR needs the user", () => {
    expect(withPr({ new_reviews: 1, new_comments: 1, new_authors: ["ana"] })).toEqual({
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: "PR: 2 novedades",
    });
    expect(withPr({ checks: { passing: 1, failing: 1, pending: 0 } }).label).toBe(
      "PR: checks fallando",
    );
    expect(withPr({ conflicts: true }).group).toBe("attention");
    expect(withPr({ review_decision: "changes_requested" }).label).toBe("PR: cambios pedidos");
  });

  it("says the CI of the PR's head and keeps a CI that never ran in sight", () => {
    const unrun = withPr({
      ci: "not_run",
      head: "abc1234ef",
      checks: { passing: 0, failing: 0, pending: 0 },
    });
    expect(unrun.label).toBe("PR: CI sin ejecutar");
    expect(unrun.group).toBe("attention");
    expect(unrun.detail).toBe("CI sin ejecutar en abc1234");
    expect(withPr({ ci: "passing", head: "abc1234ef" }).detail).toBe(
      "Nadie tiene pedida la revisión · CI pasa en abc1234",
    );
  });

  it("says who a PR waiting for approval waits on", () => {
    const asked = withPr({
      requested_reviewers: ["ana", "luis"],
      ci: "passing",
      head: "abc1234ef",
    });
    expect(asked.label).toBe("PR: espera aprobación");
    expect(asked.detail).toBe("Revisión pedida a ana, luis · CI pasa en abc1234");
    expect(withPr({}).detail).toBe("Nadie tiene pedida la revisión");
    const world = (waits_on: DeliveryTaskPhase["waits_on"]) =>
      overview({
        prs: [pr({ requested_reviewers: ["ana"] })],
        phases: { t1: { kind: "pr", number: 5, waits_on, next_stage: null } },
      });
    expect(waitsOnLabel(world("reviewers"), "t1")).toBe("ana");
    expect(waitsOnLabel(world("coordinator"), "t1")).toBe("el coordinador");
    expect(waitsOnLabel(world("agent"), "t1")).toBeNull();
  });

  it("words a merge in progress and a PR merged with CI not green", () => {
    const waits = status({ kind: "waits_merge_commit", waiting_round: 4 });
    expect(waits.label).toBe("Espera el commit del merge");
    expect(waits.detail).toContain("La ronda 4 corre encima del commit del merge");
    const mixed = status({ kind: "merge_mixed", mixed_rounds: [3] });
    expect(mixed.group).toBe("attention");
    expect(mixed.detail).toContain("la ronda 3");
    const red = status({ kind: "merged_failing_ci", number: 9, why: "checks fallando en fedcba9" });
    expect(red).toMatchObject({
      tone: "danger",
      label: "PR #9 fusionado con la CI sin pasar",
      detail: "checks fallando en fedcba9",
    });
  });

  it("waits while others review it, and offers the merge once approved", () => {
    expect(withPr({})).toMatchObject({ group: "waiting", label: "PR: espera aprobación" });
    expect(withPr({ checks: { passing: 1, failing: 0, pending: 2 } }).label).toBe(
      "PR: checks en curso",
    );
    expect(withPr({ review_decision: "approved" })).toMatchObject({
      group: "idle",
      label: "PR aprobado · siguiente: merge",
    });
    const merged = [...upToJira, approval({ id: "m", rung: "merge", status: "executed" })];
    expect(withPr({ state: "merged" }, merged).label).toBe(
      "PR fusionado · siguiente: Jira (merge)",
    );
    // Without the PR's Jira step, that one comes before the merge.
    expect(withPr({ review_decision: "approved" }, upToJira.slice(0, 3)).label).toBe(
      "PR aprobado · siguiente: Jira (PR)",
    );
    expect(prSummary(pr({ state: "closed" })).label).toBe("Cerrado sin fusionar");
    expect(nextStep(upToJira)?.label).toBe("Merge");
  });
});

describe("results left behind by later work", () => {
  const changed = [{ path: "src/app.ts", kind: "modified" }];

  it("marks a result later work superseded, and answered findings", () => {
    const implementation = job({ role: "implementation" });
    const jobs = [job({ changes: changed }), implementation, job({ attempt: 2, changes: changed })];
    expect(attemptOutcome(implementation, jobs).label).toBe("Superado");
    // The tests themselves stand, and so does a result nothing came after.
    expect(attemptOutcome(jobs[2], jobs).label).toBe("Sin hallazgos");
    expect(isSuperseded(jobs[0], jobs)).toBe(false);
    const review = job({ role: "review" });
    const undone = job({ role: "implementation", attempt: 2, changes: changed, undone_at_ms: 5 });
    expect(isSuperseded(review, [job(), review, undone])).toBe(false);

    const tests = job({ result: { ...job().result!, status: "findings", findings: ["a", "b"] } });
    expect(attemptOutcome(tests, [tests, job({ role: "implementation" })])).toEqual({
      tone: "idle",
      label: "2 hallazgos · atendidos",
    });
    expect(attemptOutcome(tests, [tests]).tone).toBe("attention");
    // Accepted as they are (resolve_job with a note): settled, not a warning.
    const accepted = { ...tests, resolved_at_ms: 9, resolved_note: "no bloquean" };
    expect(attemptOutcome(accepted, [accepted])).toEqual({
      tone: "ok",
      label: "2 hallazgos · aceptados",
    });
    const paused = job({ status: "paused", result: null });
    const resumed = job({ status: "started", ended_at_ms: null, resumes: paused.id });
    expect(attemptOutcome(paused, [paused, resumed]).label).toBe("Pausado · reanudado");
  });
});

describe("attemptOutcome", () => {
  it("reports one outcome per attempt", () => {
    expect(attemptOutcome(job()).label).toBe("Sin hallazgos");
    expect(attemptOutcome(job({ result_state: "stale" })).label).toBe("Obsoleto");
    expect(
      attemptOutcome(
        job({
          result: {
            status: "findings",
            summary: "",
            changed_paths: [],
            checks: [],
            findings: ["a", "b"],
            handoff: "",
          },
        }),
      ),
    ).toEqual({ tone: "attention", label: "2 hallazgos" });
    expect(attemptOutcome(job({ status: "queued", lease: "qa" })).label).toBe("Esperando QA");
  });

  it("says whether the sandbox or the environment blocked a stage", () => {
    const blocked = (blocked_by?: string) => {
      const base = job().result!;
      return attemptOutcome(job({ result: { ...base, status: "blocked", blocked_by } })).label;
    };
    expect(blocked("sandbox")).toBe("Bloqueado por el sandbox");
    expect(blocked("environment")).toBe("Bloqueado por el entorno");
    expect(blocked("none")).toBe("Bloqueado");
    expect(blocked()).toBe("Bloqueado");
  });
});

describe("lastHandoff", () => {
  it("takes the latest accepted handoff", () => {
    const tests = job({
      result: {
        status: "pass",
        summary: "",
        changed_paths: [],
        checks: [],
        findings: [],
        handoff: "Implement it.",
      },
    });
    expect(
      lastHandoff([tests, job({ role: "implementation", status: "started", result: null })]),
    ).toEqual({
      text: "Implement it.",
      from: tests,
    });
    expect(lastHandoff([{ ...tests, result_state: "stale" }])).toBeNull();
  });
});

describe("Tinto's checks in the history", () => {
  it("are one row per command, under their own name", () => {
    const lint = job({ role: "checks", agent: "shell", prompt: "npm run lint" });
    const test = job({ role: "checks", agent: "shell", prompt: "npm test" });
    expect(historyKey(lint)).not.toBe(historyKey(test));
    expect(historyKey({ ...lint, attempt: 2 })).toBe(historyKey(lint));
    expect(historyKey(job({ role: "review" }))).toBe("review");
    expect(stageLabel("checks")).toBe("Comprobaciones");
  });

  it("and so are the setup commands, apart from a check with the same command", () => {
    const install = job({ role: "setup", agent: "shell", prompt: "npm ci" });
    expect(historyKey(install)).not.toBe(historyKey({ ...install, role: "checks" }));
    expect(historyKey(install)).not.toBe(historyKey({ ...install, prompt: "make bindings" }));
    expect(stageLabel("setup")).toBe("Preparación");
  });
});

describe("latestOfRole and batchName", () => {
  it("prefers the attempt that still counts over a later stale one", () => {
    const accepted = job({ role: "implementation" });
    const stale = job({ role: "implementation", result_state: "stale" });
    expect(latestOfRole([accepted, stale], "implementation")).toBe(accepted);
    expect(latestOfRole([stale], "implementation")).toBe(stale);
  });

  it("names a batch once", () => {
    expect(batchName("Lote de octubre")).toBe("Lote de octubre");
    expect(batchName("Octubre")).toBe("Lote Octubre");
    expect(batchName("Loteria")).toBe("Lote Loteria");
  });
});

describe("an issue delivered in packages", () => {
  const backend: DeliveryTask = {
    ...task,
    id: "p1",
    key: "AGOS-432",
    package: { index: 1, of: 2, title: "Backend" },
  };
  const ui: DeliveryTask = {
    ...backend,
    id: "p2",
    package: { index: 2, of: 2, title: "UI", builds_on: "p1" },
  };
  // A middle package's ladder, as Tinto sends it: no Jira after the merge.
  const middle = ["commit", "push", "pr", "jira", "merge"].map((rung) => ({
    rung: rung as DeliveryApproval["rung"],
    stage: rung === "jira" ? "after_pr" : null,
  }));
  const split = overview({
    tasks: [ui, task, backend],
    phases: {
      p1: { kind: "delivered", waits_on: "nobody", next_stage: null, steps: middle },
      p2: { kind: "running", job_id: "j", role: "qa", waits_on: "agent", next_stage: null },
    },
    issues: [
      {
        repo: backend.repo,
        key: "AGOS-432",
        of: 2,
        packages: [
          {
            task_id: "p1",
            index: 1,
            title: "Backend",
            merged: true,
            delivered: false,
            archived: false,
          },
          {
            task_id: "p2",
            index: 2,
            title: "UI",
            merged: false,
            delivered: false,
            archived: false,
          },
        ],
        delivered: false,
      },
    ],
  });

  it("names each package and says where the issue's packages stand", () => {
    expect(taskLabel(ui)).toBe("AGOS-432 · 2/2");
    expect(taskLabel(task)).toBe("AGOS-501");
    expect(issueProgress(split, ui)).toBe("1/2 fusionado · 2/2 en QA");
    expect(issueProgress(split, task)).toBeNull();
  });

  it("ends a middle package's ladder at its merge", () => {
    const steps = stepsOf(split, "p1");
    expect(steps.map((step) => step.label)).toEqual(["Commit", "Push", "PR", "Jira (PR)", "Merge"]);
    const done = steps.map((step, at) =>
      approval({
        id: `s${at}`,
        task_id: "p1",
        rung: step.rung,
        stage: step.stage,
        status: "executed",
      }),
    );
    expect(isDelivered(done, steps)).toBe(true);
    expect(isDelivered(done)).toBe(false);
    // Without Tinto's steps, the whole ladder.
    expect(stepsOf(split, "p2")).toHaveLength(6);
  });

  it("lists a split issue as one entry, its packages in order, where its neediest package is", () => {
    const groups = taskListGroups([ui, task, backend], (one) =>
      one.id === "p2" ? "running" : one.id === "p1" ? "delivered" : "idle",
    );
    expect(
      groups.map((group) => [
        group.group,
        group.entries.map((entry) =>
          entry.kind === "task" ? entry.task.id : entryTasks([entry]).map((one) => one.id),
        ),
      ]),
    ).toEqual([
      ["running", [["p1", "p2"]]],
      ["idle", ["t1"]],
    ]);
    expect(issueRowId(backend)).toBe("issue:agos-432");
  });

  it("says where each package of an issue stands, created or not", () => {
    const issue = { ...split.issues![0], of: 3 };
    expect([1, 2, 3].map((index) => packageState(split, issue, index))).toEqual([
      "fusionado",
      "en QA",
      "sin crear",
    ]);
  });
});

describe("a task in a round", () => {
  const round = (over: Partial<DeliveryRound> = {}): DeliveryRound & { at: string } => ({
    id: "r2",
    task_id: "t1",
    number: 2,
    kind: "qa",
    short: "5 hallazgos de QA",
    cause: "5 hallazgos de QA",
    items: [],
    opened_at_ms: 100,
    at: "implementation",
    ...over,
  });
  const inRound = (phase: DeliveryPhase, current: DeliveryRound & { at: string }) =>
    overview({ phases: { t1: { ...phase, waits_on: "agent", next_stage: null, round: current } } });

  it("says which round and why before what happens now", () => {
    const running = job({ id: "i2", role: "implementation", status: "started", ended_at_ms: null });
    const qa = inRound({ kind: "running", job_id: "i2", role: "implementation" }, round());
    expect(taskStatus(task, { ...qa, jobs: [running] }, 76_000).label).toBe(
      "↺ Ronda 2 · 5 hallazgos de QA · implementando · 1:15",
    );
    const pr = inRound(
      { kind: "queued", job_id: "i2", role: "implementation", lease: null, position: 1 },
      round({ number: 3, kind: "pr", short: "PR", cause: "Cambios pedidos en el PR #308" }),
    );
    expect(taskStatus(task, pr, 0).label).toBe("↺ Ronda 3 · PR: en cola · 1.º");
    expect(nowLine(task, pr, 0)).toBe(
      "Ronda 3 · Cambios pedidos en el PR #308 · Implementación en cola",
    );
  });

  it("keeps the tests of an earlier round and runs again what follows them", () => {
    const tests = job({ role: "tests", created_at_ms: 1 });
    const review = job({ role: "review", created_at_ms: 2 });
    const fix = job({ role: "implementation", created_at_ms: 150 });
    const jobs = [tests, review, fix];
    expect(roundStage("tests", tests, jobs, round())).toEqual({
      tone: "ok",
      label: "Vale de la ronda anterior",
    });
    expect(roundStage("review", review, jobs, round())?.label).toBe("Se repite en esta ronda");
    expect(roundStage("qa", undefined, jobs, round())?.label).toBe("Se repite en esta ronda");
    // This round's own attempt, and any outside a round, read as they went.
    expect(roundStage("implementation", fix, jobs, round())).toBeNull();
    expect(roundStage("review", review, jobs, null)).toBeNull();
  });

  it("repeats the commit, and the push once published, until asked for in the round", () => {
    const commit = { rung: "commit" as const, label: "Commit" };
    const push = { rung: "push" as const, label: "Push" };
    const done = approval({ status: "executed", requested_at_ms: 50 });
    expect(roundRung(commit, done, round())).toBe(true);
    expect(roundRung(push, done, round())).toBe(false);
    expect(roundRung(push, done, round({ after_push: true }))).toBe(true);
    expect(roundRung(commit, approval({ requested_at_ms: 120 }), round())).toBe(false);
    expect(roundRung(commit, done, null)).toBe(false);
  });

  it("folds the rounds that ended, newest first, down to the initial delivery", () => {
    const ended = { ...round(), closed_at_ms: 200, outcome: "corregidos" };
    const open = { ...round({ id: "r3", number: 3, cause: "Cambios pedidos en el PR #308" }) };
    expect(roundHistory(overview({ rounds: [ended, open] }), "t1")).toEqual([
      "Ronda 2 · 5 hallazgos de QA → corregidos",
      "Ronda 1 · entrega inicial",
    ]);
    expect(roundHistory(overview(), "t1")).toEqual([]);
  });
});
