import { useState } from "react";
import type { ReactNode } from "react";
import { createRepoAgentsMdConfig, createRepoGitleaksConfig } from "../bus/client";

function commandMessage(error: unknown, fallback: string): string {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }
  return fallback;
}

function RepoConfigItem({
  title,
  detail,
  configured,
  configureLabel,
  configuredLabel = "Configurado",
  onConfigure,
}: {
  title: string;
  detail: ReactNode;
  configured: boolean;
  configureLabel: string;
  configuredLabel?: string;
  onConfigure: () => Promise<unknown>;
}) {
  const [isConfiguring, setIsConfiguring] = useState(false);
  const [configuredLocally, setConfiguredLocally] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isConfigured = configured || configuredLocally;

  const configure = async () => {
    if (isConfiguring || isConfigured) return;
    setIsConfiguring(true);
    setError(null);
    try {
      await onConfigure();
      setConfiguredLocally(true);
    } catch (cause) {
      setError(commandMessage(cause, "No se pudo aplicar la configuracion."));
    } finally {
      setIsConfiguring(false);
    }
  };

  return (
    <div
      className={
        isConfigured
          ? "repo-config-section__item repo-config-section__item--configured"
          : "repo-config-section__item"
      }
    >
      <div className="repo-config-section__body">
        <strong>{title}</strong>
        {(!isConfigured || error) && <span>{detail}</span>}
        {error && <span className="repo-config-section__error">{error}</span>}
      </div>
      {isConfigured ? (
        <span className="repo-config-section__status">{configuredLabel}</span>
      ) : (
        <button
          type="button"
          className="repo-config-section__action"
          onClick={() => void configure()}
          disabled={isConfiguring}
        >
          {isConfiguring ? "Configurando..." : configureLabel}
        </button>
      )}
    </div>
  );
}

export function RepoConfigSection({
  repo,
  gitleaksConfigured,
  agentsMdConfigured,
}: {
  repo: string;
  gitleaksConfigured: boolean;
  agentsMdConfigured: boolean;
}) {
  return (
    <section className="repo-config-section" data-testid="repo-config-section">
      <div className="repo-config-section__head">
        <h3>Repo configuration</h3>
      </div>
      <RepoConfigItem
        key={`gitleaks-${repo}`}
        title="Gitleaks"
        detail={
          <>
            Crea <code>.gitleaks.toml</code> para ajustar falsos positivos del escaneo local.
          </>
        }
        configured={gitleaksConfigured}
        configureLabel="Configurar"
        onConfigure={() => createRepoGitleaksConfig(repo)}
      />
      <RepoConfigItem
        key={`agents-md-${repo}`}
        title="AGENTS.md"
        detail={
          <>
            Anade la seccion IADE para que los agentes notifiquen a Tinto cuando termina un turno.
          </>
        }
        configured={agentsMdConfigured}
        configureLabel="Configurar"
        onConfigure={() => createRepoAgentsMdConfig(repo)}
      />
    </section>
  );
}
