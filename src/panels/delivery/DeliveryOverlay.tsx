import {
  useEffect,
  type CSSProperties,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { useOneOverlay } from "./useOneOverlay";

interface Place {
  top: number;
  left?: number;
  right?: number;
  maxWidth: number;
  maxHeight: number;
  arrow: number | null;
}

/** Below the control that opened it, on the side of the panel it is in, with
 *  the arrow at its middle; without a control, in the panel's top right. */
function placeNear(anchor: Element | null, host: Element): Place {
  const panel = host.getBoundingClientRect();
  if (!anchor?.isConnected) {
    return {
      top: 72,
      right: 16,
      maxWidth: panel.width - 32,
      maxHeight: panel.height - 88,
      arrow: null,
    };
  }
  const button = anchor.getBoundingClientRect();
  const middle = button.left + button.width / 2;
  const top = button.bottom - panel.top + 8;
  const maxHeight = Math.max(120, panel.height - top - 8);
  if (middle > panel.left + panel.width / 2) {
    const right = Math.max(8, panel.right - button.right);
    return {
      top,
      right,
      maxWidth: panel.width - right - 8,
      maxHeight,
      arrow: Math.max(12, panel.right - right - middle),
    };
  }
  const left = Math.max(8, button.left - panel.left);
  return {
    top,
    left,
    maxWidth: panel.width - left - 8,
    maxHeight,
    arrow: Math.max(12, middle - panel.left - left),
  };
}

/** What opens on top of the Delivery panel: a raised card next to the control
 *  that opened it, its arrow at the control, over a dim layer. Focus moves
 *  into it; Escape or a click outside closes it. */
export function Overlay({
  anchor,
  label,
  role = "dialog",
  className,
  onClose,
  onKeyDown,
  children,
}: {
  anchor: Element | null;
  label?: string;
  role?: "dialog" | "menu";
  className?: string;
  onClose: () => void;
  onKeyDown?: (event: ReactKeyboardEvent<HTMLDivElement>) => void;
  children: ReactNode;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const [host] = useState(() => anchor?.closest(".delivery-panel") ?? document.body);
  const [place] = useState(() => placeNear(anchor, host));
  useOneOverlay(surface, anchor, onClose);
  useEffect(() => {
    const first = surface.current?.querySelector<HTMLElement>(
      '[role="menuitem"]:not(:disabled), [autofocus]',
    );
    (first ?? surface.current)?.focus();
  }, []);
  const { arrow, maxHeight, ...box } = place;
  return createPortal(
    <>
      <div className="delivery-overlay__dim" />
      <div
        ref={surface}
        className={`delivery-overlay${arrow === null ? "" : ` delivery-overlay--${place.left === undefined ? "end" : "start"}`}`}
        role={role}
        aria-label={label}
        tabIndex={-1}
        style={{ ...box, ...(arrow === null ? {} : { "--arrow": `${arrow}px` }) } as CSSProperties}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            // Only the one on top: Escape reaches the ones under it through React.
            event.stopPropagation();
            onClose();
            return;
          }
          onKeyDown?.(event);
        }}
      >
        <div
          className={`delivery-overlay__body${className ? ` ${className}` : ""}`}
          style={{ maxHeight }}
        >
          {children}
        </div>
      </div>
    </>,
    host,
  );
}
