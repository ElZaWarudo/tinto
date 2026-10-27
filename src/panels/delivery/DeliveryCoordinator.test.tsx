import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { DeliveryOverview, DeliveryRun } from "../../delivery/types";
import { CoordinatorDock } from "./DeliveryCoordinator";

const sessions = vi.hoisted(() => ({ value: {} as Record<string, unknown> }));
vi.mock("../../agent/sessionStore", () => ({
  useAgentSessionState: () => ({ sessions: sessions.value }),
}));
const bus = vi.hoisted(() => ({ startAgentSession: vi.fn(() => Promise.resolve("s-new")) }));
vi.mock("../../bus/client", () => bus);
const client = vi.hoisted(() => ({
  handOverDeliveryRun: vi.fn(() => Promise.resolve()),
  setDeliveryRunNoNewTasks: vi.fn(() => Promise.resolve({})),
}));
vi.mock("../../delivery/client", () => client);
const windows = vi.hoisted(() => ({
  openDetachedTerminalWindow: vi.fn(() => Promise.resolve(true)),
}));
vi.mock("../terminal/detachTerminalWindow", () => windows);
const catalog = vi.hoisted(() => ({ value: null as unknown }));
vi.mock("../terminal/agentRuntimeCatalog", () => ({
  lastRuntimeCatalog: () => catalog.value,
  codexReasoningLabel: (value: string) => value,
}));
vi.mock("../terminal/TerminalPanel", () => ({
  TerminalPanel: ({ params }: { params: { sessionId: string } }) => (
    <div>Conversación {params.sessionId}</div>
  ),
}));

const run = (over: Partial<DeliveryRun> = {}): DeliveryRun => ({
  id: "r1",
  repo: "C:\\work\\agentos",
  title: "Lote A",
  status: "active",
  generation: 1,
  owner: null,
  created_at_ms: 1,
  updated_at_ms: 1,
  ...over,
});

const overview = { coordinators: [] } as unknown as DeliveryOverview;

function session(id: string, turn_status: "waiting" | "working", reports_turns = true) {
  return {
    id,
    repo: "C:\\work\\agentos",
    agent_type: "claude",
    status: "running",
    turn_status,
    reports_turns,
  };
}

function Dock({
  runs,
  open = true,
  focus = null,
  stalled = [],
}: {
  runs: DeliveryRun[];
  open?: boolean;
  focus?: string | null;
  stalled?: string[];
}) {
  return (
    <CoordinatorDock
      runs={runs}
      focus={focus}
      overview={overview}
      stalled={stalled}
      open={open}
      onOpenChange={() => {}}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  sessions.value = {};
  catalog.value = null;
  localStorage.removeItem("tinto:delivery-coordinator-agent");
});

describe("CoordinatorDock", () => {
  it("shows the batch's coordinator, its state, and its conversation folded", async () => {
    const user = userEvent.setup();
    sessions.value = { s1: session("s1", "working") };
    render(<Dock runs={[run({ coordinator_session: "s1", owner: "claude-1" })]} />);
    expect(
      screen.getByRole("button", { name: "Coordinador de Lote A: trabajando" }),
    ).toBeInTheDocument();
    expect(screen.getByText("· Claude Code")).toBeInTheDocument();
    // The conversation stays out of the way until the user opens it.
    const fold = screen.getByRole("button", { name: "Conversación" });
    expect(fold).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Conversación s1")).toBeNull();
    await user.click(fold);
    expect(screen.getByText("Conversación s1")).toBeInTheDocument();
  });

  it("offers a new coordinator once the batch's conversation ended", async () => {
    const user = userEvent.setup();
    sessions.value = { s1: { ...session("s1", "waiting"), status: "exited" } };
    render(<Dock runs={[run({ coordinator_session: "s1", owner: "claude-1" })]} />);
    expect(
      screen.getByRole("button", { name: "Coordinador de Lote A: sin coordinador" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Conversación" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    expect(bus.startAgentSession).toHaveBeenCalled();
  });

  it("starts Codex on the model it shows once its models load after the pick", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Dock runs={[run()]} />);
    // No catalog yet: Codex offers only its own default.
    await user.selectOptions(screen.getByRole("combobox", { name: "Agente" }), "codex");
    const effort = { value: "medium", label: "Medium", description: "" };
    catalog.value = {
      status: "ready",
      source: "codex_app_server",
      default_model: "gpt-6-luna",
      updated_at_ms: 1,
      models: ["gpt-6-luna", "gpt-5.6-terra"].map((model) => ({
        id: model,
        model,
        display_name: model,
        description: "",
        supported_reasoning_efforts: [effort],
        default_reasoning_effort: "medium",
        is_default: model === "gpt-6-luna",
      })),
    };
    rerender(<Dock runs={[run()]} />);
    expect(screen.getByRole("combobox", { name: "Modelo" })).toHaveValue("gpt-6-luna");
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    expect(bus.startAgentSession).toHaveBeenCalledWith(run().repo, "codex", "workspace", {
      model: "gpt-6-luna",
      reasoning_effort: "medium",
    });
  });

  it("starts a coordinator here and hands it the batch once it waits", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Dock runs={[run()]} />);
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    expect(bus.startAgentSession).toHaveBeenCalledWith(run().repo, "claude", "workspace", {
      reasoning_effort: "medium",
    });
    expect(client.handOverDeliveryRun).not.toHaveBeenCalled();

    // Claude Code still asks whether to trust the folder: no turns yet, and a
    // message typed now would be lost.
    sessions.value = { "s-new": session("s-new", "waiting", false) };
    rerender(<Dock runs={[run()]} />);
    await act(() => Promise.resolve());
    expect(client.handOverDeliveryRun).not.toHaveBeenCalled();

    sessions.value = { "s-new": session("s-new", "waiting") };
    rerender(<Dock runs={[run()]} />);
    await waitFor(() => expect(client.handOverDeliveryRun).toHaveBeenCalledWith(["r1"], "s-new"));
    expect(screen.getByRole("button", { name: "Conversación" })).toBeInTheDocument();
  });

  it("starts one coordinator for the batches the user ticks, the one opened from first", async () => {
    const user = userEvent.setup();
    const runs = [
      run({ id: "r1", title: "Lote A", owner: "claude-coord-1" }),
      run({ id: "r2", title: "Lote B" }),
      run({ id: "r3", title: "Lote C", owner: "codex-2" }),
    ];
    const { rerender } = render(<Dock runs={runs} focus="r2" stalled={["r3"]} />);
    const batches = screen.getByRole("group", { name: "Lotes que toma" });
    const a = within(batches).getByRole("checkbox", { name: /Lote A/ });
    const b = within(batches).getByRole("checkbox", { name: /Lote B/ });
    const c = within(batches).getByRole("checkbox", { name: /Lote C/ });
    // Each batch with its state; only the one the drawer shows is ticked.
    expect(a).not.toBeChecked();
    expect(b).toBeChecked();
    expect(c).not.toBeChecked();
    expect(within(batches).getByText("coordinado fuera de Tinto")).toBeInTheDocument();
    expect(within(batches).getByText("sin coordinador")).toBeInTheDocument();
    expect(within(batches).getByText("parado")).toBeInTheDocument();

    await user.click(a);
    await user.click(b);
    await user.click(c);
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    sessions.value = { "s-new": session("s-new", "waiting") };
    rerender(<Dock runs={runs} focus="r2" stalled={["r3"]} />);
    await waitFor(() =>
      expect(client.handOverDeliveryRun).toHaveBeenCalledWith(["r1", "r3"], "s-new"),
    );
  });

  it("starts nothing without a batch ticked", async () => {
    const user = userEvent.setup();
    render(<Dock runs={[run({ id: "r1" }), run({ id: "r2", title: "Lote B" })]} />);
    await user.click(screen.getByRole("checkbox", { name: /Lote A/ }));
    expect(screen.getByRole("button", { name: "Iniciar coordinador" })).toBeDisabled();
  });

  it("shows one conversation for every batch it holds, each with its own state", async () => {
    const user = userEvent.setup();
    sessions.value = { s1: session("s1", "waiting") };
    const runs = [
      run({ id: "r1", title: "Lote A", coordinator_session: "s1" }),
      run({ id: "r2", title: "Lote B", coordinator_session: "s1" }),
    ];
    render(<Dock runs={runs} stalled={["r2"]} />);
    expect(
      screen.getByText("Esta conversación coordina 2 lotes: Lote A · Lote B"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Coordinador de Lote A: esperando" }),
    ).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Lote" }), "r2");
    expect(
      screen.getByRole("button", { name: "Coordinador de Lote B: parado" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Conversación" }));
    expect(screen.getByText("Conversación s1")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Iniciar coordinador" })).toBeNull();
  });

  it("says when the batch is coordinated outside Tinto", () => {
    render(<Dock runs={[run({ owner: "claude-coord-1" })]} />);
    expect(screen.getByText("Fuera de Tinto")).toBeInTheDocument();
    expect(screen.getByText("· claude-coord-1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Iniciar coordinador" })).toBeInTheDocument();
  });

  it("stops new tasks and detaches the conversation", async () => {
    const user = userEvent.setup();
    sessions.value = { s1: session("s1", "waiting") };
    render(<Dock runs={[run({ coordinator_session: "s1" })]} />);
    const policy = screen.getByRole("switch", { name: "Crear tareas nuevas" });
    expect(policy).toHaveAttribute("aria-checked", "true");
    await user.click(policy);
    expect(client.setDeliveryRunNoNewTasks).toHaveBeenCalledWith("r1", true);
    await user.click(screen.getByRole("button", { name: "Abrir el coordinador en otra ventana" }));
    expect(windows.openDetachedTerminalWindow).toHaveBeenCalledWith(
      { sessionId: "s1", repo: "C:\\work\\agentos", agentType: "claude" },
      expect.any(Function),
    );
  });

  it("starts the coordinator on the chosen model and reasoning, and remembers them per repo", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<Dock runs={[run()]} />);
    await user.selectOptions(screen.getByRole("combobox", { name: "Modelo" }), "opus");
    await user.selectOptions(screen.getByRole("combobox", { name: "Razonamiento" }), "high");
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    expect(bus.startAgentSession).toHaveBeenCalledWith(run().repo, "claude", "workspace", {
      model: "opus",
      reasoning_effort: "high",
    });
    unmount();

    render(<Dock runs={[run({ id: "r2" })]} />);
    expect(screen.getByRole("combobox", { name: "Modelo" })).toHaveValue("opus");
    expect(screen.getByRole("combobox", { name: "Razonamiento" })).toHaveValue("high");
  });
});
