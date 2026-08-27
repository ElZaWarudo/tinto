import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import { StrictMode, type ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AgentHostCommandResult,
  AgentRuntimeCatalog,
  AgentSession,
  AgentSessionPermissionMode,
  FileDiff,
  McpInventory,
  McpProfileState,
  RepoDelta,
  WorkbenchConfig,
} from "../../bus/contract";

const writeAgentSessionInputMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const getCodexMcpInventoryMock = vi.fn<() => Promise<McpInventory>>(() =>
  Promise.resolve({
    provider: "codex" as const,
    target: "windows_local" as const,
    status: "empty" as const,
    definitions: [],
    error: null,
    checked_at_ms: 1,
  }),
);
const listWorkbenchesMock = vi.fn<() => Promise<WorkbenchConfig>>(() =>
  Promise.resolve({ version: 1, active: "Work", workbenches: [] }),
);
const listMcpProfilesMock = vi.fn<(workbench: string) => Promise<McpProfileState>>((workbench) => {
  void workbench;
  return Promise.resolve({
    profiles: [],
    active_profile_id: null,
    delivery_status: "unsupported" as const,
  });
});
const importCodexMcpProfileMock = vi.fn((workbench: string) => listMcpProfilesMock(workbench));
const createMcpProfileMock = vi.fn((workbench: string, name: string) => {
  void name;
  return listMcpProfilesMock(workbench);
});
const renameMcpProfileMock = vi.fn((workbench: string, profileId: string, name: string) => {
  void profileId;
  void name;
  return listMcpProfilesMock(workbench);
});
const deleteMcpProfileMock = vi.fn(
  (workbench: string, profileId: string, replacementId?: string | null) => {
    void profileId;
    void replacementId;
    return listMcpProfilesMock(workbench);
  },
);
const setMcpDefaultProfileMock = vi.fn((workbench: string, profileId: string) => {
  void profileId;
  return listMcpProfilesMock(workbench);
});
const writeAgentSessionTurnMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const steerAgentSessionTurnMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const listAgentSessionsMock = vi.fn<() => Promise<AgentSession[]>>(() => Promise.resolve([]));
const getAgentJournalSessionMock = vi.fn<() => Promise<AgentSession | null>>(() =>
  Promise.resolve(null),
);
const resumeAgentJournalSessionMock = vi.fn((sessionId: string) => {
  void sessionId;
  return Promise.resolve({ session_id: "sess-resumed", mode: "native" as const });
});
const branchAgentSessionFromMessageMock = vi.fn((sessionId: string, messageId: string) => {
  void sessionId;
  void messageId;
  return Promise.resolve({ session_id: "sess-edited", mode: "context_bridge" as const });
});
const getAgentRuntimeCatalogMock = vi.fn<
  (sessionId: string, refresh?: boolean) => Promise<AgentRuntimeCatalog | null>
>(() => Promise.resolve(runtimeCatalogFixture()));
const getAgentImagePreviewMock = vi.fn<(path: string) => Promise<string | null>>(() =>
  Promise.resolve("data:image/png;base64,cHJldmlldw=="),
);
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
const setAgentSessionPermissionModeMock =
  vi.fn<(sessionId: string, permissionMode: AgentSessionPermissionMode) => Promise<AgentSession>>();
const interruptAgentSessionTurnMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const retryAgentSessionAcpMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const respondAgentSessionAcpPermissionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const setAgentSessionAcpConfigOptionMock = vi.fn((...args: unknown[]) => {
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
const openMock = vi.fn((...args: unknown[]): Promise<string | string[] | null> => {
  void args;
  return Promise.resolve(null);
});
const scrollIntoViewMock = vi.fn();
const writeClipboardTextMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});

vi.mock("../../bus/client", () => ({
  branchAgentSessionFromMessage: (sessionId: string, messageId: string) =>
    branchAgentSessionFromMessageMock(sessionId, messageId),
  getAgentImagePreview: (path: string) => getAgentImagePreviewMock(path),
  getCodexMcpInventory: () => getCodexMcpInventoryMock(),
  getAgentJournalSession: () => getAgentJournalSessionMock(),
  getAgentRuntimeCatalog: (sessionId: string, refresh?: boolean) =>
    getAgentRuntimeCatalogMock(sessionId, refresh),
  listAgentSessions: () => listAgentSessionsMock(),
  listMcpProfiles: (workbench: string) => listMcpProfilesMock(workbench),
  listWorkbenches: () => listWorkbenchesMock(),
  importCodexMcpProfile: (workbench: string) => importCodexMcpProfileMock(workbench),
  createMcpProfile: (workbench: string, name: string) => createMcpProfileMock(workbench, name),
  renameMcpProfile: (workbench: string, profileId: string, name: string) =>
    renameMcpProfileMock(workbench, profileId, name),
  deleteMcpProfile: (workbench: string, profileId: string, replacementId?: string | null) =>
    deleteMcpProfileMock(workbench, profileId, replacementId),
  respondAgentSessionAcpPermission: (...a: unknown[]) => respondAgentSessionAcpPermissionMock(...a),
  resumeAgentJournalSession: (sessionId: string) => resumeAgentJournalSessionMock(sessionId),
  retryAgentSessionAcp: (...a: unknown[]) => retryAgentSessionAcpMock(...a),
  setAgentSessionAcpConfigOption: (...a: unknown[]) => setAgentSessionAcpConfigOptionMock(...a),
  setAgentSessionPermissionMode: (sessionId: string, permissionMode: AgentSessionPermissionMode) =>
    setAgentSessionPermissionModeMock(sessionId, permissionMode),
  setMcpDefaultProfile: (workbench: string, profileId: string) =>
    setMcpDefaultProfileMock(workbench, profileId),
  revertSession: (...a: unknown[]) => revertSessionMock(...a),
  revertSessionTurnFile: (...a: unknown[]) => revertSessionTurnFileMock(...a),
  restoreSessionTurn: (...a: unknown[]) => restoreSessionTurnMock(...a),
  runAgentHostCommand: (...a: unknown[]) => runAgentHostCommandMock(...a),
  interruptAgentSessionTurn: (...a: unknown[]) => interruptAgentSessionTurnMock(...a),
  stopAgentSession: (...a: unknown[]) => stopAgentSessionMock(...a),
  steerAgentSessionTurn: (...a: unknown[]) => steerAgentSessionTurnMock(...a),
  writeAgentSessionInput: (...a: unknown[]) => writeAgentSessionInputMock(...a),
  writeAgentSessionTurn: (...a: unknown[]) => writeAgentSessionTurnMock(...a),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: (...a: unknown[]) => confirmMock(...a),
  open: (...a: unknown[]) => openMock(...a),
}));

import {
  resetAgentComposerDraftsForTests,
  TerminalPanel,
  type TerminalPanelParams,
} from "./TerminalPanel";
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
    permission_mode: "workspace",
    status: "running",
    pid: 123,
    started_at_ms: 1,
    ended_at_ms: null,
    exit_code: null,
    error: null,
    checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
    change_log: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
    turn_status: "waiting",
    turn_interrupt_supported: true,
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
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
    secret_scan_status: { state: "not_run" },
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
    resetAgentComposerDraftsForTests();
    busStore.resetAll();
    consoleDock.resetForTests();
    scrollIntoViewMock.mockClear();
    writeClipboardTextMock.mockClear();
    writeAgentSessionInputMock.mockClear();
    writeAgentSessionTurnMock.mockClear();
    steerAgentSessionTurnMock.mockClear();
    localStorage.clear();
    openMock.mockReset();
    openMock.mockResolvedValue(null);
    runAgentHostCommandMock.mockClear();
    runAgentHostCommandMock.mockResolvedValue({
      command: "status",
      status: "completed",
      message: "Host command done.",
    });
    listAgentSessionsMock.mockReset();
    listAgentSessionsMock.mockResolvedValue([]);
    getAgentJournalSessionMock.mockClear();
    getAgentJournalSessionMock.mockResolvedValue(null);
    resumeAgentJournalSessionMock.mockClear();
    branchAgentSessionFromMessageMock.mockClear();
    getAgentRuntimeCatalogMock.mockReset();
    getAgentRuntimeCatalogMock.mockResolvedValue(runtimeCatalogFixture());
    getAgentImagePreviewMock.mockClear();
    getCodexMcpInventoryMock.mockClear();
    getCodexMcpInventoryMock.mockResolvedValue({
      provider: "codex",
      target: "windows_local",
      status: "empty",
      definitions: [],
      error: null,
      checked_at_ms: 1,
    });
    listWorkbenchesMock.mockClear();
    listWorkbenchesMock.mockResolvedValue({ version: 1, active: "Work", workbenches: [] });
    listMcpProfilesMock.mockClear();
    listMcpProfilesMock.mockResolvedValue({
      profiles: [],
      active_profile_id: null,
      delivery_status: "unsupported",
    });
    importCodexMcpProfileMock.mockClear();
    createMcpProfileMock.mockClear();
    renameMcpProfileMock.mockClear();
    deleteMcpProfileMock.mockClear();
    setMcpDefaultProfileMock.mockClear();
    revertSessionMock.mockClear();
    revertSessionTurnFileMock.mockClear();
    restoreSessionTurnMock.mockClear();
    stopAgentSessionMock.mockClear();
    setAgentSessionPermissionModeMock.mockReset();
    retryAgentSessionAcpMock.mockClear();
    respondAgentSessionAcpPermissionMock.mockClear();
    setAgentSessionAcpConfigOptionMock.mockClear();
    interruptAgentSessionTurnMock.mockClear();
    confirmMock.mockClear();
    localStorage.removeItem("tinto:runtime-presets:v1");
  });

  it("renders a product agent interface instead of a terminal surface", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture({ turn_status: "working" })]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const loading = screen.getByRole("status");
    expect(loading).toHaveAttribute("aria-live", "polite");
    expect(loading).toHaveTextContent("Cargando sesión");
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("Workspace")).toHaveAttribute(
      "title",
      "Permisos efectivos de esta sesión",
    );
    expect(screen.getByRole("button", { name: "Detener respuesta" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Enviar" })).toBeDisabled();
    expect(screen.getByText("a")).toBeInTheDocument();
    const composer = screen.getByLabelText("Mensaje para Codex");
    expect(composer).toHaveAttribute("placeholder", "Mensaje para Codex");
    const composerHint = composer.getAttribute("aria-describedby");
    expect(composerHint).toBeTruthy();
    expect(document.getElementById(composerHint!)).toHaveTextContent(
      "Escribe / para ver comandos o $ para ver habilidades.",
    );
    expect(screen.queryByText("Codex + Tinto + habilidades")).not.toBeInTheDocument();
    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(conversation).toHaveAttribute("role", "log");
    expect(conversation).toHaveAttribute("aria-live", "polite");
    expect(within(conversation).getByText("Turno en curso")).toBeInTheDocument();
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
    const sessionStatus = screen.getByTitle(/Estado: En ejecución/);
    expect(sessionStatus).toHaveTextContent("En ejecución");
    const processStatus = screen.getByRole("status", { name: "Codex está trabajando" });
    expect(processStatus).toHaveTextContent("Codex está trabajando");
    expect(processStatus).toHaveTextContent("EN CURSO");
    expect(screen.queryByTestId("terminal-surface")).not.toBeInTheDocument();
  });

  it("shows full access as the effective session permission", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ permission_mode: "full_access" }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(await screen.findByText("Acceso completo")).toHaveAttribute(
      "title",
      "Permisos efectivos de esta sesión",
    );
  });

  it("shows the Codex access selector only when the session supports dynamic changes", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ permission_mode_change_supported: true }),
    ]);
    setAgentSessionPermissionModeMock.mockResolvedValueOnce(
      sessionFixture({ permission_mode: "full_access", permission_mode_change_supported: true }),
    );

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
    await user.click(screen.getByRole("button", { name: /Configurar ejecuci/ }));
    const access = screen.getByRole("radio", { name: /^Acceso completo/ });
    expect(access).not.toBeChecked();
    await user.click(access);

    await waitFor(() =>
      expect(setAgentSessionPermissionModeMock).toHaveBeenCalledWith("sess-1", "full_access"),
    );
    expect(access).toBeChecked();
  });

  it("does not offer an access selector for Codex sessions without dynamic support", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
    await user.click(screen.getByRole("button", { name: /Configurar ejecuci/ }));
    expect(screen.queryByRole("radio", { name: "Acceso completo" })).not.toBeInTheDocument();
  });

  it("keeps the previous access and shows a notice when the backend rejects a change", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ permission_mode_change_supported: true }),
    ]);
    setAgentSessionPermissionModeMock.mockRejectedValueOnce(new Error("full access declined"));

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
    await user.click(screen.getByRole("button", { name: /Configurar ejecuci/ }));
    const workspace = screen.getByRole("radio", { name: /^Workspace/ });
    await user.click(screen.getByRole("radio", { name: /^Acceso completo/ }));

    await waitFor(() =>
      expect(screen.getByText(/No se pudo cambiar el acceso/)).toBeInTheDocument(),
    );
    expect(workspace).toBeChecked();
    expect(screen.getByRole("radio", { name: /^Acceso completo/ })).not.toBeChecked();
  });

  it("renders every ACP state, gates input, and confirms PTY migration", async () => {
    const acpSession = (state: NonNullable<AgentSession["acp_runtime"]>["state"]) =>
      sessionFixture({
        agent_type: "kimi",
        turn_status: "waiting",
        acp_runtime: {
          state,
          mode: state === "acp_ready" ? "acp" : state === "pty_compatibility" ? "pty" : null,
          detail:
            state === "authentication_required"
              ? "El proveedor solicitó autenticación."
              : state === "pty_compatibility"
                ? "El transporte ACP no superó la sonda."
                : state === "failed"
                  ? "La conexión terminó de forma inesperada."
                  : null,
          lost_capabilities:
            state === "pty_compatibility" ? ["actualizaciones estructuradas", "permisos ACP"] : [],
          retry_available: state === "authentication_required" || state === "pty_compatibility",
          image_attachments: false,
          config_options: [],
        },
        acp_permissions: [],
      });
    listAgentSessionsMock.mockResolvedValueOnce([acpSession("connecting_acp")]);
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "kimi" })} />);

    const connecting = await screen.findByRole("status", {
      name: "Estado ACP de Kimi Code: Conectando mediante ACP…",
    });
    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(conversation).not.toContainElement(connecting);
    expect(screen.getByLabelText("Mensaje para Kimi Code")).toBeDisabled();

    act(() => agentSessionStore.upsertSession(acpSession("authentication_required")));
    expect(
      await screen.findByRole("status", {
        name: "Estado ACP de Kimi Code: Autenticación necesaria",
      }),
    ).toHaveTextContent(
      "Inicia sesión con Kimi Code desde su CLI y pulsa Reintentar ACP. Tinto no recibe ni guarda credenciales.",
    );
    await user.click(screen.getByRole("button", { name: "Reintentar ACP" }));
    expect(confirmMock).not.toHaveBeenCalled();
    expect(retryAgentSessionAcpMock).toHaveBeenLastCalledWith("sess-1", false);

    act(() => agentSessionStore.upsertSession(acpSession("acp_ready")));
    expect(
      await screen.findByRole("status", { name: "Estado ACP de Kimi Code: ACP listo" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Mensaje para Kimi Code")).toBeEnabled();

    act(() => agentSessionStore.upsertSession(acpSession("pty_compatibility")));
    const compatibility = await screen.findByRole("status", {
      name: "Estado ACP de Kimi Code: Modo de compatibilidad PTY",
    });
    expect(compatibility).toHaveTextContent("actualizaciones estructuradas, permisos ACP");
    confirmMock.mockResolvedValueOnce(false);
    await user.click(screen.getByRole("button", { name: "Reintentar ACP" }));
    expect(retryAgentSessionAcpMock).toHaveBeenCalledTimes(1);
    confirmMock.mockResolvedValueOnce(true);
    await user.click(screen.getByRole("button", { name: "Reintentar ACP" }));
    expect(confirmMock).toHaveBeenLastCalledWith(
      expect.stringContaining("Se conservará la transcripción y el punto de control"),
      expect.objectContaining({ okLabel: "Cambiar a ACP", cancelLabel: "Mantener PTY" }),
    );
    expect(retryAgentSessionAcpMock).toHaveBeenLastCalledWith("sess-1", true);

    act(() => agentSessionStore.upsertSession(acpSession("failed")));
    expect(
      await screen.findByRole("status", { name: "Estado ACP de Kimi Code: ACP falló" }),
    ).toHaveTextContent("No se reenvió ni reprodujo el turno mediante PTY");
    expect(screen.getByLabelText("Mensaje para Kimi Code")).toBeDisabled();

    act(() => agentSessionStore.upsertSession(acpSession("unavailable")));
    expect(
      await screen.findByRole("status", {
        name: "Estado ACP de Kimi Code: ACP no disponible",
      }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Mensaje para Kimi Code")).toBeDisabled();
  });

  it("renders only negotiated ACP model and mode controls and changes the exact option", async () => {
    const readySession = sessionFixture({
      agent_type: "kimi",
      turn_status: "waiting",
      acp_runtime: {
        state: "acp_ready",
        mode: "acp",
        detail: null,
        lost_capabilities: [],
        retry_available: false,
        image_attachments: false,
        config_options: [
          {
            id: "model",
            label: "Modelo",
            category: "model",
            current_value: "moonshot-v1",
            values: [
              { id: "moonshot-v1", label: "Moonshot V1" },
              { id: "moonshot-v2", label: "Moonshot V2" },
            ],
          },
          {
            id: "mode",
            label: "Modo",
            category: "mode",
            current_value: "agent",
            values: [
              { id: "agent", label: "Agent" },
              { id: "plan", label: "Plan" },
            ],
          },
        ],
      },
      acp_permissions: [],
    });
    listAgentSessionsMock.mockResolvedValueOnce([readySession]);
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "kimi" })} />);

    const model = await screen.findByRole("combobox", { name: "Modelo de Kimi Code" });
    const mode = screen.getByRole("combobox", { name: "Modo de Kimi Code" });
    expect(model).toHaveValue("moonshot-v1");
    expect(mode).toHaveValue("agent");
    expect(model).toBeEnabled();
    await user.selectOptions(model, "moonshot-v2");
    expect(setAgentSessionAcpConfigOptionMock).toHaveBeenCalledWith(
      "sess-1",
      "model",
      "moonshot-v2",
    );

    act(() => agentSessionStore.upsertSession({ ...readySession, turn_status: "working" }));
    expect(model).toBeDisabled();
    expect(mode).toBeDisabled();

    act(() =>
      agentSessionStore.upsertSession({
        ...readySession,
        acp_runtime: { ...readySession.acp_runtime!, state: "connecting_acp", mode: null },
      }),
    );
    expect(model).toBeDisabled();
    expect(mode).toBeDisabled();

    act(() =>
      agentSessionStore.upsertSession({
        ...readySession,
        acp_runtime: { ...readySession.acp_runtime!, config_options: [] },
      }),
    );
    expect(screen.queryByLabelText("Configuración ACP de Kimi Code")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: /Kimi Code/ })).not.toBeInTheDocument();
  });

  it("accepts only negotiated image attachments for ACP", async () => {
    const readySession = sessionFixture({
      agent_type: "kimi",
      turn_status: "waiting",
      acp_runtime: {
        state: "acp_ready",
        mode: "acp",
        detail: null,
        lost_capabilities: [],
        retry_available: false,
        image_attachments: true,
        config_options: [],
      },
      acp_permissions: [],
    });
    listAgentSessionsMock.mockResolvedValueOnce([readySession]);
    openMock.mockResolvedValueOnce(["C:\\Temp\\screen.png", "C:\\Temp\\brief.pdf"]);
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "kimi" })} />);

    const attach = await screen.findByRole("button", { name: "Adjuntar archivos" });
    expect(attach).toBeEnabled();
    await user.click(attach);
    expect(openMock).toHaveBeenCalledWith({
      filters: [
        {
          name: "Imágenes",
          extensions: expect.arrayContaining(["png", "jpg", "jpeg", "webp", "gif"]),
        },
      ],
      multiple: true,
      title: "Adjuntar imágenes",
    });
    expect(screen.getByLabelText("Archivos adjuntos")).toHaveTextContent("screen.png");
    expect(screen.getByLabelText("Archivos adjuntos")).not.toHaveTextContent("brief.pdf");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Este proveedor ACP solo admite imágenes; se omitieron los otros archivos.",
    );

    await user.type(screen.getByLabelText("Mensaje para Kimi Code"), "Describe la captura");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    await waitFor(() =>
      expect(writeAgentSessionTurnMock).toHaveBeenCalledWith(
        "sess-1",
        "Describe la captura",
        ["C:\\Temp\\screen.png"],
        expect.any(Object),
      ),
    );

    act(() =>
      agentSessionStore.upsertSession({
        ...readySession,
        acp_runtime: { ...readySession.acp_runtime!, image_attachments: false },
      }),
    );
    expect(screen.getByRole("button", { name: "Adjuntar archivos" })).toBeDisabled();
  });

  it("renders authoritative ACP permission cards and sends the exact option or cancellation", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        agent_type: "kimi",
        turn_status: "working",
        acp_runtime: {
          state: "acp_ready",
          mode: "acp",
          detail: null,
          lost_capabilities: [],
          retry_available: false,
          image_attachments: false,
          config_options: [],
        },
        acp_permissions: [
          {
            id: "7:s:p1",
            generation: 7,
            provider_session_id: "provider-session",
            turn_id: "turn-1",
            tool_call_id: "tool-1",
            title: "Editar src/app.ts",
            options: [
              { id: "allow", label: "Permitir una vez", kind: "allow_once" },
              { id: "deny", label: "Denegar", kind: "reject_once" },
            ],
            state: "pending",
            reason: null,
            expires_at_ms: 60_000,
          },
          {
            id: "7:s:p2",
            generation: 7,
            provider_session_id: "provider-session",
            turn_id: "turn-1",
            tool_call_id: "tool-2",
            title: "Ejecutar pruebas",
            options: [{ id: "allow", label: "Permitir", kind: "allow_once" }],
            state: "pending",
            reason: null,
            expires_at_ms: 60_000,
          },
          {
            id: "6:s:old",
            generation: 6,
            provider_session_id: "old-provider-session",
            turn_id: "old-turn",
            tool_call_id: "old-tool",
            title: "Permiso anterior",
            options: [],
            state: "invalidated",
            reason: "La conexión ACP terminó antes de responder.",
            expires_at_ms: 1,
          },
        ],
      }),
    ]);
    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "kimi" })} />);

    const permissions = await screen.findByLabelText("Permisos ACP");
    const acpRegion = screen.getByRole("region", { name: "Estado ACP de Kimi Code" });
    const acpStatus = within(acpRegion).getByRole("status");
    expect(within(acpRegion).getAllByRole("status")).toHaveLength(1);
    expect(acpStatus).toHaveTextContent("Permiso Editar src/app.ts: Pendiente.");
    const editPermission = within(permissions).getByRole("article", {
      name: "Editar src/app.ts",
    });
    expect(editPermission).toHaveTextContent("Pendiente");
    await user.click(within(editPermission).getByRole("button", { name: "Denegar" }));
    expect(respondAgentSessionAcpPermissionMock).toHaveBeenCalledWith(
      "sess-1",
      "7:s:p1",
      "deny",
      false,
    );

    const testPermission = within(permissions).getByRole("article", {
      name: "Ejecutar pruebas",
    });
    await user.click(within(testPermission).getByRole("button", { name: "Denegar" }));
    expect(respondAgentSessionAcpPermissionMock).toHaveBeenCalledWith(
      "sess-1",
      "7:s:p2",
      undefined,
      true,
    );
    await user.click(within(testPermission).getByRole("button", { name: "Cancelar" }));
    expect(respondAgentSessionAcpPermissionMock).toHaveBeenCalledWith(
      "sess-1",
      "7:s:p2",
      undefined,
      false,
    );

    const invalidated = within(permissions).getByRole("article", {
      name: "Permiso anterior",
    });
    expect(invalidated).toHaveTextContent("Invalidado");
    expect(invalidated).toHaveTextContent("La conexión ACP terminó antes de responder.");
    expect(within(invalidated).queryByRole("button")).toBeNull();
    const current = agentSessionStore.getState().sessions["sess-1"]!;
    act(() =>
      agentSessionStore.upsertSession({
        ...current,
        acp_permissions: current.acp_permissions?.map((permission) =>
          permission.id === "7:s:p1" ? { ...permission, state: "allowed" } : permission,
        ),
      }),
    );
    expect(acpStatus).toHaveTextContent("Permiso Editar src/app.ts: Permitido.");
    expect(
      within(screen.getByLabelText("Conversación con Agent")).queryByLabelText("Permisos ACP"),
    ).toBeNull();
  });

  it("confirms a completed turn only when its checkpoint is ready", async () => {
    const completionLabel = "Turno completado · punto de control listo";
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "completed", pid: null, exit_code: 0, turn_status: "waiting" }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(
      await screen.findByTitle(`Indicador de estado de la sesión de Agent: ${completionLabel}.`),
    ).toHaveTextContent(completionLabel);
    expect(
      within(screen.getByLabelText("Actividad de Agent")).getByText(completionLabel),
    ).toBeInTheDocument();
  });

  it("does not claim a checkpoint for a completed turn when none is verifiable", async () => {
    const completionLabel = "Turno finalizado · sin punto de control verificable";
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        status: "completed",
        pid: null,
        exit_code: 0,
        turn_status: "waiting",
        checkpoint: null,
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(
      await screen.findByTitle(`Indicador de estado de la sesión de Agent: ${completionLabel}.`),
    ).toHaveTextContent(completionLabel);
    expect(
      within(screen.getByLabelText("Actividad de Agent")).getByText(completionLabel),
    ).toBeInTheDocument();
  });

  it("localizes an exited transcript status instead of exposing the raw lifecycle value", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "exited", pid: null, ended_at_ms: 2, turn_status: "waiting" }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(await screen.findByTitle(/Estado: Archivada/)).toHaveTextContent("Archivada");
    expect(screen.getByText("En espera")).toBeInTheDocument();
  });

  it("shows distinct live process states without exposing private reasoning", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ turn_status: "settling", output_bytes_per_second: 42 }),
    ]);

    const { unmount } = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    const settling = await screen.findByRole("status", {
      name: "Codex está revisando cambios",
    });
    expect(settling).toHaveTextContent("VERIFICANDO");
    expect(settling.querySelectorAll(".agent-panel__process-signal i")).toHaveLength(3);
    unmount();

    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture({ turn_status: "waiting" })]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await screen.findByTitle(/Estado: En ejecución/);
    expect(
      screen.queryByRole("status", { name: /Codex está trabajando|Codex está revisando cambios/ }),
    ).toBeNull();
  });

  it("prefers provider activity over a generic thinking label", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_status: "working",
        timeline: [
          {
            session_id: "sess-1",
            id: "user-1",
            kind: "user_message",
            text: "Ejecuta las pruebas",
            timestamp_ms: 10,
          },
          {
            session_id: "sess-1",
            id: "activity-1",
            kind: "activity",
            text: "Ejecutando npm test",
            timestamp_ms: 11,
          },
        ],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const conversation = screen.getByLabelText("Conversación con Agent");
    const activity = await within(conversation).findByText("Actividad en curso");
    const disclosure = activity.closest("details");
    expect(disclosure).not.toHaveAttribute("open");
    expect(disclosure).toHaveAttribute("data-active", "true");
    expect(disclosure?.querySelector(".agent-panel__thought-signal")).toBeNull();
    expect(
      within(disclosure as HTMLElement).getByText("Ejecutando npm test", {
        selector: ".agent-panel__thought-summary",
      }),
    ).toBeInTheDocument();
    expect(
      within(conversation)
        .getAllByText("Ejecutando npm test")
        .filter((element) => element.closest("summary")),
    ).toHaveLength(1);
    expect(screen.queryByRole("status", { name: "Ejecutando npm test" })).toBeNull();
    expect(screen.queryByText("Codex está trabajando")).toBeNull();

    act(() => {
      agentSessionStore.upsertSession(sessionFixture({ turn_status: "waiting" }));
    });
    expect(await within(conversation).findByText("Actividad")).toBeInTheDocument();
    expect(disclosure).not.toHaveAttribute("data-active");
    expect(disclosure).not.toHaveAttribute("open");
  });

  it("removes the PowerShell launcher from Codex command activity", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_status: "working",
        timeline: [
          {
            session_id: "sess-1",
            id: "user-1",
            kind: "user_message",
            text: "Ejecuta las pruebas",
            timestamp_ms: 10,
          },
          {
            session_id: "sess-1",
            id: "activity-1",
            kind: "activity",
            text: `Ejecutando "C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" -Command '$testTmp="C:\\repo\\tmp"; cargo test'`,
            timestamp_ms: 11,
          },
        ],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(
      (await within(conversation).findAllByText('Ejecutando $testTmp="C:\\repo\\tmp"; cargo test'))
        .length,
    ).toBeGreaterThan(0);
    expect(within(conversation).queryByText(/powershell\.exe/i)).not.toBeInTheDocument();
  });

  it("filters generic progress filler from the visible conversation", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_status: "working",
        timeline: [
          {
            session_id: "sess-1",
            id: "user-generic-progress",
            kind: "user_message",
            text: "Continúa",
            timestamp_ms: 10,
          },
          {
            session_id: "sess-1",
            id: "generic-progress",
            kind: "agent_progress",
            text: "Analizando el siguiente paso...",
            timestamp_ms: 11,
          },
        ],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect((await screen.findAllByText("Continúa")).length).toBeGreaterThan(0);
    expect(screen.queryAllByText("Analizando el siguiente paso...")).toHaveLength(0);
    expect(screen.getByRole("status", { name: "Codex está trabajando" })).toBeInTheDocument();
  });

  it("shows active host context that will steer the next turn", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        goal: {
          text: "Build the host harness",
          status: "active",
          token_budget: null,
          tokens_used: 0,
          time_used_seconds: 0,
          created_at_ms: 4,
          updated_at_ms: 4,
        },
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
    expect(context).toHaveTextContent("Estilo");
    expect(context).toHaveTextContent("precise");
    expect(context).toHaveTextContent("Plan");
    expect(context).toHaveTextContent("Activo");
    expect(context).not.toHaveTextContent("Resumen");
    expect(context).not.toHaveTextContent(
      "Review findings are structured and WSL parity is working.",
    );
    expect(screen.getByLabelText(/Objetivo En curso: Build the host harness/)).toHaveTextContent(
      "Build the host harness",
    );
  });

  it("shows the remaining provider context without exposing compacted content", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        context_usage: {
          used_tokens: 80_000,
          model_context_window: 128_000,
        },
        context_summary: {
          text: "Internal compacted summary that should stay out of the UI.",
          created_at_ms: 7,
          source_events: 3,
          source_turns: 2,
        },
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const remaining = await screen.findByRole("progressbar", {
      name: "Contexto restante: 38%",
    });
    expect(remaining).toHaveAttribute("aria-valuenow", "38");
    expect(remaining.getAttribute("title")).toContain("48");
    expect(screen.queryByText(/Internal compacted summary/)).not.toBeInTheDocument();
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

  it("resumes an exited live conversation before sending its preserved draft", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    listAgentSessionsMock
      .mockResolvedValueOnce([
        sessionFixture({
          status: "exited",
          pid: null,
          ended_at_ms: 2500,
          exit_code: 0,
          turn_status: "waiting",
          active_sessions: 0,
        }),
      ])
      .mockResolvedValueOnce([sessionFixture({ id: "sess-resumed", status: "running" })]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openAgentTerminal },
    );

    const composer = await screen.findByPlaceholderText("Continúa esta conversación");
    expect(composer).toHaveAttribute(
      "title",
      "Entrada de mensajes de Codex para a: el próximo mensaje retomará la conversación archivada.",
    );
    await user.type(composer, "Seguimos");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(resumeAgentJournalSessionMock).toHaveBeenCalledWith("sess-1"));
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-resumed", "Seguimos\r", {
      speed: "standard",
    });
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-resumed",
      repo: "/r/a",
      agentType: "codex",
      replaceSessionId: "sess-1",
    });
    expect(composer).toHaveValue("");
  });

  it("recovers a restored live conversation from the journal after an app restart", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({
        status: "exited",
        pid: null,
        ended_at_ms: 2500,
        exit_code: 0,
        turn_status: "waiting",
        active_sessions: 0,
        timeline: [
          {
            session_id: "sess-1",
            id: "archived-agent-message",
            kind: "agent_message",
            text: "Conversación conservada tras reiniciar.",
            timestamp_ms: 2,
          },
        ],
      }),
    );
    listAgentSessionsMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([sessionFixture({ id: "sess-resumed", status: "running" })]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openAgentTerminal },
    );

    expect(
      await within(screen.getByRole("log", { name: "Conversación con Agent" })).findByText(
        "Conversación conservada tras reiniciar.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Cargando sesión")).not.toBeInTheDocument();
    const composer = screen.getByPlaceholderText("Continúa esta conversación");
    await user.type(composer, "Seguimos después del reinicio");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(resumeAgentJournalSessionMock).toHaveBeenCalledWith("sess-1"));
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith(
      "sess-resumed",
      "Seguimos después del reinicio\r",
      { speed: "standard" },
    );
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-resumed",
      repo: "/r/a",
      agentType: "codex",
      replaceSessionId: "sess-1",
    });
  });

  it("automatically retries a failed first message in the same resumed conversation", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        status: "exited",
        pid: null,
        ended_at_ms: 2500,
        exit_code: 0,
        turn_status: "waiting",
        active_sessions: 0,
      }),
    ]);
    writeAgentSessionInputMock
      .mockRejectedValueOnce(new Error("Session is still starting"))
      .mockResolvedValueOnce(undefined);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openAgentTerminal },
    );

    const composer = await screen.findByPlaceholderText("Continúa esta conversación");
    await user.type(composer, "Seguimos");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalledTimes(2));
    expect(resumeAgentJournalSessionMock).toHaveBeenCalledTimes(1);
    expect(writeAgentSessionInputMock).toHaveBeenLastCalledWith("sess-resumed", "Seguimos\r", {
      speed: "standard",
    });
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-resumed",
      repo: "/r/a",
      agentType: "codex",
      replaceSessionId: "sess-1",
    });
    expect(composer).toHaveValue("");
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
    consoleError.mockRestore();
  });

  it("sends Codex turns with models discovered from the runtime catalog", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
    await user.click(screen.getByRole("button", { name: /Configurar ejecuci/ }));
    await user.click(screen.getByRole("radio", { name: /GPT-5\.6 Luna/ }));
    await user.click(screen.getByRole("radio", { name: /^Alto/ }));
    await user.keyboard("{Escape}");

    const composer = screen.getByLabelText("Mensaje para Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      model: "gpt-5.6-luna",
      reasoning_effort: "high",
      speed: "standard",
    });
  });

  it("creates, edits, applies, and deletes presets from the current runtime configuration", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const presetTrigger = await screen.findByRole("button", { name: /Configuración guardada/ });
    expect(presetTrigger).toHaveTextContent("Diario");
    await user.click(presetTrigger);
    await user.click(screen.getByRole("button", { name: /Nuevo/ }));

    const dialog = screen.getByRole("dialog", { name: /Configuraci/ });
    expect(within(dialog).getByLabelText("Modelo")).toHaveValue("auto");
    expect(within(dialog).getByLabelText("Razonamiento")).toHaveValue("auto");
    expect(within(dialog).getByLabelText("Perfil")).toHaveValue("standard");
    await user.type(within(dialog).getByLabelText("Nombre"), "Mi flujo");
    await user.click(within(dialog).getByRole("button", { name: "Rumbo" }));
    await user.click(within(dialog).getByRole("button", { name: "Color rosa" }));
    await user.click(within(dialog).getByRole("button", { name: "Guardar y aplicar" }));

    expect(presetTrigger).toHaveTextContent("Mi flujo");
    expect(localStorage.getItem("tinto:runtime-presets:v1")).toContain(
      '"name":"Mi flujo","model":"auto","reasoning":"auto","speed":"standard","icon":"compass","color":"#f43f5e"',
    );
    await user.click(
      within(dialog).getByRole("button", { name: "Editar configuración guardada Mi flujo" }),
    );
    const name = within(dialog).getByLabelText("Nombre");
    await user.clear(name);
    await user.type(name, "Mi flujo diario");
    await user.click(within(dialog).getByRole("button", { name: "Guardar y aplicar" }));

    await user.click(
      within(dialog).getByRole("button", {
        name: "Editar configuración guardada Mi flujo diario",
      }),
    );
    await user.click(within(dialog).getByRole("button", { name: "Eliminar" }));
    await user.click(within(dialog).getByRole("button", { name: "Confirmar borrado" }));
    expect(within(dialog).queryByText("Mi flujo diario")).not.toBeInTheDocument();
  });

  it("applies a preset as one atomic runtime selection", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
    await user.click(screen.getByRole("button", { name: /^Trabajo profundo/ }));
    const composer = screen.getByLabelText("Mensaje para Codex");
    await user.type(composer, "analiza el sistema");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "analiza el sistema\r", {
      reasoning_effort: "high",
      speed: "standard",
    });
  });

  it("starts new chats with the favorite preset and exposes Rápido beside it", async () => {
    localStorage.setItem(
      "tinto:runtime-presets:v1",
      JSON.stringify([
        {
          id: "favorite-fast",
          name: "Mi favorito",
          model: "auto",
          reasoning: "medium",
          speed: "fast",
          icon: "bolt",
          color: "#f59e0b",
          favorite: true,
        },
      ]),
    );
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(await screen.findByRole("button", { name: /Configuración guardada/ })).toHaveTextContent(
      "Mi favorito",
    );
    expect(screen.getByRole("button", { name: "Rápido" })).toHaveAttribute("aria-pressed", "true");
  });

  it("toggles modo rápido directly without opening runtime settings", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const fast = await screen.findByRole("button", { name: "Rápido" });
    expect(fast).toHaveAttribute("aria-pressed", "false");
    expect(fast).toHaveAttribute("title", "Activar modo rápido");
    await user.click(fast);
    expect(fast).toHaveAttribute("aria-pressed", "true");
    expect(fast).toHaveAttribute("title", "Desactivar modo rápido");
    expect(screen.queryByText("Perfil rápido para el próximo turno.")).not.toBeInTheDocument();
    await user.click(fast);
    expect(fast).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByText("Perfil normal para el próximo turno.")).not.toBeInTheDocument();
  });

  it("queues normal sends and keeps steer and interrupt as explicit active-turn actions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock
      .mockResolvedValueOnce([sessionFixture({ turn_status: "working" })])
      .mockResolvedValueOnce([
        sessionFixture({ status: "completed", pid: null, turn_status: "waiting" }),
      ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    const composerRow = composer.closest(".agent-panel__composer-row");
    const send = screen.getByRole("button", { name: "Enviar" });
    const steer = screen.getByRole("button", { name: "Intervenir en el turno activo" });
    const stop = screen.getByRole("button", { name: "Detener respuesta" });
    expect(
      screen.queryByRole("button", { name: "Encolar para el siguiente turno" }),
    ).not.toBeInTheDocument();
    expect(
      Array.from(composerRow?.querySelectorAll("button, textarea") ?? []).map(
        (control) => control.getAttribute("aria-label") ?? control.textContent?.trim(),
      ),
    ).toEqual([
      "Adjuntar archivos",
      "Mensaje para Codex",
      "Enviar",
      "Intervenir en el turno activo",
      "Detener respuesta",
    ]);
    await user.type(composer, "Haz esto después");
    screen.getByRole("button", { name: "Adjuntar archivos" }).focus();
    await user.tab();
    expect(composer).toHaveFocus();
    await user.tab();
    expect(send).toHaveFocus();
    await user.tab();
    expect(steer).toHaveFocus();
    await user.tab();
    expect(stop).toHaveFocus();

    await user.click(send);
    expect(screen.getByText("1 en cola")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Rápido" }));

    await user.type(composer, "Ten en cuenta este detalle");
    await user.click(steer);
    expect(steerAgentSessionTurnMock).toHaveBeenCalledWith(
      "sess-1",
      "Ten en cuenta este detalle",
      [],
    );

    await user.click(stop);
    expect(interruptAgentSessionTurnMock).toHaveBeenCalledWith("sess-1");
    expect(stopAgentSessionMock).not.toHaveBeenCalled();
    expect(screen.getByText("Cola pausada")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reanudar cola" })).toBeEnabled();

    act(() => {
      agentSessionStore.upsertSession(
        sessionFixture({
          turn_status: "waiting",
          turn_interrupt_supported: true,
          runtime_options: { speed: "standard" },
        }),
      );
    });
    expect(writeAgentSessionTurnMock).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Reanudar cola" }));
    await waitFor(() =>
      expect(writeAgentSessionTurnMock).toHaveBeenCalledWith("sess-1", "Haz esto después", [], {
        speed: "fast",
      }),
    );
  });

  it("migrates a legacy queue once instead of replaying it after drain", async () => {
    const legacyMessage = {
      id: "sess-1:queued:4",
      text: "Mensaje legado",
      attachments: [],
    };
    localStorage.setItem(
      "tinto.agent-message-queues.v1",
      JSON.stringify({ "sess-1": [legacyMessage] }),
    );
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({ turn_status: "working", runtime_options: { speed: "standard" } }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await screen.findByText("1 en cola");

    expect(
      JSON.parse(localStorage.getItem("tinto.agent-message-queues.v1") ?? "{}"),
    ).not.toHaveProperty("sess-1");
    expect(JSON.parse(localStorage.getItem("tinto.agent-message-queues.v2") ?? "{}")).toMatchObject(
      {
        "sess-1": { messages: [legacyMessage], paused: false },
      },
    );
  });

  it("continues restored queue ids after the highest persisted suffix", async () => {
    const user = userEvent.setup();
    localStorage.setItem(
      "tinto.agent-message-queues.v2",
      JSON.stringify({
        "sess-1": {
          messages: [
            { id: "sess-1:queued:2", text: "Dos", attachments: [] },
            { id: "sess-1:queued:5", text: "Cinco", attachments: [] },
          ],
          paused: true,
        },
      }),
    );
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({ turn_status: "working", runtime_options: { speed: "standard" } }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "Seis");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => {
      const queues = JSON.parse(
        localStorage.getItem("tinto.agent-message-queues.v2") ?? "{}",
      ) as Record<string, { messages: Array<{ id: string }> }>;
      expect(queues["sess-1"].messages.map((message) => message.id)).toEqual([
        "sess-1:queued:2",
        "sess-1:queued:5",
        "sess-1:queued:6",
      ]);
    });
  });

  it("hydrates runtime options before dispatching a restored queue in StrictMode", async () => {
    localStorage.setItem(
      "tinto.agent-message-queues.v2",
      JSON.stringify({
        "sess-1": {
          messages: [{ id: "sess-1:queued:1", text: "Usa rápido", attachments: [] }],
          paused: false,
        },
      }),
    );
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({ turn_status: "waiting", runtime_options: { speed: "fast" } }),
    ]);

    render(
      <StrictMode>
        <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />
      </StrictMode>,
    );

    await waitFor(() =>
      expect(writeAgentSessionTurnMock).toHaveBeenCalledWith("sess-1", "Usa rápido", [], {
        speed: "fast",
      }),
    );
  });

  it("pauses messages queued after interrupt and keeps the control single-flight", async () => {
    const user = userEvent.setup();
    let resolveInterrupt: (() => void) | undefined;
    interruptAgentSessionTurnMock.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveInterrupt = resolve;
        }),
    );
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({ turn_status: "working", runtime_options: { speed: "standard" } }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const interrupt = await screen.findByRole("button", { name: "Detener respuesta" });
    await user.click(interrupt);
    expect(interrupt).toBeDisabled();
    expect(interruptAgentSessionTurnMock).toHaveBeenCalledTimes(1);

    const composer = screen.getByLabelText("Mensaje para Codex");
    await user.type(composer, "Después de cancelar");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    expect(screen.getByText("Cola pausada")).toBeInTheDocument();

    resolveInterrupt?.();
    await act(async () => {});
    expect(interrupt).toBeDisabled();
    act(() => {
      agentSessionStore.upsertSession(
        sessionFixture({ turn_status: "waiting", runtime_options: { speed: "standard" } }),
      );
    });
    expect(writeAgentSessionTurnMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Reanudar cola" })).toBeEnabled();
  });

  it("reports when the runtime cannot interrupt only the active response", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ turn_status: "working", turn_interrupt_supported: false }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const interrupt = await screen.findByRole("button", { name: "Detener respuesta" });
    expect(interrupt).toBeDisabled();
    expect(interrupt).toHaveAttribute(
      "title",
      "Este runtime no admite interrumpir solo la respuesta",
    );
  });

  it("keeps an explicit model that is not present in the current catalog", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ runtime_options: { model: "future-provider-model", speed: "fast" } }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const modelTrigger = await screen.findByRole("button", { name: /Configuración guardada/ });
    await user.click(modelTrigger);
    await user.click(screen.getByRole("button", { name: /Configurar ejecuci/ }));
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

    const modelTrigger = await screen.findByRole("button", { name: /Configuración guardada/ });
    await user.click(modelTrigger);
    const newPreset = await screen.findByRole("button", { name: /Nuevo/ });
    await waitFor(() => expect(newPreset).toHaveFocus());
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

    await user.click(await screen.findByRole("button", { name: /Configuración guardada/ }));
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

  it("keeps a failed turn draft and hides technical details from the alert", async () => {
    const user = userEvent.setup();
    const writeError = new Error("Write failed");
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    writeAgentSessionInputMock.mockRejectedValueOnce(writeError);
    const { unmount } = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    const errorBanner = await screen.findByTestId("terminal-panel-error");
    expect(errorBanner).toHaveAttribute("role", "alert");
    expect(errorBanner).toHaveTextContent(
      "El mensaje no se envió. Tu borrador sigue aquí; vuelve a intentarlo cuando la sesión esté disponible.",
    );
    expect(errorBanner).not.toHaveTextContent("Write failed");
    expect(errorBanner).not.toHaveAttribute("title");
    expect(composer).toHaveValue("implementa la vista");
    expect(consoleError).toHaveBeenCalledWith("tinto: agent action failed", writeError);
    unmount();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    expect(await screen.findByLabelText("Mensaje para Codex")).toHaveValue("implementa la vista");
    consoleError.mockRestore();
  });

  it("keeps an in-process draft and attachment only for the same session", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture(), sessionFixture({ id: "sess-2" })]);
    openMock.mockResolvedValueOnce("C:\\Temp\\context.txt");
    const first = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    await user.type(await screen.findByLabelText("Mensaje para Codex"), "Conserva este contexto");
    await user.click(screen.getByRole("button", { name: "Adjuntar archivos" }));
    expect(await screen.findByLabelText("Archivos adjuntos")).toHaveTextContent("context.txt");
    first.unmount();

    const restored = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );
    expect(await screen.findByLabelText("Mensaje para Codex")).toHaveValue(
      "Conserva este contexto",
    );
    expect(screen.getByLabelText("Archivos adjuntos")).toHaveTextContent("context.txt");
    restored.unmount();

    render(<TerminalPanel {...props({ sessionId: "sess-2", repo: "/r/a", agentType: "codex" })} />);
    expect(await screen.findByLabelText("Mensaje para Codex")).toHaveValue("");
    expect(screen.queryByLabelText("Archivos adjuntos")).not.toBeInTheDocument();
  });

  it("clears the in-process draft after a confirmed send", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    const first = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    await user.type(await screen.findByLabelText("Mensaje para Codex"), "Mensaje enviado");
    await user.click(screen.getByRole("button", { name: "Enviar" }));
    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalled());
    first.unmount();

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    expect(await screen.findByLabelText("Mensaje para Codex")).toHaveValue("");
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

  it("inserts a skill mention after existing text without replacing the message", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "Usa la skill $warden");

    expect(screen.getByRole("listbox", { name: "Comandos del compositor" })).toHaveAttribute(
      "title",
      "Menú de comandos del compositor: 1 comando coincidente con $warden.",
    );
    await user.keyboard("{Enter}");

    expect(composer).toHaveValue("Usa la skill $krt-interface-warden ");
    expect(composer).toHaveFocus();
  });

  it("inserts a slash command after existing text without replacing the message", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Mensaje para Codex");
    await user.type(composer, "Ayúdame con esto /test");

    expect(screen.getByRole("listbox", { name: "Comandos del compositor" })).toHaveAttribute(
      "title",
      "Menú de comandos del compositor: 1 comando coincidente con /test.",
    );
    await user.keyboard("{Enter}");

    expect(composer).toHaveValue(
      "Ayúdame con esto Ejecuta la verificación más relevante para este repositorio y resume los fallos antes de corregirlos.",
    );
    expect(composer).toHaveFocus();
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
        message: "Objetivo activo: Build the host harness",
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
    expect(await screen.findByText("Objetivo activo: Build the host harness")).toBeInTheDocument();
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
  }, 10_000);

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
      message: "Objetivo activo: Build the host harness",
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

  it("lets the user edit the active goal from the context strip", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({
        goal: {
          text: "Ship the host harness",
          status: "active",
          token_budget: null,
          tokens_used: 0,
          time_used_seconds: 0,
          created_at_ms: 4,
          updated_at_ms: 4,
        },
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(
      await screen.findByRole("button", { name: "Editar objetivo: Ship the host harness" }),
    );

    expect(screen.getByLabelText("Mensaje para Codex")).toHaveValue("/goal Ship the host harness");
    expect(screen.getByLabelText("Mensaje para Codex")).toHaveFocus();
  });

  it("pauses and resumes the native goal from the progress bar", async () => {
    const user = userEvent.setup();
    const activeGoal = {
      text: "Ship the host harness",
      status: "active" as const,
      token_budget: 200_000,
      tokens_used: 45_000,
      time_used_seconds: 321,
      created_at_ms: 1,
      updated_at_ms: 4,
    };
    listAgentSessionsMock.mockResolvedValue([sessionFixture({ goal: activeGoal })]);
    const view = render(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
    );

    await user.click(await screen.findByRole("button", { name: "Pausar" }));
    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "goal", "pause"),
    );

    view.unmount();
    runAgentHostCommandMock.mockClear();
    listAgentSessionsMock.mockResolvedValue([
      sessionFixture({ goal: { ...activeGoal, status: "paused" } }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Reanudar" }));
    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "goal", "resume"),
    );
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
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:4",
        kind: "agent_message",
        text: "Ya quedó verificado",
        timestamp_ms: 126000,
      });
    });

    const conversation = screen.getByLabelText("Conversación con Agent");
    expect(await within(conversation).findByText("Tú")).toBeInTheDocument();
    expect(within(conversation).getByText("Turno 1")).toBeInTheDocument();
    expect(within(conversation).getByText("Haz el cambio")).toBeInTheDocument();
    expect(within(conversation).getAllByText("Agent")).toHaveLength(2);
    expect(within(conversation).getByText("Voy con ello")).toBeInTheDocument();
    expect(within(conversation).getByText("Comando")).toBeInTheDocument();
    expect(within(conversation).getAllByText("cargo test").length).toBeGreaterThan(0);
    expect(within(conversation).getByText("4 mensajes / 1 comando")).toBeInTheDocument();
    const orderedMessages = [...conversation.querySelectorAll(".agent-panel__message")].map(
      (element) => element.textContent,
    );
    expect(orderedMessages[0]).toContain("Haz el cambio");
    expect(orderedMessages[1]).toContain("Voy con ello");
    expect(orderedMessages[2]).toContain("cargo test");
    expect(orderedMessages[3]).toContain("Ya quedó verificado");

    const overview = screen.getByLabelText("Resumen de la sesión de Agent");
    const turnsMetric = within(overview).getByLabelText("Turnos: 1");
    expect(turnsMetric).toHaveTextContent("1");
    expect(turnsMetric).toHaveTextContent("Turnos");
    const messagesMetric = within(overview).getByLabelText("Mensajes: 3");
    expect(messagesMetric).toHaveTextContent("3");
    expect(messagesMetric).toHaveTextContent("Mensajes");
    const commandsMetric = within(overview).getByLabelText("Comandos: 1");
    expect(commandsMetric).toHaveTextContent("1");
    expect(commandsMetric).toHaveTextContent("Comandos");
    const filesMetric = within(overview).getByLabelText("Archivos: 0");
    expect(filesMetric).toHaveTextContent("0");
    expect(filesMetric).toHaveTextContent("Archivos");
    expect(within(overview).getByText("Actividad reciente")).toBeInTheDocument();
    expect(within(overview).getByText("Ya quedó verificado")).toBeInTheDocument();
    expect(within(overview).getByText("+0s")).toBeInTheDocument();
    const turnMap = screen.getByLabelText("Mapa de turnos");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });
    expect(firstTurnButton).toHaveTextContent("T1");
    expect(firstTurnButton).toHaveTextContent("1 cmd");
    expect(firstTurnButton).toHaveTextContent("cargo test");

    await user.click(firstTurnButton);
    expect(scrollIntoViewMock).toHaveBeenCalledWith({ block: "start", behavior: "smooth" });
  });

  it("renders image attachments as thumbnails inside the sent message", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:attachment",
        kind: "user_message",
        text: "Mira este error",
        timestamp_ms: 1000,
        attachments: [{ path: "C:\\temp\\captura.png", name: "captura.png", is_image: true }],
      });
    });

    const preview = await screen.findByRole("img", { name: "Vista previa de captura.png" });
    expect(preview).toHaveAttribute("src", "data:image/png;base64,cHJldmlldw==");
    expect(getAgentImagePreviewMock).toHaveBeenCalledWith("C:\\temp\\captura.png");
  });

  it("edits a sent message inline and continues in a branched session", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    listAgentSessionsMock
      .mockResolvedValueOnce([
        sessionFixture({
          status: "exited",
          pid: null,
          ended_at_ms: 2500,
          exit_code: 0,
          turn_status: "waiting",
          active_sessions: 0,
        }),
      ])
      .mockResolvedValueOnce([sessionFixture({ id: "sess-edited" })]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openAgentTerminal },
    );

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Primer mensaje",
        timestamp_ms: 1000,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:1",
        kind: "agent_message",
        text: "Primera respuesta",
        timestamp_ms: 1100,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:2",
        kind: "user_message",
        text: "Último mensaje",
        timestamp_ms: 2000,
      });
    });

    expect(await screen.findAllByRole("button", { name: /Editar mensaje del turno/ })).toHaveLength(
      2,
    );
    await user.click(screen.getByRole("button", { name: "Editar mensaje del turno 1" }));
    const inlineEditor = screen.getByRole("textbox", { name: "Editar mensaje del turno 1" });
    expect(inlineEditor).toHaveValue("Primer mensaje");
    expect(screen.queryByLabelText("Mensaje para Codex")).not.toBeVisible();
    await user.clear(inlineEditor);
    await user.type(inlineEditor, "Primer mensaje corregido");
    await user.click(screen.getByRole("button", { name: "Crear rama" }));

    await waitFor(() =>
      expect(branchAgentSessionFromMessageMock).toHaveBeenCalledWith("sess-1", "sess-1:user:1"),
    );
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith(
      "sess-edited",
      "Primer mensaje corregido\r",
      expect.any(Object),
    );
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-edited",
      repo: "/r/a",
      agentType: "codex",
    });
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
    expect(copyAgent).not.toHaveAttribute("title");
    await user.click(copyAgent);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith("Voy con ello"));
    expect(await screen.findByLabelText("Copiar mensaje de Agent")).toHaveTextContent("Copiado");
    expect(screen.getByLabelText("Copiar mensaje de Agent")).not.toHaveAttribute("title");
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
    expect(copyTurn).not.toHaveAttribute("title");
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
    expect(screen.getByRole("button", { name: "Copiado" })).not.toHaveAttribute("title");
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
    expect(
      within(conversation)
        .getAllByText("npm test -- --run")
        .some((element) => element.tagName === "PRE"),
    ).toBe(true);
  });

  it("reconstructs streamed agent markdown without inserting spaces between deltas", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:1",
        kind: "user_message",
        text: "Explain this",
        timestamp_ms: 1,
      });
      for (const [index, text] of ["**Git", "Nexus**", ": ready"].entries()) {
        agentSessionStore.appendTimelineItem({
          session_id: "sess-1",
          id: `sess-1:agent:${index}`,
          kind: "agent_message",
          text,
          timestamp_ms: 2 + index,
        });
      }
    });

    const rendered = await screen.findByText("GitNexus");
    expect(rendered.tagName).toBe("STRONG");
    expect(rendered.closest("p")).toHaveTextContent("GitNexus: ready");
  });

  it("preserves whitespace carried by streamed agent deltas", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:spacing",
        kind: "user_message",
        text: "Explain the architecture",
        timestamp_ms: 1,
      });
      for (const [index, text] of [
        "Voy",
        " a comprobar",
        " las fuentes",
        " del repositorio.",
      ].entries()) {
        agentSessionStore.appendTimelineItem({
          session_id: "sess-1",
          id: `sess-1:agent:spacing:${index}`,
          kind: "agent_message",
          text,
          timestamp_ms: 2 + index,
        });
      }
    });

    const conversation = screen.getByRole("log", { name: "Conversación con Agent" });
    expect(
      await within(conversation).findByText("Voy a comprobar las fuentes del repositorio."),
    ).toBeInTheDocument();
  });

  it("replaces the final progress message with the authoritative completed answer", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:authoritative",
        kind: "user_message",
        text: "Describe el proyecto",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:progress:authoritative",
        kind: "agent_progress",
        text: "**Arquitectura** Es una aplicación Tauri 2.",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:authoritative",
        kind: "agent_message",
        text: "**Arquitectura** Es una aplicación Tauri 2.",
        timestamp_ms: 3,
      });
    });

    const conversation = screen.getByRole("log", { name: "Conversación con Agent" });
    expect(await within(conversation).findByText("Arquitectura")).toHaveTextContent("Arquitectura");
    expect(within(conversation).getAllByText(/Es una aplicación Tauri 2/)).toHaveLength(1);
  });

  it("shows reasoning while active, groups commands, and collapses it after the final answer", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture({ turn_status: "working" })]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:thought",
        kind: "user_message",
        text: "Revisa el proyecto",
        timestamp_ms: 1,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:progress:thought",
        kind: "agent_progress",
        text: "Analizando la estructura",
        timestamp_ms: 2,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:activity:thought",
        kind: "activity",
        text: "Leyendo package.json",
        timestamp_ms: 3,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:activity:thought:command:1",
        kind: "activity",
        text: "Ejecutando npm test",
        timestamp_ms: 4,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:thought:1",
        kind: "command_output",
        text: "primera suite correcta",
        timestamp_ms: 5,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:activity:thought:command:2",
        kind: "activity",
        text: "Ejecutando npm run build",
        timestamp_ms: 6,
      });
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:command:thought:2",
        kind: "command_output",
        text: "build correcto",
        timestamp_ms: 7,
      });
    });

    const activeSummary = await screen.findByText("Actividad en curso");
    const disclosure = activeSummary.closest("details");
    expect(disclosure).not.toHaveAttribute("open");
    expect(
      within(disclosure!).getByText("Ejecutando npm run build", {
        selector: ".agent-panel__thought-summary",
      }),
    ).toBeInTheDocument();
    const commandGroups = within(disclosure!).getAllByRole("group", {
      name: "1 comando ejecutado",
      hidden: true,
    });
    expect(commandGroups).toHaveLength(2);
    expect(commandGroups.every((group) => !group.hasAttribute("open"))).toBe(true);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:agent:thought",
        kind: "agent_message",
        text: "La revisión ha terminado.",
        timestamp_ms: 8,
      });
    });

    const summary = await screen.findByText("Actividad");
    expect(disclosure).not.toHaveAttribute("open");
    expect(screen.getAllByText("La revisión ha terminado.")[0]).toBeVisible();
    expect(document.querySelector(".agent-panel__message-activity-signal")).toBeNull();
    expect(screen.getByLabelText("Conversación con Agent")).not.toHaveAttribute("title");

    await user.click(summary.closest("summary") as HTMLElement);
    expect(disclosure).toHaveAttribute("open");
    expect(screen.getByText("Analizando la estructura")).toBeInTheDocument();
    expect(screen.getAllByText("Leyendo package.json").length).toBeGreaterThan(0);
  }, 10_000);

  it("follows streamed conversation content until the user scrolls away from the bottom", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    const conversation = await screen.findByRole("log", { name: "Conversación con Agent" });
    Object.defineProperties(conversation, {
      clientHeight: { configurable: true, value: 400 },
      scrollHeight: { configurable: true, value: 1000 },
      scrollTop: { configurable: true, value: 600, writable: true },
    });
    fireEvent.scroll(conversation);

    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:user:auto-follow",
        kind: "user_message",
        text: "Sigue abajo",
        timestamp_ms: 1,
      });
    });
    await waitFor(() => expect(conversation.scrollTop).toBe(1000));

    conversation.scrollTop = 120;
    fireEvent.scroll(conversation);
    await act(async () => {
      agentSessionStore.appendTimelineItem({
        session_id: "sess-1",
        id: "sess-1:activity:auto-follow",
        kind: "activity",
        text: "Leyendo src/App.tsx",
        timestamp_ms: 2,
      });
    });

    expect(conversation.scrollTop).toBe(120);
  });

  it("clears the process indicator when the completed session snapshot reaches the UI", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture({ turn_status: "working" })]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(
      await screen.findByRole("status", { name: "Codex está trabajando" }),
    ).toBeInTheDocument();
    await act(async () => {
      agentSessionStore.upsertSession(sessionFixture({ turn_status: "waiting" }));
    });

    expect(screen.queryByRole("status", { name: /Codex está/ })).toBeNull();
  });

  it("attaches images and generic files to a Codex turn and keeps them removable", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    openMock.mockResolvedValueOnce([
      "C:\\Temp\\screen.png",
      "C:\\Temp\\before.jpg",
      "C:\\Temp\\brief.pdf",
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Adjuntar archivos" }));
    expect(screen.getByLabelText("Archivos adjuntos")).toHaveTextContent("screen.png");
    expect(screen.getByLabelText("Archivos adjuntos")).toHaveTextContent("brief.pdf");
    expect(getAgentImagePreviewMock).toHaveBeenCalledTimes(2);
    await user.click(screen.getByRole("button", { name: "Quitar before.jpg" }));
    await user.type(screen.getByLabelText("Mensaje para Codex"), "Revisa estos archivos");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() =>
      expect(writeAgentSessionTurnMock).toHaveBeenCalledWith(
        "sess-1",
        "Revisa estos archivos",
        ["C:\\Temp\\screen.png", "C:\\Temp\\brief.pdf"],
        expect.any(Object),
      ),
    );
    expect(screen.queryByLabelText("Archivos adjuntos")).not.toBeInTheDocument();
  });

  it("can send a Codex turn containing only an attached file", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    openMock.mockResolvedValueOnce("C:\\Temp\\requirements.docx");
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Adjuntar archivos" }));
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() =>
      expect(writeAgentSessionTurnMock).toHaveBeenCalledWith(
        "sess-1",
        "",
        ["C:\\Temp\\requirements.docx"],
        expect.any(Object),
      ),
    );
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
    expect(toggle.closest("summary")).not.toHaveAttribute("title");
    expect(within(conversation).getAllByText("npm test -- --run").length).toBeGreaterThan(0);
    const collapsedCommandBlock = toggle.closest("details");
    expect(collapsedCommandBlock).not.toHaveAttribute("title");
    expect(collapsedCommandBlock).not.toHaveAttribute("open");
    expect(within(conversation).getAllByText("npm test -- --run").length).toBeGreaterThan(0);
    const collapsedOutput = collapsedCommandBlock?.querySelector("pre");
    expect(collapsedOutput).not.toBeNull();
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
    expect(search).not.toHaveAttribute("title");
    expect(
      screen.getByText(
        "Pulsa Intro para recorrer los turnos coincidentes y Escape para borrar la búsqueda.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Resultado anterior" })).not.toHaveAttribute("title");
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).not.toHaveAttribute(
      "title",
    );

    await user.type(search, "dashboard");
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).queryByText("Segunda tarea")).not.toBeInTheDocument();
    expect(screen.getByText("1 de 2 turnos")).toBeInTheDocument();
    expect(screen.getByLabelText("1 turno coincidentes de 2 en total.")).not.toHaveAttribute(
      "title",
    );
    expect(screen.getByRole("button", { name: "Resultado anterior" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).toBeDisabled();
    const messageMatch = within(conversation).getByLabelText(
      "Coincidencias de búsqueda del turno 1",
    );
    expect(messageMatch).not.toHaveAttribute("title");
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
    expect(latestButton).not.toHaveAttribute("title");

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
    expect(screen.getByRole("button", { name: "Resultado anterior" })).not.toHaveAttribute("title");
    expect(screen.getByRole("button", { name: "Resultado siguiente" })).not.toHaveAttribute(
      "title",
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
    expect(within(secondaryActions).getByRole("button", { name: "Último" })).not.toHaveAttribute(
      "title",
    );
    expect(
      within(secondaryActions).getByRole("button", { name: "Copiar lo visible" }),
    ).not.toHaveAttribute("title");
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
    expect(screen.getByRole("button", { name: "Último" })).not.toHaveAttribute("title");
    const copyVisible = screen.getByRole("button", { name: "Copiar lo visible" });
    expect(copyVisible).not.toHaveAttribute("title");
    await user.click(copyVisible);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Dashboard task");
    expect(copied).toContain("Finished dashboard work");
    expect(copied).not.toContain("Search task");
    expect(screen.getByRole("button", { name: "Copiado" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copiado" })).not.toHaveAttribute("title");
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
    expect(screen.getByPlaceholderText("Continúa esta conversación")).toBeEnabled();
    expect(screen.getByPlaceholderText("Continúa esta conversación")).toHaveAttribute(
      "title",
      "Entrada de mensajes de Codex para a: el próximo mensaje retomará la conversación archivada.",
    );
    const archivedComposer = screen.getByPlaceholderText("Continúa esta conversación");
    const archivedComposerHint = archivedComposer.getAttribute("aria-describedby");
    expect(archivedComposerHint).toBeTruthy();
    expect(document.getElementById(archivedComposerHint!)).toHaveTextContent(
      "Escribe un mensaje para retomar esta conversación archivada.",
    );
    expect(
      screen.queryByRole("listbox", { name: "Comandos del compositor" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plan" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enviar" })).toHaveAttribute(
      "title",
      "Enviar mensaje a Codex para a: escribe un mensaje para retomar la conversación archivada.",
    );
    expect(screen.queryByRole("button", { name: "Retomar conversación" })).not.toBeInTheDocument();
    const archivedFocus = screen.getByLabelText("Turno seleccionado");
    expect(
      within(archivedFocus).getByTitle(
        "Contenedor de restauración del turno seleccionado 1: detén la sesión antes de restaurar.",
      ),
    ).toHaveClass("agent-panel__turn-focus-actions");
    expect(
      within(archivedFocus).getByRole("button", { name: "Restaurar desde este turno" }),
    ).toHaveAttribute("title", "Restaurar el turno 1: detén la sesión antes.");

    unmount();
    expect(listAgentSessionsMock).not.toHaveBeenCalled();
    expect(stopAgentSessionMock).not.toHaveBeenCalled();
  });

  it("opens a resumed journal conversation once and keeps its pending panel usable", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    const refresh = deferred<AgentSession[]>();
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({ status: "completed", pid: null, checkpoint: null }),
    );
    listAgentSessionsMock.mockReturnValueOnce(refresh.promise);
    const archivedView = renderWithWorkspaceActions(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
      { openAgentTerminal },
    );

    await user.type(await screen.findByPlaceholderText("Continúa esta conversación"), "Seguimos");
    expect(await screen.findByRole("button", { name: /Configuración guardada/ })).toBeEnabled();
    const fast = screen.getByRole("button", { name: "Rápido" });
    await user.click(fast);
    expect(fast).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(resumeAgentJournalSessionMock).toHaveBeenCalledWith("sess-1"));
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-resumed", "Seguimos\r", {
      speed: "fast",
    });
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-resumed",
      repo: "/r/a",
      agentType: "codex",
      replaceSessionId: "sess-1",
    });
    expect(openAgentTerminal).toHaveBeenCalledTimes(1);
    expect(agentSessionStore.getState().sessions["sess-resumed"]).toMatchObject({
      id: "sess-resumed",
      status: "starting",
      turn_status: "waiting",
    });
    expect(screen.getByPlaceholderText("Continúa esta conversación")).toBeEnabled();
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();

    archivedView.unmount();
    render(
      <TerminalPanel {...props({ sessionId: "sess-resumed", repo: "/r/a", agentType: "codex" })} />,
    );

    expect(await screen.findByText("Iniciando")).toBeInTheDocument();
    expect(screen.getByLabelText("Mensaje para Codex")).toBeEnabled();
    expect(listAgentSessionsMock).toHaveBeenCalledTimes(1);
    expect(openAgentTerminal).toHaveBeenCalledTimes(1);

    await act(async () =>
      refresh.resolve([sessionFixture({ id: "sess-resumed", status: "running" })]),
    );
    await waitFor(() =>
      expect(agentSessionStore.getState().sessions["sess-resumed"]?.pid).toBe(123),
    );
  });

  it("retries a resumed conversation without creating a second session", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({ status: "completed", pid: null, checkpoint: null }),
    );
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ id: "sess-resumed", status: "running" }),
    ]);
    writeAgentSessionInputMock
      .mockRejectedValueOnce({ category: "session_not_running", message: "starting" })
      .mockResolvedValueOnce(undefined);
    renderWithWorkspaceActions(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
      { openAgentTerminal },
    );

    const composer = await screen.findByPlaceholderText("Continúa esta conversación");
    await user.type(composer, "Seguimos");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalledTimes(2));
    expect(resumeAgentJournalSessionMock).toHaveBeenCalledTimes(1);
    expect(writeAgentSessionInputMock).toHaveBeenLastCalledWith("sess-resumed", "Seguimos\r", {
      speed: "standard",
    });
    expect(openAgentTerminal).toHaveBeenCalledTimes(1);
    expect(composer).toHaveValue("");
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
  });

  it("creates a fresh resumed session when the first one stops before the message is sent", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({ status: "completed", pid: null, checkpoint: null }),
    );
    resumeAgentJournalSessionMock
      .mockResolvedValueOnce({ session_id: "sess-stopped", mode: "native" })
      .mockResolvedValueOnce({ session_id: "sess-retried", mode: "native" });
    writeAgentSessionInputMock
      .mockRejectedValueOnce({ category: "session_not_found", message: "stopped" })
      .mockResolvedValueOnce(undefined);
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ id: "sess-retried", status: "running" }),
    ]);
    renderWithWorkspaceActions(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
      { openAgentTerminal },
    );

    const composer = await screen.findByPlaceholderText("Continúa esta conversación");
    await user.type(composer, "Seguimos aunque se detenga");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalledTimes(2));
    expect(resumeAgentJournalSessionMock).toHaveBeenCalledTimes(2);
    expect(writeAgentSessionInputMock.mock.calls.map(([id]) => id)).toEqual([
      "sess-stopped",
      "sess-retried",
    ]);
    expect(openAgentTerminal).toHaveBeenCalledWith({
      sessionId: "sess-retried",
      repo: "/r/a",
      agentType: "codex",
      replaceSessionId: "sess-1",
    });
    expect(composer).toHaveValue("");
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
  });

  it("shows a resumed-session refresh error and confirms it on retry", async () => {
    const user = userEvent.setup();
    const openAgentTerminal = vi.fn();
    const retry = deferred<AgentSession[]>();
    const technicalError = new Error(
      "thread panicked while reading the manifest from staging backup at root del repo",
    );
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({ status: "completed", pid: null, checkpoint: null }),
    );
    listAgentSessionsMock.mockRejectedValueOnce(technicalError);
    const archivedView = renderWithWorkspaceActions(
      <TerminalPanel
        {...props({
          sessionId: "sess-1",
          repo: "/r/a",
          agentType: "codex",
          mode: "journal",
        })}
      />,
      { openAgentTerminal },
    );

    const composer = await screen.findByPlaceholderText("Continúa esta conversación");
    await user.type(composer, "Seguimos");
    await user.click(screen.getByRole("button", { name: "Enviar" }));

    await waitFor(() =>
      expect(openAgentTerminal).toHaveBeenCalledWith({
        sessionId: "sess-resumed",
        repo: "/r/a",
        agentType: "codex",
        replaceSessionId: "sess-1",
      }),
    );
    await waitFor(() => expect(listAgentSessionsMock).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
    expect(composer).toHaveValue("");
    expect(composer).toBeEnabled();
    expect(agentSessionStore.getState().sessions["sess-resumed"]).toMatchObject({
      id: "sess-resumed",
      status: "starting",
      turn_status: "waiting",
    });
    expect(resumeAgentJournalSessionMock).toHaveBeenCalledTimes(1);
    expect(writeAgentSessionInputMock).toHaveBeenCalledTimes(1);
    expect(consoleError).toHaveBeenCalledWith(
      "tinto: resumed Agent session refresh failed",
      technicalError,
    );

    listAgentSessionsMock.mockReturnValueOnce(retry.promise);
    archivedView.unmount();
    render(
      <TerminalPanel {...props({ sessionId: "sess-resumed", repo: "/r/a", agentType: "codex" })} />,
    );

    const errorBanner = await screen.findByTestId("terminal-panel-error");
    expect(errorBanner).toHaveTextContent(
      "No se pudo confirmar la conversación retomada. El mensaje se envió y la conversación sigue disponible.",
    );
    const liveComposer = screen.getByLabelText("Mensaje para Codex");
    expect(liveComposer).toBeEnabled();
    await user.click(within(errorBanner).getByRole("button", { name: "Reintentar" }));
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
    expect(liveComposer).toBeEnabled();

    await act(async () =>
      retry.resolve([
        sessionFixture({ id: "sess-resumed", status: "running", pid: 456, turn_status: "working" }),
      ]),
    );
    await waitFor(() =>
      expect(agentSessionStore.getState().sessions["sess-resumed"]).toMatchObject({
        status: "running",
        pid: 456,
        turn_status: "working",
      }),
    );
    expect(screen.queryByTestId("terminal-panel-error")).not.toBeInTheDocument();
    expect(listAgentSessionsMock).toHaveBeenCalledTimes(2);
    expect(openAgentTerminal).toHaveBeenCalledTimes(1);
    consoleError.mockRestore();
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
    const user = userEvent.setup();
    const openFile = vi.fn();
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
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openFile },
    );

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
    expect(screen.getByRole("button", { name: "Abrir diff de src/a.ts" })).not.toHaveAttribute(
      "title",
    );
    expect(screen.getByText("1 archivo modificado")).not.toHaveAttribute("title");
    expect(screen.getByLabelText("Archivos: 1")).toBeInTheDocument();
    expect(screen.getByTitle("Turno 1: 0 comandos, 1 archivo")).toBeInTheDocument();
    const changedFile = screen.getByRole("button", { name: "Abrir diff de src/a.ts" });
    expect(changedFile).toHaveTextContent("Msrc/a.ts");
    expect(changedFile).not.toHaveTextContent("Diff");

    await user.click(changedFile);

    expect(openFile).toHaveBeenCalledWith("/r/a", "src/a.ts", true);
  });

  it("opens repository paths from Markdown links without navigating the app", async () => {
    const openFile = vi.fn();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        turn_checkpoints: [
          {
            id: "link-turn",
            index: 1,
            started_at_ms: 1,
            ended_at_ms: 2,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/control.ts", kind: "modified", timestamp_ms: 2 }],
          },
        ],
        timeline: [
          {
            session_id: "sess-1",
            id: "user-link",
            kind: "user_message",
            text: "¿Dónde está?",
            timestamp_ms: 1,
          },
          {
            session_id: "sess-1",
            id: "agent-link",
            kind: "agent_message",
            text: "Está en [TerminalPanel.tsx](src/panels/terminal/TerminalPanel.tsx#L42).",
            timestamp_ms: 2,
          },
        ],
      }),
    ]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openFile },
    );

    await screen.findByRole("button", { name: "TerminalPanel.tsx" });
    fireEvent.click(screen.getByRole("button", { name: "Abrir diff de src/control.ts" }));
    expect(openFile).toHaveBeenCalledWith("/r/a", "src/control.ts", true);
    openFile.mockClear();
    const link = screen.getByRole("button", { name: "TerminalPanel.tsx" });
    expect(link).toHaveAttribute("data-repo-path", "src/panels/terminal/TerminalPanel.tsx");
    fireEvent.click(link);

    expect(openFile).toHaveBeenCalledWith("/r/a", "src/panels/terminal/TerminalPanel.tsx", true);
    expect(openFile).toHaveBeenCalledTimes(1);
  });

  it("offers accessible file actions from Markdown links", async () => {
    const user = userEvent.setup();
    const openFile = vi.fn();
    installClipboardMock();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        timeline: [
          {
            session_id: "sess-1",
            id: "agent-file-actions",
            kind: "agent_message",
            text: "Revisa [TerminalPanel.tsx](src/panels/terminal/TerminalPanel.tsx).",
            timestamp_ms: 2,
          },
        ],
      }),
    ]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />,
      { openFile },
    );

    await screen.findByRole("button", { name: "TerminalPanel.tsx" });
    const actions = screen.getByRole("button", {
      name: "Acciones para src/panels/terminal/TerminalPanel.tsx",
    });
    actions.focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("menu", { name: /Acciones para src\/panels/ })).toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Copiar ruta" }));
    expect(writeClipboardTextMock).toHaveBeenCalledWith("src/panels/terminal/TerminalPanel.tsx");

    await user.click(actions);
    await user.click(screen.getByRole("menuitem", { name: "Abrir archivo" }));
    expect(openFile).toHaveBeenCalledWith("/r/a", "src/panels/terminal/TerminalPanel.tsx", true);
  });

  it("opens encoded absolute Windows Markdown links inside the repository dock", async () => {
    const openFile = vi.fn();
    const repo = "C:\\Users\\User\\Documents\\personal\\tinto";
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        repo,
        timeline: [
          {
            session_id: "sess-1",
            id: "user-absolute-link",
            kind: "user_message",
            text: "Abre el protocolo",
            timestamp_ms: 1,
          },
          {
            session_id: "sess-1",
            id: "agent-absolute-link",
            kind: "agent_message",
            text: "[protocolo](C:%5CUsers%5CUser%5CDocuments%5Cpersonal%5Ctinto%5Cdocs%5Cprotocol.md:12)",
            timestamp_ms: 2,
          },
        ],
      }),
    ]);
    renderWithWorkspaceActions(
      <TerminalPanel {...props({ sessionId: "sess-1", repo, agentType: "codex" })} />,
      { openFile },
    );

    const conversation = await screen.findByRole("log", { name: "Conversación con Agent" });
    const link = within(conversation).getByRole("button", { name: "protocolo" });
    expect(link).toHaveAttribute(
      "data-repo-path",
      "C:/Users/User/Documents/personal/tinto/docs/protocol.md:12",
    );
    fireEvent.click(link);

    expect(openFile).toHaveBeenCalledWith(repo, "docs/protocol.md", true);
    expect(link).not.toHaveAttribute("href");
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
        turn_status: "working",
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

  it("aligns legacy checkpoint changes with the focused turn by timestamp", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [],
        turn_checkpoints: [
          {
            id: "sess-1:turn-1",
            index: 1,
            started_at_ms: 30,
            ended_at_ms: 31,
            checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
            changes: [{ path: "src/third.ts", kind: "modified", timestamp_ms: 31 }],
          },
        ],
      }),
    ]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await act(async () => {
      for (const [index, timestamp] of [10, 20, 30].entries()) {
        agentSessionStore.appendTimelineItem({
          session_id: "sess-1",
          id: `sess-1:user:${index + 1}`,
          kind: "user_message",
          text: `Task ${index + 1}`,
          timestamp_ms: timestamp,
        });
      }
    });

    const lens = await screen.findByLabelText("Agent Lens");
    expect(within(lens).getByText("Turno 3")).toBeInTheDocument();
    expect(within(lens).getAllByText("src/third.ts").length).toBeGreaterThan(0);
    expect(
      within(lens).getByText("1", { selector: ".agent-panel__lens-metrics span" }),
    ).toBeInTheDocument();
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
    expect(within(previewActions).getByRole("button", { name: "Revertir archivo" })).toBeDisabled();
    const fileActions = within(lens).getByTitle(
      "Acciones de Agent Lens para src/agent-view.tsx: vista previa, abrir, preguntar y revertir el archivo.",
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
    expect(within(fileActions).getByRole("button", { name: "Revertir archivo" })).toBeDisabled();
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
    expect(screen.getByRole("button", { name: "Detalles" })).not.toHaveAttribute("title");
    await user.click(screen.getByRole("button", { name: "Detalles" }));
    expect(
      screen.getByTitle(
        "Detalles de la sesión: mapa de turnos, actividad actual, puntos de restauración y Agent Lens.",
      ),
    ).toHaveClass("agent-panel__details-head");
    expect(
      within(focus).getByRole("button", { name: "Restaurar desde este turno" }),
    ).toHaveAttribute("title", "Restaurar el turno 1: detén la sesión antes.");
    expect(screen.getByLabelText("Mensaje para Codex")).toHaveValue("");
  });

  it("shows the safe MCP inventory and project-local profile state from Details", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [{ path: "/r/a", alias: null, fs_watch: [] }],
        },
      ],
    });
    getCodexMcpInventoryMock.mockResolvedValueOnce({
      provider: "codex",
      target: "windows_local",
      status: "success",
      definitions: [
        {
          provider: "codex",
          target: "windows_local",
          source: "codex_mcp_servers",
          name: "same-name",
          command_available: true,
        },
        {
          provider: "codex",
          target: "windows_local",
          source: "codex_mcpServers",
          name: "same-name",
          command_available: null,
        },
      ],
      error: null,
      checked_at_ms: 1,
    });
    listMcpProfilesMock.mockResolvedValueOnce({
      profiles: [{ id: "imported", name: "Imported", definitions: [] }],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    expect(await screen.findByRole("heading", { name: "MCP del proyecto" })).toBeInTheDocument();
    expect(screen.getByText("Disponible")).toBeInTheDocument();
    expect(screen.getAllByText("same-name")).toHaveLength(2);
    expect(screen.getByText("codex_mcp_servers")).toBeInTheDocument();
    expect(screen.getByText("codex_mcpServers")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Perfil MCP predeterminado" })).toHaveValue(
      "imported",
    );
    expect(screen.getByText("Entrega no admitida")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Importar inventario actual" })).toBeEnabled();
  });

  it("binds MCP profiles to the session repository instead of the active workbench", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture({ repo: "/r/a" })]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Other",
      workbenches: [
        { name: "Session project", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] },
        { name: "Other", repos: [{ path: "/r/b", alias: null, fs_watch: [] }] },
      ],
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    await screen.findByRole("heading", { name: "MCP del proyecto" });
    expect(listMcpProfilesMock).toHaveBeenCalledWith("Session project");
    expect(listMcpProfilesMock).not.toHaveBeenCalledWith("Other");
  });

  it("renders a terminal MCP error while preserving a fulfilled profile response", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    getCodexMcpInventoryMock.mockRejectedValueOnce(new Error("unavailable"));
    listMcpProfilesMock.mockResolvedValueOnce({
      profiles: [{ id: "imported", name: "Imported", definitions: [] }],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    expect(await screen.findByText("Error seguro")).toBeInTheDocument();
    expect(screen.queryByText("Cargando")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Perfil MCP predeterminado" })).toHaveValue(
      "imported",
    );
  });

  it("manages a non-default profile without implicitly replacing the default", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    const state: McpProfileState = {
      profiles: [
        { id: "imported", name: "Imported", definitions: [] },
        { id: "review", name: "Review", definitions: [] },
      ],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    };
    listMcpProfilesMock.mockResolvedValueOnce(state);
    renameMcpProfileMock.mockResolvedValueOnce(state);

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Perfil MCP para gestionar" }),
      "review",
    );
    await user.type(screen.getByRole("textbox", { name: "Nombre de perfil MCP" }), "Reviewed");
    await user.click(screen.getByRole("button", { name: "Renombrar" }));

    expect(renameMcpProfileMock).toHaveBeenCalledWith("Work", "review", "Reviewed");
    expect(setMcpDefaultProfileMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Eliminar" })).toBeEnabled();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Perfil MCP para gestionar" }),
      "imported",
    );
    expect(screen.getByRole("button", { name: "Eliminar" })).toBeDisabled();
  });

  it("disables MCP profile selectors in a read-only journal", async () => {
    getAgentJournalSessionMock.mockResolvedValueOnce(
      sessionFixture({ status: "exited", ended_at_ms: 2 }),
    );
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    listMcpProfilesMock.mockResolvedValueOnce({
      profiles: [
        { id: "imported", name: "Imported", definitions: [] },
        { id: "review", name: "Review", definitions: [] },
      ],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
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
    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    expect(
      await screen.findByRole("combobox", { name: "Perfil MCP predeterminado" }),
    ).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Perfil MCP para gestionar" })).toBeDisabled();
    expect(setMcpDefaultProfileMock).not.toHaveBeenCalled();
  });

  it("keeps partial inventory non-importable", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    getCodexMcpInventoryMock.mockResolvedValueOnce({
      provider: "codex",
      target: "windows_local",
      status: "partial",
      definitions: [],
      error: null,
      checked_at_ms: 1,
    });
    listMcpProfilesMock.mockResolvedValueOnce({
      profiles: [{ id: "imported", name: "Imported", definitions: [] }],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    expect(await screen.findByText("Parcial")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Importar inventario actual" })).toBeDisabled();
  });

  it("keeps the last successful MCP definitions visible after a partial refresh", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValue({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    getCodexMcpInventoryMock
      .mockResolvedValueOnce({
        provider: "codex",
        target: "windows_local",
        status: "success",
        definitions: [
          {
            provider: "codex",
            target: "windows_local",
            source: "codex_mcp_servers",
            name: "known-good",
            command_available: true,
          },
        ],
        error: null,
        checked_at_ms: 1,
      })
      .mockResolvedValueOnce({
        provider: "codex",
        target: "windows_local",
        status: "partial",
        definitions: [],
        error: null,
        checked_at_ms: 2,
      });
    listMcpProfilesMock.mockResolvedValue({
      profiles: [{ id: "imported", name: "Imported", definitions: [] }],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));
    expect(await screen.findByText("known-good")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Actualizar inventario MCP" }));

    expect(await screen.findByText("Parcial")).toBeInTheDocument();
    expect(screen.getByText("known-good")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Importar inventario actual" })).toBeDisabled();
  });

  it("keeps an authoritative empty MCP catalog empty after a partial refresh", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValue({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
    });
    getCodexMcpInventoryMock
      .mockResolvedValueOnce({
        provider: "codex",
        target: "windows_local",
        status: "empty",
        definitions: [],
        error: null,
        checked_at_ms: 1,
      })
      .mockResolvedValueOnce({
        provider: "codex",
        target: "windows_local",
        status: "partial",
        definitions: [
          {
            provider: "codex",
            target: "windows_local",
            source: "codex_mcp_servers",
            name: "incomplete",
            command_available: true,
          },
        ],
        error: null,
        checked_at_ms: 2,
      });
    listMcpProfilesMock.mockResolvedValue({
      profiles: [{ id: "imported", name: "Imported", definitions: [] }],
      active_profile_id: "imported",
      delivery_status: "unsupported",
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));
    expect(await screen.findByText("Vacío")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Actualizar inventario MCP" }));

    expect(await screen.findByText("Parcial")).toBeInTheDocument();
    expect(screen.queryByText("incomplete")).not.toBeInTheDocument();
    expect(
      screen.getByText("No hay definiciones MCP no sensibles disponibles."),
    ).toBeInTheDocument();
  });

  it("reports MCP as unavailable when the session repository has no unique project", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    listWorkbenchesMock.mockResolvedValueOnce({
      version: 1,
      active: "Other",
      workbenches: [{ name: "Other", repos: [{ path: "/r/b", alias: null, fs_watch: [] }] }],
    });

    const user = userEvent.setup();
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);
    await user.click(await screen.findByRole("button", { name: "Detalles" }));

    expect(await screen.findByText("No admitido")).toBeInTheDocument();
    expect(
      screen.getByText("La sesión no pertenece a un proyecto local disponible."),
    ).toBeInTheDocument();
    expect(listMcpProfilesMock).not.toHaveBeenCalled();
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
    expect(
      within(fileActions).queryByRole("button", { name: "Revertir archivo" }),
    ).not.toBeInTheDocument();
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

    await user.click(await screen.findByRole("button", { name: "Acciones de sesión" }));
    const revert = await screen.findByRole("button", { name: "Revertir sesión" });
    expect(revert).toHaveAttribute(
      "title",
      "Revertir la sesión de Codex en a y deshacer sus cambios.",
    );
    await user.click(revert);

    expect(confirmMock).toHaveBeenCalledWith(
      "Se desharán todos los cambios hechos por esta sesión. ¿Quieres continuar?",
      {
        title: "Revertir sesión de Agent",
        kind: "warning",
        okLabel: "Revertir sesión",
        cancelLabel: "Cancelar",
      },
    );
    expect(revertSessionMock).toHaveBeenCalledWith("sess-1", true);
  });

  it("keeps stopping the whole session behind a confirmed secondary action", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "running", turn_status: "working" }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Acciones de sesión" }));
    await user.click(screen.getByRole("button", { name: "Detener sesión" }));

    expect(confirmMock).toHaveBeenCalledWith(
      "Se detendrá la sesión completa. La conversación se conservará, pero no podrás continuar este proceso. ¿Quieres continuar?",
      {
        title: "Detener sesión de Agent",
        kind: "warning",
        okLabel: "Detener sesión",
        cancelLabel: "Cancelar",
      },
    );
    expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
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
      await screen.findByTitle("Revertir el archivo src/a.ts al punto de control del turno 1."),
    );

    expect(confirmMock).toHaveBeenCalledWith(
      "¿Revertir el archivo src/a.ts al punto de control de este turno?",
      {
        title: "Revertir archivo desde el turno",
        kind: "warning",
        okLabel: "Revertir archivo",
        cancelLabel: "Cancelar",
      },
    );
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

    const restore = await screen.findByRole("button", { name: "Restaurar desde este turno" });
    expect(restore).toHaveAttribute(
      "title",
      "Restaurar los archivos y la conversación al turno 1.",
    );
    await user.click(restore);

    expect(confirmMock).toHaveBeenCalledWith(
      "Se restaurarán los archivos y la conversación al turno 1. ¿Quieres continuar?",
      {
        title: "Restaurar turno de Agent",
        kind: "warning",
        okLabel: "Restaurar desde este turno",
        cancelLabel: "Cancelar",
      },
    );
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
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "completed", pid: null, exit_code: 0, checkpoint: null }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: "Acciones de sesión" }));
    const button = await screen.findByRole("button", { name: "Revertir sesión" });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "Revertir sesión de Codex en a: no hay un punto de control reversible.",
    );
    expect(button).toHaveTextContent("Revertir sesión");
  });
});
