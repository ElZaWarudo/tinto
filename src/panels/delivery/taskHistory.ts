// A task's history as it happened: the initial delivery and each round, in
// order, with one row per stage that ran in it.

import type { DeliveryJob, DeliveryJobVerdict, DeliveryRound } from "../../delivery/types";
import { attemptOutcome, historyKey, isOpen, type Tone } from "./taskStatus";

/** One stage of an entry: its last attempt there, and every attempt. The
 *  round's checks are one row, `checks` holding each command's last run. */
export interface HistoryRow {
  key: string;
  /** The attempt the row shows: the stage's last in the entry. */
  job: DeliveryJob;
  /** Every attempt of the row in the entry, oldest first. */
  attempts: DeliveryJob[];
  /** The check row only: each command's last run, in the order they first ran. */
  checks?: DeliveryJob[];
}

export interface HistoryEntry {
  /** null: the initial delivery, before any round. */
  round: DeliveryRound | null;
  /** In the order the stages first ran in it. */
  rows: HistoryRow[];
  /** When it began and ended; `to` is null while it goes on. */
  from: number | null;
  to: number | null;
}

/** The key of the row that holds a round's checks. */
export const CHECKS_ROW = "checks";

/** "This is round 4 of the task: …": what a stage dispatched in a round was told. */
const TOLD_ROUND = /^This is round (\d+) of the task/;

/** The round a job belongs to: the one it was told about when dispatched,
 *  otherwise the last one opened at or before it was created; null for the
 *  initial delivery. */
function roundOfJob(job: DeliveryJob, rounds: DeliveryRound[]): DeliveryRound | null {
  const told = TOLD_ROUND.exec(job.round?.[0] ?? "");
  const named = told ? rounds.find((round) => round.number === Number(told[1])) : undefined;
  if (named) return named;
  let found: DeliveryRound | null = null;
  for (const round of rounds) if (round.opened_at_ms <= job.created_at_ms) found = round;
  return found;
}

function rowsOf(jobs: DeliveryJob[]): HistoryRow[] {
  const rows = new Map<string, HistoryRow>();
  for (const job of jobs) {
    const key = job.role === CHECKS_ROW ? CHECKS_ROW : historyKey(job);
    const row = rows.get(key);
    if (row) {
      row.attempts.push(job);
      row.job = job;
    } else rows.set(key, { key, job, attempts: [job] });
  }
  const checks = rows.get(CHECKS_ROW);
  if (checks) {
    // A Map keeps each command where it was first set.
    const last = new Map<string, DeliveryJob>();
    for (const job of checks.attempts) last.set(job.prompt, job);
    checks.checks = [...last.values()];
  }
  return [...rows.values()];
}

/** The task's history, newest first: each round, then the initial delivery.
 *  `jobs` oldest first, as the overview keeps them; `rounds` the task's. */
export function taskHistory(jobs: DeliveryJob[], rounds: DeliveryRound[]): HistoryEntry[] {
  const ordered = [...rounds].sort((a, b) => a.number - b.number);
  const byRound = new Map<DeliveryRound | null, DeliveryJob[]>([[null, []]]);
  for (const round of ordered) byRound.set(round, []);
  for (const job of jobs) byRound.get(roundOfJob(job, ordered))!.push(job);
  const initial = byRound.get(null)!;
  const lastEnd = Math.max(0, ...initial.map((job) => job.ended_at_ms ?? 0));
  const entries: HistoryEntry[] = [
    {
      round: null,
      rows: rowsOf(initial),
      from: initial[0]?.created_at_ms ?? null,
      to: ordered[0]?.opened_at_ms ?? (initial.some(isOpen) || lastEnd === 0 ? null : lastEnd),
    },
    ...ordered.map((round) => ({
      round,
      rows: rowsOf(byRound.get(round)!),
      from: round.opened_at_ms,
      to: round.closed_at_ms ?? null,
    })),
  ];
  return entries.reverse();
}

/** How the round's checks went: "2 de 3 pasan", red when one failed; what
 *  runs or waits says so first. */
export function checksOutcome(
  checks: DeliveryJob[],
  jobs: DeliveryJob[],
  verdicts: Record<string, DeliveryJobVerdict>,
): { tone: Tone; label: string } {
  const outcomes = checks.map((job) => attemptOutcome(job, jobs, verdicts));
  const going = outcomes.find((outcome) => outcome.tone === "live" || outcome.tone === "waiting");
  if (going) return going;
  if (checks.length === 1) return outcomes[0];
  const passed = outcomes.filter((outcome) => outcome.tone === "ok").length;
  const failed = outcomes.some((outcome) => outcome.tone === "danger");
  return {
    tone: failed ? "danger" : passed === checks.length ? "ok" : "idle",
    label: `${passed} de ${checks.length} pasan`,
  };
}

/** When something happened, as the history says it: "hoy 14:20",
 *  "ayer 18:05", then the day alone ("4/11"). */
export function whenLabel(ms: number, now: number): string {
  const at = new Date(ms);
  const day = (date: Date) =>
    new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(at)) / 86_400_000);
  const time = at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (days === 0) return `hoy ${time}`;
  if (days === 1) return `ayer ${time}`;
  return at.toLocaleDateString([], { day: "numeric", month: "numeric" });
}

/** How long a round took: "36 s", "48 min", "1 h 12 min", "2 h". */
export function spanLabel(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  if (minutes === 0) return `${Math.round(Math.max(0, ms) / 1000)} s`;
  if (minutes < 60) return `${minutes} min`;
  const rest = minutes % 60;
  return `${Math.floor(minutes / 60)} h${rest ? ` ${rest} min` : ""}`;
}
