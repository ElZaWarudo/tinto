import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import type { ReactElement } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSession, FileDiff, RepoDelta } from "../../bus/contract";

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
const stopAgentSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
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

function renderWithWorkspaceActions(
  ui: ReactElement,
  actions: Partial<WorkspaceActions>,
) {
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
    scrollIntoViewMock.mockClear();
    writeClipboardTextMock.mockClear();
    writeAgentSessionInputMock.mockClear();
    listAgentSessionsMock.mockClear();
    getAgentJournalSessionMock.mockClear();
    getAgentJournalSessionMock.mockResolvedValue(null);
    revertSessionMock.mockClear();
    revertSessionTurnFileMock.mockClear();
    stopAgentSessionMock.mockClear();
    confirmMock.mockClear();
  });

  it("renders a product agent interface instead of a terminal surface", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    expect(screen.getByTitle("Agent session status strip: loading session.")).toHaveTextContent(
      "Loading session",
    );
    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("a")).toBeInTheDocument();
    const conversation = screen.getByLabelText("Agent conversation");
    expect(conversation).toHaveAttribute(
      "title",
      "Agent conversation transcript: ready for the first turn.",
    );
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
    expect(screen.getByTitle("Transcript search count: showing all 0 transcript turns."))
      .toHaveTextContent("All turns");
    expect(
      screen.getByTitle(
        "Transcript search: find messages, commands, and files across 0 transcript turns.",
      ),
    ).toHaveTextContent("Search transcript");
    expect(screen.getByTitle("Transcript search label: Search transcript."))
      .toHaveTextContent("Search transcript");
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
    expect(within(overview).getByTitle("Agent session overview latest-activity label."))
      .toHaveTextContent("Latest activity");
    expect(
      within(overview).getByTitle(
        "Agent session overview latest activity: waiting for the first turn.",
      ),
    ).toHaveTextContent("Waiting for the first turn.");
    const activity = screen.getByLabelText("Agent activity");
    expect(within(activity).getByText("Agent is working")).toBeInTheDocument();
    expect(within(activity).getByText("filesystem checkpoint")).toBeInTheDocument();
    expect(
      within(activity).getByTitle("Agent activity facts: turns, files, checkpoint, and stream throughput."),
    ).toHaveTextContent("0 turns");
    expect(within(activity).getByTitle("Agent activity turn count: 0 turns.")).toHaveTextContent(
      "0 turns",
    );
    expect(within(activity).getByTitle("Agent activity touched-file count: 0 files."))
      .toHaveTextContent("0 files");
    expect(
      within(activity).getByTitle("Agent activity checkpoint fact: filesystem checkpoint."),
    ).toHaveTextContent("filesystem checkpoint");
    expect(within(activity).getByTitle("Agent activity stream throughput fact: Stream quiet."))
      .toHaveTextContent("Stream quiet");
    expect(screen.getByTitle("Agent session status facet: Running.")).toHaveTextContent("Running");
    expect(screen.getByTitle("Agent turn status facet: Working.")).toHaveTextContent("Working");
    expect(screen.getByTitle("Agent checkpoint status facet: filesystem checkpoint.")).toHaveTextContent(
      "filesystem checkpoint",
    );
    expect(screen.getByTitle("Agent change-log status facet: 1 change.")).toHaveTextContent(
      "1 changes",
    );
    expect(screen.queryByTestId("terminal-surface")).not.toBeInTheDocument();
  });

  it("sends composer text as an agent turn", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "implementa la vista");
    await user.click(screen.getByRole("button", { name: "Send" }));

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "implementa la vista\r");
    expect(composer).toHaveValue("");
  });

  it("prepares editable turns from quick actions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.click(screen.getByRole("button", { name: "Plan" }));

    expect(composer).toHaveValue(
      "Create a concise implementation plan for the next change before editing.",
    );

    await user.click(screen.getByRole("button", { name: "Review" }));
    expect(composer).toHaveValue(
      [
        "Create a concise implementation plan for the next change before editing.",
        "Review the current changes and call out concrete bugs, regressions, or missing tests.",
      ].join("\n\n"),
    );
  });

  it("uses Enter to send and Shift+Enter to keep composing", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([sessionFixture()]);
    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const composer = await screen.findByLabelText("Message Codex");
    await user.type(composer, "line one{Shift>}{Enter}{/Shift}line two");
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();

    await user.type(composer, "{Enter}");
    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "line one\nline two\r");
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
    expect(screen.getByTitle("Conversation message block for turn 1, You: message content and copy control."))
      .toHaveClass("agent-panel__message--user_message");
    expect(screen.getByTitle("Agent message role label: You.")).toHaveTextContent("You");
    expect(screen.getByTitle("Agent message header for You: role label and copy control."))
      .toHaveTextContent("You");
    expect(screen.getByText("Turn 1")).toBeInTheDocument();
    expect(screen.getByTitle("Conversation turn index label: Turn 1.")).toHaveTextContent(
      "Turn 1",
    );
    expect(screen.getByText("Haz el cambio")).toBeInTheDocument();
    expect(screen.getByTitle("Conversation message markdown content for turn 1, You."))
      .toHaveClass("agent-panel__markdown");
    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByTitle("Conversation message block for turn 1, Agent: message content and copy control."))
      .toHaveClass("agent-panel__message--agent_message");
    expect(screen.getByTitle("Agent message role label: Agent.")).toHaveTextContent("Agent");
    expect(screen.getByTitle("Agent message header for Agent: role label and copy control."))
      .toHaveTextContent("Agent");
    expect(screen.getByText("Voy con ello")).toBeInTheDocument();
    expect(screen.getByTitle("Conversation message markdown content for turn 1, Agent."))
      .toHaveClass("agent-panel__markdown");
    expect(screen.getByText("Command")).toBeInTheDocument();
    expect(screen.getByTitle("Conversation message block for turn 1, Command: message content and copy control."))
      .toHaveClass("agent-panel__message--command_output");
    expect(screen.getByTitle("Agent message role label: Command.")).toHaveTextContent("Command");
    expect(screen.getByTitle("Agent message header for Command: role label and copy control."))
      .toHaveTextContent("Command");
    expect(screen.getByTitle("Conversation message terminal output for turn 1, Command."))
      .toHaveClass("agent-panel__message-terminal");
    const conversation = screen.getByLabelText("Agent conversation");
    expect(conversation).toHaveAttribute(
      "title",
      "Agent conversation transcript: showing all 1 turn.",
    );
    expect(within(conversation).getByText("cargo test")).toBeInTheDocument();
    expect(
      within(conversation).getByText("3 messages / 1 commands"),
    ).toHaveAttribute(
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
    expect(within(turnsMetric).getByTitle("Agent session overview turns value: 1."))
      .toHaveTextContent("1");
    expect(within(turnsMetric).getByTitle("Agent session overview metric label: Turns."))
      .toHaveTextContent("Turns");
    const messagesMetric = within(overview).getByLabelText("Messages: 2");
    expect(messagesMetric).toHaveAttribute(
      "title",
      "Agent session overview messages metric: 2 messages.",
    );
    expect(within(messagesMetric).getByTitle("Agent session overview messages value: 2."))
      .toHaveTextContent("2");
    expect(within(messagesMetric).getByTitle("Agent session overview metric label: Messages."))
      .toHaveTextContent("Messages");
    const commandsMetric = within(overview).getByLabelText("Commands: 1");
    expect(commandsMetric).toHaveAttribute(
      "title",
      "Agent session overview commands metric: 1 command.",
    );
    expect(within(commandsMetric).getByTitle("Agent session overview commands value: 1."))
      .toHaveTextContent("1");
    expect(within(commandsMetric).getByTitle("Agent session overview metric label: Commands."))
      .toHaveTextContent("Commands");
    const filesMetric = within(overview).getByLabelText("Files: 0");
    expect(filesMetric).toHaveAttribute("title", "Agent session overview files metric: 0 files.");
    expect(within(filesMetric).getByTitle("Agent session overview files value: 0."))
      .toHaveTextContent("0");
    expect(within(filesMetric).getByTitle("Agent session overview metric label: Files."))
      .toHaveTextContent("Files");
    expect(
      within(overview).getByTitle(
        "Agent session overview latest-activity area: latest captured activity.",
      ),
    ).toHaveTextContent("cargo test");
    expect(within(overview).getByTitle("Agent session overview latest-activity label."))
      .toHaveTextContent("Latest activity");
    expect(within(overview).getByTitle("Agent session overview latest activity: cargo test."))
      .toHaveTextContent("cargo test");
    expect(within(overview).getByText("+0s")).toBeInTheDocument();
    expect(
      within(overview).getByTitle("Turn 1: 1 commands, 0 files - Recent command: cargo test"),
    ).toBeInTheDocument();
    const turnMap = screen.getByLabelText("Turn map");
    expect(turnMap).toHaveAttribute("title", "Agent session overview turn map: 1 turn.");
    const firstTurnButton = within(turnMap).getByRole("button", { name: /T1/ });
    expect(within(firstTurnButton).getByTitle("Agent session overview turn-map label: turn 1."))
      .toHaveTextContent("T1");
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
    expect(
      within(screen.getByLabelText("Agent conversation")).getByText("+0s"),
    ).toHaveAttribute("title", "Turn 1 timing relative to the first turn: +0s.");

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
    expect(toggle).toHaveAttribute("title", "Collapsed command output disclosure label: Show output.");
    expect(
      within(conversation).getByTitle("Collapsed command output summary: npm test -- --run."),
    ).toHaveTextContent("npm test -- --run");
    expect(toggle.closest("details")).not.toHaveAttribute("open");
    expect(toggle.closest("details")).toHaveAttribute(
      "title",
      "Collapsed command block for turn 1, Command: summary disclosure and terminal output.",
    );
    expect(toggle.closest("summary")).toHaveAttribute(
      "title",
      "Collapsed command summary row for turn 1, Command: command summary and disclosure label.",
    );
    expect(within(conversation).getAllByText("npm test -- --run").length).toBeGreaterThan(0);
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
    expect(screen.getByText("Press Enter to move through matching turns and Escape to clear the search."))
      .toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous result" })).toHaveAttribute(
      "title",
      "Search transcript to enable previous result navigation.",
    );
    expect(screen.getByTitle("Transcript search navigation label: previous result."))
      .toHaveTextContent("Prev");
    expect(screen.getByRole("button", { name: "Next result" })).toHaveAttribute(
      "title",
      "Search transcript to enable next result navigation.",
    );
    expect(screen.getByTitle("Transcript search navigation label: next result."))
      .toHaveTextContent("Next");

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
    const clearSearch = screen.getByRole("button", { name: "Clear search" });
    expect(clearSearch).toHaveAttribute(
      "title",
      "Clear the transcript search, restore all turns, and return focus to search.",
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
  });

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
    expect(screen.getByTitle("Transcript secondary action label: Latest."))
      .toHaveTextContent("Latest");

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
    expect(screen.getByTitle("Transcript search count: 2 matching turns out of 3 total turns."))
      .toHaveTextContent("2 of 3 turns");
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
    expect(screen.getByLabelText("No focused search result selected out of 2 matching turns.")).toHaveTextContent("- / 2");
    expect(screen.getByLabelText("No focused search result selected out of 2 matching turns.")).toHaveAttribute(
      "title",
      "Active transcript search position: no result selected out of 2 matching turns.",
    );

    await user.keyboard("{Enter}");
    expect(getElementByIdSpy).toHaveBeenLastCalledWith("agent-turn-sess-1-1");
    expect(scrollIntoViewMock).toHaveBeenLastCalledWith({ block: "center", behavior: "smooth" });
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent("1 / 2");
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
    expect(screen.getByLabelText("Focused search result 2 of 2 matching turns.")).toHaveTextContent("2 / 2");
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
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent("1 / 2");
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
    expect(screen.getByLabelText("Focused search result 1 of 2 matching turns.")).toHaveTextContent("1 / 2");

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
    expect(screen.getByTitle("Transcript search count: showing all 1 transcript turn."))
      .toHaveTextContent("All turns");
    expect(secondaryActions).toHaveAttribute(
      "title",
      "Transcript secondary actions: latest-turn jump and copy all 1 transcript turn.",
    );

    expect(within(secondaryActions).getByRole("button", { name: "Latest" })).toHaveAttribute(
      "title",
      "Jump to the latest of 1 transcript turn.",
    );
    expect(within(secondaryActions).getByTitle("Transcript secondary action label: Latest."))
      .toHaveTextContent("Latest");
    expect(within(secondaryActions).getByRole("button", { name: "Copy visible" })).toHaveAttribute(
      "title",
      "Copy all 1 transcript turn.",
    );
    expect(within(secondaryActions).getByTitle("Transcript secondary action label: Copy visible."))
      .toHaveTextContent("Copy visible");
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
    expect(screen.getByTitle("Transcript secondary action label: Copied."))
      .toHaveTextContent("Copied");
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
    expect(screen.getByRole("button", { name: "Plan" })).toBeDisabled();

    unmount();
    expect(listAgentSessionsMock).not.toHaveBeenCalled();
    expect(stopAgentSessionMock).not.toHaveBeenCalled();
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
    expect(
      screen.getByTitle(
        "Conversation turn touched-files container for turn 1: 1 touched-file chip.",
      ),
    ).toHaveClass("agent-panel__chat-turn-files");
    expect(screen.getByText("1 files touched")).toHaveAttribute(
      "title",
      "Turn 1 touched 1 file.",
    );
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
    expect(within(lens).getByTitle("Agent Lens heading label.")).toHaveTextContent("Agent Lens");
    const filesTab = within(lens).getByRole("tab", { name: /Files/ });
    expect(filesTab).toHaveAttribute("title", "Show Agent Lens Files view with 1 touched file.");
    expect(within(filesTab).getByTitle("Agent Lens tab name: Files view.")).toHaveTextContent(
      "Files",
    );
    expect(within(filesTab).getByTitle("Agent Lens Files tab count: 1 touched file."))
      .toHaveTextContent("1");
    const commandsTab = within(lens).getByRole("tab", { name: /Commands/ });
    expect(commandsTab).toHaveAttribute(
      "title",
      "Show Agent Lens Commands view with 1 command output.",
    );
    expect(within(commandsTab).getByTitle("Agent Lens tab name: Commands view."))
      .toHaveTextContent("Commands");
    expect(within(commandsTab).getByTitle("Agent Lens Commands tab count: 1 command output."))
      .toHaveTextContent("1");
    const timelineTab = within(lens).getByRole("tab", { name: /Timeline/ });
    expect(timelineTab).toHaveAttribute(
      "title",
      "Show Agent Lens Timeline view with 3 recent timeline items.",
    );
    expect(within(timelineTab).getByTitle("Agent Lens tab name: Timeline view."))
      .toHaveTextContent("Timeline");
    expect(within(timelineTab).getByTitle("Agent Lens Timeline tab count: 3 timeline items."))
      .toHaveTextContent("3");
    expect(within(lens).getByLabelText("Touched files")).toHaveAttribute(
      "title",
      "Agent Lens touched files for the focused turn: 1 file.",
    );
    const preview = within(lens).getByLabelText("Selected file preview");
    expect(preview).toHaveAttribute("title", "Selected Agent Lens file preview for src/a.ts.");
    expect(within(preview).getByTitle("Agent Lens preview is showing src/a.ts."))
      .toHaveTextContent("src/a.ts");
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

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
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
    expect(
      within(lens).getByTitle("Agent Lens has no command output in the current session."),
    ).toHaveTextContent("No commands captured yet.");

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
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
        "Agent Lens metrics summarize Working state and 1 file for the focused turn.",
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
        "Agent Lens view tabs: 1 file, 1 command output, 2 timeline items.",
      ),
    ).toHaveAttribute("role", "tablist");
    const focusedFileRow = within(lens).getByTitle(
      "Agent Lens touched file for turn 2: created src/second.ts.",
    );
    expect(focusedFileRow).toHaveTextContent("src/second.ts");
    expect(
      within(focusedFileRow).getByTitle("Agent Lens file row timing: turn 2 at +0s."),
    ).toHaveTextContent("Turn 2 - +0s");
    expect(within(focusedFileRow).getByTitle("Agent Lens file row path: src/second.ts."))
      .toHaveTextContent("src/second.ts");
    expect(
      within(focusedFileRow).getByTitle("Code Agent Lens file row change type: created."),
    ).toHaveTextContent("created");
    expect(within(lens).getAllByText("src/second.ts").length).toBeGreaterThan(0);
    expect(within(lens).queryByText("src/first.ts")).not.toBeInTheDocument();
    expect(within(lens).getByText("Focused files")).toBeInTheDocument();
    expect(within(lens).getByTitle("Agent Lens file filter controls 1 touched file."))
      .toHaveTextContent("Filter files");
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
    expect(within(firstTurnButton).getByTitle("Agent session overview turn-map label: turn 1."))
      .toHaveTextContent("T1");
    expect(
      within(firstTurnButton).getByTitle(
        "Agent session overview turn-map command summary for turn 1: npm test first.",
      ),
    ).toHaveTextContent("cmd npm test first");
    expect(
      within(firstTurnButton).getByTitle("Agent session overview turn-map file count for turn 1: 1 file."),
    ).toHaveTextContent("1 files");

    await user.click(firstTurnButton);
    expect(within(lens).getByText("Turn 1")).toBeInTheDocument();
    expect(within(lens).getByText("npm test first")).toBeInTheDocument();
    expect(within(lens).queryByText("npm test second")).not.toBeInTheDocument();

    await user.click(within(lens).getByRole("button", { name: "Session" }));
    expect(within(lens).getByRole("button", { name: "Session" })).toHaveAttribute(
      "title",
      "Scope Agent Lens to the full session.",
    );
    await user.click(within(lens).getByRole("tab", { name: /Files/ }));
    expect(within(lens).getByTitle("Agent Lens is showing the full session with 2 turns."))
      .toHaveTextContent("2 turns");
    expect(
      within(lens).getByTitle("Agent Lens inspector for the full session with 2 turns."),
    ).toHaveTextContent("Agent Lens");
    expect(within(lens).getByLabelText("Agent Lens scope")).toHaveAttribute(
      "title",
      "Agent Lens scope controls switch between the focused turn and the full session.",
    );
    expect(
      within(lens).getByTitle(
        "Agent Lens metrics summarize Working state and 2 files for the current session.",
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
        "Agent Lens view tabs: 2 files, 2 command outputs, 4 timeline items.",
      ),
    ).toHaveAttribute("role", "tablist");
    expect(within(lens).getByText("Session files")).toBeInTheDocument();
    expect(within(lens).getByTitle("Agent Lens file filter controls 2 touched files."))
      .toHaveTextContent("Filter files");
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
    const fileActions = within(lens).getByTitle(
      "Agent Lens file actions for src/agent-view.tsx: preview, open, ask, and revert controls.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "File actions for src/agent-view.tsx");
    expect(within(fileActions).getByRole("button", { name: "Preview" })).toHaveAttribute(
      "title",
      "Previewing Agent Lens details for src/agent-view.tsx.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Preview."))
      .toHaveTextContent("Preview");
    expect(within(fileActions).getByRole("button", { name: "Open" })).toHaveAttribute(
      "title",
      "Open src/agent-view.tsx from Agent Lens in the workspace.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Open."))
      .toHaveTextContent("Open");
    expect(within(fileActions).getByRole("button", { name: "Ask" })).toHaveAttribute(
      "title",
      "Draft an Agent Lens follow-up prompt for src/agent-view.tsx.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Ask."))
      .toHaveTextContent("Ask");
    expect(within(fileActions).getByRole("button", { name: "Revert" })).toHaveAttribute(
      "title",
      "Stop the session before reverting src/agent-view.tsx.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Revert."))
      .toHaveTextContent("Revert");
    await user.click(within(lens).getByRole("button", { name: "Open" }));

    expect(openFile).toHaveBeenCalledWith("/r/a", "src/agent-view.tsx", true);

    await user.click(within(lens).getByRole("button", { name: "Ask" }));

    const composerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Focus on src/agent-view.tsx.");
    expect(composerValue).toContain("It was modified in turn 1.");
    expect(composerValue).toContain("Artifact category: Code.");
    expect(composerValue).toContain("Diff summary: 1 hunk - +1 / -1.");
    expect(composerValue).toContain("next concrete edit or verification step");
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
      within(context).getByTitle(
        "Live diff summary for src/agent-view.tsx: 2 added, 1 removed.",
      ),
    ).toHaveTextContent("+2 / -1");

    const preview = within(lens).getByLabelText("Selected file preview");
    expect(preview).toHaveAttribute(
      "title",
      "Selected Agent Lens file preview for src/agent-view.tsx.",
    );
    expect(within(preview).getByTitle("Selected-file preview area for the active Agent Lens file."))
      .toHaveTextContent("Preview");
    expect(within(preview).getByTitle("Agent Lens preview is showing src/agent-view.tsx."))
      .toHaveTextContent("src/agent-view.tsx");
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
    expect(within(codeGroup).getByTitle("Agent Lens file group kind label: Code."))
      .toHaveTextContent("Code");
    const codeFileRow = within(codeGroup).getByTitle(
      "Agent Lens touched file for the session: modified src/agent-view.tsx.",
    );
    expect(codeFileRow).toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle("Agent Lens file row scope: session change log at +0s."),
    ).toHaveTextContent("Session - +0s");
    expect(within(codeFileRow).getByTitle("Agent Lens file row path: src/agent-view.tsx."))
      .toHaveTextContent("src/agent-view.tsx");
    expect(
      within(codeFileRow).getByTitle("Code Agent Lens file row change type: modified."),
    ).toHaveTextContent("modified");
    const testsGroup = within(lens).getByLabelText("Tests files");
    expect(
      within(testsGroup).getByTitle("Tests Agent Lens group heading for 1 touched file."),
    ).toHaveTextContent("Tests");
    expect(within(testsGroup).getByTitle("Agent Lens file group kind label: Tests."))
      .toHaveTextContent("Tests");
    expect(within(testsGroup).getByText("src/agent-view.test.tsx")).toBeInTheDocument();
    expect(within(within(lens).getByLabelText("Docs files")).getByText("docs/agent-view.md"))
      .toBeInTheDocument();
    expect(within(within(lens).getByLabelText("Config files")).getByText("package.json"))
      .toBeInTheDocument();
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
      within(focus).getByTitle("Focused turn hidden file overflow: 1 additional touched file for turn 2."),
    ).toHaveTextContent("+1 more");
    expect(
      within(focus).getByTitle("Focused turn action container for turn 2: prompt-drafting actions."),
    ).toHaveClass("agent-panel__turn-focus-actions");
    expect(
      within(focus).getByTitle("Focused turn utility container for turn 2: navigation and copy utilities."),
    ).toHaveClass("agent-panel__turn-focus-utilities");

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

    expect(within(focus).getByRole("button", { name: "Jump" })).toHaveAttribute(
      "title",
      "Scroll conversation turn 1 into view.",
    );
    expect(within(focus).getByTitle("Focused turn utility label: Jump.")).toHaveTextContent(
      "Jump",
    );
    const continueAction = within(focus).getByRole("button", { name: "Continue" });
    const reviewAction = within(focus).getByRole("button", { name: "Review" });
    const testAction = within(focus).getByRole("button", { name: "Test" });
    const handoffAction = within(focus).getByRole("button", { name: "Handoff" });
    expect(continueAction).toHaveAttribute("title", "Draft a continuation prompt for turn 1.");
    expect(reviewAction).toHaveAttribute("title", "Draft a focused review prompt for turn 1.");
    expect(testAction).toHaveAttribute("title", "Draft a verification prompt for turn 1.");
    expect(handoffAction).toHaveAttribute("title", "Draft a handoff prompt for turn 1.");
    expect(within(focus).getByTitle("Focused turn action label: Continue.")).toHaveTextContent(
      "Continue",
    );
    expect(within(focus).getByTitle("Focused turn action label: Review.")).toHaveTextContent(
      "Review",
    );
    expect(within(focus).getByTitle("Focused turn action label: Test.")).toHaveTextContent("Test");
    expect(within(focus).getByTitle("Focused turn action label: Handoff.")).toHaveTextContent(
      "Handoff",
    );

    await user.click(continueAction);

    const composerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Continue from turn 1.");
    expect(composerValue).toContain("Artifact summary: Code 1.");
    expect(composerValue).toContain("Touched files: modified src/first.ts.");
    expect(composerValue).not.toContain("Recent commands:");

    await user.click(testAction);

    const testComposerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement)
      .value;
    expect(testComposerValue).toContain("Test the work from turn 1.");
    expect(testComposerValue).toContain("Recent commands: npm test first.");
    expect(testComposerValue).toContain("run the most relevant verification");

    await user.click(reviewAction);

    const reviewComposerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement)
      .value;
    expect(reviewComposerValue).toContain("Review turn 1.");
    expect(reviewComposerValue).toContain("concrete bugs, regressions, or missing tests");

    await user.click(handoffAction);

    const handoffComposerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement)
      .value;
    expect(handoffComposerValue).toContain("Prepare a handoff from turn 1.");
    expect(handoffComposerValue).toContain("Artifact summary: Code 1.");
    expect(handoffComposerValue).toContain("Recent commands: npm test first.");
    expect(handoffComposerValue).toContain("changed files, verification, risks");

    const copyFocus = within(focus).getByRole("button", { name: "Copy focus" });
    expect(copyFocus).toHaveAttribute(
      "title",
      "Copy focused turn 1 context, including artifact summary and transcript.",
    );
    expect(within(focus).getByTitle("Focused turn utility label: Copy focus.")).toHaveTextContent(
      "Copy focus",
    );
    await user.click(copyFocus);

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const copied = String(
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1]?.[0] ?? "",
    );
    expect(copied).toContain("Turn 1");
    expect(copied).toContain("Artifacts: Code 1.");
    expect(copied).toContain("First task");
    expect(copied).toContain("src/first.ts");
    expect(within(focus).getByRole("button", { name: "Copied" })).toHaveAttribute(
      "title",
      "Copied focused turn 1 context to clipboard.",
    );
    expect(within(focus).getByTitle("Focused turn utility label: Copied.")).toHaveTextContent(
      "Copied",
    );
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
    expect(within(lens).getByTitle("Agent Lens preview is showing src/session-only.ts."))
      .toHaveTextContent("src/session-only.ts");
    expect(
      within(lens).getByTitle(
        "Selected-file preview placeholder for src/session-only.ts: no live hunk data available.",
      ),
    )
      .toHaveTextContent("No live hunk data available for this file.");
    expect(within(lens).getByText("Session - +0s")).toBeInTheDocument();
    const fileActions = within(lens).getByTitle(
      "Agent Lens file actions for src/session-only.ts: preview, open, and ask controls.",
    );
    expect(fileActions).toHaveAttribute("aria-label", "File actions for src/session-only.ts");
    expect(within(fileActions).getByRole("button", { name: "Preview" })).toHaveAttribute(
      "title",
      "Previewing Agent Lens details for src/session-only.ts.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Preview."))
      .toHaveTextContent("Preview");
    expect(within(fileActions).getByRole("button", { name: "Open" })).toHaveAttribute(
      "title",
      "Open src/session-only.ts from Agent Lens in the workspace.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Open."))
      .toHaveTextContent("Open");
    expect(within(fileActions).getByRole("button", { name: "Ask" })).toHaveAttribute(
      "title",
      "Draft an Agent Lens follow-up prompt for src/session-only.ts.",
    );
    expect(within(fileActions).getByTitle("Agent Lens file action label: Ask."))
      .toHaveTextContent("Ask");
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
    expect(within(lens).getByTitle("Agent Lens file filter controls 2 touched files."))
      .toHaveTextContent("Filter files");

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
        "No Agent Lens files match the current filter. Clear or change the filter to show touched files.",
      ),
    ).toHaveTextContent("No files match this filter.");
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

    await user.click(await screen.findByRole("button", { name: "Revert" }));

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
    expect(button).toHaveAttribute("title", "This session has no reversible checkpoint");
  });
});
