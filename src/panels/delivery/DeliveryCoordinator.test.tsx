import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
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

function session(id: string, turn_status: "waiting" | "working") {
  return { id, repo: "C:\\work\\agentos", agent_type: "claude", status: "running", turn_status };
}

function Dock({ runs, open = true }: { runs: DeliveryRun[]; open?: boolean }) {
  return (
    <CoordinatorDock
      runs={runs}
      overview={overview}
      stalled={[]}
      open={open}
      onOpenChange={() => {}}
    />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  sessions.value = {};
});

describe("CoordinatorDock", () => {
  it("shows the batch's coordinator conversation and its state", () => {
    sessions.value = { s1: session("s1", "working") };
    render(<Dock runs={[run({ coordinator_session: "s1", owner: "claude-1" })]} />);
    expect(
      screen.getByRole("button", { name: "Coordinador de Lote A: trabajando" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Conversación s1")).toBeInTheDocument();
  });

  it("starts a coordinator here and hands it the batch once it waits", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<Dock runs={[run()]} />);
    await user.click(screen.getByRole("button", { name: "Iniciar coordinador" }));
    expect(bus.startAgentSession).toHaveBeenCalledWith("C:\\work\\agentos", "claude", "workspace");
    expect(client.handOverDeliveryRun).not.toHaveBeenCalled();

    sessions.value = { "s-new": session("s-new", "waiting") };
    rerender(<Dock runs={[run()]} />);
    await waitFor(() => expect(client.handOverDeliveryRun).toHaveBeenCalledWith("r1", "s-new"));
    expect(screen.getByText("Conversación s-new")).toBeInTheDocument();
  });

  it("says when the batch is coordinated outside Tinto", () => {
    render(<Dock runs={[run({ owner: "claude-coord-1" })]} />);
    expect(screen.getByText(/Lo coordina claude-coord-1 fuera de Tinto/)).toBeInTheDocument();
  });

  it("stops new tasks and detaches the conversation", async () => {
    const user = userEvent.setup();
    sessions.value = { s1: session("s1", "waiting") };
    render(<Dock runs={[run({ coordinator_session: "s1" })]} />);
    await user.click(screen.getByRole("button", { name: "No crear más tareas" }));
    expect(client.setDeliveryRunNoNewTasks).toHaveBeenCalledWith("r1", true);
    await user.click(screen.getByRole("button", { name: "Abrir el coordinador en otra ventana" }));
    expect(windows.openDetachedTerminalWindow).toHaveBeenCalledWith(
      { sessionId: "s1", repo: "C:\\work\\agentos", agentType: "claude" },
      expect.any(Function),
    );
  });
});
