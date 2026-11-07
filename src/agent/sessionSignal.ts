// One state per live agent session, the same for every provider: it works,
// it needs you (a permission or approval waits on the user) or its turn is
// done and it waits for the next prompt.

import type { AgentSession } from "../bus/contract";

export type AgentSessionSignal = "working" | "needs_you" | "done";

export const SIGNAL_LABEL: Record<AgentSessionSignal, string> = {
  needs_you: "Te necesita",
  working: "Trabajando",
  done: "Listo",
};

/** What waits on the user: pending permissions and sub-agent approvals, by id. */
export function pendingRequests(session: AgentSession): string[] {
  const permissions = (session.acp_permissions ?? [])
    .filter((permission) => permission.state === "pending")
    .map((permission) => permission.id);
  const approvals = (session.subagents ?? []).flatMap((thread) =>
    thread.approval_request_id ? [thread.approval_request_id] : [],
  );
  return [...permissions, ...approvals];
}

/** `null` for an ended session, or a plain terminal whose turns Tinto cannot see. */
export function sessionSignal(session: AgentSession): AgentSessionSignal | null {
  if (session.status !== "running" && session.status !== "starting") return null;
  if (pendingRequests(session).length > 0) return "needs_you";
  if (session.status === "starting" || session.turn_status === "working") return "working";
  return session.reports_turns ? "done" : null;
}

const PRIORITY: AgentSessionSignal[] = ["needs_you", "working", "done"];

/** The repo's most urgent signal and how many of its sessions have it. */
export function repoSignal(
  sessions: Iterable<AgentSession>,
  repo: string,
): { signal: AgentSessionSignal; count: number } | null {
  const signals = [...sessions]
    .filter((session) => session.repo === repo)
    .map(sessionSignal)
    .filter((signal): signal is AgentSessionSignal => signal !== null);
  for (const signal of PRIORITY) {
    const count = signals.filter((candidate) => candidate === signal).length;
    if (count > 0) return { signal, count };
  }
  return null;
}
