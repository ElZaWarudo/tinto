import React from "react";
import { createRoot } from "react-dom/client";
import { agentSessionStore } from "../agent/sessionStore";
import type {
  AgentSession,
  AgentSessionTimelineItem,
  FileDiff,
  RepoDelta,
  WorkbenchConfig,
} from "../bus/contract";
import { busStore } from "../bus/store";
import { ConsoleDockPanel } from "../panels/terminal/ConsoleDockPanel";
import { TerminalPanel, type TerminalPanelParams } from "../panels/terminal/TerminalPanel";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import { consoleDock } from "../workspace/consoleDock";
import { markRecentAgentLaunch } from "../workspace/recentAgentLaunches";
import "../App.css";
import "./agentLensRestorable.css";

const REPO = "C:\\Users\\User\\Documents\\personal\\digital-product-passport";
const TINTO_REPO = "C:\\Users\\User\\Documents\\personal\\tinto";
const SESSION_ID = "demo-restorable-session";
const NOW = 1_798_800_000_000;
const QUERY = new URLSearchParams(window.location.search);
const LIVE_PROCESS = QUERY.get("state") === "working";
const JOURNAL_MODE = QUERY.get("mode") === "journal";
const COMPACT_FIXTURE = QUERY.get("viewport") === "compact";
const NAVIGATOR_FIXTURE = QUERY.get("surface") === "navigator";
const AGENTS_HOME_FIXTURE = QUERY.get("surface") === "agents-home";

const turnChanges = [
  { path: "src/agent-view.tsx", kind: "modified" as const, timestamp_ms: NOW + 12_000 },
  { path: "src/restore/turns.ts", kind: "created" as const, timestamp_ms: NOW + 18_000 },
  { path: "docs/agent-lens.md", kind: "modified" as const, timestamp_ms: NOW + 24_000 },
];

const timeline: AgentSessionTimelineItem[] = [
  {
    session_id: SESSION_ID,
    id: "demo:user-1",
    kind: "user_message",
    text: "Add natural restore points to Agent Lens and keep the recovery path visible.",
    timestamp_ms: NOW + 1_000,
  },
  {
    session_id: SESSION_ID,
    id: "demo:agent-1",
    kind: "agent_message",
    text: "Voy a usar ",
    timestamp_ms: NOW + 16_000,
  },
  {
    session_id: SESSION_ID,
    id: "demo:agent-1b",
    kind: "agent_message",
    text: "`caveman` para mantener la respuesta compacta, ",
    timestamp_ms: NOW + 16_001,
  },
  {
    session_id: SESSION_ID,
    id: "demo:agent-1c",
    kind: "agent_message",
    text: "`memo` para recuperar contexto previo y `dpp-wise` para reflejar la arquitectura real del repositorio. ",
    timestamp_ms: NOW + 16_002,
  },
  {
    session_id: SESSION_ID,
    id: "demo:agent-1d",
    kind: "agent_message",
    text: "Consultaré sólo las fuentes esenciales y verificaré el flujo completo antes de resumir los cambios.",
    timestamp_ms: NOW + 16_003,
  },
  {
    session_id: SESSION_ID,
    id: "demo:cmd-1",
    kind: "command_output",
    text: "npm test -- src\\panels\\terminal\\TerminalPanel.test.tsx --run",
    timestamp_ms: NOW + 20_000,
  },
  {
    session_id: SESSION_ID,
    id: "demo:user-2",
    kind: "user_message",
    text: "Now prove the completed session can be restored from a saved turn checkpoint.",
    timestamp_ms: NOW + 31_000,
  },
  {
    session_id: SESSION_ID,
    id: "demo:agent-2",
    kind: "agent_message",
    text: "Added a backend registry smoke for completed-session restore and kept the visual metric at 2/2.",
    timestamp_ms: NOW + 46_000,
  },
  ...(LIVE_PROCESS
    ? [
        {
          session_id: SESSION_ID,
          id: "demo:activity-1",
          kind: "activity" as const,
          text: "Ejecutando npm test -- --run src/panels/terminal/TerminalPanel.test.tsx",
          timestamp_ms: NOW + 47_000,
        },
      ]
    : []),
];

const session: AgentSession = {
  id: SESSION_ID,
  repo: REPO,
  agent_type: "codex",
  wsl_distro: "Ubuntu",
  status: LIVE_PROCESS ? "running" : "completed",
  pid: LIVE_PROCESS ? 4_242 : null,
  started_at_ms: NOW,
  ended_at_ms: LIVE_PROCESS ? null : NOW + 52_000,
  exit_code: LIVE_PROCESS ? null : 0,
  error: null,
  checkpoint: {
    checkpoint_type: "fs_snapshot",
    git_hash: null,
    snapshot_files: ["src/agent-view.tsx", "src/restore/turns.ts", "docs/agent-lens.md"],
  },
  change_log: turnChanges,
  turn_status: LIVE_PROCESS ? "working" : "waiting",
  turn_checkpoints: [
    {
      id: `${SESSION_ID}:turn-1`,
      index: 1,
      started_at_ms: NOW + 1_000,
      ended_at_ms: NOW + 24_000,
      checkpoint: {
        checkpoint_type: "fs_snapshot",
        git_hash: null,
        snapshot_files: ["src/agent-view.tsx"],
      },
      restore_checkpoint: {
        checkpoint_type: "fs_snapshot",
        git_hash: null,
        snapshot_files: ["src/agent-view.tsx"],
      },
      changes: turnChanges.slice(0, 2),
    },
    {
      id: `${SESSION_ID}:turn-2`,
      index: 2,
      started_at_ms: NOW + 31_000,
      ended_at_ms: NOW + 50_000,
      checkpoint: {
        checkpoint_type: "fs_snapshot",
        git_hash: null,
        snapshot_files: ["docs/agent-lens.md"],
      },
      restore_checkpoint: {
        checkpoint_type: "fs_snapshot",
        git_hash: null,
        snapshot_files: ["docs/agent-lens.md"],
      },
      changes: turnChanges.slice(2),
    },
  ],
  timeline,
  runtime_options: {
    model: "gpt-5.5",
    reasoning_effort: "high",
    speed: "standard",
  },
  goal: {
    text: "Make Agent Lens expose restorable turns for completed Codex sessions.",
    status: "active",
    token_budget: 200_000,
    tokens_used: 36_000,
    time_used_seconds: 482,
    created_at_ms: NOW - 452_000,
    updated_at_ms: NOW + 30_000,
  },
  personality: null,
  plan_mode: null,
  feedback: [],
  context_summary: {
    text: "Two-turn restorable-session demo with live file context and completed backend restore coverage.",
    created_at_ms: NOW + 52_000,
    source_events: 5,
    source_turns: 2,
  },
  active_sessions: LIVE_PROCESS ? 1 : 0,
  age_ms: 52_000,
  output_bytes_per_second: LIVE_PROCESS ? 84 : 0,
};

const diff = (path: string, added: string[], removed: string[] = []): FileDiff => ({
  path,
  old_path: null,
  is_binary: false,
  hunks: [
    {
      old_start: 1,
      new_start: 1,
      lines: [
        ...removed.map((content, index) => ({
          kind: "Removed" as const,
          content,
          old_lineno: index + 1,
          new_lineno: null,
        })),
        ...added.map((content, index) => ({
          kind: "Added" as const,
          content,
          old_lineno: null,
          new_lineno: index + 1,
        })),
      ],
    },
  ],
});

const repoDelta: RepoDelta = {
  repo: REPO,
  revision: 7,
  status: {
    modified: ["src/agent-view.tsx", "docs/agent-lens.md"],
    staged: ["src/restore/turns.ts"],
    untracked: [],
  },
  branch: {
    name: "codex/rdm-016-agent-lens-restore",
    detached: false,
    unborn: false,
    ahead: 3,
    behind: 0,
  },
  head: {
    id: "a1b2c3d",
    summary: "Agent Lens restore metric",
    author: "Tinto Fixture",
    timestamp: Math.floor(NOW / 1000),
  },
  last_activity_ms: NOW + 52_000,
  error: null,
  metrics: { changed_files: 3, lines_added: 48, lines_removed: 9 },
  gitleaks_configured: true,
  agents_md_configured: true,
  secret_scan_status: { state: "clean", engine: "gitleaks", version: "8.30.1" },
  signals: [],
  secret_findings: [],
  subscribed_diffs: [
    diff("src/agent-view.tsx", ["const restorePoints = '2/2';"], ["const restorePoints = null;"]),
    diff("src/restore/turns.ts", ["export const restoreReady = true;"]),
    diff("docs/agent-lens.md", ["- Completed sessions show restore coverage in Agent Lens."]),
  ],
};

const config: WorkbenchConfig = {
  version: 1,
  active: "Agent Lens fixture",
  workbenches: [
    {
      name: "Agent Lens fixture",
      repos: [{ path: REPO, alias: "digital-product-passport", fs_watch: [] }],
    },
  ],
};

function installTauriFixture() {
  const windowWithTauri = window as typeof window & {
    __TAURI_INTERNALS__?: {
      invoke?: (cmd: string, args?: unknown) => Promise<unknown>;
      transformCallback?: () => number;
    };
  };
  windowWithTauri.__TAURI_INTERNALS__ = {
    ...(windowWithTauri.__TAURI_INTERNALS__ ?? {}),
    invoke: async (cmd: string) => {
      if (cmd === "list_agent_sessions") return [session];
      if (cmd === "list_agent_journal_sessions") {
        const savedSessions = [
          {
            id: "demo-saved-session",
            repo: REPO,
            agent_type: "codex",
            status: "completed",
            started_at_ms: NOW - 120_000,
            ended_at_ms: NOW - 60_000,
            updated_at_ms: NOW - 60_000,
            event_count: 4,
            last_event_kind: "agent_message",
            last_event_text: "Cambios verificados y listos.",
            last_event_at_ms: NOW - 60_000,
          },
        ];
        if (!AGENTS_HOME_FIXTURE) return savedSessions;
        return [
          ...savedSessions,
          ...[
            "Replantea el panel de Agents sin cortar el historial",
            "Corrige la navegación de los archivos del chat",
            "Conserva el hilo al reanudar una sesión",
            "Revisa los estados de carga y recuperación",
            "Pulir el flujo de intervención durante el turno",
            "Verifica la aplicación en una ventana estrecha",
          ].map((firstUserMessage, index) => ({
            ...savedSessions[0],
            id: `demo-saved-session-${index + 2}`,
            repo: index >= 3 ? TINTO_REPO : REPO,
            first_user_message: firstUserMessage,
            updated_at_ms: NOW - (index + 2) * 60_000,
            event_count: 18 + index * 7,
            last_event_text: "Cambios aplicados y verificados en el panel.",
          })),
        ];
      }
      if (cmd === "get_agent_journal_session") return session;
      if (cmd === "resume_agent_journal_session") {
        return { session_id: "fixture-resumed", mode: "native" };
      }
      if (cmd === "restore_session_turn") return session;
      if (cmd === "revert_session_turn_file") return session;
      if (cmd === "revert_session") return { ...session, status: "reverted" };
      if (cmd === "stop_agent_session") return null;
      if (cmd === "write_agent_session_input") return null;
      if (cmd === "write_agent_session_turn") return null;
      if (cmd === "get_agent_image_preview") return null;
      if (cmd === "plugin:dialog|open") {
        return ["C:\\Temp\\screen.png", "C:\\Temp\\requirements.pdf"];
      }
      if (cmd === "run_agent_host_command") return { status: "completed", message: "demo" };
      return null;
    },
    transformCallback: () => 0,
  };
}

installTauriFixture();
agentSessionStore.reset();
agentSessionStore.setSessions([session]);
busStore.resetAll();
busStore.setConfig(config);
busStore.loadSnapshot([repoDelta], { available: true });
if (AGENTS_HOME_FIXTURE) {
  markRecentAgentLaunch({ repo: REPO, agentType: "codex" });
  markRecentAgentLaunch({
    repo: TINTO_REPO,
    agentType: "codex",
  });
}
if (NAVIGATOR_FIXTURE) {
  consoleDock.openTerminal({ sessionId: SESSION_ID, repo: REPO, agentType: "codex" });
}

const actions: WorkspaceActions = {
  openRepo: () => {},
  addRepo: () => {},
  removeRepo: () => {},
  openFile: () => {},
  openTimeline: () => {},
  openDashboard: () => {},
  openAgents: () => {},
  openAgentTerminal: (params) => {
    document.documentElement.dataset.openedAgentSession = params.sessionId;
  },
};

const panelProps = {
  params: {
    sessionId: SESSION_ID,
    repo: REPO,
    agentType: "codex",
    mode: JOURNAL_MODE ? "journal" : "live",
  } satisfies TerminalPanelParams,
  api: {
    id: "agent-lens-restorable-fixture",
    isActive: true,
    setActive: () => {},
    onDidActiveChange: () => ({ dispose: () => {} }),
  },
  containerApi: {},
} as unknown as React.ComponentProps<typeof TerminalPanel>;

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <WorkspaceActionsContext.Provider value={actions}>
      <div className={`agent-lens-fixture${COMPACT_FIXTURE ? " agent-lens-fixture--compact" : ""}`}>
        <div className="agent-lens-fixture__panel">
          {NAVIGATOR_FIXTURE || AGENTS_HOME_FIXTURE ? (
            <ConsoleDockPanel />
          ) : (
            <TerminalPanel {...panelProps} />
          )}
        </div>
      </div>
    </WorkspaceActionsContext.Provider>
  </React.StrictMode>,
);
