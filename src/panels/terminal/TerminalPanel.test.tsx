import { act, render, screen } from "@testing-library/react";
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
const confirmMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(true);
});

vi.mock("../../bus/client", () => ({
  listAgentSessions: () => listAgentSessionsMock(),
  revertSession: (...a: unknown[]) => revertSessionMock(...a),
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
    disposed = false;
    container: HTMLElement | null = null;

    constructor() {
      xtermMocks.terminalInstances.push(this);
    }

    loadAddon(addon: { activate?: (terminal: FakeTerminal) => void }) {
      addon.activate?.(this);
    }

    open(container: HTMLElement) {
      this.container = container;
    }

    onData(handler: (data: string) => void) {
      this.dataHandler = handler;
      return { dispose: vi.fn() };
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

function props(params: TerminalPanelParams) {
  return { params } as IDockviewPanelProps<TerminalPanelParams>;
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
    confirmMock.mockClear();
  });

  it("opens an xterm surface and publishes the initial fitted size", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1", agentType: "codex" })} />);

    expect(await screen.findByTestId("terminal-surface")).toBeInTheDocument();
    expect(xtermMocks.terminalInstances[0].container).toBe(screen.getByTestId("terminal-surface"));
    expect(xtermMocks.fitInstances[0].fit).toHaveBeenCalled();
    expect(resizeAgentSessionMock).toHaveBeenCalledWith("sess-1", 120, 36);
  });

  it("forwards terminal input bytes to the backend wrapper", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1" })} />);

    await act(async () => {
      xtermMocks.terminalInstances[0].dataHandler?.("hello\r");
    });

    expect(writeAgentSessionInputMock).toHaveBeenCalledWith("sess-1", "hello\r");
  });

  it("writes matching session output and ignores other sessions", async () => {
    render(<TerminalPanel {...props({ sessionId: "sess-1" })} />);

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

    render(<TerminalPanel {...props({ sessionId: "sess-1" })} />);

    expect(xtermMocks.terminalInstances[0].writes).toHaveLength(1);
    expect(Array.from(xtermMocks.terminalInstances[0].writes[0] as Uint8Array)).toEqual([
      67, 111, 100, 101, 120, 32, 114, 101, 97, 100, 121, 13,
    ]);
  });

  it("cleans up xterm resources on unmount", async () => {
    const { unmount } = render(<TerminalPanel {...props({ sessionId: "sess-1" })} />);

    unmount();
    await act(async () => {});

    expect(xtermMocks.terminalInstances[0].disposed).toBe(true);
    expect(xtermMocks.fitInstances[0].disposed).toBe(true);
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

    render(<TerminalPanel {...props({ sessionId: "sess-1", agentType: "codex" })} />);

    const button = await screen.findByRole("button", { name: "Revert" });
    await user.click(button);

    expect(confirmMock).toHaveBeenCalled();
    expect(revertSessionMock).toHaveBeenCalledWith("sess-1", true);
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

    render(<TerminalPanel {...props({ sessionId: "sess-1", agentType: "codex" })} />);

    const button = await screen.findByRole("button", { name: "Revert" });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "This session has no reversible checkpoint");
  });
});
