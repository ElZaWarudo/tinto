import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  listAgentSessions,
  resizeAgentSession,
  revertSession,
  revertSessionTurnFile,
  stopAgentSession,
  writeAgentSessionInput,
} from "../../bus/client";
import { agentSessionStore, useAgentSession, useAgentSessionState } from "../../agent/sessionStore";
import type { AgentSession } from "../../bus/contract";
import { consumeTerminalDetachedMarker } from "./detachTerminalWindow";

export interface TerminalPanelParams {
  sessionId: string;
  repo?: string;
  agentType?: string;
}

interface FocusTerminalOptions {
  activatePanel?: boolean;
}

type TerminalPanelProps = IDockviewPanelProps<TerminalPanelParams>;
const panelCloseStopTimers = new Map<string, number>();
const PANEL_CLOSE_STOP_DELAY_MS = 250;
const DETACHED_TRANSFER_STOP_DELAY_MS = 5000;
const TERMINAL_FOCUS_DELAY_MS = 0;
const SESSION_REFRESH_INTERVAL_MS = 1500;

export function TerminalPanel({ params, api }: TerminalPanelProps) {
  const sessionId = params?.sessionId ?? "";
  const repo = params?.repo;
  const agentType = params?.agentType ?? "agent";
  const terminalRef = useRef<HTMLDivElement | null>(null);
  const session = useAgentSession(sessionId);
  const { output, outputTotal } = useAgentSessionState();
  const sessionOutput = useMemo(() => output[sessionId] ?? [], [output, sessionId]);
  const sessionOutputTotal = outputTotal[sessionId] ?? 0;
  const [error, setError] = useState<string | null>(null);
  const [reverting, setReverting] = useState(false);
  const [revertingFile, setRevertingFile] = useState<string | null>(null);
  const terminalInstanceRef = useRef<Terminal | null>(null);
  const writtenOutputRef = useRef(0);
  const panelActiveRef = useRef(false);

  const shortSession = useMemo(
    () => (sessionId.length > 8 ? sessionId.slice(0, 8) : sessionId),
    [sessionId],
  );

  const focusTerminal = useCallback(
    (options: FocusTerminalOptions = {}) => {
      const terminal = terminalInstanceRef.current;
      const container = terminalRef.current;
      if (!terminal) return;
      const textarea = terminal.textarea;
      if (textarea) stabilizeTerminalTextarea(textarea);
      if (textarea && document.activeElement === textarea && api.isActive) return;
      if (!api.isActive) {
        if (!options.activatePanel) return;
        api.setActive();
      }
      container?.focus({ preventScroll: true });
      terminal.focus();
      textarea?.focus({ preventScroll: true });
      if (textarea) {
        window.requestAnimationFrame(() => stabilizeTerminalTextarea(textarea));
      }
    },
    [api],
  );

  useEffect(() => {
    const container = terminalRef.current;
    if (!container || !sessionId) return;
    cancelPanelCloseStop(sessionId);

    let active = true;
    let resizeTimer: number | undefined;
    const settleTimers: number[] = [];
    const settleFrames: number[] = [];
    let lastSize: { cols: number; rows: number } | null = null;
    panelActiveRef.current = api.isActive;
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
    terminalInstanceRef.current = terminal;
    const textarea = terminal.textarea;
    if (textarea) {
      textarea.spellcheck = false;
      textarea.setAttribute("autocapitalize", "off");
      textarea.setAttribute("autocomplete", "off");
      textarea.setAttribute("autocorrect", "off");
      stabilizeTerminalTextarea(textarea);
    }
    const focusLater = (activatePanel = false) => {
      window.setTimeout(() => focusTerminal({ activatePanel }), TERMINAL_FOCUS_DELAY_MS);
      window.requestAnimationFrame(() => focusTerminal({ activatePanel }));
    };
    const recoverTerminalFocus = () => {
      window.requestAnimationFrame(() => {
        if (!active || !panelActiveRef.current) return;
        const focused = document.activeElement;
        if (textarea && focused === textarea) {
          stabilizeTerminalTextarea(textarea);
          return;
        }
        if (shouldPreserveFocusedElement(focused, container)) return;
        focusTerminal();
      });
    };
    focusTerminal();
    const focusFrame = window.requestAnimationFrame(() => focusTerminal());
    writtenOutputRef.current = 0;

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

    const clearSettledFitTimers = () => {
      while (settleFrames.length > 0) {
        const frame = settleFrames.pop();
        if (frame !== undefined) window.cancelAnimationFrame(frame);
      }
      while (settleTimers.length > 0) {
        const timer = settleTimers.pop();
        if (timer !== undefined) window.clearTimeout(timer);
      }
    };
    const scheduleFit = () => {
      window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(publishSize, 80);
    };
    const scheduleSettledFit = () => {
      clearSettledFitTimers();
      publishSize();
      scheduleFit();
      settleFrames.push(window.requestAnimationFrame(scheduleFit));
      settleTimers.push(window.setTimeout(scheduleFit, 160));
      settleTimers.push(window.setTimeout(scheduleFit, 360));
    };

    const activeSubscription = api.onDidActiveChange(({ isActive }) => {
      panelActiveRef.current = isActive;
      if (!isActive) return;
      scheduleSettledFit();
      window.requestAnimationFrame(() => focusTerminal());
    });

    const dataSubscription = terminal.onData((data) => {
      void writeAgentSessionInput(sessionId, data).catch((e) => {
        if (active) setError(commandMessage(e));
      });
    });
    const cursorSubscription = terminal.onCursorMove(() => {
      if (textarea) stabilizeTerminalTextarea(textarea);
    });
    const relayInput = (input: string) => {
      if (!input) return;
      scheduleSettledFit();
      focusTerminal();
      terminal.input(input, true);
    };
    const pasteText = (text: string) => {
      if (!text) return;
      scheduleSettledFit();
      focusTerminal();
      terminal.paste(text);
    };
    terminal.attachCustomKeyEventHandler((event) => {
      if (
        !active ||
        event.type !== "keydown" ||
        event.isComposing ||
        event.key === "Process" ||
        event.keyCode === 229
      ) {
        return true;
      }
      if (isPasteShortcut(event) && navigator.clipboard?.readText) {
        event.preventDefault();
        event.stopPropagation();
        void navigator.clipboard
          .readText()
          .then((text) => {
            if (active) pasteText(text);
          })
          .catch(() => {
            if (active) focusLater();
          });
        return false;
      }
      return true;
    });
    const handlePointerDown = () => {
      scheduleSettledFit();
      focusLater(true);
    };
    const handleClick = () => {
      scheduleSettledFit();
      focusLater(true);
    };
    const handleFocusIn = () => {
      scheduleSettledFit();
      focusLater(true);
    };
    const handleTextareaFocus = () => {
      if (textarea) stabilizeTerminalTextarea(textarea);
    };
    const handleTextareaBlur = () => {
      recoverTerminalFocus();
    };
    const handleTextareaPaste = (event: ClipboardEvent) => {
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (!text) return;
      event.preventDefault();
      event.stopPropagation();
      pasteText(text);
    };
    const handleWindowPaste = (event: ClipboardEvent) => {
      if (
        !active ||
        !isTerminalPanelActive(panelActiveRef.current, container, textarea, event.target)
      ) {
        return;
      }
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (!text) return;
      event.preventDefault();
      event.stopPropagation();
      pasteText(text);
    };
    const handleWindowKeyDown = (event: KeyboardEvent) => {
      if (
        !active ||
        event.defaultPrevented ||
        event.isComposing ||
        !isTerminalPanelActive(panelActiveRef.current, container, textarea, event.target)
      ) {
        return;
      }
      if (isPasteShortcut(event) && navigator.clipboard?.readText) {
        event.preventDefault();
        event.stopPropagation();
        void navigator.clipboard
          .readText()
          .then((text) => {
            if (active) pasteText(text);
          })
          .catch(() => {
            if (active) focusLater();
          });
        return;
      }
      const input = keyboardEventToTerminalInput(event);
      if (input === null) return;
      event.preventDefault();
      event.stopPropagation();
      relayInput(input);
    };
    container.addEventListener("pointerdown", handlePointerDown);
    container.addEventListener("click", handleClick);
    container.addEventListener("focusin", handleFocusIn);
    textarea?.addEventListener("focus", handleTextareaFocus);
    textarea?.addEventListener("blur", handleTextareaBlur);
    textarea?.addEventListener("paste", handleTextareaPaste);
    window.addEventListener("paste", handleWindowPaste, true);
    window.addEventListener("keydown", handleWindowKeyDown, true);
    scheduleSettledFit();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => {
            scheduleFit();
          });
    observer?.observe(container);

    return () => {
      active = false;
      window.cancelAnimationFrame(focusFrame);
      clearSettledFitTimers();
      window.clearTimeout(resizeTimer);
      observer?.disconnect();
      container.removeEventListener("pointerdown", handlePointerDown);
      container.removeEventListener("click", handleClick);
      container.removeEventListener("focusin", handleFocusIn);
      textarea?.removeEventListener("focus", handleTextareaFocus);
      textarea?.removeEventListener("blur", handleTextareaBlur);
      textarea?.removeEventListener("paste", handleTextareaPaste);
      window.removeEventListener("paste", handleWindowPaste, true);
      window.removeEventListener("keydown", handleWindowKeyDown, true);
      activeSubscription.dispose();
      dataSubscription.dispose();
      cursorSubscription.dispose();
      fitAddon.dispose();
      terminal.dispose();
      terminalInstanceRef.current = null;
      schedulePanelCloseStop(
        sessionId,
        consumeTerminalDetachedMarker(sessionId)
          ? DETACHED_TRANSFER_STOP_DELAY_MS
          : PANEL_CLOSE_STOP_DELAY_MS,
      );
    };
  }, [focusTerminal, sessionId]);

  useEffect(() => {
    const terminal = terminalInstanceRef.current;
    if (!terminal) return;
    const totalAppended = sessionOutputTotal;
    const alreadyWritten = writtenOutputRef.current;
    const newChunkCount = totalAppended - alreadyWritten;
    if (newChunkCount > 0) {
      const startIdx = Math.max(0, sessionOutput.length - newChunkCount);
      for (let index = startIdx; index < sessionOutput.length; index += 1) {
        terminal.write(decodeBase64(sessionOutput[index].chunk_base64));
      }
      writtenOutputRef.current = totalAppended;
    }
    if (
      panelActiveRef.current &&
      terminal.textarea &&
      document.activeElement !== terminal.textarea
    ) {
      window.requestAnimationFrame(() => focusTerminal());
    }
  }, [focusTerminal, sessionOutput, sessionOutputTotal]);

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

  useEffect(() => {
    if (!sessionId) return;
    let active = true;
    const refresh = () => {
      void listAgentSessions()
        .then((sessions) => {
          if (active) agentSessionStore.setSessions(sessions);
        })
        .catch(() => {});
    };
    const timer = window.setInterval(refresh, SESSION_REFRESH_INTERVAL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [sessionId]);

  const canRevert =
    !!session &&
    !!session.checkpoint &&
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

  const canRevertTurnFile =
    !!session &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const onRevertTurnFile = async (turnCheckpointId: string, path: string) => {
    if (!sessionId || !canRevertTurnFile || revertingFile) return;
    const ok = await confirm(`Revert ${path} from this turn checkpoint?`, {
      title: "Revert file from turn",
      kind: "warning",
      okLabel: "Revert file",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setRevertingFile(`${turnCheckpointId}:${path}`);
    setError(null);
    try {
      const updated = await revertSessionTurnFile(sessionId, turnCheckpointId, path, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setRevertingFile(null);
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
              : session && !session.checkpoint
                ? "This session has no reversible checkpoint"
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
      {session && (
        <AgentLens
          session={session}
          canRevertTurnFile={canRevertTurnFile}
          revertingFile={revertingFile}
          onRevertTurnFile={onRevertTurnFile}
        />
      )}
      <div
        className="terminal-panel__surface"
        ref={terminalRef}
        data-testid="terminal-surface"
        tabIndex={0}
      />
    </div>
  );
}

function AgentLens({
  session,
  canRevertTurnFile,
  revertingFile,
  onRevertTurnFile,
}: {
  session: AgentSession;
  canRevertTurnFile: boolean;
  revertingFile: string | null;
  onRevertTurnFile: (turnCheckpointId: string, path: string) => void;
}) {
  const turns = session.turn_checkpoints ?? [];
  const visibleTurns = turns.slice(-3);
  return (
    <section className="terminal-panel__lens" aria-label="Agent Lens">
      <div className="terminal-panel__lens-summary">
        <span className="terminal-panel__lens-title">Agent Lens</span>
        <span>{turnStatusLabel(session.turn_status ?? "waiting")}</span>
        <span>{turns.length} checkpoints</span>
        <span>{session.change_log?.length ?? 0} session changes</span>
      </div>
      {visibleTurns.length > 0 && (
        <div className="terminal-panel__turns">
          {visibleTurns.map((turn) => (
            <div className="terminal-panel__turn" key={turn.id}>
              <div className="terminal-panel__turn-head">
                <span>Turn {turn.index}</span>
                <span>{turn.changes.length} changes during turn</span>
              </div>
              <div className="terminal-panel__turn-files">
                {turn.changes.map((change) => {
                  const key = `${turn.id}:${change.path}`;
                  return (
                    <button
                      className="terminal-panel__turn-file"
                      key={`${change.kind}:${change.path}`}
                      disabled={!canRevertTurnFile || revertingFile === key}
                      onClick={() => onRevertTurnFile(turn.id, change.path)}
                      title={
                        canRevertTurnFile
                          ? `Revert ${change.path} from turn ${turn.index}`
                          : "Stop the session before reverting files"
                      }
                    >
                      <span>{change.kind}</span>
                      <span>{change.path}</span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function turnStatusLabel(status: string): string {
  switch (status) {
    case "working":
      return "Working";
    case "settling":
      return "Settling";
    default:
      return "Waiting";
  }
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
    `Age: ${Math.round(session.age_ms / 1000)}s`,
    `Active sessions: ${session.active_sessions}`,
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

function isPasteShortcut(event: KeyboardEvent): boolean {
  const key = event.key.toLowerCase();
  return ((event.ctrlKey || event.metaKey) && key === "v") || (event.shiftKey && key === "insert");
}

function isTerminalPanelActive(
  panelActive: boolean,
  container: HTMLElement,
  textarea: HTMLTextAreaElement | undefined,
  target: EventTarget | null,
): boolean {
  if (!panelActive) return false;
  if (!target) return true;
  if (textarea && target === textarea) return false;
  if (target instanceof HTMLElement) {
    if (container.contains(target)) {
      return !isEditableElement(target);
    }
    if (target === document.body || target === document.documentElement) {
      return true;
    }
    return false;
  }
  return true;
}

function isEditableElement(target: HTMLElement): boolean {
  const tag = target.tagName;
  return target.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

function isFocusableElement(target: HTMLElement): boolean {
  return (
    target.matches(
      "a[href], button, input, select, textarea, [contenteditable='true'], [tabindex]:not([tabindex='-1'])",
    ) || target.tabIndex >= 0
  );
}

function shouldPreserveFocusedElement(target: Element | null, container: HTMLElement): boolean {
  if (!target || target === document.body || target === document.documentElement) {
    return false;
  }
  if (!(target instanceof HTMLElement)) {
    return true;
  }
  if (container.contains(target)) {
    return isEditableElement(target);
  }
  return isFocusableElement(target);
}

function stabilizeTerminalTextarea(textarea: HTMLTextAreaElement) {
  textarea.style.minWidth = "1px";
  textarea.style.minHeight = "1px";
  textarea.style.zIndex = "1";
  textarea.style.opacity = "0";
  textarea.style.pointerEvents = "none";
  textarea.style.background = "transparent";
  textarea.style.color = "transparent";
  textarea.style.caretColor = "transparent";
}

function keyboardEventToTerminalInput(event: KeyboardEvent): string | null {
  if (event.metaKey && !event.ctrlKey) return null;
  switch (event.key) {
    case "Enter":
      return "\r";
    case "Tab":
      return event.shiftKey ? "\u001b[Z" : "\t";
    case "Backspace":
      return "\u007f";
    case "Escape":
      return "\u001b";
    case "ArrowUp":
      return "\u001b[A";
    case "ArrowDown":
      return "\u001b[B";
    case "ArrowRight":
      return "\u001b[C";
    case "ArrowLeft":
      return "\u001b[D";
    case "Home":
      return "\u001b[H";
    case "End":
      return "\u001b[F";
    case "Insert":
      return event.shiftKey ? null : "\u001b[2~";
    case "Delete":
      return "\u001b[3~";
    case "PageUp":
      return "\u001b[5~";
    case "PageDown":
      return "\u001b[6~";
    default:
      break;
  }

  if (event.ctrlKey && !event.altKey && !event.metaKey) {
    return controlCharacter(event.key);
  }
  if (event.key.length !== 1) return null;
  if (event.altKey && !event.ctrlKey && !event.metaKey) {
    return `\u001b${event.key}`;
  }
  if (event.ctrlKey || event.metaKey) return null;
  return event.key;
}

function controlCharacter(key: string): string | null {
  const lower = key.toLowerCase();
  if (lower >= "a" && lower <= "z") {
    return String.fromCharCode(lower.charCodeAt(0) - 96);
  }
  switch (key) {
    case " ":
    case "@":
    case "2":
      return "\u0000";
    case "[":
    case "3":
      return "\u001b";
    case "\\":
    case "4":
      return "\u001c";
    case "]":
    case "5":
      return "\u001d";
    case "^":
    case "6":
      return "\u001e";
    case "_":
    case "/":
    case "7":
      return "\u001f";
    case "8":
      return "\u007f";
    default:
      return null;
  }
}

function cancelPanelCloseStop(sessionId: string) {
  const timer = panelCloseStopTimers.get(sessionId);
  if (timer === undefined) return;
  window.clearTimeout(timer);
  panelCloseStopTimers.delete(sessionId);
}

function schedulePanelCloseStop(sessionId: string, delayMs = PANEL_CLOSE_STOP_DELAY_MS) {
  cancelPanelCloseStop(sessionId);
  const timer = window.setTimeout(() => {
    panelCloseStopTimers.delete(sessionId);
    void stopAgentSession(sessionId)
      .then(() => listAgentSessions())
      .then((sessions) => agentSessionStore.setSessions(sessions))
      .catch(() => {});
  }, delayMs);
  panelCloseStopTimers.set(sessionId, timer);
}
