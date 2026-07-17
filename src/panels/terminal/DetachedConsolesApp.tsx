import { useEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { useBusConnection } from "../../bus/connection";
import { consoleDock } from "../../workspace/consoleDock";
import { armExternalTabDetach } from "../../workspace/externalTabDetach";
import { ConsoleDockPanel } from "./ConsoleDockPanel";
import {
  closeCurrentDetachedWindow,
  markTerminalDetached,
  onDetachedConsolesOpenTerminal,
  reattachDetachedConsoles,
} from "./detachTerminalWindow";
import type { TerminalPanelParams } from "./TerminalPanel";

export function DetachedConsolesApp({
  initialTerminals = [],
}: {
  initialTerminals?: TerminalPanelParams[];
}) {
  useBusConnection();
  const reattachRunning = useRef(false);
  const transferCompletedRef = useRef(false);
  const [transferCompleted, setTransferCompleted] = useState(false);
  const [reattachPending, setReattachPending] = useState(false);
  const [reattachError, setReattachError] = useState<string | null>(null);

  useEffect(() => {
    initialTerminals.forEach((params) => consoleDock.openTerminal(params));
  }, [initialTerminals]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void onDetachedConsolesOpenTerminal((params) => {
      consoleDock.openTerminal(params);
    })
      .then((dispose) => {
        if (!active) {
          dispose();
          return;
        }
        unlisten = dispose;
      })
      .catch((error) => {
        console.error("Failed to listen for detached console terminal events", error);
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const reattach = async () => {
    if (reattachRunning.current) return;
    reattachRunning.current = true;
    setReattachPending(true);
    setReattachError(null);
    try {
      if (transferCompletedRef.current) {
        const closed = await closeCurrentDetachedWindow();
        if (!closed) {
          setReattachError(
            "Los Agents ya están reanexados; no se pudo cerrar esta ventana. Inténtalo de nuevo.",
          );
        }
        return;
      }
      const terminals = consoleDock.openTerminalParams();
      const reattached = await reattachDetachedConsoles(terminals);
      if (!reattached) {
        setReattachError("No se pudieron reanexar los Agents. Inténtalo de nuevo.");
        return;
      }
      terminals.forEach((params) => markTerminalDetached(params.sessionId));
      transferCompletedRef.current = true;
      setTransferCompleted(true);
      const closed = await closeCurrentDetachedWindow();
      if (!closed) {
        setReattachError(
          "Los Agents ya están reanexados; no se pudo cerrar esta ventana. Inténtalo de nuevo.",
        );
      }
    } catch (error) {
      console.error("tinto: detached agent reattach failed", error);
      setReattachError(
        transferCompletedRef.current
          ? "Los Agents ya están reanexados; no se pudo cerrar esta ventana. Inténtalo de nuevo."
          : "No se pudieron reanexar los Agents. Inténtalo de nuevo.",
      );
    } finally {
      reattachRunning.current = false;
      setReattachPending(false);
    }
  };

  const onHandlePointerDown = (event: PointerEvent<HTMLDivElement>) => {
    armExternalTabDetach(event.nativeEvent, reattach);
  };

  return (
    <div className="detached-terminal-window">
      <div className="detached-terminal-window__bar">
        <div
          className="detached-terminal-window__drag-tab"
          onPointerDown={onHandlePointerDown}
          title="Arrastrar para devolver los Agents a la ventana principal"
        >
          <span aria-hidden="true">⠿</span>
          <span>Agents</span>
        </div>
        <button
          className="detached-terminal-window__reattach"
          type="button"
          onClick={() => void reattach()}
          title="Reanexar Agents a la ventana principal"
          disabled={reattachPending}
          aria-busy={reattachPending ? "true" : undefined}
        >
          {reattachPending
            ? "Reanexando Agents…"
            : reattachError
              ? transferCompleted
                ? "Reintentar cierre"
                : "Reintentar reanexado"
              : "Reanexar Agents"}
        </button>
        {reattachError && (
          <span className="detached-terminal-window__error" role="alert">
            {reattachError}
          </span>
        )}
      </div>
      <div className="detached-terminal-window__body">
        <ConsoleDockPanel restoreTransferLayout />
      </div>
    </div>
  );
}
