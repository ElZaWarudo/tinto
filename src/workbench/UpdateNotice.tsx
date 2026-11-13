import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { errorText } from "../panels/delivery/errorText";

/** Offers the latest release when Tinto starts; installing it restarts Tinto. */
export function UpdateNotice() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  // Whole download percent while the size is known; null before, without a
  // known size, and once the download has finished.
  const [percent, setPercent] = useState<number | null>(null);
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
    let total = 0;
    let downloaded = 0;
    let shown = -1;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          if (total > 0) setPercent((shown = 0));
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          const next = Math.min(100, Math.floor((downloaded / total) * 100));
          // Chunks arrive far more often than the whole percent moves.
          if (total > 0 && next !== shown) setPercent((shown = next));
        } else {
          setPercent(null);
        }
      });
      await relaunch();
    } catch (cause) {
      setError(`No se pudo instalar: ${errorText(cause)}`);
      setInstalling(false);
      setPercent(null);
    }
  };

  return (
    <div className="app-shell__notice" role="status" data-testid="update-notice">
      <span>Tinto {update.version} está disponible.</span>
      {error && <span>{error}</span>}
      <button type="button" disabled={installing} onClick={() => void install()}>
        {!installing
          ? "Actualizar y reiniciar"
          : percent !== null
            ? `Descargando ${percent} %`
            : "Instalando…"}
      </button>
      {!installing && (
        <button type="button" onClick={() => setUpdate(null)}>
          Más tarde
        </button>
      )}
      {percent !== null && (
        <span
          className="app-shell__notice-progress"
          aria-hidden="true"
          style={{ transform: `scaleX(${percent / 100})` }}
        />
      )}
    </div>
  );
}
