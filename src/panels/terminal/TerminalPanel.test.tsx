import { act, render, screen } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSessionOutput } from "../../bus/contract";

const writeAgentSessionInputMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const resizeAgentSessionMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve();
});
const unlistenMock = vi.fn();
let outputHandler: ((output: AgentSessionOutput) => void) | null = null;

vi.mock("../../bus/client", () => ({
  writeAgentSessionInput: (...a: unknown[]) => writeAgentSessionInputMock(...a),
  resizeAgentSession: (...a: unknown[]) => resizeAgentSessionMock(...a),
  onAgentSessionOutput: (cb: (output: AgentSessionOutput) => void) => {
    outputHandler = cb;
    return Promise.resolve(unlistenMock);
  },
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

function props(params: TerminalPanelParams) {
  return { params } as IDockviewPanelProps<TerminalPanelParams>;
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    xtermMocks.terminalInstances.length = 0;
    xtermMocks.fitInstances.length = 0;
    outputHandler = null;
    writeAgentSessionInputMock.mockClear();
    resizeAgentSessionMock.mockClear();
    unlistenMock.mockClear();
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
      outputHandler?.({
        session_id: "other",
        chunk_base64: "b3RoZXI=",
        timestamp_ms: 1,
      });
      outputHandler?.({
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

  it("cleans up listener and xterm resources on unmount", async () => {
    const { unmount } = render(<TerminalPanel {...props({ sessionId: "sess-1" })} />);

    unmount();
    await act(async () => {});

    expect(unlistenMock).toHaveBeenCalledOnce();
    expect(xtermMocks.terminalInstances[0].disposed).toBe(true);
    expect(xtermMocks.fitInstances[0].disposed).toBe(true);
  });
});
