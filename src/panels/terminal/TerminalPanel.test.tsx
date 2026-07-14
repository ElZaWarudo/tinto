import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AgentHostCommandResult,
  AgentRuntimeCatalog,
  AgentSession,
  FileDiff,
  RepoDelta,
} from "../../bus/contract";

const writeAgentSessionInputMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const listAgentSessionsMock = vi.fn<() => Promise<AgentSession[]>>(() => Promise.resolve([]));
const getAgentJournalSessionMock = vi.fn<() => Promise<AgentSession | null>>(() =>
  Promise.resolve(null),
);
const getAgentRuntimeCatalogMock = vi.fn<
  (sessionId: string, refresh?: boolean) => Promise<AgentRuntimeCatalog | null>
>(() => Promise.resolve(runtimeCatalogFixture()));
const revertSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(sessionFixture({ status: "reverted", reverted_at_ms: 3 }));
});
const revertSessionTurnFileMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(sessionFixture({ status: "completed", turn_checkpoints: [] }));
});
const restoreSessionTurnMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(
    sessionFixture({ status: "completed", restored_to_turn_index: 1, turn_checkpoints: [] }),
  );
});
const stopAgentSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const runAgentHostCommandMock = vi.fn((...args: unknown[]): Promise<AgentHostCommandResult> => {
  void args;
  return Promise.resolve({ command: "status", status: "completed", message: "Host command done." });
});
const confirmMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(true);
});
const scrollIntoViewMock = vi.fn();
const writeClipboardTextMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});

vi.mock("../../bus/client", () => ({
  getAgentJournalSession: () => getAgentJournalSessionMock(),
  getAgentRuntimeCatalog: (sessionId: string, refresh?: boolean) =>
    getAgentRuntimeCatalogMock(sessionId, refresh),
  listAgentSessions: () => listAgentSessionsMock(),
  revertSession: (...a: unknown[]) => revertSessionMock(...a),
  revertSessionTurnFile: (...a: unknown[]) => revertSessionTurnFileMock(...a),
  restoreSessionTurn: (...a: unknown[]) => restoreSessionTurnMock(...a),
  runAgentHostCommand: (...a: unknown[]) => runAgentHostCommandMock(...a),
  stopAgentSession: (...a: unknown[]) => stopAgentSessionMock(...a),
  writeAgentSessionInput: (...a: unknown[]) => writeAgentSessionInputMock(...a),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: (...a: unknown[]) => confirmMock(...a),
}));

import { TerminalPanel, type TerminalPanelParams } from "./TerminalPanel";
import { agentSessionStore } from "../../agent/sessionStore";
import { busStore } from "../../bus/store";
import { markTerminalDetached } from "./detachTerminalWindow";
import { WorkspaceActionsContext, type WorkspaceActions } from "../../workspace/actions";
import { consoleDock } from "../../workspace/consoleDock";

function sessionFixture(overrides: Partial<AgentSession> = {}): AgentSession {
  return {
    id: "sess-1",
    repo: "/r/a",
    agent_type: "codex",
    status: "running",
    pid: 123,
    started_at_ms: 1,
    ended_at_ms: null,
    exit_code: null,
    error: null,
    checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
    change_log: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
    turn_status: "working",
    turn_checkpoints: [],
    reverted_at_ms: null,
    restored_to_turn_index: null,
    active_sessions: 1,
    age_ms: 1,
    output_bytes_per_second: null,
    ...overrides,
  };
}

function runtimeCatalogFixture(overrides: Partial<AgentRuntimeCatalog> = {}): AgentRuntimeCatalog {
  return {
    status: "ready",
    source: "codex_app_server",
    default_model: "gpt-5.6-sol",
    error: null,
    updated_at_ms: 4,
    models: [
      {
        id: "gpt-5.6-sol",
        model: "gpt-5.6-sol",
        display_name: "GPT-5.6 Sol",
        description: "Modelo actual de propósito general.",
        supported_reasoning_efforts: [
          { value: "medium", description: "Equilibrado" },
          { value: "high", description: "Profundo" },
        ],
        default_reasoning_effort: "medium",
        service_tiers: [{ id: "fast", name: "Rápido", description: "Prioriza la latencia." }],
        default_service_tier: null,
        is_default: true,
      },
      {
        id: "gpt-5.6-luna",
        model: "gpt-5.6-luna",
        display_name: "GPT-5.6 Luna",
        description: "Modelo actual para trabajo deliberado.",
        supported_reasoning_efforts: [
          { value: "high", description: "Profundo" },
          { value: "xhigh", description: "Máximo" },
        ],
        default_reasoning_effort: "high",
        service_tiers: [],
        default_service_tier: null,
        is_default: false,
      },
    ],
    ...overrides,
  };
}

function props(params: TerminalPanelParams) {
  return {
    params,
    api: {
      id: `agent-terminal:${params.sessionId}`,
      isActive: true,
      setActive: vi.fn(),
      onDidActiveChange: vi.fn(() => ({ dispose: vi.fn() })),
    },
  } as unknown as IDockviewPanelProps<TerminalPanelParams>;
}

function b64(text: string): string {
  return btoa(text);
}

function repoDelta(overrides: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: "/r/a",
    revision: 1,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    subscribed_diffs: null,
    ...overrides,
  };
}

function fileDiff(path: string, lines: FileDiff["hunks"][number]["lines"]): FileDiff {
  return {
    path,
    old_path: null,
    is_binary: false,
    hunks: [{ old_start: 1, new_start: 1, lines }],
  };
}

function installClipboardMock() {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: writeClipboardTextMock },
  });
}

function renderWithWorkspaceActions(ui: ReactElement, actions: Partial<WorkspaceActions>) {
  const defaults: WorkspaceActions = {
    openRepo: vi.fn(),
    addRepo: vi.fn(),
    removeRepo: vi.fn(),
    openFile: vi.fn(),
    openTimeline: vi.fn(),
    openDashboard: vi.fn(),
    openAgents: vi.fn(),
    openAgentTerminal: vi.fn(),
  };
  return render(
    <WorkspaceActionsContext.Provider value={{ ...defaults, ...actions }}>
      {ui}
    </WorkspaceActionsContext.Provider>,
  );
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoViewMock,
    });
    installClipboardMock();
    agentSessionStore.reset();
    busStore.resetAll();
    consoleDock.resetForTests();
    scrollIntoViewMock.mockClear();
    writeClipboardTextMock.mockClear();
    writeAgentSessionInputMock.mockClear();
    runAgentHostCommandMock.mockClear();
    runAgentHostCommandMock.mockResolvedValue({
      command: "status",
      status: "completed",
      message: "Host command done.",
    });
    listAgentSessionsMock.mockClear();
    getAgentJournalSessionMock.mockClear();
    getAgentJournalSessionMock.mockResolvedValue(null);
    getAgentRuntimeCatalogMock.mockReset();
    getAgentRuntimeCatalogMock.mockResolvedValue(runtimeCatalogFixture());
    revertSessionMock.mockClear();
    revertSessionTurnFileMock.mockClear();
    restoreSessionTurnMock.mockClear();
    stopAgentSessionMock.mockClear();
    confirmMock.mockClear();
  });

  it("renders a product agent interface instead of a terminal surface", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const loading = screen.getByRole("status");
    expect(loading).toHaveAttribute("aria-live", "polite");
    expect(loading).toHaveTextContent("Cargando sesión");
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Detener" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Revertir" })).toBeDisabled();
    expect(screen.getByText("a")).toBeInTheDocument();
    const composer = screen.getByLabelText("Mensaje para Codex");
    expect(composer).toHaveAttribute("placeholder", "Mensaje para Codex");
    const composerHint = composer.getAttribute("aria-describedby");
    expect(composerHint).toBeTruthy();
    expect(document.getElementById(composerHint!)).toHaveTextContent(
      "Escribe / para ver comandos o $ para ver skills.",
    );
    expect(screen.queryByText("Codex + Tinto + skills")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enviar" })).toBeDisabled();
    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(conversation).toHaveAttribute("role", "log");
    expect(conversation).toHaveAttribute("aria-live", "polite");
    expect(within(conversation).getByText("Listo")).toBeInTheDocument();
    expect(
      within(conversation).queryByText("Inicia un turno desde el compositor inferior."),
    ).not.toBeInTheDocument();
    const idleFocus = screen.getByLabelText("Turno seleccionado");
    expect(idleFocus).toHaveTextContent("Inactivo");
    expect(idleFocus).toHaveTextContent("Ningún turno seleccionado");
    expect(screen.getByLabelText("Buscar en la transcripción")).toBeInTheDocument();
    expect(screen.getByText("Todos los turnos")).toBeInTheDocument();
    const overview = screen.getByLabelText("Resumen de la sesión de Agent");
    expect(overview).toHaveTextContent("Turnos");
    expect(overview).toHaveTextContent("Actividad reciente");
    expect(overview).toHaveTextContent("Esperando el primer turno.");
    const activity = screen.getByLabelText("Actividad de Agent");
    expect(activity).toHaveTextContent("Agent está trabajando");
    expect(activity).toHaveTextContent("punto de control del sistema de archivos");
    expect(activity).toHaveTextContent("0 turnos");
    expect(activity).toHaveTextContent("0 archivos");
    expect(activity).toHaveTextContent("Transmisión en reposo");
    expect(screen.getByRole("status")).toHaveTextContent("En ejecución");
    expect(screen.getByRole("status")).toHaveTextContent("Trabajando");
    expect(screen.getByRole("status")).toHaveTextContent("1 cambio");
    expect(screen.queryByTestId("terminal-surface")).not.toBeInTheDocument();
  });

  it("shows active host context that will steer the next turn", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        goal: { text: "Build the host harness", updated_at_ms: 4 },
        personality: { name: "precise", updated_at_ms: 5 },
        plan_mode: { enabled: true, updated_at_ms: 6 },
        context_summary: {
          text: "Review findings are structured and WSL parity is working.",
          created_at_ms: 7,
          source_events: 3,
          source_turns: 2,
        },
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const context = await screen.findByLabelText("Contexto del turno");
    expect(context).toHaveTextContent("Objetivo");
    expect(context).toHaveTextContent("Build the host harness");
    expect(context).toHaveTextContent("Estilo");
    expect(context).toHaveTextContent("precise");
    expect(context).toHaveTextContent("Plan");
    expect(context).toHaveTextContent("Activo");
    expect(context).toHaveTextContent("Resumen");
    expect(context).toHaveTextContent("Review findings are structured and WSL parity is working.");
  });

  it("sends composer text as an agent turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      speed: "standard",
    });
    expect(composer).toHaveValue("");
  });

  it("sends Codex turns with models discovered from the runtime catalog", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Modelo/ }));
    await user.click(screen.getByRole("radio", { name: /GPT-5\.6 Luna/ }));
    await user.click(screen.getByRole("button", { name: /Razonamiento/ }));
    await user.click(screen.getByRole("radio", { name: /^Alto/ }));

    const composer = screen.getByLabelText("Mensaje para Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      model: "gpt-5.6-luna",
      reasoning_effort: "high",
      speed: "standard",
    });
  });

  it("keeps an explicit model that is not present in the current catalog", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ runtime_options: { model: "future-provider-model", speed: "fast" } }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const modelTrigger = await screen.findByRole("button", {
      name: /Modelo: future-provider-model/,
    });
    await user.click(modelTrigger);
    expect(screen.getByRole("radio", { name: /future-provider-model/ })).toBeChecked();
    expect(screen.getByText("NO CATALOGADO")).toBeInTheDocument();
    await user.keyboard("{Escape}");

    const composer = screen.getByLabelText("Mensaje para Codex");
    await user.type(composer, "continúa");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "continúa\r", {
      model: "future-provider-model",
      speed: "fast",
    });
  });

  it("moves focus into the runtime inspector and restores it on Escape", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const modelTrigger = await screen.findByRole("button", { name: /Modelo/ });
    await user.click(modelTrigger);
    const checkedModel = await screen.findByRole("radio", { name: /^Predeterminado/ });
    await waitFor(() => expect(checkedModel).toHaveFocus());
    expect(modelTrigger).toHaveAttribute("aria-expanded", "true");

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Configuración de ejecución" }),
    ).not.toBeInTheDocument();
    expect(modelTrigger).toHaveFocus();
    expect(modelTrigger).toHaveAttribute("aria-expanded", "false");
  });

  it("retries a failed runtime catalog explicitly", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    getAgentRuntimeCatalogMock.mockResolvedValueOnce(
      runtimeCatalogFixture({ status: "error", models: [], error: "No disponible" }),
    );
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Modelo/ }));
    await user.click(screen.getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(getAgentRuntimeCatalogMock).toHaveBeenCalledWith("sess-1", true));
  });

  it("applies natural runtime slash aliases without sending them to the agent", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/modelo gpt-5.5");
    await user.type(composer, "{Enter}");
    await user.type(composer, "/razonamiento alto");
    await user.type(composer, "{Enter}");
    await user.type(composer, "/rápido");
    await user.type(composer, "{Enter}");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledTimes(1);
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      model: "gpt-5.5",
      reasoning_effort: "high",
      speed: "fast",
    });
  }, 10000);

  it("shows a titled error banner when sending a turn fails", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    writeAgentSessionInputMock.mockRejectedValueOnce(new Error("Write failed"));
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const errorBanner = await screen.findByTestId("terminal-panel-error");
    expect(errorBanner).toHaveAttribute("role", "alert");
    expect(errorBanner).toHaveTextContent("Write failed");
    expect(errorBanner).toHaveAttribute(
      "title",
      "Aviso de error de la sesión de Agent: Write failed",
    );
  });

  it("prepares editable turns from composer commands and skill mentions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/test");
    expect(screen.getByRole("listbox", { name: "Comandos del compositor" })).toHaveAttribute(
      "title",
      "Menú de comandos del compositor: 1 comando coincidente con /test.",
    );
    expect(screen.getByRole("option", { name: /\/test/ })).toHaveAttribute(
      "title",
      "Ejecutar /test: Ejecutar la verificación más relevante para este repositorio.",
    );
    await user.type(composer, "{Enter}");

    expect(composer).toHaveValue(
      "Ejecuta la verificación más relevante para este repositorio y resume los fallos antes de corregirlos.",
    );

    await user.type(composer, "{Shift>}{Enter}{/Shift}$warden");
    expect(screen.getByRole("listbox", { name: "Comandos del compositor" })).toHaveAttribute(
      "title",
      "Menú de comandos del compositor: 1 comando coincidente con $warden.",
    );
    expect(screen.getByRole("option", { name: /\$krt-interface-warden/ })).toHaveAttribute(
      "title",
      "Ejecutar $krt-interface-warden: Diseñar o revisar una interfaz de trabajo con identidad propia.",
    );
    await user.type(composer, "{Enter}");
    expect(composer).toHaveValue(
      [
        "Ejecuta la verificación más relevante para este repositorio y resume los fallos antes de corregirlos.",
        "$krt-interface-warden ",
      ].join("\n\n"),
    );

    await user.clear(composer);
    await user.type(composer, "/details");
    expect(screen.getByRole("option", { name: /\/details/ })).toHaveAttribute(
      "title",
      "Ejecutar /details: Abrir los detalles, archivos, comandos, Timeline y puntos de restauración de la sesión.",
    );
  });

  it("shows the non-memory Codex-style command palette", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/");

    const menu = screen.getByRole("listbox", { name: "Comandos del compositor" });
    expect(menu).toHaveAttribute(
      "title",
      expect.stringMatching(/^Menú de comandos del compositor: /),
    );
    for (const command of [
      "/branch",
      "/comments",
      "/compact",
      "/status",
      "/init",
      "/fork",
      "/mcp",
      "/mascot",
      "/model",
      "/plan",
      "/goal",
      "/personality",
      "/reasoning",
      "/review",
      "/fast",
    ]) {
      expect(
        within(menu).getByTitle(`Activador del comando del compositor: ${command}.`),
      ).toBeInTheDocument();
    }
    expect(within(menu).queryByRole("option", { name: /\/memories/ })).not.toBeInTheDocument();
    expect(within(menu).queryByText(/Memorias/)).not.toBeInTheDocument();
  });

  it("supports accessible keyboard navigation in the composer command palette", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/");

    const menu = screen.getByRole("listbox", { name: "Comandos del compositor" });
    expect(composer).toHaveAttribute("aria-controls", menu.id);
    expect(composer).toHaveAttribute("aria-expanded", "true");
    expect(composer).toHaveAttribute("aria-haspopup", "listbox");
    expect(composer).toHaveAttribute("aria-activedescendant", "composer-command-sess-1-branch");
    expect(document.getElementById("composer-command-sess-1-branch")).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.keyboard("{ArrowDown}");
    expect(composer).toHaveAttribute("aria-activedescendant", "composer-command-sess-1-comments");
    expect(document.getElementById("composer-command-sess-1-comments")).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.keyboard("{End}");
    expect(composer).toHaveAttribute("aria-activedescendant", "composer-command-sess-1-details");
    expect(document.getElementById("composer-command-sess-1-details")).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.keyboard("{Home}");
    expect(composer).toHaveAttribute("aria-activedescendant", "composer-command-sess-1-branch");

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("listbox", { name: "Comandos del compositor" }),
    ).not.toBeInTheDocument();
    expect(composer).toHaveAttribute("aria-expanded", "false");
    expect(composer).not.toHaveAttribute("aria-controls");
    expect(composer).not.toHaveAttribute("aria-activedescendant");
  });

  it("opens details from the slash command without sending it as an agent turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "details",
      status: "completed",
      message: "Session details opened in Tinto.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/details");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "details", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(
      await screen.findByTitle(
        "Detalles de la sesión: mapa de turnos, actividad actual, puntos de restauración y Agent Lens.",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText("Session details opened in Tinto.")).toBeInTheDocument();
  });

  it("toggles plan mode through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "plan",
      status: "completed",
      message: "Plan mode enabled for this session.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/plan on");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "plan", "on"),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(await screen.findByText("Plan mode enabled for this session.")).toBeInTheDocument();
  });

  it("runs host slash commands without sending them to the agent", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "status",
      status: "completed",
      message: "Session sess-1: running.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/status");
    expect(screen.getByRole("option", { name: /\/status/ })).toHaveAttribute(
      "title",
      "Ejecutar /status: Mostrar el ID del chat, estado, uso y runtime. Alias disponibles: /estado.",
    );
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "status", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(composer).toHaveValue("");
    expect(await screen.findByText("Session sess-1: running.")).toBeInTheDocument();
  });

  it("toggles the local Tinto companion from the mascot command", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/mascot");
    await user.type(composer, "{Enter}");

    const mascot = await screen.findByLabelText("Asistente Tinto");
    expect(mascot).toHaveAttribute("title", "El asistente Tinto está activo con Codex en a.");
    expect(within(mascot).getByTitle("Estado del asistente Tinto.")).toHaveTextContent("Activo");
    expect(
      await screen.findByText("La mascota está activa en este panel de Agent."),
    ).toBeInTheDocument();
    expect(runAgentHostCommandMock).not.toHaveBeenCalled();
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();

    await user.type(composer, "/mascot");
    await user.type(composer, "{Enter}");

    await waitFor(() => expect(screen.queryByLabelText("Asistente Tinto")).not.toBeInTheDocument());
    expect(await screen.findByText("Mascota oculta.")).toBeInTheDocument();
    expect(runAgentHostCommandMock).not.toHaveBeenCalled();
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("defers memory slash commands without sending them to the agent", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/memorias");
    expect(screen.queryByRole("option", { name: /memories|memorias/i })).not.toBeInTheDocument();
    await user.type(composer, "{Enter}");

    expect(
      await screen.findByText(
        "Los comandos de memoria quedan pendientes para una futura fase de Tinto.",
      ),
    ).toBeInTheDocument();
    expect(composer).toHaveValue("");
    expect(runAgentHostCommandMock).not.toHaveBeenCalled();
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("routes natural Codex slash aliases through canonical host commands", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock
      .mockResolvedValueOnce({
        command: "goal",
        status: "completed",
        message: "Goal set: Build the host harness.",
      })
      .mockResolvedValueOnce({
        command: "review",
        status: "completed",
        message: "Review summary for branch main: no local changes detected.",
        review_summary: {
          branch: "main",
          changed_files: 0,
          working_shortstat: null,
          staged_shortstat: null,
          files: [],
          truncated_count: 0,
        },
        review_findings: [],
      })
      .mockResolvedValueOnce({
        command: "fork",
        status: "completed",
        message: "Forked session sess-child from sess-1.",
        session_id: "sess-child",
        repo: "/r/a",
        agent_type: "codex",
      });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/objective");
    expect(screen.getByRole("option", { name: /\/goal/ })).toHaveAttribute(
      "title",
      "Ejecutar /goal: Establecer un objetivo hacia el que Codex seguirá trabajando. Alias disponibles: /objective, /objetivo.",
    );
    await user.type(composer, " Build the host harness");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith(
        "sess-1",
        "goal",
        "Build the host harness",
      ),
    );
    expect(await screen.findByText("Goal set: Build the host harness.")).toBeInTheDocument();
    await waitFor(() => expect(composer).toHaveValue(""));

    await user.type(composer, "/revisión");
    expect(screen.getByRole("option", { name: /\/review/ })).toBeInTheDocument();
    await user.type(composer, "{Enter}");
    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "review", undefined),
    );
    expect(
      await screen.findByText("Review summary for branch main: no local changes detected."),
    ).toBeInTheDocument();
    await waitFor(() => expect(composer).toHaveValue(""));

    await user.type(composer, "/lateral");
    expect(screen.getByRole("option", { name: /\/fork/ })).toBeInTheDocument();
    await user.type(composer, "{Enter}");
    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "fork", undefined),
    );

    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("runs init through the host command backend from the palette", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "init",
      status: "completed",
      message: "AGENTS.md is configured for this repo.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/init");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "init", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("runs review through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "review",
      status: "completed",
      message: "Review summary for branch main: 2 changed file(s).",
      review_summary: {
        branch: "main",
        changed_files: 2,
        working_shortstat: "1 file changed, 3 insertions(+)",
        staged_shortstat: null,
        files: [" M src/App.tsx", "?? docs/review.md"],
        truncated_count: 0,
      },
      review_findings: [
        {
          severity: "high",
          title: "Conflict marker present",
          detail: "src/App.tsx still contains a merge conflict marker.",
          path: "src/App.tsx",
          line: 12,
        },
      ],
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "review", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    const review = await screen.findByLabelText("Resumen de la revisión");
    expect(review).toHaveAttribute(
      "title",
      "Resumen de la revisión de main: 2 archivos modificados.",
    );
    expect(within(review).getByTitle("Rama revisada.")).toHaveTextContent("main");
    expect(within(review).getByText("1 file changed, 3 insertions(+)")).toBeInTheDocument();
    expect(within(review).getByText("?? docs/review.md")).toBeInTheDocument();
    expect(within(review).getByLabelText("Hallazgos de la revisión")).toBeInTheDocument();
    expect(within(review).getByText("Conflict marker present")).toBeInTheDocument();
    expect(within(review).getByText("src/App.tsx:12")).toBeInTheDocument();
    const copySummary = within(review).getByRole("button", {
      name: "Copiar resumen estructurado de la revisión",
    });
    expect(copySummary).toHaveAttribute(
      "title",
      "Copiar el resumen estructurado de la revisión al portapapeles.",
    );
    expect(
      within(copySummary).getByTitle(
        "Acción para copiar el resumen estructurado de la revisión: Copiar resumen.",
      ),
    ).toHaveTextContent("Copiar resumen");

    installClipboardMock();
    fireEvent.click(copySummary);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Resumen estructurado de la revisión:",
          "Rama: main",
          "Archivos modificados: 2",
          "Diff del árbol de trabajo: 1 file changed, 3 insertions(+)",
          "Diff preparado: sin cambios de líneas preparados",
          "Archivos:",
          "-  M src/App.tsx",
          "- ?? docs/review.md",
          "Hallazgos de la revisión local:",
          "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
        ].join("\n"),
      ),
    );
    expect(copySummary).toHaveAttribute(
      "title",
      "Resumen estructurado de la revisión copiado al portapapeles.",
    );
    expect(
      within(copySummary).getByTitle(
        "Acción para copiar el resumen estructurado de la revisión: Copiado.",
      ),
    ).toHaveTextContent("Copiado");

    const copyFiles = within(review).getByRole("button", {
      name: "Copiar archivos modificados de la revisión",
    });
    expect(copyFiles).toHaveAttribute("title", "Copiar 2 archivos al portapapeles.");
    expect(
      within(copyFiles).getByTitle("Acción para copiar los archivos modificados: Copiar archivos."),
    ).toHaveTextContent("Copiar archivos");

    installClipboardMock();
    fireEvent.click(copyFiles);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        ["Archivos modificados de la revisión:", "-  M src/App.tsx", "- ?? docs/review.md"].join(
          "\n",
        ),
      ),
    );
    expect(copyFiles).toHaveAttribute(
      "title",
      "Archivos modificados de la revisión copiados al portapapeles.",
    );
    expect(
      within(copyFiles).getByTitle("Acción para copiar los archivos modificados: Copiado."),
    ).toHaveTextContent("Copiado");

    const copyFindings = within(review).getByRole("button", {
      name: "Copiar hallazgos automáticos de la revisión",
    });
    expect(copyFindings).toHaveAttribute("title", "Copiar 1 hallazgo al portapapeles.");
    expect(
      within(copyFindings).getByTitle(
        "Acción para copiar los hallazgos automáticos: Copiar hallazgos.",
      ),
    ).toHaveTextContent("Copiar hallazgos");

    installClipboardMock();
    fireEvent.click(copyFindings);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Hallazgos de la revisión local:",
          "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
        ].join("\n"),
      ),
    );
    expect(copyFindings).toHaveAttribute(
      "title",
      "Hallazgos automáticos de la revisión copiados al portapapeles.",
    );
    expect(
      within(copyFindings).getByTitle("Acción para copiar los hallazgos automáticos: Copiados."),
    ).toHaveTextContent("Copiados");

    const reviewPromptButton = within(review).getByRole("button", {
      name: "Preparar prompt de revisión semántica",
    });
    expect(reviewPromptButton).toHaveAttribute(
      "title",
      "Preparar un prompt de revisión semántica del código a partir de este resumen con 1 hallazgo.",
    );
    expect(
      within(reviewPromptButton).getByTitle("Acción para solicitar una revisión semántica."),
    ).toHaveTextContent("Pedir revisión");

    await user.click(reviewPromptButton);
    expect(
      within(review).getByTitle("El prompt de revisión semántica está preparado en el compositor."),
    ).toHaveTextContent("Borrador de revisión listo");
    const draftReset = within(review).getByRole("button", {
      name: "Reiniciar el flujo de revisión semántica",
    });
    expect(draftReset).toHaveAttribute(
      "title",
      "Reiniciar el borrador del prompt de revisión semántica de este resumen.",
    );
    expect(within(draftReset).getByTitle("Reiniciar la revisión semántica.")).toHaveTextContent(
      "Reiniciar revisión",
    );
    const expectedReviewPrompt = [
      "Revisa los cambios actuales de Git en busca de errores, regresiones, riesgos de seguridad y pruebas ausentes.",
      "Rama: main",
      "Archivos modificados: 2",
      "Diff del árbol de trabajo: 1 file changed, 3 insertions(+)",
      "Archivos:",
      "-  M src/App.tsx",
      "- ?? docs/review.md",
      "Hallazgos de la revisión local que debes comprobar primero:",
      "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
      "Presenta primero los hallazgos, ordenados por gravedad y con referencias a archivo y línea cuando sea posible. Si no hay problemas, indícalo con claridad y menciona cualquier carencia que quede en las pruebas.",
    ].join("\n");
    expect(composer).toHaveValue(expectedReviewPrompt);

    const copyPrompt = within(review).getByRole("button", {
      name: "Copiar prompt de revisión semántica",
    });
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Copiar el borrador del prompt de revisión semántica al portapapeles.",
    );
    expect(
      within(copyPrompt).getByTitle(
        "Acción para copiar el prompt de revisión semántica: Copiar prompt.",
      ),
    ).toHaveTextContent("Copiar prompt");

    installClipboardMock();
    fireEvent.click(copyPrompt);
    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith(expectedReviewPrompt));
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Prompt de revisión semántica copiado al portapapeles.",
    );
    expect(
      within(copyPrompt).getByTitle("Acción para copiar el prompt de revisión semántica: Copiado."),
    ).toHaveTextContent("Copiado");

    await user.type(composer, "{Enter}");
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", `${expectedReviewPrompt}\r`, {
      speed: "standard",
    });
    expect(
      await within(review).findByTitle(
        "El prompt de revisión semántica se envió como un turno de Agent.",
      ),
    ).toHaveTextContent("Solicitud de revisión enviada");

    const sentPrompt = String(writeAgentSessionInputMock.mock.calls[0]?.[1] ?? "").trim();
    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:review",
        kind: "user_message",
        text: sentPrompt,
        timestamp_ms: 10,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:review",
        kind: "agent_message",
        text: "Found one high severity issue in src/App.tsx:12. Add a regression test before merging.",
        timestamp_ms: 20,
      });
    });

    expect(
      await within(review).findByTitle(
        "Respuesta de revisión semántica recibida en el turno 1; verifica los hallazgos antes de actuar.",
      ),
    ).toHaveTextContent("Respuesta de revisión recibida");
    expect(within(review).getByText(/Found one high severity issue/)).toBeInTheDocument();
    const responseReset = within(review).getByRole("button", {
      name: "Reiniciar el flujo de revisión semántica",
    });
    expect(responseReset).toHaveAttribute(
      "title",
      "Reiniciar la respuesta capturada y la solicitud de revisión semántica de este resumen.",
    );

    const showRequest = screen.getByRole("button", {
      name: "Mostrar el turno de la solicitud de revisión semántica",
    });
    expect(showRequest).toHaveAttribute(
      "title",
      "Mostrar la solicitud de revisión semántica enviada en el turno 1.",
    );
    expect(
      within(showRequest).getByTitle("Mostrar la solicitud de revisión semántica."),
    ).toHaveTextContent("Ver solicitud");

    scrollIntoViewMock.mockClear();
    fireEvent.click(showRequest);
    await waitFor(() => expect(scrollIntoViewMock).toHaveBeenCalled());

    const showResponse = screen.getByRole("button", {
      name: "Mostrar el turno de la respuesta de revisión semántica",
    });
    expect(showResponse).toHaveAttribute(
      "title",
      "Mostrar la respuesta completa de la revisión semántica en el turno 1.",
    );
    expect(
      within(showResponse).getByTitle("Mostrar la respuesta de revisión semántica."),
    ).toHaveTextContent("Ver respuesta");

    fireEvent.click(showResponse);
    await waitFor(() => expect(scrollIntoViewMock).toHaveBeenCalled());

    const copyResponse = screen.getByRole("button", {
      name: "Copiar respuesta de revisión semántica",
    });
    expect(copyResponse).toHaveAttribute(
      "title",
      "Copiar la respuesta de revisión semántica al portapapeles.",
    );
    expect(
      within(copyResponse).getByTitle(
        "Acción para copiar la respuesta de revisión semántica: Copiar respuesta.",
      ),
    ).toHaveTextContent("Copiar respuesta");

    installClipboardMock();
    fireEvent.click(copyResponse);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        "Found one high severity issue in src/App.tsx:12. Add a regression test before merging.",
      ),
    );
    expect(copyResponse).toHaveAttribute(
      "title",
      "Respuesta de revisión semántica copiada al portapapeles.",
    );
    expect(
      within(copyResponse).getByTitle(
        "Acción para copiar la respuesta de revisión semántica: Copiada.",
      ),
    ).toHaveTextContent("Copiada");

    const copyExchange = screen.getByRole("button", {
      name: "Copiar intercambio de revisión semántica",
    });
    expect(copyExchange).toHaveAttribute(
      "title",
      "Copiar la solicitud y la respuesta de revisión semántica al portapapeles.",
    );
    expect(
      within(copyExchange).getByTitle(
        "Acción para copiar el intercambio de revisión semántica: Copiar intercambio.",
      ),
    ).toHaveTextContent("Copiar intercambio");

    installClipboardMock();
    fireEvent.click(copyExchange);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Solicitud de revisión semántica:",
          expectedReviewPrompt,
          "",
          "Respuesta de revisión semántica:",
          "Found one high severity issue in src/App.tsx:12. Add a regression test before merging.",
        ].join("\n"),
      ),
    );
    expect(copyExchange).toHaveAttribute(
      "title",
      "Solicitud y respuesta de revisión semántica copiadas al portapapeles.",
    );
    expect(
      within(copyExchange).getByTitle(
        "Acción para copiar el intercambio de revisión semántica: Copiado.",
      ),
    ).toHaveTextContent("Copiado");

    fireEvent.click(responseReset);
    expect(
      within(review).queryByTitle(
        "El prompt de revisión semántica se envió como un turno de Agent.",
      ),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByTitle(
        "Respuesta de revisión semántica recibida en el turno 1; verifica los hallazgos antes de actuar.",
      ),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Reiniciar el flujo de revisión semántica" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", {
        name: "Mostrar el turno de la respuesta de revisión semántica",
      }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copiar respuesta de revisión semántica" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copiar prompt de revisión semántica" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", {
        name: "Mostrar el turno de la solicitud de revisión semántica",
      }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copiar intercambio de revisión semántica" }),
    ).not.toBeInTheDocument();
  });

  it("resets semantic review copied state when redrafting the review prompt", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "review",
      status: "completed",
      message: "Review summary for branch main: 1 changed file.",
      review_summary: {
        branch: "main",
        changed_files: 1,
        working_shortstat: "1 file changed, 1 insertion(+)",
        staged_shortstat: null,
        files: [" M src/App.tsx"],
        truncated_count: 0,
      },
      review_findings: [],
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    const review = await screen.findByLabelText("Resumen de la revisión");
    const reviewPromptButton = within(review).getByRole("button", {
      name: "Preparar prompt de revisión semántica",
    });
    await user.click(reviewPromptButton);

    const copyPrompt = within(review).getByRole("button", {
      name: "Copiar prompt de revisión semántica",
    });
    installClipboardMock();
    fireEvent.click(copyPrompt);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        expect.stringContaining("Revisa los cambios actuales de Git"),
      ),
    );
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Prompt de revisión semántica copiado al portapapeles.",
    );

    await user.click(reviewPromptButton);
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Copiar el borrador del prompt de revisión semántica al portapapeles.",
    );
    expect(
      within(copyPrompt).getByTitle(
        "Acción para copiar el prompt de revisión semántica: Copiar prompt.",
      ),
    ).toHaveTextContent("Copiar prompt");
  });

  it("resets review clipboard state when the structured review summary refreshes", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock
      .mockResolvedValueOnce({
        command: "review",
        status: "completed",
        message: "Review summary for branch main: 1 changed file.",
        review_summary: {
          branch: "main",
          changed_files: 1,
          working_shortstat: "1 file changed, 1 insertion(+)",
          staged_shortstat: null,
          files: [" M src/App.tsx"],
          truncated_count: 0,
        },
        review_findings: [],
      })
      .mockResolvedValueOnce({
        command: "review",
        status: "completed",
        message: "Review summary for branch feature: 1 changed file.",
        review_summary: {
          branch: "feature",
          changed_files: 1,
          working_shortstat: "1 file changed, 2 insertions(+)",
          staged_shortstat: null,
          files: [" M src/Feature.tsx"],
          truncated_count: 0,
        },
        review_findings: [],
      });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    const review = await screen.findByLabelText("Resumen de la revisión");
    const copySummary = within(review).getByRole("button", {
      name: "Copiar resumen estructurado de la revisión",
    });
    installClipboardMock();
    fireEvent.click(copySummary);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(expect.stringContaining("Rama: main")),
    );
    expect(copySummary).toHaveAttribute(
      "title",
      "Resumen estructurado de la revisión copiado al portapapeles.",
    );

    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    await waitFor(() => expect(within(review).getByText("M src/Feature.tsx")).toBeInTheDocument());
    expect(copySummary).toHaveAttribute(
      "title",
      "Copiar el resumen estructurado de la revisión al portapapeles.",
    );
    expect(
      within(copySummary).getByTitle(
        "Acción para copiar el resumen estructurado de la revisión: Copiar resumen.",
      ),
    ).toHaveTextContent("Copiar resumen");
  });

  it("sets a persistent session goal through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "goal",
      status: "completed",
      message: "Goal set: Build the host harness.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/goal Build the host harness");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith(
        "sess-1",
        "goal",
        "Build the host harness",
      ),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("sets a persistent session personality through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "personality",
      status: "completed",
      message: "Personality set: precise.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/personality precise");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "personality", "precise"),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("saves feedback through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "feedback",
      status: "completed",
      message: "Saved feedback: Keep the controls native.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/feedback Keep the controls native.");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith(
        "sess-1",
        "feedback",
        "Keep the controls native.",
      ),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("saves comments through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "comments",
      status: "completed",
      message: "Saved comment: The palette should explain unavailable actions.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/comments The palette should explain unavailable actions.");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith(
        "sess-1",
        "comments",
        "The palette should explain unavailable actions.",
      ),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("compacts session context through the host command backend", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "compact",
      status: "completed",
      message: "Context summary saved from 3 events across 1 turns.",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/compact");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "compact", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("opens a child terminal when a host command returns a forked session", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture(),
      sessionFixture({ id: "sess-child", repo: "/r/a", agent_type: "codex" }),
    ]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "fork",
      status: "completed",
      message: "Forked session sess-child from sess-1.",
      session_id: "sess-child",
      repo: "/r/a",
      agent_type: "codex",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/fork");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "fork", undefined),
    );
    expect(consoleDock.openTerminalParams()).toEqual(
      expect.arrayContaining([{ sessionId: "sess-child", repo: "/r/a", agentType: "codex" }]),
    );
    expect(await screen.findByText("Forked session sess-child from sess-1.")).toBeInTheDocument();
  });

  it("routes branch through the host fork backend and opens the child terminal", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture(),
      sessionFixture({ id: "sess-branch", repo: "/r/branch", agent_type: "codex" }),
    ]);
    runAgentHostCommandMock.mockResolvedValueOnce({
      command: "branch",
      status: "completed",
      message: "Forked worktree session sess-branch from sess-1.",
      session_id: "sess-branch",
      repo: "/r/branch",
      agent_type: "codex",
    });
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "/branch worktree");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "branch", "worktree"),
    );
    expect(consoleDock.openTerminalParams()).toEqual(
      expect.arrayContaining([{ sessionId: "sess-branch", repo: "/r/branch", agentType: "codex" }]),
    );
    expect(
      await screen.findByText("Forked worktree session sess-branch from sess-1."),
    ).toBeInTheDocument();
  });

  it("uses Enter to send and Shift+Enter to keep composing", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "line one{Shift>}{Enter}{/Shift}line two");
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();

    await user.type(composer, "{Enter}");
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "line one\nline two\r", {
      speed: "standard",
    });
  });

  it("renders streamed output as a readable transcript", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendOutput({
        session_id: "sess-1",
        chunk_base64: b64("\u001b[32mDone\u001b[0m\n\nChanged src/a.ts"),
        timestamp_ms: 1,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(await within(conversation).findByText("Done")).toBeInTheDocument();
    expect(within(conversation).getByText("Changed src/a.ts")).toBeInTheDocument();
  });

  it("renders native timeline items as conversational turns", async () => {
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Haz el cambio",
        timestamp_ms: 1000,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Voy con ello",
        timestamp_ms: 65000,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:3",
        kind: "command_output",
        text: "cargo test",
        timestamp_ms: 125000,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(await within(conversation).findByText("Tú")).toBeInTheDocument();
    expect(within(conversation).getByText("Turno 1")).toBeInTheDocument();
    expect(within(conversation).getByText("Haz el cambio")).toBeInTheDocument();
    expect(within(conversation).getByText("Agent")).toBeInTheDocument();
    expect(within(conversation).getByText("Voy con ello")).toBeInTheDocument();
    expect(within(conversation).getByText("Comando")).toBeInTheDocument();
    expect(within(conversation).getByText("cargo test")).toBeInTheDocument();
    expect(within(conversation).getByText("3 mensajes / 1 comando")).toBeInTheDocument();

    const overview = screen.getByLabelText("Resumen de la sesión de Agent");
    const turnsMetric = within(overview).getByLabelText("Turnos: 1");
    expect(turnsMetric).toHaveTextContent("1");
    expect(turnsMetric).toHaveTextContent("Turnos");
    const messagesMetric = within(overview).getByLabelText("Mensajes: 2");
    expect(messagesMetric).toHaveTextContent("2");
    expect(messagesMetric).toHaveTextContent("Mensajes");
    const commandsMetric = within(overview).getByLabelText("Comandos: 1");
    expect(commandsMetric).toHaveTextContent("1");
    expect(commandsMetric).toHaveTextContent("Comandos");
    const filesMetric = within(overview).getByLabelText("Archivos: 0");
    expect(filesMetric).toHaveTextContent("0");
    expect(filesMetric).toHaveTextContent("Archivos");
    expect(within(overview).getByText("Actividad reciente")).toBeInTheDocument();
    expect(within(overview).getByText("cargo test")).toBeInTheDocument();
    expect(within(overview).getByText("+0s")).toBeInTheDocument();
    const turnMap = screen.getByLabelText("Mapa de turnos");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });
    expect(firstTurnButton).toHaveTextContent("T1");
    expect(firstTurnButton).toHaveTextContent("1 cmd");
    expect(firstTurnButton).toHaveTextContent("cargo test");

    await user.click(firstTurnButton);
    expect(scrollIntoViewMock).toHaveBeenCalledWith({ block: "start", behavior: "smooth" });
  });

  it("copies individual message blocks to the clipboard", async () => {
    const user = userEvent.setup();
    installClipboardMock();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Haz el cambio",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Voy con ello",
        timestamp_ms: 2,
      });
    });

    const copyAgent = await screen.findByLabelText("Copiar mensaje de Agent");
    expect(copyAgent).toHaveAttribute("title", "Copiar el mensaje de Agent del turno 1.");
    expect(screen.getAllByTitle("Acción para copiar el mensaje: Copiar.")).toHaveLength(2);
    await user.click(copyAgent);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith("Voy con ello"));
    expect(await screen.findByLabelText("Copiar mensaje de Agent")).toHaveTextContent("Copiado");
    expect(screen.getByLabelText("Copiar mensaje de Agent")).toHaveAttribute(
      "title",
      "Mensaje de Agent del turno 1 copiado al portapapeles.",
    );
    expect(screen.getByTitle("Acción para copiar el mensaje: Copiado.")).toHaveTextContent(
      "Copiado",
    );
  });

  it("copies a complete turn with messages, commands, and files", async () => {
    const user = userEvent.setup();
    installClipboardMock();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1000,
            ended_at_ms: 2000,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2000 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Haz el cambio",
        timestamp_ms: 1000,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Voy con ello",
        timestamp_ms: 1500,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:3",
        kind: "command_output",
        text: "npm test",
        timestamp_ms: 2000,
      });
    });

    const copyTurn = await screen.findByRole("button", { name: "Copiar turno" });
    expect(copyTurn).toHaveAttribute(
      "title",
      "Copiar la transcripción completa del turno 1, incluidos mensajes, comandos y archivos modificados.",
    );
    expect(
      screen.getByTitle("Acción para copiar el turno de la conversación: Copiar turno."),
    ).toHaveTextContent("Copiar turno");
    await user.click(copyTurn);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Turno 1 (+0s)");
    expect(copied).toContain("Tú:\nHaz el cambio");
    expect(copied).toContain("Agent:\nVoy con ello");
    expect(copied).toContain("Comando:\nnpm test");
    expect(copied).toContain("- modificado src/a.ts");
    expect(screen.getByRole("button", { name: "Copiado" })).toHaveAttribute(
      "title",
      "Transcripción completa del turno 1 copiada al portapapeles.",
    );
    expect(
      screen.getByTitle("Acción para copiar el turno de la conversación: Copiado."),
    ).toHaveTextContent("Copiado");
  });

  it("renders conversational messages as markdown while commands stay technical", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Explain this",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Done:\n\n- changed `src/a.ts`\n- ran tests",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:3",
        kind: "command_output",
        text: "npm test -- --run",
        timestamp_ms: 3,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(await within(conversation).findByRole("list")).toBeInTheDocument();
    expect(within(conversation).getByText("src/a.ts")).toBeInTheDocument();
    expect(within(conversation).getByText("npm test -- --run").tagName).toBe("PRE");
  });

  it("collapses long command output behind a technical summary", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    const longOutput = [
      "npm test -- --run",
      "suite a passed",
      "suite b passed",
      "suite c passed",
      "suite d passed",
      "suite e passed",
      "suite f passed",
      "suite g passed",
      "suite h passed",
      "suite i passed",
    ].join("\n");

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:1",
        kind: "command_output",
        text: longOutput,
        timestamp_ms: 1,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    const toggle = await within(conversation).findByText("Mostrar salida");
    expect(toggle.closest("summary")).toHaveAttribute(
      "title",
      "Fila de resumen de la salida contraída del comando del turno 1: actívala para mostrar u ocultar la salida completa.",
    );
    expect(
      within(conversation).getByTitle(
        "Resumen de la salida contraída del comando: npm test -- --run.",
      ),
    ).toHaveTextContent("npm test -- --run");
    const collapsedCommandBlock = toggle.closest("details");
    expect(collapsedCommandBlock).toHaveAttribute(
      "title",
      "Contenedor de la salida contraída del comando del turno 1: control de resumen y texto completo de la salida.",
    );
    expect(collapsedCommandBlock).not.toHaveAttribute("open");
    expect(within(conversation).getAllByText("npm test -- --run").length).toBeGreaterThan(0);
    const collapsedOutput = within(conversation).getByTitle(
      "Texto de salida de comandos del turno 1.",
    );
    expect(collapsedOutput).toHaveTextContent("npm test -- --run");
    expect(collapsedOutput).toHaveTextContent("suite i passed");
  });

  it("searches transcript turns across messages, commands, and touched files", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-2",
            index: 2,
            started_at_ms: 3,
            ended_at_ms: 4,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/search-panel.tsx", kind: "modified", timestamp_ms: 4 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Primera tarea",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Termine el dashboard",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:3",
        kind: "user_message",
        text: "Segunda tarea",
        timestamp_ms: 3,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:4",
        kind: "command_output",
        text: "npm test\nsearch-panel",
        timestamp_ms: 4,
      });
    });

    const search = await screen.findByLabelText("Buscar en la transcripción");
    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(search).toHaveAttribute("aria-describedby", "agent-transcript-search-hint");
    expect(search).toHaveAttribute(
      "title",
      "Busca mensajes, comandos o archivos. Pulsa Escape para borrar la búsqueda.",
    );
    expect(
      screen.getByText(
        "Pulsa Intro para recorrer los turnos coincidentes y Escape para borrar la búsqueda.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Resultado anterior" })).toHaveAttribute(
      "title",
      "Busca en la transcripción para navegar al resultado anterior.",
    );
    expect(
      screen.getByTitle(
        "Etiqueta de navegación de la búsqueda en la transcripción: resultado anterior.",
      ),
    ).toHaveTextContent("Anterior");
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).toHaveAttribute(
      "title",
      "Busca en la transcripción para navegar al resultado siguiente.",
    );
    expect(
      screen.getByTitle(
        "Etiqueta de navegación de la búsqueda en la transcripción: resultado siguiente.",
      ),
    ).toHaveTextContent("Siguiente");

    await user.type(search, "dashboard");
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).queryByText("Segunda tarea")).not.toBeInTheDocument();
    expect(screen.getByText("1 de 2 turnos")).toBeInTheDocument();
    expect(screen.getByLabelText("1 turno coincidentes de 2 en total.")).toHaveAttribute(
      "title",
      "Cantidad de resultados de la búsqueda en la transcripción: 1 turno coincidentes entre 2 en total.",
    );
    expect(screen.getByRole("button", { name: "Resultado anterior" })).toHaveAttribute(
      "title",
      "Se necesitan al menos dos turnos coincidentes para ir al resultado anterior.",
    );
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).toHaveAttribute(
      "title",
      "Se necesitan al menos dos turnos coincidentes para ir al resultado siguiente.",
    );
    const messageMatch = within(conversation).getByLabelText(
      "Coincidencias de búsqueda del turno 1",
    );
    expect(messageMatch).toHaveAttribute(
      "title",
      "Coincidencias de búsqueda del turno 1: explica por qué este turno coincide con la búsqueda.",
    );
    expect(within(messageMatch).getByText("Coincidencia en mensaje")).toBeInTheDocument();
    expect(within(messageMatch).queryByText("Coincidencia en comando")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "npm test search-panel");
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(within(conversation).queryByText("Termine el dashboard")).not.toBeInTheDocument();
    expect(screen.getByText("1 de 2 turnos")).toBeInTheDocument();
    const commandMatch = within(conversation).getByLabelText(
      "Coincidencias de búsqueda del turno 2",
    );
    expect(within(commandMatch).getByText("Coincidencia en comando")).toBeInTheDocument();
    expect(within(commandMatch).queryByText("Coincidencia en archivo")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "src/search-panel.tsx");
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(within(conversation).queryByText("Termine el dashboard")).not.toBeInTheDocument();
    const fileMatch = within(conversation).getByLabelText("Coincidencias de búsqueda del turno 2");
    expect(within(fileMatch).getByText("Coincidencia en archivo")).toBeInTheDocument();
    expect(within(fileMatch).queryByText("Coincidencia en mensaje")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "nope");
    expect(screen.getByText("Sin coincidencias")).toBeInTheDocument();
    expect(
      screen.getByTitle(
        "Estado vacío de la conversación con Agent: ningún turno coincide con la búsqueda actual.",
      ),
    ).toHaveClass("agent-panel__empty-chat");
    expect(
      screen.getByTitle(
        "Etiqueta del estado vacío de la conversación con Agent: Sin coincidencias.",
      ),
    ).toHaveTextContent("Sin coincidencias");
    expect(
      screen.queryByText("Prueba otra búsqueda entre mensajes, comandos y archivos modificados."),
    ).not.toBeInTheDocument();
    const clearSearch = screen.getByRole("button", { name: "Borrar búsqueda" });
    expect(clearSearch).toHaveAttribute(
      "title",
      "Acción del estado vacío de la conversación con Agent: borrar la búsqueda, recuperar todos los turnos y devolver el foco al buscador.",
    );
    expect(screen.getByTitle("Borrar la búsqueda de la transcripción.")).toHaveTextContent(
      "Borrar búsqueda",
    );
    await user.click(clearSearch);
    expect(search).toHaveValue("");
    expect(search).toHaveFocus();
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(screen.queryByText("Sin coincidencias")).not.toBeInTheDocument();

    await user.type(search, "dashboard");
    expect(within(conversation).queryByText("Segunda tarea")).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(search).toHaveValue("");
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
  }, 10000);

  it("jumps to the latest visible turn from the chat toolbar", async () => {
    const user = userEvent.setup();
    const getElementByIdSpy = vi.spyOn(document, "getElementById");
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Primer turno",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:2",
        kind: "user_message",
        text: "Segundo turno",
        timestamp_ms: 2,
      });
    });

    const latestButton = await screen.findByRole("button", { name: "Último" });
    expect(latestButton).toHaveAttribute(
      "title",
      "Ir al último de los 2 turnos de la transcripción.",
    );
    expect(screen.getByTitle("Acción secundaria de la transcripción: Último.")).toHaveTextContent(
      "Último",
    );

    await user.click(latestButton);

    expect(getElementByIdSpy).toHaveBeenCalledWith("agent-turn-sess-1-2");
    expect(scrollIntoViewMock).toHaveBeenCalledWith({ block: "end", behavior: "smooth" });
    getElementByIdSpy.mockRestore();
  });

  it("navigates previous and next transcript search results", async () => {
    const user = userEvent.setup();
    const getElementByIdSpy = vi.spyOn(document, "getElementById");
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Dashboard alpha",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:2",
        kind: "user_message",
        text: "Unrelated turn",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:3",
        kind: "user_message",
        text: "Dashboard beta",
        timestamp_ms: 3,
      });
    });

    const search = await screen.findByLabelText("Buscar en la transcripción");
    await user.type(search, "dashboard");
    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(screen.getByText("2 de 3 turnos")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Resultado anterior" })).toHaveAttribute(
      "title",
      "Ir al resultado anterior",
    );
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).toHaveAttribute(
      "title",
      "Ir al resultado siguiente",
    );
    expect(
      screen.getByLabelText("No hay ningún resultado seleccionado entre los 2 coincidentes."),
    ).toHaveTextContent("- / 2");

    await user.keyboard("{Enter}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(scrollIntoViewMock).toHaveBeenLastCalledWith({ block: "center", behavior: "smooth" });
    expect(screen.getByLabelText("Resultado de búsqueda seleccionado: 1 de 2.")).toHaveTextContent(
      "1 / 2",
    );
    expect(within(conversation).getByText("Dashboard alpha").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.keyboard("{Enter}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-3");
    expect(screen.getByLabelText("Resultado de búsqueda seleccionado: 2 de 2.")).toHaveTextContent(
      "2 / 2",
    );
    expect(within(conversation).getByText("Dashboard beta").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(screen.getByLabelText("Resultado de búsqueda seleccionado: 1 de 2.")).toHaveTextContent(
      "1 / 2",
    );
    expect(within(conversation).getByText("Dashboard alpha").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Resultado siguiente" }));
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-3");
    expect(within(conversation).getByText("Dashboard beta").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Resultado anterior" }));
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(within(conversation).getByText("Dashboard alpha").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    getElementByIdSpy.mockRestore();
  });

  it("clears focused search-result selection when transcript search resets", async () => {
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Dashboard alpha",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:2",
        kind: "user_message",
        text: "Unrelated turn",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:3",
        kind: "user_message",
        text: "Dashboard beta",
        timestamp_ms: 3,
      });
    });

    const search = await screen.findByLabelText("Buscar en la transcripción");
    await user.type(search, "dashboard");
    const conversation = screen.getByLabelText("Conversación con Agent");

    await user.keyboard("{Enter}");
    const alphaTurn = within(conversation).getByText("Dashboard alpha").closest("article");
    expect(alphaTurn).toHaveAttribute("aria-current", "true");
    expect(screen.getByLabelText("Resultado de búsqueda seleccionado: 1 de 2.")).toHaveTextContent(
      "1 / 2",
    );

    await user.keyboard("{Escape}");

    expect(search).toHaveValue("");
    expect(screen.queryByText("1 / 2")).not.toBeInTheDocument();
    expect(alphaTurn).not.toHaveAttribute("aria-current", "true");
    expect(within(conversation).getByText("Unrelated turn")).toBeInTheDocument();
  });

  it("groups lower-priority transcript actions for responsive toolbar collapse", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Primer turno",
        timestamp_ms: 1,
      });
    });

    const secondaryActions = await screen.findByRole("group", {
      name: "Acciones de la transcripción",
    });
    const resultNavigation = screen.getByRole("group", {
      name: "Navegación por los resultados de la transcripción",
    });
    expect(within(resultNavigation).getByText("Todos los turnos")).toBeInTheDocument();
    expect(
      within(resultNavigation).getByRole("button", { name: "Resultado anterior" }),
    ).toBeDisabled();
    expect(
      within(resultNavigation).getByRole("button", { name: "Resultado siguiente" }),
    ).toBeDisabled();
    expect(within(secondaryActions).getByRole("button", { name: "Último" })).toHaveAttribute(
      "title",
      "Ir al último de los 1 turno de la transcripción.",
    );
    expect(
      within(secondaryActions).getByTitle("Acción secundaria de la transcripción: Último."),
    ).toHaveTextContent("Último");
    expect(
      within(secondaryActions).getByRole("button", { name: "Copiar lo visible" }),
    ).toHaveAttribute("title", "Copiar los 1 turno de la transcripción.");
    expect(
      within(secondaryActions).getByTitle(
        "Acción secundaria de la transcripción: Copiar lo visible.",
      ),
    ).toHaveTextContent("Copiar lo visible");
    expect(
      within(secondaryActions).queryByRole("button", { name: "Resultado anterior" }),
    ).not.toBeInTheDocument();
    expect(
      within(secondaryActions).queryByRole("button", { name: "Resultado siguiente" }),
    ).not.toBeInTheDocument();
  });

  it("copies only the visible filtered transcript", async () => {
    const user = userEvent.setup();
    installClipboardMock();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Dashboard task",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Finished dashboard work",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:3",
        kind: "user_message",
        text: "Search task",
        timestamp_ms: 3,
      });
    });

    await user.type(await screen.findByLabelText("Buscar en la transcripción"), "dashboard");
    expect(screen.getByRole("button", { name: "Último" })).toHaveAttribute(
      "title",
      "Ir al último turno filtrado de 2 turnos en total.",
    );
    const copyVisible = screen.getByRole("button", { name: "Copiar lo visible" });
    expect(copyVisible).toHaveAttribute("title", "Copiar 1 turno filtrados de 2 en total.");
    await user.click(copyVisible);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Dashboard task");
    expect(copied).toContain("Finished dashboard work");
    expect(copied).not.toContain("Search task");
    expect(screen.getByRole("button", { name: "Copiado" })).toBeInTheDocument();
    expect(screen.getByTitle("Acción secundaria de la transcripción: Copiado.")).toHaveTextContent(
      "Copiado",
    );
  });

  it("loads archived transcripts without enabling live input or stop cleanup", async () => {
    const archived = sessionFixture({
      status: "completed",
      pid: null,
      checkpoint: null,
      timeline: [
        {
          session_id: "sess-1",
          id: "evt-1",
          kind: "agent_message",
          text: "Archived answer",
          timestamp_ms: 2,
        },
      ],
    });
    getAgentJournalSessionMock.mockResolvedValueOnce(archived);
    const { unmount } = render(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
    );

    expect(
      await within(screen.getByLabelText("Conversación con Agent")).findByText("Archived answer"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("Actividad de Agent")).getByText("Transcripción archivada"),
    ).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Transcripción archivada")).toBeDisabled();
    expect(screen.getByPlaceholderText("Transcripción archivada")).toHaveAttribute(
      "title",
      "Entrada de mensajes de Codex para a: la transcripción archivada es de solo lectura.",
    );
    const archivedComposer = screen.getByPlaceholderText("Transcripción archivada");
    const archivedComposerHint = archivedComposer.getAttribute("aria-describedby");
    expect(archivedComposerHint).toBeTruthy();
    expect(document.getElementById(archivedComposerHint!)).toHaveTextContent(
      "La transcripción está archivada y es de solo lectura.",
    );
    expect(
      screen.queryByRole("listbox", { name: "Comandos del compositor" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plan" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enviar" })).toHaveAttribute(
      "title",
      "Enviar mensaje a Codex para a: la transcripción archivada es de solo lectura.",
    );
    const archivedFocus = screen.getByLabelText("Turno seleccionado");
    expect(
      within(archivedFocus).getByTitle(
        "Contenedor de restauración del turno seleccionado 1: detén la sesión antes de restaurar.",
      ),
    ).toHaveClass("agent-panel__turn-focus-actions");
    expect(within(archivedFocus).getByRole("button", { name: "Restaurar aquí" })).toHaveAttribute(
      "title",
      "Restaurar el turno 1: detén la sesión antes.",
    );

    unmount();
    expect(listAgentSessionsMock).not.toHaveBeenCalled();
    expect(stopAgentSessionMock).not.toHaveBeenCalled();
  });

  it("labels an archived empty transcript state", async () => {
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({
        status: "completed",
        pid: null,
        checkpoint: null,
        timeline: [],
        turn_checkpoints: [],
      }),
    );

    render(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
    );

    expect(
      await screen.findByTitle(
        "Estado vacío de la conversación con Agent: la transcripción archivada no contiene turnos guardados.",
      ),
    ).toHaveClass("agent-panel__empty-chat");
    expect(
      screen.getByTitle("Etiqueta del estado vacío de la conversación con Agent: Transcripción."),
    ).toHaveTextContent("Transcripción");
    expect(
      screen.getByTitle("Estado de la transcripción archivada: no se guardaron turnos."),
    ).toHaveTextContent("No se guardaron eventos de Timeline para esta sesión.");
  });

  it("combines timeline turns with checkpoint file changes", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Edita src/a.ts",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "He modificado el archivo.",
        timestamp_ms: 2,
      });
    });

    expect(await screen.findByText("Edita src/a.ts")).toBeInTheDocument();
    expect(
      screen.getByTitle("Archivo modificado en el turno 1: modificado src/a.ts."),
    ).toHaveTextContent("modificado src/a.ts");
    expect(screen.getByText("1 archivo modificado")).toHaveAttribute(
      "title",
      "El turno 1 modificó 1 archivo.",
    );
    expect(screen.getByLabelText("Archivos: 1")).toBeInTheDocument();
    expect(screen.getByTitle("Turno 1: 0 comandos, 1 archivo")).toBeInTheDocument();
  });

  it("summarizes touched artifacts for each turn", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [
              { path: "src/agent-view.tsx", kind: "modified", timestamp_ms: 2 },
              { path: "src/agent-view.test.tsx", kind: "modified", timestamp_ms: 2 },
              { path: "docs/agent-view.md", kind: "modified", timestamp_ms: 2 },
              { path: "package.json", kind: "modified", timestamp_ms: 2 },
            ],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Review artifacts",
        timestamp_ms: 1,
      });
    });

    const turnSummary = await screen.findByLabelText("Resumen de artefactos del turno 1");
    expect(within(turnSummary).getByText("Código 1")).toBeInTheDocument();
    expect(within(turnSummary).getByText("Pruebas 1")).toBeInTheDocument();
    expect(within(turnSummary).getByText("Documentación 1")).toBeInTheDocument();
    expect(within(turnSummary).getByText("Configuración 1")).toBeInTheDocument();

    const focusedSummary = screen.getByLabelText("Resumen de artefactos del turno seleccionado");
    expect(within(focusedSummary).getByText("Código 1")).toBeInTheDocument();
    expect(within(focusedSummary).getByText("Pruebas 1")).toBeInTheDocument();
  });

  it("uses Agent Lens as a files, commands, and timeline inspector", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Edita src/a.ts",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Voy con ello",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:3",
        kind: "command_output",
        text: "npm test",
        timestamp_ms: 3,
      });
    });

    const lens = await screen.findByLabelText("Agent Lens");
    expect(lens.parentElement).toHaveClass("agent-panel__lens-pane");
    expect(lens.closest(".agent-panel__side-rail")).toHaveAttribute(
      "aria-label",
      "Panel de inspección de Agent",
    );
    const filesPanel = within(lens).getByRole("tabpanel", { name: /Archivos/ });
    expect(filesPanel).toHaveAttribute("id", "agent-lens-sess-1-files-panel");
    expect(filesPanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-files-tab");
    const filesTab = within(lens).getByRole("tab", { name: /Archivos/ });
    expect(filesTab).toHaveAttribute("id", "agent-lens-sess-1-files-tab");
    expect(filesTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-files-panel");
    expect(filesTab).toHaveAttribute("tabindex", "0");
    const commandsTab = within(lens).getByRole("tab", { name: /Comandos/ });
    expect(commandsTab).toHaveAttribute("id", "agent-lens-sess-1-commands-tab");
    expect(commandsTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-commands-panel");
    expect(commandsTab).toHaveAttribute("tabindex", "-1");
    const timelineTab = within(lens).getByRole("tab", { name: /Timeline/ });
    expect(timelineTab).toHaveAttribute("id", "agent-lens-sess-1-timeline-tab");
    expect(timelineTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-timeline-panel");
    expect(timelineTab).toHaveAttribute("tabindex", "-1");
    const preview = within(lens).getByLabelText("Vista previa del archivo seleccionado");
    expect(preview).toHaveTextContent("src/a.ts");
    expect(preview).toHaveTextContent("No hay datos en vivo de los fragmentos de este archivo.");
    expect(within(lens).getAllByText("src/a.ts").length).toBeGreaterThan(0);

    await user.click(commandsTab);
    const commandsPanel = within(lens).getByRole("tabpanel", { name: /Comandos/ });
    expect(commandsPanel).toHaveAttribute("id", "agent-lens-sess-1-commands-panel");
    expect(commandsPanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-commands-tab");
    expect(within(commandsPanel).getByLabelText("Salida de comandos")).toHaveTextContent(
      "npm test",
    );
    const commandFilter = within(commandsPanel).getByLabelText("Filtrar la salida de comandos");

    await user.type(commandFilter, "missing");
    expect(
      within(commandsPanel).getByText("Ningún comando coincide con este filtro."),
    ).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(within(commandsPanel).getByLabelText("Salida de comandos")).toHaveTextContent(
      "npm test",
    );

    await user.click(timelineTab);
    const timelinePanel = within(lens).getByRole("tabpanel", { name: /Timeline/ });
    expect(timelinePanel).toHaveAttribute("id", "agent-lens-sess-1-timeline-panel");
    expect(timelinePanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-timeline-tab");
    expect(within(timelinePanel).getByLabelText("Timeline reciente")).toHaveTextContent("npm test");
    expect(within(timelinePanel).getByLabelText("Timeline reciente")).toHaveTextContent(
      "Voy con ello",
    );
    const timelineFilter = within(timelinePanel).getByLabelText("Filtrar eventos de Timeline");

    await user.type(timelineFilter, "Agent");
    expect(within(timelinePanel).queryByText("npm test")).not.toBeInTheDocument();
    expect(within(timelinePanel).getByText("Voy con ello")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(within(timelinePanel).getByLabelText("Timeline reciente")).toHaveTextContent("npm test");
  });

  it("navigates Agent Lens tabs with keyboard shortcuts", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Edita src/a.ts",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:2",
        kind: "command_output",
        text: "npm test",
        timestamp_ms: 2,
      });
    });

    const lens = await screen.findByLabelText("Agent Lens");
    const filesTab = within(lens).getByRole("tab", { name: /Archivos/ });
    const commandsTab = within(lens).getByRole("tab", { name: /Comandos/ });
    const timelineTab = within(lens).getByRole("tab", { name: /Timeline/ });

    expect(filesTab).toHaveAttribute("aria-selected", "true");
    filesTab.focus();

    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(commandsTab).toHaveFocus());
    expect(commandsTab).toHaveAttribute("aria-selected", "true");
    expect(commandsTab).toHaveAttribute("tabindex", "0");
    expect(within(lens).getByRole("tabpanel", { name: /Comandos/ })).toHaveAttribute(
      "id",
      "agent-lens-sess-1-commands-panel",
    );

    await user.keyboard("{End}");
    await waitFor(() => expect(timelineTab).toHaveFocus());
    expect(timelineTab).toHaveAttribute("aria-selected", "true");
    expect(timelineTab).toHaveAttribute("tabindex", "0");
    expect(within(lens).getByRole("tabpanel", { name: /Timeline/ })).toHaveAttribute(
      "id",
      "agent-lens-sess-1-timeline-panel",
    );

    await user.keyboard("{Home}");
    await waitFor(() => expect(filesTab).toHaveFocus());
    expect(filesTab).toHaveAttribute("aria-selected", "true");
    expect(filesTab).toHaveAttribute("tabindex", "0");
    expect(within(lens).getByRole("tabpanel", { name: /Archivos/ })).toHaveAttribute(
      "id",
      "agent-lens-sess-1-files-panel",
    );
  });

  it("filters Agent Lens files with live status, Escape clear, and no-results recovery", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [
              { path: "src/a.ts", kind: "modified", timestamp_ms: 2 },
              { path: "docs/guide.md", kind: "created", timestamp_ms: 2 },
            ],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Touch files",
        timestamp_ms: 1,
      });
    });

    const lens = await screen.findByLabelText("Agent Lens");
    const filter = within(lens).getByLabelText("Filtrar archivos modificados");
    expect(filter).toHaveAttribute("aria-describedby", "agent-lens-sess-1-file-filter-status");
    expect(filter).toHaveAttribute(
      "title",
      "Filtrar 2 archivos modificados de Agent Lens por ruta, tipo de cambio, estado o categoría.",
    );
    expect(within(lens).getByText("2 archivos")).toBeInTheDocument();

    await user.type(filter, "docs");

    expect(filter).toHaveAttribute(
      "title",
      "Filtrar 2 archivos modificados de Agent Lens por ruta, tipo de cambio, estado o categoría. Pulsa Escape para borrar el filtro.",
    );
    expect(within(lens).getByText("1 de 2 archivos")).toBeInTheDocument();
    expect(within(lens).getAllByText("docs/guide.md").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/a.ts")).not.toBeInTheDocument();
    expect(within(lens).getByRole("button", { name: "Borrar" })).toBeInTheDocument();

    await user.clear(filter);
    await user.type(filter, "missing");

    expect(within(lens).getByText("Ningún archivo coincide con este filtro.")).toBeInTheDocument();
    const emptyClear = within(lens).getAllByRole("button", { name: "Borrar" })[1]!;

    await user.click(emptyClear);

    await waitFor(() => expect(filter).toHaveFocus());
    expect(filter).toHaveValue("");
    expect(within(lens).getAllByText("src/a.ts").length).toBeGreaterThan(0);
    expect(within(lens).getAllByText("docs/guide.md").length).toBeGreaterThan(0);

    await user.type(filter, "src");
    await user.keyboard("{Escape}");

    await waitFor(() => expect(filter).toHaveFocus());
    expect(filter).toHaveValue("");
    expect(within(lens).getAllByText("src/a.ts").length).toBeGreaterThan(0);
    expect(within(lens).getAllByText("docs/guide.md").length).toBeGreaterThan(0);
  });

  it("explains empty Agent Lens command and timeline states", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");

    await user.click(within(lens).getByRole("tab", { name: /Comandos/ }));
    expect(
      within(lens).getByTitle("Agent Lens no tiene salidas de comandos en la sesión actual."),
    ).toHaveTextContent("Aún no se han capturado comandos.");

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
    expect(
      within(lens).getByTitle("Agent Lens no tiene eventos de Timeline en la sesión actual."),
    ).toHaveTextContent("Aún no se han capturado eventos de Timeline.");
  });

  it("scopes Agent Lens files and commands to the focused turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/first.ts", kind: "modified", timestamp_ms: 2 }],
          },
          {
            id: "sess-1:turn-2",
            index: 2,
            started_at_ms: 3,
            ended_at_ms: 4,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/second.ts", kind: "created", timestamp_ms: 4 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "First task",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:2",
        kind: "command_output",
        text: "npm test first",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:3",
        kind: "user_message",
        text: "Second task",
        timestamp_ms: 3,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:4",
        kind: "command_output",
        text: "npm test second",
        timestamp_ms: 4,
      });
    });

    const lens = await screen.findByLabelText("Agent Lens");
    expect(within(lens).getByRole("button", { name: "Turno" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(lens).getByRole("button", { name: "Sesión" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(within(lens).getByText("Turno 2")).toBeInTheDocument();
    expect(within(lens).getByText("Trabajando")).toBeInTheDocument();
    expect(within(lens).getAllByText("src/second.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/first.ts")).not.toBeInTheDocument();

    await user.click(within(lens).getByRole("tab", { name: /Comandos/ }));
    expect(within(lens).getByText("npm test second")).toBeInTheDocument();
    expect(within(lens).queryByText("npm test first")).not.toBeInTheDocument();

    const turnMap = screen.getByLabelText("Mapa de turnos");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });

    await user.click(firstTurnButton);
    expect(within(lens).getByText("Turno 1")).toBeInTheDocument();
    expect(within(lens).getByText("npm test first")).toBeInTheDocument();
    expect(within(lens).queryByText("npm test second")).not.toBeInTheDocument();

    await user.click(within(lens).getByRole("button", { name: "Sesión" }));
    await user.click(within(lens).getByRole("tab", { name: /Archivos/ }));
    expect(within(lens).getByRole("button", { name: "Sesión" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(lens).getAllByText("src/first.ts").length).toBeGreaterThan(0);
    expect(within(lens).getAllByText("src/second.ts").length).toBeGreaterThan(0);
  });

  it("opens focused files and drafts file-specific follow-up prompts", async () => {
    const user = userEvent.setup();
    const openFile = vi.fn();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/agent-view.tsx", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);
    act(() => {
      busStore.applyDelta(
        repoDelta({
          status: { modified: ["src/agent-view.tsx"], staged: [], untracked: [] },
          subscribed_diffs: [
            fileDiff("src/agent-view.tsx", [
              { kind: "Added", content: "new", old_lineno: null, new_lineno: 2 },
              { kind: "Removed", content: "old", old_lineno: 2, new_lineno: null },
            ]),
          ],
        }),
      );
    });
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openFile },
    );

    const lens = await screen.findByLabelText("Agent Lens");
    const preview = within(lens).getByLabelText("Vista previa del archivo seleccionado");
    const previewActions = within(preview).getByLabelText(
      "Acciones de vista previa para src/agent-view.tsx",
    );
    expect(
      within(previewActions).getByRole("button", { name: "Abrir el archivo seleccionado" }),
    ).toHaveAttribute(
      "title",
      "Abrir src/agent-view.tsx desde Agent Lens en el espacio de trabajo.",
    );
    expect(
      within(previewActions).getByRole("button", { name: "Preguntar por el archivo seleccionado" }),
    ).toHaveAttribute(
      "title",
      "Preparar un prompt de seguimiento de Agent Lens para src/agent-view.tsx.",
    );
    expect(
      within(previewActions).getByRole("button", { name: "Revertir el archivo seleccionado" }),
    ).toBeDisabled();
    const fileActions = within(lens).getByTitle(
      "Acciones de Agent Lens para src/agent-view.tsx: vista previa, abrir, preguntar y revertir.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "Acciones para src/agent-view.tsx");
    expect(within(fileActions).getByRole("button", { name: "Vista previa" })).toHaveAttribute(
      "title",
      "Se muestran los detalles de src/agent-view.tsx en Agent Lens.",
    );
    expect(within(fileActions).getByRole("button", { name: "Abrir" })).toHaveAttribute(
      "title",
      "Abrir src/agent-view.tsx desde Agent Lens en el espacio de trabajo.",
    );
    expect(within(fileActions).getByRole("button", { name: "Preguntar" })).toHaveAttribute(
      "title",
      "Preparar un prompt de seguimiento de Agent Lens para src/agent-view.tsx.",
    );
    expect(within(fileActions).getByRole("button", { name: "Revertir" })).toBeDisabled();
    await user.click(within(lens).getByRole("button", { name: "Abrir" }));

    expect(openFile).toHaveBeenCalledWith("/r/a", "src/agent-view.tsx", true);

    await user.click(
      within(previewActions).getByRole("button", { name: "Abrir el archivo seleccionado" }),
    );

    expect(openFile).toHaveBeenCalledTimes(2);
    expect(openFile).toHaveBeenLastCalledWith("/r/a", "src/agent-view.tsx", true);

    await user.click(within(lens).getByRole("button", { name: "Preguntar" }));

    let composerValue = (screen.getByLabelText("Mensaje para Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Céntrate en src/agent-view.tsx.");
    expect(composerValue).toContain("Este archivo figura como modificado en turno 1.");
    expect(composerValue).toContain("Categoría del artefacto: Código.");
    expect(composerValue).toContain("Resumen del diff: 1 fragmento - +1 / -1.");
    expect(composerValue).toContain("siguiente cambio o paso de verificación concreto");

    await user.clear(screen.getByLabelText("Mensaje para Codex"));
    await user.click(
      within(previewActions).getByRole("button", { name: "Preguntar por el archivo seleccionado" }),
    );

    composerValue = (screen.getByLabelText("Mensaje para Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Céntrate en src/agent-view.tsx.");
    expect(composerValue).toContain("Resumen del diff: 1 fragmento - +1 / -1.");
  });

  it("shows live repo status and diff context for Agent Lens files", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/agent-view.tsx", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);
    act(() => {
      busStore.applyDelta(
        repoDelta({
          status: {
            modified: ["src/agent-view.tsx"],
            staged: ["src/agent-view.tsx"],
            untracked: [],
          },
          subscribed_diffs: [
            fileDiff("src/agent-view.tsx", [
              { kind: "Context", content: "ctx", old_lineno: 1, new_lineno: 1 },
              { kind: "Added", content: "new a", old_lineno: null, new_lineno: 2 },
              { kind: "Added", content: "new b", old_lineno: null, new_lineno: 3 },
              { kind: "Removed", content: "old", old_lineno: 2, new_lineno: null },
            ]),
          ],
        }),
      );
    });

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    const context = within(lens).getByLabelText("Contexto en vivo de src/agent-view.tsx");
    expect(
      within(context).getByTitle(
        "Estado en vivo del repositorio para src/agent-view.tsx: modificado.",
      ),
    ).toHaveTextContent("modificado");
    expect(
      within(context).getByTitle(
        "Estado en vivo del repositorio para src/agent-view.tsx: preparado.",
      ),
    ).toHaveTextContent("preparado");
    expect(
      within(context).getByTitle(
        "Resumen del diff en vivo de src/agent-view.tsx: 2 añadidas y 1 eliminadas.",
      ),
    ).toHaveTextContent("+2 / -1");

    const preview = within(lens).getByLabelText("Vista previa del archivo seleccionado");
    expect(preview).toHaveTextContent("Vista previa");
    expect(
      within(preview).getByTitle("La vista previa de Agent Lens muestra src/agent-view.tsx."),
    ).toHaveTextContent("src/agent-view.tsx");
    expect(within(preview).getByText("src/agent-view.tsx")).toBeInTheDocument();
    const previewDetails = within(preview).getByLabelText(
      "Detalles de la vista previa de src/agent-view.tsx",
    );
    expect(previewDetails).toHaveAttribute(
      "title",
      "Detalles de la vista previa de Agent Lens para src/agent-view.tsx: resumen y ubicación del primer fragmento.",
    );
    expect(
      within(previewDetails).getByTitle(
        "Resumen de la vista previa del archivo seleccionado src/agent-view.tsx: 1 fragmento, 2 añadidas, 1 eliminadas.",
      ),
    ).toHaveTextContent("1 fragmento - +2 / -1");
    expect(
      within(previewDetails).getByTitle(
        "Detalle de la vista previa del archivo seleccionado src/agent-view.tsx: Primer fragmento @@ -1 +1.",
      ),
    ).toHaveTextContent("Primer fragmento @@ -1 +1.");
    expect(within(lens).getByRole("button", { name: "Vista previa" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("navigates Agent Lens file previews with controls and keyboard shortcuts", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [
              { path: "src/agent-view.tsx", kind: "modified", timestamp_ms: 2 },
              { path: "docs/agent-view.md", kind: "created", timestamp_ms: 2 },
            ],
          },
        ],
      }),
    ]);
    act(() => {
      busStore.applyDelta(
        repoDelta({
          status: {
            modified: ["src/agent-view.tsx"],
            staged: [],
            untracked: ["docs/agent-view.md"],
          },
          subscribed_diffs: [
            fileDiff("src/agent-view.tsx", [
              { kind: "Added", content: "component", old_lineno: null, new_lineno: 1 },
            ]),
            fileDiff("docs/agent-view.md", [
              { kind: "Added", content: "notes", old_lineno: null, new_lineno: 1 },
            ]),
          ],
        }),
      );
    });

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    const preview = within(lens).getByLabelText("Vista previa del archivo seleccionado");
    expect(
      within(preview).getByTitle(
        "Posición de la vista previa de Agent Lens: 1 de 2 archivos visibles.",
      ),
    ).toHaveTextContent("1 / 2");
    const navigation = within(preview).getByLabelText("Navegación por la vista previa");
    expect(navigation).toHaveAttribute(
      "title",
      "Navegación de la vista previa de Agent Lens para src/agent-view.tsx: recorre 2 archivos visibles.",
    );
    expect(within(navigation).getByRole("button", { name: "Anterior" })).toHaveAttribute(
      "title",
      "Mostrar el archivo anterior en la vista previa de Agent Lens: docs/agent-view.md.",
    );
    expect(within(navigation).getByRole("button", { name: "Siguiente" })).toHaveAttribute(
      "title",
      "Mostrar el archivo siguiente en la vista previa de Agent Lens: docs/agent-view.md.",
    );

    await user.click(within(navigation).getByRole("button", { name: "Siguiente" }));

    expect(
      within(preview).getByTitle("La vista previa de Agent Lens muestra docs/agent-view.md."),
    ).toHaveTextContent("docs/agent-view.md");
    expect(
      within(preview).getByTitle(
        "Posición de la vista previa de Agent Lens: 2 de 2 archivos visibles.",
      ),
    ).toHaveTextContent("2 / 2");

    preview.focus();
    await user.keyboard("{ArrowLeft}");

    expect(
      within(preview).getByTitle("La vista previa de Agent Lens muestra src/agent-view.tsx."),
    ).toHaveTextContent("src/agent-view.tsx");

    await user.keyboard("{ArrowRight}");

    expect(
      within(preview).getByTitle("La vista previa de Agent Lens muestra docs/agent-view.md."),
    ).toHaveTextContent("docs/agent-view.md");
  });

  it("groups Agent Lens files by artifact type", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [
          { path: "src/agent-view.tsx", kind: "modified", timestamp_ms: 10 },
          { path: "src/agent-view.test.tsx", kind: "modified", timestamp_ms: 11 },
          { path: "docs/agent-view.md", kind: "created", timestamp_ms: 12 },
          { path: "package.json", kind: "modified", timestamp_ms: 13 },
        ],
        turn_checkpoints: [],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    const codeGroup = within(lens).getByLabelText("Archivos de código");
    expect(codeGroup).toHaveTextContent("Código");
    const codeFileRow = within(codeGroup).getByTitle(
      "Archivo modificado de Agent Lens para la sesión: modificado src/agent-view.tsx.",
    );
    expect(codeFileRow).toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle(
        "Ámbito de la fila de archivo de Agent Lens: registro de cambios de la sesión en +0s.",
      ),
    ).toHaveTextContent("Sesión - +0s");
    expect(
      within(codeFileRow).getByTitle(
        "Ruta de la fila de archivo de Agent Lens: src/agent-view.tsx.",
      ),
    ).toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle(
        "Tipo de cambio de la fila de archivo Código de Agent Lens: modificado.",
      ),
    ).toHaveTextContent("modificado");
    const testsGroup = within(lens).getByLabelText("Archivos de pruebas");
    expect(testsGroup).toHaveTextContent("Pruebas");
    expect(within(testsGroup).getByText("src/agent-view.test.tsx")).toBeInTheDocument();
    expect(
      within(within(lens).getByLabelText("Archivos de documentación")).getByText(
        "docs/agent-view.md",
      ),
    ).toBeInTheDocument();
    expect(
      within(within(lens).getByLabelText("Archivos de configuración")).getByText("package.json"),
    ).toBeInTheDocument();
  });

  it("focuses turns from the turn map and copies the focused transcript", async () => {
    const user = userEvent.setup();
    installClipboardMock();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/first.ts", kind: "modified", timestamp_ms: 2 }],
          },
          {
            id: "sess-1:turn-2",
            index: 2,
            started_at_ms: 3,
            ended_at_ms: 4,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [
              { path: "src/second.ts", kind: "created", timestamp_ms: 4 },
              { path: "src/second.test.ts", kind: "created", timestamp_ms: 4 },
              { path: "docs/second.md", kind: "created", timestamp_ms: 4 },
              { path: "package.json", kind: "modified", timestamp_ms: 4 },
              { path: "src/hidden.ts", kind: "modified", timestamp_ms: 4 },
            ],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "First task",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "First done",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:3",
        kind: "command_output",
        text: "npm test first",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:4",
        kind: "user_message",
        text: "Second task",
        timestamp_ms: 3,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:5",
        kind: "agent_message",
        text: "Second done",
        timestamp_ms: 4,
      });
    });

    const focus = await screen.findByLabelText("Turno seleccionado");
    expect(within(focus).getByText("Turno 2")).toBeInTheDocument();
    expect(within(focus).getByText("Second done")).toBeInTheDocument();
    expect(within(focus).getByLabelText("Archivos del turno seleccionado")).toBeInTheDocument();
    expect(
      within(focus).getByTitle(
        "Fila de archivo modificado del turno seleccionado 2: creado src/second.ts.",
      ),
    ).toHaveTextContent("creado src/second.ts");
    expect(
      within(focus).getByTitle(
        "Archivos adicionales ocultos del turno seleccionado: 1 archivo modificado en el turno 2.",
      ),
    ).toHaveTextContent("+1 más");

    await user.click(
      within(screen.getByLabelText("Mapa de turnos")).getByRole("button", { name: /T1/ }),
    );

    expect(within(focus).getByText("Turno 1")).toBeInTheDocument();
    expect(
      within(focus).getByTitle("Etiqueta de índice del turno seleccionado: Turno 1."),
    ).toHaveTextContent("Turno 1");
    expect(within(focus).getByText("3 mensajes / 1 comando / 1 archivo")).toBeInTheDocument();
    expect(within(focus).getByText("npm test first")).toHaveAttribute(
      "title",
      "Actividad registrada más reciente del turno seleccionado 1: npm test first",
    );
    expect(within(focus).queryByText("Second done")).not.toBeInTheDocument();
    expect(
      within(focus).getByLabelText("Resumen de artefactos del turno seleccionado"),
    ).toHaveTextContent("Código 1");
    expect(
      within(focus).getByLabelText("Resumen de comandos del turno seleccionado"),
    ).toHaveTextContent("Comando reciente: npm test first");
    expect(scrollIntoViewMock).toHaveBeenCalled();
    const firstTurnArticle = screen.getByText("First task").closest("article");
    expect(firstTurnArticle).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("button", { name: "Detalles" })).toHaveAttribute(
      "title",
      "Mostrar los detalles, puntos de restauración, archivos, comandos y Timeline.",
    );
    await user.click(screen.getByRole("button", { name: "Detalles" }));
    expect(
      screen.getByTitle(
        "Detalles de la sesión: mapa de turnos, actividad actual, puntos de restauración y Agent Lens.",
      ),
    ).toHaveClass("agent-panel__details-head");
    expect(within(focus).getByRole("button", { name: "Restaurar aquí" })).toHaveAttribute(
      "title",
      "Restaurar el turno 1: detén la sesión antes.",
    );
    expect(screen.getByLabelText("Mensaje para Codex")).toHaveValue("");
  });

  it("shows session change log files in Agent Lens when turn checkpoints are unavailable", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [{ path: "src/session-only.ts", kind: "modified", timestamp_ms: 10 }],
        turn_checkpoints: [],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendOutput({
        session_id: "sess-1",
        chunk_base64: b64("Fallback transcript without turn checkpoints"),
        timestamp_ms: 11,
      });
    });

    const lens = await screen.findByLabelText("Agent Lens");
    expect(within(lens).getByLabelText("Archivos modificados")).toBeInTheDocument();
    expect(within(lens).getAllByText("src/session-only.ts").length).toBeGreaterThan(0);
    expect(
      within(lens).getByTitle("La vista previa de Agent Lens muestra src/session-only.ts."),
    ).toHaveTextContent("src/session-only.ts");
    expect(
      within(lens).getByTitle(
        "Vista previa del archivo seleccionado src/session-only.ts: no hay datos de fragmentos en vivo.",
      ),
    ).toHaveTextContent("No hay datos en vivo de los fragmentos de este archivo.");
    expect(within(lens).getByText("Sesión - +0s")).toBeInTheDocument();
    const fileActions = within(lens).getByTitle(
      "Acciones de Agent Lens para src/session-only.ts: vista previa, abrir y preguntar.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "Acciones para src/session-only.ts");
    expect(within(fileActions).getByRole("button", { name: "Vista previa" })).toHaveAttribute(
      "title",
      "Se muestran los detalles de src/session-only.ts en Agent Lens.",
    );
    expect(within(fileActions).getByRole("button", { name: "Abrir" })).toHaveAttribute(
      "title",
      "Abrir src/session-only.ts desde Agent Lens en el espacio de trabajo.",
    );
    expect(within(fileActions).getByRole("button", { name: "Preguntar" })).toHaveAttribute(
      "title",
      "Preparar un prompt de seguimiento de Agent Lens para src/session-only.ts.",
    );
    expect(within(fileActions).queryByRole("button", { name: "Revertir" })).not.toBeInTheDocument();
    expect(revertSessionTurnFileMock).not.toHaveBeenCalled();
  });

  it("filters touched files inside Agent Lens", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [
          { path: "src/alpha.ts", kind: "modified", timestamp_ms: 10 },
          { path: "src/beta.ts", kind: "created", timestamp_ms: 11 },
        ],
        turn_checkpoints: [],
      }),
    ]);
    act(() => {
      busStore.applyDelta(
        repoDelta({
          status: { modified: [], staged: ["src/beta.ts"], untracked: [] },
          subscribed_diffs: [],
        }),
      );
    });

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    const fileFilter = within(lens).getByLabelText("Filtrar archivos modificados");
    expect(fileFilter).toHaveAttribute(
      "title",
      "Filtrar 2 archivos modificados de Agent Lens por ruta, tipo de cambio, estado o categoría.",
    );
    expect(within(lens).getByText("2 archivos")).toBeInTheDocument();

    await user.type(fileFilter, "beta");

    expect(within(lens).getAllByText("src/beta.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/alpha.ts")).not.toBeInTheDocument();
    expect(within(lens).getByText("1 de 2 archivos")).toBeInTheDocument();

    await user.clear(within(lens).getByLabelText("Filtrar archivos modificados"));
    await user.type(within(lens).getByLabelText("Filtrar archivos modificados"), "removed");

    expect(within(lens).getByText("Ningún archivo coincide con este filtro.")).toBeInTheDocument();
    expect(
      within(lens).getByTitle(
        "Se muestran 0 de 2 archivos modificados de Agent Lens después de filtrar.",
      ),
    ).toHaveTextContent("0 de 2 archivos");

    await user.clear(within(lens).getByLabelText("Filtrar archivos modificados"));
    await user.type(within(lens).getByLabelText("Filtrar archivos modificados"), "preparado");

    expect(within(lens).getAllByText("src/beta.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/alpha.ts")).not.toBeInTheDocument();

    await user.clear(within(lens).getByLabelText("Filtrar archivos modificados"));
    await user.type(within(lens).getByLabelText("Filtrar archivos modificados"), "código");

    expect(within(lens).getAllByText("src/alpha.ts").length).toBeGreaterThan(0);
    expect(within(lens).getAllByText("src/beta.ts").length).toBeGreaterThan(0);
  });

  it("explains empty Agent Lens file states", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    expect(
      within(lens).getByTitle("Agent Lens no tiene archivos modificados en la sesión actual."),
    ).toHaveTextContent("Aún no hay archivos modificados.");
  });

  it("confirms and reverts completed sessions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "completed", pid: null, exit_code: 0 }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const revert = await screen.findByRole("button", { name: "Revertir" });
    expect(revert).toHaveAttribute("title", "Revertir los cambios de la sesión de Codex en a.");
    await user.click(revert);

    expect(confirmMock).toHaveBeenCalled();
    expect(revertSessionMock).toHaveBeenCalledWith("sess-1", true);
  });

  it("stops a running session from the header and refreshes sessions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock
      .mockResolvedValueOnce([sessionFixture({ status: "running" })])
      .mockResolvedValueOnce([
        sessionFixture({
          status: "completed",
          pid: null,
          exit_code: 0,
          turn_status: "waiting",
        }),
      ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Detener" }));

    expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
    await waitFor(() => expect(listAgentSessionsMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Sesión completada")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Detener" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Detener" })).toHaveAttribute(
      "title",
      "Detener Codex en a: la sesión no está en ejecución.",
    );
  });

  it("reverts a single file from Agent Lens", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        status: "completed",
        pid: null,
        exit_code: 0,
        turn_status: "waiting",
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(
      await screen.findByTitle("Revertir src/a.ts al punto de control de el turno 1."),
    );

    expect(confirmMock).toHaveBeenCalled();
    expect(revertSessionTurnFileMock).toHaveBeenCalledWith(
      "sess-1",
      "sess-1:turn-1",
      "src/a.ts",
      true,
    );
    expect(revertSessionMock).not.toHaveBeenCalled();
  });

  it("restores files and chat view to a completed turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        status: "completed",
        pid: null,
        exit_code: 0,
        turn_status: "waiting",
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            restore_checkpoint: {
              checkpoint_type: "fs_snapshot",
              git_hash: null,
              snapshot_files: [],
            },
            changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "First request",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:1",
        kind: "agent_message",
        text: "First done",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:2",
        kind: "user_message",
        text: "Second request",
        timestamp_ms: 3,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "Second done",
        timestamp_ms: 4,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(await within(conversation).findByText("Second done")).toBeInTheDocument();
    const lens = await screen.findByLabelText("Agent Lens");
    expect(
      within(lens).getByTitle(
        "Valor de puntos de restauración de Agent Lens: 1 de 1 puntos de control son restaurables. Último turno restaurable: 1.",
      ),
    ).toHaveTextContent("1/1");
    expect(
      within(lens).getByTitle(
        "Métrica de puntos de restauración de Agent Lens: 1 punto de restauración de 1 punto de control. El último turno restaurable es el 1.",
      ),
    ).toHaveTextContent("Puntos de restauración");
    await user.click(
      within(screen.getByLabelText("Mapa de turnos")).getByRole("button", { name: /T1/ }),
    );

    const restore = await screen.findByRole("button", { name: "Restaurar aquí" });
    expect(restore).toHaveAttribute(
      "title",
      "Restaurar los archivos y la conversación al turno 1.",
    );
    await user.click(restore);

    expect(confirmMock).toHaveBeenCalled();
    expect(restoreSessionTurnMock).toHaveBeenCalledWith("sess-1", "sess-1:turn-1", true);
    await waitFor(() =>
      expect(within(conversation).queryByText("Second done")).not.toBeInTheDocument(),
    );
    expect(within(conversation).getByText("First done")).toBeInTheDocument();
  });

  it("stops the backend session when the panel closes", async () => {
    const { unmount } = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    unmount();
    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 300));
    });

    expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
  });

  it("gives detached transfers a grace window before stopping", async () => {
    vi.useFakeTimers();
    try {
      markTerminalDetached("sess-1");
      const { unmount } = render(
        <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      );

      unmount();
      await act(async () => {
        vi.advanceTimersByTime(300);
      });
      expect(stopAgentSessionMock).not.toHaveBeenCalled();

      await act(async () => {
        vi.advanceTimersByTime(5000);
      });
      expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
    } finally {
      vi.useRealTimers();
    }
  });

  it("disables revert when a completed session has no checkpoint", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "completed", pid: null, exit_code: 0, checkpoint: null }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const button = await screen.findByRole("button", { name: "Revertir" });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "Revertir cambios de Codex en a: no hay un punto de control reversible.",
    );
    expect(screen.getByTitle("Acción para revertir Agent: Revertir.")).toHaveTextContent(
      "Revertir",
    );
  });
});
