import { describe, it, expect } from "vitest";
import type {
  DeliveryApproval,
  DeliveryDecision,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPhase,
  DeliveryPr,
  DeliveryReviewRequest,
  DeliveryTask,
  DeliveryTaskPhase,
  DeliveryWaiter,
} from "../../delivery/types";
import { deliveryQueue } from "./queueItems";

const REPO = "C:\\work\\agentos";

const task = (id: string, key = id.toUpperCase()): DeliveryTask => ({
  id,
  run_id: "r1",
  repo: REPO,
  distro: null,
  key,
  title: "",
  worktree: `C:\\wt\\${id}`,
  branch: `delivery/${id}`,
  base_ref: "develop",
  base_commit: "0123456789",
  state: "tests",
  contract_version: 1,
  created_at_ms: 1,
  updated_at_ms: 1,
});

const phase = (of: DeliveryPhase, waits_on: DeliveryWaiter = "user"): DeliveryTaskPhase => ({
  ...of,
  waits_on,
  next_stage: null,
});

const approval = (over: Partial<DeliveryApproval> = {}): DeliveryApproval => ({
  id: "a1",
  task_id: "t1",
  rung: "commit",
  title: "AGOS-1: Fix",
  body: "",
  status: "pending",
  requested_by: "coord",
  requested_at_ms: 1,
  decided_at_ms: null,
  executed_at_ms: null,
  outcome: null,
  ...over,
});

const decision = (taskId: string): DeliveryDecision => ({
  id: `d-${taskId}`,
  task_id: taskId,
  kind: "choice",
  question: "¿Sí?",
  detail: "",
  options: [],
  text: "",
  command: null,
  undo: "",
  status: "pending",
  answer: null,
  requested_by: "coord",
  requested_at_ms: 1,
  decided_by: null,
  decided_at_ms: null,
});

const job = (over: Partial<DeliveryJob> = {}): DeliveryJob => ({
  id: "j1",
  task_id: "t1",
  role: "implementation",
  attempt: 2,
  agent: "codex",
  model: null,
  access: "workspace",
  prompt: "",
  contract_version: 1,
  writes: true,
  lease: null,
  timeout_minutes: 45,
  status: "failed",
  created_at_ms: 1,
  started_at_ms: 1,
  ended_at_ms: 2,
  pid: null,
  provider_session_id: null,
  exit_code: 1,
  error: "boom",
  start_candidate: null,
  end_candidate: null,
  changes: [],
  result: null,
  result_state: "accepted",
  result_note: null,
  undone_at_ms: null,
  ...over,
});

const pr = (over: Partial<DeliveryPr> = {}): DeliveryPr => ({
  task_id: "t1",
  url: "https://github.com/acme/api/pull/5",
  repo: "acme/api",
  number: 5,
  state: "open",
  draft: false,
  conflicts: false,
  review_decision: null,
  checks: { passing: 1, failing: 0, pending: 0 },
  new_reviews: 0,
  new_comments: 0,
  new_authors: [],
  latest_activity: null,
  seen_until: null,
  checked_at_ms: 1,
  error: null,
  ...over,
});

const review = (repo: string, number = 9): DeliveryReviewRequest => ({
  repo,
  number,
  title: "Rate-limit the export",
  url: `https://github.com/${repo}/pull/${number}`,
  author: "ana",
  updated_at: "2026-11-10T10:00:00Z",
  draft: false,
});

const overview = (over: Partial<DeliveryOverview> = {}): DeliveryOverview => ({
  phases: {},
  runs: [],
  tasks: [task("t1")],
  jobs: [],
  leases: [],
  approvals: [],
  decisions: [],
  settings: { capacity: 3, quota_stop_percent: 99 },
  coordinator: null,
  ...over,
});

/** The queue of REPO with the overview's tasks in their order. */
const queue = (
  of: DeliveryOverview,
  reviews: DeliveryReviewRequest[] = [],
  edited: string[] = [],
) => deliveryQueue(of, of.tasks, reviews, 0, (id) => edited.includes(id));

const brief = (items: ReturnType<typeof queue>) =>
  items.map((item) => ({
    key: item.key,
    kind: item.kind,
    need: item.need,
    inPlace: item.inPlace?.kind ?? null,
  }));

describe("deliveryQueue", () => {
  it("lists a task's decision and approval, approving in place only a step with no text of its own", () => {
    const of = (step: DeliveryApproval) =>
      overview({
        phases: { t1: phase({ kind: "decisions", count: 1 }) },
        decisions: [decision("t1")],
        approvals: [step],
      });
    const push = approval({ rung: "push", title: "Publicar delivery/t1" });
    expect(brief(queue(of(push)))).toEqual([
      { key: "t1:decisions", kind: "holds", need: "1 decisión", inPlace: null },
      {
        key: "t1:a1",
        kind: "holds",
        // The line says what the click approves.
        need: "Aprobar push · Publicar delivery/t1",
        inPlace: "approve",
      },
    ]);
    const [, approve] = queue(of(push));
    expect(approve.inPlace).toMatchObject({ kind: "approve", label: "Aprobar y publicar" });
    // Edited in the task, noticed by Tinto, with text it publishes (commit
    // messages, a Jira comment) or something to choose: answered in the task.
    for (const [step, edited] of [
      [push, ["a1"]],
      [approval({ rung: "push", notice: "La rama cambió" }), []],
      [approval({ commits: [{ message: "fix: a\n\nbody", paths: ["a.ts"] }] }), []],
      [approval({ rung: "jira" }), []],
      [approval({ rung: "sync", body: "Merge develop: resuelve a.ts" }), []],
      [approval({ rung: "pr" }), []],
      [approval({ rung: "merge" }), []],
    ] as const) {
      const [, item] = queue(of(step), [], [...edited]);
      expect(item.inPlace).toBeNull();
      expect(item.ask).toBe("a1");
    }
  });

  it.each<[string, Partial<DeliveryOverview>, string, "retry" | null]>([
    [
      "its stage broke, retried from the queue",
      {
        jobs: [job()],
        phases: {
          t1: {
            ...phase({ kind: "next_stage", role: "implementation" }, "coordinator"),
            verdicts: { j1: { broken: true } },
          },
        },
      },
      "Falló implementación · intento 2",
      "retry",
    ],
    [
      "its stage broke while a blocker holds it, seen in the task: a retry would be refused",
      {
        tasks: [
          {
            ...task("t1"),
            blockers: [
              { id: "b1", type: "note", text: "Espera a infra", status: "", cleared_at_ms: null },
            ],
          },
        ],
        jobs: [job()],
        phases: {
          t1: {
            ...phase({ kind: "next_stage", role: "implementation" }, "coordinator"),
            verdicts: { j1: { broken: true } },
          },
        },
      },
      "Falló implementación · intento 2",
      null,
    ],
    [
      "Tinto's checks fail",
      {
        phases: {
          t1: {
            ...phase({ kind: "next_stage", role: "implementation" }, "coordinator"),
            failing_checks: ["npm test"],
          },
        },
      },
      "Fallan las comprobaciones: npm test",
      null,
    ],
    [
      "its PR's checks fail",
      {
        prs: [pr({ checks: { passing: 0, failing: 2, pending: 0 } })],
        phases: {
          t1: phase(
            { kind: "pr", number: 5, wait: { kind: "review", reviewers: ["ana"] } },
            "reviewers",
          ),
        },
      },
      "PR #5: checks fallando",
      null,
    ],
  ])("lists what broke on a task that waits on someone else: %s", (_, over, need, inPlace) => {
    const of = overview(over);
    expect(brief(queue(of))).toEqual([{ key: "t1:broke", kind: "broke", need, inPlace }]);
  });

  it.each<[string, Partial<DeliveryOverview>, string, string]>([
    [
      "a paused stage, resumed on its job's row",
      {
        jobs: [job({ id: "j1", role: "tests", status: "paused" })],
        phases: { t1: phase({ kind: "paused", job_id: "j1", role: "tests" }) },
      },
      "Tests en pausa",
      "job:j1",
    ],
    [
      "a PR nobody was asked to review, moved in its section",
      {
        prs: [pr()],
        phases: { t1: phase({ kind: "pr", number: 5, wait: { kind: "nobody" } }) },
      },
      "PR: nadie tiene pedida la revisión",
      "pr",
    ],
  ])(
    "lists a task that waits on the user without an ask of its own, as its row says it: %s",
    (_, over, need, ask) => {
      const items = queue(overview(over));
      expect(brief(items)).toEqual([{ key: "t1:status", kind: "holds", need, inPlace: null }]);
      expect(items[0].ask).toBe(ask);
    },
  );

  it.each<[string, Partial<DeliveryOverview>]>([
    [
      "a job open for it (its checks running)",
      {
        jobs: [
          job({ id: "c1", role: "checks", agent: "shell", status: "started", ended_at_ms: null }),
        ],
        phases: {
          t1: {
            ...phase({ kind: "running", job_id: "c1", role: "checks" }, "coordinator"),
            failing_checks: ["npm test"],
          },
        },
      },
    ],
    [
      "a phase that waits on an agent",
      {
        phases: {
          t1: {
            ...phase({ kind: "next_stage", role: "implementation" }, "agent"),
            failing_checks: ["npm test"],
          },
        },
      },
    ],
    [
      "a phase that waits on the queue, its PR's checks failing",
      {
        prs: [pr({ checks: { passing: 0, failing: 2, pending: 0 } })],
        phases: {
          t1: phase(
            { kind: "queued", job_id: "j9", role: "implementation", lease: null, position: 1 },
            "queue",
          ),
        },
      },
    ],
  ])("leaves out failing checks somebody is working on: %s", (_, over) => {
    expect(queue(overview(over))).toEqual([]);
  });

  it("lists the PRs to review of this repo only: of the GitHub repository its tasks' PRs are in", () => {
    const named = overview({ prs: [pr()] });
    const reviews = [review("acme/api"), review("acme/web", 10), review("other/agentos", 11)];
    expect(queue(named, reviews).map((item) => [item.title, item.need, item.review?.url])).toEqual([
      ["PR #9 · ana", "Rate-limit the export", "https://github.com/acme/api/pull/9"],
    ]);
    // A folder named like a repository says nothing of whose it is.
    expect(queue(overview(), reviews)).toEqual([]);
  });

  it("leaves batch-level waits out: a settings proposal, a stopped coordinator, a paused batch", () => {
    const of = (kind: "settings_proposal" | "batch_paused") =>
      overview({
        phases: {
          t1:
            kind === "settings_proposal"
              ? phase({ kind })
              : phase({ kind, job_id: "j1", role: "tests" }),
        },
        runs: [
          {
            id: "r1",
            repo: REPO,
            title: "Lote",
            status: "active",
            generation: 1,
            owner: "coord",
            created_at_ms: 1,
            updated_at_ms: 1,
          },
        ],
        coordinators: [{ run_id: "r1", waiting: false, unread: 3, unread_since_ms: 1 }],
        settings_proposals: [
          {
            id: "p1",
            repo: REPO,
            task_id: "t1",
            run_id: "r1",
            reason: "",
            changes: {},
            status: "pending",
            approved: null,
            requested_by: "coord",
            requested_at_ms: 1,
            decided_by: null,
            decided_at_ms: null,
          },
        ],
      });
    expect(queue(of("settings_proposal"))).toEqual([]);
    expect(queue(of("batch_paused"))).toEqual([]);
  });

  it("orders what holds a task, then what broke, then PRs to review, each by the list's order", () => {
    const tasks = [task("t1"), task("t2"), task("t3")];
    const of = overview({
      tasks,
      // t1 broke; t2 and t3 have decisions; t3 also lands as a drifted push.
      jobs: [job()],
      phases: {
        t1: {
          ...phase(
            { kind: "broken", job_id: "j1", role: "implementation", attempt: 2, blocked: false },
            "coordinator",
          ),
          verdicts: { j1: { broken: true } },
        },
        t2: phase({ kind: "decisions", count: 1 }),
        t3: phase({ kind: "drifted", rung: "push", stage: null }),
      },
      decisions: [decision("t2")],
      prs: [pr({ repo: "other/agentos" })],
    });
    expect(queue(of, [review("other/agentos")]).map((item) => item.key)).toEqual([
      "t2:decisions",
      "t3:status",
      "t1:broke",
      "review:https://github.com/other/agentos/pull/9",
    ]);
    expect(queue(of)[1].need).toBe("Push: ya no coincide");
  });
});
