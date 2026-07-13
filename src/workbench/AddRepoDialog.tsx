import { useEffect, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  addWslRepoFlow,
  listWslDirectoryFlow,
  listWslDistrosFlow,
  normalizeWslLinuxPath,
} from "./operations";
import type { WslDirectoryListing } from "../bus/client";
import { useAccessibleDialog } from "./useAccessibleDialog";

const FALLBACK_DISTROS = ["Ubuntu", "Ubuntu-24.04", "Ubuntu-22.04", "Ubuntu-20.04"];

export function AddRepoDialog({
  activeWorkbench,
  onClose,
  onAddLocal,
}: {
  activeWorkbench: string;
  onClose: () => void;
  onAddLocal?: () => Promise<void> | void;
}) {
  const [path, setPath] = useState("");
  const [alias, setAlias] = useState("");
  const [distro, setDistro] = useState("Ubuntu");
  const [distros, setDistros] = useState<string[]>(FALLBACK_DISTROS);
  const [listing, setListing] = useState<WslDirectoryListing | null>(null);
  const [error, setError] = useState("");
  const [localError, setLocalError] = useState("");
  const [hasLoadedDistros, setHasLoadedDistros] = useState(false);
  const [isLoadingDistros, setIsLoadingDistros] = useState(true);
  const [isBrowsing, setIsBrowsing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isAddingLocal, setIsAddingLocal] = useState(false);
  const pathInputRef = useRef<HTMLInputElement>(null);
  const localButtonRef = useRef<HTMLButtonElement>(null);
  const browseRequestRef = useRef(0);
  const isMutationBusy = isSaving || isAddingLocal;
  const isBrowseBusy = isBrowsing || isLoadingDistros;
  const requestClose = () => {
    if (!isMutationBusy) onClose();
  };
  const dialogRef = useAccessibleDialog<HTMLFormElement>({
    onClose: requestClose,
    initialFocusRef: onAddLocal ? localButtonRef : pathInputRef,
  });

  useEffect(() => {
    let active = true;
    void listWslDistrosFlow().then((found) => {
      if (!active) return;
      const next = found.length ? found : FALLBACK_DISTROS;
      setDistros(next);
      setIsBrowsing(true);
      setDistro((current) => (next.includes(current) ? current : next[0]));
      setHasLoadedDistros(true);
      setIsLoadingDistros(false);
    });
    return () => {
      active = false;
    };
  }, []);

  useEffect(
    () => () => {
      browseRequestRef.current += 1;
    },
    [],
  );

  useEffect(() => {
    let active = true;
    if (!distro || !hasLoadedDistros) return;
    const request = ++browseRequestRef.current;
    void listWslDirectoryFlow(distro, null).then((next) => {
      if (!active || request !== browseRequestRef.current) return;
      setListing(next);
      if (next) {
        setPath(next.path);
        setError("");
      } else {
        setError("No se pudo leer esa carpeta en WSL.");
      }
      setIsBrowsing(false);
    });
    return () => {
      active = false;
    };
  }, [distro, hasLoadedDistros]);

  const browseTo = async (nextPath: string | null) => {
    const request = ++browseRequestRef.current;
    setIsBrowsing(true);
    const next = await listWslDirectoryFlow(distro, nextPath);
    if (request !== browseRequestRef.current) return;
    setListing(next);
    if (next) {
      setPath(next.path);
      setError("");
    } else {
      setError("No se pudo leer esa carpeta en WSL.");
    }
    setIsBrowsing(false);
  };

  const browseTypedPath = async () => {
    const normalized = normalizeWslLinuxPath(path);
    if (!normalized) {
      setError("Usa una ruta Linux absoluta, por ejemplo /home/usuario/proyecto.");
      return;
    }
    await browseTo(normalized);
  };

  const parentPath = (current: string): string | null => {
    const normalized = normalizeWslLinuxPath(current);
    if (!normalized || normalized === "/") return null;
    const parent = normalized.slice(0, normalized.lastIndexOf("/")) || "/";
    return parent;
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (isMutationBusy || isBrowseBusy) return;
    setLocalError("");
    const normalized = normalizeWslLinuxPath(path);
    if (!normalized) {
      setError("Usa una ruta Linux absoluta, por ejemplo /home/usuario/proyecto.");
      return;
    }
    setError("");
    setIsSaving(true);
    try {
      const stored = await addWslRepoFlow(activeWorkbench, {
        distro,
        path: normalized,
        alias,
      });
      if (stored) onClose();
      else setError("No se pudo agregar el repo.");
    } catch (e) {
      setError(extractErrorMessage(e, "No se pudo agregar el repo."));
    } finally {
      setIsSaving(false);
    }
  };
  const addLocal = async () => {
    if (!onAddLocal || isMutationBusy) return;
    setLocalError("");
    setIsAddingLocal(true);
    try {
      await onAddLocal();
    } catch (cause) {
      setLocalError(extractErrorMessage(cause, "No se pudo añadir el repositorio local."));
    } finally {
      setIsAddingLocal(false);
    }
  };
  const isUnified = Boolean(onAddLocal);

  return (
    <div className="addons-backdrop" data-testid="add-repo-backdrop" onClick={requestClose}>
      <form
        ref={dialogRef}
        className="addons-modal"
        role="dialog"
        aria-label="Agregar repo"
        aria-modal="true"
        aria-busy={isMutationBusy || isBrowseBusy}
        data-testid="add-repo-dialog"
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => void submit(event)}
      >
        <header className="addons-modal__head">
          <h2 className="addons-modal__title">Agregar repo</h2>
          <button
            type="button"
            className="addons-modal__close"
            aria-label="Cerrar"
            data-testid="add-repo-close"
            disabled={isMutationBusy}
            onClick={requestClose}
          >
            ×
          </button>
        </header>

        <div className="addons-modal__body">
          {onAddLocal && (
            <section className="addons-card addons-card--repo-source">
              <div className="addons-card__main">
                <h3 className="addons-card__title">Carpeta local</h3>
              </div>
              <button
                ref={localButtonRef}
                type="button"
                className="addons-refresh"
                data-testid="add-local-repo"
                disabled={isMutationBusy}
                onClick={() => void addLocal()}
              >
                {isAddingLocal ? "Abriendo…" : "Elegir carpeta"}
              </button>
            </section>
          )}
          <section className="addons-card addons-card--wsl">
            {isUnified && (
              <div className="addons-card__main">
                <h3 className="addons-card__title">Linux en WSL</h3>
              </div>
            )}
            <div className="wsl-form-grid">
              <label className="wsl-field" htmlFor="wsl-distro">
                <span className="wsl-field__label">Distro</span>
                <select
                  id="wsl-distro"
                  className="wsl-select"
                  data-testid="wsl-distro"
                  value={distro}
                  disabled={isBrowseBusy || isMutationBusy}
                  onChange={(event) => {
                    const nextDistro = event.target.value;
                    if (nextDistro === distro) return;
                    browseRequestRef.current += 1;
                    setDistro(nextDistro);
                    setPath("");
                    setListing(null);
                    setIsBrowsing(true);
                  }}
                >
                  {distros.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>

              <label className="wsl-field" htmlFor="wsl-alias">
                <span className="wsl-field__label">Alias</span>
                <input
                  id="wsl-alias"
                  className="wsl-input"
                  data-testid="wsl-alias"
                  value={alias}
                  disabled={isMutationBusy}
                  onChange={(event) => setAlias(event.target.value)}
                  placeholder="Nombre visible"
                />
              </label>
            </div>
            {isLoadingDistros && <p className="addons-card__text">Detectando distros...</p>}

            <label className="wsl-field" htmlFor="wsl-path">
              <span className="wsl-field__label">Ruta Linux</span>
              <div className="wsl-path-row">
                <input
                  ref={pathInputRef}
                  id="wsl-path"
                  className="wsl-input wsl-input--path"
                  data-testid="wsl-path"
                  value={path}
                  disabled={isMutationBusy}
                  onChange={(event) => setPath(event.target.value)}
                  placeholder="/home/usuario/proyecto"
                />
                <button
                  type="button"
                  className="wsl-browser__primary"
                  data-testid="wsl-browse-path"
                  disabled={isBrowseBusy || isMutationBusy}
                  onClick={() => void browseTypedPath()}
                >
                  {isBrowsing ? "Leyendo..." : "Ir"}
                </button>
              </div>
            </label>

            <div className="wsl-browser__toolbar">
              <button
                type="button"
                className="wsl-browser__nav"
                data-testid="wsl-home"
                disabled={isBrowseBusy || isMutationBusy}
                onClick={() => void browseTo(null)}
              >
                Inicio
              </button>
              <button
                type="button"
                className="wsl-browser__nav"
                data-testid="wsl-up"
                disabled={isBrowseBusy || isMutationBusy || !parentPath(path)}
                onClick={() => void browseTo(parentPath(path))}
              >
                Subir
              </button>
            </div>

            {listing && (
              <div className="wsl-browser" data-testid="wsl-browser">
                <div className="wsl-browser__status">
                  <span>{listing.path}</span>
                  {listing.is_git_repo && <strong>Repositorio Git</strong>}
                </div>
                <div className="wsl-browser__list">
                  {listing.entries.length === 0 ? (
                    <p className="addons-card__text">No hay subcarpetas.</p>
                  ) : (
                    listing.entries.map((entry) => (
                      <button
                        key={entry.path}
                        type="button"
                        className="wsl-browser__item"
                        data-testid={`wsl-dir-${entry.path}`}
                        title={entry.path}
                        disabled={isBrowseBusy || isMutationBusy}
                        onClick={() => void browseTo(entry.path)}
                      >
                        {entry.name}
                      </button>
                    ))
                  )}
                </div>
              </div>
            )}

            {(localError || error) && (
              <p className="addons-card__error" data-testid="add-repo-error" role="alert">
                {localError || error}
              </p>
            )}

            <div className="addons-card__footer">
              <button
                type="button"
                className="addons-refresh"
                disabled={isMutationBusy}
                onClick={requestClose}
              >
                Cancelar
              </button>
              <button
                type="submit"
                className="addons-refresh"
                data-testid="add-repo-submit"
                disabled={isMutationBusy || isBrowseBusy}
              >
                {isSaving ? "Agregando..." : "Agregar"}
              </button>
            </div>
          </section>
        </div>
      </form>
    </div>
  );
}

function extractErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string" &&
    error.message.trim()
  ) {
    return error.message;
  }
  return fallback;
}
