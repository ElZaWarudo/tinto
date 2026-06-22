import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, act, fireEvent, within } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";

const getCommitLogMock = vi.fn();
const retryRepoMock = vi.fn();
const updateRepoFsWatchMock = vi.fn();
const createRepoGitleaksConfigMock = vi.fn();
let nestedDockviewProps: Record<string, unknown> | null = null;
vi.mock("../bus/client", () => ({
  getCommitLog: (...a: unknown[]) => getCommitLogMock(...a),
  retryRepo: (...a: unknown[]) => retryRepoMock(...a),
  createRepoGitleaksConfig: (...a: unknown[]) => createRepoGitleaksConfigMock(...a),
  // FileView (imported by RepoPanel) pulls in the subscription reconciler, which
  // binds setSubscriptions at module load. The overview tests never render it,
  // but the export must exist.
  setSubscriptions: vi.fn(() => Promise.resolve(true)),
  getWorktreeDiff: vi.fn(() => Promise.resolve([])),
  getFileContent: vi.fn(() => Promise.resolve({ encoding: "utf8", content: "", truncated: false })),
  // ProjectExplorer (left pane of the project tab) loads the repo's file tree.
  listRepoTree: vi.fn(() => Promise.resolve({ entries: [], truncated: false })),
}));
vi.mock("../workbench/operations", () => ({
  updateRepoFsWatch: (...a: unknown[]) => updateRepoFsWatchMock(...a),
}));
// The nested file dock can't render in jsdom; stub it. With no onReady firing,
// the project shows its overview (open file count stays 0), which is what these
// tests assert against.
vi.mock("dockview-react", () => ({
  DockviewReact: (props: Record<string, unknown>) => {
    nestedDockviewProps = props;
    return <div data-testid="mock-dockview" />;
  },
  themeVisualStudio: {},
  DockviewDefaultTab: () => null,
}));

import { RepoPanel } from "./RepoPanel";
import { openRepoPanel } from "../workspace/openRepo";
import { busStore } from "../bus/store";
import { fileDock } from "../workspace/fileDock";
import { repoTreeStore } from "../workspace/repoTreeStore";
import type { RepoDelta } from "../bus/contract";

function delta(repo: string, over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo,
    revision: 1,
    status: { modified: ["src/a.rs"], staged: [], untracked: ["new.txt"] },
    branch: { name: "main", detached: false, unborn: false, ahead: 0, behind: 0 },
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

const panelProps = (repo: string) =>
  ({ params: { repo } }) as unknown as IDockviewPanelProps<{ repo: string }>;

describe("RepoPanel", () => {
  beforeEach(() => {
    localStorage.clear();
    busStore.resetAll();
    fileDock.drop("/r/api");
    fileDock.drop("/r/gone");
    repoTreeStore.reset();
    getCommitLogMock.mockReset();
    retryRepoMock.mockReset();
    updateRepoFsWatchMock.mockReset();
    createRepoGitleaksConfigMock.mockReset();
    updateRepoFsWatchMock.mockResolvedValue(undefined);
    createRepoGitleaksConfigMock.mockResolvedValue(undefined);
    nestedDockviewProps = null;
  });

  it("renders the full status lists and the commit log", async () => {
    getCommitLogMock.mockResolvedValue([
      { id: "abc123", summary: "fix parser", author: "me", timestamp: 1_699_999_000 },
    ]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    render(<RepoPanel {...panelProps("/r/api")} />);

    // Full file lists (the differentiator from the card), not just counts.
    expect(screen.getByTestId("status-lists")).toHaveTextContent("src/a.rs");
    expect(screen.getByTestId("status-lists")).toHaveTextContent("new.txt");
    await waitFor(() => expect(screen.getByTestId("commit-log")).toHaveTextContent("fix parser"));
    expect(getCommitLogMock).toHaveBeenCalledWith("/r/api", 0, 30);
  });

  it("keeps the nested file dock mounted and measurable behind the overview", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));

    render(<RepoPanel {...panelProps("/r/api")} />);

    const dockHost = screen.getByTestId("mock-dockview").parentElement;
    expect(dockHost).toHaveClass("repo-panel__files", "repo-panel__files--empty");
    expect(dockHost).not.toHaveStyle({ display: "none" });
    expect(screen.getByTestId("repo-overview-wrap-/r/api")).toBeInTheDocument();
  });

  it("uses pointer-driven drag and drop for the nested file dock", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));

    render(<RepoPanel {...panelProps("/r/api")} />);

    expect(nestedDockviewProps?.dndStrategy).toBe("pointer");
  });

  it("collapses and restores the project file tree", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    render(<RepoPanel {...panelProps("/r/api")} />);

    fireEvent.click(screen.getByTestId("project-explorer-collapse-/r/api"));
    expect(screen.getByTestId("project-explorer-expand-/r/api")).toBeInTheDocument();
    expect(localStorage.getItem("tinto:explorer-collapsed:/r/api")).toBe("1");

    fireEvent.click(screen.getByTestId("project-explorer-expand-/r/api"));
    expect(screen.getByTestId("project-explorer-collapse-/r/api")).toBeInTheDocument();
    expect(localStorage.getItem("tinto:explorer-collapsed:/r/api")).toBe("0");
  });

  it("restores the collapsed project file tree from localStorage", () => {
    localStorage.setItem("tinto:explorer-collapsed:/r/api", "1");
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));

    render(<RepoPanel {...panelProps("/r/api")} />);

    expect(screen.getByTestId("project-explorer-expand-/r/api")).toBeInTheDocument();
    expect(screen.queryByTestId("project-explorer-collapse-/r/api")).not.toBeInTheDocument();
  });

  it("keeps the collapsed project file tree in sync between mounted views", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));

    render(
      <>
        <div data-testid="repo-view-a">
          <RepoPanel {...panelProps("/r/api")} />
        </div>
        <div data-testid="repo-view-b">
          <RepoPanel {...panelProps("/r/api")} />
        </div>
      </>,
    );

    fireEvent.click(
      within(screen.getByTestId("repo-view-a")).getByTestId("project-explorer-collapse-/r/api"),
    );

    expect(
      within(screen.getByTestId("repo-view-a")).getByTestId("project-explorer-expand-/r/api"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("repo-view-b")).getByTestId("project-explorer-expand-/r/api"),
    ).toBeInTheDocument();
  });

  it("renders passive metrics, repo signals, and status-file signal chips", async () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() =>
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            metrics: { changed_files: 2, lines_added: 12, lines_removed: 3 },
            signals: [
              {
                kind: "config_change",
                severity: "warning",
                path: "src/a.rs",
                message: "Configuration file changed",
              },
            ],
          }),
        ],
        { available: true },
      ),
    );
    render(<RepoPanel {...panelProps("/r/api")} />);
    expect(screen.getByTestId("repo-signals")).toHaveTextContent("2 files · +12 -3");
    expect(screen.getByTestId("repo-signals")).toHaveTextContent("Configuration file changed");
    expect(screen.getByTestId("status-file-src/a.rs")).toHaveTextContent("Config");
  });

  it("configures the repo Gitleaks file from the overview notice", async () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() =>
      busStore.loadSnapshot([delta("/r/api", { gitleaks_configured: false })], { available: true }),
    );
    render(<RepoPanel {...panelProps("/r/api")} />);

    fireEvent.click(screen.getByText("Configurar"));

    expect(createRepoGitleaksConfigMock).toHaveBeenCalledWith("/r/api");
    expect(await screen.findByText("Configurado")).toBeInTheDocument();
  });

  it("shows a terminal error with a working retry", async () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() =>
      busStore.loadSnapshot(
        [
          delta("/r/api", {
            error: { class: "terminal", category: "repo-removed", message: "gone" },
          }),
        ],
        { available: true },
      ),
    );
    render(<RepoPanel {...panelProps("/r/api")} />);
    expect(screen.getByTestId("repo-panel-error")).toHaveTextContent("gone");
    screen.getByTestId("repo-panel-retry").click();
    expect(retryRepoMock).toHaveBeenCalledWith("/r/api");
  });

  it("shows a graceful message when the repo left the workbench", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([], { available: true }));
    render(<RepoPanel {...panelProps("/r/gone")} />);
    expect(screen.getByText(/no longer in the active workbench/i)).toBeInTheDocument();
  });

  // Covers AE9: a status-list file pins (opens) its tab on double-click.
  it("pins a file tab when a status file is double-clicked", () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => busStore.loadSnapshot([delta("/r/api")], { available: true }));
    const openFile = vi.fn();
    const value: WorkspaceActions = {
      openRepo: vi.fn(),
      addRepo: vi.fn(),
      removeRepo: vi.fn(),
      openFile,
      openTimeline: vi.fn(),
      openDashboard: vi.fn(),
      openAgentTerminal: vi.fn(),
    };
    render(
      <WorkspaceActionsContext.Provider value={value}>
        <RepoPanel {...panelProps("/r/api")} />
      </WorkspaceActionsContext.Provider>,
    );
    fireEvent.doubleClick(screen.getByTestId("status-file-src/a.rs"));
    expect(openFile).toHaveBeenCalledWith("/r/api", "src/a.rs", true);
  });

  it("renders watched-file events and saves fs_watch through the active workbench", async () => {
    getCommitLogMock.mockResolvedValue([]);
    act(() => {
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          { name: "Work", repos: [{ path: "/r/api", alias: null, fs_watch: [".env"] }] },
        ],
      });
      busStore.loadSnapshot([delta("/r/api")], { available: true });
      busStore.applyFsEvents({
        repo: "/r/api",
        events: [
          {
            path: ".env",
            kind: "modified",
            timestamp_ms: 1_700_000_000_000,
            size: 20,
            size_delta: 4,
          },
        ],
      });
    });

    render(<RepoPanel {...panelProps("/r/api")} />);

    expect(screen.getByTestId("watched-files")).toHaveTextContent(".env");
    expect(screen.getByTestId("watch-events")).toHaveTextContent("20 B (+4 B)");
    fireEvent.change(screen.getByLabelText("watch pattern 1"), {
      target: { value: "secrets/*.json" },
    });
    fireEvent.click(screen.getByText("Save patterns"));
    await waitFor(() =>
      expect(updateRepoFsWatchMock).toHaveBeenCalledWith("Work", "/r/api", ["secrets/*.json"]),
    );
  });
});

describe("openRepoPanel (dedup)", () => {
  // Covers AE6: opening the same repo focuses the existing panel, no duplicate.
  function fakeApi() {
    const panels: Record<string, { id: string; api: { setActive: ReturnType<typeof vi.fn> } }> = {};
    let activePanel: { id: string; api: { setActive: ReturnType<typeof vi.fn> } } | null = null;
    return {
      get activePanel() {
        return activePanel;
      },
      addPanel: vi.fn((opts: { id: string }) => {
        panels[opts.id] = {
          id: opts.id,
          api: {
            setActive: vi.fn(() => {
              activePanel = panels[opts.id];
            }),
          },
        };
        activePanel = panels[opts.id];
        return panels[opts.id];
      }),
      getPanel: vi.fn((id: string) => panels[id]),
      _panels: panels,
    };
  }

  it("adds a panel the first time and focuses it the second time", () => {
    const api = fakeApi();
    // first open -> addPanel
    openRepoPanel(api as never, "/r/api", "api");
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    const created = api._panels["repo:/r/api"];
    // second open -> focus existing, no new panel
    openRepoPanel(api as never, "/r/api", "api");
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });

  it("opens a second repo as a right split of the active repo", () => {
    const api = fakeApi();

    openRepoPanel(api as never, "/r/api", "api");
    openRepoPanel(api as never, "/r/web", "web");

    expect(api.addPanel).toHaveBeenLastCalledWith(
      expect.objectContaining({
        id: "repo:/r/web",
        position: {
          direction: "right",
          referencePanel: "repo:/r/api",
        },
      }),
    );
  });
});
