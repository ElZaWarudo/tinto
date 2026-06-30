import { useEffect, useMemo, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { confirm } from "@tauri-apps/plugin-dialog";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  getAgentJournalSession,
  listAgentSessions,
  revertSession,
  revertSessionTurnFile,
  stopAgentSession,
  writeAgentSessionInput,
} from "../../bus/client";
import {
  agentSessionStore,
  useAgentSession,
  useAgentSessionOutput,
  useAgentSessionTimeline,
} from "../../agent/sessionStore";
import type {
  AgentSession,
  AgentSessionOutput,
  AgentSessionTimelineItem,
} from "../../bus/contract";
import codexLogo from "../../assets/agents/codex.svg";
import claudeLogo from "../../assets/agents/claude.svg";
import opencodeLogo from "../../assets/agents/opencode.svg";
import { consumeTerminalDetachedMarker } from "./detachTerminalWindow";

export interface TerminalPanelParams {
  sessionId: string;
  repo?: string;
  agentType?: string;
  mode?: "live" | "journal";
}

type TerminalPanelProps = IDockviewPanelProps<TerminalPanelParams>;

const panelCloseStopTimers = new Map<string, number>();
const PANEL_CLOSE_STOP_DELAY_MS = 250;
const DETACHED_TRANSFER_STOP_DELAY_MS = 5000;

const AGENT_QUICK_ACTIONS = [
  {
    id: "plan",
    label: "Plan",
    title: "Draft a plan for the next turn",
    prompt: "Create a concise implementation plan for the next change before editing.",
  },
  {
    id: "review",
    label: "Review",
    title: "Ask for a focused review",
    prompt: "Review the current changes and call out concrete bugs, regressions, or missing tests.",
  },
  {
    id: "test",
    label: "Test",
    title: "Ask for verification",
    prompt:
      "Run the most relevant verification for this repo and summarize failures before fixing them.",
  },
  {
    id: "handoff",
    label: "Handoff",
    title: "Ask for a handoff summary",
    prompt:
      "Summarize the current session state, changed files, verification, and next recommended step.",
  },
] as const;

const FOCUSED_TURN_ACTIONS = [
  { id: "continue", label: "Continue" },
  { id: "review", label: "Review" },
  { id: "test", label: "Test" },
  { id: "handoff", label: "Handoff" },
] as const;

type FocusedTurnAction = (typeof FOCUSED_TURN_ACTIONS)[number]["id"];

interface AgentTurnView {
  id: string;
  index: number;
  startedAtMs: number | null;
  updatedAtMs: number | null;
  userText: string | null;
  agentText: string[];
  commandText: string[];
  systemText: string[];
  changes: Array<{ path: string; kind: string }>;
}

interface AgentSessionOverviewView {
  turns: number;
  messages: number;
  commands: number;
  files: number;
  latest: string | null;
  turnMap: Array<{
    id: string;
    index: number;
    commands: number;
    files: number;
    active: boolean;
    timeLabel: string | null;
  }>;
}

export function TerminalPanel({ params }: TerminalPanelProps) {
  const sessionId = params?.sessionId ?? "";
  const repo = params?.repo;
  const agentType = params?.agentType ?? "agent";
  const mode = params?.mode ?? "live";
  const readOnly = mode === "journal";
  const session = useAgentSession(sessionId);
  const { chunks: sessionOutput } = useAgentSessionOutput(sessionId);
  const timeline = useAgentSessionTimeline(sessionId);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [transcriptQuery, setTranscriptQuery] = useState("");
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [reverting, setReverting] = useState(false);
  const [revertingFile, setRevertingFile] = useState<string | null>(null);
  const [focusedTurnIndex, setFocusedTurnIndex] = useState<number | null>(null);

  const turns = useMemo(
    () => agentTurns(timeline, sessionOutput, session),
    [session, sessionOutput, timeline],
  );
  const overview = useMemo(() => agentSessionOverview(turns), [turns]);
  const visibleTurns = useMemo(
    () => filterAgentTurns(turns, transcriptQuery),
    [transcriptQuery, turns],
  );
  const focusedTurn = useMemo(
    () => turns.find((turn) => turn.index === focusedTurnIndex) ?? turns[turns.length - 1] ?? null,
    [focusedTurnIndex, turns],
  );
  const hasTranscriptQuery = transcriptQuery.trim().length > 0;
  const canCompose =
    !!sessionId &&
    !readOnly &&
    !sending &&
    session?.status !== "completed" &&
    session?.status !== "failed" &&
    session?.status !== "reverted" &&
    session?.status !== "error";
  const canSend = canCompose && draft.trim().length > 0;

  useEffect(() => {
    if (!sessionId) return;
    cancelPanelCloseStop(sessionId);
    let active = true;
    const loadSession =
      mode === "journal"
        ? getAgentJournalSession(sessionId).then((session) => (session ? [session] : []))
        : listAgentSessions();
    void loadSession
      .then((sessions) => {
        if (!active) return;
        if (mode === "journal") {
          const session = sessions[0];
          if (session) agentSessionStore.upsertSession(session);
          else setError("Session transcript was not found.");
        } else {
          agentSessionStore.setSessions(sessions);
        }
      })
      .catch((e) => {
        if (active) setError(commandMessage(e));
      });
    return () => {
      active = false;
      if (readOnly) return;
      schedulePanelCloseStop(
        sessionId,
        consumeTerminalDetachedMarker(sessionId)
          ? DETACHED_TRANSFER_STOP_DELAY_MS
          : PANEL_CLOSE_STOP_DELAY_MS,
      );
    };
  }, [mode, readOnly, sessionId]);

  useEffect(() => {
    if (focusedTurnIndex == null) return;
    if (!turns.some((turn) => turn.index === focusedTurnIndex)) {
      setFocusedTurnIndex(null);
    }
  }, [focusedTurnIndex, turns]);

  const canRevert =
    !!session &&
    !readOnly &&
    !!session.checkpoint &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const canRevertTurnFile =
    !!session &&
    !readOnly &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const canStop =
    !!session &&
    !readOnly &&
    !stopping &&
    (session.status === "running" || session.status === "starting");

  const sendDraft = async () => {
    if (!canSend) return;
    const text = draft.trimEnd();
    setSending(true);
    setError(null);
    try {
      await writeAgentSessionInput(sessionId, `${text}\r`);
      setDraft("");
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setSending(false);
    }
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    void sendDraft();
  };

  const onDraftKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    void sendDraft();
  };

  const applyQuickAction = (prompt: string) => {
    if (!canCompose) return;
    setDraft((current) => {
      const trimmed = current.trimEnd();
      return trimmed ? `${trimmed}\n\n${prompt}` : prompt;
    });
  };

  const applyFocusedTurnAction = (turn: AgentTurnView, action: FocusedTurnAction) => {
    if (!canCompose) return;
    setFocusedTurnIndex(turn.index);
    setDraft((current) => {
      const trimmed = current.trimEnd();
      const prompt = focusedTurnPrompt(turn, action);
      return trimmed ? `${trimmed}\n\n${prompt}` : prompt;
    });
  };

  const copyText = async (target: string, text: string) => {
    try {
      await writeClipboardText(text);
      setCopiedTarget(target);
      window.setTimeout(
        () => setCopiedTarget((current) => (current === target ? null : current)),
        1600,
      );
    } catch (e) {
      setError(commandMessage(e));
    }
  };

  const onRevert = async () => {
    if (!sessionId || !canRevert || reverting) return;
    const ok = await confirm("This will undo all changes made by this session. Continue?", {
      title: "Revert agent session",
      kind: "warning",
      okLabel: "Revert",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setReverting(true);
    setError(null);
    try {
      const updated = await revertSession(sessionId, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setReverting(false);
    }
  };

  const onStop = async () => {
    if (!sessionId || !canStop) return;
    setStopping(true);
    setError(null);
    try {
      await stopAgentSession(sessionId);
      const sessions = await listAgentSessions();
      agentSessionStore.setSessions(sessions);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setStopping(false);
    }
  };

  const onRevertTurnFile = async (turnCheckpointId: string, path: string) => {
    if (!sessionId || !canRevertTurnFile || revertingFile) return;
    const ok = await confirm(`Revert ${path} from this turn checkpoint?`, {
      title: "Revert file from turn",
      kind: "warning",
      okLabel: "Revert file",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setRevertingFile(`${turnCheckpointId}:${path}`);
    setError(null);
    try {
      const updated = await revertSessionTurnFile(sessionId, turnCheckpointId, path, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setRevertingFile(null);
    }
  };

  return (
    <div className="agent-panel" data-testid={`terminal-panel-${sessionId}`}>
      <header className="agent-panel__header">
        <span
          className={`agent-panel__logo agent-panel__logo--${agentLogoClass(agentType)}`}
          aria-hidden="true"
        >
          {agentLogoSrc(agentType) ? (
            <img src={agentLogoSrc(agentType) ?? ""} alt="" />
          ) : (
            <span>{agentLogoText(agentType)}</span>
          )}
        </span>
        <div className="agent-panel__identity">
          <span className="agent-panel__agent">{agentLabel(agentType)}</span>
          <span className="agent-panel__repo" title={repo}>
            {repo ? repoName(repo) : "Agent session"}
          </span>
        </div>
        <SessionStatus session={session} />
        <div className="agent-panel__header-actions">
          <button
            className="agent-panel__stop"
            disabled={!canStop}
            onClick={onStop}
            title={
              readOnly
                ? "Archived transcripts are read-only"
                : canStop
                  ? "Stop this agent session"
                  : "Session is not running"
            }
            type="button"
          >
            {stopping ? "Stopping" : "Stop"}
          </button>
          <button
            className="agent-panel__revert"
            disabled={!canRevert || reverting}
            onClick={onRevert}
            title={
              readOnly
                ? "Archived transcripts are read-only"
                : session?.status === "reverted"
                  ? "Session already reverted"
                  : session && !session.checkpoint
                    ? "This session has no reversible checkpoint"
                    : canRevert
                      ? "Revert this agent session"
                      : "Stop the session before reverting"
            }
            type="button"
          >
            {reverting ? "Reverting" : "Revert"}
          </button>
        </div>
      </header>

      {error && (
        <div className="agent-panel__error" data-testid="terminal-panel-error">
          {error}
        </div>
      )}

      <div className="agent-panel__workspace">
        <AgentSessionOverview
          overview={overview}
          focusedTurnIndex={focusedTurn?.index ?? null}
          onSelectTurn={(turnIndex) => {
            setFocusedTurnIndex(turnIndex);
            scrollToAgentTurn(sessionId, turnIndex, "start");
          }}
        />
        <AgentActivityStrip overview={overview} readOnly={readOnly} session={session} />

        <section className="agent-panel__chat-shell">
          <div className="agent-panel__chat-tools">
            <label className="agent-panel__chat-search">
              <span>Search transcript</span>
              <input
                aria-label="Search transcript"
                value={transcriptQuery}
                onChange={(event) => setTranscriptQuery(event.currentTarget.value)}
                placeholder="Find messages, commands, files..."
                type="search"
              />
            </label>
            <span className="agent-panel__chat-search-count" aria-live="polite">
              {hasTranscriptQuery ? `${visibleTurns.length} of ${turns.length} turns` : "All turns"}
            </span>
            <button
              className="agent-panel__chat-nav"
              disabled={visibleTurns.length === 0}
              onClick={() => {
                const latest = visibleTurns[visibleTurns.length - 1];
                if (latest) {
                  setFocusedTurnIndex(latest.index);
                  scrollToAgentTurn(sessionId, latest.index, "end");
                }
              }}
              type="button"
            >
              Latest
            </button>
            <button
              className="agent-panel__chat-copy"
              disabled={visibleTurns.length === 0}
              onClick={() => void copyText("transcript", transcriptText(visibleTurns))}
              type="button"
            >
              {copiedTarget === "transcript" ? "Copied" : "Copy visible"}
            </button>
          </div>
          <main className="agent-panel__chat" aria-label="Agent conversation">
            {visibleTurns.length > 0 ? (
              visibleTurns.map((turn) => (
                <AgentTurn
                  key={turn.id}
                  copiedTarget={copiedTarget}
                  firstTurnAtMs={turns[0]?.startedAtMs ?? null}
                  focused={turn.index === focusedTurn?.index}
                  onCopyMessage={(target, text) => void copyText(target, text)}
                  onCopyTurn={(target, text) => void copyText(target, text)}
                  turn={turn}
                  turnElementId={agentTurnElementId(sessionId, turn.index)}
                />
              ))
            ) : (
              <div className="agent-panel__empty-chat">
                <span>{hasTranscriptQuery ? "No matches" : readOnly ? "Transcript" : "Ready"}</span>
                <p>
                  {hasTranscriptQuery
                    ? "Try another search across messages, commands, and touched files."
                    : readOnly
                      ? "No timeline items were saved for this session."
                      : "Start a turn from the composer below."}
                </p>
              </div>
            )}
          </main>
        </section>

        {session && (
          <aside className="agent-panel__side-rail" aria-label="Agent inspection rail">
            <AgentTurnFocus
              copiedTarget={copiedTarget}
              firstTurnAtMs={turns[0]?.startedAtMs ?? null}
              canContinue={canCompose}
              onAction={applyFocusedTurnAction}
              onCopyTurn={(target, text) => void copyText(target, text)}
              onJump={(turnIndex) => scrollToAgentTurn(sessionId, turnIndex, "start")}
              turn={focusedTurn}
            />
            <AgentLens
              session={session}
              turns={turns}
              canRevertTurnFile={canRevertTurnFile}
              revertingFile={revertingFile}
              onRevertTurnFile={onRevertTurnFile}
            />
          </aside>
        )}
      </div>

      <form className="agent-panel__composer" onSubmit={onSubmit}>
        <div className="agent-panel__composer-actions" aria-label="Agent quick actions">
          {AGENT_QUICK_ACTIONS.map((action) => (
            <button
              className="agent-panel__quick-action"
              disabled={!canCompose}
              key={action.id}
              onClick={() => applyQuickAction(action.prompt)}
              title={action.title}
              type="button"
            >
              {action.label}
            </button>
          ))}
        </div>
        <div className="agent-panel__composer-row">
          <textarea
            aria-label={`Message ${agentLabel(agentType)}`}
            value={draft}
            onChange={(event) => setDraft(event.currentTarget.value)}
            onKeyDown={onDraftKeyDown}
            placeholder={
              readOnly
                ? "Archived transcript"
                : `Ask ${agentLabel(agentType)} what to do in this repo...`
            }
            disabled={!canCompose}
            rows={2}
          />
          <button className="agent-panel__send" type="submit" disabled={!canSend}>
            {sending ? "Sending" : "Send"}
          </button>
        </div>
      </form>
    </div>
  );
}

function AgentSessionOverview({
  focusedTurnIndex,
  overview,
  onSelectTurn,
}: {
  focusedTurnIndex: number | null;
  overview: AgentSessionOverviewView;
  onSelectTurn: (turnIndex: number) => void;
}) {
  return (
    <section className="agent-panel__overview" aria-label="Agent session overview">
      <div className="agent-panel__overview-metrics">
        <OverviewMetric value={overview.turns} label="Turns" />
        <OverviewMetric value={overview.messages} label="Messages" />
        <OverviewMetric value={overview.commands} label="Commands" />
        <OverviewMetric value={overview.files} label="Files" />
      </div>
      <div className="agent-panel__overview-activity">
        <span>Latest activity</span>
        <p title={overview.latest ?? undefined}>
          {overview.latest ?? "Waiting for the first turn."}
        </p>
      </div>
      {overview.turnMap.length > 0 && (
        <div className="agent-panel__overview-turns" aria-label="Turn map">
          {overview.turnMap.map((turn) => (
            <button
              className={`agent-panel__overview-turn${
                turn.index === focusedTurnIndex ? " agent-panel__overview-turn--active" : ""
              }`}
              aria-pressed={turn.index === focusedTurnIndex}
              key={turn.id}
              onClick={() => onSelectTurn(turn.index)}
              title={`Turn ${turn.index}: ${turn.commands} commands, ${turn.files} files`}
              type="button"
            >
              <strong>T{turn.index}</strong>
              {turn.timeLabel && <small>{turn.timeLabel}</small>}
              {turn.commands > 0 && <small>{turn.commands} cmd</small>}
              {turn.files > 0 && <small>{turn.files} files</small>}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

function OverviewMetric({ value, label }: { value: number; label: string }) {
  return (
    <div className="agent-panel__overview-metric" aria-label={`${label}: ${value}`}>
      <span>{value}</span>
      <small>{label}</small>
    </div>
  );
}

function AgentActivityStrip({
  overview,
  readOnly,
  session,
}: {
  overview: AgentSessionOverviewView;
  readOnly: boolean;
  session: AgentSession | undefined;
}) {
  const activity = agentActivitySummary(session, readOnly);
  return (
    <section className="agent-panel__activity" aria-label="Agent activity">
      <div className="agent-panel__activity-main">
        <span className={`agent-panel__activity-dot agent-panel__activity-dot--${activity.tone}`} />
        <div>
          <strong>{activity.title}</strong>
          <p>{activity.detail}</p>
        </div>
      </div>
      <div className="agent-panel__activity-facts">
        <span>{overview.turns} turns</span>
        <span>{overview.files} files</span>
        <span>{activity.checkpoint}</span>
        <span>{activity.throughput}</span>
      </div>
    </section>
  );
}

function AgentTurnFocus({
  canContinue,
  copiedTarget,
  firstTurnAtMs,
  onAction,
  onCopyTurn,
  onJump,
  turn,
}: {
  canContinue: boolean;
  copiedTarget: string | null;
  firstTurnAtMs: number | null;
  onAction: (turn: AgentTurnView, action: FocusedTurnAction) => void;
  onCopyTurn: (target: string, text: string) => void;
  onJump: (turnIndex: number) => void;
  turn: AgentTurnView | null;
}) {
  if (!turn) {
    return (
      <section className="agent-panel__turn-focus" aria-label="Focused turn">
        <div className="agent-panel__turn-focus-head">
          <span>Focused turn</span>
          <small>Idle</small>
        </div>
        <strong>No turn selected</strong>
        <p>The next agent response will appear here as a navigable turn.</p>
      </section>
    );
  }

  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const latest = latestActivityText(turn);
  const copyTarget = `${turn.id}:focus`;
  const visibleChanges = turn.changes.slice(0, 4);
  const hiddenChangeCount = Math.max(0, turn.changes.length - visibleChanges.length);
  return (
    <section className="agent-panel__turn-focus" aria-label="Focused turn">
      <div className="agent-panel__turn-focus-head">
        <span>Focused turn</span>
        {timeLabel && <small>{timeLabel}</small>}
      </div>
      <div className="agent-panel__turn-focus-title">
        <strong>Turn {turn.index}</strong>
        <small>{turnSummaryLabel(turn)}</small>
      </div>
      <p title={latest ?? undefined}>
        {latest ? compactActivityText(latest) : "No text captured."}
      </p>
      <div className="agent-panel__turn-focus-facts">
        <span>{turn.commandText.length} commands</span>
        <span>{turn.changes.length} files</span>
      </div>
      {visibleChanges.length > 0 && (
        <div className="agent-panel__turn-focus-files" aria-label="Focused turn files">
          {visibleChanges.map((change) => (
            <span key={`${change.kind}:${change.path}`}>
              {change.kind} {change.path}
            </span>
          ))}
          {hiddenChangeCount > 0 && <span>+{hiddenChangeCount} more</span>}
        </div>
      )}
      <div className="agent-panel__turn-focus-actions">
        {FOCUSED_TURN_ACTIONS.map((action) => (
          <button
            disabled={!canContinue}
            key={action.id}
            onClick={() => onAction(turn, action.id)}
            type="button"
          >
            {action.label}
          </button>
        ))}
      </div>
      <div className="agent-panel__turn-focus-utilities">
        <button onClick={() => onJump(turn.index)} type="button">
          Jump
        </button>
        <button
          onClick={() => onCopyTurn(copyTarget, turnTranscriptText(turn, firstTurnAtMs))}
          type="button"
        >
          {copiedTarget === copyTarget ? "Copied" : "Copy focus"}
        </button>
      </div>
    </section>
  );
}

function AgentTurn({
  copiedTarget,
  firstTurnAtMs,
  focused,
  onCopyMessage,
  onCopyTurn,
  turn,
  turnElementId,
}: {
  copiedTarget: string | null;
  firstTurnAtMs: number | null;
  focused: boolean;
  onCopyMessage: (target: string, text: string) => void;
  onCopyTurn: (target: string, text: string) => void;
  turn: AgentTurnView;
  turnElementId: string;
}) {
  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const turnTarget = `${turn.id}:turn`;
  return (
    <article
      aria-current={focused ? "true" : undefined}
      className={`agent-panel__chat-turn${focused ? " agent-panel__chat-turn--focused" : ""}`}
      id={turnElementId}
    >
      <div className="agent-panel__chat-turn-head">
        <div className="agent-panel__chat-turn-title">
          <span>Turn {turn.index}</span>
          <small>{turnSummaryLabel(turn)}</small>
        </div>
        <div className="agent-panel__chat-turn-meta">
          {timeLabel && <small>{timeLabel}</small>}
          {turn.changes.length > 0 && <small>{turn.changes.length} files touched</small>}
          <button
            className="agent-panel__turn-copy"
            onClick={() => onCopyTurn(turnTarget, turnTranscriptText(turn, firstTurnAtMs))}
            type="button"
          >
            {copiedTarget === turnTarget ? "Copied" : "Copy turn"}
          </button>
        </div>
      </div>
      {turn.userText && (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:user`}
          kind="user_message"
          label="You"
          onCopy={() => onCopyMessage(`${turn.id}:user`, turn.userText ?? "")}
          text={turn.userText}
        />
      )}
      {turn.agentText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:agent:${index}`}
          kind="agent_message"
          label="Agent"
          onCopy={() => onCopyMessage(`${turn.id}:agent:${index}`, text)}
          text={text}
          key={`a-${index}`}
        />
      ))}
      {turn.commandText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:command:${index}`}
          kind="command_output"
          label="Command"
          onCopy={() => onCopyMessage(`${turn.id}:command:${index}`, text)}
          text={text}
          key={`c-${index}`}
        />
      ))}
      {turn.systemText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:system:${index}`}
          kind="lifecycle"
          label="System"
          onCopy={() => onCopyMessage(`${turn.id}:system:${index}`, text)}
          text={text}
          key={`s-${index}`}
        />
      ))}
      {turn.changes.length > 0 && (
        <div className="agent-panel__chat-turn-files">
          {turn.changes.map((change) => (
            <span key={`${change.kind}:${change.path}`}>
              {change.kind} {change.path}
            </span>
          ))}
        </div>
      )}
    </article>
  );
}

function AgentMessageBlock({
  copied,
  kind,
  label,
  onCopy,
  text,
}: {
  copied: boolean;
  kind: AgentSessionTimelineItem["kind"];
  label: string;
  onCopy: () => void;
  text: string;
}) {
  const technical = kind === "command_output";
  const commandSummary = technical ? commandOutputSummary(text) : null;
  const collapseCommand = technical && shouldCollapseCommandOutput(text);
  return (
    <div className={`agent-panel__message agent-panel__message--${kind}`}>
      <div className="agent-panel__message-head">
        <div className="agent-panel__message-role">{label}</div>
        <button
          className="agent-panel__message-copy"
          aria-label={`Copy ${label} message`}
          onClick={onCopy}
          title={`Copy ${label} message`}
          type="button"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {technical ? (
        collapseCommand ? (
          <details className="agent-panel__command-block">
            <summary>
              <span>{commandSummary}</span>
              <small>Show output</small>
            </summary>
            <pre className="agent-panel__message-terminal">{text}</pre>
          </details>
        ) : (
          <pre className="agent-panel__message-terminal">{text}</pre>
        )
      ) : (
        <div className="agent-panel__markdown">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
        </div>
      )}
    </div>
  );
}

function shouldCollapseCommandOutput(text: string): boolean {
  return text.length > 360 || text.split(/\r\n|\r|\n/).length > 8;
}

function commandOutputSummary(text: string): string {
  const firstLine = text
    .split(/\r\n|\r|\n/)
    .map((line) => line.trim())
    .find(Boolean);
  if (!firstLine) return "Command output";
  return firstLine.length > 96 ? `${firstLine.slice(0, 93)}...` : firstLine;
}

function SessionStatus({ session }: { session: AgentSession | undefined }) {
  if (!session) {
    return <div className="agent-panel__status-strip">Loading session</div>;
  }
  return (
    <div className="agent-panel__status-strip" title={auditTitle(session)}>
      <span className={`agent-panel__status agent-panel__status--${session.status}`}>
        {sessionStatusLabel(session.status)}
      </span>
      <span>{turnStatusLabel(session.turn_status ?? "waiting")}</span>
      <span>
        {session.checkpoint ? checkpointLabel(session.checkpoint.checkpoint_type) : "No checkpoint"}
      </span>
      {(session.change_log?.length ?? 0) > 0 && <span>{session.change_log?.length} changes</span>}
    </div>
  );
}

function AgentLens({
  session,
  turns,
  canRevertTurnFile,
  revertingFile,
  onRevertTurnFile,
}: {
  session: AgentSession;
  turns: AgentTurnView[];
  canRevertTurnFile: boolean;
  revertingFile: string | null;
  onRevertTurnFile: (turnCheckpointId: string, path: string) => void;
}) {
  const [activeTab, setActiveTab] = useState<"files" | "commands" | "timeline">("files");
  const [fileQuery, setFileQuery] = useState("");
  const turnCheckpoints = session.turn_checkpoints ?? [];
  const fileItems = agentLensFileItems(turnCheckpoints, session.change_log ?? []);
  const filteredFileItems = filterAgentLensFileItems(fileItems, fileQuery);
  const hasFileQuery = fileQuery.trim().length > 0;
  const commandItems = agentLensCommandItems(turns);
  const timelineItems = agentLensTimelineItems(turns);
  return (
    <aside className="agent-panel__lens" aria-label="Agent Lens">
      <div className="agent-panel__lens-head">
        <span>Agent Lens</span>
        <small>{turnCheckpoints.length} checkpoints</small>
      </div>
      <div className="agent-panel__lens-metrics">
        <div>
          <span>{turnStatusLabel(session.turn_status ?? "waiting")}</span>
          <small>Turn state</small>
        </div>
        <div>
          <span>{session.change_log?.length ?? 0}</span>
          <small>Session changes</small>
        </div>
      </div>

      <div className="agent-panel__lens-tabs" role="tablist" aria-label="Agent Lens views">
        <AgentLensTabButton
          active={activeTab === "files"}
          count={fileItems.length}
          label="Files"
          onClick={() => setActiveTab("files")}
        />
        <AgentLensTabButton
          active={activeTab === "commands"}
          count={commandItems.length}
          label="Commands"
          onClick={() => setActiveTab("commands")}
        />
        <AgentLensTabButton
          active={activeTab === "timeline"}
          count={timelineItems.length}
          label="Timeline"
          onClick={() => setActiveTab("timeline")}
        />
      </div>

      {activeTab === "files" &&
        (fileItems.length > 0 ? (
          <>
            <label className="agent-panel__lens-filter">
              <span>Filter files</span>
              <input
                aria-label="Filter touched files"
                value={fileQuery}
                onChange={(event) => setFileQuery(event.currentTarget.value)}
                placeholder="Path or change type..."
                type="search"
              />
              <small>
                {hasFileQuery
                  ? `${filteredFileItems.length} of ${fileItems.length} files`
                  : `${fileItems.length} files`}
              </small>
            </label>
            {filteredFileItems.length > 0 ? (
              <div className="agent-panel__lens-list" aria-label="Touched files">
                {filteredFileItems.map((item) => {
                  if (!item.turnCheckpointId) {
                    return (
                      <div
                        className="agent-panel__lens-file agent-panel__lens-file--readonly"
                        key={item.id}
                      >
                        <span>{item.timeLabel ? `Session - ${item.timeLabel}` : "Session"}</span>
                        <strong>{item.path}</strong>
                        <small>{item.kind}</small>
                      </div>
                    );
                  }
                  const key = `${item.turnCheckpointId}:${item.path}`;
                  return (
                    <button
                      className="agent-panel__lens-file"
                      key={item.id}
                      disabled={!canRevertTurnFile || revertingFile === key}
                      onClick={() => onRevertTurnFile(item.turnCheckpointId ?? "", item.path)}
                      title={
                        canRevertTurnFile
                          ? `Revert ${item.path} from turn ${item.turnIndex}`
                          : "Stop the session before reverting files"
                      }
                      type="button"
                    >
                      <span>
                        Turn {item.turnIndex}
                        {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                      </span>
                      <strong>{item.path}</strong>
                      <small>{item.kind}</small>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="agent-panel__empty-lens">No files match this filter.</div>
            )}
          </>
        ) : (
          <div className="agent-panel__empty-lens">No touched files yet.</div>
        ))}

      {activeTab === "commands" &&
        (commandItems.length > 0 ? (
          <div className="agent-panel__lens-list" aria-label="Command output">
            {commandItems.map((item) => (
              <div className="agent-panel__lens-event" key={item.id}>
                <span>
                  Turn {item.turnIndex} command
                  {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                </span>
                <pre>{item.text}</pre>
              </div>
            ))}
          </div>
        ) : (
          <div className="agent-panel__empty-lens">No commands captured yet.</div>
        ))}

      {activeTab === "timeline" &&
        (timelineItems.length > 0 ? (
          <div className="agent-panel__lens-list" aria-label="Recent timeline">
            {timelineItems.map((item) => (
              <div className="agent-panel__lens-event" key={item.id}>
                <span>
                  Turn {item.turnIndex} - {item.label}
                  {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                </span>
                <pre>{item.text}</pre>
              </div>
            ))}
          </div>
        ) : (
          <div className="agent-panel__empty-lens">No timeline captured yet.</div>
        ))}
    </aside>
  );
}

function AgentLensTabButton({
  active,
  count,
  label,
  onClick,
}: {
  active: boolean;
  count: number;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={`agent-panel__lens-tab${active ? " agent-panel__lens-tab--active" : ""}`}
      aria-selected={active}
      onClick={onClick}
      role="tab"
      type="button"
    >
      <span>{label}</span>
      <small>{count}</small>
    </button>
  );
}

function agentLensFileItems(
  turnCheckpoints: NonNullable<AgentSession["turn_checkpoints"]>,
  changeLog: NonNullable<AgentSession["change_log"]>,
) {
  const firstCheckpointAtMs = turnCheckpoints[0]?.started_at_ms ?? null;
  const checkpointItems = turnCheckpoints
    .slice()
    .reverse()
    .flatMap((turn) =>
      turn.changes.map((change) => ({
        id: `${turn.id}:${change.kind}:${change.path}`,
        turnCheckpointId: turn.id,
        turnIndex: turn.index,
        timeLabel: timeOffsetLabel(turn.started_at_ms, firstCheckpointAtMs),
        path: change.path,
        kind: change.kind,
      })),
    );
  const checkpointPaths = new Set(checkpointItems.map((item) => item.path));
  const firstChangeAtMs = changeLog[0]?.timestamp_ms ?? null;
  const sessionItems = changeLog
    .filter((change) => !checkpointPaths.has(change.path))
    .slice()
    .reverse()
    .map((change) => ({
      id: `session:${change.kind}:${change.path}:${change.timestamp_ms}`,
      turnCheckpointId: null,
      turnIndex: null,
      timeLabel: timeOffsetLabel(change.timestamp_ms, firstChangeAtMs),
      path: change.path,
      kind: change.kind,
    }));
  return [...checkpointItems, ...sessionItems];
}

function filterAgentLensFileItems<T extends { path: string; kind: string }>(
  items: T[],
  query: string,
): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return items;
  return items.filter((item) =>
    `${item.path}\n${item.kind}`.toLocaleLowerCase().includes(normalized),
  );
}

function agentLensCommandItems(turns: AgentTurnView[]) {
  return turns
    .flatMap((turn) =>
      turn.commandText.map((text, index) => ({
        id: `${turn.id}:command:${index}`,
        turnIndex: turn.index,
        timeLabel: turnTimeLabel(turn, turns[0]?.startedAtMs ?? null),
        text: compactActivityText(text),
      })),
    )
    .reverse();
}

function agentLensTimelineItems(turns: AgentTurnView[]) {
  return turns
    .flatMap((turn) => {
      const items: Array<{
        id: string;
        turnIndex: number;
        label: string;
        text: string;
        timeLabel: string | null;
      }> = [];
      const timeLabel = turnTimeLabel(turn, turns[0]?.startedAtMs ?? null);
      if (turn.userText) {
        items.push({
          id: `${turn.id}:user`,
          turnIndex: turn.index,
          label: "You",
          timeLabel,
          text: compactActivityText(turn.userText),
        });
      }
      turn.agentText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:agent:${index}`,
          turnIndex: turn.index,
          label: "Agent",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      turn.commandText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:command:${index}`,
          turnIndex: turn.index,
          label: "Command",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      turn.systemText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:system:${index}`,
          turnIndex: turn.index,
          label: "System",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      return items;
    })
    .reverse()
    .slice(0, 12);
}

function agentSessionOverview(turns: AgentTurnView[]): AgentSessionOverviewView {
  const files = new Set<string>();
  let messages = 0;
  let commands = 0;
  let latest: string | null = null;

  for (const turn of turns) {
    messages += (turn.userText ? 1 : 0) + turn.agentText.length + turn.systemText.length;
    commands += turn.commandText.length;
    for (const change of turn.changes) files.add(change.path);
    latest = latestActivityText(turn) ?? latest;
  }

  const activeTurnId = turns[turns.length - 1]?.id ?? null;
  const firstTurnAtMs = turns[0]?.startedAtMs ?? null;
  return {
    turns: turns.length,
    messages,
    commands,
    files: files.size,
    latest: latest ? compactActivityText(latest) : null,
    turnMap: turns.map((turn) => ({
      id: turn.id,
      index: turn.index,
      commands: turn.commandText.length,
      files: turn.changes.length,
      active: turn.id === activeTurnId,
      timeLabel: turnTimeLabel(turn, firstTurnAtMs),
    })),
  };
}

function agentActivitySummary(
  session: AgentSession | undefined,
  readOnly: boolean,
): {
  title: string;
  detail: string;
  checkpoint: string;
  throughput: string;
  tone: "idle" | "working" | "done" | "failed";
} {
  if (!session) {
    return {
      title: "Loading agent session",
      detail: "Connecting the workspace view to the agent runtime.",
      checkpoint: "Checkpoint unknown",
      throughput: "No stream yet",
      tone: "idle",
    };
  }
  const turnState = turnStatusLabel(session.turn_status ?? "waiting");
  const status = sessionStatusLabel(session.status);
  const changeCount = session.change_log?.length ?? 0;
  const checkpoint = session.checkpoint
    ? checkpointLabel(session.checkpoint.checkpoint_type)
    : "No checkpoint";
  const throughput =
    session.output_bytes_per_second != null
      ? `${Math.round(session.output_bytes_per_second)} B/s`
      : "Stream quiet";
  if (readOnly) {
    return {
      title: "Archived transcript",
      detail: `${changeCount} recorded changes. Session is read-only.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.status === "failed" || session.status === "error") {
    return {
      title: "Needs attention",
      detail: session.error
        ? commandMessage(session.error)
        : `${status}. Review the transcript and recent command output.`,
      checkpoint,
      throughput,
      tone: "failed",
    };
  }
  if (session.status === "completed" || session.status === "reverted") {
    return {
      title: session.status === "reverted" ? "Changes reverted" : "Session complete",
      detail: `${changeCount} changes tracked. ${turnState} turn state.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.turn_status === "working" || session.status === "running") {
    return {
      title: "Agent is working",
      detail: `${turnState}. ${changeCount} changes tracked so far.`,
      checkpoint,
      throughput,
      tone: "working",
    };
  }
  return {
    title: "Ready for the next turn",
    detail: `${turnState}. ${changeCount} changes tracked so far.`,
    checkpoint,
    throughput,
    tone: "idle",
  };
}

function latestActivityText(turn: AgentTurnView): string | null {
  const candidates = [
    turn.userText ?? "",
    ...turn.agentText,
    ...turn.commandText,
    ...turn.systemText,
  ].filter(Boolean);
  return candidates[candidates.length - 1] ?? null;
}

function turnTimeLabel(turn: AgentTurnView, firstTurnAtMs: number | null): string | null {
  return timeOffsetLabel(turn.startedAtMs, firstTurnAtMs);
}

function timeOffsetLabel(
  timestampMs: number | null | undefined,
  baseMs: number | null | undefined,
): string | null {
  if (timestampMs == null || baseMs == null) return null;
  const deltaSeconds = Math.max(0, Math.round((timestampMs - baseMs) / 1000));
  if (deltaSeconds < 60) return `+${deltaSeconds}s`;
  const minutes = Math.floor(deltaSeconds / 60);
  const seconds = deltaSeconds % 60;
  if (minutes < 60) return seconds === 0 ? `+${minutes}m` : `+${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes === 0 ? `+${hours}h` : `+${hours}h ${remainingMinutes}m`;
}

function compactActivityText(text: string): string {
  const oneLine = text.replace(/\s+/g, " ").trim();
  return oneLine.length > 140 ? `${oneLine.slice(0, 137)}...` : oneLine;
}

function agentTurnElementId(sessionId: string, turnIndex: number): string {
  return `agent-turn-${sessionId}-${turnIndex}`;
}

function scrollToAgentTurn(sessionId: string, turnIndex: number, block: ScrollLogicalPosition) {
  document
    .getElementById(agentTurnElementId(sessionId, turnIndex))
    ?.scrollIntoView({ block, behavior: "smooth" });
}

async function writeClipboardText(text: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard is not available in this window.");
  }
  await navigator.clipboard.writeText(text);
}

function transcriptText(turns: AgentTurnView[]): string {
  const firstTurnAtMs = turns[0]?.startedAtMs ?? null;
  return turns.map((turn) => turnTranscriptText(turn, firstTurnAtMs)).join("\n\n");
}

function turnTranscriptText(turn: AgentTurnView, firstTurnAtMs: number | null): string {
  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const parts: string[] = [`Turn ${turn.index}${timeLabel ? ` (${timeLabel})` : ""}`];
  if (turn.userText) parts.push(`You:\n${turn.userText}`);
  turn.agentText.forEach((text) => parts.push(`Agent:\n${text}`));
  turn.commandText.forEach((text) => parts.push(`Command:\n${text}`));
  turn.systemText.forEach((text) => parts.push(`System:\n${text}`));
  if (turn.changes.length > 0) {
    parts.push(
      `Files:\n${turn.changes.map((change) => `- ${change.kind} ${change.path}`).join("\n")}`,
    );
  }
  return parts.join("\n\n");
}

function focusedTurnPrompt(turn: AgentTurnView, action: FocusedTurnAction): string {
  const latest = latestActivityText(turn);
  const files = turn.changes.map((change) => `${change.kind} ${change.path}`).slice(0, 6);
  const actionLine = focusedTurnActionLine(action, turn.index);
  const instruction = focusedTurnInstruction(action);
  const parts = [
    actionLine,
    latest ? `Recent context: ${compactActivityText(latest)}` : null,
    files.length > 0 ? `Touched files: ${files.join(", ")}.` : null,
    instruction,
  ];
  return parts.filter(Boolean).join("\n");
}

function focusedTurnActionLine(action: FocusedTurnAction, turnIndex: number): string {
  switch (action) {
    case "review":
      return `Review turn ${turnIndex}.`;
    case "test":
      return `Test the work from turn ${turnIndex}.`;
    case "handoff":
      return `Prepare a handoff from turn ${turnIndex}.`;
    case "continue":
    default:
      return `Continue from turn ${turnIndex}.`;
  }
}

function focusedTurnInstruction(action: FocusedTurnAction): string {
  switch (action) {
    case "review":
      return "Use this turn as the immediate context and call out concrete bugs, regressions, or missing tests.";
    case "test":
      return "Use this turn as the immediate context, run the most relevant verification, and summarize failures before fixing them.";
    case "handoff":
      return "Use this turn as the immediate context and summarize state, changed files, verification, risks, and the next recommended step.";
    case "continue":
    default:
      return "Use this turn as the immediate context for the next step.";
  }
}

function turnSummaryLabel(turn: AgentTurnView): string {
  const messages =
    (turn.userText ? 1 : 0) +
    turn.agentText.length +
    turn.systemText.length +
    turn.commandText.length;
  const parts = [`${messages} messages`];
  if (turn.commandText.length > 0) parts.push(`${turn.commandText.length} commands`);
  if (turn.changes.length > 0) parts.push(`${turn.changes.length} files`);
  return parts.join(" / ");
}

function filterAgentTurns(turns: AgentTurnView[], query: string): AgentTurnView[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return turns;
  return turns.filter((turn) => agentTurnSearchText(turn).includes(normalized));
}

function agentTurnSearchText(turn: AgentTurnView): string {
  return [
    turn.userText ?? "",
    ...turn.agentText,
    ...turn.commandText,
    ...turn.systemText,
    ...turn.changes.flatMap((change) => [change.path, change.kind]),
  ]
    .join("\n")
    .toLocaleLowerCase();
}

function agentTurns(
  timeline: AgentSessionTimelineItem[],
  chunks: AgentSessionOutput[],
  session: AgentSession | undefined,
): AgentTurnView[] {
  if (timeline.length > 0) {
    return attachCheckpointChanges(buildTurnsFromTimeline(timeline), session);
  }
  const text = chunks.map((chunk) => decodeBase64Text(chunk.chunk_base64)).join("");
  const cleaned = stripAnsi(text).replace(/\r\n/g, "\n").replace(/\r/g, "\n").trim();
  if (!cleaned) return [];
  const fallbackBlocks = cleaned
    .split(/\n{3,}/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part);
  if (fallbackBlocks.length === 0) return [];
  return attachCheckpointChanges(
    [
      {
        id: "fallback-output",
        index: 1,
        startedAtMs: chunks[0]?.timestamp_ms ?? null,
        updatedAtMs: chunks[chunks.length - 1]?.timestamp_ms ?? chunks[0]?.timestamp_ms ?? null,
        userText: null,
        agentText: fallbackBlocks,
        commandText: [],
        systemText: [],
        changes: [],
      },
    ],
    session,
  );
}

function buildTurnsFromTimeline(timeline: AgentSessionTimelineItem[]): AgentTurnView[] {
  const turns: AgentTurnView[] = [];
  let current: AgentTurnView | null = null;
  for (const item of timeline) {
    const text = item.text.trim();
    if (!text) continue;
    if (item.kind === "user_message") {
      current = {
        id: item.id,
        index: turns.length + 1,
        startedAtMs: item.timestamp_ms,
        updatedAtMs: item.timestamp_ms,
        userText: text,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
      };
      turns.push(current);
      continue;
    }
    if (!current) {
      current = {
        id: item.id,
        index: turns.length + 1,
        startedAtMs: item.timestamp_ms,
        updatedAtMs: item.timestamp_ms,
        userText: null,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
      };
      turns.push(current);
    }
    current.updatedAtMs = item.timestamp_ms;
    appendTimelineText(current, item.kind, text);
  }
  return turns;
}

function appendTimelineText(
  turn: AgentTurnView,
  kind: AgentSessionTimelineItem["kind"],
  text: string,
) {
  const target =
    kind === "command_output"
      ? turn.commandText
      : kind === "lifecycle"
        ? turn.systemText
        : turn.agentText;
  const previous = target[target.length - 1];
  if (previous) {
    target[target.length - 1] = `${previous}${needsLineBreak(previous, text) ? "\n" : ""}${text}`;
  } else {
    target.push(text);
  }
}

function attachCheckpointChanges(
  turns: AgentTurnView[],
  session: AgentSession | undefined,
): AgentTurnView[] {
  if (!session?.turn_checkpoints?.length) return turns;
  const next = turns.map((turn) => ({ ...turn, changes: [...turn.changes] }));
  for (const checkpoint of session.turn_checkpoints) {
    let turn = next.find((candidate) => candidate.index === checkpoint.index);
    if (!turn) {
      turn = {
        id: checkpoint.id,
        index: checkpoint.index,
        startedAtMs: checkpoint.started_at_ms,
        updatedAtMs: checkpoint.ended_at_ms ?? checkpoint.started_at_ms,
        userText: null,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
      };
      next.push(turn);
    }
    turn.startedAtMs = turn.startedAtMs ?? checkpoint.started_at_ms;
    turn.updatedAtMs = Math.max(
      turn.updatedAtMs ?? checkpoint.started_at_ms,
      checkpoint.ended_at_ms ?? checkpoint.started_at_ms,
    );
    turn.changes = checkpoint.changes.map((change) => ({
      path: change.path,
      kind: change.kind,
    }));
  }
  return next.sort((a, b) => a.index - b.index);
}

function needsLineBreak(left: string, right: string): boolean {
  if (!left || !right) return false;
  return !left.endsWith("\n") && !right.startsWith("\n");
}

function decodeBase64Text(chunk: string): string {
  const binary = atob(chunk);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}

function stripAnsi(text: string): string {
  return text.replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "");
}

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Agent command failed.");
  }
  return String(error || "Agent command failed.");
}

function turnStatusLabel(status: string): string {
  switch (status) {
    case "working":
      return "Working";
    case "settling":
      return "Settling";
    default:
      return "Waiting";
  }
}

function sessionStatusLabel(status: string): string {
  switch (status) {
    case "starting":
      return "Starting";
    case "running":
      return "Running";
    case "completed":
      return "Done";
    case "failed":
      return "Failed";
    case "reverted":
      return "Reverted";
    case "error":
      return "Error";
    default:
      return status;
  }
}

function checkpointLabel(type: string): string {
  return type === "git_ref" ? "git checkpoint" : "filesystem checkpoint";
}

function agentLabel(agentType: string): string {
  switch (agentType) {
    case "codex":
      return "Codex";
    case "claude":
      return "Claude Code";
    case "opencode":
      return "OpenCode";
    default:
      return agentType;
  }
}

function agentLogoText(agentType: string): string {
  switch (agentType) {
    case "codex":
      return "Cx";
    case "claude":
      return "Cl";
    case "opencode":
      return "OC";
    default:
      return agentType.slice(0, 2).toUpperCase();
  }
}

function agentLogoSrc(agentType: string): string | null {
  switch (agentType) {
    case "codex":
      return codexLogo;
    case "claude":
      return claudeLogo;
    case "opencode":
      return opencodeLogo;
    default:
      return null;
  }
}

function agentLogoClass(agentType: string): string {
  return agentType.replace(/[^a-z0-9_-]/gi, "").toLowerCase() || "agent";
}

function repoName(repo: string): string {
  const parts = repo.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? repo;
}

function auditTitle(session: AgentSession): string {
  const pieces = [
    `Status: ${session.status}`,
    session.checkpoint
      ? `Checkpoint: ${checkpointLabel(session.checkpoint.checkpoint_type)}`
      : null,
    session.checkpoint?.git_hash ? `Git: ${session.checkpoint.git_hash.slice(0, 12)}` : null,
    `Changes: ${session.change_log?.length ?? 0}`,
    `Age: ${Math.round(session.age_ms / 1000)}s`,
    `Active sessions: ${session.active_sessions}`,
  ];
  return pieces.filter(Boolean).join(" / ");
}

function cancelPanelCloseStop(sessionId: string) {
  const timer = panelCloseStopTimers.get(sessionId);
  if (timer === undefined) return;
  window.clearTimeout(timer);
  panelCloseStopTimers.delete(sessionId);
}

function schedulePanelCloseStop(sessionId: string, delayMs = PANEL_CLOSE_STOP_DELAY_MS) {
  cancelPanelCloseStop(sessionId);
  const timer = window.setTimeout(() => {
    panelCloseStopTimers.delete(sessionId);
    void stopAgentSession(sessionId)
      .then(() => listAgentSessions())
      .then((sessions) => agentSessionStore.setSessions(sessions))
      .catch(() => {});
  }, delayMs);
  panelCloseStopTimers.set(sessionId, timer);
}
