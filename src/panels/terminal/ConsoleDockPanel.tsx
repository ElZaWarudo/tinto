import { useEffect, useRef, useState } from "react";
import { DockviewReact, themeVisualStudio } from "dockview-react";
import type {
  DockviewReadyEvent,
  DockviewWillDropEvent,
  IDockviewPanel,
  IDockviewPanelProps,
} from "dockview-react";
import {
  getAgentJournalSession,
  listAgentJournalSessions,
  listAgentSessions,
  startAgentSession,
} from "../../bus/client";
import { agentSessionStore, useAgentSessionState } from "../../agent/sessionStore";
import { busStore, useBusState } from "../../bus/store";
import type {
  AgentJournalSessionSummary,
  AgentSession,
  AgentSessionTimelineItem,
} from "../../bus/contract";
import codexLogo from "../../assets/agents/codex.svg";
import claudeLogo from "../../assets/agents/claude.svg";
import opencodeLogo from "../../assets/agents/opencode.svg";
import {
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "../../workspace/panels";
import { consoleDock } from "../../workspace/consoleDock";
import { armExternalTabDetach } from "../../workspace/externalTabDetach";
import {
  forgetRecentAgentLaunch,
  readRecentAgentLaunches,
  type RecentAgentLaunch,
} from "../../workspace/recentAgentLaunches";
import { TerminalPanel, type TerminalPanelParams } from "./TerminalPanel";
import { markTerminalDetached, openDetachedTerminalWindow } from "./detachTerminalWindow";

const consoleComponents = {
  [PANEL_AGENT_TERMINAL]: TerminalPanel,
};

const detachingTerminals = new Set<string>();

interface RecentAgentLaunchGroup {
  repo: string;
  launches: RecentAgentLaunch[];
  lastUsedAt: number;
}

export async function detachTerminalFromConsoleDrop(
  event: DockviewWillDropEvent,
  api: DockviewReadyEvent["api"] | null,
): Promise<boolean> {
  if (!api || event.kind !== "edge") return false;
  const panelId = event.getData()?.panelId;
  const sessionId = panelId ? sessionIdFromAgentTerminalPanelId(panelId) : null;
  if (!panelId || !sessionId) return false;

  const panel = api.getPanel(panelId);
  event.preventDefault();
  return detachTerminalPanel(api, panelId, panel);
}

export async function detachTerminalPanel(
  api: DockviewReadyEvent["api"],
  panelId: string,
  panel = api.getPanel(panelId),
): Promise<boolean> {
  const sessionId = sessionIdFromAgentTerminalPanelId(panelId);
  if (!sessionId || detachingTerminals.has(panelId)) return false;

  detachingTerminals.add(panelId);
  try {
    const params = terminalParamsFromPanel(panel, sessionId);
    const opened = await openDetachedTerminalWindow(params);
    if (!opened) return false;

    markTerminalDetached(sessionId);
    const current = api.getPanel(panelId);
    if (current) {
      api.removePanel(current);
    }
    return true;
  } finally {
    detachingTerminals.delete(panelId);
  }
}

function terminalParamsFromPanel(
  panel: IDockviewPanel | undefined,
  sessionId: string,
): TerminalPanelParams {
  const params = panel?.params as Partial<TerminalPanelParams> | undefined;
  return {
    sessionId,
    repo: typeof params?.repo === "string" ? params.repo : undefined,
    agentType: typeof params?.agentType === "string" ? params.agentType : undefined,
  };
}

type ConsoleDockPanelProps = Partial<IDockviewPanelProps> & {
  restoreTransferLayout?: boolean;
};

export function ConsoleDockPanel({ restoreTransferLayout = false }: ConsoleDockPanelProps) {
  const { repos } = useBusState();
  const agentState = useAgentSessionState();
  const apiRef = useRef<DockviewReadyEvent["api"] | null>(null);
  const movePanelDisposeRef = useRef<(() => void) | null>(null);
  const [terminalCount, setTerminalCount] = useState(0);
  const [openTerminals, setOpenTerminals] = useState<TerminalPanelParams[]>([]);
  const [recentLaunches, setRecentLaunches] = useState<RecentAgentLaunch[]>(() =>
    readRecentAgentLaunches(),
  );
  const [journalSessions, setJournalSessions] = useState<AgentJournalSessionSummary[]>([]);
  const [launchingKey, setLaunchingKey] = useState<string | null>(null);
  const [openingJournalId, setOpeningJournalId] = useState<string | null>(null);
  const [launchError, setLaunchError] = useState<string | null>(null);

  useEffect(() => {
    const updateTerminalState = () => {
      setTerminalCount(consoleDock.openTerminalSessionIds().length);
      setOpenTerminals(consoleDock.openTerminalParams());
      setRecentLaunches(readRecentAgentLaunches());
    };
    const unsubscribe = consoleDock.subscribe(updateTerminalState);
    updateTerminalState();

    return () => {
      unsubscribe();
      movePanelDisposeRef.current?.();
      movePanelDisposeRef.current = null;
      if (apiRef.current) {
        consoleDock.unregister(apiRef.current);
      }
    };
  }, []);

  useEffect(() => {
    let active = true;
    void listAgentJournalSessions(18)
      .then((sessions) => {
        if (active) setJournalSessions(sessions);
      })
      .catch(() => {
        if (active) setJournalSessions([]);
      });
    return () => {
      active = false;
    };
  }, [terminalCount]);

  const onReady = (event: DockviewReadyEvent) => {
    movePanelDisposeRef.current?.();
    apiRef.current = event.api;
    consoleDock.register(event.api, { restoreTransferLayout });
    const updateTerminalCount = () => {
      setTerminalCount(consoleDock.openTerminalSessionIds().length);
      setOpenTerminals(consoleDock.openTerminalParams());
    };
    updateTerminalCount();
    const layoutDisposable = event.api.onDidLayoutChange(updateTerminalCount);
    const disposable = event.api.onDidMovePanel((moveEvent) => {
      if (moveEvent.panel.api.location.type === "grid") return;
      void detachTerminalPanel(event.api, moveEvent.panel.id, moveEvent.panel);
    });
    const dragDisposable = event.api.onWillDragPanel((dragEvent) => {
      const panelId = dragEvent.panel.id;
      if (!sessionIdFromAgentTerminalPanelId(panelId)) return;
      armExternalTabDetach(dragEvent.nativeEvent, () =>
        detachTerminalPanel(event.api, panelId, dragEvent.panel),
      );
    });
    movePanelDisposeRef.current = () => {
      layoutDisposable.dispose();
      disposable.dispose();
      dragDisposable.dispose();
    };
  };

  const onWillDrop = (event: DockviewWillDropEvent) => {
    void detachTerminalFromConsoleDrop(event, apiRef.current);
  };

  const visibleQuickLaunches = recentLaunches.filter((launch) => {
    const repoKeys = Object.keys(repos);
    return repoKeys.length === 0 || repoKeys.includes(launch.repo);
  });
  const quickLaunchGroups = groupRecentLaunches(visibleQuickLaunches);

  const ensureTerminalPanelVisible = (params: TerminalPanelParams) => {
    const api = apiRef.current;
    if (!api) return;
    const id = agentTerminalPanelId(params.sessionId);
    const existing = api.getPanel(id);
    if (existing) {
      existing.api.setActive();
      setOpenTerminals(consoleDock.openTerminalParams());
      return;
    }
    try {
      const panel = api.addPanel({
        id,
        component: PANEL_AGENT_TERMINAL,
        title: terminalTitle(params),
        params,
      });
      panel.api.setActive();
      setOpenTerminals(consoleDock.openTerminalParams());
    } catch {
      api.getPanel(id)?.api.setActive();
      setOpenTerminals(consoleDock.openTerminalParams());
    }
  };

  const launchRecent = (launch: RecentAgentLaunch) => {
    const key = recentLaunchKey(launch);
    if (launchingKey) return;
    setLaunchingKey(key);
    setLaunchError(null);
    void startAgentSession(launch.repo, launch.agentType)
      .then((sessionId) => {
        consoleDock.openTerminal({
          sessionId,
          repo: launch.repo,
          agentType: launch.agentType,
        });
        ensureTerminalPanelVisible({
          sessionId,
          repo: launch.repo,
          agentType: launch.agentType,
        });
        void listAgentSessions()
          .then((sessions) => agentSessionStore.setSessions(sessions))
          .catch(() => {});
      })
      .catch((error) => {
        setLaunchError(commandMessage(error));
      })
      .finally(() => {
        setLaunchingKey(null);
      });
  };

  const removeRecent = (launch: RecentAgentLaunch) => {
    forgetRecentAgentLaunch(launch);
    setRecentLaunches(readRecentAgentLaunches());
    setLaunchError(null);
  };

  const openJournalTranscript = (summary: AgentJournalSessionSummary) => {
    if (openingJournalId) return;
    setOpeningJournalId(summary.id);
    setLaunchError(null);
    void getAgentJournalSession(summary.id)
      .then((session) => {
        if (!session) {
          setLaunchError("Session transcript was not found.");
          return;
        }
        agentSessionStore.upsertSession(session);
        const params: TerminalPanelParams = {
          sessionId: session.id,
          repo: session.repo,
          agentType: session.agent_type,
          mode: "journal",
        };
        consoleDock.openTerminal(params);
        ensureTerminalPanelVisible(params);
      })
      .catch((error) => {
        setLaunchError(commandMessage(error));
      })
      .finally(() => {
        setOpeningJournalId(null);
      });
  };

  return (
    <div className="console-dock-panel" data-testid="console-dock-panel">
      {terminalCount > 0 && (
        <AgentNavigator
          openTerminals={openTerminals}
          sessions={agentState.sessions}
          timeline={agentState.timeline}
          journalSessions={journalSessions}
          openingJournalId={openingJournalId}
          onFocus={ensureTerminalPanelVisible}
          onOpenJournal={openJournalTranscript}
        />
      )}
      <DockviewReact
        components={consoleComponents}
        dndStrategy="pointer"
        theme={themeVisualStudio}
        onReady={onReady}
        onWillDrop={onWillDrop}
      />
      {terminalCount === 0 && (
        <div className="console-dock-panel__empty" data-testid="console-empty">
          <div className="console-dock-panel__empty-title">No active agents</div>
          {quickLaunchGroups.length > 0 ? (
            <>
              <div className="console-dock-panel__empty-subtitle">Quick launch</div>
              <div className="console-dock-panel__quick-browser">
                {quickLaunchGroups.map((group) => {
                  return (
                    <div className="console-dock-panel__quick-project" key={group.repo}>
                      <div className="console-dock-panel__quick-project-title" title={group.repo}>
                        {busStore.displayName(group.repo)}
                      </div>
                      <div className="console-dock-panel__quick-agents">
                        {group.launches.map((launch) => {
                          const key = recentLaunchKey(launch);
                          const logo = agentLogoSrc(launch.agentType);
                          return (
                            <div className="console-dock-panel__quick-row" key={key}>
                              <button
                                className="console-dock-panel__quick"
                                type="button"
                                aria-label={`Launch ${busStore.displayName(
                                  launch.repo,
                                )} with ${agentLabel(launch.agentType)}`}
                                disabled={!!launchingKey}
                                onPointerDown={(event) => event.stopPropagation()}
                                onMouseDown={(event) => event.stopPropagation()}
                                onClick={(event) => {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  launchRecent(launch);
                                }}
                              >
                                <span
                                  className={`console-dock-panel__quick-icon console-dock-panel__quick-icon--${agentLogoClass(
                                    launch.agentType,
                                  )}`}
                                  aria-hidden="true"
                                >
                                  {logo ? (
                                    <img src={logo} alt="" />
                                  ) : (
                                    <span>{agentLogoText(launch.agentType)}</span>
                                  )}
                                </span>
                                <span className="console-dock-panel__quick-main">
                                  <span>{agentLabel(launch.agentType)}</span>
                                  <small>Recent</small>
                                </span>
                                <span className="console-dock-panel__quick-action">
                                  {launchingKey === key ? "Starting" : "Run"}
                                </span>
                              </button>
                              <button
                                className="console-dock-panel__quick-remove"
                                type="button"
                                aria-label={`Remove ${busStore.displayName(
                                  launch.repo,
                                )} with ${agentLabel(launch.agentType)} from quick launch`}
                                onPointerDown={(event) => event.stopPropagation()}
                                onMouseDown={(event) => event.stopPropagation()}
                                onClick={(event) => {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  removeRecent(launch);
                                }}
                              >
                                x
                              </button>
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  );
                })}
              </div>
              {launchError && (
                <div className="console-dock-panel__quick-error" role="status">
                  {launchError}
                </div>
              )}
              {journalSessions.length > 0 && (
                <AgentJournalBrowser
                  sessions={journalSessions}
                  openingJournalId={openingJournalId}
                  onOpen={openJournalTranscript}
                />
              )}
            </>
          ) : (
            <>
              <div className="console-dock-panel__empty-subtitle">Launch an agent to open one.</div>
              {journalSessions.length > 0 && (
                <AgentJournalBrowser
                  sessions={journalSessions}
                  openingJournalId={openingJournalId}
                  onOpen={openJournalTranscript}
                />
              )}
              {launchError && (
                <div className="console-dock-panel__quick-error" role="status">
                  {launchError}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

function AgentNavigator({
  openTerminals,
  sessions,
  timeline,
  journalSessions,
  openingJournalId,
  onFocus,
  onOpenJournal,
}: {
  openTerminals: TerminalPanelParams[];
  sessions: Record<string, AgentSession>;
  timeline: Record<string, AgentSessionTimelineItem[]>;
  journalSessions: AgentJournalSessionSummary[];
  openingJournalId: string | null;
  onFocus: (params: TerminalPanelParams) => void;
  onOpenJournal: (session: AgentJournalSessionSummary) => void;
}) {
  return (
    <aside className="console-dock-panel__navigator" aria-label="Agent sessions">
      <div className="console-dock-panel__navigator-section">
        <div className="console-dock-panel__navigator-head">
          <span>Active</span>
          <small>{openTerminals.length}</small>
        </div>
        <div className="console-dock-panel__navigator-list">
          {openTerminals.map((terminal) => {
            const agentType = terminal.agentType ?? "agent";
            const logo = agentLogoSrc(agentType);
            const session = sessions[terminal.sessionId];
            const preview = activeSessionPreview(session, timeline[terminal.sessionId]);
            const tone = sessionStatusTone(session);
            return (
              <button
                className={`console-dock-panel__navigator-item console-dock-panel__navigator-item--${tone}`}
                type="button"
                key={terminal.sessionId}
                aria-label={`Focus ${busStore.displayName(
                  terminal.repo ?? terminal.sessionId,
                )} ${agentLabel(agentType)}`}
                onClick={() => onFocus(terminal)}
              >
                <span
                  className={`console-dock-panel__quick-icon console-dock-panel__quick-icon--${agentLogoClass(
                    agentType,
                  )}`}
                  aria-hidden="true"
                >
                  {logo ? <img src={logo} alt="" /> : <span>{agentLogoText(agentType)}</span>}
                </span>
                <span className="console-dock-panel__navigator-main">
                  <span>{terminal.repo ? busStore.displayName(terminal.repo) : "Session"}</span>
                  <small>
                    <i aria-hidden="true" />
                    {agentLabel(agentType)} / {sessionStatusLabel(session)}
                  </small>
                  {preview && <em>{preview}</em>}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {journalSessions.length > 0 && (
        <div className="console-dock-panel__navigator-section">
          <div className="console-dock-panel__navigator-head">
            <span>Saved</span>
            <small>{journalSessions.length}</small>
          </div>
          <div className="console-dock-panel__navigator-list">
            {journalSessions.slice(0, 8).map((session) => {
              const logo = agentLogoSrc(session.agent_type);
              return (
                <button
                  className="console-dock-panel__navigator-item"
                  type="button"
                  key={session.id}
                  aria-label={`Open ${busStore.displayName(session.repo)} ${agentLabel(
                    session.agent_type,
                  )} transcript`}
                  onClick={() => onOpenJournal(session)}
                >
                  <span
                    className={`console-dock-panel__quick-icon console-dock-panel__quick-icon--${agentLogoClass(
                      session.agent_type,
                    )}`}
                    aria-hidden="true"
                  >
                    {logo ? (
                      <img src={logo} alt="" />
                    ) : (
                      <span>{agentLogoText(session.agent_type)}</span>
                    )}
                  </span>
                  <span className="console-dock-panel__navigator-main">
                    <span>{busStore.displayName(session.repo)}</span>
                    <small>
                      {openingJournalId === session.id
                        ? "Opening"
                        : `${agentLabel(session.agent_type)} transcript`}
                    </small>
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      )}
    </aside>
  );
}

function activeSessionPreview(
  session: AgentSession | undefined,
  timeline: AgentSessionTimelineItem[] | undefined,
): string | null {
  const lastMessage = [...(timeline ?? [])]
    .reverse()
    .find((item) => item.kind !== "lifecycle" && item.text.trim().length > 0);
  if (lastMessage) return lastMessage.text;
  if (!session) return null;
  if (session.turn_status === "working") return "Working on the current turn";
  if (session.turn_status === "settling") return "Collecting changed files";
  return null;
}

function sessionStatusLabel(session: AgentSession | undefined): string {
  if (!session) return "opening";
  if (session.status === "running" && session.turn_status === "working") return "working";
  if (session.status === "running" && session.turn_status === "settling") return "settling";
  if (session.status === "running") return "ready";
  return session.status.replace(/_/g, " ");
}

function sessionStatusTone(session: AgentSession | undefined): "live" | "settling" | "done" {
  if (!session) return "live";
  if (session.status === "running" && session.turn_status === "settling") return "settling";
  if (session.status === "running") return "live";
  return "done";
}

function AgentJournalBrowser({
  sessions,
  openingJournalId,
  onOpen,
}: {
  sessions: AgentJournalSessionSummary[];
  openingJournalId: string | null;
  onOpen: (session: AgentJournalSessionSummary) => void;
}) {
  return (
    <section className="console-dock-panel__journal" aria-label="Recent agent sessions">
      <div className="console-dock-panel__journal-head">
        <span>Recent sessions</span>
        <small>Saved locally</small>
      </div>
      <div className="console-dock-panel__journal-list">
        {sessions.map((session) => {
          const logo = agentLogoSrc(session.agent_type);
          return (
            <button
              className="console-dock-panel__journal-card"
              type="button"
              key={session.id}
              aria-label={`Open ${busStore.displayName(session.repo)} ${agentLabel(
                session.agent_type,
              )} transcript`}
              onPointerDown={(event) => event.stopPropagation()}
              onMouseDown={(event) => event.stopPropagation()}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onOpen(session);
              }}
            >
              <span
                className={`console-dock-panel__quick-icon console-dock-panel__quick-icon--${agentLogoClass(
                  session.agent_type,
                )}`}
                aria-hidden="true"
              >
                {logo ? (
                  <img src={logo} alt="" />
                ) : (
                  <span>{agentLogoText(session.agent_type)}</span>
                )}
              </span>
              <span className="console-dock-panel__journal-main">
                <span>{busStore.displayName(session.repo)}</span>
                <small>
                  {agentLabel(session.agent_type)} / {sessionLabel(session)}
                </small>
                {session.last_event_text && <em>{session.last_event_text}</em>}
              </span>
              <span className="console-dock-panel__journal-action">
                {openingJournalId === session.id ? "Opening" : "Open"}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function terminalTitle(params: TerminalPanelParams): string {
  const agent = params.agentType ?? "agent";
  const short = params.sessionId.length > 8 ? params.sessionId.slice(0, 8) : params.sessionId;
  return `${agent} ${short}`;
}

function groupRecentLaunches(launches: RecentAgentLaunch[]): RecentAgentLaunchGroup[] {
  const groups = new Map<string, RecentAgentLaunchGroup>();
  for (const launch of launches) {
    const group = groups.get(launch.repo) ?? {
      repo: launch.repo,
      launches: [],
      lastUsedAt: 0,
    };
    group.launches.push(launch);
    group.lastUsedAt = Math.max(group.lastUsedAt, launch.lastUsedAt);
    groups.set(launch.repo, group);
  }
  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      launches: group.launches.sort(
        (a, b) => b.lastUsedAt - a.lastUsedAt || a.agentType.localeCompare(b.agentType),
      ),
    }))
    .sort((a, b) => b.lastUsedAt - a.lastUsedAt || a.repo.localeCompare(b.repo));
}

function recentLaunchKey(launch: RecentAgentLaunch): string {
  return `${launch.repo}\u0000${launch.agentType}`;
}

function sessionLabel(session: AgentJournalSessionSummary): string {
  const status = session.status === "exited" ? "archived" : session.status.replace(/_/g, " ");
  const count = session.event_count === 1 ? "1 event" : `${session.event_count} events`;
  return `${status} / ${count}`;
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

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Launch failed");
  }
  return String(error || "Launch failed");
}
