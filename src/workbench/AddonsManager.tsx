// Addons management modal for optional integrations (for example, Gitleaks).

import { useEffect, useState } from "react";
import { getGitleaksSetupStatus, installGitleaks } from "../bus/client";
import type { GitleaksInstallResult, GitleaksSetupStatus } from "../bus/contract";
import { useAccessibleDialog } from "./useAccessibleDialog";

function detectStatusClass(status: GitleaksSetupStatus | null): string {
  if (!status) return "addons-status--loading";
  return status.installed ? "addons-status--ok" : "addons-status--warn";
}

function detectStatusText(status: GitleaksSetupStatus | null): string {
  if (!status) return "Comprobando estado…";
  if (!status.installed) return "No instalado";
  return `Disponible${status.version ? ` · ${status.version}` : ""}`;
}

function formatBinaryPath(status: GitleaksSetupStatus | null): string {
  if (!status?.installed) return "";
  return status.binary_path ? ` (${status.binary_path})` : "";
}

function extractErrorMessage(error: unknown): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  return "No se pudo completar la acción de instalación automática de Gitleaks.";
}

function toStatus(result: GitleaksInstallResult): GitleaksSetupStatus {
  return {
    installed: result.installed,
    version: result.version,
    binary_path: result.binary_path,
  };
}

export function AddonsManager({ onClose }: { onClose: () => void }) {
  const [status, setStatus] = useState<GitleaksSetupStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [installMessage, setInstallMessage] = useState<string>("");
  const [installError, setInstallError] = useState(false);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({ onClose });

  const refresh = async () => {
    setError(false);
    setIsLoading(true);
    try {
      setStatus(await getGitleaksSetupStatus());
    } catch {
      setError(true);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    let active = true;
    void getGitleaksSetupStatus()
      .then((nextStatus) => {
        if (!active) return;
        setStatus(nextStatus);
        setError(false);
      })
      .catch(() => {
        if (active) setError(true);
      })
      .finally(() => {
        if (active) setIsLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const runInstall = async () => {
    setIsInstalling(true);
    setInstallError(false);
    setInstallMessage("Instalando Gitleaks...");
    try {
      const result = await installGitleaks();
      setStatus(toStatus(result));
      setInstallMessage(result.message);
    } catch (err) {
      setInstallError(true);
      setInstallMessage(extractErrorMessage(err));
    } finally {
      setIsInstalling(false);
      await refresh();
    }
  };

  const isInstalled = status?.installed ?? false;

  return (
    <div className="addons-backdrop" data-testid="addons-backdrop" onClick={onClose}>
      <div
        ref={dialogRef}
        className="addons-modal"
        role="dialog"
        aria-label="Gestión de complementos"
        aria-modal="true"
        data-testid="addons-modal"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="addons-modal__head">
          <h2 className="addons-modal__title">Complementos</h2>
          <button
            type="button"
            className="addons-modal__close"
            aria-label="Cerrar"
            data-testid="addons-close"
            onClick={onClose}
          >
            ×
          </button>
        </header>

        <div className="addons-modal__body">
          <section className="addons-card">
            <div className="addons-card__head">
              <h3 className="addons-card__title">Gitleaks</h3>
              <span
                className={`addons-status ${detectStatusClass(status)}`}
                data-testid="gitleaks-status"
                role="status"
                aria-live="polite"
              >
                {detectStatusText(status)}
              </span>
            </div>

            {error ? (
              <p className="addons-card__error" role="alert">
                No se pudo consultar el estado del sistema desde Tinto. Reintenta en unos segundos.
              </p>
            ) : (
              <>
                <p className="addons-card__text">
                  Estado actual: {isLoading ? "Comprobando estado…" : "listo"}.
                  {formatBinaryPath(status)}
                </p>

                {installMessage && (
                  <p
                    className={installError ? "addons-card__error" : "addons-card__text"}
                    role={installError ? "alert" : "status"}
                    aria-live={installError ? "assertive" : "polite"}
                  >
                    {installMessage}
                  </p>
                )}
              </>
            )}

            <div className="addons-card__footer">
              <a
                href="https://github.com/gitleaks/gitleaks"
                target="_blank"
                rel="noreferrer"
                className="addons-docs-link"
              >
                Ver documentación oficial de Gitleaks
              </a>
              <button
                type="button"
                className="addons-refresh"
                onClick={() => void runInstall()}
                disabled={isLoading || isInstalling}
                data-testid="gitleaks-install"
              >
                {isInstalling
                  ? "Instalando…"
                  : isInstalled
                    ? "Revalidar instalación"
                    : "Instalar automáticamente"}
              </button>
              <button type="button" className="addons-refresh" onClick={() => void refresh()}>
                Recomprobar instalación
              </button>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
