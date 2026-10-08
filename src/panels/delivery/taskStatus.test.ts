import { describe, it, expect } from "vitest";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryTask,
} from "../../delivery/types";
import {
  attemptOutcome,
  batchName,
  lastHandoff,
  latestOfRole,
  nextRole,
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
      "Registrar Jira",
    );
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
    expect(status({ approvals: [approval({ rung: "jira", status: "executed" })] }).label).toBe(
      "Entregada",
    );
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
