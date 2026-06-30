import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSession } from "../../bus/contract";

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
import { markTerminalDetached } from "./detachTerminalWindow";

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

function installClipboardMock() {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: writeClipboardTextMock },
  });
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoViewMock,
    });
    installClipboardMock();
    agentSessionStore.reset();
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

    expect(await screen.findByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("a")).toBeInTheDocument();
    expect(screen.getByLabelText("Agent conversation")).toBeInTheDocument();
    const activity = screen.getByLabelText("Agent activity");
    expect(within(activity).getByText("Agent is working")).toBeInTheDocument();
    expect(within(activity).getByText("filesystem checkpoint")).toBeInTheDocument();
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
    expect(screen.getByText("Turn 1")).toBeInTheDocument();
    expect(screen.getByText("Haz el cambio")).toBeInTheDocument();
    expect(screen.getByText("Agent")).toBeInTheDocument();
    expect(screen.getByText("Voy con ello")).toBeInTheDocument();
    expect(screen.getByText("Command")).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("Agent conversation")).getByText("cargo test"),
    ).toBeInTheDocument();

    const overview = screen.getByLabelText("Agent session overview");
    expect(within(overview).getByLabelText("Turns: 1")).toBeInTheDocument();
    expect(within(overview).getByLabelText("Messages: 2")).toBeInTheDocument();
    expect(within(overview).getByLabelText("Commands: 1")).toBeInTheDocument();
    expect(within(overview).getByLabelText("Files: 0")).toBeInTheDocument();
    expect(within(overview).getByText("Latest activity")).toBeInTheDocument();
    expect(within(overview).getByText("cargo test")).toBeInTheDocument();
    expect(within(overview).getByText("+0s")).toBeInTheDocument();
    expect(within(overview).getByTitle("Turn 1: 1 commands, 0 files")).toBeInTheDocument();
    expect(
      within(screen.getByLabelText("Agent conversation")).getByText("+0s"),
    ).toBeInTheDocument();

    await user.click(within(overview).getByRole("button", { name: /T1/ }));
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

    await user.click(await screen.findByLabelText("Copy Agent message"));

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalledWith("Voy con ello"));
    expect(await screen.findByLabelText("Copy Agent message")).toHaveTextContent("Copied");
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

    await user.click(await screen.findByRole("button", { name: "Copy turn" }));

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Turn 1 (+0s)");
    expect(copied).toContain("You:\nHaz el cambio");
    expect(copied).toContain("Agent:\nVoy con ello");
    expect(copied).toContain("Command:\nnpm test");
    expect(copied).toContain("- modified src/a.ts");
    expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
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
    expect(toggle.closest("details")).not.toHaveAttribute("open");
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
        text: "npm test search-panel",
        timestamp_ms: 4,
      });
    });

    const search = await screen.findByLabelText("Search transcript");
    const conversation = screen.getByLabelText("Agent conversation");

    await user.type(search, "dashboard");
    expect(within(conversation).getByText("Termine el dashboard")).toBeInTheDocument();
    expect(within(conversation).queryByText("Segunda tarea")).not.toBeInTheDocument();
    expect(screen.getByText("1 of 2 turns")).toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "src/search-panel.tsx");
    expect(within(conversation).getByText("Segunda tarea")).toBeInTheDocument();
    expect(within(conversation).queryByText("Termine el dashboard")).not.toBeInTheDocument();

    await user.clear(search);
    await user.type(search, "nope");
    expect(screen.getByText("No matches")).toBeInTheDocument();
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

    await user.click(await screen.findByRole("button", { name: "Latest" }));

    expect(getElementByIdSpy).toHaveBeenCalledWith("agent-turn-sess-1-2");
    expect(scrollIntoViewMock).toHaveBeenCalledWith({ block: "end", behavior: "smooth" });
    getElementByIdSpy.mockRestore();
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
    await user.click(screen.getByRole("button", { name: "Copy visible" }));

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const lastCall =
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1];
    const copied = String(lastCall?.[0] ?? "");
    expect(copied).toContain("Dashboard task");
    expect(copied).toContain("Finished dashboard work");
    expect(copied).not.toContain("Search task");
    expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
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
    expect(screen.getAllByText("modified src/a.ts").length).toBeGreaterThan(0);
    expect(screen.getByText("1 files touched")).toBeInTheDocument();
    expect(screen.getByLabelText("Files: 1")).toBeInTheDocument();
    expect(screen.getByTitle("Turn 1: 0 commands, 1 files")).toBeInTheDocument();
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
    expect(within(lens).getByLabelText("Touched files")).toBeInTheDocument();
    expect(within(lens).getByText("src/a.ts")).toBeInTheDocument();

    await user.click(within(lens).getByRole("tab", { name: /Commands/ }));
    expect(within(lens).getByLabelText("Command output")).toBeInTheDocument();
    expect(within(lens).getByText("npm test")).toBeInTheDocument();

    await user.click(within(lens).getByRole("tab", { name: /Timeline/ }));
    expect(within(lens).getByLabelText("Recent timeline")).toBeInTheDocument();
    expect(within(lens).getByText(/Turn 1 - Command - \+0s/)).toBeInTheDocument();
    expect(within(lens).getByText("Voy con ello")).toBeInTheDocument();
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
        id: "sess-1:agent:2",
        kind: "agent_message",
        text: "First done",
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
        id: "sess-1:agent:4",
        kind: "agent_message",
        text: "Second done",
        timestamp_ms: 4,
      });
    });

    const focus = await screen.findByLabelText("Focused turn");
    expect(within(focus).getByText("Turn 2")).toBeInTheDocument();
    expect(within(focus).getByText("Second done")).toBeInTheDocument();

    await user.click(within(screen.getByLabelText("Turn map")).getByRole("button", { name: /T1/ }));

    expect(within(focus).getByText("Turn 1")).toBeInTheDocument();
    expect(within(focus).getByText("First done")).toBeInTheDocument();
    expect(within(focus).queryByText("Second done")).not.toBeInTheDocument();
    expect(scrollIntoViewMock).toHaveBeenCalled();
    expect(screen.getByText("First task").closest("article")).toHaveAttribute(
      "aria-current",
      "true",
    );

    await user.click(within(focus).getByRole("button", { name: "Continue" }));

    const composerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement).value;
    expect(composerValue).toContain("Continue from turn 1.");
    expect(composerValue).toContain("Touched files: modified src/first.ts.");

    await user.click(within(focus).getByRole("button", { name: "Review" }));

    const reviewComposerValue = (screen.getByLabelText("Message Codex") as HTMLTextAreaElement)
      .value;
    expect(reviewComposerValue).toContain("Review turn 1.");
    expect(reviewComposerValue).toContain("concrete bugs, regressions, or missing tests");

    await user.click(within(focus).getByRole("button", { name: "Copy focus" }));

    await waitFor(() => expect(writeClipboardTextMock).toHaveBeenCalled());
    const copied = String(
      writeClipboardTextMock.mock.calls[writeClipboardTextMock.mock.calls.length - 1]?.[0] ?? "",
    );
    expect(copied).toContain("Turn 1");
    expect(copied).toContain("First task");
    expect(copied).toContain("src/first.ts");
  });

  it("shows session change log files in Agent Lens when turn checkpoints are unavailable", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      sessionFixture({
        change_log: [{ path: "src/session-only.ts", kind: "modified", timestamp_ms: 10 }],
        turn_checkpoints: [],
      }),
    ]);

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    expect(within(lens).getByLabelText("Touched files")).toBeInTheDocument();
    expect(within(lens).getByText("src/session-only.ts")).toBeInTheDocument();
    expect(within(lens).getByText("Session - +0s")).toBeInTheDocument();
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

    render(<TerminalPanel {...props({ sessionId: "sess-1", repo: "/r/a", agentType: "codex" })} />);

    const lens = await screen.findByLabelText("Agent Lens");
    await user.type(within(lens).getByLabelText("Filter touched files"), "beta");

    expect(within(lens).getByText("src/beta.ts")).toBeInTheDocument();
    expect(within(lens).queryByText("src/alpha.ts")).not.toBeInTheDocument();
    expect(within(lens).getByText("1 of 2 files")).toBeInTheDocument();

    await user.clear(within(lens).getByLabelText("Filter touched files"));
    await user.type(within(lens).getByLabelText("Filter touched files"), "removed");

    expect(within(lens).getByText("No files match this filter.")).toBeInTheDocument();
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

    await user.click(await screen.findByTitle("Revert src/a.ts from turn 1"));

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
