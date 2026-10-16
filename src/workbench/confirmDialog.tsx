// In-app replacement for the plugin-dialog `confirm`. Native OS dialogs can
// open behind the window and are invisible to UI automation (Pumarejo), so
// confirmations render inside the WebView. Same call shape as the plugin.

import { useRef } from "react";
import { createRoot } from "react-dom/client";
import { useAccessibleDialog } from "./useAccessibleDialog";

export interface ConfirmOptions {
  title?: string;
  kind?: "info" | "warning" | "error";
  okLabel?: string;
  cancelLabel?: string;
}

export function confirm(message: string, options: ConfirmOptions = {}): Promise<boolean> {
  return new Promise((resolve) => {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    const settle = (value: boolean) => {
      root.unmount();
      host.remove();
      resolve(value);
    };
    root.render(<ConfirmDialog message={message} options={options} onSettle={settle} />);
  });
}

// eslint-disable-next-line react-refresh/only-export-components -- private to confirm()
function ConfirmDialog({
  message,
  options,
  onSettle,
}: {
  message: string;
  options: ConfirmOptions;
  onSettle: (value: boolean) => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({
    onClose: () => onSettle(false),
    initialFocusRef: cancelRef,
  });
  const title = options.title ?? "Confirmar";
  const destructive = options.kind === "warning" || options.kind === "error";
  return (
    <div
      ref={dialogRef}
      className="file-op-modal-overlay"
      role="alertdialog"
      aria-modal="true"
      aria-label={title}
      onClick={(event) => {
        if (event.target === event.currentTarget) onSettle(false);
      }}
    >
      <div className="file-op-modal">
        <h2 className="file-op-modal__title">{title}</h2>
        <p className="file-op-modal__body file-op-modal__body--message">{message}</p>
        <div className="file-op-modal__actions">
          <button
            ref={cancelRef}
            type="button"
            className="file-op-modal__button file-op-modal__button--cancel"
            onClick={() => onSettle(false)}
          >
            {options.cancelLabel ?? "Cancelar"}
          </button>
          <button
            type="button"
            className={`file-op-modal__button${destructive ? " file-op-modal__button--confirm" : ""}`}
            onClick={() => onSettle(true)}
          >
            {options.okLabel ?? "Aceptar"}
          </button>
        </div>
      </div>
    </div>
  );
}
