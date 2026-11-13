// A batch's coordinator inside Tinto: a button at the end of the batch bar,
// with its state, that unfolds under the bar its Agents conversation
// (messages, files, everything Agents does),
// resumes it here once it ended, starts one when the batch has none here (for
// that batch and any others the user ticks: one conversation may coordinate
// several), stops new tasks, and detaches the conversation into its own window.

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { pendingRequests } from "../../agent/sessionSignal";
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
import { WorkspaceActionsContext, useWorkspaceActions } from "../../workspace/actions";

import { confirm } from "../../workbench/confirmDialog";
import { Icon } from "../../workbench/Icon";
import { ActionButton } from "./ActionButton";
import { errorText } from "./errorText";
import type { CoordinatorStall } from "./DeliveryNotifications";
import {
  agentChoice,
  effortChoices,
  modelChoices,
  withModel,
  type AgentChoice,
} from "./agentChoice";
import { batchName } from "./taskStatus";

type CoordinatorState = "asks" | "waiting" | "working" | "stalled" | "outside" | "none";

const STATE_LABEL: Record<CoordinatorState, string> = {
  asks: "te pide permiso",
  waiting: "esperando",
  working: "trabajando",
  stalled: "parado",
  outside: "fuera de Tinto",
  none: "sin coordinador",
};

/** A batch's state where the user picks the batches a new coordinator takes. */
const PICK_LABEL: Record<CoordinatorState, string> = {
  asks: "coordinado en Tinto",
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

/** The button, placed in the batch bar, and, when open, the sheet with the
 *  conversation under that bar. */
export function CoordinatorDock({
  runs,
  focus = null,
  request = null,
  overview,
  stalls,
  onHideStall,
  open,
  onOpenChange,
}: {
  /** The repo's active batches. */
  runs: DeliveryRun[];
  /** The batch shown until the user picks another; else the first. */
  focus?: string | null;
  /** An explicit request to show `focus` (a stopped coordinator opened from
   *  the queue): each new one, even for the same batch, drops the one picked. */
  request?: object | null;
  overview: DeliveryOverview;
  /** Batches whose coordinator stopped with changes unread (not hidden). */
  stalls: CoordinatorStall[];
  /** Hides a stop until the coordinator stops again. */
  onHideStall: (stall: CoordinatorStall) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const sessions = useAgentSessionState().sessions;
  const actions = useWorkspaceActions();
  // The batch the user picked, while what to show is still the same: a new
  // batch or request to show (the queue's stopped coordinator) wins over it.
  const shownFor: unknown = request ?? focus;
  const [chosenRun, setChosenRun] = useState<{ id: string; shownFor: unknown } | null>(null);
  const runId = chosenRun && chosenRun.shownFor === shownFor ? chosenRun.id : null;
  const run =
    runs.find((candidate) => candidate.id === (runId ?? focus)) ??
    runs.find((candidate) => candidate.id === focus) ??
    runs[0];
  // A conversation just started for some batches, until it takes them over.
  const [started, setStarted] = useState<{ runIds: string[]; sessionId: string } | null>(null);
  const handedOver = useRef<string | null>(null);
  // Conversations the user resumed here, by the id they ended with, until
  // the batch names the new one.
  const [resumed, setResumed] = useState<Record<string, string>>({});
  const [detached, setDetached] = useState<string | null>(null);
  const [choice, setChoice] = useState<AgentChoice | null>(null);
  // Full access: no sandbox and no permission prompts, as the user allows.
  const [fullAccess, setFullAccess] = useState<boolean | null>(null);
  // The batches a new coordinator takes; the one shown until the user ticks others.
  const [picked, setPicked] = useState<string[] | null>(null);
  // The conversation stays folded until the user opens it.
  const [chatShown, setChatShown] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The sheet opens right under the bar that holds the button, however many
  // lines the bar takes: its top follows the bar's bottom.
  const sheet = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const bar = sheet.current?.closest<HTMLElement>(".delivery-repo__bar");
    if (bar) sheet.current!.style.setProperty("--below", `${bar.offsetTop + bar.offsetHeight}px`);
  });

  // A conversation that ended coordinates nothing: the batch can start another.
  const ended = (id: string) => {
    const status = sessions[id]?.status;
    return status !== undefined && status !== "starting" && status !== "running";
  };
  const isLive = (id: string) => sessions[id] !== undefined && !ended(id);
  // A batch's Agents conversation here: its coordinator's, even once it
  // ended (the user resumes it), unless one was just started for it.
  const conversationOf = (target: DeliveryRun) => {
    const bound = target.coordinator_session
      ? (resumed[target.coordinator_session] ?? target.coordinator_session)
      : null;
    if (bound && isLive(bound)) return bound;
    if (started?.runIds.includes(target.id) && !ended(started.sessionId)) return started.sessionId;
    return bound;
  };
  // The conversation that coordinates a batch here now, if any.
  const sessionOf = (target: DeliveryRun) => {
    const id = conversationOf(target);
    return id && isLive(id) ? id : null;
  };
  const conversationId = run ? conversationOf(run) : null;
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

  // An ended conversation opens from the Agents history, as Agents resumes
  // it, whether it exited or failed, and loaded or not.
  const conversation = conversationId ? sessions[conversationId] : undefined;
  const fromHistory = conversationId !== null && conversationId !== sessionId;
  const conversationRepo = conversation?.repo ?? run?.repo;
  const conversationAgent = conversation?.agent_type ?? "agent";
  const params = useMemo(
    () =>
      conversationId && conversationRepo
        ? ({
            params: {
              sessionId: conversationId,
              repo: conversationRepo,
              agentType: conversationAgent,
              mode: fromHistory ? "journal" : "live",
            },
            api: drawerApi,
          } as IDockviewPanelProps<TerminalPanelParams>)
        : null,
    [conversationId, conversationRepo, conversationAgent, fromHistory],
  );
  // Resuming the conversation here keeps it here, not in an Agents tab.
  const drawerActions = useMemo(
    () => ({
      ...actions,
      openAgentTerminal: (target: Parameters<typeof actions.openAgentTerminal>[0]) => {
        const from = target.replaceSessionId;
        if (from && from === conversationId) {
          setResumed((current) => ({ ...current, [from]: target.sessionId }));
        } else {
          actions.openAgentTerminal(target);
        }
      },
    }),
    [actions, conversationId],
  );

  if (!run) return null;
  // Each batch's own state: stalls and waits are per batch, even when one
  // conversation holds several.
  const stateOf = (target: DeliveryRun): CoordinatorState => {
    const id = sessionOf(target);
    const live = id ? sessions[id] : undefined;
    const coordinator = (overview.coordinators ?? []).find((state) => state.run_id === target.id);
    const bound = conversationOf(target);
    // A permission it waits on holds the whole batch: its turn still reads
    // as working, so it is said before that.
    return stalls.some((stall) => stall.run.id === target.id) || (bound && !isLive(bound))
      ? "stalled"
      : live && pendingRequests(live).length > 0
        ? "asks"
        : live?.turn_status === "working"
          ? "working"
          : coordinator?.waiting || live
            ? "waiting"
            : target.owner && target.owner !== "user"
              ? "outside"
              : "none";
  };
  const state = stateOf(run);
  const stall = stalls.find((candidate) => candidate.run.id === run.id);
  const isDetached = session !== undefined && detached === session.id;
  // What the start form shows: what the user picked, else the last start in
  // this repo. Read against the models offered now, which may have loaded
  // since: a model the select cannot show would start with another one.
  const shown = agentChoice(choice ?? rememberedChoice(run.repo));
  const chosen = (picked ?? [run.id]).filter((id) => runs.some((candidate) => candidate.id === id));

  const privileged = fullAccess ?? rememberedFullAccess(run.repo);

  const start = async () => {
    setError(null);
    if (
      privileged &&
      !(await confirm(
        "El coordinador correrá sin sandbox ni confirmaciones: podrá ejecutar cualquier comando y cambiar archivos fuera del repositorio con tus permisos.",
        { title: "Coordinador con acceso completo", kind: "warning", okLabel: "Iniciar" },
      ))
    ) {
      return false;
    }
    const id = await startAgentSession(
      run.repo,
      shown.agent,
      privileged ? "full_access" : "workspace",
      runtimeOptions(shown),
    );
    rememberChoice(run.repo, shown, privileged);
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
  const who = conversation
    ? [
        AGENT_LABEL[conversation.agent_type] ?? conversation.agent_type,
        conversation.runtime_options?.model,
        conversation.permission_mode === "full_access" ? "acceso completo" : null,
      ]
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
        className={`delivery-button--quiet delivery-coordinator-button delivery-coordinator-button--${state}`}
        aria-expanded={open}
        aria-label={`Coordinador de ${batchName(run.title || run.id)}: ${STATE_LABEL[state]}`}
        onClick={toggle}
      >
        <span className="delivery-coordinator-button__dot" aria-hidden />
        Coordinador · {STATE_LABEL[state]}
      </button>
      {open && (
        <aside
          ref={sheet}
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
                    setChosenRun({ id: event.target.value, shownFor });
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
                <Icon name="x" />
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
          {stall && <StallNotice stall={stall} asked={request} onHide={() => onHideStall(stall)} />}
          {error && (
            <p className="delivery-coordinator__error" role="alert">
              {error}
            </p>
          )}
          {params && (
            <>
              <button
                type="button"
                className="delivery-coordinator__fold"
                aria-expanded={chatShown}
                onClick={() => setChatShown(!chatShown)}
              >
                <Icon name={chatShown ? "down" : "right"} />
                Conversación
              </button>
              {chatShown && (
                <div className="delivery-coordinator__chat">
                  <WorkspaceActionsContext.Provider value={drawerActions}>
                    <TerminalPanel {...params} />
                  </WorkspaceActionsContext.Provider>
                </div>
              )}
            </>
          )}
          {/* An ended conversation is resumed from the panel; folded, the
              user may start another instead. */}
          {!session && !chatOpen && (
            <div className="delivery-coordinator__start">
              <p className="delivery-muted">
                {params
                  ? "Su conversación terminó: ábrela y escríbele para retomarla con su contexto, o inicia otro coordinador."
                  : runs.length > 1
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
                {/* Under the three choices: the access on the left, the start on the right. */}
                <div className="delivery-coordinator__go">
                  <label title="Sin sandbox ni confirmaciones de permisos: puede ejecutar cualquier comando con tus permisos.">
                    <input
                      type="checkbox"
                      checked={privileged}
                      onChange={(event) => setFullAccess(event.target.checked)}
                    />
                    Acceso completo
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
            </div>
          )}
        </aside>
      )}
    </>
  );
}

/** Why a stopped coordinator holds its batch and what to tell it: the stop's
 *  one home, where the queue's "Ver" opens. Hidden, it comes back if it stops again. */
function StallNotice({
  stall,
  asked,
  onHide,
}: {
  stall: CoordinatorStall;
  /** The request that opened the sheet on this stop, if one did. */
  asked: object | null;
  onHide: () => void;
}) {
  // Opened from the queue, the notice takes the focus, so Tab goes on to its
  // actions; once per request.
  const notice = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (asked) notice.current?.focus();
  }, [asked]);
  const { run, unread, since } = stall;
  const changes = unread === 1 ? "1 cambio" : `${unread} cambios`;
  const at = new Date(since).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const message = `Sigue con wait_run de Tinto Delivery: tienes ${changes} sin leer desde las ${at}. Si las herramientas de Delivery fallan, reconéctalas.`;
  return (
    <div
      ref={notice}
      tabIndex={-1}
      className="delivery-coordinator__stall"
      role="group"
      aria-label="Coordinador parado"
    >
      <p>
        {run.owner && run.owner !== "user" ? run.owner : "Su coordinador"} no está esperando: tiene{" "}
        {changes} sin leer desde las {at}. Pídele que siga con wait_run; si sus herramientas de
        Delivery fallan, que las reconecte.
      </p>
      <div className="delivery-actions">
        <button
          type="button"
          onClick={() => void navigator.clipboard?.writeText(message).catch(() => {})}
        >
          Copiar mensaje
        </button>
        <button type="button" title="Vuelve a avisar si se para otra vez" onClick={onHide}>
          Ocultar aviso
        </button>
      </div>
    </div>
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

/** Whether the coordinator last started for a repo had full access. */
function rememberedFullAccess(repo: string): boolean {
  try {
    const saved = JSON.parse(localStorage.getItem(CHOICE_KEY) ?? "{}") as Record<
      string,
      { full_access?: boolean }
    >;
    return saved[repo]?.full_access === true;
  } catch {
    return false;
  }
}

function rememberChoice(repo: string, choice: AgentChoice, fullAccess: boolean) {
  try {
    const saved = JSON.parse(localStorage.getItem(CHOICE_KEY) ?? "{}") as Record<
      string,
      DeliveryStageAgent & { full_access?: boolean }
    >;
    saved[repo] = {
      agent: choice.agent,
      model: choice.model,
      reasoning: choice.reasoning,
      full_access: fullAccess,
    };
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
