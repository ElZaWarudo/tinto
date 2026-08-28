import { useSyncExternalStore } from "react";
import type {
  AgentSession,
  AgentSessionChange,
  AgentSessionOutput,
  AgentSessionTimelineItem,
  AgentSubagentThread,
} from "../bus/contract";

export interface AgentSessionState {
  sessions: Record<string, AgentSession>;
  output: Record<string, AgentSessionOutput[]>;
  outputTotal: Record<string, number>;
  timeline: Record<string, AgentSessionTimelineItem[]>;
}

const EMPTY: AgentSessionState = {
  sessions: {},
  output: {},
  outputTotal: {},
  timeline: {},
};

const MAX_OUTPUT_CHUNKS_PER_SESSION = 20000;
const MAX_TIMELINE_ITEMS_PER_SESSION = 2000;
const EMPTY_OUTPUT: AgentSessionOutput[] = [];
const EMPTY_TIMELINE: AgentSessionTimelineItem[] = [];
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
    let timeline = this.state.timeline;
    for (const session of sessions) {
      const normalized = normalizeSession(session);
      next[session.id] = normalized;
      timeline = mergeSessionTimeline(timeline, normalized);
    }
    this.set({ ...this.state, sessions: next, timeline });
  }

  upsertSession(session: AgentSession) {
    const normalized = normalizeSession(session);
    this.set({
      ...this.state,
      sessions: {
        ...this.state.sessions,
        [session.id]: normalized,
      },
      timeline: mergeSessionTimeline(this.state.timeline, normalized),
    });
  }

  removeSession(sessionId: string) {
    const sessions = { ...this.state.sessions };
    const output = { ...this.state.output };
    const outputTotal = { ...this.state.outputTotal };
    const timeline = { ...this.state.timeline };
    delete sessions[sessionId];
    delete output[sessionId];
    delete outputTotal[sessionId];
    delete timeline[sessionId];
    outputSnapshotCache.delete(sessionId);
    this.set({ sessions, output, outputTotal, timeline });
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

  appendTimelineItem(item: AgentSessionTimelineItem) {
    const current = this.state.timeline[item.session_id] ?? [];
    if (current.some((existing) => existing.id === item.id)) return;
    this.set({
      ...this.state,
      timeline: {
        ...this.state.timeline,
        [item.session_id]: [...current, item].slice(-MAX_TIMELINE_ITEMS_PER_SESSION),
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
    timeline: session.timeline ?? [],
    subagents: (session.subagents ?? []).map(normalizeSubagent),
  };
}

function normalizeSubagent(thread: AgentSubagentThread): AgentSubagentThread {
  return {
    ...thread,
    parent_id: thread.parent_id ?? null,
    agent_path: thread.agent_path ?? [],
    activities: (thread.activities ?? []).slice(-2000),
    timeline: (thread.timeline ?? []).slice(-2000),
    capabilities: { ...thread.capabilities },
    result: thread.result ?? null,
  };
}

function mergeSessionTimeline(
  stateTimeline: Record<string, AgentSessionTimelineItem[]>,
  session: AgentSession,
): Record<string, AgentSessionTimelineItem[]> {
  if (!session.timeline || session.timeline.length === 0) return stateTimeline;
  const current = stateTimeline[session.id] ?? [];
  const seen = new Set(current.map((item) => item.id));
  const merged = [...current];
  for (const item of session.timeline) {
    if (seen.has(item.id)) continue;
    seen.add(item.id);
    merged.push(item);
  }
  const nextItems = merged
    .sort((a, b) => a.timestamp_ms - b.timestamp_ms || a.id.localeCompare(b.id))
    .slice(-MAX_TIMELINE_ITEMS_PER_SESSION);
  if (nextItems === current) return stateTimeline;
  return {
    ...stateTimeline,
    [session.id]: nextItems,
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

export function useAgentSessionTimeline(sessionId: string): AgentSessionTimelineItem[] {
  return useSyncExternalStore(
    agentSessionStore.subscribe,
    () => agentSessionStore.getState().timeline[sessionId] ?? EMPTY_TIMELINE,
    () => EMPTY_TIMELINE,
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
