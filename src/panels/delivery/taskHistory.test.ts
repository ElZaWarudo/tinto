import { describe, expect, it } from "vitest";
import type { DeliveryJob, DeliveryRound } from "../../delivery/types";
import { checksOutcome, spanLabel, taskHistory, whenLabel } from "./taskHistory";

function job(id: string, created: number, over: Partial<DeliveryJob> = {}): DeliveryJob {
  return {
    id,
    task_id: "t1",
    role: "implementation",
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
    started_at_ms: created,
    ended_at_ms: created + 1,
    pid: null,
    provider_session_id: null,
    exit_code: 0,
    error: null,
    start_candidate: null,
    end_candidate: null,
    changes: [],
    result: null,
    result_state: "accepted",
    result_note: null,
    undone_at_ms: null,
    ...over,
  };
}

const check = (id: string, created: number, prompt: string, over: Partial<DeliveryJob> = {}) =>
  job(id, created, { role: "checks", agent: "shell", prompt, ...over });

function round(number: number, opened: number, closed: number | null = null): DeliveryRound {
  return {
    id: `r${number}`,
    task_id: "t1",
    number,
    kind: "qa",
    short: `ronda ${number}`,
    cause: `causa ${number}`,
    items: [],
    opened_at_ms: opened,
    closed_at_ms: closed,
  };
}

/** Each entry, newest first: its round (0 for the initial delivery) and its rows' jobs. */
function shape(jobs: DeliveryJob[], rounds: DeliveryRound[]) {
  return taskHistory(jobs, rounds).map((entry) => [
    entry.round?.number ?? 0,
    entry.rows.map((row) => row.attempts.map((attempt) => attempt.id)),
  ]);
}

describe("taskHistory", () => {
  it.each([
    {
      name: "a task with no rounds is all initial delivery, stages in the order they ran",
      jobs: [job("t", 1, { role: "tests" }), job("i", 2), job("r", 3, { role: "review" })],
      rounds: [],
      expected: [[0, [["t"], ["i"], ["r"]]]],
      latest: ["t", "i", "r"],
    },
    {
      name: "jobs go to the last round opened before them, newest round first",
      jobs: [job("i", 1), job("r", 2, { role: "review" }), job("i2", 6), job("i3", 12)],
      rounds: [round(2, 5, 10), round(3, 10)],
      expected: [
        [3, [["i3"]]],
        [2, [["i2"]]],
        [0, [["i"], ["r"]]],
      ],
      latest: ["i3", "r"],
    },
    {
      name: "what a stage was told about its round wins over the time it was created",
      jobs: [
        job("i", 1),
        job("told", 12, { round: ["This is round 2 of the task: its work came back."] }),
      ],
      rounds: [round(2, 5), round(3, 10)],
      expected: [
        [3, []],
        [2, [["told"]]],
        [0, [["i"]]],
      ],
      latest: ["told"],
    },
    {
      name: "two attempts of a stage in a round are one row, the last shown",
      jobs: [job("i", 6), job("r", 7, { role: "review" }), job("i2", 8, { attempt: 2 })],
      rounds: [round(2, 5)],
      expected: [
        [2, [["i", "i2"], ["r"]]],
        [0, []],
      ],
      latest: ["i2", "r"],
    },
    {
      name: "a resumed round comes back on top, with the stage's latest attempt",
      jobs: [
        job("i1", 1),
        job("i3", 12, { round: ["This is round 3 of the task: its work came back."] }),
        job("i2", 22, { round: ["This is round 2 of the task: its work came back."] }),
      ],
      // Round 2, interrupted by round 3, opened again at 20.
      rounds: [round(2, 20), round(3, 10, 18)],
      expected: [
        [2, [["i2"]]],
        [3, [["i3"]]],
        [0, [["i1"]]],
      ],
      latest: ["i2"],
    },
    {
      name: "a check goes to the round it records, even before the round's resumed opening",
      jobs: [job("i1", 1), check("c2", 8, "npm test", { round_number: 2 })],
      // Round 2 opened at 5, interrupted by round 3, opened again at 20.
      rounds: [round(2, 20), round(3, 10, 18)],
      expected: [
        [2, [["c2"]]],
        [3, []],
        [0, [["i1"]]],
      ],
      latest: ["i1"],
    },
  ])("$name", ({ jobs, rounds, expected, latest }) => {
    expect(shape(jobs, rounds)).toEqual(expected);
    // Where a stage picked on the track lands: its latest job's row, one per stage.
    const marked = taskHistory(jobs, rounds).flatMap((entry) =>
      entry.rows.filter((row) => row.latest).map((row) => row.job.id),
    );
    expect(marked).toEqual(latest);
    for (const entry of taskHistory(jobs, rounds)) {
      for (const row of entry.rows) expect(row.job).toBe(row.attempts[row.attempts.length - 1]);
    }
  });

  it("gathers a round's checks in one row, each command by its last run", () => {
    const jobs = [
      job("i", 1),
      check("lint", 2, "npm run lint", { status: "failed", exit_code: 1 }),
      check("test", 3, "npm test"),
      check("lint2", 4, "npm run lint", { attempt: 2 }),
    ];
    const [initial] = taskHistory(jobs, []);
    expect(initial.rows.map((row) => row.key)).toEqual(["implementation", "checks"]);
    const checks = initial.rows[1];
    expect(checks.attempts.map((attempt) => attempt.id)).toEqual(["lint", "test", "lint2"]);
    expect(checks.checks!.map((last) => last.id)).toEqual(["lint2", "test"]);
    expect(checksOutcome(checks.checks!, jobs, {})).toEqual({ tone: "ok", label: "2 de 2 pasan" });
    expect(checksOutcome([jobs[1], jobs[2]], jobs, {})).toEqual({
      tone: "danger",
      label: "1 de 2 pasan",
    });
  });

  it("dates the entries: from the first job, a round from when it opened until it closed", () => {
    const [current, past, initial] = taskHistory(
      [job("i", 2), job("r", 6, { role: "review" })],
      [round(2, 5, 9), round(3, 9)],
    );
    expect([initial.from, initial.to, past.from, past.to, current.from, current.to]).toEqual([
      2,
      5,
      5,
      9,
      9,
      null,
    ]);
  });
});

describe("history times", () => {
  const now = new Date(2026, 10, 13, 15, 0).getTime();
  it.each([
    [new Date(2026, 10, 13, 14, 20).getTime(), /^hoy \S/],
    [new Date(2026, 10, 12, 18, 5).getTime(), /^ayer \S/],
    [new Date(2026, 10, 4, 9, 0).getTime(), /^[^a-z]+$/],
  ])("says when %s happened", (at, expected) => {
    expect(whenLabel(at, now)).toMatch(expected);
  });

  it.each([
    [36_000, "36 s"],
    [48 * 60_000, "48 min"],
    [72 * 60_000, "1 h 12 min"],
    [120 * 60_000, "2 h"],
  ])("says %s ms as %s", (ms, expected) => {
    expect(spanLabel(ms)).toBe(expected);
  });
});
