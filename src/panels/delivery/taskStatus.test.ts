import { describe, it, expect } from "vitest";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPr,
  DeliveryTask,
} from "../../delivery/types";
import {
  attemptOutcome,
  batchName,
  isSuperseded,
  lastHandoff,
  latestOfRole,
  nextRole,
  nextStep,
  prSummary,
  taskStatus,
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

const status = (over: Partial<DeliveryOverview>) => taskStatus(task, overview(over), 76_000);

describe("taskStatus", () => {
  it("puts a pending approval first, even over a running stage", () => {
    expect(
      status({
        approvals: [approval({ rung: "pr" })],
        jobs: [job({ status: "started", ended_at_ms: null })],
      }),
    ).toEqual({ group: "attention", tone: "attention", icon: "flag", label: "Aprobar PR" });
  });

  it("asks to record an approved PR or Jira step", () => {
    expect(status({ approvals: [approval({ rung: "jira", status: "approved" })] }).label).toBe(
      "Registrar Jira (PR)",
    );
    const closing = approval({ rung: "jira", stage: "after_merge", status: "approved" });
    expect(status({ approvals: [closing] }).label).toBe("Registrar Jira (merge)");
  });

  it("flags a QA lease this task left quarantined", () => {
    const lease = {
      name: "qa",
      state: "quarantined" as const,
      generation: 1,
      holder_job_id: "j9",
      holder_task_id: "t1",
      acquired_at_ms: 1,
      note: null,
      queue: [],
    };
    expect(status({ leases: [lease] })).toMatchObject({
      group: "attention",
      label: "QA en cuarentena",
    });
    // Its implementation running is what the task is doing.
    const implementing = job({ role: "implementation", status: "started", ended_at_ms: null });
    expect(status({ leases: [lease], jobs: [implementing] }).group).toBe("running");
    // A QA job waiting on it is stuck until someone frees it, whoever left it.
    const waiting = job({ role: "qa", status: "queued", lease: "qa" });
    const other = {
      ...lease,
      holder_task_id: "t2",
      queue: [{ job_id: waiting.id, task_id: "t1", requested_at_ms: 1 }],
    };
    expect(status({ leases: [other], jobs: [waiting] })).toMatchObject({
      group: "attention",
      label: "Espera QA, en cuarentena",
    });
  });

  it("holds a paused stage until it is resumed, and says when an agent goes quiet", () => {
    const paused = job({ status: "paused", result: null });
    expect(status({ jobs: [paused] })).toMatchObject({
      group: "waiting",
      label: "Tests en pausa",
    });
    expect(nextRole([paused])).toBeNull();
    const resumed = job({ status: "started", ended_at_ms: null, resumes: paused.id });
    expect(status({ jobs: [paused, resumed] }).group).toBe("running");
    expect(attemptOutcome(paused, [paused, resumed]).label).toBe("Pausado · reanudado");
    // 76 s after the start the agent is still talking; 7 min later it is quiet.
    const quiet = job({ status: "started", ended_at_ms: null, activity_at_ms: 1_000 });
    expect(status({ jobs: [quiet] }).label).not.toContain("sin actividad");
    expect(taskStatus(task, overview({ jobs: [quiet] }), 421_000).label).toContain(
      "sin actividad hace 7 min",
    );
  });

  it("flags a settings proposal only on the task that asked, once its agents stop", () => {
    const proposal = (taskId: string) => ({
      id: `p-${taskId}`,
      repo: task.repo,
      task_id: taskId,
      run_id: "r1",
      reason: "",
      changes: { checks_add: ["npm test"] },
      status: "pending" as const,
      approved: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
    });
    expect(status({ settings_proposals: [proposal("t1")] }).label).toBe(
      "Aprobar cambio de ajustes",
    );
    expect(status({ settings_proposals: [proposal("t2")] }).group).not.toBe("attention");
    expect(
      status({
        settings_proposals: [proposal("t1")],
        jobs: [job({ role: "qa", status: "started", ended_at_ms: null })],
      }).group,
    ).toBe("running");
  });

  it("shows a running stage with its clock", () => {
    expect(
      status({ jobs: [job({ role: "implementation", status: "started", ended_at_ms: null })] }),
    ).toEqual({ group: "running", tone: "live", icon: "live", label: "Implementando · 1:15" });
  });

  it("gives the position in the QA queue and in the capacity queue", () => {
    const waiting = job({ role: "qa", status: "queued", lease: "qa" });
    const lease = {
      name: "qa",
      state: "active" as const,
      generation: 1,
      holder_job_id: "x",
      holder_task_id: "t9",
      acquired_at_ms: 1,
      note: null,
      queue: [{ job_id: waiting.id, task_id: "t1", requested_at_ms: 1 }],
    };
    expect(status({ jobs: [waiting], leases: [lease] }).label).toBe(
      "Esperando QA · 1.º en la cola",
    );

    const ahead = job({ task_id: "t2", status: "queued" });
    const mine = job({ status: "queued" });
    expect(status({ jobs: [ahead, mine] }).label).toBe("En cola · 2.º");
  });

  it("flags the latest attempt when it failed, even with an invalid result", () => {
    expect(
      status({
        jobs: [
          job(),
          job({
            role: "implementation",
            attempt: 2,
            status: "failed",
            result: null,
            result_state: "invalid",
          }),
        ],
      }),
    ).toEqual({
      group: "attention",
      tone: "danger",
      icon: "x",
      label: "Falló implementación · intento 2",
    });
  });

  it("names the next stage when idle, and says when it was delivered", () => {
    expect(status({ jobs: [job()] }).label).toBe("Siguiente: implementación");
    const done = (rung: DeliveryApproval["rung"], stage?: "after_pr" | "after_merge") =>
      approval({ id: `${rung}-${stage}`, rung, stage, status: "executed" });
    const upToMerge = [done("commit"), done("push"), done("pr"), done("jira"), done("merge")];
    // The PR's Jira step alone is not the delivery.
    expect(status({ approvals: upToMerge }).label).not.toBe("Entregada");
    expect(nextStep(upToMerge)?.label).toBe("Jira (merge)");
    expect(status({ approvals: [...upToMerge, done("jira", "after_merge")] }).label).toBe(
      "Entregada",
    );
  });
});

describe("pending decisions", () => {
  it("come before anything else", () => {
    const pending = {
      id: "d1",
      task_id: "t1",
      kind: "choice" as const,
      question: "?",
      detail: "",
      options: [],
      text: "",
      command: null,
      undo: "",
      status: "pending" as const,
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
    };
    expect(status({ decisions: [pending], approvals: [approval({ rung: "commit" })] })).toEqual({
      group: "attention",
      tone: "attention",
      icon: "flag",
      label: "1 decisión pendiente",
    });
    expect(status({ decisions: [{ ...pending, status: "answered", answer: "x" }] }).label).toBe(
      "Siguiente: tests",
    );
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
    status({ prs: [pr(over)], approvals });

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

  it("waits while others review it, and offers the merge once approved", () => {
    expect(withPr({})).toMatchObject({ group: "waiting", label: "PR: en revisión" });
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
  });
});

describe("nextRole", () => {
  it("starts with tests and follows the built-in order", () => {
    expect(nextRole([])).toBe("tests");
    expect(nextRole([job(), job({ role: "implementation" })])).toBe("review");
  });

  it("goes back to implementation after findings", () => {
    const review = job({
      role: "review",
      result: {
        status: "findings",
        summary: "",
        changed_paths: [],
        checks: [],
        findings: ["a"],
        handoff: "",
      },
    });
    expect(nextRole([job(), job({ role: "implementation" }), review])).toBe("implementation");
  });

  it("retries a broken stage and ignores stale results", () => {
    expect(nextRole([job(), job({ role: "implementation", status: "failed", result: null })])).toBe(
      "implementation",
    );
    expect(nextRole([job({ result_state: "stale" })])).toBe("tests");
  });

  it("suggests nothing while any stage runs, and counts answered findings as done", () => {
    const tests = job({
      result: { ...job().result!, status: "findings", findings: ["a", "b"] },
    });
    const implemented = job({ role: "implementation" });
    const reviewing = job({ role: "review", status: "started", ended_at_ms: null });
    // The review runs: no stage is offered, not even the tests with findings.
    expect(nextRole([tests, implemented, reviewing])).toBeNull();
    // Once the implementation passed, the tests' findings are answered.
    expect(nextRole([tests, implemented, job({ role: "review" })])).toBe("qa");
    expect(attemptOutcome(tests, [tests, implemented])).toEqual({
      tone: "idle",
      label: "2 hallazgos · atendidos",
    });
    expect(attemptOutcome(tests, [tests]).tone).toBe("attention");
  });

  it("offers no stage once the task was committed after its last one", () => {
    const tests = job({
      result: { ...job().result!, status: "findings", findings: ["a"] },
    });
    const reviewed = job({ role: "review", ended_at_ms: 61_000 });
    const committed = approval({ status: "executed", executed_at_ms: 90_000 });
    expect(nextRole([tests, job({ role: "implementation" }), reviewed], [committed])).toBeNull();
    // The sidebar names the delivery step instead (here, the push).
    expect(
      status({
        jobs: [tests, job({ role: "implementation" }), reviewed],
        approvals: [committed],
      }).label,
    ).toBe("Siguiente: push");
    // New work after the commit brings the stages back.
    const later = job({
      role: "implementation",
      ended_at_ms: 120_000,
      changes: [{ path: "src/app.ts", kind: "modified" }],
    });
    expect(nextRole([tests, reviewed, later], [committed])).toBe("review");
  });

  it("suggests nothing while the next stage runs or once every stage passed", () => {
    expect(nextRole([job(), job({ role: "implementation", status: "started" })])).toBeNull();
    expect(
      nextRole([
        job(),
        job({ role: "implementation" }),
        job({ role: "review" }),
        job({ role: "qa" }),
      ]),
    ).toBeNull();
  });
});

describe("results left behind by later work", () => {
  const changed = [{ path: "src/app.ts", kind: "modified" }];

  it("asks for the implementation again after the tests are redone", () => {
    const implementation = job({ role: "implementation" });
    const jobs = [job({ changes: changed }), implementation, job({ attempt: 2, changes: changed })];
    expect(nextRole(jobs)).toBe("implementation");
    expect(attemptOutcome(implementation, jobs).label).toBe("Superado");
    // The tests themselves stand, and so does a result nothing came after.
    expect(attemptOutcome(jobs[2], jobs).label).toBe("Sin hallazgos");
    expect(isSuperseded(jobs[0], jobs)).toBe(false);
  });

  it("asks for the review again after new code, unless that code was undone", () => {
    const review = job({ role: "review" });
    const fix = job({ role: "implementation", attempt: 2, changes: changed });
    const before = [job(), job({ role: "implementation", result_state: "stale" }), review];
    expect(nextRole([...before, fix])).toBe("review");
    expect(nextRole([...before, { ...fix, undone_at_ms: 5 }])).toBe("implementation");
    expect(isSuperseded(review, [...before, { ...fix, undone_at_ms: 5 }])).toBe(false);
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
