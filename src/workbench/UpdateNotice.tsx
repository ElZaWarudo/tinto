import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errorText } from "../panels/delivery/errorText";

/** Offers the latest release when Tinto starts; installing it restarts Tinto. */
export function UpdateNotice() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    check()
      .then(setUpdate)
      .catch((cause) => console.warn("Could not check for a Tinto update", cause));
  }, []);

  if (!update) return null;

  const install = async () => {
    setInstalling(true);
    setError(null);
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (cause) {
      setError(`No se pudo instalar: ${errorText(cause)}`);
      setInstalling(false);
    }
  };

  return (
    <div className="app-shell__notice" role="status" data-testid="update-notice">
      <span>Tinto {update.version} está disponible.</span>
      {error && <span>{error}</span>}
      <button type="button" disabled={installing} onClick={() => void install()}>
        {installing ? "Instalando…" : "Actualizar y reiniciar"}
      </button>
      {!installing && (
        <button type="button" onClick={() => setUpdate(null)}>
          Más tarde
        </button>
      )}
    </div>
  );
}
