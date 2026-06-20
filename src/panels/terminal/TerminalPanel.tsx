import { useEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  listAgentSessions,
  onAgentSessionOutput,
  resizeAgentSession,
  revertSession,
  writeAgentSessionInput,
} from "../../bus/client";
import { agentSessionStore, useAgentSession } from "../../agent/sessionStore";
import type { AgentSession } from "../../bus/contract";

export interface TerminalPanelParams {
  sessionId: string;
  repo?: string;
  agentType?: string;
}

type TerminalPanelProps = IDockviewPanelProps<TerminalPanelParams>;

export function TerminalPanel({ params }: TerminalPanelProps) {
  const sessionId = params?.sessionId ?? "";
  const repo = params?.repo;
  const agentType = params?.agentType ?? "agent";
  const terminalRef = useRef<HTMLDivElement | null>(null);
  const session = useAgentSession(sessionId);
  const [error, setError] = useState<string | null>(null);
  const [reverting, setReverting] = useState(false);

  const shortSession = useMemo(
    () => (sessionId.length > 8 ? sessionId.slice(0, 8) : sessionId),
    [sessionId],
  );

  useEffect(() => {
    const container = terminalRef.current;
    if (!container || !sessionId) return;

    let active = true;
    let resizeTimer: number | undefined;
    let lastSize: { cols: number; rows: number } | null = null;
    const terminal = new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: "Consolas, 'Cascadia Mono', 'SFMono-Regular', monospace",
      fontSize: 13,
      theme: {
        background: "#1e1e1e",
        foreground: "#d4d4d4",
        cursor: "#4ec9b0",
        selectionBackground: "#264f78",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);

    const publishSize = () => {
      try {
        fitAddon.fit();
        if (!terminal.cols || !terminal.rows) return;
        const size = { cols: terminal.cols, rows: terminal.rows };
        if (lastSize && lastSize.cols === size.cols && lastSize.rows === size.rows) return;
        lastSize = size;
        void resizeAgentSession(sessionId, size.cols, size.rows).catch((e) => {
          if (active) setError(commandMessage(e));
        });
      } catch (e) {
        if (active) setError(commandMessage(e));
      }
    };

    const scheduleFit = () => {
      window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(publishSize, 80);
    };

    const dataSubscription = terminal.onData((data) => {
      void writeAgentSessionInput(sessionId, data).catch((e) => {
        if (active) setError(commandMessage(e));
      });
    });
    const outputSubscription = onAgentSessionOutput((output) => {
      if (!active || output.session_id !== sessionId) return;
      terminal.write(decodeBase64(output.chunk_base64));
    });

    publishSize();
    const frame = window.requestAnimationFrame(scheduleFit);
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => {
            scheduleFit();
          });
    observer?.observe(container);

    return () => {
      active = false;
      window.cancelAnimationFrame(frame);
      window.clearTimeout(resizeTimer);
      observer?.disconnect();
      dataSubscription.dispose();
      void outputSubscription.then((unlisten) => unlisten());
      fitAddon.dispose();
      terminal.dispose();
    };
  }, [sessionId]);

  useEffect(() => {
    let active = true;
    void listAgentSessions()
      .then((sessions) => {
        if (active) agentSessionStore.setSessions(sessions);
      })
      .catch((e) => {
        if (active) setError(commandMessage(e));
      });
    return () => {
      active = false;
    };
  }, []);

  const canRevert =
    !!session &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const onRevert = async () => {
    if (!sessionId || !canRevert || reverting) return;
    const ok = await confirm("This will undo all changes made by this session. Continue?", {
      title: "Revert agent session",
      kind: "warning",
      okLabel: "Revert",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setReverting(true);
    setError(null);
    try {
      const updated = await revertSession(sessionId, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setReverting(false);
    }
  };

  return (
    <div className="terminal-panel" data-testid={`terminal-panel-${sessionId}`}>
      <header className="terminal-panel__head">
        <span className="terminal-panel__title">Terminal</span>
        <span
          className="terminal-panel__meta"
          title={[agentType, repo, sessionId].filter(Boolean).join(" · ")}
        >
          {agentType} · {shortSession}
        </span>
        {session && (
          <span className="terminal-panel__audit" title={auditTitle(session)}>
            {session.status}
            {session.checkpoint ? ` · ${checkpointLabel(session.checkpoint.checkpoint_type)}` : ""}
            {(session.change_log?.length ?? 0) > 0
              ? ` · ${session.change_log?.length} changes`
              : ""}
          </span>
        )}
        <button
          className="terminal-panel__revert"
          disabled={!canRevert || reverting}
          onClick={onRevert}
          title={
            session?.status === "reverted"
              ? "Session already reverted"
              : canRevert
                ? "Revert this agent session"
                : "Stop the session before reverting"
          }
        >
          {reverting ? "Reverting" : "Revert"}
        </button>
        {error && (
          <span className="terminal-panel__error" data-testid="terminal-panel-error">
            {error}
          </span>
        )}
      </header>
      <div className="terminal-panel__surface" ref={terminalRef} data-testid="terminal-surface" />
    </div>
  );
}

function checkpointLabel(type: string): string {
  return type === "git_ref" ? "git checkpoint" : "filesystem checkpoint";
}

function auditTitle(session: AgentSession): string {
  const pieces = [
    `Status: ${session.status}`,
    session.checkpoint
      ? `Checkpoint: ${checkpointLabel(session.checkpoint.checkpoint_type)}`
      : null,
    session.checkpoint?.git_hash ? `Git: ${session.checkpoint.git_hash.slice(0, 12)}` : null,
    `Changes: ${session.change_log?.length ?? 0}`,
  ];
  return pieces.filter(Boolean).join(" · ");
}

function decodeBase64(chunk: string): Uint8Array {
  const binary = atob(chunk);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Terminal command failed.");
  }
  return String(error || "Terminal command failed.");
}
