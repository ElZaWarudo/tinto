import { useCallback, useEffect, useRef, useState } from "react";
import { DockviewReact, themeVisualStudio } from "dockview-react";
import type {
  DockviewReadyEvent,
  DockviewWillDropEvent,
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
  PANEL_AGENT_CONSOLES,
  PANEL_AGENT_TERMINAL,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "../../workspace/panels";
import { consoleDock } from "../../workspace/consoleDock";
import { armExternalTabDetach } from "../../workspace/externalTabDetach";
import { ensureCompactAgentWorkspace } from "../../workspace/openAgentTerminal";
import {
  forgetRecentAgentLaunch,
  readRecentAgentLaunches,
  type RecentAgentLaunch,
} from "../../workspace/recentAgentLaunches";
import { TerminalPanel, type TerminalPanelParams } from "./TerminalPanel";
import { detachTerminalFromConsoleDrop, detachTerminalPanel } from "./detachTerminalPanel";

const consoleComponents = {
  [PANEL_AGENT_TERMINAL]: TerminalPanel,
};

interface RecentAgentLaunchGroup {
  repo: string;
  launches: RecentAgentLaunch[];
  lastUsedAt: number;
}

type JournalLoadState = "loading" | "ready" | "error";

type ConsoleDockPanelProps = Partial<IDockviewPanelProps> & {
  restoreTransferLayout?: boolean;
};

export function ConsoleDockPanel({
  api: workspacePanelApi,
  containerApi,
  restoreTransferLayout = false,
}: ConsoleDockPanelProps) {
  const { repos } = useBusState();
  const agentState = useAgentSessionState();
  const apiRef = useRef<DockviewReadyEvent["api"] | null>(null);
  const dockHostRef = useRef<HTMLDivElement | null>(null);
  const layoutFrameRef = useRef<number | null>(null);
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
  const [journalLoadState, setJournalLoadState] = useState<JournalLoadState>("loading");
  const [journalRequestVersion, setJournalRequestVersion] = useState(0);
  const [workspaceExpanded, setWorkspaceExpanded] = useState(
    () => workspacePanelApi?.isMaximized() ?? false,
  );

  const scheduleConsoleDockLayout = useCallback(() => {
    if (layoutFrameRef.current !== null) {
      window.cancelAnimationFrame(layoutFrameRef.current);
    }
    layoutFrameRef.current = window.requestAnimationFrame(() => {
      layoutFrameRef.current = null;
      const api = apiRef.current;
      const host = dockHostRef.current;
      if (!api || !host) return;
      const bounds = host.getBoundingClientRect();
      const width = Math.round(bounds.width);
      const height = Math.round(bounds.height);
      if (width > 0 && height > 0) {
        api.layout(width, height, true);
      }
    });
  }, []);

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
      if (layoutFrameRef.current !== null) {
        window.cancelAnimationFrame(layoutFrameRef.current);
        layoutFrameRef.current = null;
      }
      if (apiRef.current) {
        consoleDock.unregister(apiRef.current);
      }
    };
  }, []);

  useEffect(() => {
    const host = dockHostRef.current;
    if (!host) return;
    const observer = new ResizeObserver(scheduleConsoleDockLayout);
    observer.observe(host);
    scheduleConsoleDockLayout();
    return () => observer.disconnect();
  }, [scheduleConsoleDockLayout]);

  useEffect(() => {
    if (!containerApi || !workspacePanelApi) return;
    const updateExpandedState = () => setWorkspaceExpanded(workspacePanelApi.isMaximized());
    updateExpandedState();
    const disposable = containerApi.onDidMaximizedGroupChange(updateExpandedState);
    return () => disposable.dispose();
  }, [containerApi, workspacePanelApi]);

  useEffect(() => {
    let active = true;
    void listAgentJournalSessions(18)
      .then((sessions) => {
        if (active) {
          setJournalSessions(sessions);
          setJournalLoadState("ready");
        }
      })
      .catch(() => {
        if (active) {
          setJournalSessions([]);
          setJournalLoadState("error");
        }
      });
    return () => {
      active = false;
    };
  }, [terminalCount, journalRequestVersion]);

  const retryJournalLoad = () => {
    setJournalLoadState("loading");
    setJournalRequestVersion((version) => version + 1);
  };

  const onReady = (event: DockviewReadyEvent) => {
    movePanelDisposeRef.current?.();
    apiRef.current = event.api;
    consoleDock.register(event.api, { restoreTransferLayout });
    scheduleConsoleDockLayout();
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
    if (containerApi) {
      setWorkspaceExpanded(ensureCompactAgentWorkspace(containerApi));
      containerApi.getPanel(PANEL_AGENT_CONSOLES)?.api.setActive();
    }
    const api = apiRef.current;
    if (!api) {
      scheduleConsoleDockLayout();
      return;
    }
    const id = agentTerminalPanelId(params.sessionId);
    const existing = api.getPanel(id);
    if (existing) {
      existing.api.setActive();
      setOpenTerminals(consoleDock.openTerminalParams());
      scheduleConsoleDockLayout();
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
      scheduleConsoleDockLayout();
    } catch {
      api.getPanel(id)?.api.setActive();
      setOpenTerminals(consoleDock.openTerminalParams());
      scheduleConsoleDockLayout();
    }
  };

  const restoreWorkspaceLayout = () => {
    workspacePanelApi?.exitMaximized();
    setWorkspaceExpanded(false);
    scheduleConsoleDockLayout();
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
          setLaunchError("No se encontró la transcripción de esta sesión.");
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
          journalLoadState={journalLoadState}
          workspaceExpanded={workspaceExpanded}
          onFocus={ensureTerminalPanelVisible}
          onOpenJournal={openJournalTranscript}
          onRetryJournalLoad={retryJournalLoad}
          onRestoreWorkspace={restoreWorkspaceLayout}
        />
      )}
      <div className="console-dock-panel__dock" ref={dockHostRef}>
        <DockviewReact
          components={consoleComponents}
          dndStrategy="pointer"
          theme={themeVisualStudio}
          onReady={onReady}
          onWillDrop={onWillDrop}
        />
      </div>
      {terminalCount === 0 && (
        <div className="console-dock-panel__empty" data-testid="console-empty">
          <div className="console-dock-panel__empty-title">No hay Agents activos</div>
          <JournalLoadNotice state={journalLoadState} onRetry={retryJournalLoad} />
          {quickLaunchGroups.length > 0 ? (
            <>
              <div className="console-dock-panel__empty-subtitle">Inicio rápido</div>
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
                                aria-label={`Iniciar ${busStore.displayName(
                                  launch.repo,
                                )} con ${agentLabel(launch.agentType)}`}
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
                                  <small>Reciente</small>
                                </span>
                                <span className="console-dock-panel__quick-action">
                                  {launchingKey === key ? "Iniciando…" : "Ejecutar"}
                                </span>
                              </button>
                              <button
                                className="console-dock-panel__quick-remove"
                                type="button"
                                aria-label={`Quitar ${busStore.displayName(
                                  launch.repo,
                                )} con ${agentLabel(launch.agentType)} del inicio rápido`}
                                onPointerDown={(event) => event.stopPropagation()}
                                onMouseDown={(event) => event.stopPropagation()}
                                onClick={(event) => {
                                  event.preventDefault();
                                  event.stopPropagation();
                                  removeRecent(launch);
                                }}
                              >
                                ×
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
                <div className="console-dock-panel__quick-error" role="alert">
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
              {journalSessions.length > 0 && (
                <AgentJournalBrowser
                  sessions={journalSessions}
                  openingJournalId={openingJournalId}
                  onOpen={openJournalTranscript}
                />
              )}
              {launchError && (
                <div className="console-dock-panel__quick-error" role="alert">
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

function JournalLoadNotice({ state, onRetry }: { state: JournalLoadState; onRetry: () => void }) {
  if (state === "ready") return null;
  if (state === "loading") {
    return (
      <div className="console-dock-panel__quick-error" role="status" aria-live="polite">
        Cargando sesiones guardadas…
      </div>
    );
  }
  return (
    <div className="console-dock-panel__quick-error" role="alert">
      <span>No se pudieron cargar las sesiones guardadas.</span>
      <button type="button" onClick={onRetry}>
        Reintentar
      </button>
    </div>
  );
}

function AgentNavigator({
  openTerminals,
  sessions,
  timeline,
  journalSessions,
  openingJournalId,
  journalLoadState,
  workspaceExpanded,
  onFocus,
  onOpenJournal,
  onRetryJournalLoad,
  onRestoreWorkspace,
}: {
  openTerminals: TerminalPanelParams[];
  sessions: Record<string, AgentSession>;
  timeline: Record<string, AgentSessionTimelineItem[]>;
  journalSessions: AgentJournalSessionSummary[];
  openingJournalId: string | null;
  journalLoadState: JournalLoadState;
  workspaceExpanded: boolean;
  onFocus: (params: TerminalPanelParams) => void;
  onOpenJournal: (session: AgentJournalSessionSummary) => void;
  onRetryJournalLoad: () => void;
  onRestoreWorkspace: () => void;
}) {
  return (
    <aside className="console-dock-panel__navigator" aria-label="Sesiones de Agents">
      <div className="console-dock-panel__navigator-section">
        <div className="console-dock-panel__navigator-head">
          <span>Activas</span>
          <span className="console-dock-panel__navigator-head-actions">
            <small>{openTerminals.length}</small>
            {workspaceExpanded && (
              <button
                className="console-dock-panel__navigator-restore"
                type="button"
                onClick={onRestoreWorkspace}
              >
                Restaurar
              </button>
            )}
          </span>
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
                aria-label={`Mostrar ${busStore.displayName(
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
                  <span>{terminal.repo ? busStore.displayName(terminal.repo) : "Sesión"}</span>
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

      <JournalLoadNotice state={journalLoadState} onRetry={onRetryJournalLoad} />

      {journalSessions.length > 0 && (
        <div className="console-dock-panel__navigator-section">
          <div className="console-dock-panel__navigator-head">
            <span>Guardadas</span>
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
                  aria-label={`Abrir la transcripción de ${busStore.displayName(
                    session.repo,
                  )} con ${agentLabel(session.agent_type)}`}
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
                        ? "Abriendo…"
                        : `Transcripción de ${agentLabel(session.agent_type)}`}
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
  if (session.turn_status === "working") return "Trabajando en el turno actual";
  if (session.turn_status === "settling") return "Recopilando los archivos modificados";
  return null;
}

function sessionStatusLabel(session: AgentSession | undefined): string {
  if (!session) return "abriendo";
  if (session.status === "running" && session.turn_status === "working") return "trabajando";
  if (session.status === "running" && session.turn_status === "settling") {
    return "recopilando cambios";
  }
  if (session.status === "running") return "listo";
  return localizedSessionStatus(session.status);
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
    <section className="console-dock-panel__journal" aria-label="Sesiones recientes de Agents">
      <div className="console-dock-panel__journal-head">
        <span>Sesiones recientes</span>
        <small>Guardadas en este equipo</small>
      </div>
      <div className="console-dock-panel__journal-list">
        {sessions.map((session) => {
          const logo = agentLogoSrc(session.agent_type);
          return (
            <button
              className="console-dock-panel__journal-card"
              type="button"
              key={session.id}
              aria-label={`Abrir la transcripción de ${busStore.displayName(
                session.repo,
              )} con ${agentLabel(session.agent_type)}`}
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
                {openingJournalId === session.id ? "Abriendo…" : "Abrir"}
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
  const status = localizedSessionStatus(session.status);
  const count = session.event_count === 1 ? "1 evento" : `${session.event_count} eventos`;
  return `${status} / ${count}`;
}

function localizedSessionStatus(status: string): string {
  switch (status) {
    case "running":
      return "en ejecución";
    case "completed":
      return "completada";
    case "failed":
      return "fallida";
    case "cancelled":
    case "canceled":
      return "cancelada";
    case "exited":
      return "archivada";
    case "starting":
      return "iniciando";
    case "waiting":
      return "en espera";
    default:
      return status.replace(/_/g, " ");
  }
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
    return String((error as { message?: unknown }).message ?? "No se pudo iniciar el Agent");
  }
  return String(error || "No se pudo iniciar el Agent");
}
