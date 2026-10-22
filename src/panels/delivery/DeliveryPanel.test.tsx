import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { busStore } from "../../bus/store";
import type { AgentSession } from "../../bus/contract";
import { agentSessionStore } from "../../agent/sessionStore";
import { setWindowsHostOverrideForTests } from "../../workbench/platform";
import { WorkspaceActionsContext, type WorkspaceActions } from "../../workspace/actions";
import type {
  DeliveryApproval,
  DeliveryDecision,
  DeliveryQaProblem,
  DeliveryJob,
  DeliveryOverview,
  DeliveryPhase,
  DeliveryTask,
  DeliveryTaskPhase,
  DeliveryWaiter,
} from "../../delivery/types";

const client = vi.hoisted(() => ({
  getDeliveryOverview: vi.fn(),
  getDeliveryTaskChanges: vi.fn(),
  getDeliveryIssueUrl: vi.fn((): Promise<string | null> => Promise.resolve(null)),
  getDeliveryIssue: vi.fn(),
  openDeliveryVisualRecord: vi.fn(),
  openDeliveryUrl: vi.fn(() => Promise.resolve()),
  pauseDeliveryJob: vi.fn(),
  resumeDeliveryJob: vi.fn(),
  setDeliveryRunPaused: vi.fn(),
  createDeliveryTask: vi.fn(),
  dispatchDeliveryJob: vi.fn(),
  decideDeliveryApproval: vi.fn(),
  requestDeliveryApproval: vi.fn(),
  getDeliveryCommitDraft: vi.fn(),
  undoDeliveryCommit: vi.fn(),
  undoDeliveryJob: vi.fn(),
  cancelDeliveryJob: vi.fn(),
  retryDeliveryJob: vi.fn(),
  resolveDeliveryJob: vi.fn(),
  removeDeliveryTask: vi.fn(),
  updateDeliveryTask: vi.fn(),
  updateDeliverySettings: vi.fn(),
  releaseDeliveryLease: vi.fn(),
  getDeliveryJobLog: vi.fn(),
  getDeliveryRepoSettings: vi.fn(),
  setDeliveryRepoSettings: vi.fn(),
  setDeliveryModelCatalog: vi.fn(() => Promise.resolve()),
  openDeliveryTaskInAgents: vi.fn(),
  openDeliveryJobInAgents: vi.fn(),
  completeDeliveryApproval: vi.fn(),
  answerDeliveryDecision: vi.fn(),
  changeDeliveryDecision: vi.fn(),
  renewDeliveryPermissions: vi.fn(),
  withdrawDeliveryDecision: vi.fn(),
  acceptRecommendedDecisions: vi.fn(),
  approveDeliverySettingsProposal: vi.fn(),
  rejectDeliverySettingsProposal: vi.fn(),
  refreshDeliveryPr: vi.fn(() => Promise.resolve(null)),
  checkDeliveryTask: vi.fn(() => Promise.resolve(null)),
  checkDeliveryEnvironment: vi.fn((): Promise<{ name: string; ok: boolean; output: string }[]> =>
    Promise.resolve([]),
  ),
  findDeliveryPr: vi.fn(() => Promise.resolve(null)),
  closeDeliveryPr: vi.fn(() => Promise.resolve(null)),
  sendDeliveryQaProblems: vi.fn(() => Promise.resolve([])),
  dismissDeliveryQaProblem: vi.fn(() => Promise.resolve({})),
  reportDeliveryQaProblem: vi.fn(() => Promise.resolve({})),
  handOverDeliveryRun: vi.fn(() => Promise.resolve()),
  openDeliveryPr: vi.fn(() => Promise.resolve({})),
  deliveryPrToDraft: vi.fn(() => Promise.resolve(null)),
  getDeliveryMergeMethod: vi.fn(() => Promise.resolve("squash")),
  getDeliveryPrReviewers: vi.fn((): Promise<string[]> => Promise.resolve([])),
  correctDeliveryApproval: vi.fn(),
  archiveDeliveryTask: vi.fn(),
  getArchivedDeliveryTasks: vi.fn(),
  unarchiveDeliveryTask: vi.fn(),
  markDeliveryPrSeen: vi.fn(),
  setDeliveryRunQaJiraComment: vi.fn(),
  createDeliveryRun: vi.fn(),
  takeoverDeliveryRun: vi.fn(),
  closeDeliveryRun: vi.fn(),
  addDeliveryBlocker: vi.fn(() => Promise.resolve({})),
  clearDeliveryBlockers: vi.fn(() => Promise.resolve({})),
  onDeliveryChanged: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../../delivery/client", () => client);

const confirmMock = vi.hoisted(() => vi.fn(() => Promise.resolve(true)));
vi.mock("../../workbench/confirmDialog", () => ({ confirm: confirmMock }));

const detachMock = vi.hoisted(() => ({
  openDetachedPanelWindow: vi.fn(() => Promise.resolve(true)),
  watchDetachedPanelWindow: vi.fn<(panel: unknown, onClosed: () => void) => Promise<boolean>>(() =>
    Promise.resolve(false),
  ),
}));
vi.mock("../../workspace/detachPanelWindow", () => detachMock);

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

/** A task's phase as the backend derives it (src-tauri/src/delivery/phase.rs). */
function phase(
  of: DeliveryPhase,
  waits_on: DeliveryWaiter = "user",
  next_stage: string | null = null,
): DeliveryTaskPhase {
  return { ...of, waits_on, next_stage };
}

/** Where the default task is: its tests passed, the implementation is due. */
const NEXT_IMPLEMENTATION = phase(
  { kind: "next_stage", role: "implementation" },
  "coordinator",
  "implementation",
);

function overview(over: Partial<DeliveryOverview> = {}): DeliveryOverview {
  return {
    phases: { t1: NEXT_IMPLEMENTATION },
    runs: [],
    tasks: [task()],
    jobs: [job()],
    leases: [],
    approvals: [],
    decisions: [],
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

/** An open step is one line in the task; its approval opens on top. */
async function openStep(button: string, rung: string) {
  await userEvent.click(await screen.findByRole("button", { name: button }));
  return screen.getByRole("group", { name: `Aprobación ${rung}` });
}

function renderPanel() {
  return render(
    <WorkspaceActionsContext.Provider value={actions}>
      <DeliveryPanel />
    </WorkspaceActionsContext.Provider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.removeItem("tinto:delivery-list-view");
  localStorage.removeItem("tinto:delivery-dock");
  localStorage.removeItem("tinto:delivery-detached");
  detachMock.watchDetachedPanelWindow.mockResolvedValue(false);
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
  it("gives each repo its own tab with only its tasks", async () => {
    const user = userEvent.setup();
    const other = "C:\\work\\tinto";
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [
          task(),
          task({ id: "t2", repo: other, key: "TIN-7", worktree: "C:\\work\\tinto-wt\\TIN-7" }),
        ],
      }),
    );
    renderPanel();
    expect(await screen.findByRole("heading", { name: /AGOS-501/ })).toBeInTheDocument();
    await user.click(screen.getByTitle(other));
    const view = await screen.findByLabelText("Delivery de tinto");
    const list = within(view).getByRole("navigation", { name: "Tareas de Delivery" });
    expect(within(list).getByText("TIN-7")).toBeInTheDocument();
    expect(within(list).queryByText("AGOS-501")).toBeNull();
  });

  it("lists an issue's packages together and ends a middle package at its merge", async () => {
    const user = userEvent.setup();
    const backend = task({
      key: "AGOS-432",
      title: "Relaciones",
      package: { index: 1, of: 2, title: "Backend" },
      worktree: "C:\\work\\agentos-wt\\AGOS-432-1",
      branch: "delivery/agos-432-1",
    });
    const ui = task({
      ...backend,
      id: "t2",
      package: { index: 2, of: 2, title: "UI", builds_on: "t1" },
      worktree: "C:\\work\\agentos-wt\\AGOS-432-2",
      branch: "delivery/agos-432-2",
    });
    // Package 1's ladder as Tinto sends it: no Jira after its merge.
    const middle = (["commit", "push", "pr", "jira", "merge"] as const).map((rung) => ({
      rung,
      stage: rung === "jira" ? "after_pr" : null,
    }));
    const packages = [backend, ui].map((one) => ({
      task_id: one.id,
      index: one.package!.index,
      title: one.package!.title,
      merged: false,
      delivered: false,
      archived: false,
    }));
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [ui, backend],
        jobs: [],
        phases: {
          t1: { ...NEXT_IMPLEMENTATION, steps: middle },
          t2: phase({ kind: "blocked", open: 1 }, "dependency"),
        },
        issues: [{ repo: REPO, key: "AGOS-432", of: 2, packages, delivered: false }],
      }),
    );
    renderPanel();
    const list = await screen.findByRole("navigation", { name: "Tareas de Delivery" });
    expect(await within(list).findByText("AGOS-432 · 2/2")).toBeInTheDocument();
    expect(within(list).getByText("Backend · Relaciones")).toBeInTheDocument();
    expect(within(list).getAllByText("1/2 en curso · 2/2 bloqueado")).toHaveLength(2);

    await user.click(within(list).getByText("AGOS-432 · 1/2"));
    expect(await screen.findByRole("heading", { name: /AGOS-432 · 1\/2/ })).toBeInTheDocument();
    const ladder = screen.getByRole("list", { name: "Entrega" });
    expect(within(ladder).getByText("Merge")).toBeInTheDocument();
    expect(within(ladder).queryByText("Jira (merge)")).toBeNull();
  });

  it("leaves a repo's tab out while its window outlives a reload", async () => {
    localStorage.setItem("tinto:delivery-detached", JSON.stringify({ "c:/work/agentos": REPO }));
    let closeWindow = () => {};
    detachMock.watchDetachedPanelWindow.mockImplementation((_panel, onClosed) => {
      closeWindow = onClosed;
      return Promise.resolve(true);
    });
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    await waitFor(() =>
      expect(detachMock.watchDetachedPanelWindow).toHaveBeenCalledWith(
        { kind: "delivery", repo: REPO },
        expect.any(Function),
      ),
    );
    expect(await screen.findByText("Archivadas…")).toBeInTheDocument();
    await act(async () => {});
    expect(screen.queryByTitle(REPO)).toBeNull();

    act(() => closeWindow());
    expect(await screen.findByTitle(REPO)).toBeInTheDocument();
    expect(localStorage.getItem("tinto:delivery-detached")).toBe("{}");
  });

  it("does not restore a saved tab for a repo outside the workbench", async () => {
    const old = "C:\\work\\old";
    client.getDeliveryOverview.mockResolvedValue(
      overview({ tasks: [task(), task({ id: "t2", repo: old, key: "OLD-1" })] }),
    );
    const first = renderPanel();
    expect(await screen.findByTitle(old)).toBeInTheDocument();
    first.unmount();
    expect(localStorage.getItem("tinto:delivery-dock")).toContain("c:/work/old");

    // Its tasks are gone and it is not in the workbench.
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    expect(await screen.findByTitle(REPO)).toBeInTheDocument();
    expect(screen.queryByTitle(old)).toBeNull();
  });

  it("opens a workbench repo without tasks from the tabs' +", async () => {
    const user = userEvent.setup();
    const other = "C:\\work\\tinto";
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              { source: "local", path: REPO, alias: null, fs_watch: [] },
              { source: "local", path: other, alias: null, fs_watch: [] },
            ],
          },
        ],
      });
    });
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Abrir un repositorio" }));
    await user.click(screen.getByRole("menuitem", { name: "tinto" }));
    expect(await screen.findByTitle(other)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Abrir un repositorio" })).toBeNull();
  });

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

  it("shows what a running agent does, its plan and use, and pauses it", async () => {
    const user = userEvent.setup();
    const started = Date.now() - 60_000;
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job({
            status: "started",
            started_at_ms: started,
            ended_at_ms: null,
            result: null,
            activity: "npm test -- parser",
            activity_at_ms: Date.now() - 5_000,
            plan: [
              { text: "Leer el test", status: "completed" },
              { text: "Arreglar el parser", status: "in_progress" },
            ],
            usage: {
              input_tokens: 21_600,
              cached_tokens: 21_120,
              output_tokens: 226,
              cost_micro_usd: 250_000,
            },
          }),
        ],
      }),
    );
    client.pauseDeliveryJob.mockResolvedValue({});
    renderPanel();
    const activity = await screen.findByLabelText("Actividad de Tests");
    expect(activity).toHaveTextContent("npm test -- parser");
    expect(activity).toHaveTextContent(/hace \d+ s/);
    const plan = screen.getByRole("list", { name: "Plan del agente" });
    expect(within(plan).getByText("Arreglar el parser")).toBeInTheDocument();
    expect(
      screen.getByText("Consumo: 21.6k de entrada · 21.1k en caché · 226 de salida · $0.25"),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Pausar" }));
    await waitFor(() => expect(client.pauseDeliveryJob).toHaveBeenCalledWith("j1"));
  });

  it("shows the command a running agent waits on, and how long, instead of a quiet agent", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job({
            status: "started",
            started_at_ms: Date.now() - 20 * 60_000,
            ended_at_ms: null,
            result: null,
            activity: "Bash: npm --prefix frontend run lint",
            activity_at_ms: Date.now() - 8 * 60_000,
            current_command: "npm --prefix frontend run lint",
            current_command_at_ms: Date.now() - 8 * 60_000,
          }),
        ],
      }),
    );
    renderPanel();
    const activity = await screen.findByLabelText("Actividad de Tests");
    expect(activity).toHaveTextContent("ejecutando npm --prefix frontend run lint 8 min");
    expect(activity).not.toHaveTextContent("sin actividad");
  });

  it("says why a queued job has not started, and when what it waits for looks stuck", async () => {
    const since = Date.now() - 31 * 60_000;
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [job({ status: "queued", started_at_ms: null, ended_at_ms: null, result: null })],
        queued: {
          j1: {
            reason: "espera a que terminen las comprobaciones (trabajo c1) de esta tarea",
            since_ms: since,
            warning: "el trabajo c1 lleva 50 min, más que su límite de 30 min",
          },
        },
      }),
    );
    renderPanel();
    const wait = await screen.findByLabelText("Por qué espera Tests");
    expect(wait).toHaveTextContent(
      "espera a que terminen las comprobaciones (trabajo c1) de esta tarea en cola hace 31 min",
    );
    expect(wait).toHaveTextContent("el trabajo c1 lleva 50 min, más que su límite de 30 min");
  });

  it("says where a finished job's time went", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job({
            timing: {
              model_ms: 5 * 60_000,
              commands_ms: 78 * 60_000,
              turns: 37,
              slowest: [
                { command: "npm run preflight", total_ms: 31 * 60_000, runs: 5, at_ceiling: 2 },
                { command: "npm test", total_ms: 10 * 60_000, runs: 1, at_ceiling: 1 },
                { command: "git status", total_ms: 4_000, runs: 3, at_ceiling: 0 },
              ],
            },
          }),
        ],
      }),
    );
    renderPanel();
    expect(await screen.findByLabelText("Tiempo de Tests")).toHaveTextContent(
      "Modelo 5 min · Comandos 78 min · 37 turnos · más lentos: " +
        "npm run preflight ×5 (31 min, 2 al límite de 10 min), " +
        "npm test (10 min, al límite de 10 min), git status ×3 (4 s)",
    );
  });

  it("lists each of Tinto's checks by its command, with its own earlier runs", async () => {
    const user = userEvent.setup();
    const check = (id: string, prompt: string, over: Partial<DeliveryJob> = {}) =>
      job({
        id,
        role: "checks",
        agent: "shell",
        model: null,
        prompt,
        provider_session_id: null,
        start_candidate: null,
        end_candidate: null,
        changes: [],
        result: null,
        ...over,
      });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job({
            id: "j1",
            role: "implementation",
            task_checks: ["Tinto runs this task's checks itself on what each stage changes."],
          }),
          check("c1", "npm run lint", { status: "failed", exit_code: 1, created_at_ms: 2 }),
          check("c2", "npm test", { created_at_ms: 3 }),
          check("c3", "npm run lint", { attempt: 2, created_at_ms: 4 }),
        ],
      }),
    );
    renderPanel();
    const history = await screen.findByRole("region", { name: "Historial" });
    expect(within(history).getByText("npm test")).toBeInTheDocument();
    // The earlier, failed lint run folds under the current one.
    expect(within(history).getAllByText("npm run lint", { selector: "code" })).toHaveLength(1);
    expect(
      within(history).getByRole("button", { name: /^1 intento anterior\s*· falló/ }),
    ).toBeInTheDocument();
    await user.click(within(history).getByRole("button", { name: /^Implementación/ }));
    expect(within(history).getByText("Comprobaciones de Tinto al empezar")).toBeInTheDocument();
  });

  it("resumes a paused stage and pauses the batch", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        runs: [
          {
            id: "r1",
            repo: REPO,
            title: "Lote A",
            status: "active",
            generation: 1,
            owner: "coord",
            created_at_ms: 1,
            updated_at_ms: 1,
          },
        ],
        tasks: [task({ run_id: "r1" })],
        jobs: [job({ status: "paused", result: null })],
      }),
    );
    client.resumeDeliveryJob.mockResolvedValue({});
    client.setDeliveryRunPaused.mockResolvedValue({});
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Reanudar" }));
    await waitFor(() => expect(client.resumeDeliveryJob).toHaveBeenCalledWith("j1"));
    expect(screen.queryByRole("button", { name: "Nuevo intento" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Pausar lote" }));
    await waitFor(() => expect(client.setDeliveryRunPaused).toHaveBeenCalledWith("r1", true));
  });

  it("shows the code a decision is about as a review thread, before its options", async () => {
    const user = userEvent.setup();
    const asked = {
      id: "d1",
      task_id: "t1",
      kind: "choice",
      question: "¿Qué carpetas puede reemplazar --force?",
      detail: "",
      references: [
        {
          kind: "pr_comment",
          path: "cli/src/commands.ts",
          line: 1092,
          url: "https://github.com/o/r/pull/299#discussion_r2105",
          outdated: false,
          lines: [
            { kind: "del", old: 980, new: null, text: "await installIntoDir(old);" },
            {
              kind: "add",
              old: null,
              new: 1092,
              text: "await installIntoDir(force);",
              target: true,
            },
          ],
          comment: {
            author: "Sn4pe",
            body: "With `--force`, a folder is deleted.",
            created_at: "2026-10-19T10:00:00Z",
            replies: 0,
          },
        },
        {
          kind: "code",
          path: "cli/src/commands.ts",
          line: 918,
          lines: [],
          error: "el archivo no existe en el worktree de la tarea",
        },
      ],
      options: [
        { label: "Solo copias de esta skill", consequence: "a", recommended: true },
        { label: "Cualquiera", consequence: "b", recommended: false },
      ],
      text: "",
      command: null,
      undo: "",
      status: "pending",
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
    } as unknown as DeliveryDecision;
    client.getDeliveryOverview.mockResolvedValue(
      overview({ decisions: [asked], phases: { t1: phase({ kind: "decisions", count: 1 }) } }),
    );
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Responder" }));
    const panel = screen.getByRole("dialog", { name: "Responder decisiones" });
    const diff = within(panel).getByRole("table", { name: "Líneas de cli/src/commands.ts" });
    expect(within(diff).getByText("await installIntoDir(force);")).toBeInTheDocument();
    expect(within(panel).getByText("--force").tagName).toBe("CODE");
    expect(within(panel).getByText("Sn4pe")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "Ver en GitHub" })).toHaveAttribute(
      "href",
      "https://github.com/o/r/pull/299#discussion_r2105",
    );
    // What Tinto could not read says so, with no made-up code.
    expect(
      within(panel).getByText("No disponible: el archivo no existe en el worktree de la tarea"),
    ).toBeInTheDocument();
    // The thread comes before the options.
    const thread = within(panel).getByText("Sn4pe");
    const option = within(panel).getByText("Solo copias de esta skill");
    expect(thread.compareDocumentPosition(option) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("lets the user pick a fix for each QA problem, dismiss one and send them together", async () => {
    const user = userEvent.setup();
    const problem = (over: Partial<DeliveryQaProblem>): DeliveryQaProblem => ({
      id: "p1",
      task_id: "t1",
      job_id: "qa-1",
      attempt: 2,
      number: 1,
      label: "P1",
      change: "Error toast",
      place: "Run panel",
      text: "The toast covers Retry on mobile.",
      suggestions: ["Move the toast up.", "Keep the error in the panel."],
      status: "open",
      solution: null,
      reason: null,
      notes: [],
      created_at_ms: 1,
      updated_at_ms: 1,
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        qa_problems: [
          problem({}),
          problem({
            id: "p2",
            number: 2,
            label: "P2",
            text: "The menu is cut off.",
            suggestions: [],
          }),
          problem({
            id: "p3",
            number: 3,
            text: "Old one",
            status: "verified",
            solution: "Done",
            notes: ["implementation 3"],
          }),
        ],
      }),
    );
    renderPanel();
    // They open on top of the task from the QA pill in its header.
    const pill = await screen.findByRole("button", { name: "Problemas de la QA, 2 por decidir" });
    expect(pill.closest(".delivery-task__heading")).not.toBeNull();
    await user.click(pill);
    const float = screen.getByRole("dialog", { name: "Problemas de la QA" });
    const section = within(float).getByRole("region", { name: "Problemas de la QA" });
    // What is settled hides until the user asks for everything.
    expect(within(section).queryByText("Old one")).toBeNull();
    const first = within(section).getByRole("article", { name: "Problema #1" });
    expect(within(first).getByText("P1 de la QA intento 2", { exact: false })).toBeInTheDocument();
    await user.click(within(first).getByRole("button", { name: "Aplicar solución sugerida" }));
    // Its own words for the second.
    const second = within(section).getByRole("article", { name: "Problema #2" });
    await user.type(
      within(second).getByRole("textbox", { name: "Tu solución para el problema #2" }),
      "Let it scroll",
    );
    expect(within(section).getByText(/2 problemas listos/)).toBeInTheDocument();
    await user.click(within(section).getByRole("button", { name: "Enviar al coordinador" }));
    expect(client.sendDeliveryQaProblems).toHaveBeenCalledWith("t1", [
      { problem_id: "p1", solution: "Move the toast up." },
      { problem_id: "p2", solution: "Let it scroll" },
    ]);

    // Dismissing offers to say why, without asking for it.
    await user.click(within(first).getByRole("button", { name: "No es un problema…" }));
    expect(
      within(first).getByRole("textbox", {
        name: "Por qué el problema #1 no es un problema (opcional)",
      }),
    ).toHaveValue("");
    await user.click(within(first).getByRole("button", { name: "Descartar" }));
    expect(client.dismissDeliveryQaProblem).toHaveBeenCalledWith("p1", "");

    await user.click(within(section).getByRole("button", { name: "Todos (3)" }));
    expect(within(section).getByText("Coordinador: implementation 3")).toBeInTheDocument();
  });

  it("keeps pending decisions to one line until the user opens them", async () => {
    const user = userEvent.setup();
    const asked: DeliveryDecision = {
      id: "d1",
      task_id: "t1",
      kind: "choice",
      question: "¿Formato del aviso?",
      detail: "",
      options: [
        { label: "Corto", consequence: "a", recommended: true },
        { label: "Largo", consequence: "b", recommended: false },
      ],
      text: "",
      command: null,
      undo: "",
      status: "pending",
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
    } as unknown as DeliveryDecision;
    client.getDeliveryOverview.mockResolvedValue(
      overview({ decisions: [asked], phases: { t1: phase({ kind: "decisions", count: 1 }) } }),
    );
    renderPanel();
    // In the task, one line (the sidebar says it too) and no cards.
    expect(await screen.findAllByText("1 decisión pendiente")).toHaveLength(2);
    expect(screen.queryByText("¿Formato del aviso?")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Responder" }));
    const panel = screen.getByRole("dialog", { name: "Responder decisiones" });
    expect(within(panel).getByText("¿Formato del aviso?")).toBeInTheDocument();
    await user.click(within(panel).getByRole("button", { name: "Cerrar decisiones" }));
    expect(screen.queryByRole("dialog", { name: "Responder decisiones" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Responder" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Responder decisiones" })).toBeNull();
  });

  it("opens the visual changes page QA left", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [job({ role: "qa", visual_record: "C:/jobs/j1/qa/AGOS-501.html" })],
      }),
    );
    client.openDeliveryVisualRecord.mockResolvedValue(undefined);
    renderPanel();
    // The job keeps its own button too, besides the one in the task's header.
    await user.click(await screen.findByRole("button", { name: "Abrir registro visual" }));
    await waitFor(() => expect(client.openDeliveryVisualRecord).toHaveBeenCalledWith("j1"));
  });

  it("opens the task's changes since its base in the files view", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryTaskChanges.mockResolvedValue({
      repo: "C:/work/agentos-wt/AGOS-501",
      base_ref: "develop",
      base_commit: "0123456789abcdef",
      files: [],
    });
    let answer: (changes: unknown) => void = () => {};
    client.getDeliveryTaskChanges.mockReturnValue(new Promise((resolve) => (answer = resolve)));
    renderPanel();
    const button = await screen.findByRole("button", { name: "Ver cambios" });
    await user.click(button);
    // It says so at once while the changes are read, at the same size.
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(button).toHaveTextContent("Ver cambios");
    answer({
      repo: "C:/work/agentos-wt/AGOS-501",
      base_ref: "develop",
      base_commit: "0123456789abcdef",
      files: [],
    });
    await waitFor(() =>
      expect(actions.openRepo).toHaveBeenCalledWith("C:/work/agentos-wt/AGOS-501"),
    );
    await waitFor(() => expect(button).not.toHaveAttribute("aria-busy"));
    expect(client.getDeliveryTaskChanges).toHaveBeenCalledWith("t1");
  });

  it("shows the task's Jira issue over the task when Tinto knows the host", async () => {
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryIssueUrl.mockResolvedValue("https://jira.example.com/browse/AGOS-501");
    client.getDeliveryIssue.mockRejectedValue({ message: "falta el token de Jira" });
    renderPanel();
    const button = await screen.findByRole("button", { name: "Ver AGOS-501 de Jira" });
    expect(button).toHaveTextContent("Jira");
    expect(client.getDeliveryIssueUrl).toHaveBeenCalledWith("t1");
    await userEvent.click(button);
    const card = screen.getByRole("dialog", { name: "Incidencia AGOS-501 de Jira" });
    expect(client.getDeliveryIssue).toHaveBeenCalledWith("t1");
    expect(await within(card).findByRole("alert")).toHaveTextContent("falta el token de Jira");
    // The full page is still there; the system's browser opens it.
    await userEvent.click(within(card).getByRole("link", { name: "Abrir en Jira ↗" }));
    expect(client.openDeliveryUrl).toHaveBeenCalledWith("https://jira.example.com/browse/AGOS-501");
    await userEvent.click(within(card).getByRole("button", { name: "Cerrar la incidencia" }));
    expect(screen.queryByRole("dialog", { name: /Incidencia/ })).toBeNull();
  });

  it("changes who runs a stage from now on in the repo, from its card", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({ jobs: [job({ role: "tests", reasoning: "high" })] }),
    );
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      stages: { review: { agent: "codex", model: null, reasoning: null } },
    });
    renderPanel();
    const track = await screen.findByRole("list", { name: "Agentes" });
    await user.click(
      within(track).getByRole("button", { name: "Agente, modelo y razonamiento de Tests" }),
    );
    await user.selectOptions(await screen.findByLabelText("Agente de Tests"), "claude");
    await waitFor(() =>
      expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(
        REPO,
        expect.objectContaining({
          stages: {
            review: { agent: "codex", model: null, reasoning: null },
            tests: expect.objectContaining({ agent: "claude" }),
          },
        }),
      ),
    );
    // The model and reasoning come with it, to change right there.
    expect(screen.getByLabelText("Modelo de Tests")).toBeInTheDocument();
    expect(screen.getByLabelText("Razonamiento de Tests")).toBeInTheDocument();
  });

  it("names the model and reasoning each stage runs with, defaults included", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job({ role: "tests", reasoning: "high" }),
          job({
            id: "j2",
            role: "implementation",
            agent: "claude",
            model: null,
            session_model: "claude-opus-5-5",
            status: "started",
            ended_at_ms: null,
          }),
        ],
      }),
    );
    renderPanel();
    const track = await screen.findByRole("list", { name: "Agentes" });
    expect(within(track).getByText("gpt-6-astra · alto")).toBeInTheDocument();
    expect(
      within(track).getByText("claude-opus-5-5 · razonamiento predeterminado"),
    ).toBeInTheDocument();
  });

  it("says what a blocked task waits for, linked, and lets the user remove, override or add", async () => {
    const blocker = task({ id: "t2", key: "AGOS-580", branch: "delivery/agos-580" });
    const blocked = task({
      blockers: [
        {
          id: "b1",
          type: "task",
          task_id: "t2",
          key: "AGOS-580",
          until: "merged",
          status: "PR abierto",
          cleared_at_ms: null,
        },
        {
          id: "b2",
          type: "pr",
          url: "https://github.com/acme/api/pull/305",
          until: "merged",
          state: "open",
          status: "abierto",
          cleared_at_ms: null,
        },
        { id: "b3", type: "note", text: "ya resuelto", status: "", cleared_at_ms: 5 },
      ],
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [blocked, blocker],
        phases: { t1: phase({ kind: "blocked", open: 2 }, "dependency"), t2: NEXT_IMPLEMENTATION },
      }),
    );
    renderPanel();
    const bar = await screen.findByRole("group", { name: "Bloqueos de la tarea" });
    expect(within(bar).getByText(/PR abierto/)).toBeInTheDocument();
    expect(within(bar).queryByText("ya resuelto")).not.toBeInTheDocument();
    expect(
      within(bar).getByRole("link", { name: "Abrir PR acme/api#305 en GitHub" }),
    ).toHaveAttribute("href", "https://github.com/acme/api/pull/305");

    await userEvent.click(
      within(bar).getByRole("button", { name: "Quitar el bloqueo PR acme/api#305" }),
    );
    expect(client.clearDeliveryBlockers).toHaveBeenCalledWith("t1", ["b2"]);
    await userEvent.click(within(bar).getByRole("button", { name: "Desbloquear" }));
    await waitFor(() => expect(client.clearDeliveryBlockers).toHaveBeenLastCalledWith("t1"));

    await userEvent.click(within(bar).getByRole("button", { name: "Añadir bloqueo" }));
    const form = screen.getByRole("form", { name: "Añadir bloqueo" });
    await userEvent.type(within(form).getByRole("textbox"), "AGOS-600");
    await userEvent.selectOptions(within(form).getByLabelText("Hasta que"), "pr_opened");
    await userEvent.click(within(form).getByRole("button", { name: "Bloquear" }));
    expect(client.addDeliveryBlocker).toHaveBeenCalledWith("t1", {
      type: "task",
      task: "AGOS-600",
      until: "pr_opened",
    });

    // The blocking task opens from its name.
    await userEvent.click(
      within(bar).getByRole("button", { name: "AGOS-580 (necesita: fusionado)" }),
    );
    expect(await screen.findByRole("heading", { name: /AGOS-580/ })).toBeInTheDocument();
  });

  it("opens a task's actions on a right click: what applies now, and copying its key", async () => {
    const other = task({ id: "t2", key: "AGOS-580", branch: "delivery/agos-580" });
    const blocked = task({
      blockers: [
        {
          id: "b1",
          type: "note",
          text: "Docker Desktop está apagado",
          status: "",
          cleared_at_ms: null,
        },
      ],
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [blocked, other],
        phases: { t1: phase({ kind: "blocked", open: 1 }, "external"), t2: NEXT_IMPLEMENTATION },
      }),
    );
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderPanel();
    const row = await screen.findByRole("button", { name: /^AGOS-580/ });
    await userEvent.pointer({ keys: "[MouseRight]", target: row });

    const menu = await screen.findByRole("menu", { name: "Acciones de AGOS-580" });
    const entries = within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent);
    expect(entries).toContain("Lanzar implementación");
    expect(entries).toContain("Bloquear por…");
    expect(entries).not.toContain("Quitar bloqueo");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Copiar clave (AGOS-580)" }));
    expect(writeText).toHaveBeenCalledWith("AGOS-580");
    expect(screen.queryByRole("menu", { name: "Acciones de AGOS-580" })).not.toBeInTheDocument();

    // The blocked task's menu offers to clear what it waits for.
    await userEvent.pointer({
      keys: "[MouseRight]",
      target: screen.getByRole("button", { name: /^AGOS-501/ }),
    });
    const blockedMenu = await screen.findByRole("menu", { name: /^Acciones de AGOS-501/ });
    await userEvent.click(within(blockedMenu).getByRole("menuitem", { name: "Quitar bloqueo" }));
    await waitFor(() => expect(client.clearDeliveryBlockers).toHaveBeenCalledWith("t1"));
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
    expect(within(dialog).queryByText(/Tomado de/)).toBeNull();
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
      within(dialog).queryByRole("textbox", { name: /Comprobaciones del repositorio/ }),
    ).toBeNull();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "claude");
    const commands = within(dialog).getByRole("textbox", {
      name: /Comprobaciones del repositorio/,
    });
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
      qa_commands: [],
      qa_browser: false,
      stages: { implementation: { agent: "claude", model: null, reasoning: "medium" } },
    });
    expect(confirmMock).not.toHaveBeenCalled();
  });

  it("gives the QA stage a browser and QA commands, saved for the repo", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      checks: ["npm test"],
    });
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Otra etapa…" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    expect(within(dialog).queryByRole("checkbox", { name: /Navegador para la QA/ })).toBeNull();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: /Etapa/ }), "qa");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "claude");
    const browser = within(dialog).getByRole("checkbox", { name: /Navegador para la QA/ });
    expect(browser).not.toBeChecked();
    await user.click(browser);
    await user.type(
      within(dialog).getByRole("textbox", { name: /Comandos de QA/ }),
      "agentos skills install",
    );
    await user.type(
      within(dialog).getByPlaceholderText("Qué debe hacer esta etapa y cómo comprobarlo."),
      "Run the QA matrix",
    );
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() => expect(client.dispatchDeliveryJob).toHaveBeenCalled());
    expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(
      REPO,
      expect.objectContaining({
        checks: ["npm test"],
        qa_commands: ["agentos skills install"],
        qa_browser: true,
      }),
    );
    expect(client.dispatchDeliveryJob).toHaveBeenCalledWith(
      expect.objectContaining({ role: "qa", agent: "claude" }),
    );
  });

  it("reports the catalog and its default model for Codex jobs that name none", async () => {
    localStorage.setItem(
      "tinto.agents.lastRuntimeCatalog",
      JSON.stringify({
        status: "ready",
        models: [{ id: "gpt-6-astra", model: "gpt-6-astra" }],
        default_model: "gpt-6-astra",
      }),
    );
    client.getDeliveryOverview.mockResolvedValue(overview());
    renderPanel();
    await waitFor(() =>
      expect(client.setDeliveryModelCatalog).toHaveBeenCalledWith("codex", "gpt-6-astra", [
        "gpt-6-astra",
      ]),
    );
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
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [approval],
        phases: { t1: phase({ kind: "approval", rung: "commit", stage: null }) },
      }),
    );
    client.decideDeliveryApproval.mockResolvedValue({ ...approval, status: "executed" });
    renderPanel();
    const group = await openStep("Revisar commit", "commit");
    expect(within(group).getByText(/Parser now rejects empty input\./)).toBeInTheDocument();
    expect(within(group).getByText(/preparado por/)).toHaveTextContent("el coordinador coord");
    expect(within(group).getByText(/No publica nada/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1 tarea te necesita" })).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText("Aprobar commit"),
    ).toBeInTheDocument();
    await user.click(within(group).getByRole("button", { name: "Aprobar y hacer commit" }));
    await waitFor(() => expect(client.decideDeliveryApproval).toHaveBeenCalledWith("a1", true));
  });

  const commitStep = (over: Partial<DeliveryApproval>): DeliveryApproval => ({
    id: "a1",
    task_id: "t1",
    rung: "commit",
    title: "2 commits",
    body: "",
    status: "pending",
    requested_by: "user",
    requested_at_ms: 1,
    decided_at_ms: null,
    executed_at_ms: null,
    outcome: null,
    commits: [
      { message: "feat(cli): add a", paths: ["cli/a.ts"] },
      { message: "test(cli): cover a\n\nWhy.", paths: ["cli/a.test.ts"] },
    ],
    left_out: ["scratch.log"],
    created: [],
    ...over,
  });

  it("shows the whole commit plan and who prepared it", async () => {
    client.getDeliveryOverview.mockResolvedValue(overview({ approvals: [commitStep({})] }));
    renderPanel();
    // The task shows one line; the plan opens on top.
    expect(await screen.findByText("· 2 commits · 2 archivos")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Aprobación commit" })).not.toBeInTheDocument();
    const group = await openStep("Revisar commit", "commit");
    expect(group).toHaveTextContent("preparado desde la interfaz de Tinto");
    const plan = within(group).getByRole("list", { name: "Plan de commits" });
    expect(plan).toHaveTextContent("feat(cli): add a");
    expect(plan).toHaveTextContent("Why.");
    expect(within(plan).getByRole("list", { name: "Archivos del commit 2" })).toHaveTextContent(
      "cli/a.test.ts",
    );
    expect(within(plan).getByRole("list", { name: "Archivos que quedan fuera" })).toHaveTextContent(
      "scratch.log",
    );
    expect(group).toHaveTextContent("estos commits, en este orden,");
  });

  it("folds a commit's long file list behind its count", async () => {
    const user = userEvent.setup();
    const paths = Array.from({ length: 7 }, (_, index) => `proto/gen/File${index}.ts`);
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [
          commitStep({
            commits: [
              { message: "feat(proto): regenerate the bindings", paths },
              { message: "test(proto): cover them", paths: ["proto/gen.test.ts"] },
            ],
          }),
        ],
      }),
    );
    renderPanel();
    const group = await openStep("Revisar commit", "commit");
    const long = within(group).getByText("7 archivos").closest("details")!;
    const short = within(group).getByText("1 archivo").closest("details")!;
    expect(long).not.toHaveAttribute("open");
    expect(short).toHaveAttribute("open");
    await user.click(within(group).getByText("7 archivos"));
    expect(long).toHaveAttribute("open");
  });

  it("prepares a plan of several commits without imposing a message", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryCommitDraft.mockResolvedValue({
      files: ["cli/a.ts", "cli/a.test.ts", "scratch.log"],
      recent: ["feat(cli): previous - AGOS-1"],
    });
    client.requestDeliveryApproval.mockResolvedValue({});
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Preparar commit…" }));
    const form = await screen.findByRole("form", { name: "Pedir aprobación de commit" });
    expect(form).toHaveTextContent("feat(cli): previous - AGOS-1");
    const first = within(form).getByRole("textbox", { name: /Mensaje del commit 1/ });
    expect(first).toHaveValue("");
    expect(within(form).getByRole("button", { name: "Pedir aprobación" })).toBeDisabled();
    await user.type(first, "feat(cli): add a");
    await user.click(within(form).getByRole("button", { name: "Añadir otro commit" }));
    await user.type(
      within(form).getByRole("textbox", { name: /Mensaje del commit 2/ }),
      "test(cli): cover a",
    );
    await user.selectOptions(
      within(form).getByRole("combobox", { name: "Destino de cli/a.test.ts" }),
      "Commit 2",
    );
    await user.selectOptions(
      within(form).getByRole("combobox", { name: "Destino de scratch.log" }),
      "Dejar sin commit",
    );
    expect(form).toHaveTextContent("Quedarán sin confirmar: scratch.log.");
    await user.click(within(form).getByRole("button", { name: "Pedir aprobación" }));
    await waitFor(() =>
      expect(client.requestDeliveryApproval).toHaveBeenCalledWith("t1", "commit", "", "", {
        commits: [
          { message: "feat(cli): add a", paths: ["cli/a.ts"] },
          { message: "test(cli): cover a", paths: ["cli/a.test.ts"] },
        ],
        leaveOut: ["scratch.log"],
      }),
    );
  });

  it("undoes an unpushed commit step, and asks for the commit again when it is gone", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [
          commitStep({ status: "executed", created: ["1111111", "2222222"], outcome: "2 commits" }),
        ],
      }),
    );
    client.undoDeliveryCommit.mockResolvedValue({});
    const view = renderPanel();
    await user.click(await screen.findByRole("button", { name: "Deshacer los 2 commits" }));
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("No se pierde nada"),
      expect.objectContaining({ title: "Deshacer el commit" }),
    );
    await waitFor(() => expect(client.undoDeliveryCommit).toHaveBeenCalledWith("a1"));
    view.unmount();

    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [
          commitStep({
            status: "undone",
            created: ["1111111"],
            outcome: "Los commits que hizo Tinto (1111111) ya no están en la rama.",
          }),
        ],
      }),
    );
    renderPanel();
    expect(await screen.findByRole("button", { name: "Preparar commit…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Preparar push…" })).toBeNull();
    expect(screen.getByRole("group", { name: "Paso de commit" })).toHaveTextContent(
      "ya no están en la rama",
    );
    expect(screen.queryByRole("button", { name: /Deshacer (el|los)/ })).toBeNull();
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
    // Nothing waits on it: no banner, only the header, which releases it.
    const chip = await screen.findByRole("button", { name: "QA en cuarentena" });
    expect(screen.queryByRole("button", { name: "Liberar…" })).toBeNull();
    await user.click(chip);
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

  it("raises the quarantine banner only when a QA waits on it", async () => {
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
            queue: [{ job_id: "j10", task_id: "t1", requested_at_ms: 2 }],
          },
        ],
      }),
    );
    renderPanel();
    expect(await screen.findByText(/^Una QA espera/)).toHaveTextContent(
      "Una QA espera a que liberes el recurso QA, en cuarentena tras AGOS-501.",
    );
    expect(screen.getByRole("button", { name: "Liberar…" })).toBeInTheDocument();
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

  it("clamps a blocked stage's long summary and settles it with a note", async () => {
    const user = userEvent.setup();
    const summary = `I stopped without changing anything. ${"Another session is running the same job in this worktree. ".repeat(8)}`;
    const blocked = job({
      id: "j2",
      role: "implementation",
      attempt: 22,
      status: "finished",
      error: "Tinto se cerró mientras el trabajo corría.",
      result: {
        status: "blocked",
        summary,
        changed_paths: [],
        checks: [],
        findings: [],
        handoff: "",
      },
    });
    client.getDeliveryOverview.mockResolvedValue(overview({ jobs: [job(), blocked] }));
    client.resolveDeliveryJob.mockResolvedValue({
      ...blocked,
      resolved_at_ms: 5,
      resolved_note: "Otra sesión terminó el trabajo",
    });
    renderPanel();
    const banner = await screen.findByRole("group", { name: "Etapa fallida" });
    expect(banner).toHaveTextContent("Implementación está bloqueada");
    expect(banner).toHaveTextContent("dejó 2 archivos cambiados");
    // The long reason shows its first lines until asked for all of it.
    const why = banner.querySelector(".delivery-broken__why")!;
    expect(why).toHaveClass("delivery-broken__why--clamped");
    await user.click(within(banner).getByRole("button", { name: "Ver todo" }));
    expect(why).not.toHaveClass("delivery-broken__why--clamped");
    expect(within(banner).getByRole("button", { name: "Ver menos" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );

    // Settled without running it again: a one-line note is required.
    await user.click(within(banner).getByRole("button", { name: "Ya está resuelta…" }));
    const settle = within(banner).getByRole("button", { name: "Dar por resuelta" });
    await user.click(settle);
    expect(client.resolveDeliveryJob).not.toHaveBeenCalled();
    await user.type(
      within(banner).getByRole("textbox", { name: /Cómo quedó resuelta/ }),
      "Otra sesión terminó el trabajo",
    );
    await user.click(settle);
    await waitFor(() =>
      expect(client.resolveDeliveryJob).toHaveBeenCalledWith(
        "j2",
        "Otra sesión terminó el trabajo",
      ),
    );
  });

  it("shows a settled stage as resolved in its history, without the alert", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [
          job(),
          job({
            id: "j2",
            role: "implementation",
            status: "interrupted",
            error: "Tinto se cerró mientras el trabajo corría.",
            result: null,
            resolved_at_ms: 5,
            resolved_note: "Otra sesión terminó el trabajo",
          }),
        ],
      }),
    );
    renderPanel();
    await screen.findByRole("heading", { name: /AGOS-501/ });
    expect(screen.queryByRole("group", { name: "Etapa fallida" })).toBeNull();
    const row = screen.getByRole("button", { name: /Implementación · intento/ });
    expect(row).toHaveTextContent("Resuelta aparte");
    if (row.getAttribute("aria-expanded") !== "true") await user.click(row);
    expect(
      screen.getByText("Dada por resuelta sin repetirla: Otra sesión terminó el trabajo"),
    ).toBeInTheDocument();
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

  it("shows a stopped coordinator as a chip that copies the nudge and hides this stop", async () => {
    const since = Date.now() - 10 * 60_000;
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        runs: [
          {
            id: "r1",
            repo: REPO,
            title: "Lote A",
            status: "active",
            generation: 1,
            owner: "claude-coord-1",
            created_at_ms: 1,
            updated_at_ms: 1,
          },
        ],
        coordinators: [{ run_id: "r1", waiting: false, unread: 2, unread_since_ms: since }],
      }),
    );
    const user = userEvent.setup();
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    localStorage.removeItem("tinto:delivery-hidden-stalls");
    renderPanel();
    expect(
      await screen.findByRole("button", { name: /1 coordinador te necesita/ }),
    ).toBeInTheDocument();
    // A chip in the batch bar, not a line across the panel.
    expect(screen.queryByText(/no está esperando y tiene/)).not.toBeInTheDocument();
    const chip = "Coordinador parado, 2 cambios sin leer";
    await user.click(await screen.findByRole("button", { name: chip }));
    const card = screen.getByRole("menu");
    expect(within(card).getByText(/claude-coord-1 tiene 2 cambios sin leer/)).toBeInTheDocument();
    await user.click(within(card).getByRole("menuitem", { name: "Copiar mensaje" }));
    expect(writeText).toHaveBeenCalledWith(expect.stringMatching(/^Sigue con wait_run/));

    // Hidden, it stays hidden for this stop.
    await user.click(screen.getByRole("button", { name: chip }));
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Ocultar" }));
    expect(screen.queryByRole("button", { name: chip })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /coordinador te necesita/ }),
    ).not.toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("tinto:delivery-hidden-stalls") ?? "[]")).toEqual([
      `r1:${since}`,
    ]);
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
    const menu = screen.getByRole("menu");
    expect(
      within(menu).getAllByRole("button", { name: /^Tomar el control del lote/ }),
    ).toHaveLength(3);
  });

  it("waits for the user's decisions before any stage runs", async () => {
    const user = userEvent.setup();
    const decision = (over: Partial<DeliveryDecision>): DeliveryDecision => ({
      id: "d1",
      task_id: "t1",
      kind: "choice",
      question: "¿Instalamos también para Codex?",
      detail: "--dir apunta a la carpeta final",
      options: [
        { label: "Sí", consequence: "Instala en Claude y Codex", recommended: true },
        { label: "No", consequence: "Solo Claude", recommended: false },
      ],
      text: "",
      command: null,
      undo: "",
      status: "pending",
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        // The stage due stays known while decisions hold it.
        phases: { t1: phase({ kind: "decisions", count: 2 }, "user", "implementation") },
        decisions: [
          decision({}),
          decision({
            id: "d2",
            kind: "permission",
            question: "¿Escribir en tu ~/.claude real?",
            options: [],
            command: "agentos skills install",
            undo: "agentos skills uninstall demo",
          }),
          decision({
            id: "d3",
            question: "¿Formato del aviso?",
            status: "answered",
            answer: "Corto",
            decided_by: "user",
            decided_at_ms: 2,
          }),
        ],
      }),
    );
    client.answerDeliveryDecision.mockResolvedValue({});
    client.acceptRecommendedDecisions.mockResolvedValue([]);
    renderPanel();
    // The task shows one line; the cards open in a floating panel.
    await userEvent.click(await screen.findByRole("button", { name: "Responder" }));
    const pending = await screen.findByRole("region", { name: "Decisiones pendientes" });
    expect(pending).toHaveTextContent("2 pendientes");
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText(
        "2 decisiones pendientes",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Lanzar implementación" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Otra etapa…" })).toBeDisabled();
    expect(within(pending).getByRole("radio", { name: /Sí/ })).toBeChecked();
    expect(within(pending).getByText(/agentos skills install/)).toBeInTheDocument();
    expect(
      within(pending).getByText(/Cómo se deshace: agentos skills uninstall demo/),
    ).toBeInTheDocument();

    await user.click(within(pending).getByRole("radio", { name: /No/ }));
    await user.click(within(pending).getByRole("button", { name: "Decidir" }));
    await waitFor(() => expect(client.answerDeliveryDecision).toHaveBeenCalledWith("d1", "No"));
    await user.click(within(pending).getByRole("button", { name: "Permitir" }));
    await waitFor(() =>
      expect(client.answerDeliveryDecision).toHaveBeenCalledWith("d2", "allowed"),
    );
    await user.click(within(pending).getByRole("button", { name: "Aceptar recomendadas" }));
    await waitFor(() => expect(client.acceptRecommendedDecisions).toHaveBeenCalledWith("t1"));

    const log = screen.getByRole("region", { name: "Decisiones" });
    expect(log).toHaveTextContent("¿Formato del aviso?");
    expect(log).toHaveTextContent("Corto");
  });

  it("shows a settings proposal as a diff and approves only what stays checked", async () => {
    const user = userEvent.setup();
    const checks = [
      "npm --prefix services/memories test",
      "npm --prefix frontend test",
      "npm --prefix frontend run preflight",
    ];
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      checks: ["npm --prefix cli test", "npm --prefix cli run build"],
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        phases: { t1: phase({ kind: "settings_proposal" }) },
        settings_proposals: [
          {
            id: "p1",
            repo: REPO,
            task_id: "t1",
            run_id: "r1",
            reason: "AGOS-474 necesita correr los tests de memories y del frontend",
            changes: { checks_add: checks, checks_remove: ["npm --prefix cli run build"] },
            status: "pending",
            approved: null,
            requested_by: "coord",
            requested_at_ms: 1,
            decided_by: null,
            decided_at_ms: null,
          },
        ],
      }),
    );
    client.approveDeliverySettingsProposal.mockResolvedValue({});
    renderPanel();

    const card = await screen.findByRole("region", { name: "Cambios de ajustes por aprobar" });
    expect(card).toHaveTextContent("AGOS-474 necesita correr los tests de memories y del frontend");
    expect(card).toHaveTextContent("Lo pide coord");
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText(
        "Aprobar cambio de ajustes",
      ),
    ).toBeInTheDocument();
    await waitFor(() => expect(card).toHaveTextContent("antes 2 · después 4"));
    expect(within(card).getByText("npm --prefix cli test")).toBeInTheDocument();

    await user.click(
      within(card).getByRole("checkbox", { name: "Añadir npm --prefix frontend run preflight" }),
    );
    expect(card).toHaveTextContent("antes 2 · después 3");
    await user.click(within(card).getByRole("button", { name: "Aprobar 3 de 4" }));
    await waitFor(() =>
      expect(client.approveDeliverySettingsProposal).toHaveBeenCalledWith("p1", {
        checks_add: checks.slice(0, 2),
        checks_remove: ["npm --prefix cli run build"],
        qa_commands_add: [],
        setup_add: [],
        setup_remove: [],
      }),
    );
  });

  it("shows the setup commands a coordinator proposes and approves the ones left checked", async () => {
    const user = userEvent.setup();
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      setup: ["npm --prefix cli ci"],
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        phases: { t1: phase({ kind: "settings_proposal" }) },
        settings_proposals: [
          {
            id: "p1",
            repo: REPO,
            task_id: "t1",
            run_id: "r1",
            reason: "AGOS-474 toca el frontend y sus bindings",
            changes: {
              setup_add: ["npm --prefix frontend ci", "npm run bindings"],
              setup_remove: ["npm --prefix cli ci"],
            },
            status: "pending",
            approved: null,
            requested_by: "coord",
            requested_at_ms: 1,
            decided_by: null,
            decided_at_ms: null,
          },
        ],
      }),
    );
    client.approveDeliverySettingsProposal.mockResolvedValue({});
    renderPanel();

    const card = await screen.findByRole("region", { name: "Cambios de ajustes por aprobar" });
    await waitFor(() => expect(card).toHaveTextContent("Comandos de preparación"));
    await waitFor(() => expect(card).toHaveTextContent("antes 1 · después 2"));
    await user.click(within(card).getByRole("checkbox", { name: "Quitar npm --prefix cli ci" }));
    expect(card).toHaveTextContent("antes 1 · después 3");
    await user.click(within(card).getByRole("button", { name: "Aprobar 2 de 3" }));
    await waitFor(() =>
      expect(client.approveDeliverySettingsProposal).toHaveBeenCalledWith("p1", {
        checks_add: [],
        checks_remove: [],
        qa_commands_add: [],
        setup_add: ["npm --prefix frontend ci", "npm run bindings"],
        setup_remove: [],
      }),
    );
  });

  it("shows the variables for checks a coordinator proposes and approves the ones left checked", async () => {
    const user = userEvent.setup();
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      check_env: { DB_NAME: "agos" },
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        phases: { t1: phase({ kind: "settings_proposal" }) },
        settings_proposals: [
          {
            id: "p1",
            repo: REPO,
            task_id: "t1",
            run_id: "r1",
            reason: "Cada tarea necesita su propia base de datos de pruebas",
            changes: {
              check_env: {
                DB_NAME: "agos_test",
                TEST_DB_MONGO_URL: "mongodb://127.0.0.1:27419/agos{task}",
              },
            },
            status: "pending",
            approved: null,
            requested_by: "coord",
            requested_at_ms: 1,
            decided_by: null,
            decided_at_ms: null,
          },
        ],
      }),
    );
    client.approveDeliverySettingsProposal.mockResolvedValue({});
    renderPanel();

    const card = await screen.findByRole("region", { name: "Cambios de ajustes por aprobar" });
    await waitFor(() => expect(card).toHaveTextContent("Variable DB_NAME: agos → agos_test"));
    expect(card).toHaveTextContent(
      "Variable TEST_DB_MONGO_URL: (sin definir) → mongodb://127.0.0.1:27419/agos{task}",
    );
    await user.click(within(card).getByRole("checkbox", { name: "Cambiar la variable DB_NAME" }));
    await user.click(within(card).getByRole("button", { name: "Aprobar 1 de 2" }));
    await waitFor(() =>
      expect(client.approveDeliverySettingsProposal).toHaveBeenCalledWith("p1", {
        checks_add: [],
        checks_remove: [],
        qa_commands_add: [],
        setup_add: [],
        setup_remove: [],
        check_env: { TEST_DB_MONGO_URL: "mongodb://127.0.0.1:27419/agos{task}" },
      }),
    );
  });

  it("shows exactly what a permission lets QA run and renews it for new code", async () => {
    const user = userEvent.setup();
    const permission = (over: Partial<DeliveryDecision>): DeliveryDecision => ({
      id: "p1",
      task_id: "t1",
      kind: "permission",
      question: "¿Probar la CLI del worktree?",
      detail: "",
      options: [],
      text: "",
      command: "bash C:/work/agentos/.agent/qa/run.sh",
      undo: "borrar la carpeta temporal",
      scripts: [{ path: "C:/work/agentos/.agent/qa/run.sh", content: "exec tsx cli/src/index.ts" }],
      read_dirs: ["C:/Users/me/.claude/skills"],
      runs_candidate: true,
      unfrozen: ["C:/work/agentos/.agent/env/qa.env"],
      write_dirs: ["C:/Users/me/.agents/skills", "C:/Users/me/.codex"],
      clients: ["codex"],
      full_access: true,
      warnings: ["Con Codex como agente de QA: bash no puede ejecutarse."],
      candidate: null,
      status: "pending",
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        decisions: [
          permission({}),
          permission({
            id: "p2",
            question: "¿Instalar en tu ~/.claude real?",
            status: "answered",
            answer: "allowed",
            candidate: "0a1b2c3d4e5f",
            decided_by: "user",
            decided_at_ms: 2,
          }),
        ],
      }),
    );
    client.renewDeliveryPermissions.mockResolvedValue([]);
    client.withdrawDeliveryDecision.mockResolvedValue(undefined);
    renderPanel();
    // The task shows one line; the cards open in a floating panel.
    await userEvent.click(await screen.findByRole("button", { name: "Responder" }));
    const pending = await screen.findByRole("region", { name: "Decisiones pendientes" });
    expect(pending).toHaveTextContent("Script congelado: C:/work/agentos/.agent/qa/run.sh");
    expect(pending).toHaveTextContent("exec tsx cli/src/index.ts");
    expect(pending).toHaveTextContent("leer y listar, sin modificar: C:/Users/me/.claude/skills");
    expect(pending).toHaveTextContent("no están congelados: C:/work/agentos/.agent/env/qa.env");
    expect(pending).toHaveTextContent("Ejecuta el código del worktree de la tarea");
    expect(pending).toHaveTextContent(
      "escribir y borrar en estas carpetas fuera del worktree, y en ninguna otra: C:/Users/me/.agents/skills C:/Users/me/.codex",
    );
    expect(pending).toHaveTextContent("arranca tu cliente real Codex, con tu sesión");
    expect(pending).toHaveTextContent("Acceso completo para la QA de esta tarea.");
    expect(pending).toHaveTextContent("correrá sin sandbox");
    expect(pending).toHaveTextContent(
      "Aviso: Con Codex como agente de QA: bash no puede ejecutarse.",
    );

    const log = screen.getByRole("region", { name: "Decisiones" });
    expect(log).toHaveTextContent("Permitido para el código 0a1b2c3d, que ya cambió.");
    expect(log).toHaveTextContent("El código cambió desde que permitiste 1 permiso");
    await user.click(
      within(log).getByRole("button", { name: "Volver a permitir para el código actual" }),
    );
    await waitFor(() => expect(client.renewDeliveryPermissions).toHaveBeenCalledWith("t1"));

    // A question that stopped making sense is taken back, after confirming.
    await user.click(
      within(pending).getByRole("button", {
        name: "Retirar la decisión «¿Probar la CLI del worktree?»",
      }),
    );
    await waitFor(() => expect(client.withdrawDeliveryDecision).toHaveBeenCalledWith("p1"));
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("se retira sin respuesta"),
      expect.objectContaining({ title: "Retirar la decisión" }),
    );
  });

  it("does not ask to renew a permission allowed for the current code", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        decisions: [
          {
            id: "p2",
            task_id: "t1",
            kind: "permission",
            question: "¿Probar la CLI del worktree?",
            detail: "",
            options: [],
            text: "",
            command: "node C:/work/agentos-wt/AGOS-501/cli.js",
            undo: "nada",
            runs_candidate: true,
            // The tree the task's latest stage left (the job's end candidate).
            candidate: "222222222",
            status: "answered",
            answer: "allowed",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_by: "user",
            decided_at_ms: 2,
          },
        ],
      }),
    );
    renderPanel();
    const log = await screen.findByRole("region", { name: "Decisiones" });
    expect(log).toHaveTextContent("Permitido para el código 22222222, el actual.");
    expect(within(log).queryByRole("button", { name: /Volver a permitir/ })).toBeNull();
  });

  it("lets the user choose where a Claude job runs and shows where each ran", async () => {
    const user = userEvent.setup();
    setWindowsHostOverrideForTests(true);
    client.getDeliveryOverview.mockResolvedValue(
      overview({ jobs: [job({ agent: "claude", ran_on: "WSL · Ubuntu-24.04" })] }),
    );
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
    });
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    expect(await screen.findByText(/· en WSL · Ubuntu-24.04/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Otra etapa…" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    expect(within(dialog).getByRole("combobox", { name: "Agente" })).toHaveValue("claude");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "codex");
    expect(within(dialog).queryByRole("combobox", { name: /Dónde se ejecuta/ })).toBeNull();
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "claude");
    await user.selectOptions(
      within(dialog).getByRole("combobox", { name: /Dónde se ejecuta/ }),
      "windows",
    );
    await user.type(
      within(dialog).getByPlaceholderText("Qué debe hacer esta etapa y cómo comprobarlo."),
      "Run QA",
    );
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() =>
      expect(client.dispatchDeliveryJob).toHaveBeenCalledWith(
        expect.objectContaining({ agent: "claude", platform: "windows" }),
      ),
    );
    setWindowsHostOverrideForTests(null);
  });

  it("keeps showing a task after it leaves the needs-you group", async () => {
    const user = userEvent.setup();
    const pending: DeliveryDecision = {
      id: "d1",
      task_id: "t2",
      kind: "choice",
      question: "¿Codex también?",
      detail: "",
      options: [
        { label: "Sí", consequence: "", recommended: true },
        { label: "No", consequence: "", recommended: false },
      ],
      text: "",
      command: null,
      undo: "",
      status: "pending",
      answer: null,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: null,
      decided_at_ms: null,
    };
    const tasks = [task(), task({ id: "t2", key: "DEC-1", title: "Decide first" })];
    const next = (role: string) => phase({ kind: "next_stage", role }, "coordinator", role);
    client.getDeliveryOverview
      .mockResolvedValueOnce(
        overview({
          tasks,
          jobs: [],
          decisions: [pending],
          phases: { t1: next("tests"), t2: phase({ kind: "decisions", count: 1 }) },
        }),
      )
      .mockResolvedValue(
        overview({
          tasks,
          jobs: [],
          decisions: [{ ...pending, status: "answered", answer: "Sí", decided_by: "user" }],
          phases: { t1: next("tests"), t2: next("tests") },
        }),
      );
    client.answerDeliveryDecision.mockResolvedValue({});
    renderPanel();
    expect(await screen.findByRole("heading", { name: /DEC-1/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Responder" }));
    await user.click(screen.getByRole("button", { name: "Decidir" }));
    await waitFor(() =>
      expect(screen.getByRole("region", { name: "Decisiones" })).toHaveTextContent("Sí"),
    );
    expect(screen.getByRole("heading", { name: /DEC-1/ })).toBeInTheDocument();
  });

  it("names each batch's buttons and never shows the coordinator token", async () => {
    const user = userEvent.setup();
    const batch = (id: string, title: string, created: number) => ({
      id,
      // As the interface stores it, with Windows' verbatim prefix.
      repo: "\\\\?\\" + REPO,
      title,
      status: "active",
      generation: 1,
      owner: id === "r1" ? "coord" : null,
      created_at_ms: created,
      updated_at_ms: created,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({ runs: [batch("r1", "Lote A", 1), batch("r2", "Lote B", 2)] }),
    );
    renderPanel();
    // The batches are the header's, not settings.
    await user.click(await screen.findByRole("button", { name: "2 lotes activos" }));
    const batches = screen.getByRole("menu");
    expect(
      within(batches).getByRole("button", { name: "Cerrar el lote Lote A" }),
    ).toBeInTheDocument();
    expect(
      within(batches).getByRole("button", { name: "Cerrar el lote Lote B" }),
    ).toBeInTheDocument();
    expect(
      within(batches).getByRole("button", { name: "Tomar el control del lote Lote A" }),
    ).toBeInTheDocument();

    // What applies to every repo is a dialog over the tasks.
    await user.click(screen.getByRole("button", { name: "Ajustes de Delivery" }));
    const settings = screen.getByRole("dialog", { name: "Ajustes de Delivery" });
    expect(settings).not.toHaveTextContent("secret");
    await user.click(
      within(settings).getByRole("button", { name: "Copiar la configuración de Claude Code" }),
    );
    expect(await navigator.clipboard.readText()).toContain("Bearer secret");
    expect(within(settings).getByText("Copiado, con el token.")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Ajustes de Delivery" })).toBeNull();

    // New tasks go to the repo's most recent active batch by default.
    await user.click(screen.getByRole("button", { name: "Nueva tarea" }));
    const dialog = screen.getByRole("dialog", { name: "Nueva tarea" });
    expect(within(dialog).getByRole("combobox", { name: "Lote" })).toHaveValue("r2");
  });

  it("keeps the batch's Jira policy and the repo's QA setup in Ajustes", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        runs: [
          {
            id: "r1",
            repo: REPO,
            title: "Lote A",
            status: "active",
            generation: 1,
            owner: "coord",
            created_at_ms: 1,
            updated_at_ms: 1,
            qa_jira_comment: null,
          },
        ],
      }),
    );
    client.createDeliveryRun.mockResolvedValue({});
    client.setDeliveryRunQaJiraComment.mockResolvedValue({});
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      check_env: { DB_NAME: "agentos_test" },
    });
    client.setDeliveryRepoSettings.mockImplementation((_repo, settings) =>
      Promise.resolve(settings),
    );
    renderPanel();
    // The batch's policy and a new batch are in the header's batch menu.
    await user.click(await screen.findByRole("button", { name: "Lote A" }));
    const batches = screen.getByRole("menu");

    const policy = within(batches).getByRole("combobox", {
      name: "Resultado de QA en Jira del lote Lote A",
    });
    expect(policy).toHaveValue("");
    await user.selectOptions(policy, "yes");
    await waitFor(() =>
      expect(client.setDeliveryRunQaJiraComment).toHaveBeenCalledWith("r1", true),
    );

    // The coordinator ends the brainstorm; the batch only shows where it is.
    expect(within(batches).getByText("Brainstorm")).toBeInTheDocument();

    await user.type(within(batches).getByPlaceholderText("Lote de octubre"), "Lote B");
    await user.selectOptions(
      within(batches).getByRole("combobox", {
        name: /^Resultado de QA en Jira.*El coordinador lo lee/,
      }),
      "yes",
    );
    await user.click(within(batches).getByRole("button", { name: "Crear lote" }));
    await waitFor(() =>
      expect(client.createDeliveryRun).toHaveBeenCalledWith(REPO, "Lote B", true),
    );

    // The repo's setup is behind its tab's gear, in place of its tasks.
    await user.click(screen.getByRole("button", { name: "Ajustes de agentos" }));
    expect(screen.queryByRole("navigation", { name: "Tareas de Delivery" })).toBeNull();
    const qa = screen.getByRole("region", { name: "Ajustes de agentos" });
    const save = await within(qa).findByRole("button", { name: "Guardar" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.type(
      within(qa).getByRole("textbox", { name: /Entorno de QA/ }),
      "Windows, ámbito personal",
    );
    await user.click(within(qa).getByRole("checkbox", { name: "Navegador para la QA" }));
    // The checks' environment shows as plain text, one variable per line.
    const env = within(qa).getByRole("textbox", { name: /Variables de entorno/ });
    expect(env).toHaveValue("DB_NAME=agentos_test");
    await user.click(env);
    await user.paste("\nTEST_DB_MONGO_URL = mongodb://localhost:27017/test");
    // Each stage's agent is fixed here too.
    expect(within(qa).queryByRole("combobox", { name: "Modelo de Revisión" })).toBeNull();
    await user.selectOptions(
      within(qa).getByRole("combobox", { name: "Agente de Revisión" }),
      "claude",
    );
    await user.selectOptions(
      within(qa).getByRole("combobox", { name: "Modelo de Revisión" }),
      "opus",
    );
    // And its time limit, for a long suite; empty keeps the role's.
    const limit = within(qa).getByRole("spinbutton", { name: "Tiempo máximo de Revisión" });
    expect(limit).toHaveValue(null);
    await user.type(limit, "90");
    await user.tab();
    expect(within(qa).getByText("Cambios sin guardar.")).toBeInTheDocument();
    await user.click(save);
    await waitFor(() =>
      expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(
        REPO,
        expect.objectContaining({
          qa_environment: "Windows, ámbito personal",
          qa_browser: true,
          check_env: {
            DB_NAME: "agentos_test",
            TEST_DB_MONGO_URL: "mongodb://localhost:27017/test",
          },
          stages: {
            review: { agent: "claude", model: "opus", reasoning: "medium", timeout_minutes: 90 },
          },
        }),
      ),
    );
    expect(await within(qa).findByText(/^Guardado para/)).toBeInTheDocument();
  });

  it("checks the repo's environment when its view opens and again on demand", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.checkDeliveryEnvironment
      .mockResolvedValueOnce([
        { name: "Docker", ok: true, output: "Docker 27.1.1 responde." },
        { name: "pg_isready -h localhost", ok: false, output: "localhost:5432 - no response" },
      ])
      .mockResolvedValueOnce([
        { name: "Docker", ok: true, output: "Docker 27.1.1 responde." },
        { name: "pg_isready -h localhost", ok: true, output: "accepting connections" },
      ]);
    renderPanel();
    const failing = await screen.findByRole("button", {
      name: "Entorno: falla pg_isready -h localhost. Comprobar de nuevo",
    });
    expect(client.checkDeliveryEnvironment).toHaveBeenCalledWith(REPO, null);
    expect(failing).toHaveClass("delivery-env--fail");
    expect(failing.title).toContain("✗ pg_isready -h localhost\nlocalhost:5432 - no response");
    expect(failing.title).toContain("✓ Docker");

    await user.click(failing);
    const ok = await screen.findByRole("button", { name: "Entorno: bien. Comprobar de nuevo" });
    expect(ok).toHaveClass("delivery-env--ok");
    expect(client.checkDeliveryEnvironment).toHaveBeenCalledTimes(2);
  });

  it("sets retrying on restart and marks verification commands as environment probes", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(overview());
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      checks: ["npm test", "pg_isready -h localhost"],
    });
    client.setDeliveryRepoSettings.mockImplementation((_repo, settings) =>
      Promise.resolve(settings),
    );
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Ajustes de agentos" }));
    const settings = screen.getByRole("region", { name: "Ajustes de agentos" });
    const save = await within(settings).findByRole("button", { name: "Guardar" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(within(settings).getByRole("checkbox", { name: /^Reintentar al reiniciar/ }));
    await user.click(within(settings).getByRole("checkbox", { name: "pg_isready -h localhost" }));
    expect(within(settings).getByRole("checkbox", { name: "npm test" })).not.toBeChecked();
    await user.type(
      within(settings).getByRole("textbox", { name: /^Comandos de preparación/ }),
      "npm --prefix frontend ci{Enter}npm run bindings",
    );
    await user.click(save);
    await waitFor(() =>
      expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(
        REPO,
        expect.objectContaining({
          retry_on_restart: true,
          environment_probes: ["pg_isready -h localhost"],
          setup: ["npm --prefix frontend ci", "npm run bindings"],
        }),
      ),
    );
  });

  it("picks the model and reasoning from selectors, Sol 6.1 at medium by default", async () => {
    const user = userEvent.setup();
    const effort = (value: string) => ({ value, description: value });
    localStorage.setItem(
      "tinto.agents.lastRuntimeCatalog",
      JSON.stringify({
        status: "ready",
        source: "test",
        updated_at_ms: 1,
        default_model: "gpt-6-astra",
        models: [
          {
            id: "gpt-6-astra",
            model: "gpt-6-astra",
            display_name: "GPT-6 Astra",
            description: "",
            supported_reasoning_efforts: [effort("low"), effort("high")],
            default_reasoning_effort: "low",
            is_default: true,
          },
          {
            id: "gpt-6.1-sol",
            model: "gpt-6.1-sol",
            display_name: "GPT-6.1-Sol",
            description: "",
            supported_reasoning_efforts: [effort("low"), effort("medium"), effort("high")],
            default_reasoning_effort: "low",
            is_default: false,
          },
        ],
      }),
    );
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [],
        phases: { t1: phase({ kind: "next_stage", role: "tests" }, "coordinator", "tests") },
      }),
    );
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Otra etapa…" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    const model = within(dialog).getByRole("combobox", { name: "Modelo" });
    const reasoning = within(dialog).getByRole("combobox", { name: "Razonamiento" });
    expect(model).toHaveValue("gpt-6.1-sol");
    expect(reasoning).toHaveValue("medium");

    // Astra has no medium: the level falls back to one it supports.
    await user.selectOptions(model, "gpt-6-astra");
    expect(reasoning).toHaveValue("low");
    await user.selectOptions(model, "gpt-6.1-sol");
    await user.selectOptions(reasoning, "high");

    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "claude");
    expect(model).toHaveValue("");
    expect(
      within(model)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual(["Predeterminado", "Opus", "Sonnet", "Fable", "Haiku"]);
    expect(reasoning).toHaveValue("medium");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Agente" }), "codex");
    await user.type(
      within(dialog).getByPlaceholderText("Qué debe hacer esta etapa y cómo comprobarlo."),
      "Write the tests",
    );
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() =>
      expect(client.dispatchDeliveryJob).toHaveBeenCalledWith(
        expect.objectContaining({ agent: "codex", model: "gpt-6.1-sol", reasoning: "medium" }),
      ),
    );
    // The stage keeps what it was launched with.
    expect(client.setDeliveryRepoSettings).toHaveBeenCalledWith(
      REPO,
      expect.objectContaining({
        stages: { tests: { agent: "codex", model: "gpt-6.1-sol", reasoning: "medium" } },
      }),
    );
    localStorage.removeItem("tinto.agents.lastRuntimeCatalog");
  });

  it("starts each stage from its own agent, else from the task's last one", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({ jobs: [job({ agent: "claude", model: "sonnet", reasoning: "high" })] }),
    );
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      stages: { review: { agent: "claude", model: "opus", reasoning: "xhigh" } },
    });
    client.dispatchDeliveryJob.mockResolvedValue(job({ id: "j2", status: "queued" }));
    renderPanel();
    expect(await screen.findByText(/Claude Code · sonnet · /)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Lanzar implementación" }));
    const dialog = screen.getByRole("dialog", { name: "Lanzar etapa en AGOS-501" });
    const model = within(dialog).getByRole("combobox", { name: "Modelo" });
    // Implementation has no agent of its own: the one the task last ran with.
    await waitFor(() => expect(model).toHaveValue("sonnet"));
    expect(within(dialog).getByRole("combobox", { name: "Razonamiento" })).toHaveValue("high");
    await user.selectOptions(within(dialog).getByRole("combobox", { name: /Etapa/ }), "review");
    expect(model).toHaveValue("opus");
    expect(within(dialog).getByRole("combobox", { name: "Razonamiento" })).toHaveValue("xhigh");
    await user.click(within(dialog).getByRole("button", { name: "Poner en cola" }));
    await waitFor(() =>
      expect(client.dispatchDeliveryJob).toHaveBeenCalledWith(
        expect.objectContaining({ role: "review", agent: "claude", model: "opus" }),
      ),
    );
    // Nothing changed for the stage, so nothing is saved.
    expect(client.setDeliveryRepoSettings).not.toHaveBeenCalled();
  });

  it("records a PR with or without its link, and the delivery goes on", async () => {
    const user = userEvent.setup();
    const approval = (over: Partial<DeliveryApproval>): DeliveryApproval => ({
      id: "a1",
      task_id: "t1",
      rung: "commit",
      title: "x",
      body: "",
      status: "executed",
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "hecho",
      ...over,
    });
    const upTo = (rungs: DeliveryApproval[]) =>
      overview({ tasks: [task({ state: "intake" })], approvals: rungs });
    client.getDeliveryOverview.mockResolvedValue(
      upTo([
        approval({}),
        approval({ id: "a2", rung: "push" }),
        approval({ id: "a3", rung: "pr", status: "approved", outcome: null }),
      ]),
    );
    client.completeDeliveryApproval.mockResolvedValue({});
    renderPanel();
    const banner = await openStep("Ver PR", "pr");
    expect(screen.getByText("Estado: Entrada")).toBeInTheDocument();
    const done = within(banner).getByRole("button", { name: "Marcar como hecho" });
    const outcome = within(banner).getByRole("textbox", { name: "Resultado del paso" });
    // Without a URL Tinto looks for the branch's PR; with one it checks it.
    expect(done).toBeEnabled();
    await user.type(outcome, "https://github.com/acme/agentos/pull/279");
    const upToPrJira = [
      approval({}),
      approval({ id: "a2", rung: "push" }),
      approval({ id: "a3", rung: "pr" }),
      approval({ id: "a4", rung: "jira", stage: "after_pr" }),
    ];
    client.getDeliveryOverview.mockResolvedValue(upTo(upToPrJira));
    await user.click(done);
    await waitFor(() =>
      expect(client.completeDeliveryApproval).toHaveBeenCalledWith(
        "a3",
        true,
        "https://github.com/acme/agentos/pull/279",
      ),
    );
    // The PR's Jira step is not the delivery: the merge and its Jira step are left.
    const ladder = await screen.findByRole("list", { name: "Entrega" });
    expect(within(ladder).getByText("Jira (PR)")).toBeInTheDocument();
    expect(within(ladder).getByText("Jira (merge)")).toBeInTheDocument();
    expect(screen.getByText("Estado: Entrada")).toBeInTheDocument();
  });

  it("shows a sync with the base in the ladder only once the task has one", async () => {
    const step = (over: Partial<DeliveryApproval>): DeliveryApproval => ({
      id: "a1",
      task_id: "t1",
      rung: "commit",
      title: "x",
      body: "",
      status: "executed",
      requested_by: "coord",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "Commit abc1234",
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({ tasks: [task({ state: "intake" })], approvals: [step({})] }),
    );
    const { unmount } = renderPanel();
    let ladder = await screen.findByRole("list", { name: "Entrega" });
    expect(within(ladder).queryByText("Sync con la base")).toBeNull();
    unmount();

    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          step({}),
          step({
            id: "a2",
            rung: "sync",
            title: "Merge develop",
            outcome: "origin/develop (abc1234) tiene conflictos con la rama en: shared.txt.",
            conflicts: ["shared.txt"],
          }),
          step({ id: "a3", rung: "sync", status: "pending", executed_at_ms: null, outcome: null }),
        ],
      }),
    );
    renderPanel();
    ladder = await screen.findByRole("list", { name: "Entrega" });
    const names = within(ladder)
      .getAllByRole("listitem")
      .map((item) => item.querySelector(".delivery-node__name")?.textContent);
    expect(names.slice(0, 3)).toEqual(["Commit", "Sync con la base", "Push"]);
    // The latest sync is the one shown: it waits on the user.
    expect(within(ladder).getByText("Tu aprobación")).toBeInTheDocument();
    const banner = await openStep("Revisar sync con la base", "sync");
    expect(within(banner).getByRole("button", { name: "Aprobar y sincronizar" })).toBeEnabled();
    expect(within(banner).getByText(/el merge queda en curso/)).toBeInTheDocument();
  });

  it("marks a sync stopped on conflicts in the ladder", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          {
            id: "a2",
            task_id: "t1",
            rung: "sync",
            title: "Merge develop",
            body: "",
            status: "executed",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_at_ms: 2,
            executed_at_ms: 3,
            outcome: "conflictos",
            conflicts: ["shared.txt", "a.ts"],
          },
        ],
      }),
    );
    renderPanel();
    const ladder = await screen.findByRole("list", { name: "Entrega" });
    expect(within(ladder).getByText("Conflictos en shared.txt, a.ts")).toBeInTheDocument();
  });

  it("waits on the coordinator after a PR step is approved, without holding the user", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          {
            id: "a3",
            task_id: "t1",
            rung: "pr",
            title: "feat: x",
            body: "",
            status: "approved",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_at_ms: 2,
            executed_at_ms: null,
            outcome: null,
            notice:
              "Tinto no pudo abrir el PR (GitHub no está conectado): el coordinador lo abre con este título y cuerpo y lo informa",
          },
        ],
      }),
    );
    renderPanel();
    // Nothing opens by itself: the step is one line in the task.
    await screen.findByRole("button", { name: "Ver PR" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const banner = await openStep("Ver PR", "pr");
    expect(within(banner).getByText("Esperando al coordinador: PR")).toBeInTheDocument();
    expect(within(banner).getByText(/Tinto no pudo abrir el PR/)).toBeInTheDocument();
    // Recording it by hand is a correction tucked away, not a prompt.
    expect(
      within(banner)
        .getByText("Lo hice fuera de Tinto: registrar el paso hecho")
        .closest("details"),
    ).not.toHaveAttribute("open");
    // Tinto can open it itself, with the approved text.
    await userEvent
      .setup()
      .click(within(banner).getByRole("button", { name: "Abrir el PR en GitHub" }));
    expect(client.openDeliveryPr).toHaveBeenCalledWith("a3");
    expect(screen.getByRole("button", { name: /^(Otra etapa|Lanzar etapa)…$/ })).toBeEnabled();
  });

  it("says what a rejected Jira step leaves open and offers to close the PR", async () => {
    const user = userEvent.setup();
    const step = (
      id: string,
      rung: DeliveryApproval["rung"],
      status: DeliveryApproval["status"],
      at: number,
      stage: DeliveryApproval["stage"] = null,
    ): DeliveryApproval => ({
      id,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status,
      requested_by: "coord",
      requested_at_ms: at,
      decided_at_ms: at,
      executed_at_ms: at,
      outcome: "hecho",
      stage,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          step("a1", "commit", "executed", 1),
          step("a2", "push", "executed", 2),
          step("a3", "pr", "executed", 3),
          step("a4", "jira", "rejected", 4, "after_pr"),
        ],
        prs: [
          {
            task_id: "t1",
            url: "https://github.com/o/r/pull/299",
            repo: "o/r",
            number: 299,
            state: "open",
            draft: false,
            conflicts: false,
            review_decision: null,
            checks: { passing: 0, failing: 0, pending: 0 },
            new_reviews: 0,
            new_comments: 0,
            new_authors: [],
            latest_activity: null,
            seen_until: null,
            checked_at_ms: 1,
            error: null,
          },
        ],
      }),
    );
    renderPanel();
    const notice = await screen.findByRole("group", { name: "Paso rechazado" });
    // The task's row in the list names its PR.
    const list = screen.getByRole("navigation", { name: "Tareas de Delivery" });
    expect(within(list).getByTitle("PR #299")).toHaveTextContent("#299");
    expect(
      within(notice).getByText(
        "El PR #299 sigue abierto en GitHub; rechazar este paso solo evita los cambios en Jira.",
      ),
    ).toBeInTheDocument();
    await user.click(within(notice).getByRole("button", { name: "Pasar el PR a borrador" }));
    expect(client.deliveryPrToDraft).toHaveBeenCalledWith("t1");
    await user.click(within(notice).getByRole("button", { name: "Cerrar el PR #299" }));
    expect(client.closeDeliveryPr).toHaveBeenCalledWith("t1");
  });

  it("says before rejecting a step what stays as it is", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          {
            id: "a2",
            task_id: "t1",
            rung: "push",
            title: "Publicar",
            body: "",
            status: "pending",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_at_ms: null,
            executed_at_ms: null,
            outcome: null,
          },
        ],
      }),
    );
    renderPanel();
    const banner = await openStep("Revisar push", "push");
    expect(
      within(banner).getByText(/Rechazar no deshace los commits: siguen en .*, sin publicar\./),
    ).toBeInTheDocument();
  });

  it("shows why a single commit holds the whole change", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          {
            id: "a1",
            task_id: "t1",
            rung: "commit",
            title: "feat: all",
            body: "",
            status: "pending",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_at_ms: null,
            executed_at_ms: null,
            outcome: null,
            commits: [{ message: "feat: all", paths: ["src/a.rs", "tests/a.rs"] }],
            single_commit_reason: "the test only compiles with the change",
          },
        ],
      }),
    );
    renderPanel();
    const banner = await openStep("Revisar commit", "commit");
    expect(
      within(banner).getByText("Un solo commit porque: the test only compiles with the change"),
    ).toBeInTheDocument();
  });

  it("hands the batch to another Agents conversation of its repo", async () => {
    const user = userEvent.setup();
    const session = (id: string, repo: string, status: AgentSession["status"]): AgentSession =>
      ({
        id,
        repo,
        agent_type: "muse",
        status,
        pid: 1,
        started_at_ms: 1,
        exit_code: null,
        error: null,
        turn_status: "waiting",
      }) as AgentSession;
    agentSessionStore.setSessions([
      session("current-coordinator", "C:/work/agentos", "running"),
      session("next-coordinator", "C:\\work\\agentos\\", "running"),
      session("other-repo", "C:/work/elsewhere", "running"),
      session("ended", "C:/work/agentos", "exited"),
    ]);
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        runs: [
          {
            id: "run-1",
            repo: "C:/work/agentos",
            title: "Backlog",
            status: "active",
            generation: 1,
            owner: "coord",
            created_at_ms: 1,
            updated_at_ms: 1,
            coordinator_session: "current-coordinator",
          },
        ],
      }),
    );
    renderPanel();
    await user.click(await screen.findByRole("button", { name: /Cambiar coordinador/ }));
    const choices = screen.getAllByRole("menuitem");
    expect(choices.map((choice) => choice.textContent)).toEqual(["Muse Code · next-coo"]);
    await user.click(choices[0]);
    expect(client.handOverDeliveryRun).toHaveBeenCalledWith(["run-1"], "next-coordinator");
    agentSessionStore.setSessions([]);
  });

  it("opens the QA's visual record from the task's header", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [job({ id: "qa-1", role: "qa", visual_record: "C:/jobs/qa-1/qa/AGOS-501.html" })],
      }),
    );
    renderPanel();
    const button = await screen.findByRole("button", { name: "Abrir registro visual de QA" });
    // In the task's header, next to Jira and the PR.
    expect(button.closest(".delivery-task__heading")).not.toBeNull();
    expect(button.querySelector(".delivery-task__record-count")).toBeNull();
    await user.click(button);
    expect(client.openDeliveryVisualRecord).toHaveBeenCalledWith("qa-1");
    expect(screen.getByText("Con registro visual ↓")).toBeInTheDocument();
  });

  it("counts on the visual record's button the QA problems the user has to decide", async () => {
    const problem = (id: string, status: DeliveryQaProblem["status"]): DeliveryQaProblem => ({
      id,
      task_id: "t1",
      job_id: "qa-1",
      attempt: 1,
      number: 1,
      label: "P1",
      change: "",
      place: "",
      text: id,
      suggestions: [],
      status,
      solution: null,
      reason: null,
      notes: [],
      created_at_ms: 1,
      updated_at_ms: 1,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [job({ id: "qa-1", role: "qa", visual_record: "C:/jobs/qa-1/qa/AGOS-501.html" })],
        qa_problems: [
          problem("a", "open"),
          problem("b", "still_failing"),
          problem("c", "sent"),
          problem("d", "verified"),
        ],
      }),
    );
    renderPanel();
    expect(
      await screen.findByRole("button", { name: "Abrir registro visual de QA" }),
    ).toBeEnabled();
    const problems = screen.getByRole("button", { name: "Problemas de la QA, 2 por decidir" });
    expect(within(problems).getByText("2")).toBeInTheDocument();
    expect(problems).toHaveAttribute("aria-expanded", "false");
  });

  it("says when the QA's visual record could not be made", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [job({ id: "qa-1", role: "qa", visual_record_error: "faltan las capturas" })],
      }),
    );
    renderPanel();
    const banner = await screen.findByRole("group", { name: "Registro visual de QA" });
    expect(within(banner).getByText("faltan las capturas")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /registro visual de QA/ })).toBeNull();
  });

  it("opens the task's PR on GitHub from its header", async () => {
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        prs: [
          {
            task_id: "t1",
            url: "https://github.com/o/r/pull/5",
            repo: "o/r",
            number: 5,
            state: "open",
            draft: false,
            conflicts: false,
            review_decision: null,
            checks: { passing: 0, failing: 0, pending: 0 },
            new_reviews: 0,
            new_comments: 0,
            new_authors: [],
            latest_activity: null,
            seen_until: null,
            checked_at_ms: 1,
            error: null,
          },
        ],
      }),
    );
    renderPanel();
    const link = await screen.findByRole("link", { name: "Abrir el PR #5 en GitHub" });
    expect(link).toHaveAttribute("href", "https://github.com/o/r/pull/5");
  });

  it("approves a Jira step with nothing to fill in", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          {
            id: "a4",
            task_id: "t1",
            rung: "jira",
            title: "AGOS-474 a In Review",
            body: "",
            status: "pending",
            requested_by: "coord",
            requested_at_ms: 1,
            decided_at_ms: null,
            executed_at_ms: null,
            outcome: null,
            stage: "after_pr",
          },
        ],
      }),
    );
    client.decideDeliveryApproval.mockResolvedValue({});
    renderPanel();
    const banner = await openStep("Revisar Jira (PR)", "jira");
    // Approving asks for nothing: the coordinator does the step and reports it.
    expect(within(banner).queryByRole("textbox")).not.toBeInTheDocument();
    const approve = within(banner).getByRole("button", { name: "Aprobar" });
    expect(approve).toBeEnabled();
    await user.click(approve);
    expect(client.decideDeliveryApproval).toHaveBeenCalledWith("a4", true);
  });

  it("says a task is delivered only once both Jira steps are done", async () => {
    const done = (id: string, rung: DeliveryApproval["rung"], stage?: "after_merge") => ({
      id,
      task_id: "t1",
      rung,
      stage,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "hecho",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [task({ state: "intake" })],
        approvals: [
          done("a1", "commit"),
          done("a2", "push"),
          done("a3", "pr"),
          done("a4", "jira"),
          done("a5", "merge"),
          done("a6", "jira", "after_merge"),
        ],
      }),
    );
    renderPanel();
    // The coordinator never moved its label; the delivery still shows.
    expect(await screen.findByText("Estado: Entregada")).toBeInTheDocument();
  });

  it("warns of a PR step done without a linked PR and looks for the branch's PR", async () => {
    const user = userEvent.setup();
    const executed = (rung: "commit" | "push" | "pr") => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "hecho",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({ approvals: [executed("commit"), executed("push"), executed("pr")] }),
    );
    renderPanel();
    expect(await screen.findByText("Hecho · sin PR vinculado")).toBeInTheDocument();
    const checks = screen.getByRole("group", { name: "Comprobaciones de la entrega" });
    await user.click(within(checks).getByRole("button", { name: "Buscar el PR de la rama" }));
    await waitFor(() => expect(client.findDeliveryPr).toHaveBeenCalledWith("t1"));
  });

  it("says when a push no longer matches the remote and asks for it again", async () => {
    const user = userEvent.setup();
    const step = (rung: "commit" | "push", over: Partial<DeliveryApproval> = {}) => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "ok",
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [],
        phases: { t1: phase({ kind: "drifted", rung: "push", stage: null }) },
        approvals: [
          step("commit"),
          step("push", { status: "drifted", notice: "La rama remota ya no existe" }),
        ],
      }),
    );
    client.requestDeliveryApproval.mockResolvedValue({});
    renderPanel();
    expect(await screen.findByText("Ya no coincide")).toBeInTheDocument();
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText(
        "Push: ya no coincide",
      ),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "Comprobaciones de la entrega" })).getByText(
        "Push: ya no coincide. La rama remota ya no existe.",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Volver a pedir el push…" }));
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

  it("corrects a step's result with a reason and keeps the previous one", async () => {
    const user = userEvent.setup();
    const step = (rung: "commit" | "push" | "pr", over: Partial<DeliveryApproval> = {}) => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "ok",
      ...over,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [
          step("commit"),
          step("push"),
          step("pr", {
            outcome: "https://github.com/acme/agentos/pull/279",
            corrections: [
              {
                previous: "PR abierto",
                outcome: "https://github.com/acme/agentos/pull/279",
                reason: "Faltaba la URL",
                at_ms: 5,
              },
            ],
          }),
        ],
      }),
    );
    client.correctDeliveryApproval.mockResolvedValue({});
    renderPanel();
    const results = await screen.findByRole("region", { name: "Resultados de la entrega" });
    const pr = within(results).getByRole("listitem", { name: "PR" });
    expect(within(pr).getByText("PR abierto")).toBeInTheDocument();
    expect(within(pr).getByText(/Por qué: Faltaba la URL/)).toBeInTheDocument();
    // Commit and push are checked against git: no correction for them.
    expect(
      within(within(results).getByRole("listitem", { name: "Push" })).queryByRole("button"),
    ).toBeNull();

    await user.click(screen.getByRole("button", { name: "Más acciones de la tarea" }));
    await user.click(screen.getByRole("menuitem", { name: "Corregir resultado…" }));
    const dialog = screen.getByRole("dialog", { name: "Corregir resultado" });
    const outcome = within(dialog).getByRole("textbox", { name: "Resultado" });
    expect(outcome).toHaveValue("https://github.com/acme/agentos/pull/279");
    await user.clear(outcome);
    await user.type(outcome, "https://github.com/acme/agentos/pull/280");
    await user.click(within(dialog).getByRole("button", { name: "Guardar corrección" }));
    expect(client.correctDeliveryApproval).not.toHaveBeenCalled();
    await user.type(
      within(dialog).getByRole("textbox", { name: "Por qué se corrige" }),
      "Era otro PR",
    );
    await user.click(within(dialog).getByRole("button", { name: "Guardar corrección" }));
    await waitFor(() =>
      expect(client.correctDeliveryApproval).toHaveBeenCalledWith(
        "pr",
        "https://github.com/acme/agentos/pull/280",
        "Era otro PR",
      ),
    );
  });

  it("names the next step and proposes the base's merge method with GitHub's message", async () => {
    const user = userEvent.setup();
    const done = (rung: DeliveryApproval["rung"]) => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: rung === "pr" ? "https://github.com/o/r/pull/5" : "hecho",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: ["tests", "implementation", "review", "qa"].map((role, index) =>
          job({ id: `j${index}`, role }),
        ),
        approvals: [done("commit"), done("push"), done("pr"), done("jira")],
        phases: { t1: phase({ kind: "pr", number: 279 }, "coordinator") },
        prs: [
          {
            task_id: "t1",
            url: "https://github.com/o/r/pull/5",
            repo: "o/r",
            number: 5,
            state: "open",
            draft: false,
            conflicts: false,
            review_decision: "approved",
            checks: { passing: 3, failing: 0, pending: 0 },
            new_reviews: 0,
            new_comments: 0,
            new_authors: [],
            latest_activity: null,
            seen_until: null,
            checked_at_ms: 1,
            error: null,
          },
        ],
      }),
    );
    client.getDeliveryMergeMethod.mockResolvedValueOnce("merge");
    renderPanel();
    expect(
      await screen.findByText("Todas las etapas pasaron. Lo siguiente es el merge."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Preparar merge…" }));
    const form = screen.getByRole("form", { name: "Pedir aprobación de merge" });
    await waitFor(() =>
      expect(within(form).getByRole("combobox", { name: "Cómo fusionar" })).toHaveValue("merge"),
    );
    expect(within(form).getByRole("textbox", { name: "Mensaje del commit de fusión" })).toHaveValue(
      "Merge pull request #5 from o/delivery/agos-501",
    );
    expect(client.getDeliveryMergeMethod).toHaveBeenCalledWith("t1");
  });

  it("approves the PR step only with a reviewer chosen from GitHub's list", async () => {
    const user = userEvent.setup();
    const step = (rung: DeliveryApproval["rung"], status: DeliveryApproval["status"]) => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung === "pr" ? "AGOS-501: Fix the thing" : rung,
      body: "",
      status,
      requested_by: "coord",
      requested_at_ms: 1,
      decided_at_ms: null,
      executed_at_ms: null,
      outcome: null,
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [step("commit", "executed"), step("push", "executed"), step("pr", "pending")],
      }),
    );
    client.getDeliveryPrReviewers.mockResolvedValueOnce(["ana", "luis"]);
    client.decideDeliveryApproval.mockResolvedValue({});
    renderPanel();
    const banner = await openStep("Revisar PR", "pr");
    const reviewers = within(banner).getByRole("group", { name: "Revisores" });
    const approve = within(banner).getByRole("button", { name: "Aprobar y abrir el PR" });
    await within(reviewers).findByRole("checkbox", { name: "ana" });
    expect(approve).toBeDisabled();
    await user.click(within(reviewers).getByRole("checkbox", { name: "luis" }));
    await user.click(approve);
    await waitFor(() =>
      expect(client.decideDeliveryApproval).toHaveBeenCalledWith("pr", true, undefined, ["luis"]),
    );
    expect(client.getDeliveryPrReviewers).toHaveBeenCalledWith("t1");
  });

  it("asks the closing Jira step with the repo's statuses when Tinto runs Jira", async () => {
    const user = userEvent.setup();
    const done = (rung: DeliveryApproval["rung"]) => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "hecho",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        approvals: [done("commit"), done("push"), done("pr"), done("jira"), done("merge")],
      }),
    );
    client.getDeliveryRepoSettings.mockResolvedValue({
      worktree_root: null,
      bootstrap: null,
      default_base: null,
      jira: {
        host: "https://jira.example.com",
        project: "AGOS",
        after_pr: ["En ejecución", "In Review"],
        after_merge: ["Cerrada"],
        has_token: true,
      },
    });
    client.requestDeliveryApproval.mockResolvedValue({});
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Preparar Jira (merge)…" }));
    const form = screen.getByRole("form", { name: "Pedir aprobación de Jira (merge)" });
    const statuses = await within(form).findByRole("textbox", {
      name: "Estados de AGOS-501 en Jira, en orden",
    });
    expect(statuses).toHaveValue("Cerrada");
    await user.click(within(form).getByRole("button", { name: "Pedir aprobación" }));
    await waitFor(() =>
      expect(client.requestDeliveryApproval).toHaveBeenCalledWith(
        "t1",
        "jira",
        "AGOS-501: Fix the thing",
        "",
        undefined,
        undefined,
        "after_merge",
        ["Cerrada"],
      ),
    );
  });

  it("archives a delivered task and brings one back by its PR number", async () => {
    const user = userEvent.setup();
    const done = (id: string, rung: DeliveryApproval["rung"], stage?: "after_merge") => ({
      id,
      task_id: "t1",
      rung,
      stage,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: "hecho",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        jobs: [],
        approvals: [
          done("a1", "commit"),
          done("a2", "push"),
          done("a3", "pr"),
          done("a4", "jira"),
          done("a5", "merge"),
          done("a6", "jira", "after_merge"),
        ],
      }),
    );
    client.archiveDeliveryTask.mockResolvedValue(undefined);
    client.getArchivedDeliveryTasks.mockResolvedValue([
      {
        task: task({ id: "t9", key: "AGOS-455", title: "Skills" }),
        pr_number: 279,
        pr_url: "https://github.com/o/r/pull/279",
      },
    ]);
    client.unarchiveDeliveryTask.mockResolvedValue(task({ id: "t9", key: "AGOS-455" }));
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Archivar tarea…" }));
    await waitFor(() => expect(client.archiveDeliveryTask).toHaveBeenCalledWith("t1"));

    await user.click(screen.getByRole("button", { name: "Archivadas…" }));
    const dialog = screen.getByRole("dialog", { name: "Tareas archivadas" });
    await user.type(within(dialog).getByRole("textbox", { name: /Issue o número de PR/ }), "#279");
    await user.click(within(dialog).getByRole("button", { name: "Buscar" }));
    await waitFor(() => expect(client.getArchivedDeliveryTasks).toHaveBeenCalledWith("#279"));
    const found = await within(dialog).findByRole("listitem", { name: "AGOS-455" });
    expect(within(found).getByRole("link", { name: "PR #279" })).toBeInTheDocument();
    await user.click(within(found).getByRole("button", { name: "Desarchivar" }));
    await waitFor(() => expect(client.unarchiveDeliveryTask).toHaveBeenCalledWith("t9"));
  });

  it("shows one batch's tasks and folds the delivered ones", async () => {
    const user = userEvent.setup();
    const batch = (id: string, title: string) => ({
      id,
      repo: REPO,
      title,
      status: "active",
      generation: 1,
      owner: null,
      created_at_ms: 1,
      updated_at_ms: 1,
    });
    const delivered = phase({ kind: "delivered" }, "nobody");
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        runs: [batch("r1", "Lote A"), batch("r2", "Lote B")],
        tasks: [
          task({ id: "t1", key: "AGOS-1", run_id: "r1" }),
          task({ id: "t2", key: "AGOS-2", run_id: "r1" }),
          task({ id: "t3", key: "AGOS-3", run_id: "r2" }),
        ],
        phases: { t1: delivered, t2: NEXT_IMPLEMENTATION, t3: NEXT_IMPLEMENTATION },
        jobs: [],
      }),
    );
    renderPanel();
    const list = await screen.findByRole("navigation", { name: "Tareas de Delivery" });
    await within(list).findByText("AGOS-2");
    const folded = within(list).getByRole("button", { name: /Entregadas/ });
    expect(folded).toHaveAttribute("aria-expanded", "false");
    expect(within(list).queryByText("AGOS-1")).toBeNull();
    await user.click(folded);
    expect(within(list).getByText("AGOS-1")).toBeInTheDocument();

    await user.selectOptions(within(list).getByRole("combobox", { name: "Lote" }), "r2");
    expect(within(list).getByText("AGOS-3")).toBeInTheDocument();
    expect(within(list).queryByText("AGOS-1")).toBeNull();
    expect(within(list).queryByText("AGOS-2")).toBeNull();
    expect(JSON.parse(localStorage.getItem("tinto:delivery-list-view")!)).toEqual({
      lot: "r2",
      folded: [],
    });
  });

  it("archives every delivered task in one go", async () => {
    const user = userEvent.setup();
    const delivered = phase({ kind: "delivered" }, "nobody");
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        tasks: [
          task({ id: "t1", key: "AGOS-1" }),
          task({ id: "t2", key: "AGOS-2" }),
          task({ id: "t3", key: "AGOS-3" }),
        ],
        phases: { t1: delivered, t2: delivered, t3: NEXT_IMPLEMENTATION },
        jobs: [],
      }),
    );
    client.archiveDeliveryTask.mockResolvedValue(undefined);
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Archivar todas" }));
    expect(confirmMock).toHaveBeenCalledWith(
      expect.stringContaining("Se archivan 2 tareas entregadas"),
      expect.anything(),
    );
    await waitFor(() => expect(client.archiveDeliveryTask).toHaveBeenCalledTimes(2));
    expect(client.archiveDeliveryTask).toHaveBeenCalledWith("t1");
    expect(client.archiveDeliveryTask).toHaveBeenCalledWith("t2");
  });

  it("gives outside agents tinto.exe as their MCP server, which opens Tinto", async () => {
    const user = userEvent.setup();
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        coordinator: {
          url: "http://127.0.0.1:47920/mcp",
          token: "secret",
          command: "C:\\Users\\me\\AppData\\Local\\tinto\\tinto.exe",
        },
      }),
    );
    renderPanel();
    await user.click(await screen.findByRole("button", { name: "Ajustes de Delivery" }));
    const settings = screen.getByRole("dialog", { name: "Ajustes de Delivery" });
    expect(settings).not.toHaveTextContent("secret");
    await user.click(
      within(settings).getByRole("button", { name: "Copiar la configuración de Claude Code" }),
    );
    expect(await navigator.clipboard.readText()).toBe(
      'claude mcp add tinto-delivery -- "C:/Users/me/AppData/Local/tinto/tinto.exe" --delivery-mcp-proxy',
    );
  });

  it("shows the coordinator's own calls apart, without blocking, and lets the user change one", async () => {
    const user = userEvent.setup();
    const own: DeliveryDecision = {
      id: "d9",
      task_id: "t1",
      kind: "choice",
      question: "¿Formato del manifiesto?",
      detail: "",
      options: [
        { label: "Mantener y añadir copias", consequence: "Los scripts siguen", recommended: true },
        { label: "Formato nuevo", consequence: "Rompe scripts", recommended: false },
      ],
      text: "",
      command: null,
      undo: "",
      blocking: false,
      status: "answered",
      answer: "Mantener y añadir copias",
      requested_by: "coord",
      requested_at_ms: 1,
      decided_by: "coord",
      decided_at_ms: 2,
    };
    client.getDeliveryOverview.mockResolvedValue(overview({ decisions: [own] }));
    client.changeDeliveryDecision.mockResolvedValue({});
    renderPanel();
    const section = await screen.findByRole("region", { name: "Decidido por el coordinador" });
    expect(screen.queryByRole("region", { name: "Te necesitan" })).toBeNull();
    expect(screen.getByRole("button", { name: /Lanzar implementación/ })).toBeEnabled();
    await user.click(
      within(section).getByRole("button", { name: "Cambiar «¿Formato del manifiesto?»" }),
    );
    await user.click(within(section).getByRole("radio", { name: /Formato nuevo/ }));
    await user.click(within(section).getByRole("button", { name: "Guardar" }));
    await waitFor(() =>
      expect(client.changeDeliveryDecision).toHaveBeenCalledWith("d9", "Formato nuevo"),
    );
  });

  it("follows the task's PR and merges it through its own step", async () => {
    const user = userEvent.setup();
    const pr = {
      task_id: "t1",
      url: "https://github.com/o/r/pull/5",
      repo: "o/r",
      number: 5,
      state: "open",
      draft: false,
      conflicts: false,
      review_decision: "approved",
      checks: { passing: 2, failing: 0, pending: 0 },
      new_reviews: 1,
      new_comments: 2,
      new_authors: ["ana", "luis"],
      latest_activity: null,
      seen_until: null,
      checked_at_ms: 1,
      error: null,
    };
    const executed = (rung: "commit" | "push" | "pr" | "jira") => ({
      id: rung,
      task_id: "t1",
      rung,
      title: rung,
      body: "",
      status: "executed" as const,
      requested_by: "user",
      requested_at_ms: 1,
      decided_at_ms: 2,
      executed_at_ms: 3,
      outcome: rung === "pr" ? "https://github.com/o/r/pull/5" : "ok",
    });
    client.getDeliveryOverview.mockResolvedValue(
      overview({
        prs: [pr],
        phases: { t1: phase({ kind: "pr", number: pr.number }) },
        approvals: [executed("commit"), executed("push"), executed("pr"), executed("jira")],
      }),
    );
    client.markDeliveryPrSeen.mockResolvedValue(null);
    client.requestDeliveryApproval.mockResolvedValue({});
    renderPanel();

    const section = await screen.findByRole("region", { name: "Pull request" });
    expect(within(section).getByRole("link", { name: "o/r#5" })).toHaveAttribute(
      "href",
      "https://github.com/o/r/pull/5",
    );
    expect(within(section).getByRole("status")).toHaveTextContent(
      "1 revisión nueva y 2 comentarios nuevos de ana, luis",
    );
    await waitFor(() => expect(client.checkDeliveryTask).toHaveBeenCalledWith("t1"));
    await user.click(within(section).getByRole("button", { name: "Marcar como visto" }));
    await waitFor(() => expect(client.markDeliveryPrSeen).toHaveBeenCalledWith("t1"));
    expect(
      within(screen.getByRole("region", { name: "Te necesitan" })).getByText("PR: 3 novedades"),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Preparar merge…" }));
    const form = screen.getByRole("form", { name: "Pedir aprobación de merge" });
    await user.selectOptions(
      within(form).getByRole("combobox", { name: "Cómo fusionar" }),
      "rebase",
    );
    await user.click(within(form).getByRole("button", { name: "Pedir aprobación" }));
    await waitFor(() =>
      expect(client.requestDeliveryApproval).toHaveBeenCalledWith(
        "t1",
        "merge",
        "AGOS-501: Fix the thing",
        "",
        undefined,
        "rebase",
      ),
    );
  });
});
