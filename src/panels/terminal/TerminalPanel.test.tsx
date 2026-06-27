import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { IDockviewPanelProps } from "dockview-react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSession } from "../../bus/contract";

const writeAgentSessionInputMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const resizeAgentSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const listAgentSessionsMock = vi.fn<() => Promise<AgentSession[]>>(() => Promise.resolve([]));
const revertSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve({
    id: "sess-1",
    repo: "/r/a",
    agent_type: "codex",
    status: "reverted",
    pid: null,
    started_at_ms: 1,
    ended_at_ms: 2,
    exit_code: 0,
    error: null,
    checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
    change_log: [],
    reverted_at_ms: 3,
    active_sessions: 0,
    age_ms: 2,
    output_bytes_per_second: null,
  });
});
const revertSessionTurnFileMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve({
    id: "sess-1",
    repo: "/r/a",
    agent_type: "codex",
    status: "completed",
    pid: null,
    started_at_ms: 1,
    ended_at_ms: 2,
    exit_code: 0,
    error: null,
    checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
    change_log: [],
    turn_status: "waiting",
    turn_checkpoints: [],
    reverted_at_ms: null,
    active_sessions: 0,
    age_ms: 2,
    output_bytes_per_second: null,
  });
});
const stopAgentSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const clipboardReadTextMock = vi.fn<() => Promise<string>>(() => Promise.resolve(""));
const confirmMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(true);
});

vi.mock("../../bus/client", () => ({
  listAgentSessions: () => listAgentSessionsMock(),
  revertSession: (...a: unknown[]) => revertSessionMock(...a),
  revertSessionTurnFile: (...a: unknown[]) => revertSessionTurnFileMock(...a),
  stopAgentSession: (...a: unknown[]) => stopAgentSessionMock(...a),
  writeAgentSessionInput: (...a: unknown[]) => writeAgentSessionInputMock(...a),
  resizeAgentSession: (...a: unknown[]) => resizeAgentSessionMock(...a),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: (...a: unknown[]) => confirmMock(...a),
}));

const xtermMocks = vi.hoisted(() => {
  class FakeTerminal {
    cols = 120;
    rows = 36;
    writes: Array<string | Uint8Array> = [];
    dataHandler: ((data: string) => void) | null = null;
    cursorMoveHandler: (() => void) | null = null;
    customKeyEventHandler: ((event: KeyboardEvent) => boolean) | null = null;
    disposed = false;
    container: HTMLElement | null = null;
    textarea: HTMLTextAreaElement | undefined;
    focus = vi.fn();
    paste = vi.fn((data: string) => {
      this.dataHandler?.(data);
    });
    input = vi.fn((data: string) => {
      this.dataHandler?.(data);
    });

    constructor() {
      xtermMocks.terminalInstances.push(this);
    }

    loadAddon(addon: { activate?: (terminal: FakeTerminal) => void }) {
      addon.activate?.(this);
    }

    open(container: HTMLElement) {
      this.container = container;
      this.textarea = document.createElement("textarea");
      container.appendChild(this.textarea);
    }

    onData(handler: (data: string) => void) {
      this.dataHandler = handler;
      return { dispose: vi.fn() };
    }

    onCursorMove(handler: () => void) {
      this.cursorMoveHandler = handler;
      return { dispose: vi.fn() };
    }

    attachCustomKeyEventHandler(handler: (event: KeyboardEvent) => boolean) {
      this.customKeyEventHandler = handler;
    }

    write(data: string | Uint8Array) {
      this.writes.push(data);
    }

    dispose() {
      this.disposed = true;
    }
  }

  class FakeFitAddon {
    terminal: FakeTerminal | null = null;
    fit = vi.fn();
    disposed = false;

    constructor() {
      xtermMocks.fitInstances.push(this);
    }

    activate(terminal: FakeTerminal) {
      this.terminal = terminal;
    }

    dispose() {
      this.disposed = true;
    }
  }

  return {
    FakeTerminal,
    FakeFitAddon,
    terminalInstances: [] as FakeTerminal[],
    fitInstances: [] as FakeFitAddon[],
  };
});

vi.mock("@xterm/xterm", () => ({
  Terminal: xtermMocks.FakeTerminal,
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: xtermMocks.FakeFitAddon,
}));

import { TerminalPanel, type TerminalPanelParams } from "./TerminalPanel";
import { agentSessionStore } from "../../agent/sessionStore";

interface PanelApiMock {
  id: string;
  isActive: boolean;
  setActive: ReturnType<typeof vi.fn>;
  onDidActiveChange: (cb: (event: { isActive: boolean }) => void) => {
    dispose: () => void;
  };
  __setActive: (isActive: boolean) => void;
}

function createPanelApi(id: string): PanelApiMock {
  const listeners = new Set<(event: { isActive: boolean }) => void>();
  let active = true;
  return {
    id,
    get isActive() {
      return active;
    },
    setActive: vi.fn(() => {
      active = true;
      listeners.forEach((listener) => listener({ isActive: true }));
    }),
    onDidActiveChange: (cb) => {
      listeners.add(cb);
      return { dispose: () => listeners.delete(cb) };
    },
    __setActive: (isActive) => {
      active = isActive;
      listeners.forEach((listener) => listener({ isActive }));
    },
  };
}

function props(params: TerminalPanelParams) {
  const panelApi = createPanelApi(`agent-terminal:${params.sessionId}`);
  return {
    panelApi,
    props: {
      params,
      api: panelApi,
    } as unknown as IDockviewPanelProps<TerminalPanelParams>,
  };
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    xtermMocks.terminalInstances.length = 0;
    xtermMocks.fitInstances.length = 0;
    agentSessionStore.reset();
    writeAgentSessionInputMock.mockClear();
    resizeAgentSessionMock.mockClear();
    listAgentSessionsMock.mockClear();
    revertSessionMock.mockClear();
    revertSessionTurnFileMock.mockClear();
    stopAgentSessionMock.mockClear();
    confirmMock.mockClear();
    clipboardReadTextMock.mockClear();
    Object.defineProperty(window.navigator, "clipboard", {
      configurable: true,
      value: { readText: clipboardReadTextMock },
    });
  });

  it("opens an xterm surface and publishes the initial fitted size", async () => {
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    expect(await screen.findByTestId("terminal-surface")).toBeInTheDocument();
    expect(xtermMocks.terminalInstances[0].container).toBe(screen.getByTestId("terminal-surface"));
    expect(xtermMocks.terminalInstances[0].focus).toHaveBeenCalled();
    expect(xtermMocks.fitInstances[0].fit).toHaveBeenCalled();
    expect(resizeAgentSessionMock).toHaveBeenCalledWith("sess-1", 120, 36);
  });

  it("stabilizes the xterm textarea for embedded-webview input", async () => {
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const textarea = xtermMocks.terminalInstances[0].textarea;
    expect(textarea?.style.minWidth).toBe("1px");
    expect(textarea?.style.minHeight).toBe("1px");
    expect(textarea?.style.zIndex).toBe("1");
    expect(textarea?.style.pointerEvents).toBe("none");
  });

  it("refocuses xterm when the terminal surface is selected", async () => {
    const user = userEvent.setup();
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    terminal.focus.mockClear();
    await user.click(await screen.findByTestId("terminal-surface"));

    await waitFor(() => expect(terminal.focus).toHaveBeenCalled());
  });

  it("pastes clipboard text from a paste event on the terminal surface", async () => {
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    const event = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", {
      configurable: true,
      value: { getData: () => "hola pegado" },
    });
    terminal.textarea?.dispatchEvent(event);

    expect(terminal.paste).toHaveBeenCalledWith("hola pegado");
  });

  it("reads the clipboard on Ctrl+V while the terminal tab is active", async () => {
    clipboardReadTextMock.mockResolvedValueOnce("desde clipboard");
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    fireEvent.keyDown(window, { key: "v", ctrlKey: true });

    await waitFor(() => expect(clipboardReadTextMock).toHaveBeenCalled());
    await waitFor(() => expect(terminal.paste).toHaveBeenCalledWith("desde clipboard"));
  });

  it("recovers terminal focus when the textarea blurs into no focused control", async () => {
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    terminal.focus.mockClear();
    screen.getByTestId("terminal-surface").focus();
    terminal.textarea?.dispatchEvent(new FocusEvent("blur"));

    await waitFor(() => expect(terminal.focus).toHaveBeenCalled());
  });

  it("does not reclaim focus when another control legitimately takes it", async () => {
    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    terminal.focus.mockClear();
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    terminal.textarea?.dispatchEvent(new FocusEvent("blur", { relatedTarget: button }));

    await act(async () => {});
    expect(terminal.focus).not.toHaveBeenCalled();
    button.remove();
  });

  it("forwards terminal input bytes to the backend wrapper", async () => {
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    await act(async () => {
      xtermMocks.terminalInstances[0].dataHandler?.("hello\r");
    });

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "hello\r");
  });

  it("forwards typed keys while the terminal tab is active even without textarea focus", async () => {
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    fireEvent.keyDown(window, { key: "h" });
    fireEvent.keyDown(window, { key: "Enter" });

    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "h"));
    await waitFor(() => expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "\r"));
  });

  it("lets xterm handle ordinary textarea key events", async () => {
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    const handled = terminal.customKeyEventHandler?.(new KeyboardEvent("keydown", { key: "h" }));

    expect(handled).toBe(true);
    expect(terminal.input).not.toHaveBeenCalled();
  });

  it("intercepts paste shortcuts on the textarea key handler", async () => {
    clipboardReadTextMock.mockResolvedValueOnce("desde shortcut");
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    const terminal = xtermMocks.terminalInstances[0];
    const handled = terminal.customKeyEventHandler?.(
      new KeyboardEvent("keydown", { key: "v", ctrlKey: true }),
    );

    expect(handled).toBe(false);
    await waitFor(() => expect(clipboardReadTextMock).toHaveBeenCalled());
    await waitFor(() => expect(terminal.paste).toHaveBeenCalledWith("desde shortcut"));
  });

  it("does not steal keys when the terminal tab is inactive", async () => {
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    panel.panelApi.__setActive(false);
    fireEvent.keyDown(window, { key: "h" });

    await act(async () => {});
    expect(writeAgentSessionInputMock).not.toHaveBeenCalled();
  });

  it("writes matching session output and ignores other sessions", async () => {
    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    await act(async () => {
      agentSessionStore.appendOutput({
        session_id: "other",
        chunk_base64: "b3RoZXI=",
        timestamp_ms: 1,
      });
      agentSessionStore.appendOutput({
        session_id: "sess-1",
        chunk_base64: "aGkN",
        timestamp_ms: 2,
      });
    });

    expect(xtermMocks.terminalInstances[0].writes).toHaveLength(1);
    expect(Array.from(xtermMocks.terminalInstances[0].writes[0] as Uint8Array)).toEqual([
      104, 105, 13,
    ]);
  });

  it("replays output buffered before the terminal panel opens", async () => {
    agentSessionStore.appendOutput({
      session_id: "sess-1",
      chunk_base64: "Q29kZXggcmVhZHkN",
      timestamp_ms: 1,
    });

    const panel = props({ sessionId: "sess-1" });
    render(<TerminalPanel {...panel.props} />);

    expect(xtermMocks.terminalInstances[0].writes).toHaveLength(1);
    expect(Array.from(xtermMocks.terminalInstances[0].writes[0] as Uint8Array)).toEqual([
      67, 111, 100, 101, 120, 32, 114, 101, 97, 100, 121, 13,
    ]);
  });

  it("cleans up xterm resources on unmount", async () => {
    const panel = props({ sessionId: "sess-1" });
    const { unmount } = render(<TerminalPanel {...panel.props} />);

    unmount();
    await act(async () => {});

    expect(xtermMocks.terminalInstances[0].disposed).toBe(true);
    expect(xtermMocks.fitInstances[0].disposed).toBe(true);
  });

  it("stops the backend session when the terminal panel closes", async () => {
    const panel = props({ sessionId: "sess-1" });
    const { unmount } = render(<TerminalPanel {...panel.props} />);

    unmount();
    await act(async () => {
      await new Promise((resolve) => window.setTimeout(resolve, 300));
    });

    expect(stopAgentSessionMock).toHaveBeenCalledWith("sess-1");
  });

  it("confirms and reverts completed sessions", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      {
        id: "sess-1",
        repo: "/r/a",
        agent_type: "codex",
        status: "completed",
        pid: null,
        started_at_ms: 1,
        ended_at_ms: 2,
        exit_code: 0,
        error: null,
        checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
        change_log: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
        reverted_at_ms: null,
        active_sessions: 1,
        age_ms: 1,
        output_bytes_per_second: null,
      },
    ]);

    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const button = await screen.findByRole("button", { name: "Revert" });
    await user.click(button);

    expect(confirmMock).toHaveBeenCalled();
    expect(revertSessionMock).toHaveBeenCalledWith("sess-1", true);
  });

  it("reverts a single file from a turn checkpoint", async () => {
    const user = userEvent.setup();
    listAgentSessionsMock.mockResolvedValueOnce([
      {
        id: "sess-1",
        repo: "/r/a",
        agent_type: "codex",
        status: "completed",
        pid: null,
        started_at_ms: 1,
        ended_at_ms: 2,
        exit_code: 0,
        error: null,
        checkpoint: { checkpoint_type: "fs_snapshot", git_hash: null, snapshot_files: [] },
        change_log: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 2 }],
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
        reverted_at_ms: null,
        active_sessions: 1,
        age_ms: 1,
        output_bytes_per_second: null,
      },
    ]);

    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const fileButton = await screen.findByTitle("Revert src/a.ts from turn 1");
    await user.click(fileButton);

    expect(confirmMock).toHaveBeenCalled();
    expect(revertSessionTurnFileMock).toHaveBeenCalledWith(
      "sess-1",
      "sess-1:turn-1",
      "src/a.ts",
      true,
    );
    expect(revertSessionMock).not.toHaveBeenCalled();
  });

  it("disables revert when a completed session has no checkpoint", async () => {
    listAgentSessionsMock.mockResolvedValueOnce([
      {
        id: "sess-1",
        repo: "/home/me/repo",
        agent_type: "codex",
        status: "completed",
        pid: null,
        started_at_ms: 1,
        ended_at_ms: 2,
        exit_code: 0,
        error: null,
        checkpoint: null,
        change_log: [],
        reverted_at_ms: null,
        active_sessions: 0,
        age_ms: 1,
        output_bytes_per_second: null,
      },
    ]);

    const panel = props({ sessionId: "sess-1", agentType: "codex" });
    render(<TerminalPanel {...panel.props} />);

    const button = await screen.findByRole("button", { name: "Revert" });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "This session has no reversible checkpoint");
  });
});
