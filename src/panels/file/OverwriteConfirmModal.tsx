// Confirmation modal for destructive file operations (overwrite existing
// files). Used by the explorer drag/drop + paste + move surfaces.

import { useCallback, useEffect } from "react";
import type { FileConflict } from "../../bus/contract";
import { conflictDescription, type FileOpReport } from "./fileOps";

interface OverwriteConfirmModalProps {
  report: FileOpReport;
  onConfirm: () => void;
  onCancel: () => void;
}

export function OverwriteConfirmModal({ report, onConfirm, onCancel }: OverwriteConfirmModalProps) {
  const handleKey = useCallback(
    (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCancel();
      } else if (event.key === "Enter") {
        event.preventDefault();
        onConfirm();
      }
    },
    [onCancel, onConfirm],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [handleKey]);

  const conflicts: FileConflict[] = report.conflicts.filter(
    (c) => c.kind === "file_exists" || c.kind === "dir_exists",
  );

  return (
    <div
      className="file-op-modal-overlay"
      role="dialog"
      aria-label="Confirmar sobrescritura"
      aria-modal="true"
      data-testid="overwrite-confirm-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="file-op-modal" data-testid="overwrite-confirm-modal">
        <h2 className="file-op-modal__title">Sobrescribir archivos</h2>
        <p className="file-op-modal__body">
          Los siguientes {conflicts.length} archivo(s) ya existen en el destino:
        </p>
        <ul className="file-op-modal__list" data-testid="overwrite-confirm-list">
          {conflicts.map((c) => (
            <li key={c.dest_rel} className="file-op-modal__item">
              <span className="file-op-modal__kind" data-testid={`conflict-kind-${c.kind}`}>
                {c.kind === "dir_exists" ? "dir" : "file"}
              </span>
              <span className="file-op-modal__path" title={c.dest_rel}>
                {c.dest_rel}
              </span>
            </li>
          ))}
        </ul>
        <p className="file-op-modal__warning">¿Sobrescribir?</p>
        <div className="file-op-modal__actions">
          <button
            type="button"
            className="file-op-modal__button file-op-modal__button--cancel"
            data-testid="overwrite-confirm-cancel"
            onClick={onCancel}
          >
            Cancelar
          </button>
          <button
            type="button"
            className="file-op-modal__button file-op-modal__button--confirm"
            data-testid="overwrite-confirm-ok"
            onClick={onConfirm}
          >
            Sobrescribir
          </button>
        </div>
        <p className="file-op-modal__hint">Enter = Sobrescribir · Esc = Cancelar</p>
        {conflicts.some((c) => c.kind === "dir_exists") && (
          <p
            className="file-op-modal__warning file-op-modal__warning--dir"
            data-testid="overwrite-confirm-dir-warning"
          >
            Los directorios existentes serán reemplazados por completo.
          </p>
        )}
        {/* Unused imported helper preserved for future per-conflict labels. */}
        {conflicts.length > 0 && <span hidden>{conflictDescription(conflicts[0])}</span>}
      </div>
    </div>
  );
}
