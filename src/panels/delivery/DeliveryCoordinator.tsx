// A batch's coordinator inside Tinto: a floating button with its state that
// unfolds its Agents conversation (messages, files, everything Agents does),
// starts one when the batch has none here (for that batch and any others the
// user ticks: one conversation may coordinate several), stops new tasks, and
// detaches the conversation into its own window.

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
import { errorText } from "./errorText";
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

/** A batch's state where the user picks the batches a new coordinator takes. */
const PICK_LABEL: Record<CoordinatorState, string> = {
  waiting: "coordinado en Tinto",
  working: "coordinado en Tinto",
  stalled: "parado",
  outside: "coordinado fuera de Tinto",
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
  focus = null,
  overview,
  stalled,
  open,
  onOpenChange,
}: {
  /** The repo's active batches. */
  runs: DeliveryRun[];
  /** The batch shown until the user picks another; else the first. */
  focus?: string | null;
  overview: DeliveryOverview;
  /** Batches whose coordinator stopped with changes unread. */
  stalled: string[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const sessions = useAgentSessionState().sessions;
  const [runId, setRunId] = useState<string | null>(null);
  const run =
    runs.find((candidate) => candidate.id === (runId ?? focus)) ??
    runs.find((candidate) => candidate.id === focus) ??
    runs[0];
  // A conversation just started for some batches, until it takes them over.
  const [started, setStarted] = useState<{ runIds: string[]; sessionId: string } | null>(null);
  const handedOver = useRef<string | null>(null);
  const [detached, setDetached] = useState<string | null>(null);
  const [choice, setChoice] = useState<AgentChoice | null>(null);
  // The batches a new coordinator takes; the one shown until the user ticks others.
  const [picked, setPicked] = useState<string[] | null>(null);
  // The conversation stays folded until the user opens it.
  const [chatShown, setChatShown] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // A conversation that ended coordinates nothing: the batch can start another.
  const ended = (id: string) => {
    const status = sessions[id]?.status;
    return status !== undefined && status !== "starting" && status !== "running";
  };
  // The Agents conversation that coordinates a batch here, if any.
  const sessionOf = (target: DeliveryRun) =>
    (target.coordinator_session &&
    sessions[target.coordinator_session] &&
    !ended(target.coordinator_session)
      ? target.coordinator_session
      : null) ??
    (started?.runIds.includes(target.id) && !ended(started.sessionId) ? started.sessionId : null);
  const sessionId = run ? sessionOf(run) : null;
  const session = sessionId ? sessions[sessionId] : undefined;
  // Every batch that conversation coordinates.
  const held = sessionId ? runs.filter((candidate) => sessionOf(candidate) === sessionId) : [];

  // Once the new conversation waits for its first message, Tinto asks it to
  // take the batches over. A conversation that does not report its turns yet
  // (a terminal still starting) could swallow the message: Tinto waits.
  useEffect(() => {
    if (!started || handedOver.current === started.sessionId) return;
    const live = sessions[started.sessionId];
    if (live?.status !== "running" || !live.reports_turns || live.turn_status !== "waiting") {
      return;
    }
    handedOver.current = started.sessionId;
    handOverDeliveryRun(started.runIds, started.sessionId).catch((cause: unknown) =>
      setError(errorText(cause)),
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
  // Each batch's own state: stalls and waits are per batch, even when one
  // conversation holds several.
  const stateOf = (target: DeliveryRun): CoordinatorState => {
    const id = sessionOf(target);
    const live = id ? sessions[id] : undefined;
    const coordinator = (overview.coordinators ?? []).find((state) => state.run_id === target.id);
    return stalled.includes(target.id)
      ? "stalled"
      : live?.turn_status === "working"
        ? "working"
        : coordinator?.waiting || live
          ? "waiting"
          : target.owner &&
              target.owner !== "user" &&
              !(target.coordinator_session && ended(target.coordinator_session))
            ? "outside"
            : "none";
  };
  const state = stateOf(run);
  const isDetached = session !== undefined && detached === session.id;
  // What the start form shows: what the user picked, else the last start in
  // this repo. Read against the models offered now, which may have loaded
  // since: a model the select cannot show would start with another one.
  const shown = agentChoice(choice ?? rememberedChoice(run.repo));
  const chosen = (picked ?? [run.id]).filter((id) => runs.some((candidate) => candidate.id === id));

  const start = async () => {
    setError(null);
    const id = await startAgentSession(run.repo, shown.agent, "workspace", runtimeOptions(shown));
    rememberChoice(run.repo, shown);
    setStarted({ runIds: chosen, sessionId: id });
    setPicked(null);
  };

  const pick = (id: string, on: boolean) =>
    setPicked(on ? [...chosen, id] : chosen.filter((candidate) => candidate !== id));

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
                  onChange={(event) => {
                    setRunId(event.target.value);
                    setPicked(null);
                  }}
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
            {held.length > 1 && (
              <p className="delivery-coordinator__held">
                Esta conversación coordina {held.length} lotes:{" "}
                {held.map((candidate) => batchName(candidate.title || candidate.id)).join(" · ")}
              </p>
            )}
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
                {runs.length > 1
                  ? "Inicia aquí un coordinador: tomará los lotes que marques y seguirá con sus tareas, todos en una conversación."
                  : state === "outside"
                    ? "Puedes seguir así o iniciar aquí un coordinador que tome el lote."
                    : "Inicia aquí un coordinador: tomará el lote y seguirá con sus tareas."}
              </p>
              {runs.length > 1 && (
                <fieldset className="delivery-coordinator__batches">
                  <legend>Lotes que toma</legend>
                  {runs.map((candidate) => (
                    <label key={candidate.id}>
                      <input
                        type="checkbox"
                        checked={chosen.includes(candidate.id)}
                        onChange={(event) => pick(candidate.id, event.target.checked)}
                      />
                      <span>{batchName(candidate.title || candidate.id)}</span>
                      <span className="delivery-muted">{PICK_LABEL[stateOf(candidate)]}</span>
                    </label>
                  ))}
                </fieldset>
              )}
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
                <ActionButton
                  className="delivery-button--primary"
                  disabled={chosen.length === 0}
                  onAction={start}
                >
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
