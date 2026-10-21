// A batch's coordinator inside Tinto: a floating button with its state that
// unfolds its Agents conversation (messages, files, everything Agents does),
// starts one when the batch has none here, stops new tasks, and detaches
// the conversation into its own window.

import { useEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { useAgentSessionState } from "../../agent/sessionStore";
import { startAgentSession } from "../../bus/client";
import { handOverDeliveryRun, setDeliveryRunNoNewTasks } from "../../delivery/client";
import type { DeliveryOverview, DeliveryRun } from "../../delivery/types";
import { TerminalPanel, type TerminalPanelParams } from "../terminal/TerminalPanel";
import { openDetachedTerminalWindow } from "../terminal/detachTerminalWindow";

import { ActionButton } from "./ActionButton";
import { batchName } from "./taskStatus";

type CoordinatorState = "waiting" | "working" | "stalled" | "outside" | "none";

const STATE_LABEL: Record<CoordinatorState, string> = {
  waiting: "esperando",
  working: "trabajando",
  stalled: "parado",
  outside: "fuera de Tinto",
  none: "sin coordinador",
};

// The conversation view outside the dock: always active.
const drawerApi = {
  get isActive() {
    return true;
  },
  setActive: () => {},
  onDidActiveChange: () => ({ dispose: () => {} }),
} as unknown as IDockviewPanelProps<TerminalPanelParams>["api"];

/** The floating button and, when open, the drawer with the conversation. */
export function CoordinatorDock({
  runs,
  overview,
  stalled,
  open,
  onOpenChange,
}: {
  /** The repo's active batches; the first is shown unless one is chosen. */
  runs: DeliveryRun[];
  overview: DeliveryOverview;
  /** Batches whose coordinator stopped with changes unread. */
  stalled: string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const sessions = useAgentSessionState().sessions;
  const [runId, setRunId] = useState<string | null>(null);
  const run = runs.find((candidate) => candidate.id === runId) ?? runs[0];
  // A conversation just started for a batch, until it takes it over.
  const [started, setStarted] = useState<{ runId: string; sessionId: string } | null>(null);
  const handedOver = useRef<string | null>(null);
  const [detached, setDetached] = useState<string | null>(null);
  const [agentType, setAgentType] = useState("claude");
  const [error, setError] = useState<string | null>(null);

  const sessionId =
    (run?.coordinator_session && sessions[run.coordinator_session]
      ? run.coordinator_session
      : null) ?? (started && started.runId === run?.id ? started.sessionId : null);
  const session = sessionId ? sessions[sessionId] : undefined;

  // Once the new conversation waits for its first message, Tinto asks it to
  // take the batch over.
  useEffect(() => {
    if (!started || handedOver.current === started.sessionId) return;
    const live = sessions[started.sessionId];
    if (live?.status !== "running" || live.turn_status !== "waiting") return;
    handedOver.current = started.sessionId;
    handOverDeliveryRun(started.runId, started.sessionId).catch((cause: unknown) =>
      setError(cause instanceof Error ? cause.message : String(cause)),
    );
  }, [started, sessions]);

  const params = useMemo(
    () =>
      session
        ? ({
            params: { sessionId: session.id, repo: session.repo, agentType: session.agent_type },
            api: drawerApi,
          } as IDockviewPanelProps<TerminalPanelParams>)
        : null,
    [session],
  );

  if (!run) return null;
  const coordinator = (overview.coordinators ?? []).find((state) => state.run_id === run.id);
  const state: CoordinatorState = stalled.includes(run.id)
    ? "stalled"
    : session?.turn_status === "working"
      ? "working"
      : coordinator?.waiting
        ? "waiting"
        : session
          ? "waiting"
          : run.owner && run.owner !== "user"
            ? "outside"
            : "none";
  const isDetached = session !== undefined && detached === session.id;

  const start = async () => {
    setError(null);
    const id = await startAgentSession(run.repo, agentType, "workspace");
    setStarted({ runId: run.id, sessionId: id });
  };

  const detach = async () => {
    if (!session) return;
    const opened = await openDetachedTerminalWindow(
      { sessionId: session.id, repo: session.repo, agentType: session.agent_type },
      () => setDetached((current) => (current === session.id ? null : current)),
    );
    if (!opened) return;
    setDetached(session.id);
    onOpenChange(false);
  };

  const toggle = () => {
    // A conversation in its own window comes forward instead.
    if (isDetached && session) {
      void openDetachedTerminalWindow({
        sessionId: session.id,
        repo: session.repo,
        agentType: session.agent_type,
      });
      return;
    }
    onOpenChange(!open);
  };

  return (
    <>
      <button
        type="button"
        className={`delivery-coordinator-button delivery-coordinator-button--${state}`}
        aria-expanded={open}
        aria-label={`Coordinador de ${batchName(run.title || run.id)}: ${STATE_LABEL[state]}`}
        onClick={toggle}
      >
        <span className="delivery-coordinator-button__dot" aria-hidden />
        Coordinador · {STATE_LABEL[state]}
      </button>
      {open && (
        <aside className="delivery-coordinator" aria-label="Coordinador del lote">
          <header className="delivery-coordinator__header">
            {runs.length > 1 ? (
              <select
                aria-label="Lote"
                value={run.id}
                onChange={(event) => setRunId(event.target.value)}
              >
                {runs.map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {batchName(candidate.title || candidate.id)}
                  </option>
                ))}
              </select>
            ) : (
              <strong>{batchName(run.title || run.id)}</strong>
            )}
            <span className="delivery-muted">{STATE_LABEL[state]}</span>
            <span className="delivery-panel__spacer" />
            <ActionButton
              aria-pressed={run.no_new_tasks ?? false}
              title={
                run.no_new_tasks
                  ? "El coordinador puede volver a crear tareas en este lote"
                  : "El coordinador termina las tareas que tiene y no crea más"
              }
              onAction={() => setDeliveryRunNoNewTasks(run.id, !run.no_new_tasks)}
            >
              {run.no_new_tasks ? "Permitir tareas nuevas" : "No crear más tareas"}
            </ActionButton>
            {session && (
              <button
                type="button"
                className="delivery-icon-button"
                aria-label="Abrir el coordinador en otra ventana"
                title="Abrir en otra ventana"
                onClick={() => void detach()}
              >
                ↗
              </button>
            )}
            <button
              type="button"
              className="delivery-icon-button"
              aria-label="Cerrar el coordinador"
              onClick={() => onOpenChange(false)}
            >
              ×
            </button>
          </header>
          {error && (
            <p className="delivery-coordinator__error" role="alert">
              {error}
            </p>
          )}
          {params ? (
            <div className="delivery-coordinator__chat">
              <TerminalPanel {...params} />
            </div>
          ) : (
            <div className="delivery-coordinator__start">
              <p>
                {state === "outside"
                  ? `Lo coordina ${run.owner} fuera de Tinto. Puedes seguir así, o iniciar aquí un coordinador que tome el lote.`
                  : "Este lote no tiene coordinador. Inicia uno aquí: tomará el lote y seguirá con sus tareas."}
              </p>
              <label>
                Agente
                <select value={agentType} onChange={(event) => setAgentType(event.target.value)}>
                  <option value="claude">Claude Code</option>
                  <option value="codex">Codex</option>
                </select>
              </label>
              <ActionButton className="delivery-button--primary" onAction={start}>
                Iniciar coordinador
              </ActionButton>
            </div>
          )}
        </aside>
      )}
    </>
  );
}
