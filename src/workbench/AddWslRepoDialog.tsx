import { useEffect, useState } from "react";
import type { FormEvent } from "react";
import { addWslRepoFlow, normalizeWslLinuxPath } from "./operations";

export function AddWslRepoDialog({
  activeWorkbench,
  onClose,
}: {
  activeWorkbench: string;
  onClose: () => void;
}) {
  const [path, setPath] = useState("");
  const [alias, setAlias] = useState("");
  const [error, setError] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

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
      distro: "Ubuntu",
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
          <section className="addons-card">
            <label className="addons-card__text" htmlFor="wsl-distro">
              Distro
            </label>
            <select id="wsl-distro" data-testid="wsl-distro" value="Ubuntu" disabled>
              <option value="Ubuntu">Ubuntu</option>
            </select>

            <label className="addons-card__text" htmlFor="wsl-path">
              Path Linux
            </label>
            <input
              id="wsl-path"
              data-testid="wsl-path"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="/home/usuario/proyecto"
              autoFocus
            />

            <label className="addons-card__text" htmlFor="wsl-alias">
              Alias
            </label>
            <input
              id="wsl-alias"
              data-testid="wsl-alias"
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              placeholder="Nombre visible"
            />

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
