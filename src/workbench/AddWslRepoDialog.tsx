import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import {
  addWslRepoFlow,
  listWslDirectoryFlow,
  listWslDistrosFlow,
  normalizeWslLinuxPath,
} from "./operations";
import type { WslDirectoryListing } from "../bus/client";

const FALLBACK_DISTROS = ["Ubuntu", "Ubuntu-24.04", "Ubuntu-22.04", "Ubuntu-20.04"];

export function AddWslRepoDialog({
  activeWorkbench,
  onClose,
}: {
  activeWorkbench: string;
  onClose: () => void;
}) {
  const [path, setPath] = useState("");
  const [alias, setAlias] = useState("");
  const [distro, setDistro] = useState("Ubuntu");
  const [distros, setDistros] = useState<string[]>(FALLBACK_DISTROS);
  const [listing, setListing] = useState<WslDirectoryListing | null>(null);
  const [error, setError] = useState("");
  const [hasLoadedDistros, setHasLoadedDistros] = useState(false);
  const [isLoadingDistros, setIsLoadingDistros] = useState(true);
  const [isBrowsing, setIsBrowsing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

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

  useEffect(() => {
    let active = true;
    if (!distro || !hasLoadedDistros) return;
    void listWslDirectoryFlow(distro, null).then((next) => {
      if (!active) return;
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
    setIsBrowsing(true);
    const next = await listWslDirectoryFlow(distro, nextPath);
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
      setError("Usa un path Linux absoluto, por ejemplo /home/usuario/proyecto.");
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
    const normalized = normalizeWslLinuxPath(path);
    if (!normalized) {
      setError("Usa un path Linux absoluto, por ejemplo /home/usuario/proyecto.");
      return;
    }
    setError("");
    setIsSaving(true);
    const stored = await addWslRepoFlow(activeWorkbench, {
      distro,
      path: normalized,
      alias,
    });
    setIsSaving(false);
    if (stored) onClose();
    else setError("No se pudo agregar el repo WSL.");
  };

  return (
    <div className="addons-backdrop" data-testid="add-wsl-backdrop" onClick={onClose}>
      <form
        className="addons-modal"
        role="dialog"
        aria-label="Agregar repo WSL"
        aria-modal="true"
        data-testid="add-wsl-dialog"
        onClick={(event) => event.stopPropagation()}
        onSubmit={(event) => void submit(event)}
      >
        <header className="addons-modal__head">
          <h2 className="addons-modal__title">Agregar repo WSL</h2>
          <button
            type="button"
            className="addons-modal__close"
            aria-label="Cerrar"
            data-testid="add-wsl-close"
            onClick={onClose}
          >
            ×
          </button>
        </header>

        <div className="addons-modal__body">
          <section className="addons-card addons-card--wsl">
            <div className="wsl-form-grid">
              <label className="wsl-field" htmlFor="wsl-distro">
                <span className="wsl-field__label">Distro</span>
                <select
                  id="wsl-distro"
                  className="wsl-select"
                  data-testid="wsl-distro"
                  value={distro}
                  onChange={(event) => {
                    setDistro(event.target.value);
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
                  onChange={(event) => setAlias(event.target.value)}
                  placeholder="Nombre visible"
                />
              </label>
            </div>
            {isLoadingDistros && <p className="addons-card__text">Detectando distros...</p>}

            <label className="wsl-field" htmlFor="wsl-path">
              <span className="wsl-field__label">Path Linux</span>
              <div className="wsl-path-row">
                <input
                  id="wsl-path"
                  className="wsl-input wsl-input--path"
                  data-testid="wsl-path"
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  placeholder="/home/usuario/proyecto"
                  autoFocus
                />
                <button
                  type="button"
                  className="wsl-browser__primary"
                  data-testid="wsl-browse-path"
                  disabled={isBrowsing}
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
                disabled={isBrowsing}
                onClick={() => void browseTo(null)}
              >
                Home
              </button>
              <button
                type="button"
                className="wsl-browser__nav"
                data-testid="wsl-up"
                disabled={isBrowsing || !parentPath(path)}
                onClick={() => void browseTo(parentPath(path))}
              >
                Subir
              </button>
            </div>

            {listing && (
              <div className="wsl-browser" data-testid="wsl-browser">
                <div className="wsl-browser__status">
                  <span>{listing.path}</span>
                  {listing.is_git_repo && <strong>Git repo</strong>}
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
                        disabled={isBrowsing}
                        onClick={() => void browseTo(entry.path)}
                      >
                        {entry.name}
                      </button>
                    ))
                  )}
                </div>
              </div>
            )}

            {error && (
              <p className="addons-card__error" data-testid="add-wsl-error">
                {error}
              </p>
            )}

            <div className="addons-card__footer">
              <button type="button" className="addons-refresh" onClick={onClose}>
                Cancelar
              </button>
              <button
                type="submit"
                className="addons-refresh"
                data-testid="add-wsl-submit"
                disabled={isSaving}
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
