import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, waitFor } from "@testing-library/react";
import type { DeliveryDecision, DeliveryOverview, DeliveryTask } from "../../delivery/types";
import { qualityStore } from "../../qol/state";

const client = vi.hoisted(() => ({
  getDeliveryOverview: vi.fn(),
  onDeliveryChanged: vi.fn(),
}));
vi.mock("../../delivery/client", () => client);

import { DeliveryNotificationWatcher, deliveryNeeds } from "./DeliveryNotifications";

const task: DeliveryTask = {
  id: "t1",
  run_id: null,
  repo: "C:\\work\\agentos",
  distro: null,
  key: "AGOS-501",
  title: "",
  worktree: "C:\\work\\agentos-wt\\AGOS-501",
  branch: "delivery/agos-501",
  base_ref: "develop",
  base_commit: "0123456789",
  state: "intake",
  contract_version: 1,
  created_at_ms: 1,
  updated_at_ms: 1,
};

function decision(id: string): DeliveryDecision {
  return {
    id,
    task_id: "t1",
    kind: "text",
    question: "¿Texto del aviso?",
    detail: "",
    options: [],
    text: "Instalado",
    command: null,
    undo: "",
    status: "pending",
    answer: null,
    requested_by: "coord",
    requested_at_ms: 1,
    decided_by: null,
    decided_at_ms: null,
  };
}

function overview(decisions: DeliveryDecision[]): DeliveryOverview {
  const pending = decisions.filter((decision) => decision.status === "pending").length;
  return {
    // As the backend derives it for this task: its decisions, else its tests.
    phases: {
      [task.id]:
        pending > 0
          ? { kind: "decisions", count: pending, waits_on: "user", next_stage: "tests" }
          : { kind: "next_stage", role: "tests", waits_on: "coordinator", next_stage: "tests" },
    },
    runs: [],
    tasks: [task],
    jobs: [],
    leases: [],
    approvals: [],
    decisions,
    settings: { capacity: 3 },
    coordinator: null,
  };
}

describe("Delivery notifications", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    qualityStore.reset();
  });

  it("names what each task that needs the user is waiting for", () => {
    expect(deliveryNeeds(overview([]), 0)).toEqual([]);
    expect(deliveryNeeds(overview([decision("d1"), decision("d2")]), 0)).toEqual([
      { keys: ["d1", "d2"], title: "Tinto: AGOS-501", body: "2 decisiones pendientes" },
    ]);
  });

  it("notifies once per new need, and only when notifications are on", async () => {
    let changed = () => {};
    client.onDeliveryChanged.mockImplementation((callback: () => void) => {
      changed = callback;
      return Promise.resolve(() => {});
    });
    client.getDeliveryOverview.mockResolvedValue(overview([decision("d1"), decision("d2")]));
    const adapter = { ensurePermission: vi.fn(), send: vi.fn() };
    render(<DeliveryNotificationWatcher adapter={adapter} />);
    expect(client.getDeliveryOverview).not.toHaveBeenCalled();

    act(() => qualityStore.setNotificationState({ enabled: true, status: "ready" }));
    await waitFor(() =>
      expect(adapter.send).toHaveBeenCalledWith({
        key: "d1",
        title: "Tinto: AGOS-501",
        body: "2 decisiones pendientes",
      }),
    );

    // Answering one of them is not news; a new question is.
    client.getDeliveryOverview.mockResolvedValue(overview([decision("d2")]));
    changed();
    // Changes are read at most once a second.
    await waitFor(() => expect(client.getDeliveryOverview).toHaveBeenCalledTimes(2), {
      timeout: 3000,
    });
    client.getDeliveryOverview.mockResolvedValue(overview([decision("d2"), decision("d3")]));
    changed();
    await waitFor(() => expect(adapter.send).toHaveBeenCalledTimes(2), { timeout: 3000 });
    expect(adapter.send).toHaveBeenLastCalledWith(
      expect.objectContaining({ body: "2 decisiones pendientes" }),
    );
  });
});
