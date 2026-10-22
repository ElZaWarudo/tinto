// Restarting or updating Tinto without stopping the work: Delivery jobs keep
// running while it is closed and the live Agents conversations resume when it
// opens again.

import { invoke } from "@tauri-apps/api/core";

/** A Delivery job that keeps running while Tinto is closed. */
export interface RestartJob {
  task_key: string;
  role: string;
  agent: string;
}

/** An Agents conversation that Tinto resumes when it opens again. */
export interface RestartConversation {
  session_id: string;
  repo: string;
  agent_type: string;
  /** In the middle of a turn: it is told to go on. */
  working: boolean;
}

export interface RestartPlan {
  jobs: RestartJob[];
  conversations: RestartConversation[];
}

/** A conversation Tinto resumed when it started. */
export interface RestoredAgentSession {
  session_id: string;
  repo: string;
  agent_type: string;
  /** It coordinates a Delivery batch: the Delivery view shows it, not a tab. */
  coordinator: boolean;
}

export const getRestartPlan = () => invoke<RestartPlan>("restart_plan");

/** Closes Tinto and opens it again, or runs `installer`, which opens it once done. */
export const restartTinto = (installer?: string) =>
  invoke<void>("restart_tinto", { installer: installer ?? null });

/** The conversations that were live when Tinto last closed, resumed; taken once. */
export const restoreLiveAgentSessions = () =>
  invoke<RestoredAgentSession[]>("restore_live_agent_sessions");

/** Resumes the last session's live conversations and opens a tab for each but the coordinators. */
export async function openRestoredConversations(
  openConversation: (params: { sessionId: string; repo: string; agentType: string }) => void,
): Promise<void> {
  const restored = (await restoreLiveAgentSessions()) ?? [];
  restored
    .filter((session) => !session.coordinator)
    .forEach((session) =>
      openConversation({
        sessionId: session.session_id,
        repo: session.repo,
        agentType: session.agent_type,
      }),
    );
}
