import { useSyncExternalStore } from "react";
import type { AgentSession, AgentSessionChange, AgentSessionOutput } from "../bus/contract";

export interface AgentSessionState {
  sessions: Record<string, AgentSession>;
  output: Record<string, AgentSessionOutput[]>;
  outputTotal: Record<string, number>;
}

const EMPTY: AgentSessionState = {
  sessions: {},
  output: {},
  outputTotal: {},
};

const MAX_OUTPUT_CHUNKS_PER_SESSION = 20000;
const EMPTY_OUTPUT: AgentSessionOutput[] = [];
const outputSnapshotCache = new Map<
  string,
  { chunks: AgentSessionOutput[]; total: number; snapshot: AgentSessionOutputSnapshot }
>();

export interface AgentSessionOutputSnapshot {
  chunks: AgentSessionOutput[];
  total: number;
}

export class AgentSessionStore {
  private state: AgentSessionState = EMPTY;
  private listeners = new Set<() => void>();

  getState = (): AgentSessionState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(next: AgentSessionState) {
    this.state = next;
    this.listeners.forEach((listener) => listener());
  }

  setSessions(sessions: AgentSession[]) {
    const next: Record<string, AgentSession> = {};
    for (const session of sessions) next[session.id] = normalizeSession(session);
    this.set({ ...this.state, sessions: next });
  }

  upsertSession(session: AgentSession) {
    this.set({
      ...this.state,
      sessions: {
        ...this.state.sessions,
        [session.id]: normalizeSession(session),
      },
    });
  }

  applyChangeLog(sessionId: string, changes: AgentSessionChange[]) {
    const session = this.state.sessions[sessionId];
    if (!session) return;
    this.upsertSession({ ...session, change_log: changes });
  }

  appendOutput(output: AgentSessionOutput) {
    const current = this.state.output[output.session_id] ?? [];
    const currentTotal = this.state.outputTotal[output.session_id] ?? 0;
    const nextTotal = currentTotal + 1;
    this.set({
      ...this.state,
      output: {
        ...this.state.output,
        [output.session_id]: [...current, output].slice(-MAX_OUTPUT_CHUNKS_PER_SESSION),
      },
      outputTotal: {
        ...this.state.outputTotal,
        [output.session_id]: nextTotal,
      },
    });
  }

  reset() {
    outputSnapshotCache.clear();
    this.set(EMPTY);
  }
}

function normalizeSession(session: AgentSession): AgentSession {
  return {
    ...session,
    change_log: session.change_log ?? [],
    turn_status: session.turn_status ?? "waiting",
    turn_checkpoints: session.turn_checkpoints ?? [],
  };
}

export const agentSessionStore = new AgentSessionStore();

export function useAgentSessionState(): AgentSessionState {
  return useSyncExternalStore(agentSessionStore.subscribe, agentSessionStore.getState);
}

export function useAgentSession(sessionId: string): AgentSession | undefined {
  return useSyncExternalStore(
    agentSessionStore.subscribe,
    () => agentSessionStore.getState().sessions[sessionId],
    () => undefined,
  );
}

export function useAgentSessionOutput(sessionId: string): {
  chunks: AgentSessionOutput[];
  total: number;
} {
  return useSyncExternalStore(
    agentSessionStore.subscribe,
    () => outputSnapshot(sessionId),
    () => ({ chunks: EMPTY_OUTPUT, total: 0 }),
  );
}

function outputSnapshot(sessionId: string): AgentSessionOutputSnapshot {
  const state = agentSessionStore.getState();
  const chunks = state.output[sessionId] ?? EMPTY_OUTPUT;
  const total = state.outputTotal[sessionId] ?? 0;
  const cached = outputSnapshotCache.get(sessionId);
  if (cached?.chunks === chunks && cached.total === total) return cached.snapshot;
  const snapshot = { chunks, total };
  outputSnapshotCache.set(sessionId, { chunks, total, snapshot });
  return snapshot;
}
