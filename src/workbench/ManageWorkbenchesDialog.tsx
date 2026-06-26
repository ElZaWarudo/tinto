// Manage workbenches modal: list every workbench with the repos it owns,
// sorted by recent use, with rename / delete / activate actions per row. The
// active workbench is expanded by default; others collapse. A footer form
// creates a new workbench inline.

import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { basename } from "../bus/store";
import type { Workbench, WorkbenchConfig } from "../bus/contract";
import {
  createAndActivate,
  deleteWorkbenchFlow,
  renameWorkbenchFlow,
  switchWorkbench,
} from "./operations";
import { sortByRecency } from "./recentWorkbenches";

interface ManageWorkbenchesDialogProps {
  config: WorkbenchConfig;
  onClose: () => void;
}

function repoLabel(repo: Workbench["repos"][number]): string {
  if (repo.alias && repo.alias.trim()) return repo.alias;
  if (repo.source === "wsl") return `${repo.distro ?? "WSL"}:${repo.path}`;
  return basename(repo.path) || repo.path;
}

function repoSubtitle(repo: Workbench["repos"][number]): string {
  if (repo.alias && repo.alias.trim()) return repo.path;
  if (repo.source === "wsl") return repo.path;
  return "";
}

export function ManageWorkbenchesDialog({ config, onClose }: ManageWorkbenchesDialogProps) {
  const ordered = useMemo(() => {
    const names = config.workbenches.map((w) => w.name);
    return sortByRecency(names);
  }, [config.workbenches]);

  // Default expansion: active is open, the rest are collapsed.
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([config.active ?? ""]));
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const renameInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (renaming && renameInputRef.current) {
      renameInputRef.current.focus();
      renameInputRef.current.select();
    }
  }, [renaming]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        if (renaming) {
          setRenaming(null);
          setRenameValue("");
        } else {
          onClose();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, renaming]);

  const toggle = (name: string) => {
    setExpanded((cur) => {
      const next = new Set(cur);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const startRename = (name: string) => {
    setRenaming(name);
    setRenameValue(name);
    setExpanded((cur) => new Set(cur).add(name));
  };

  const commitRename = async () => {
    if (!renaming) return;
    const target = renaming;
    const next = renameValue.trim();
    setRenaming(null);
    setRenameValue("");
    if (!next || next === target) return;
    setBusy(target);
    setError(null);
    try {
      await renameWorkbenchFlow(target, next);
    } catch (e) {
      setError(extractErrorMessage(e, "No se pudo renombrar la workbench."));
    } finally {
      setBusy(null);
    }
  };

  const handleActivate = async (name: string) => {
    setBusy(name);
    setError(null);
    try {
      await switchWorkbench(name, config.active);
    } catch (e) {
      setError(extractErrorMessage(e, "No se pudo activar la workbench."));
    } finally {
      setBusy(null);
    }
  };

  const handleDelete = async (name: string) => {
    const ok = await safeConfirm(
      `¿Eliminar la workbench "${name}"?\nLos repos en disco no se tocan. Las workbenches que comparten repos con esta no se ven afectadas.`,
      { title: "Eliminar workbench", kind: "warning", okLabel: "Eliminar", cancelLabel: "Cancelar" },
    );
    if (!ok) return;
    setBusy(name);
    setError(null);
    try {
      await deleteWorkbenchFlow(name);
    } catch (e) {
      setError(extractErrorMessage(e, "No se pudo eliminar la workbench."));
    } finally {
      setBusy(null);
    }
  };

  const handleCreate = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = newName.trim();
    if (!trimmed) return;
    setBusy("__new__");
    setError(null);
    try {
      await createAndActivate(trimmed);
      setNewName("");
      setExpanded((cur) => new Set(cur).add(trimmed));
    } catch (e) {
      setError(extractErrorMessage(e, "No se pudo crear la workbench."));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div
      className="addons-backdrop"
      data-testid="manage-workbenches-backdrop"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="addons-modal manage-workbenches-modal"
        role="dialog"
        aria-label="Gestionar workbenches"
        aria-modal="true"
        data-testid="manage-workbenches-modal"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="addons-modal__head">
          <h2 className="addons-modal__title">Gestionar workbenches</h2>
          <button
            type="button"
            className="addons-modal__close"
            aria-label="Cerrar"
            data-testid="manage-workbenches-close"
            onClick={onClose}
          >
            ×
          </button>
        </header>

        <div className="addons-modal__body manage-workbenches-modal__body">
          <p className="addons-modal__intro">
            Activá, renombrá o eliminá workbenches. Los repos compartidos entre varias no se
            tocan al eliminar una.
          </p>

          {error && (
            <p className="addons-card__error" data-testid="manage-workbenches-error">
              {error}
            </p>
          )}

          <ul className="manage-workbenches-modal__list" data-testid="manage-workbenches-list">
            {ordered.map((name) => {
              const wb = config.workbenches.find((w) => w.name === name);
              if (!wb) return null;
              const isActive = name === config.active;
              const isOpen = expanded.has(name);
              const isRenaming = renaming === name;
              const isBusy = busy === name;
              return (
                <li
                  key={name}
                  className={
                    "manage-workbench" + (isActive ? " manage-workbench--active" : "")
                  }
                  data-testid={`manage-workbench-row-${name}`}
                >
                  <div className="manage-workbench__head">
                    <button
                      type="button"
                      className="manage-workbench__toggle"
                      aria-expanded={isOpen}
                      data-testid={`manage-workbench-toggle-${name}`}
                      onClick={() => toggle(name)}
                    >
                      <span className="manage-workbench__caret" aria-hidden="true">
                        {isOpen ? "▾" : "▸"}
                      </span>
                      {isRenaming ? (
                        <input
                          ref={renameInputRef}
                          type="text"
                          className="manage-workbench__rename-input"
                          data-testid={`manage-workbench-rename-input-${name}`}
                          value={renameValue}
                          onChange={(e) => setRenameValue(e.target.value)}
                          onClick={(e) => e.stopPropagation()}
                          onKeyDown={(e) => {
                            e.stopPropagation();
                            if (e.key === "Enter") {
                              e.preventDefault();
                              void commitRename();
                            } else if (e.key === "Escape") {
                              e.preventDefault();
                              setRenaming(null);
                              setRenameValue("");
                            }
                          }}
                        />
                      ) : (
                        <span className="manage-workbench__name" title={name}>
                          {name}
                        </span>
                      )}
                      {isActive && (
                        <span
                          className="manage-workbench__badge"
                          data-testid={`manage-workbench-active-badge-${name}`}
                        >
                          activa
                        </span>
                      )}
                      <span className="manage-workbench__count">
                        {wb.repos.length === 1
                          ? "1 repo"
                          : `${wb.repos.length} repos`}
                      </span>
                    </button>
                    <div className="manage-workbench__actions">
                      <button
                        type="button"
                        className="addons-refresh"
                        disabled={isActive || isBusy}
                        data-testid={`manage-workbench-activate-${name}`}
                        onClick={() => void handleActivate(name)}
                      >
                        {isActive ? "Activa" : "Activar"}
                      </button>
                      <button
                        type="button"
                        className="addons-refresh"
                        disabled={isBusy || isRenaming}
                        data-testid={`manage-workbench-rename-${name}`}
                        onClick={() => startRename(name)}
                      >
                        Renombrar
                      </button>
                      <button
                        type="button"
                        className="addons-refresh manage-workbench__delete"
                        disabled={isBusy}
                        data-testid={`manage-workbench-delete-${name}`}
                        onClick={() => void handleDelete(name)}
                      >
                        Eliminar
                      </button>
                      {isRenaming && (
                        <button
                          type="button"
                          className="addons-refresh manage-workbench__rename-confirm"
                          data-testid={`manage-workbench-rename-confirm-${name}`}
                          onClick={() => void commitRename()}
                        >
                          Guardar
                        </button>
                      )}
                    </div>
                  </div>

                  {isOpen && (
                    <div
                      className="manage-workbench__repos"
                      data-testid={`manage-workbench-repos-${name}`}
                    >
                      {wb.repos.length === 0 ? (
                        <p className="manage-workbench__empty">Sin repos.</p>
                      ) : (
                        <ul className="manage-workbench__repo-list">
                          {wb.repos.map((repo) => (
                            <li
                              key={`${repo.source ?? "local"}:${repo.path}`}
                              className="manage-workbench__repo"
                              data-testid={`manage-workbench-repo-${name}-${repo.path}`}
                            >
                              <span className="manage-workbench__repo-label">
                                {repoLabel(repo)}
                              </span>
                              {repoSubtitle(repo) && (
                                <span
                                  className="manage-workbench__repo-subtitle"
                                  title={repoSubtitle(repo)}
                                >
                                  {repoSubtitle(repo)}
                                </span>
                              )}
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>

          <form
            className="manage-workbenches-modal__new"
            onSubmit={(e) => void handleCreate(e)}
          >
            <label className="manage-workbenches-modal__new-label" htmlFor="new-workbench-name">
              Nueva workbench
            </label>
            <div className="manage-workbenches-modal__new-row">
              <input
                id="new-workbench-name"
                type="text"
                className="wsl-input"
                placeholder="Ej. Cliente X"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                disabled={busy === "__new__"}
                data-testid="manage-workbench-new-input"
              />
              <button
                type="submit"
                className="addons-refresh"
                disabled={busy === "__new__" || !newName.trim()}
                data-testid="manage-workbench-new-submit"
              >
                Crear y activar
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}

async function safeConfirm(
  message: string,
  options: { title: string; kind: "warning"; okLabel: string; cancelLabel: string },
): Promise<boolean> {
  try {
    return await confirm(message, options);
  } catch {
    return window.confirm(message);
  }
}

function extractErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string" && message.trim()) return message;
  }
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
