// A batch's coordinator inside Tinto: a floating button with its state that
// unfolds its Agents conversation (messages, files, everything Agents does),
// starts one when the batch has none here, stops new tasks, and detaches
// the conversation into its own window.

import { useEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { useAgentSessionState } from "../../agent/sessionStore";
import { startAgentSession } from "../../bus/client";
import { handOverDeliveryRun, setDeliveryRunNoNewTasks } from "../../delivery/client";
import type { AgentSessionRuntimeOptions } from "../../bus/contract";
import type {
  DeliveryAgent,
  DeliveryOverview,
  DeliveryRun,
  DeliveryStageAgent,
} from "../../delivery/types";
import { TerminalPanel, type TerminalPanelParams } from "../terminal/TerminalPanel";
import { openDetachedTerminalWindow } from "../terminal/detachTerminalWindow";
import { lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";

import { ActionButton } from "./ActionButton";
import {
  agentChoice,
  effortChoices,
  modelChoices,
  withModel,
  type AgentChoice,
} from "./agentChoice";
import { batchName } from "./taskStatus";

type CoordinatorState = "waiting" | "working" | "stalled" | "outside" | "none";

const STATE_LABEL: Record<CoordinatorState, string> = {
  waiting: "esperando",
  working: "trabajando",
  stalled: "parado",
  outside: "fuera de Tinto",
  none: "sin coordinador",
};

const AGENT_LABEL: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  muse: "Muse Code",
};

/** The coordinator last started in each repo, by repo path. */
const CHOICE_KEY = "tinto:delivery-coordinator-agent";

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
  const [choice, setChoice] = useState<AgentChoice | null>(null);
  // The conversation stays folded until the user opens it.
  const [chatShown, setChatShown] = useState(false);
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
  // What the start form shows: what the user picked, else the last start in this repo.
  const shown = choice ?? rememberedChoice(run.repo);

  const start = async () => {
    setError(null);
    const id = await startAgentSession(run.repo, shown.agent, "workspace", runtimeOptions(shown));
    rememberChoice(run.repo, shown);
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

  // Who coordinates, after its state: the conversation's agent here, the
  // owner's name outside Tinto.
  const who = session
    ? [AGENT_LABEL[session.agent_type] ?? session.agent_type, session.runtime_options?.model]
        .filter(Boolean)
        .join(" · ")
    : state === "outside"
      ? run.owner
      : null;
  const models = modelChoices(shown.agent);
  const efforts = effortChoices(shown.agent, shown.model);
  const chatOpen = params !== null && chatShown;

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
        <aside
          className={`delivery-coordinator${chatOpen ? " delivery-coordinator--chat" : ""}`}
          aria-label="Coordinador del lote"
        >
          <header className="delivery-coordinator__header">
            <div className="delivery-coordinator__title">
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
            </div>
            <div className="delivery-coordinator__status">
              <span
                className={`delivery-coordinator-button__dot delivery-coordinator-button--${state}`}
                aria-hidden
              />
              <span>{STATE_LABEL[state][0].toUpperCase() + STATE_LABEL[state].slice(1)}</span>
              {who && <span className="delivery-muted">· {who}</span>}
              <span className="delivery-panel__spacer" />
              <ActionButton
                role="switch"
                className="delivery-switch"
                aria-checked={!run.no_new_tasks}
                aria-label="Crear tareas nuevas"
                title={
                  run.no_new_tasks
                    ? "Apagado: el coordinador termina las tareas que tiene y no crea más"
                    : "Encendido: el coordinador puede crear tareas nuevas en este lote"
                }
                onAction={() => setDeliveryRunNoNewTasks(run.id, !run.no_new_tasks)}
              >
                <span className="delivery-switch__track" aria-hidden />
                Tareas nuevas
              </ActionButton>
            </div>
          </header>
          {error && (
            <p className="delivery-coordinator__error" role="alert">
              {error}
            </p>
          )}
          {params ? (
            <>
              <button
                type="button"
                className="delivery-coordinator__fold"
                aria-expanded={chatShown}
                onClick={() => setChatShown(!chatShown)}
              >
                <span aria-hidden>{chatShown ? "▾" : "▸"}</span>
                Conversación
              </button>
              {chatShown && (
                <div className="delivery-coordinator__chat">
                  <TerminalPanel {...params} />
                </div>
              )}
            </>
          ) : (
            <div className="delivery-coordinator__start">
              <p className="delivery-muted">
                {state === "outside"
                  ? "Puedes seguir así o iniciar aquí un coordinador que tome el lote."
                  : "Inicia aquí un coordinador: tomará el lote y seguirá con sus tareas."}
              </p>
              <div className="delivery-coordinator__pick">
                <label>
                  Agente
                  <select
                    value={shown.agent}
                    onChange={(event) =>
                      setChoice(
                        agentChoice({
                          agent: event.target.value as DeliveryAgent,
                          model: null,
                          reasoning: null,
                        }),
                      )
                    }
                  >
                    {Object.entries(AGENT_LABEL).map(([value, label]) => (
                      <option key={value} value={value}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Modelo
                  <select
                    value={shown.model}
                    onChange={(event) => setChoice(withModel(shown, event.target.value))}
                  >
                    {models.choices.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Razonamiento
                  <select
                    value={shown.reasoning}
                    onChange={(event) => setChoice({ ...shown, reasoning: event.target.value })}
                  >
                    {efforts.choices.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <ActionButton className="delivery-button--primary" onAction={start}>
                  Iniciar coordinador
                </ActionButton>
              </div>
            </div>
          )}
        </aside>
      )}
    </>
  );
}

/** The coordinator last started for a repo, to start the next one the same way. */
function rememberedChoice(repo: string): AgentChoice {
  try {
    const saved = JSON.parse(localStorage.getItem(CHOICE_KEY) ?? "{}") as Record<
      string,
      DeliveryStageAgent
    >;
    if (saved[repo]) return agentChoice(saved[repo]);
  } catch {
    // Unreadable storage starts on the defaults.
  }
  return agentChoice({ agent: "claude", model: null, reasoning: null });
}

function rememberChoice(repo: string, choice: AgentChoice) {
  try {
    const saved = JSON.parse(localStorage.getItem(CHOICE_KEY) ?? "{}") as Record<
      string,
      DeliveryStageAgent
    >;
    saved[repo] = { agent: choice.agent, model: choice.model, reasoning: choice.reasoning };
    localStorage.setItem(CHOICE_KEY, JSON.stringify(saved));
  } catch {
    // Not remembered: the next start offers the defaults.
  }
}

/** What the conversation starts on: Codex and Muse name a model by the
 *  catalog's id; "" is the agent's own default. */
function runtimeOptions(choice: AgentChoice): AgentSessionRuntimeOptions {
  const options: AgentSessionRuntimeOptions = {};
  if (choice.model) {
    const catalog =
      choice.agent === "codex" || choice.agent === "muse" ? lastRuntimeCatalog(choice.agent) : null;
    options.model =
      catalog?.models?.find((model) => model.model === choice.model)?.id ?? choice.model;
  }
  if (choice.reasoning) options.reasoning_effort = choice.reasoning;
  return options;
}
