import { useSyncExternalStore } from "react";
import type { AgentSession, AgentSessionChange } from "../bus/contract";

export interface AgentSessionState {
  sessions: Record<string, AgentSession>;
}

const EMPTY: AgentSessionState = {
  sessions: {},
};

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
    this.set({ sessions: next });
  }

  upsertSession(session: AgentSession) {
    this.set({
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

  reset() {
    this.set(EMPTY);
  }
}

function normalizeSession(session: AgentSession): AgentSession {
  return {
    ...session,
    change_log: session.change_log ?? [],
  };
}

export const agentSessionStore = new AgentSessionStore();

export function useAgentSessionState(): AgentSessionState {
  return useSyncExternalStore(agentSessionStore.subscribe, agentSessionStore.getState);
}

export function useAgentSession(sessionId: string): AgentSession | undefined {
  return useAgentSessionState().sessions[sessionId];
}
