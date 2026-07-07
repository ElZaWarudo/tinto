import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentHostCommandResult, AgentSession, FileDiff, RepoDelta } from "../../bus/contract";

const writeAgentSessionInputMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const listAgentSessionsMock = vi.fn<() => Promise<AgentSession[]>>(() => Promise.resolve([]));
const getAgentJournalSessionMock = vi.fn<() => Promise<AgentSession | null>>(() =>
  Promise.resolve(null),
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
    revertSessionMock.mockClear();
    revertSessionTurnFileMock.mockClear();
    restoreSessionTurnMock.mockClear();
    stopAgentSessionMock.mockClear();
    confirmMock.mockClear();
  });

  it("renders a product agent interface instead of a terminal surface", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(screen.getByTitle("Agent session status strip: loading session.")).toHaveTextContent(
      "Loading session",
    );
    expect(
      screen.getByTitle("Agent session status-strip label: Loading session."),
    ).toHaveTextContent("Loading session");
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByTestId("terminal-panel-sess-1")).toHaveAttribute(
      "title",
      "Codex agent session surface for a: header, status, conversation, Agent Lens, and composer.",
    );
    expect(
      screen.getByTitle("Codex agent header for a: identity, status, and session controls."),
    ).toHaveClass("agent-panel__header");
    expect(screen.getByTitle("Codex agent logo for a: agent mark.")).toHaveClass(
      "agent-panel__logo",
    );
    expect(screen.getByTitle("Codex agent identity for a: agent name and repo label.")).toHaveClass(
      "agent-panel__identity",
    );
    expect(screen.getByTitle("Codex agent display-name label for a.")).toHaveClass(
      "agent-panel__agent",
    );
    expect(screen.getByTitle("Codex agent repo label for a: full path /r/a.")).toHaveClass(
      "agent-panel__repo",
    );
    expect(
      screen.getByTitle("Codex agent header actions for a: Stop and Revert controls."),
    ).toHaveClass("agent-panel__header-actions");
    expect(screen.getByRole("button", { name: "Stop" })).toHaveAttribute(
      "title",
      "Codex stop control for a: stop the running session.",
    );
    expect(screen.getByTitle("Agent stop control label: Stop.")).toHaveTextContent("Stop");
    expect(screen.getByRole("button", { name: "Revert" })).toHaveAttribute(
      "title",
      "Codex revert control for a: stop the session before reverting.",
    );
    expect(screen.getByTitle("Agent revert control label: Revert.")).toHaveTextContent("Revert");
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(
      screen.getByTitle(
        "Codex agent workspace for a: overview, activity, conversation, inspection rail, and composer.",
      ),
    ).toHaveClass("agent-panel__workspace");
    expect(
      screen.getByTitle("Codex agent chat shell for a: transcript tools and conversation column."),
    ).toHaveClass("agent-panel__chat-shell");
    expect(
      screen.getByTitle("Codex agent side rail for a: focused turn and Agent Lens column."),
    ).toHaveClass("agent-panel__side-rail");
    expect(
      screen.getByTitle(
        "Codex agent composer for a: command menu, skill mentions, and message input.",
      ),
    ).toHaveClass("agent-panel__composer");
    expect(
      screen.getByTitle(
        "Codex command menu for a: type / for Codex and Tinto commands or $ for skills.",
      ),
    ).toHaveClass("agent-panel__composer-actions");
    expect(
      screen.getByTitle(
        "Composer command hint: type / for Codex and Tinto commands or $ for skills.",
      ),
    ).toHaveTextContent("Type / for commands or $ for skills");
    expect(
      screen.getByTitle(
        "Composer command scopes: Codex prompt commands, Tinto session commands, and skills.",
      ),
    ).toHaveTextContent("Codex + Tinto + Skills");
    expect(
      screen.getByTitle("Codex agent composer input row for a: message draft and send control."),
    ).toHaveClass("agent-panel__composer-row");
    expect(screen.getByLabelText("Message Codex")).toHaveAttribute(
      "title",
      "Codex agent message input for a: draft the next instruction.",
    );
    expect(screen.getByRole("button", { name: "Send" })).toHaveAttribute(
      "title",
      "Codex agent send control for a: message input is empty.",
    );
    expect(screen.getByTitle("Composer send button label: Send.")).toHaveTextContent("Send");
    const conversation = screen.getByLabelText("Agent conversation");
    expect(conversation).toHaveAttribute(
      "title",
      "Agent conversation transcript: ready for the first turn.",
    );
    expect(
      screen.getByTitle("Agent conversation empty state: ready for the first turn."),
    ).toHaveClass("agent-panel__empty-chat");
    expect(screen.getByTitle("Agent conversation empty-state label: Ready.")).toHaveTextContent(
      "Ready",
    );
    expect(
      screen.getByTitle("Agent conversation empty-state helper: start a turn from the composer."),
    ).toHaveTextContent("Start a turn from the composer below.");
    expect(
      screen.getByTitle(
        "Agent transcript tools: search, result navigation, latest, and copy controls waiting for transcript turns.",
      ),
    ).toHaveTextContent("Search transcript");
    const idleFocus = screen.getByLabelText("Focused turn");
    expect(idleFocus).toHaveAttribute("title", "Focused turn card container: no turn selected.");
    expect(within(idleFocus).getByTitle("Focused turn idle status label: Idle.")).toHaveTextContent(
      "Idle",
    );
    expect(
      within(idleFocus).getByTitle("Focused turn empty-state label: No turn selected."),
    ).toHaveTextContent("No turn selected");
    expect(
      within(idleFocus).getByTitle(
        "Focused turn idle helper text: the next agent response will appear as a navigable turn.",
      ),
    ).toHaveTextContent("The next agent response will appear here as a navigable turn.");
    expect(
      screen.getByTitle("Transcript search count: showing all 0 transcript turns."),
    ).toHaveTextContent("All turns");
    expect(
      screen.getByTitle(
        "Transcript search: find messages, commands, and files across 0 transcript turns.",
      ),
    ).toHaveTextContent("Search transcript");
    expect(screen.getByTitle("Transcript search label: Search transcript.")).toHaveTextContent(
      "Search transcript",
    );
    expect(screen.getByRole("group", { name: "Transcript secondary actions" })).toHaveAttribute(
      "title",
      "Transcript secondary actions: latest-turn jump and copy-visible controls waiting for transcript turns.",
    );
    const overview = screen.getByLabelText("Agent session overview");
    expect(overview).toHaveAttribute(
      "title",
      "Agent session overview: 0 turns, 0 messages, 0 commands, 0 files; latest activity: waiting for the first turn; turn map waiting for turns.",
    );
    expect(
      within(overview).getByTitle(
        "Agent session overview metrics: 0 turns, 0 messages, 0 commands, 0 files.",
      ),
    ).toHaveTextContent("Turns");
    expect(
      within(overview).getByTitle(
        "Agent session overview latest-activity area: waiting for the first turn.",
      ),
    ).toHaveTextContent("Waiting for the first turn.");
    expect(
      within(overview).getByTitle("Agent session overview latest-activity label."),
    ).toHaveTextContent("Latest activity");
    expect(
      within(overview).getByTitle(
        "Agent session overview latest activity: waiting for the first turn.",
      ),
    ).toHaveTextContent("Waiting for the first turn.");
    const activity = screen.getByLabelText("Agent activity");
    expect(activity).toHaveAttribute(
      "title",
      "Agent activity strip: Agent is working; Working. 1 changes tracked so far.; 0 turns, 0 files.",
    );
    expect(
      within(activity).getByTitle("Agent activity main status: Agent is working."),
    ).toHaveClass("agent-panel__activity-main");
    expect(within(activity).getByTitle("Agent activity pulse: working state.")).toHaveClass(
      "agent-panel__activity-dot",
    );
    expect(
      within(activity).getByTitle(
        "Agent activity status text: Agent is working; Working. 1 changes tracked so far.",
      ),
    ).toHaveTextContent("Agent is working");
    expect(
      within(activity).getByTitle("Agent activity headline: Agent is working."),
    ).toHaveTextContent("Agent is working");
    expect(
      within(activity).getByTitle("Agent activity detail: Working. 1 changes tracked so far."),
    ).toHaveTextContent("Working. 1 changes tracked so far.");
    expect(within(activity).getByText("Agent is working")).toBeInTheDocument();
    expect(within(activity).getByText("filesystem checkpoint")).toBeInTheDocument();
    expect(
      within(activity).getByTitle(
        "Agent activity facts: turns, files, checkpoint, and stream throughput.",
      ),
    ).toHaveTextContent("0 turns");
    expect(within(activity).getByTitle("Agent activity turn count: 0 turns.")).toHaveTextContent(
      "0 turns",
    );
    expect(
      within(activity).getByTitle("Agent activity touched-file count: 0 files."),
    ).toHaveTextContent("0 files");
    expect(
      within(activity).getByTitle("Agent activity checkpoint fact: filesystem checkpoint."),
    ).toHaveTextContent("filesystem checkpoint");
    expect(
      within(activity).getByTitle("Agent activity stream throughput fact: Stream quiet."),
    ).toHaveTextContent("Stream quiet");
    expect(screen.getByTitle("Agent session status facet: Running.")).toHaveTextContent("Running");
    expect(screen.getByTitle("Agent turn status facet: Working.")).toHaveTextContent("Working");
    expect(
      screen.getByTitle("Agent checkpoint status facet: filesystem checkpoint."),
    ).toHaveTextContent("filesystem checkpoint");
    expect(screen.getByTitle("Agent change-log status facet: 1 change.")).toHaveTextContent(
      "1 changes",
    );
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

    const context = await screen.findByLabelText("Turn context");
    expect(context).toHaveAttribute(
      "title",
      "Turn context strip: Goal Build the host harness; Style precise; Plan On; Compact Review findings are structured and WSL parity is working.",
    );
    expect(within(context).getByTitle("Turn context label.")).toHaveTextContent("Turn context");
    expect(within(context).getByTitle("Turn context items: 4 items.")).toBeInTheDocument();
    expect(
      within(context).getByTitle("Turn context goal: Build the host harness."),
    ).toHaveTextContent("Build the host harness");
    expect(within(context).getByTitle("Turn context style: precise.")).toHaveTextContent("precise");
    expect(within(context).getByTitle("Turn context plan: On.")).toHaveTextContent("On");
    expect(
      within(context).getByTitle(
        "Turn context compact: Review findings are structured and WSL parity is working.",
      ),
    ).toHaveTextContent("Review findings are structured and WSL parity is working.");
  });

  it("sends composer text as an agent turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      speed: "standard",
    });
    expect(composer).toHaveValue("");
  });

  it("sends Codex turns with runtime controls selected from the composer popover", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    await user.click(await screen.findByRole("button", { name: /Reasoning/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "Alto" }));
    await user.click(screen.getByRole("button", { name: /Model/ }));
    await user.click(screen.getByRole("menuitemradio", { name: "GPT-5.5" }));

    const composer = screen.getByLabelText("Message Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r", {
      model: "gpt-5.5",
      reasoning_effort: "high",
      speed: "standard",
    });
  });

  it("applies natural runtime slash aliases without sending them to the agent", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/modelo gpt-5.5");
    await user.type(composer, "{Enter}");
    await user.type(composer, "/razonamiento alto");
    await user.type(composer, "{Enter}");
    await user.type(composer, "/rápido");
    await user.type(composer, "{Enter}");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Send" }));

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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Send" }));

    const errorBanner = await screen.findByTestId("terminal-panel-error");
    expect(errorBanner).toHaveTextContent("Write failed");
    expect(errorBanner).toHaveAttribute("title", "Agent session error banner: Write failed");
  });

  it("prepares editable turns from composer commands and skill mentions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/test");
    expect(screen.getByRole("listbox", { name: "Composer commands" })).toHaveAttribute(
      "title",
      "Composer command menu: 1 command matching /test.",
    );
    expect(screen.getByRole("option", { name: /\/test/ })).toHaveAttribute(
      "title",
      "Run /test: Run the most relevant verification for this repo.",
    );
    await user.type(composer, "{Enter}");

    expect(composer).toHaveValue(
      "Run the most relevant verification for this repo and summarize failures before fixing them.",
    );

    await user.type(composer, "{Shift>}{Enter}{/Shift}$warden");
    expect(screen.getByRole("listbox", { name: "Composer commands" })).toHaveAttribute(
      "title",
      "Composer command menu: 1 command matching $warden.",
    );
    expect(screen.getByRole("option", { name: /\$krt-interface-warden/ })).toHaveAttribute(
      "title",
      "Run $krt-interface-warden: Design or revise a distinctive working-surface interface.",
    );
    await user.type(composer, "{Enter}");
    expect(composer).toHaveValue(
      [
        "Run the most relevant verification for this repo and summarize failures before fixing them.",
        "$krt-interface-warden ",
      ].join("\n\n"),
    );

    await user.clear(composer);
    await user.type(composer, "/details");
    expect(screen.getByRole("option", { name: /\/details/ })).toHaveAttribute(
      "title",
      "Run /details: Open session details, files, commands, timeline, and restore points.",
    );
  });

  it("shows the non-memory Codex-style command palette", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/");

    const menu = screen.getByRole("listbox", { name: "Composer commands" });
    expect(menu).toHaveAttribute("title", expect.stringMatching(/^Composer command menu: /));
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
      expect(within(menu).getByTitle(`Composer command trigger: ${command}.`)).toBeInTheDocument();
    }
    expect(within(menu).queryByRole("option", { name: /\/memories/ })).not.toBeInTheDocument();
    expect(within(menu).queryByText(/Memorias/)).not.toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/details");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "details", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(
      await screen.findByTitle(
        "Session details: turn map, current activity, restore points, and Agent Lens.",
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

    const composer = await screen.findByLabelText("Message Codex");
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/status");
    expect(screen.getByRole("option", { name: /\/status/ })).toHaveAttribute(
      "title",
      "Run /status: Mostrar el ID del chat, estado, uso y runtime. Aliases: /estado.",
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/mascot");
    await user.type(composer, "{Enter}");

    const mascot = await screen.findByLabelText("Tinto companion");
    expect(mascot).toHaveAttribute("title", "Tinto companion is awake for Codex on a.");
    expect(within(mascot).getByTitle("Tinto companion status.")).toHaveTextContent("Awake");
    expect(await screen.findByText("Mascot awake in this agent panel.")).toBeInTheDocument();
    expect(runAgentHostCommandMock).not.toHaveBeenCalled();
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();

    await user.type(composer, "/mascot");
    await user.type(composer, "{Enter}");

    await waitFor(() => expect(screen.queryByLabelText("Tinto companion")).not.toBeInTheDocument());
    expect(await screen.findByText("Mascot hidden.")).toBeInTheDocument();
    expect(runAgentHostCommandMock).not.toHaveBeenCalled();
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("defers memory slash commands without sending them to the agent", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValue([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/memorias");
    expect(screen.queryByRole("option", { name: /memories|memorias/i })).not.toBeInTheDocument();
    await user.type(composer, "{Enter}");

    expect(
      await screen.findByText("Memory commands are deferred for the later Tinto memory plan."),
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/objective");
    expect(screen.getByRole("option", { name: /\/goal/ })).toHaveAttribute(
      "title",
      "Run /goal: Establecer un objetivo hacia el que Codex seguirá trabajando. Aliases: /objective, /objetivo.",
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/init");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "init", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(await screen.findByText("AGENTS.md is configured for this repo.")).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "review", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(
      await screen.findByText("Review summary for branch main: 2 changed file(s)."),
    ).toBeInTheDocument();
    const review = await screen.findByLabelText("Review summary");
    expect(review).toHaveAttribute("title", "Review summary for main: 2 changed files.");
    expect(within(review).getByTitle("Review summary branch.")).toHaveTextContent("main");
    expect(within(review).getByText("1 file changed, 3 insertions(+)")).toBeInTheDocument();
    expect(within(review).getByText("?? docs/review.md")).toBeInTheDocument();
    expect(within(review).getByLabelText("Review findings")).toBeInTheDocument();
    expect(within(review).getByText("Conflict marker present")).toBeInTheDocument();
    expect(within(review).getByText("src/App.tsx:12")).toBeInTheDocument();
    const copySummary = within(review).getByRole("button", {
      name: "Copy structured review summary",
    });
    expect(copySummary).toHaveAttribute(
      "title",
      "Copy the structured review summary to the clipboard.",
    );
    expect(
      within(copySummary).getByTitle("Structured review summary copy label: Copy summary."),
    ).toHaveTextContent("Copy summary");

    installClipboardMock();
    fireEvent.click(copySummary);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Structured review summary:",
          "Branch: main",
          "Changed files: 2",
          "Working tree diff: 1 file changed, 3 insertions(+)",
          "Staged diff: no staged line diff",
          "Files:",
          "-  M src/App.tsx",
          "- ?? docs/review.md",
          "Host review findings:",
          "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
        ].join("\n"),
      ),
    );
    expect(copySummary).toHaveAttribute("title", "Copied structured review summary to clipboard.");
    expect(
      within(copySummary).getByTitle("Structured review summary copy label: Copied."),
    ).toHaveTextContent("Copied");

    const copyFiles = within(review).getByRole("button", {
      name: "Copy review changed files",
    });
    expect(copyFiles).toHaveAttribute("title", "Copy 2 files to the clipboard.");
    expect(
      within(copyFiles).getByTitle("Review changed files copy label: Copy files."),
    ).toHaveTextContent("Copy files");

    installClipboardMock();
    fireEvent.click(copyFiles);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        ["Review changed files:", "-  M src/App.tsx", "- ?? docs/review.md"].join("\n"),
      ),
    );
    expect(copyFiles).toHaveAttribute("title", "Copied review changed files to clipboard.");
    expect(
      within(copyFiles).getByTitle("Review changed files copy label: Copied."),
    ).toHaveTextContent("Copied");

    const copyFindings = within(review).getByRole("button", {
      name: "Copy deterministic review findings",
    });
    expect(copyFindings).toHaveAttribute("title", "Copy 1 finding to the clipboard.");
    expect(
      within(copyFindings).getByTitle("Deterministic review findings copy label: Copy findings."),
    ).toHaveTextContent("Copy findings");

    installClipboardMock();
    fireEvent.click(copyFindings);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Host review findings:",
          "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
        ].join("\n"),
      ),
    );
    expect(copyFindings).toHaveAttribute(
      "title",
      "Copied deterministic review findings to clipboard.",
    );
    expect(
      within(copyFindings).getByTitle("Deterministic review findings copy label: Copied."),
    ).toHaveTextContent("Copied");

    const reviewPromptButton = within(review).getByRole("button", {
      name: "Draft semantic review prompt",
    });
    expect(reviewPromptButton).toHaveAttribute(
      "title",
      "Draft a semantic code-review prompt from this review summary with 1 finding.",
    );
    expect(
      within(reviewPromptButton).getByTitle("Review semantic prompt action label."),
    ).toHaveTextContent("Ask review");

    await user.click(reviewPromptButton);
    expect(
      within(review).getByTitle("Semantic review prompt is drafted in the composer."),
    ).toHaveTextContent("Review draft ready");
    const draftReset = within(review).getByRole("button", {
      name: "Reset semantic review workflow",
    });
    expect(draftReset).toHaveAttribute(
      "title",
      "Reset the drafted semantic review prompt state for this review summary.",
    );
    expect(
      within(draftReset).getByTitle("Semantic review reset label: Reset review."),
    ).toHaveTextContent("Reset review");
    const expectedReviewPrompt = [
      "Review the current Git changes for correctness, regressions, security risks, and missing tests.",
      "Branch: main",
      "Changed files: 2",
      "Working tree diff: 1 file changed, 3 insertions(+)",
      "Files:",
      "-  M src/App.tsx",
      "- ?? docs/review.md",
      "Host review findings to verify first:",
      "- high: Conflict marker present (src/App.tsx:12) - src/App.tsx still contains a merge conflict marker.",
      "Return findings first, ordered by severity, with file/line references when possible. If there are no issues, say that clearly and mention any residual test gaps.",
    ].join("\n");
    expect(composer).toHaveValue(expectedReviewPrompt);

    const copyPrompt = within(review).getByRole("button", {
      name: "Copy semantic review prompt",
    });
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Copy the drafted semantic review prompt to the clipboard.",
    );
    expect(
      within(copyPrompt).getByTitle("Semantic review prompt copy label: Copy prompt."),
    ).toHaveTextContent("Copy prompt");

    installClipboardMock();
    fireEvent.click(copyPrompt);
    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith(expectedReviewPrompt));
    expect(copyPrompt).toHaveAttribute("title", "Copied semantic review prompt to clipboard.");
    expect(
      within(copyPrompt).getByTitle("Semantic review prompt copy label: Copied."),
    ).toHaveTextContent("Copied");

    await user.type(composer, "{Enter}");
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", `${expectedReviewPrompt}\r`, {
      speed: "standard",
    });
    expect(
      await within(review).findByTitle("Semantic review prompt was sent as an agent turn."),
    ).toHaveTextContent("Review request sent");

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
        "Semantic review response captured from turn 1; verify findings before acting.",
      ),
    ).toHaveTextContent("Review response captured");
    expect(within(review).getByText(/Found one high severity issue/)).toBeInTheDocument();
    const responseReset = within(review).getByRole("button", {
      name: "Reset semantic review workflow",
    });
    expect(responseReset).toHaveAttribute(
      "title",
      "Reset the captured semantic review response and request state for this review summary.",
    );

    const showRequest = screen.getByRole("button", {
      name: "Show semantic review request turn",
    });
    expect(showRequest).toHaveAttribute(
      "title",
      "Show the sent semantic review request in conversation turn 1.",
    );
    expect(
      within(showRequest).getByTitle("Semantic review request navigation label: Show request."),
    ).toHaveTextContent("Show request");

    scrollIntoViewMock.mockClear();
    fireEvent.click(showRequest);
    await waitFor(() => expect(scrollIntoViewMock).toHaveBeenCalled());

    const showResponse = screen.getByRole("button", {
      name: "Show semantic review response turn",
    });
    expect(showResponse).toHaveAttribute(
      "title",
      "Show the full semantic review response in conversation turn 1.",
    );
    expect(
      within(showResponse).getByTitle("Semantic review response navigation label: Show response."),
    ).toHaveTextContent("Show response");

    fireEvent.click(showResponse);
    await waitFor(() => expect(scrollIntoViewMock).toHaveBeenCalled());

    const copyResponse = screen.getByRole("button", {
      name: "Copy semantic review response",
    });
    expect(copyResponse).toHaveAttribute(
      "title",
      "Copy the captured semantic review response to the clipboard.",
    );
    expect(
      within(copyResponse).getByTitle("Semantic review response copy label: Copy response."),
    ).toHaveTextContent("Copy response");

    installClipboardMock();
    fireEvent.click(copyResponse);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        "Found one high severity issue in src/App.tsx:12. Add a regression test before merging.",
      ),
    );
    expect(copyResponse).toHaveAttribute("title", "Copied semantic review response to clipboard.");
    expect(
      within(copyResponse).getByTitle("Semantic review response copy label: Copied."),
    ).toHaveTextContent("Copied");

    const copyExchange = screen.getByRole("button", {
      name: "Copy semantic review exchange",
    });
    expect(copyExchange).toHaveAttribute(
      "title",
      "Copy the semantic review request and captured response to the clipboard.",
    );
    expect(
      within(copyExchange).getByTitle("Semantic review exchange copy label: Copy exchange."),
    ).toHaveTextContent("Copy exchange");

    installClipboardMock();
    fireEvent.click(copyExchange);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        [
          "Semantic review request:",
          expectedReviewPrompt,
          "",
          "Semantic review response:",
          "Found one high severity issue in src/App.tsx:12. Add a regression test before merging.",
        ].join("\n"),
      ),
    );
    expect(copyExchange).toHaveAttribute(
      "title",
      "Copied semantic review request and response to clipboard.",
    );
    expect(
      within(copyExchange).getByTitle("Semantic review exchange copy label: Copied."),
    ).toHaveTextContent("Copied");

    fireEvent.click(responseReset);
    expect(
      within(review).queryByTitle("Semantic review prompt was sent as an agent turn."),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByTitle(
        "Semantic review response captured from turn 1; verify findings before acting.",
      ),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Reset semantic review workflow" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Show semantic review response turn" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copy semantic review response" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copy semantic review prompt" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Show semantic review request turn" }),
    ).not.toBeInTheDocument();
    expect(
      within(review).queryByRole("button", { name: "Copy semantic review exchange" }),
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    const review = await screen.findByLabelText("Review summary");
    const reviewPromptButton = within(review).getByRole("button", {
      name: "Draft semantic review prompt",
    });
    await user.click(reviewPromptButton);

    const copyPrompt = within(review).getByRole("button", {
      name: "Copy semantic review prompt",
    });
    installClipboardMock();
    fireEvent.click(copyPrompt);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(
        expect.stringContaining("Review the current Git changes"),
      ),
    );
    expect(copyPrompt).toHaveAttribute("title", "Copied semantic review prompt to clipboard.");

    await user.click(reviewPromptButton);
    expect(copyPrompt).toHaveAttribute(
      "title",
      "Copy the drafted semantic review prompt to the clipboard.",
    );
    expect(
      within(copyPrompt).getByTitle("Semantic review prompt copy label: Copy prompt."),
    ).toHaveTextContent("Copy prompt");
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    const review = await screen.findByLabelText("Review summary");
    const copySummary = within(review).getByRole("button", {
      name: "Copy structured review summary",
    });
    installClipboardMock();
    fireEvent.click(copySummary);
    await waitFor(() =>
      expect(writeClipboardTextMock).toHaveBeenCalledWith(expect.stringContaining("Branch: main")),
    );
    expect(copySummary).toHaveAttribute("title", "Copied structured review summary to clipboard.");

    await user.type(composer, "/review");
    await user.type(composer, "{Enter}");

    await waitFor(() => expect(within(review).getByText("M src/Feature.tsx")).toBeInTheDocument());
    expect(copySummary).toHaveAttribute(
      "title",
      "Copy the structured review summary to the clipboard.",
    );
    expect(
      within(copySummary).getByTitle("Structured review summary copy label: Copy summary."),
    ).toHaveTextContent("Copy summary");
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

    const composer = await screen.findByLabelText("Message Codex");
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
    expect(await screen.findByText("Goal set: Build the host harness.")).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/personality precise");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "personality", "precise"),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(await screen.findByText("Personality set: precise.")).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
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
    expect(
      await screen.findByText("Saved feedback: Keep the controls native."),
    ).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
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
    expect(
      await screen.findByText("Saved comment: The palette should explain unavailable actions."),
    ).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "/compact");
    await user.type(composer, "{Enter}");

    await waitFor(() =>
      expect(runAgentHostCommandMock).toHaveBeenCalledWith("sess-1", "compact", undefined),
    );
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
    expect(
      await screen.findByText("Context summary saved from 3 events across 1 turns."),
    ).toBeInTheDocument();
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

    const composer = await screen.findByLabelText("Message Codex");
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

    const composer = await screen.findByLabelText("Message Codex");
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

    const composer = await screen.findByLabelText("Message Codex");
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

    const conversation = screen.getByLabelText("Agent conversation");
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

    expect(await screen.findByText("You")).toBeInTheDocument();
    expect(screen.getByTitle("Agent message role label: You.")).toHaveTextContent("You");
    expect(
      screen.getByTitle("Agent message block for turn 1: You content and copy control."),
    ).toHaveClass("agent-panel__message--user_message");
    expect(
      screen.getByTitle("Agent message header for You: role label and copy control."),
    ).toHaveTextContent("You");
    expect(screen.getByTitle("Conversation turn index label: Turn 1.")).toHaveTextContent("Turn 1");
    expect(screen.getByText("Haz el cambio")).toBeInTheDocument();
    expect(screen.getByTitle("Rendered You Markdown content for turn 1.")).toHaveTextContent(
      "Haz el cambio",
    );
    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByTitle("Agent message role label: Agent.")).toHaveTextContent("Agent");
    expect(
      screen.getByTitle("Agent message block for turn 1: Agent content and copy control."),
    ).toHaveClass("agent-panel__message--agent_message");
    expect(
      screen.getByTitle("Agent message header for Agent: role label and copy control."),
    ).toHaveTextContent("Agent");
    expect(screen.getByText("Voy con ello")).toBeInTheDocument();
    expect(screen.getByTitle("Rendered Agent Markdown content for turn 1.")).toHaveTextContent(
      "Voy con ello",
    );
    expect(screen.getByText("Command")).toBeInTheDocument();
    expect(screen.getByTitle("Agent message role label: Command.")).toHaveTextContent("Command");
    expect(
      screen.getByTitle("Agent message block for turn 1: Command content and copy control."),
    ).toHaveClass("agent-panel__message--command_output");
    expect(
      screen.getByTitle("Agent message header for Command: role label and copy control."),
    ).toHaveTextContent("Command");
    const conversation = screen.getByLabelText("Agent conversation");
    expect(conversation).toHaveAttribute(
      "title",
      "Agent conversation transcript: showing all 1 turn.",
    );
    expect(within(conversation).getByText("cargo test")).toBeInTheDocument();
    expect(within(conversation).getByTitle("Command output text for turn 1.")).toHaveTextContent(
      "cargo test",
    );
    expect(within(conversation).getByText("3 messages / 1 commands")).toHaveAttribute(
      "title",
      "Turn 1 transcript, command, and file counts: 3 messages / 1 commands.",
    );

    const overview = screen.getByLabelText("Agent session overview");
    expect(overview).toHaveAttribute(
      "title",
      "Agent session overview: 1 turn, 2 messages, 1 command, 0 files; latest activity: cargo test; turn map 1 turn.",
    );
    expect(
      within(overview).getByTitle(
        "Agent session overview metrics: 1 turn, 2 messages, 1 command, 0 files.",
      ),
    ).toHaveTextContent("Turns");
    const turnsMetric = within(overview).getByLabelText("Turns: 1");
    expect(turnsMetric).toHaveAttribute("title", "Agent session overview turns metric: 1 turn.");
    expect(
      within(turnsMetric).getByTitle("Agent session overview turns value: 1."),
    ).toHaveTextContent("1");
    expect(
      within(turnsMetric).getByTitle("Agent session overview metric label: Turns."),
    ).toHaveTextContent("Turns");
    const messagesMetric = within(overview).getByLabelText("Messages: 2");
    expect(messagesMetric).toHaveAttribute(
      "title",
      "Agent session overview messages metric: 2 messages.",
    );
    expect(
      within(messagesMetric).getByTitle("Agent session overview messages value: 2."),
    ).toHaveTextContent("2");
    expect(
      within(messagesMetric).getByTitle("Agent session overview metric label: Messages."),
    ).toHaveTextContent("Messages");
    const commandsMetric = within(overview).getByLabelText("Commands: 1");
    expect(commandsMetric).toHaveAttribute(
      "title",
      "Agent session overview commands metric: 1 command.",
    );
    expect(
      within(commandsMetric).getByTitle("Agent session overview commands value: 1."),
    ).toHaveTextContent("1");
    expect(
      within(commandsMetric).getByTitle("Agent session overview metric label: Commands."),
    ).toHaveTextContent("Commands");
    const filesMetric = within(overview).getByLabelText("Files: 0");
    expect(filesMetric).toHaveAttribute("title", "Agent session overview files metric: 0 files.");
    expect(
      within(filesMetric).getByTitle("Agent session overview files value: 0."),
    ).toHaveTextContent("0");
    expect(
      within(filesMetric).getByTitle("Agent session overview metric label: Files."),
    ).toHaveTextContent("Files");
    expect(
      within(overview).getByTitle(
        "Agent session overview latest-activity area: latest captured activity.",
      ),
    ).toHaveTextContent("cargo test");
    expect(
      within(overview).getByTitle("Agent session overview latest-activity label."),
    ).toHaveTextContent("Latest activity");
    expect(
      within(overview).getByTitle("Agent session overview latest activity: cargo test."),
    ).toHaveTextContent("cargo test");
    expect(within(overview).getByText("+0s")).toBeInTheDocument();
    expect(
      within(overview).getByTitle("Turn 1: 1 commands, 0 files - Recent command: cargo test"),
    ).toBeInTheDocument();
    const turnMap = screen.getByLabelText("Turn map");
    expect(turnMap).toHaveAttribute("title", "Agent session overview turn map: 1 turn.");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });
    expect(
      within(firstTurnButton).getByTitle("Agent session overview turn-map label: turn 1."),
    ).toHaveTextContent("T1");
    expect(
      within(firstTurnButton).getByTitle("Agent session overview turn-map timing for turn 1: +0s."),
    ).toHaveTextContent("+0s");
    expect(
      within(firstTurnButton).getByTitle(
        "Agent session overview turn-map command count for turn 1: 1 command.",
      ),
    ).toHaveTextContent("1 cmd");
    expect(
      within(firstTurnButton).getByTitle(
        "Agent session overview turn-map command summary for turn 1: cargo test.",
      ),
    ).toHaveTextContent("cmd cargo test");
    expect(within(screen.getByLabelText("Agent conversation")).getByText("+0s")).toHaveAttribute(
      "title",
      "Turn 1 timing relative to the first turn: +0s.",
    );

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

    const copyAgent = await screen.findByLabelText("Copy Agent message");
    expect(copyAgent).toHaveAttribute("title", "Copy Agent message from turn 1.");
    expect(screen.getAllByTitle("Message block copy label: Copy.")).toHaveLength(2);
    await user.click(copyAgent);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith("Voy con ello"));
    expect(await screen.findByLabelText("Copy Agent message")).toHaveTextContent("Copied");
    expect(screen.getByLabelText("Copy Agent message")).toHaveAttribute(
      "title",
      "Copied Agent message from turn 1 to clipboard.",
    );
    expect(screen.getByTitle("Message block copy label: Copied.")).toHaveTextContent("Copied");
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

    const copyTurn = await screen.findByRole("button", { name: "Copy turn" });
    expect(copyTurn).toHaveAttribute(
      "title",
      "Copy full transcript for turn 1, including messages, commands, and touched files.",
    );
    expect(screen.getByTitle("Conversation turn copy label: Copy turn.")).toHaveTextContent(
      "Copy turn",
    );
    await user.click(copyTurn);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Turn 1 (+0s)");
    expect(copied).toContain("You:\nHaz el cambio");
    expect(copied).toContain("Agent:\nVoy con ello");
    expect(copied).toContain("Command:\nnpm test");
    expect(copied).toContain("- modified src/a.ts");
    expect(screen.getByRole("button", { name: "Copied" })).toHaveAttribute(
      "title",
      "Copied full transcript for turn 1 to clipboard.",
    );
    expect(screen.getByTitle("Conversation turn copy label: Copied.")).toHaveTextContent("Copied");
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

    const conversation = screen.getByLabelText("Agent conversation");
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

    const conversation = screen.getByLabelText("Agent conversation");
    const toggle = await within(conversation).findByText("Show output");
    expect(toggle).toHaveAttribute(
      "title",
      "Collapsed command output disclosure label: Show output.",
    );
    expect(toggle.closest("summary")).toHaveAttribute(
      "title",
      "Collapsed command output summary row for turn 1: click to show or hide full output.",
    );
    expect(
      within(conversation).getByTitle("Collapsed command output summary: npm test -- --run."),
    ).toHaveTextContent("npm test -- --run");
    const collapsedCommandBlock = toggle.closest("details");
    expect(collapsedCommandBlock).toHaveAttribute(
      "title",
      "Collapsed command output container for turn 1: summary disclosure and full output text.",
    );
    expect(collapsedCommandBlock).not.toHaveAttribute("open");
    expect(within(conversation).getAllByText("npm test -- --run").length).toBeGreaterThan(0);
    const collapsedOutput = within(conversation).getByTitle("Command output text for turn 1.");
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

    const search = await screen.findByLabelText("Search transcript");
    const conversation = screen.getByLabelText("Agent conversation");
    expect(search).toHaveAttribute("aria-describedby", "agent-transcript-search-hint");
    expect(search).toHaveAttribute(
      "title",
      "Transcript search input placeholder: Find messages, commands, files. Press Escape to clear the search.",
    );
    expect(
      screen.getByText(
        "Press Enter to move through matching turns and Escape to clear the search.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous result" })).toHaveAttribute(
      "title",
      "Search transcript to enable previous result navigation.",
    );
    expect(
      screen.getByTitle("Transcript search navigation label: previous result."),
    ).toHaveTextContent("Prev");
    expect(screen.getByRole("button", { name: "Next result" })).toHaveAttribute(
      "title",
      "Search transcript to enable next result navigation.",
    );
    expect(screen.getByTitle("Transcript search navigation label: next result.")).toHaveTextContent(
      "Next",
    );

    await user.type(search, "dashboard");
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).queryByText("Segunda tarea")).not.toBeInTheDocument();
    expect(screen.getByText("1 of 2 turns")).toBeInTheDocument();
    expect(screen.getByLabelText("1 matching turn out of 2 total turns.")).toHaveAttribute(
      "title",
      "Transcript search count: 1 matching turn out of 2 total turns.",
    );
    expect(screen.getByRole("button", { name: "Previous result" })).toHaveAttribute(
      "title",
      "Previous result navigation needs at least two matching turns.",
    );
    expect(screen.getByRole("button", { name: "Next result" })).toHaveAttribute(
      "title",
      "Next result navigation needs at least two matching turns.",
    );
    const messageMatch = within(conversation).getByLabelText("Turn 1 search matches");
    expect(messageMatch).toHaveAttribute(
      "title",
      "Turn 1 search matches: why this visible turn matched the transcript search.",
    );
    expect(within(messageMatch).getByText("Message match")).toBeInTheDocument();
    expect(within(messageMatch).queryByText("Command match")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "npm test search-panel");
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(within(conversation).queryByText("Termine el dashboard")).not.toBeInTheDocument();
    expect(screen.getByText("1 of 2 turns")).toBeInTheDocument();
    const commandMatch = within(conversation).getByLabelText("Turn 2 search matches");
    expect(within(commandMatch).getByText("Command match")).toBeInTheDocument();
    expect(within(commandMatch).queryByText("File match")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "src/search-panel.tsx");
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(within(conversation).queryByText("Termine el dashboard")).not.toBeInTheDocument();
    const fileMatch = within(conversation).getByLabelText("Turn 2 search matches");
    expect(within(fileMatch).getByText("File match")).toBeInTheDocument();
    expect(within(fileMatch).queryByText("Message match")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "nope");
    expect(screen.getByText("No matches")).toBeInTheDocument();
    expect(
      screen.getByTitle(
        "Agent conversation empty state: no transcript matches for the current search.",
      ),
    ).toHaveClass("agent-panel__empty-chat");
    expect(
      screen.getByTitle("Agent conversation empty-state label: No matches."),
    ).toHaveTextContent("No matches");
    expect(
      screen.getByTitle("Agent conversation empty-state helper: try another transcript search."),
    ).toHaveTextContent("Try another search across messages, commands, and touched files.");
    const clearSearch = screen.getByRole("button", { name: "Clear search" });
    expect(clearSearch).toHaveAttribute(
      "title",
      "Agent conversation empty-state action: clear search, restore all turns, and return focus to search.",
    );
    expect(screen.getByTitle("Transcript clear-search label: Clear search.")).toHaveTextContent(
      "Clear search",
    );
    await user.click(clearSearch);
    expect(search).toHaveValue("");
    expect(search).toHaveFocus();
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(screen.queryByText("No matches")).not.toBeInTheDocument();

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

    const latestButton = await screen.findByRole("button", { name: "Latest" });
    expect(latestButton).toHaveAttribute("title", "Jump to the latest of 2 transcript turns.");
    expect(screen.getByTitle("Transcript secondary action label: Latest.")).toHaveTextContent(
      "Latest",
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

    const search = await screen.findByLabelText("Search transcript");
    await user.type(search, "dashboard");
    const conversation = screen.getByLabelText("Agent conversation");
    expect(conversation).toHaveAttribute(
      "title",
      "Agent conversation transcript: 2 matching turns out of 3 total turns.",
    );
    expect(
      screen.getByTitle(
        "Agent transcript tools: search active with 2 matching turns out of 3 total turns.",
      ),
    ).toHaveTextContent("2 of 3 turns");
    expect(
      screen.getByTitle("Transcript search: 2 matching turns out of 3 total turns."),
    ).toHaveTextContent("Search transcript");
    expect(
      screen.getByTitle("Transcript search count: 2 matching turns out of 3 total turns."),
    ).toHaveTextContent("2 of 3 turns");
    expect(screen.getByRole("group", { name: "Transcript secondary actions" })).toHaveAttribute(
      "title",
      "Transcript secondary actions: latest filtered-turn jump and copy 2 filtered transcript turns out of 3 total turns.",
    );
    expect(screen.getByRole("button", { name: "Previous result" })).toHaveAttribute(
      "title",
      "Previous search result",
    );
    expect(screen.getByRole("button", { name: "Next result" })).toHaveAttribute(
      "title",
      "Next search result",
    );
    expect(
      screen.getByLabelText("No focused search result selected out of 2 matching turns."),
    ).toHaveTextContent("- / 2");
    expect(
      screen.getByLabelText("No focused search result selected out of 2 matching turns."),
    ).toHaveAttribute(
      "title",
      "Active transcript search position: no result selected out of 2 matching turns.",
    );

    await user.keyboard("{Enter}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(scrollIntoViewMock).toHaveBeenLastCalledWith({ block: "center", behavior: "smooth" });
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent(
      "1 / 2",
    );
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveAttribute(
      "title",
      "Active transcript search position: result 1 of 2 matching turns.",
    );
    expect(within(conversation).getByText("Dashboard alpha").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.keyboard("{Enter}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-3");
    expect(screen.getByLabelText("Focused search result 2 of 2 matching turns.")).toHaveTextContent(
      "2 / 2",
    );
    expect(screen.getByLabelText("Focused search result 2 of 2 matching turns.")).toHaveAttribute(
      "title",
      "Active transcript search position: result 2 of 2 matching turns.",
    );
    expect(within(conversation).getByText("Dashboard beta").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent(
      "1 / 2",
    );
    expect(within(conversation).getByText("Dashboard alpha").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Next result" }));
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-3");
    expect(within(conversation).getByText("Dashboard beta").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Previous result" }));
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

    const search = await screen.findByLabelText("Search transcript");
    await user.type(search, "dashboard");
    const conversation = screen.getByLabelText("Agent conversation");

    await user.keyboard("{Enter}");
    const alphaTurn = within(conversation).getByText("Dashboard alpha").closest("article");
    expect(alphaTurn).toHaveAttribute("aria-current", "true");
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent(
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
      name: "Transcript secondary actions",
    });
    expect(
      screen.getByTitle(
        "Agent transcript tools: search, result navigation, latest, and copy controls for all 1 transcript turn.",
      ),
    ).toHaveTextContent("All turns");
    expect(
      screen.getByTitle(
        "Transcript search: find messages, commands, and files across 1 transcript turn.",
      ),
    ).toHaveTextContent("Search transcript");
    expect(
      screen.getByTitle("Transcript search count: showing all 1 transcript turn."),
    ).toHaveTextContent("All turns");
    expect(secondaryActions).toHaveAttribute(
      "title",
      "Transcript secondary actions: latest-turn jump and copy all 1 transcript turn.",
    );

    expect(within(secondaryActions).getByRole("button", { name: "Latest" })).toHaveAttribute(
      "title",
      "Jump to the latest of 1 transcript turn.",
    );
    expect(
      within(secondaryActions).getByTitle("Transcript secondary action label: Latest."),
    ).toHaveTextContent("Latest");
    expect(within(secondaryActions).getByRole("button", { name: "Copy visible" })).toHaveAttribute(
      "title",
      "Copy all 1 transcript turn.",
    );
    expect(
      within(secondaryActions).getByTitle("Transcript secondary action label: Copy visible."),
    ).toHaveTextContent("Copy visible");
    expect(
      within(secondaryActions).queryByRole("button", { name: "Previous result" }),
    ).not.toBeInTheDocument();
    expect(
      within(secondaryActions).queryByRole("button", { name: "Next result" }),
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

    await user.type(await screen.findByLabelText("Search transcript"), "dashboard");
    expect(screen.getByRole("button", { name: "Latest" })).toHaveAttribute(
      "title",
      "Jump to the latest filtered transcript turn out of 2 total turns.",
    );
    const copyVisible = screen.getByRole("button", { name: "Copy visible" });
    expect(copyVisible).toHaveAttribute(
      "title",
      "Copy 1 filtered transcript turn out of 2 total turns.",
    );
    await user.click(copyVisible);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Dashboard task");
    expect(copied).toContain("Finished dashboard work");
    expect(copied).not.toContain("Search task");
    expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
    expect(screen.getByTitle("Transcript secondary action label: Copied.")).toHaveTextContent(
      "Copied",
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
      await within(screen.getByLabelText("Agent conversation")).findByText("Archived answer"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("Agent activity")).getByText("Archived transcript"),
    ).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Archived transcript")).toBeDisabled();
    expect(screen.getByPlaceholderText("Archived transcript")).toHaveAttribute(
      "title",
      "Codex agent message input for a: archived transcript is read-only.",
    );
    expect(
      screen.getByTitle("Codex command menu for a: archived transcripts are read-only."),
    ).toHaveClass("agent-panel__composer-actions");
    expect(
      screen.getByTitle("Composer commands are disabled because this transcript is archived."),
    ).toHaveTextContent("Archived transcript");
    expect(screen.queryByRole("listbox", { name: "Composer commands" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Plan" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send" })).toHaveAttribute(
      "title",
      "Codex agent send control for a: archived transcript is read-only.",
    );
    const archivedFocus = screen.getByLabelText("Focused turn");
    expect(
      within(archivedFocus).getByTitle(
        "Focused turn restore container for turn 1: stop the session before restoring.",
      ),
    ).toHaveClass("agent-panel__turn-focus-actions");
    expect(within(archivedFocus).getByRole("button", { name: "Restore here" })).toHaveAttribute(
      "title",
      "Restore turn 1: stop the session before restoring.",
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
        "Agent conversation empty state: archived transcript has no saved timeline items.",
      ),
    ).toHaveClass("agent-panel__empty-chat");
    expect(
      screen.getByTitle("Agent conversation empty-state label: Transcript."),
    ).toHaveTextContent("Transcript");
    expect(
      screen.getByTitle("Agent conversation empty-state helper: no timeline items were saved."),
    ).toHaveTextContent("No timeline items were saved for this session.");
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
    expect(screen.getByTitle("Touched file in turn 1: modified src/a.ts.")).toHaveTextContent(
      "modified src/a.ts",
    );
    expect(screen.getByText("1 files touched")).toHaveAttribute("title", "Turn 1 touched 1 file.");
    expect(screen.getByLabelText("Files: 1")).toBeInTheDocument();
    expect(screen.getByTitle("Turn 1: 0 commands, 1 files")).toBeInTheDocument();
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

    const turnSummary = await screen.findByLabelText("Turn 1 artifact summary");
    expect(turnSummary).toHaveAttribute(
      "title",
      "Conversation turn artifact-summary container for turn 1: 4 artifact category chips.",
    );
    expect(within(turnSummary).getByText("Code 1")).toHaveAttribute(
      "title",
      "Code artifacts touched in turn 1: 1 file.",
    );
    expect(within(turnSummary).getByText("Tests 1")).toBeInTheDocument();
    expect(within(turnSummary).getByText("Docs 1")).toBeInTheDocument();
    expect(within(turnSummary).getByText("Config 1")).toHaveAttribute(
      "title",
      "Config artifacts touched in turn 1: 1 file.",
    );

    const focusedSummary = screen.getByLabelText("Focused turn artifact summary");
    expect(focusedSummary).toHaveAttribute(
      "title",
      "Focused turn artifact-summary container for turn 1: 4 artifact category chips.",
    );
    expect(within(focusedSummary).getByText("Code 1")).toHaveAttribute(
      "title",
      "Code artifacts touched in turn 1: 1 file.",
    );
    expect(within(focusedSummary).getByText("Tests 1")).toBeInTheDocument();
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
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: focused turn 1; Files view active; 1 file, 1 command output, 3 timeline items; turn state Working.",
    );
    expect(within(lens).getByTitle("Agent Lens heading label.")).toHaveTextContent("Agent Lens");
    const filesPanel = within(lens).getByRole("tabpanel", { name: /Files/ });
    expect(filesPanel).toHaveAttribute("id", "agent-lens-sess-1-files-panel");
    expect(filesPanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-files-tab");
    expect(filesPanel).toHaveAttribute(
      "title",
      "Agent Lens Files view for the focused turn: 1 touched file.",
    );
    const filesTab = within(lens).getByRole("tab", { name: /Files/ });
    expect(filesTab).toHaveAttribute("id", "agent-lens-sess-1-files-tab");
    expect(filesTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-files-panel");
    expect(filesTab).toHaveAttribute("tabindex", "0");
    expect(filesTab).toHaveAttribute("title", "Show Agent Lens Files view with 1 touched file.");
    expect(within(filesTab).getByTitle("Agent Lens tab name: Files view.")).toHaveTextContent(
      "Files",
    );
    expect(
      within(filesTab).getByTitle("Agent Lens Files tab count: 1 touched file."),
    ).toHaveTextContent("1");
    const commandsTab = within(lens).getByRole("tab", { name: /Commands/ });
    expect(commandsTab).toHaveAttribute("id", "agent-lens-sess-1-commands-tab");
    expect(commandsTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-commands-panel");
    expect(commandsTab).toHaveAttribute("tabindex", "-1");
    expect(commandsTab).toHaveAttribute(
      "title",
      "Show Agent Lens Commands view with 1 command output.",
    );
    expect(within(commandsTab).getByTitle("Agent Lens tab name: Commands view.")).toHaveTextContent(
      "Commands",
    );
    expect(
      within(commandsTab).getByTitle("Agent Lens Commands tab count: 1 command output."),
    ).toHaveTextContent("1");
    const timelineTab = within(lens).getByRole("tab", { name: /Timeline/ });
    expect(timelineTab).toHaveAttribute("id", "agent-lens-sess-1-timeline-tab");
    expect(timelineTab).toHaveAttribute("aria-controls", "agent-lens-sess-1-timeline-panel");
    expect(timelineTab).toHaveAttribute("tabindex", "-1");
    expect(timelineTab).toHaveAttribute(
      "title",
      "Show Agent Lens Timeline view with 3 recent timeline items.",
    );
    expect(within(timelineTab).getByTitle("Agent Lens tab name: Timeline view.")).toHaveTextContent(
      "Timeline",
    );
    expect(
      within(timelineTab).getByTitle("Agent Lens Timeline tab count: 3 timeline items."),
    ).toHaveTextContent("3");
    expect(within(lens).getByLabelText("Touched files")).toHaveAttribute(
      "title",
      "Agent Lens touched files for the focused turn: 1 file.",
    );
    const preview = within(lens).getByLabelText("Selected file preview");
    expect(preview).toHaveAttribute(
      "title",
      "Selected Agent Lens file preview for src/a.ts; item 1 of 1.",
    );
    expect(within(preview).getByTitle("Agent Lens preview is showing src/a.ts.")).toHaveTextContent(
      "src/a.ts",
    );
    expect(
      within(preview).getByTitle(
        "Selected-file preview placeholder for src/a.ts: no live hunk data available.",
      ),
    ).toHaveTextContent("No live hunk data available for this file.");
    expect(within(lens).getByTitle("Agent Lens file filter label.")).toHaveTextContent(
      "Filter files",
    );
    expect(within(lens).getAllByText("src/a.ts").length).toBeGreaterThan(0);

    await user.click(within(lens).getByRole("tab", { name: /Commands/ }));
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: focused turn 1; Commands view active; 1 file, 1 command output, 3 timeline items; turn state Working.",
    );
    const commandsPanel = within(lens).getByRole("tabpanel", { name: /Commands/ });
    expect(commandsPanel).toHaveAttribute("id", "agent-lens-sess-1-commands-panel");
    expect(commandsPanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-commands-tab");
    expect(commandsPanel).toHaveAttribute(
      "title",
      "Agent Lens Commands view for the focused turn: 1 command output.",
    );
    expect(within(lens).getByLabelText("Command output")).toHaveAttribute(
      "title",
      "Agent Lens command output for the focused turn: 1 command output.",
    );
    const commandEvent = within(lens).getByTitle(
      "Command output captured in Agent Lens for turn 1 at +0s: npm test",
    );
    expect(commandEvent).toHaveTextContent("npm test");
    expect(
      within(commandEvent).getByTitle("Agent Lens command event metadata: turn 1 command at +0s."),
    ).toHaveTextContent("Turn 1 command - +0s");
    expect(
      within(commandEvent).getByTitle("Captured Agent Lens command output for turn 1: npm test"),
    ).toHaveTextContent("npm test");
    const commandFilter = within(commandsPanel).getByLabelText("Filter command output");
    expect(commandFilter).toHaveAttribute("title", "Filter 1 Agent Lens command output by text.");
    expect(
      within(commandsPanel).getByTitle("Agent Lens commands filter controls 1 command output."),
    ).toHaveTextContent("Filter commands");
    expect(within(commandsPanel).getByTitle("Agent Lens command filter label.")).toHaveTextContent(
      "Filter commands",
    );
    expect(
      within(commandsPanel).getByTitle("Showing all 1 Agent Lens command output."),
    ).toHaveTextContent("1 command");

    await user.type(commandFilter, "missing");

    expect(commandsPanel).toHaveAttribute(
      "title",
      "Agent Lens Commands view for the focused turn: showing 0 command outputs from 1 command output.",
    );
    expect(
      within(commandsPanel).getByTitle(
        'No Agent Lens commands match "missing". Clear or change the filter to show captured items.',
      ),
    ).toHaveTextContent("No commands match this filter.");
    expect(
      within(commandsPanel).getByTitle(
        'Agent Lens command output filter empty result for "missing".',
      ),
    ).toHaveTextContent("No commands match this filter.");
    const clearCommandFilter = within(commandsPanel).getByTitle(
      'Clear Agent Lens commands filter "missing" and restore captured items.',
    );
    expect(clearCommandFilter).toHaveAttribute(
      "title",
      'Clear Agent Lens commands filter "missing" and restore captured items.',
    );
    expect(
      within(clearCommandFilter).getByTitle("Agent Lens clear-command-filter label: Clear."),
    ).toHaveTextContent("Clear");

    await user.click(clearCommandFilter);

    expect(
      within(commandsPanel).getByTitle(
        "Command output captured in Agent Lens for turn 1 at +0s: npm test",
      ),
    ).toHaveTextContent("npm test");

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: focused turn 1; Timeline view active; 1 file, 1 command output, 3 timeline items; turn state Working.",
    );
    const timelinePanel = within(lens).getByRole("tabpanel", { name: /Timeline/ });
    expect(timelinePanel).toHaveAttribute("id", "agent-lens-sess-1-timeline-panel");
    expect(timelinePanel).toHaveAttribute("aria-labelledby", "agent-lens-sess-1-timeline-tab");
    expect(timelinePanel).toHaveAttribute(
      "title",
      "Agent Lens Timeline view for the focused turn: 3 timeline items.",
    );
    expect(within(lens).getByLabelText("Recent timeline")).toHaveAttribute(
      "title",
      "Agent Lens recent timeline for the focused turn: 3 timeline items.",
    );
    const timelineCommandEvent = within(lens).getByTitle(
      "Timeline command event captured in Agent Lens for turn 1 at +0s: npm test",
    );
    expect(timelineCommandEvent).toHaveTextContent(/Turn 1 - Command - \+0s/);
    expect(
      within(timelineCommandEvent).getByTitle(
        "Agent Lens timeline event metadata: turn 1 Command event at +0s.",
      ),
    ).toHaveTextContent("Turn 1 - Command - +0s");
    expect(
      within(timelineCommandEvent).getByTitle(
        "Captured Agent Lens timeline text for turn 1 Command event: npm test",
      ),
    ).toHaveTextContent("npm test");
    const timelineAgentEvent = within(lens).getByTitle(
      "Timeline agent event captured in Agent Lens for turn 1 at +0s: Voy con ello",
    );
    expect(timelineAgentEvent).toHaveTextContent("Voy con ello");
    expect(
      within(timelineAgentEvent).getByTitle(
        "Agent Lens timeline event metadata: turn 1 Agent event at +0s.",
      ),
    ).toHaveTextContent("Turn 1 - Agent - +0s");
    expect(
      within(timelineAgentEvent).getByTitle(
        "Captured Agent Lens timeline text for turn 1 Agent event: Voy con ello",
      ),
    ).toHaveTextContent("Voy con ello");
    const timelineFilter = within(timelinePanel).getByLabelText("Filter timeline events");
    expect(timelineFilter).toHaveAttribute(
      "title",
      "Filter 3 Agent Lens timeline events by text or event type.",
    );
    expect(
      within(timelinePanel).getByTitle("Agent Lens timeline filter controls 3 timeline events."),
    ).toHaveTextContent("Filter timeline");
    expect(within(timelinePanel).getByTitle("Agent Lens timeline filter label.")).toHaveTextContent(
      "Filter timeline",
    );
    expect(
      within(timelinePanel).getByTitle("Showing all 3 Agent Lens timeline events."),
    ).toHaveTextContent("3 events");

    await user.type(timelineFilter, "Agent");

    expect(timelinePanel).toHaveAttribute(
      "title",
      "Agent Lens Timeline view for the focused turn: showing 1 timeline item from 3 timeline items.",
    );
    expect(within(timelinePanel).getByLabelText("Recent timeline")).toHaveAttribute(
      "title",
      "Filtered Agent Lens recent timeline for the focused turn: 1 timeline item.",
    );
    expect(
      within(timelinePanel).queryByTitle(
        "Timeline command event captured in Agent Lens for turn 1 at +0s: npm test",
      ),
    ).not.toBeInTheDocument();
    expect(
      within(timelinePanel).getByTitle(
        "Timeline agent event captured in Agent Lens for turn 1 at +0s: Voy con ello",
      ),
    ).toHaveTextContent("Voy con ello");

    await user.keyboard("{Escape}");

    expect(
      within(timelinePanel).getByTitle(
        "Timeline command event captured in Agent Lens for turn 1 at +0s: npm test",
      ),
    ).toHaveTextContent("npm test");
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
    const filesTab = within(lens).getByRole("tab", { name: /Files/ });
    const commandsTab = within(lens).getByRole("tab", { name: /Commands/ });
    const timelineTab = within(lens).getByRole("tab", { name: /Timeline/ });

    expect(filesTab).toHaveAttribute("aria-selected", "true");
    filesTab.focus();

    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(commandsTab).toHaveFocus());
    expect(commandsTab).toHaveAttribute("aria-selected", "true");
    expect(commandsTab).toHaveAttribute("tabindex", "0");
    expect(within(lens).getByRole("tabpanel", { name: /Commands/ })).toHaveAttribute(
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
    expect(within(lens).getByRole("tabpanel", { name: /Files/ })).toHaveAttribute(
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
    const filter = within(lens).getByLabelText("Filter touched files");
    expect(filter).toHaveAttribute("aria-describedby", "agent-lens-sess-1-file-filter-status");
    expect(filter).toHaveAttribute(
      "title",
      "Filter 2 Agent Lens touched files by path, change type, status, or artifact category.",
    );
    expect(within(lens).getByTitle("Showing all 2 Agent Lens touched files.")).toHaveTextContent(
      "2 files",
    );

    await user.type(filter, "docs");

    expect(filter).toHaveAttribute(
      "title",
      "Filter 2 Agent Lens touched files by path, change type, status, or artifact category. Press Escape to clear the filter.",
    );
    expect(
      within(lens).getByTitle("Showing 1 of 2 Agent Lens touched files after filtering."),
    ).toHaveTextContent("1 of 2 files");
    expect(within(lens).getAllByText("docs/guide.md").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/a.ts")).not.toBeInTheDocument();
    const clearButton = within(lens).getByRole("button", { name: "Clear" });
    expect(clearButton).toHaveAttribute(
      "title",
      "Clear Agent Lens file filter and show all 2 touched files; currently showing 1 file.",
    );

    await user.clear(filter);
    await user.type(filter, "missing");

    expect(
      within(lens).getByTitle(
        'No Agent Lens files match "missing". Clear or change the filter to show touched files.',
      ),
    ).toHaveTextContent("No files match this filter.");
    expect(
      within(lens).getByTitle('Agent Lens file filter empty result for "missing".'),
    ).toHaveTextContent("No files match this filter.");
    const emptyClear = within(lens).getByTitle(
      'Clear Agent Lens file filter "missing" and restore touched files.',
    );
    expect(emptyClear).toHaveAttribute(
      "title",
      'Clear Agent Lens file filter "missing" and restore touched files.',
    );

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

    await user.click(within(lens).getByRole("tab", { name: /Commands/ }));
    expect(within(lens).getByRole("tabpanel", { name: /Commands/ })).toHaveAttribute(
      "title",
      "Agent Lens Commands view for the current session: 0 command outputs.",
    );
    expect(
      within(lens).getByTitle("Agent Lens has no command output in the current session."),
    ).toHaveTextContent("No commands captured yet.");

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
    expect(within(lens).getByRole("tabpanel", { name: /Timeline/ })).toHaveAttribute(
      "title",
      "Agent Lens Timeline view for the current session: 0 timeline items.",
    );
    expect(
      within(lens).getByTitle("Agent Lens has no timeline events in the current session."),
    ).toHaveTextContent("No timeline captured yet.");
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
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: focused turn 2; Files view active; 1 file, 1 command output, 2 timeline items; turn state Working.",
    );
    expect(within(lens).getByRole("button", { name: "Focus" })).toHaveAttribute(
      "title",
      "Scope Agent Lens to focused turn 2.",
    );
    expect(within(lens).getByRole("button", { name: "Session" })).toHaveAttribute(
      "title",
      "Scope Agent Lens to the full session.",
    );
    expect(within(lens).getByTitle("Agent Lens is showing focused turn 2.")).toHaveTextContent(
      "Turn 2",
    );
    expect(within(lens).getByTitle("Agent Lens inspector for focused turn 2.")).toHaveTextContent(
      "Agent Lens",
    );
    expect(within(lens).getByTitle("Agent Lens heading label.")).toHaveTextContent("Agent Lens");
    expect(within(lens).getByLabelText("Agent Lens scope")).toHaveAttribute(
      "title",
      "Agent Lens scope controls are focused on turn 2.",
    );
    expect(
      within(lens).getByTitle(
        "Agent Lens metrics summarize Working state, 1 file, and 0 restorable turn checkpoints for the focused turn.",
      ),
    ).toHaveTextContent("Working");
    expect(within(lens).getByTitle("Agent Lens turn state value: Working.")).toHaveTextContent(
      "Working",
    );
    expect(within(lens).getByTitle("Current Agent Lens turn state: Working.")).toHaveTextContent(
      "Turn state",
    );
    expect(
      within(lens).getByTitle("Agent Lens focused scope file count value: 1 file."),
    ).toHaveTextContent("1");
    expect(within(lens).getByTitle("Agent Lens focused scope includes 1 file.")).toHaveTextContent(
      "Focused files",
    );
    expect(
      within(lens).getByTitle(
        "Agent Lens restore-point value: 0 of 2 turn checkpoints are restorable.",
      ),
    ).toHaveTextContent("0/2");
    expect(
      within(lens).getByTitle(
        "Agent Lens restore-point metric: 0 restore points from 2 turn checkpoints.",
      ),
    ).toHaveTextContent("Restore points");
    expect(
      within(lens).getByTitle("Agent Lens view tabs: 1 file, 1 command output, 2 timeline items."),
    ).toHaveAttribute("role", "tablist");
    const focusedFileRow = within(lens).getByTitle(
      "Agent Lens touched file for turn 2: created src/second.ts.",
    );
    expect(focusedFileRow).toHaveTextContent("src/second.ts");
    expect(
      within(focusedFileRow).getByTitle("Agent Lens file row timing: turn 2 at +0s."),
    ).toHaveTextContent("Turn 2 - +0s");
    expect(
      within(focusedFileRow).getByTitle("Agent Lens file row path: src/second.ts."),
    ).toHaveTextContent("src/second.ts");
    expect(
      within(focusedFileRow).getByTitle("Code Agent Lens file row change type: created."),
    ).toHaveTextContent("created");
    expect(within(lens).getAllByText("src/second.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/first.ts")).not.toBeInTheDocument();
    expect(within(lens).getByText("Focused files")).toBeInTheDocument();
    expect(
      within(lens).getByTitle("Agent Lens file filter controls 1 touched file."),
    ).toHaveTextContent("Filter files");
    expect(within(lens).getByTitle("Agent Lens file filter label.")).toHaveTextContent(
      "Filter files",
    );

    await user.click(within(lens).getByRole("tab", { name: /Commands/ }));
    expect(within(lens).getByText("npm test second")).toBeInTheDocument();
    expect(within(lens).queryByText("npm test first")).not.toBeInTheDocument();

    const turnMap = screen.getByLabelText("Turn map");
    expect(screen.getByLabelText("Agent session overview")).toHaveAttribute(
      "title",
      "Agent session overview: 2 turns, 2 messages, 2 commands, 2 files; latest activity: npm test second; turn map 2 turns.",
    );
    expect(
      screen.getByTitle(
        "Agent session overview metrics: 2 turns, 2 messages, 2 commands, 2 files.",
      ),
    ).toHaveTextContent("Turns");
    expect(turnMap).toHaveAttribute("title", "Agent session overview turn map: 2 turns.");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });
    expect(firstTurnButton).toHaveAttribute(
      "title",
      "Turn 1: 1 commands, 1 files - Recent command: npm test first",
    );
    expect(
      within(firstTurnButton).getByTitle("Agent session overview turn-map label: turn 1."),
    ).toHaveTextContent("T1");
    expect(
      within(firstTurnButton).getByTitle(
        "Agent session overview turn-map command summary for turn 1: npm test first.",
      ),
    ).toHaveTextContent("cmd npm test first");
    expect(
      within(firstTurnButton).getByTitle(
        "Agent session overview turn-map file count for turn 1: 1 file.",
      ),
    ).toHaveTextContent("1 files");

    await user.click(firstTurnButton);
    expect(within(lens).getByText("Turn 1")).toBeInTheDocument();
    expect(within(lens).getByText("npm test first")).toBeInTheDocument();
    expect(within(lens).queryByText("npm test second")).not.toBeInTheDocument();

    await user.click(within(lens).getByRole("button", { name: "Session" }));
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: current session with 2 turns; Commands view active; 2 files, 2 command outputs, 4 timeline items; turn state Working.",
    );
    expect(within(lens).getByRole("button", { name: "Session" })).toHaveAttribute(
      "title",
      "Scope Agent Lens to the full session.",
    );
    await user.click(within(lens).getByRole("tab", { name: /Files/ }));
    expect(lens).toHaveAttribute(
      "title",
      "Agent Lens inspector: current session with 2 turns; Files view active; 2 files, 2 command outputs, 4 timeline items; turn state Working.",
    );
    expect(within(lens).getByRole("tabpanel", { name: /Files/ })).toHaveAttribute(
      "title",
      "Agent Lens Files view for the current session: 2 touched files.",
    );
    expect(
      within(lens).getByTitle("Agent Lens is showing the full session with 2 turns."),
    ).toHaveTextContent("2 turns");
    expect(
      within(lens).getByTitle("Agent Lens inspector for the full session with 2 turns."),
    ).toHaveTextContent("Agent Lens");
    expect(within(lens).getByLabelText("Agent Lens scope")).toHaveAttribute(
      "title",
      "Agent Lens scope controls switch between the focused turn and the full session.",
    );
    expect(
      within(lens).getByTitle(
        "Agent Lens metrics summarize Working state, 2 files, and 0 restorable turn checkpoints for the current session.",
      ),
    ).toHaveTextContent("2");
    expect(
      within(lens).getByTitle("Agent Lens session scope file count value: 2 files."),
    ).toHaveTextContent("2");
    expect(within(lens).getByTitle("Agent Lens session scope includes 2 files.")).toHaveTextContent(
      "Session files",
    );
    expect(
      within(lens).getByTitle(
        "Agent Lens restore-point value: 0 of 2 turn checkpoints are restorable.",
      ),
    ).toHaveTextContent("0/2");
    expect(
      within(lens).getByTitle(
        "Agent Lens view tabs: 2 files, 2 command outputs, 4 timeline items.",
      ),
    ).toHaveAttribute("role", "tablist");
    expect(within(lens).getByText("Session files")).toBeInTheDocument();
    expect(
      within(lens).getByTitle("Agent Lens file filter controls 2 touched files."),
    ).toHaveTextContent("Filter files");
    expect(within(lens).getByTitle("Agent Lens file filter label.")).toHaveTextContent(
      "Filter files",
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
    const preview = within(lens).getByLabelText("Selected file preview");
    const previewActions = within(preview).getByLabelText("Preview actions for src/agent-view.tsx");
    expect(previewActions).toHaveAttribute(
      "title",
      "Agent Lens preview actions for src/agent-view.tsx: open, ask, and revert controls for the selected preview file.",
    );
    expect(
      within(previewActions).getByRole("button", { name: "Open selected preview file" }),
    ).toHaveAttribute("title", "Open src/agent-view.tsx from Agent Lens in the workspace.");
    expect(
      within(previewActions).getByTitle("Agent Lens preview action label: Open."),
    ).toHaveTextContent("Open");
    expect(
      within(previewActions).getByRole("button", { name: "Ask about selected preview file" }),
    ).toHaveAttribute("title", "Draft an Agent Lens follow-up prompt for src/agent-view.tsx.");
    expect(
      within(previewActions).getByTitle("Agent Lens preview action label: Ask."),
    ).toHaveTextContent("Ask");
    expect(
      within(previewActions).getByRole("button", { name: "Revert selected preview file" }),
    ).toHaveAttribute(
      "title",
      "Selected preview file: Stop the session before reverting src/agent-view.tsx.",
    );
    expect(
      within(previewActions).getByTitle("Agent Lens preview action label: Revert."),
    ).toHaveTextContent("Revert");
    const fileActions = within(lens).getByTitle(
      "Agent Lens file actions for src/agent-view.tsx: preview, open, ask, and revert controls.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "File actions for src/agent-view.tsx");
    expect(within(fileActions).getByRole("button", { name: "Preview" })).toHaveAttribute(
      "title",
      "Previewing Agent Lens details for src/agent-view.tsx.",
    );
    expect(
      within(fileActions).getByTitle("Agent Lens file action label: Preview."),
    ).toHaveTextContent("Preview");
    expect(within(fileActions).getByRole("button", { name: "Open" })).toHaveAttribute(
      "title",
      "Open src/agent-view.tsx from Agent Lens in the workspace.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Open.")).toHaveTextContent(
      "Open",
    );
    expect(within(fileActions).getByRole("button", { name: "Ask" })).toHaveAttribute(
      "title",
      "Draft an Agent Lens follow-up prompt for src/agent-view.tsx.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Ask.")).toHaveTextContent(
      "Ask",
    );
    expect(within(fileActions).getByRole("button", { name: "Revert" })).toHaveAttribute(
      "title",
      "Stop the session before reverting src/agent-view.tsx.",
    );
    expect(
      within(fileActions).getByTitle("Agent Lens file action label: Revert."),
    ).toHaveTextContent("Revert");
    await user.click(within(lens).getByRole("button", { name: "Open" }));

    expect(openFile).toHaveBeenCalledWith("/r/a", "src/agent-view.tsx", true);

    await user.click(
      within(previewActions).getByRole("button", { name: "Open selected preview file" }),
    );

    expect(openFile).toHaveBeenCalledTimes(2);
    expect(openFile).toHaveBeenLastCalledWith("/r/a", "src/agent-view.tsx", true);

    await user.click(within(lens).getByRole("button", { name: "Ask" }));

    let composerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Focus on src/agent-view.tsx.");
    expect(composerValue).toContain("It was modified in turn 1.");
    expect(composerValue).toContain("Artifact category: Code.");
    expect(composerValue).toContain("Diff summary: 1 hunk - +1 / -1.");
    expect(composerValue).toContain("next concrete edit or verification step");

    await user.clear(screen.getByLabelText("Message Codex"));
    await user.click(
      within(previewActions).getByRole("button", { name: "Ask about selected preview file" }),
    );

    composerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Focus on src/agent-view.tsx.");
    expect(composerValue).toContain("Diff summary: 1 hunk - +1 / -1.");
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
    const context = within(lens).getByLabelText("Live context for src/agent-view.tsx");
    expect(context).toHaveAttribute(
      "title",
      "Live Agent Lens context for src/agent-view.tsx: repo status and diff chips.",
    );
    expect(
      within(context).getByTitle("Live repo status for src/agent-view.tsx: modified."),
    ).toHaveTextContent("modified");
    expect(
      within(context).getByTitle("Live repo status for src/agent-view.tsx: staged."),
    ).toHaveTextContent("staged");
    expect(
      within(context).getByTitle("Live diff summary for src/agent-view.tsx: 2 added, 1 removed."),
    ).toHaveTextContent("+2 / -1");

    const preview = within(lens).getByLabelText("Selected file preview");
    expect(preview).toHaveAttribute(
      "title",
      "Selected Agent Lens file preview for src/agent-view.tsx; item 1 of 1.",
    );
    expect(
      within(preview).getByTitle("Selected-file preview area for the active Agent Lens file."),
    ).toHaveTextContent("Preview");
    expect(
      within(preview).getByTitle("Agent Lens preview is showing src/agent-view.tsx."),
    ).toHaveTextContent("src/agent-view.tsx");
    expect(within(preview).getByText("src/agent-view.tsx")).toBeInTheDocument();
    const previewDetails = within(preview).getByLabelText("Preview details for src/agent-view.tsx");
    expect(previewDetails).toHaveAttribute(
      "title",
      "Agent Lens preview details for src/agent-view.tsx: hunk summary and first-hunk location.",
    );
    expect(
      within(previewDetails).getByTitle(
        "Selected-file preview summary for src/agent-view.tsx: 1 hunk, 2 added, 1 removed.",
      ),
    ).toHaveTextContent("1 hunk - +2 / -1");
    expect(
      within(previewDetails).getByTitle(
        "Selected-file preview detail for src/agent-view.tsx: First hunk @@ -1 +1.",
      ),
    ).toHaveTextContent("First hunk @@ -1 +1.");
    expect(within(lens).getByRole("button", { name: "Preview" })).toHaveAttribute(
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
    const preview = within(lens).getByLabelText("Selected file preview");
    expect(preview).toHaveAttribute(
      "title",
      "Selected Agent Lens file preview for src/agent-view.tsx; item 1 of 2. Use arrow keys to move between previewed files.",
    );
    expect(
      within(preview).getByTitle("Agent Lens preview position: 1 of 2 visible files."),
    ).toHaveTextContent("1 / 2");
    const navigation = within(preview).getByLabelText("Preview navigation");
    expect(navigation).toHaveAttribute(
      "title",
      "Agent Lens preview navigation for src/agent-view.tsx: move through 2 visible files.",
    );
    expect(within(navigation).getByRole("button", { name: "Previous" })).toHaveAttribute(
      "title",
      "Show the previous Agent Lens preview file: docs/agent-view.md.",
    );
    expect(within(navigation).getByRole("button", { name: "Next" })).toHaveAttribute(
      "title",
      "Show the next Agent Lens preview file: docs/agent-view.md.",
    );

    await user.click(within(navigation).getByRole("button", { name: "Next" }));

    expect(
      within(preview).getByTitle("Agent Lens preview is showing docs/agent-view.md."),
    ).toHaveTextContent("docs/agent-view.md");
    expect(
      within(preview).getByTitle("Agent Lens preview position: 2 of 2 visible files."),
    ).toHaveTextContent("2 / 2");

    preview.focus();
    await user.keyboard("{ArrowLeft}");

    expect(
      within(preview).getByTitle("Agent Lens preview is showing src/agent-view.tsx."),
    ).toHaveTextContent("src/agent-view.tsx");

    await user.keyboard("{ArrowRight}");

    expect(
      within(preview).getByTitle("Agent Lens preview is showing docs/agent-view.md."),
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
    const codeGroup = within(lens).getByLabelText("Code files");
    expect(codeGroup).toHaveAttribute(
      "title",
      "Agent Lens Code file group contains 1 touched file.",
    );
    expect(within(codeGroup).getByTitle("Code artifact group contains 1 file.")).toHaveTextContent(
      "1",
    );
    expect(
      within(codeGroup).getByTitle("Code Agent Lens group heading for 1 touched file."),
    ).toHaveTextContent("Code");
    expect(
      within(codeGroup).getByTitle("Agent Lens file group kind label: Code."),
    ).toHaveTextContent("Code");
    const codeFileRow = within(codeGroup).getByTitle(
      "Agent Lens touched file for the session: modified src/agent-view.tsx.",
    );
    expect(codeFileRow).toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle("Agent Lens file row scope: session change log at +0s."),
    ).toHaveTextContent("Session - +0s");
    expect(
      within(codeFileRow).getByTitle("Agent Lens file row path: src/agent-view.tsx."),
    ).toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle("Code Agent Lens file row change type: modified."),
    ).toHaveTextContent("modified");
    const testsGroup = within(lens).getByLabelText("Tests files");
    expect(
      within(testsGroup).getByTitle("Tests Agent Lens group heading for 1 touched file."),
    ).toHaveTextContent("Tests");
    expect(
      within(testsGroup).getByTitle("Agent Lens file group kind label: Tests."),
    ).toHaveTextContent("Tests");
    expect(within(testsGroup).getByText("src/agent-view.test.tsx")).toBeInTheDocument();
    expect(
      within(within(lens).getByLabelText("Docs files")).getByText("docs/agent-view.md"),
    ).toBeInTheDocument();
    expect(
      within(within(lens).getByLabelText("Config files")).getByText("package.json"),
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

    const focus = await screen.findByLabelText("Focused turn");
    expect(focus).toHaveAttribute("title", "Focused turn card container: selected turn 2.");
    expect(within(focus).getByTitle("Focused turn heading label: Focused turn.")).toHaveTextContent(
      "Focused turn",
    );
    expect(within(focus).getByText("Turn 2")).toBeInTheDocument();
    expect(within(focus).getByText("Second done")).toBeInTheDocument();
    expect(
      within(focus).getByTitle(
        "Focused turn files container for turn 2: 4 visible touched files, plus 1 hidden touched file.",
      ),
    ).toHaveAttribute("aria-label", "Focused turn files");
    expect(
      within(focus).getByTitle("Focused turn touched file row for turn 2: created src/second.ts."),
    ).toHaveTextContent("created src/second.ts");
    expect(
      within(focus).getByTitle(
        "Focused turn hidden file overflow: 1 additional touched file for turn 2.",
      ),
    ).toHaveTextContent("+1 more");
    expect(
      within(focus).getByTitle(
        "Focused turn restore container for turn 2: stop the session before restoring.",
      ),
    ).toHaveClass("agent-panel__turn-focus-actions");

    await user.click(within(screen.getByLabelText("Turn map")).getByRole("button", { name: /T1/ }));

    expect(within(focus).getByText("Turn 1")).toBeInTheDocument();
    expect(within(focus).getByTitle("Focused turn index label: Turn 1.")).toHaveTextContent(
      "Turn 1",
    );
    expect(within(focus).getByText("+0s")).toHaveAttribute(
      "title",
      "Focused turn 1 timing relative to the first turn: +0s.",
    );
    expect(within(focus).getByText("3 messages / 1 commands / 1 files")).toHaveAttribute(
      "title",
      "Focused turn 1 transcript, command, and file counts: 3 messages / 1 commands / 1 files.",
    );
    expect(within(focus).getByText("npm test first")).toHaveAttribute(
      "title",
      "Most recent captured activity for focused turn 1: npm test first",
    );
    expect(within(focus).queryByText("Second done")).not.toBeInTheDocument();
    expect(
      within(focus).getByTitle("Focused turn facts container for turn 1: 1 command, 1 file."),
    ).toHaveClass("agent-panel__turn-focus-facts");
    expect(within(focus).getByText("1 commands")).toHaveAttribute(
      "title",
      "Focused turn 1 has 1 command.",
    );
    expect(within(focus).getByText("1 files")).toHaveAttribute(
      "title",
      "Focused turn 1 has 1 file.",
    );
    const artifactSummary = within(focus).getByLabelText("Focused turn artifact summary");
    expect(artifactSummary).toHaveAttribute(
      "title",
      "Focused turn artifact-summary container for turn 1: 1 artifact category chip.",
    );
    const commandSummary = within(focus).getByLabelText("Focused turn command summary");
    expect(commandSummary).toHaveAttribute(
      "title",
      "Focused turn command-summary container for turn 1: 1 recent command summary.",
    );
    expect(within(commandSummary).getByText("Recent command npm test first")).toHaveAttribute(
      "title",
      "Compact recent command output summary for turn 1: npm test first",
    );
    expect(scrollIntoViewMock).toHaveBeenCalled();
    const firstTurnArticle = screen.getByText("First task").closest("article");
    expect(firstTurnArticle).toHaveAttribute("aria-current", "true");
    expect(firstTurnArticle).toHaveAttribute(
      "title",
      "Conversation turn card container for turn 1: focused; 3 messages / 1 commands / 1 files.",
    );
    expect(
      within(firstTurnArticle!).getByTitle(
        "Conversation turn header container for turn 1: title, metadata, and copy control.",
      ),
    ).toHaveClass("agent-panel__chat-turn-head");
    expect(
      within(firstTurnArticle!).getByTitle(
        "Conversation turn title container for turn 1: Turn label and transcript summary.",
      ),
    ).toHaveClass("agent-panel__chat-turn-title");
    expect(
      within(firstTurnArticle!).getByTitle(
        "Conversation turn metadata container for turn 1: timing, touched-file count, and copy control.",
      ),
    ).toHaveClass("agent-panel__chat-turn-meta");
    const turnCommandSummary = within(firstTurnArticle!).getByLabelText("Turn 1 command summary");
    expect(turnCommandSummary).toHaveAttribute(
      "title",
      "Conversation turn command-summary container for turn 1: 1 recent command summary.",
    );
    expect(within(turnCommandSummary).getByText("Recent command npm test first")).toHaveAttribute(
      "title",
      "Compact recent command output summary for turn 1: npm test first",
    );
    expect(
      within(firstTurnArticle!).getByTitle(
        "Conversation turn touched-files container for turn 1: 1 touched-file chip.",
      ),
    ).toHaveClass("agent-panel__chat-turn-files");

    expect(within(focus).queryByRole("button", { name: "Jump" })).not.toBeInTheDocument();
    expect(within(focus).queryByRole("button", { name: "Copy focus" })).not.toBeInTheDocument();
    expect(within(focus).queryByRole("button", { name: "Continue" })).not.toBeInTheDocument();
    expect(within(focus).queryByRole("button", { name: "Review" })).not.toBeInTheDocument();
    expect(within(focus).queryByRole("button", { name: "Test" })).not.toBeInTheDocument();
    expect(within(focus).queryByRole("button", { name: "Handoff" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Details" })).toHaveAttribute(
      "title",
      "Show session details, restore points, files, commands, and timeline.",
    );
    await user.click(screen.getByRole("button", { name: "Details" }));
    expect(
      screen.getByTitle(
        "Session details: turn map, current activity, restore points, and Agent Lens.",
      ),
    ).toHaveClass("agent-panel__details-head");
    expect(within(focus).getByRole("button", { name: "Restore here" })).toHaveAttribute(
      "title",
      "Restore turn 1: stop the session before restoring.",
    );
    expect(screen.getByLabelText("Message Codex")).toHaveValue("");
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
    expect(within(lens).getByLabelText("Touched files")).toBeInTheDocument();
    expect(within(lens).getAllByText("src/session-only.ts").length).toBeGreaterThan(0);
    expect(
      within(lens).getByTitle("Agent Lens preview is showing src/session-only.ts."),
    ).toHaveTextContent("src/session-only.ts");
    expect(
      within(lens).getByTitle(
        "Selected-file preview placeholder for src/session-only.ts: no live hunk data available.",
      ),
    ).toHaveTextContent("No live hunk data available for this file.");
    expect(within(lens).getByText("Session - +0s")).toBeInTheDocument();
    const fileActions = within(lens).getByTitle(
      "Agent Lens file actions for src/session-only.ts: preview, open, and ask controls.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "File actions for src/session-only.ts");
    expect(within(fileActions).getByRole("button", { name: "Preview" })).toHaveAttribute(
      "title",
      "Previewing Agent Lens details for src/session-only.ts.",
    );
    expect(
      within(fileActions).getByTitle("Agent Lens file action label: Preview."),
    ).toHaveTextContent("Preview");
    expect(within(fileActions).getByRole("button", { name: "Open" })).toHaveAttribute(
      "title",
      "Open src/session-only.ts from Agent Lens in the workspace.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Open.")).toHaveTextContent(
      "Open",
    );
    expect(within(fileActions).getByRole("button", { name: "Ask" })).toHaveAttribute(
      "title",
      "Draft an Agent Lens follow-up prompt for src/session-only.ts.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Ask.")).toHaveTextContent(
      "Ask",
    );
    expect(within(lens).queryByTitle(/Revert src\/session-only\.ts/)).not.toBeInTheDocument();
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
    const fileFilter = within(lens).getByLabelText("Filter touched files");
    expect(fileFilter).toHaveAttribute(
      "title",
      "Filter 2 Agent Lens touched files by path, change type, status, or artifact category.",
    );
    expect(within(lens).getByTitle("Showing all 2 Agent Lens touched files.")).toHaveTextContent(
      "2 files",
    );
    expect(
      within(lens).getByTitle("Agent Lens file filter controls 2 touched files."),
    ).toHaveTextContent("Filter files");

    await user.type(fileFilter, "beta");

    expect(within(lens).getAllByText("src/beta.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/alpha.ts")).not.toBeInTheDocument();
    expect(within(lens).getByLabelText("Touched files")).toHaveAttribute(
      "title",
      "Filtered Agent Lens touched files for the current session: 1 file.",
    );
    expect(
      within(lens).getByTitle("Showing 1 of 2 Agent Lens touched files after filtering."),
    ).toHaveTextContent("1 of 2 files");
    expect(
      within(lens).getByTitle("Agent Lens file filter is showing 1 file from 2 touched files."),
    ).toHaveTextContent("Filter files");

    await user.clear(within(lens).getByLabelText("Filter touched files"));
    await user.type(within(lens).getByLabelText("Filter touched files"), "removed");

    expect(within(lens).getByText("No files match this filter.")).toBeInTheDocument();
    expect(
      within(lens).getByTitle(
        'No Agent Lens files match "removed". Clear or change the filter to show touched files.',
      ),
    ).toHaveTextContent("No files match this filter.");
    expect(
      within(lens).getByTitle('Clear Agent Lens file filter "removed" and restore touched files.'),
    ).toHaveTextContent("Clear");
    expect(
      within(lens).getByTitle("Showing 0 of 2 Agent Lens touched files after filtering."),
    ).toHaveTextContent("0 of 2 files");
    expect(
      within(lens).getByTitle("Agent Lens file filter is showing 0 files from 2 touched files."),
    ).toHaveTextContent("Filter files");

    await user.clear(within(lens).getByLabelText("Filter touched files"));
    await user.type(within(lens).getByLabelText("Filter touched files"), "staged");

    expect(within(lens).getAllByText("src/beta.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/alpha.ts")).not.toBeInTheDocument();

    await user.clear(within(lens).getByLabelText("Filter touched files"));
    await user.type(within(lens).getByLabelText("Filter touched files"), "code");

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
      within(lens).getByTitle("Agent Lens has no touched files in the current session."),
    ).toHaveTextContent("No touched files yet.");
  });

  it("confirms and reverts completed sessions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({ status: "completed", pid: null, exit_code: 0 }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const revert = await screen.findByRole("button", { name: "Revert" });
    expect(revert).toHaveAttribute("title", "Codex revert control for a: revert session changes.");
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

    await user.click(await screen.findByRole("button", { name: "Stop" }));

    expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
    await waitFor(() => expect(listAgentSessionsMock).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("Session complete")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stop" })).toHaveAttribute(
      "title",
      "Codex stop control for a: session is not running.",
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

    await user.click(await screen.findByTitle("Revert src/a.ts from turn 1 checkpoint."));

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

    const conversation = screen.getByLabelText("Agent conversation");
    expect(await within(conversation).findByText("Second done")).toBeInTheDocument();
    const lens = await screen.findByLabelText("Agent Lens");
    expect(
      within(lens).getByTitle(
        "Agent Lens restore-point value: 1 of 1 turn checkpoints are restorable. Latest restorable turn: 1.",
      ),
    ).toHaveTextContent("1/1");
    expect(
      within(lens).getByTitle(
        "Agent Lens restore-point metric: 1 restore point from 1 turn checkpoint. Latest restorable turn is 1.",
      ),
    ).toHaveTextContent("Restore points");
    await user.click(within(screen.getByLabelText("Turn map")).getByRole("button", { name: /T1/ }));

    const restore = await screen.findByRole("button", { name: "Restore here" });
    expect(restore).toHaveAttribute(
      "title",
      "Restore turn 1: return files and chat view to this turn.",
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

    const button = await screen.findByRole("button", { name: "Revert" });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "Codex revert control for a: no reversible checkpoint.",
    );
    expect(screen.getByTitle("Agent revert control label: Revert.")).toHaveTextContent("Revert");
  });
});
