import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

const clientMocks = vi.hoisted(() => ({
  retryRepo: vi.fn((...args: unknown[]) => {
    void args;
    return Promise.resolve();
  }),
  startAgentSession: vi.fn((...args: unknown[]) => {
    void args;
    return Promise.resolve("sess-1");
  }),
  listAgentSessions: vi.fn(() => Promise.resolve([])),
  agentProviderReadinessForRepo: vi.fn((...args: unknown[]) => {
    void args;
    return Promise.resolve({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "binary_available",
    });
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../bus/client", () => ({
  retryRepo: (...args: unknown[]) => clientMocks.retryRepo(...args),
  startAgentSession: (...args: unknown[]) => clientMocks.startAgentSession(...args),
  listAgentSessions: () => clientMocks.listAgentSessions(),
  agentProviderReadinessForRepo: (...args: unknown[]) =>
    clientMocks.agentProviderReadinessForRepo(...args),
}));

import { DashboardPanel } from "./DashboardPanel";
import { resetAgentAvailabilityCacheForTests } from "./agentAvailability";
import { busStore } from "../bus/store";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import type { RepoDelta } from "../bus/contract";

function delta(repo: string, revision: number, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision,
    status: { modified: [], staged: [], untracked: [] },
    branch: { name: "main", detached: false, unborn: false, ahead: 0, behind: 0 },
    head: null,
    last_activity_ms: revision * 1000,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
    ...over,
  };
}

function renderDash(actions: Partial<WorkspaceActions> = {}) {
  const value: WorkspaceActions = {
    openRepo: vi.fn(),
    addRepo: vi.fn(),
    removeRepo: vi.fn(),
    openFile: vi.fn(),
    openTimeline: vi.fn(),
    openDashboard: vi.fn(),
    openAgents: vi.fn(),
    openAgentTerminal: vi.fn(),
    ...actions,
  };
  render(
    <WorkspaceActionsContext.Provider value={value}>
      <DashboardPanel />
    </WorkspaceActionsContext.Provider>,
  );
  return value;
}

describe("DashboardPanel", () => {
  beforeEach(() => {
    busStore.resetAll();
    clientMocks.retryRepo.mockClear();
    clientMocks.startAgentSession.mockClear();
    clientMocks.listAgentSessions.mockClear();
    clientMocks.agentProviderReadinessForRepo.mockReset();
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "binary_available",
    });
    resetAgentAvailabilityCacheForTests();
  });

  // Covers AE12: loading skeletons before the snapshot
  it("shows skeletons until the snapshot is loaded", () => {
    renderDash();
    expect(screen.getByTestId("skeletons")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Cargando repos");
    expect(screen.getByTestId("dashboard-loading-preview")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getByTestId("dashboard-loading-overlay")).toBeInTheDocument();
  });

  it("previews configured repos behind the initial loading overlay", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              { path: "/r/api", alias: "API", source: "local", distro: null, fs_watch: [] },
              { path: "/r/web", alias: null, source: "local", distro: null, fs_watch: [] },
            ],
          },
        ],
      });
    });

    renderDash();

    const preview = screen.getByTestId("dashboard-loading-preview");
    expect(preview).toHaveTextContent("Work");
    expect(preview).toHaveTextContent("API");
    expect(preview).toHaveTextContent("web");
    expect(screen.getByRole("status")).toHaveTextContent("Cargando repos");
  });

  // Covers AE12: zero-repos state with an Add action
  it("shows the zero-repos state with a working Add button", () => {
    act(() => busStore.loadSnapshot([], { available: true }));
    const { addRepo } = renderDash();
    expect(screen.getByTestId("zero-repos")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("dashboard-add-repo"));
    expect(addRepo).toHaveBeenCalledOnce();
  });

  it("keeps the dashboard add action source-neutral", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    const addRepo = vi.fn();
    renderDash({ addRepo });

    fireEvent.click(screen.getByTestId("dashboard-add-repo"));

    expect(addRepo).toHaveBeenCalledOnce();
    expect(screen.getAllByRole("button", { name: /añadir repo/i })).toHaveLength(1);
  });

  it("opens the agents panel from the dashboard action bar", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    const openAgents = vi.fn();
    renderDash({ openAgents });

    fireEvent.click(screen.getByTestId("dashboard-open-agents"));

    expect(openAgents).toHaveBeenCalledOnce();
  });

  it("renders a card per repo", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1), delta("/r/b", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("card-/r/a")).toBeInTheDocument();
    expect(screen.getByTestId("card-/r/b")).toBeInTheDocument();
  });

  it("keeps configured WSL repos visible as loading while their live snapshot is pending", () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              {
                path: "/home/me/chat-n-food",
                alias: null,
                source: "wsl",
                distro: "Ubuntu",
                fs_watch: [],
              },
            ],
          },
        ],
      });
      busStore.loadSnapshot([], { available: true });
    });

    renderDash();

    expect(screen.getByTestId("card-/home/me/chat-n-food")).toBeInTheDocument();
    expect(screen.getByTestId("repo-source-badge")).toHaveTextContent("WSL");
    expect(screen.getByTestId("repo-source-badge")).toHaveAttribute("title", "WSL · Ubuntu");
    expect(screen.getByTestId("repo-pending")).toHaveTextContent(
      "Esperando la primera instantánea del repo",
    );
    expect(screen.getByTestId("repo-pending")).toHaveAttribute("role", "status");
    expect(screen.queryByTestId("error-detail")).not.toBeInTheDocument();
    expect(screen.queryByTestId("error-badge")).not.toBeInTheDocument();
    expect(clientMocks.agentProviderReadinessForRepo).not.toHaveBeenCalled();
  });

  // Covers AE2: a status change updates that card live
  it("updates a card's counts when a newer delta arrives", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("card-/r/a").querySelector(".count--modified")).toHaveTextContent(
      "0M",
    );
    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, { status: { modified: ["x", "y"], staged: [], untracked: [] } }),
      ),
    );
    expect(screen.getByTestId("card-/r/a").querySelector(".count--modified")).toHaveTextContent(
      "2M",
    );
  });

  it("announces rapid material deltas once per repo in one polite batch", async () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1), delta("/r/b", 1)], { available: true }));
    renderDash();

    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, { status: { modified: ["x"], staged: [], untracked: [] } }),
      ),
    );
    act(() =>
      busStore.applyDelta(
        delta("/r/a", 3, { status: { modified: ["x", "y"], staged: [], untracked: [] } }),
      ),
    );
    act(() =>
      busStore.applyDelta(
        delta("/r/b", 2, { status: { modified: [], staged: ["z"], untracked: [] } }),
      ),
    );

    const liveRegion = screen.getByTestId("dashboard-delta-announcement");
    expect(liveRegion).toHaveAttribute("aria-live", "polite");
    await waitFor(() => expect(liveRegion).toHaveTextContent("a actualizado: 2 archivos"));
    expect(liveRegion).toHaveTextContent("b actualizado: 1 archivo");
    expect(liveRegion.textContent?.match(/a actualizado/g)).toHaveLength(1);
  });

  it("announces consecutive material batches even when their summaries match", async () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    renderDash();

    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, { status: { modified: ["x", "y"], staged: [], untracked: [] } }),
      ),
    );
    const firstMessage = await screen.findByTestId("dashboard-delta-announcement-message");
    expect(firstMessage).toHaveTextContent("a actualizado: 2 archivos");

    act(() =>
      busStore.applyDelta(
        delta("/r/a", 3, { status: { modified: ["u", "v"], staged: [], untracked: [] } }),
      ),
    );

    await waitFor(() =>
      expect(screen.getByTestId("dashboard-delta-announcement-message")).not.toBe(firstMessage),
    );
    expect(screen.getByTestId("dashboard-delta-announcement-message")).toHaveTextContent(
      "a actualizado: 2 archivos",
    );
  });

  it("treats a switched workbench snapshot as a baseline before announcing later deltas", async () => {
    act(() =>
      busStore.loadWorkbench(
        {
          version: 1,
          active: "A",
          workbenches: [
            {
              name: "A",
              repos: [{ path: "/r/a", alias: null, source: "local", distro: null, fs_watch: [] }],
            },
          ],
        },
        [delta("/r/a", 1)],
        { available: true },
      ),
    );
    renderDash();

    act(() =>
      busStore.loadWorkbench(
        {
          version: 1,
          active: "B",
          workbenches: [
            {
              name: "B",
              repos: [{ path: "/r/b", alias: null, source: "local", distro: null, fs_watch: [] }],
            },
          ],
        },
        [
          delta("/r/b", 1, {
            status: { modified: ["snapshot.ts"], staged: [], untracked: [] },
          }),
        ],
        { available: true },
      ),
    );
    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 250));
    });
    expect(screen.queryByTestId("dashboard-delta-announcement-message")).not.toBeInTheDocument();

    act(() =>
      busStore.applyDelta(
        delta("/r/b", 2, {
          status: { modified: ["after-switch.ts"], staged: [], untracked: [] },
        }),
      ),
    );
    expect(await screen.findByTestId("dashboard-delta-announcement-message")).toHaveTextContent(
      "b actualizado: 1 archivo",
    );
  });

  // Covers AE3: a new commit / branch update reflects live
  it("reflects a branch update from a newer delta", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    renderDash();
    expect(screen.getByTestId("branch")).toHaveTextContent("main");
    act(() =>
      busStore.applyDelta(
        delta("/r/a", 2, {
          branch: { name: "feature/x", detached: false, unborn: false, ahead: 2, behind: 0 },
        }),
      ),
    );
    expect(screen.getByTestId("branch")).toHaveTextContent("feature/x");
  });

  // Covers AE8: degraded watching banner
  it("shows the degraded banner when watching is unavailable", () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: false, reason: "inotify" }));
    renderDash();
    expect(screen.getByTestId("degraded-banner")).toHaveTextContent("inotify");
  });

  it("launches an agent session and opens the terminal for the returned session", async () => {
    act(() => busStore.loadSnapshot([delta("/r/a", 1)], { available: true }));
    const openAgentTerminal = vi.fn();
    renderDash({ openAgentTerminal });

    await waitFor(() =>
      expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledWith("/r/a", "codex"),
    );
    fireEvent.click(await screen.findByTestId("agent-launch"));

    await waitFor(() =>
      expect(clientMocks.startAgentSession).toHaveBeenCalledWith("/r/a", "codex"),
    );
    await waitFor(() =>
      expect(openAgentTerminal).toHaveBeenCalledWith({
        sessionId: "sess-1",
        repo: "/r/a",
        agentType: "codex",
      }),
    );
  });

  it("deduplicates agent availability checks by WSL environment", async () => {
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              {
                path: "/home/me/api",
                alias: null,
                source: "wsl",
                distro: "Ubuntu-24.04",
                fs_watch: [],
              },
              {
                path: "/home/me/web",
                alias: null,
                source: "wsl",
                distro: "Ubuntu-24.04",
                fs_watch: [],
              },
            ],
          },
        ],
      });
      busStore.loadSnapshot([delta("/home/me/api", 1), delta("/home/me/web", 1)], {
        available: true,
      });
    });

    renderDash();

    await waitFor(() => expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledTimes(1));
    expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledWith("/home/me/api", "codex");
  });
});
