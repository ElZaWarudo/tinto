import { useState } from "react";
import { createRepoAgentsMdConfig, createRepoGitleaksConfig } from "../bus/client";
import type { SecretScanStatus } from "../bus/contract";
import { SecretScanIndicator } from "./SecretScanIndicator";

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
  configured,
  configureLabel,
  configuredLabel = "Configurado",
  onConfigure,
}: {
  title: string;
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
      setError(commandMessage(cause, "No se pudo aplicar la configuración."));
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
        {error && (
          <span className="repo-config-section__error" role="alert">
            Error: {error}
          </span>
        )}
      </div>
      {isConfigured ? (
        <span className="repo-config-section__status" role="status" aria-live="polite">
          {configuredLabel}
        </span>
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
  secretScanStatus,
  secretFindings,
}: {
  repo: string;
  gitleaksConfigured: boolean;
  agentsMdConfigured: boolean;
  secretScanStatus: SecretScanStatus;
  secretFindings: number;
}) {
  return (
    <section className="repo-config-section" data-testid="repo-config-section">
      <div className="repo-config-section__head">
        <h3>Configuración del repo</h3>
      </div>
      <SecretScanIndicator status={secretScanStatus} findings={secretFindings} />
      <RepoConfigItem
        key={`gitleaks-${repo}`}
        title="Gitleaks"
        configured={gitleaksConfigured}
        configureLabel="Configurar"
        onConfigure={() => createRepoGitleaksConfig(repo)}
      />
      <RepoConfigItem
        key={`agents-md-${repo}`}
        title="AGENTS.md"
        configured={agentsMdConfigured}
        configureLabel="Configurar"
        onConfigure={() => createRepoAgentsMdConfig(repo)}
      />
    </section>
  );
}
