import { useEffect } from "react";
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

  const onHandlePointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    armExternalTabDetach(event.nativeEvent, async () => {
      const terminals = consoleDock.openTerminalParams();
      terminals.forEach((params) => markTerminalDetached(params.sessionId));
      const reattached = await reattachDetachedConsoles(terminals);
      if (reattached) {
        await closeCurrentDetachedWindow();
      }
    });
  };

  return (
    <div className="detached-terminal-window">
      <div className="detached-terminal-window__bar">
        <button
          className="detached-terminal-window__drag-tab"
          type="button"
          onPointerDown={onHandlePointerDown}
        >
          Agents
        </button>
      </div>
      <div className="detached-terminal-window__body">
        <ConsoleDockPanel restoreTransferLayout />
      </div>
    </div>
  );
}
