import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { busStore } from "../../bus/store";
import { WorkspaceActionsContext, type WorkspaceActions } from "../../workspace/actions";
import type {
  DeliveryApproval,
  DeliveryJob,
  DeliveryOverview,
  DeliveryTask,
} from "../../delivery/types";

const client = vi.hoisted(() => ({
  getDeliveryOverview: vi.fn(),
  createDeliveryTask: vi.fn(),
  dispatchDeliveryJob: vi.fn(),
  decideDeliveryApproval: vi.fn(),
  requestDeliveryApproval: vi.fn(),
  undoDeliveryJob: vi.fn(),
  cancelDeliveryJob: vi.fn(),
  retryDeliveryJob: vi.fn(),
  removeDeliveryTask: vi.fn(),
  updateDeliveryTask: vi.fn(),
  updateDeliverySettings: vi.fn(),
  releaseDeliveryLease: vi.fn(),
  getDeliveryJobLog: vi.fn(),
  getDeliveryRepoSettings: vi.fn(),
  setDeliveryRepoSettings: vi.fn(),
  setDeliveryCodexModel: vi.fn(() => Promise.resolve()),
  openDeliveryTaskInAgents: vi.fn(),
  openDeliveryJobInAgents: vi.fn(),
  completeDeliveryApproval: vi.fn(),
  createDeliveryRun: vi.fn(),
  takeoverDeliveryRun: vi.fn(),
  closeDeliveryRun: vi.fn(),
  onDeliveryChanged: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../../delivery/client", () => client);

const confirmMock = vi.hoisted(() => vi.fn(() => Promise.resolve(true)));
vi.mock("../../workbench/confirmDialog", () => ({ confirm: confirmMock }));

import { DeliveryPanel } from "./DeliveryPanel";

const REPO = "C:\\work\\agentos";

function task(over: Partial<DeliveryTask> = {}): DeliveryTask {
  return {
    id: "t1",
    run_id: null,
    repo: REPO,
    distro: null,
    key: "AGOS-501",
    title: "Fix the thing",
    worktree: "C:\\work\\agentos-wt\\AGOS-501",
    branch: "delivery/agos-501",
    base_ref: "develop",
    base_commit: "0123456789abcdef",
    state: "tests",
    contract_version: 1,
    created_at_ms: 1,
    updated_at_ms: 1,
    ...over,
  };
}

function job(over: Partial<DeliveryJob> = {}): DeliveryJob {
  return {
    id: "j1",
    task_id: "t1",
    role: "tests",
    attempt: 1,
    agent: "codex",
    model: "gpt-6-astra",
    access: "workspace",
    prompt: "Write the failing test.",
    contract_version: 1,
    writes: true,
    lease: null,
    timeout_minutes: 45,
    status: "finished",
    created_at_ms: 1,
    started_at_ms: 1000,
    ended_at_ms: 31000,
    pid: 5,
    provider_session_id: "thread-1",
    exit_code: 0,
    error: null,
    start_candidate: "abc:111111111",
    end_candidate: "abc:222222222",
    changes: [
      { path: "tests/new.test.ts", kind: "created" },
      { path: "src/app.ts", kind: "modified" },
    ],
    result: {
      status: "pass",
      summary: "Added the failing test.",
      changed_paths: ["tests/new.test.ts"],
      checks: [{ command: "npm test", result: "1 failing as expected" }],
      findings: [],
      handoff: "Implement the parser change.",
    },
    result_state: "accepted",
    result_note: null,
    undone_at_ms: null,
    ...over,
  };
}

function overview(over: Partial<DeliveryOverview> = {}): DeliveryOverview {
  return {
    runs: [],
    tasks: [task()],
    jobs: [job()],
    leases: [],
    approvals: [],
    settings: { capacity: 3 },
    coordinator: { url: "http://127.0.0.1:47920/mcp", token: "secret" },
    ...over,
  };
}

const actions: WorkspaceActions = {
  openRepo: vi.fn(),
  addRepo: vi.fn(),
  removeRepo: vi.fn(),
  openFile: vi.fn(),
  openTimeline: vi.fn(),
  openDashboard: vi.fn(),
  openAgents: vi.fn(),
  openDelivery: vi.fn(),
  openAgentTerminal: vi.fn(),
};

function renderPanel() {
  return render(
    <WorkspaceActionsContext.Provider value={actions}>
      <DeliveryPanel />
    </WorkspaceActionsContext.Provider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  client.getDeliveryJobLog.mockResolvedValue({ entries: [], next_line: 0, stderr_tail: "" });
  client.getDeliveryRepoSettings.mockResolvedValue({
    worktree_root: null,
    bootstrap: null,
    default_base: null,
  });
  act(() => {
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [{ source: "local", path: REPO, alias: null, fs_watch: [] }],
        },
      ],
    });
  });
});

describe("DeliveryPanel", () => {
  it("lists tasks and shows a job's result, changes and candidate", async () => {
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    expect(await screen.findByRole("heading", { name: /AGOS-501/ })).toBeInTheDocument();
    expect(screen.getByText("delivery/agos-501")).toBeInTheDocument();
    expect(screen.getByText("Added the failing test.")).toBeInTheDocument();
    const changes = screen.getByRole("list", { name: "Archivos cambiados" });
    expect(within(changes).getByText("tests/new.test.ts")).toBeInTheDocument();
    expect(within(changes).getByText("+")).toBeInTheDocument();
    expect(within(changes).getByText("−")).toBeInTheDocument();
    expect(screen.getByText("11111111 → 22222222")).toBeInTheDocument();
    expect(screen.getByText("Implement the parser change.")).toBeInTheDocument();
  });

  it("creates a task in the chosen repository", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview({ tasks: [], jobs: [] }));
    client.createDeliveryTask.mockResolvedValue(task({ id: "t2", key: "AGOS-502" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Nueva tarea" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva tarea" });
    await user.type(within(dialog).getByPlaceholderText("AGOS-501"), "AGOS-502");
    await user.type(within(dialog).getByPlaceholderText("Qué hay que entregar"), "Next");
    await user.click(within(dialog).getByRole("button", { name: "Crear tarea" }));
    await waitFor(() =>
      expect(client.createDeliveryTask).toHaveBeenCalledWith(
        expect.objectContaining({ repo: REPO, distro: null, key: "AGOS-502", title: "Next" }),
      ),
    );
    expect(client.setDeliveryRepoSettings).not.toHaveBeenCalled();
  });

  it("launches the next stage with the previous handoff, asking before full access", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Lanzar implementación" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    expect(within(dialog).getByRole("combobox", { name: /Etapa/ })).toHaveValue("implementation");
    const instructions = within(dialog).getByRole("textbox", { name: /Instrucciones/ });
    expect(instructions).toHaveValue("Implement the parser change.");
    expect(
      within(dialog).getByText(/Tomado de «Para la siguiente etapa» de Tests/),
    ).toBeInTheDocument();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Acceso" }), "full");
    await user.type(instructions, " Make it pass");
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() => expect(client.dispatchDeliveryJob).toHaveBeenCalled());
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("C:\\work\\agentos-wt\\AGOS-501"),
      expect.objectContaining({ title: "Dar acceso completo" }),
    );
    expect(client.dispatchDeliveryJob).toHaveBeenCalledWith(
      expect.objectContaining({
        taskId: "t1",
        role: "implementation",
        agent: "codex",
        access: "full",
        prompt: "Implement the parser change. Make it pass",
      }),
    );
  });

  it("lets Claude without full access run the repo's commands, saved for the repo", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: "npm ci",
      default_base: null,
      checks: ["npm test"],
    });
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Otra etapa…" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    expect(
      within(dialog).queryByRole("textbox", { name: /Comandos que puede ejecutar/ }),
    ).toBeNull();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "claude");
    const commands = within(dialog).getByRole("textbox", { name: /Comandos que puede ejecutar/ });
    await waitFor(() => expect(commands).toHaveValue("npm test"));
    await user.type(commands, "\nnpm run lint");
    await user.type(
      within(dialog).getByPlaceholderText("Qué debe hacer esta etapa y cómo comprobarlo."),
      "Review it",
    );
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() => expect(client.dispatchDeliveryJob).toHaveBeenCalled());
    expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(REPO, {
      worktree_root: null,
      bootstrap: "npm ci",
      default_base: null,
      checks: ["npm test", "npm run lint"],
    });
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("reports the catalog's default model for Codex jobs that name none", async () => {
    localStorage.setItem(
      "tinto.agents.lastRuntimeCatalog",
      JSON.stringify({
        status: "ready",
        models: [{ id: "gpt-6-astra" }],
        default_model: "gpt-6-astra",
      }),
    );
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    await waitFor(() => expect(client.setDeliveryCodexModel).toHaveBeenCalledWith("gpt-6-astra"));
    localStorage.removeItem("tinto.agents.lastRuntimeCatalog");
  });

  it("shows the exact text of a pending approval and approves it", async () => {
    const user = userEvent.setup();
    const approval: DeliveryApproval = {
      id: "a1",
      task_id: "t1",
      rung: "commit",
      title: "AGOS-501: Fix the thing",
      body: "Parser now rejects empty input.",
      status: "pending",
      requested_by: "coord",
      requested_at_ms: 1,
      decided_at_ms: null,
      executed_at_ms: null,
      outcome: null,
    };
    client.getDeliveryOverview.mockResolvedValue(overview({ approvals: [approval] }));
    client.decideDeliveryApproval.mockResolvedValue({ ...approval, status: "executed" });
    renderPanel();
    const group = await screen.findByRole("group", { name: "Aprobación commit" });
    expect(within(group).getByText(/Parser now rejects empty input\./)).toBeInTheDocument();
    expect(within(group).getByText(/pedido por/)).toHaveTextContent("coord");
    expect(within(group).getByText(/No publica nada/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 tarea te necesita" })).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText("Aprobar commit"),
    ).toBeInTheDocument();
    await user.click(within(group).getByRole("button", { name: "Aprobar y hacer commit" }));
    await waitFor(() => expect(client.decideDeliveryApproval).toHaveBeenCalledWith("a1", true));
  });

  it("undoes the latest job after confirming", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.undoDeliveryJob.mockResolvedValue(job({ undone_at_ms: 5 }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Deshacer estos cambios" }));
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("2 archivos"),
      expect.objectContaining({ title: "Deshacer intento" }),
    );
    await waitFor(() => expect(client.undoDeliveryJob).toHaveBeenCalledWith("j1"));
  });

  it("flags a quarantined QA resource and asks how it was checked before releasing", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        leases: [
          {
            name: "qa",
            state: "quarantined",
            generation: 3,
            holder_job_id: "j9",
            holder_task_id: "t1",
            acquired_at_ms: 1,
            note: "El trabajo qa terminó sin liberar el recurso.",
            queue: [],
          },
        ],
      }),
    );
    client.releaseDeliveryLease.mockResolvedValue({});
    renderPanel();
    expect(await screen.findByRole("status")).toHaveTextContent(
      "El recurso QA está en cuarentena tras AGOS-501.",
    );
    await user.click(screen.getByRole("button", { name: "Liberar…" }));
    const dialog = screen.getByRole("dialog", { name: "Liberar el recurso qa" });
    await user.type(
      within(dialog).getByRole("textbox", { name: "Qué comprobaste" }),
      "Browser closed",
    );
    await user.click(within(dialog).getByRole("button", { name: "Liberar" }));
    await waitFor(() =>
      expect(client.releaseDeliveryLease).toHaveBeenCalledWith("qa", "Browser closed"),
    );
  });

  it("continues a finished Codex job in Agents", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.openDeliveryJobInAgents.mockResolvedValue({
      session_id: "s1",
      repo: "C:\\work\\agentos-wt\\AGOS-501",
      agent_type: "codex",
    });
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Continuar en Agents" }));
    await waitFor(() =>
      expect(actions.openAgentTerminal).toHaveBeenCalledWith({
        sessionId: "s1",
        repo: "C:\\work\\agentos-wt\\AGOS-501",
        agentType: "codex",
      }),
    );
  });

  it("flags a failed stage and retries it from the banner", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job(),
          job({
            id: "j2",
            role: "implementation",
            attempt: 2,
            status: "failed",
            error: "El proceso terminó con código 1 sin un resultado válido.",
            result: null,
            result_state: "invalid",
            changes: [],
          }),
        ],
      }),
    );
    client.retryDeliveryJob.mockResolvedValue(job({ id: "j3" }));
    renderPanel();
    const banner = await screen.findByRole("group", { name: "Etapa fallida" });
    expect(banner).toHaveTextContent("Implementación falló");
    expect(banner).toHaveTextContent("código 1");
    expect(screen.queryByRole("button", { name: "Lanzar implementación" })).toBeNull();
    await user.click(within(banner).getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(client.retryDeliveryJob).toHaveBeenCalledWith("j2"));
  });

  it("has no manual state control and deletes the task from the menu", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.removeDeliveryTask.mockResolvedValue(undefined);
    renderPanel();
    await screen.findByRole("heading", { name: /AGOS-501/ });
    expect(screen.queryByRole("combobox", { name: /Estado/ })).toBeNull();
    expect(screen.getByText("Estado: Tests")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Más acciones de la tarea" }));
    await user.click(screen.getByRole("menuitem", { name: "Eliminar tarea…" }));
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("C:\\work\\agentos-wt\\AGOS-501"),
      expect.objectContaining({ title: "Eliminar tarea" }),
    );
    await waitFor(() => expect(client.removeDeliveryTask).toHaveBeenCalledWith("t1", false));
  });

  it("offers only the next delivery rung, behind a button", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [
          {
            id: "a1",
            task_id: "t1",
            rung: "commit",
            title: "AGOS-501: Fix the thing",
            body: "",
            status: "executed",
            requested_by: "user",
            requested_at_ms: 1,
            decided_at_ms: 2,
            executed_at_ms: 3,
            outcome: "abc1234",
          },
        ],
      }),
    );
    client.requestDeliveryApproval.mockResolvedValue({});
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Preparar push…" }));
    const form = screen.getByRole("form", { name: "Pedir aprobación de push" });
    await user.click(within(form).getByRole("button", { name: "Pedir aprobación" }));
    await waitFor(() =>
      expect(client.requestDeliveryApproval).toHaveBeenCalledWith(
        "t1",
        "push",
        "Publicar delivery/agos-501",
        "",
      ),
    );
  });

  it("shows one batch inline and several behind a single button", async () => {
    const user = userEvent.setup();
    const batch = (id: string, title: string) => ({
      id,
      repo: REPO,
      title,
      status: "active",
      generation: 1,
      owner: "coord",
      created_at_ms: 1,
      updated_at_ms: 1,
    });
    client.getDeliveryOverview.mockResolvedValue(overview({ runs: [batch("r1", "Lote A")] }));
    const { unmount } = renderPanel();
    expect(await screen.findByText("Lote A")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tomar el control" })).toBeInTheDocument();
    unmount();

    client.getDeliveryOverview.mockResolvedValue(
      overview({ runs: [batch("r1", "Lote A"), batch("r2", "Lote B"), batch("r3", "Lote C")] }),
    );
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "3 lotes activos" }));
    const dialog = screen.getByRole("dialog", { name: "Ajustes de Delivery" });
    expect(within(dialog).getAllByRole("button", { name: "Tomar el control" })).toHaveLength(3);
  });
});
