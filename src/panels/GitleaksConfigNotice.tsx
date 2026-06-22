import { useState } from "react";
import { createRepoGitleaksConfig } from "../bus/client";

export function GitleaksConfigNotice({
  repo,
  compact = false,
  onAction,
}: {
  repo: string;
  compact?: boolean;
  onAction?: () => void;
}) {
  const [isConfiguring, setIsConfiguring] = useState(false);
  const [configured, setConfigured] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const configure = async () => {
    if (isConfiguring || configured) return;
    setIsConfiguring(true);
    setError(null);
    try {
      await createRepoGitleaksConfig(repo);
      setConfigured(true);
      onAction?.();
    } catch (cause) {
      if (
        typeof cause === "object" &&
        cause !== null &&
        "message" in cause &&
        typeof cause.message === "string"
      ) {
        setError(cause.message);
      } else {
        setError("No se pudo crear .gitleaks.toml.");
      }
    } finally {
      setIsConfiguring(false);
    }
  };

  return (
    <div
      className={compact ? "gitleaks-notice gitleaks-notice--compact" : "gitleaks-notice"}
      data-testid={compact ? "gitleaks-config-notice-compact" : "gitleaks-config-notice"}
    >
      <div className="gitleaks-notice__body">
        <strong className="gitleaks-notice__title">Gitleaks sin configuración local</strong>
        <span className="gitleaks-notice__text">
          Tinto puede crear <code>.gitleaks.toml</code> en este repo para ajustar falsos positivos.
        </span>
        {error && <span className="gitleaks-notice__error">{error}</span>}
      </div>
      <button
        type="button"
        className="gitleaks-notice__action"
        onClick={() => void configure()}
        disabled={isConfiguring || configured}
      >
        {isConfiguring ? "Configurando…" : configured ? "Configurado" : "Configurar"}
      </button>
    </div>
  );
}
