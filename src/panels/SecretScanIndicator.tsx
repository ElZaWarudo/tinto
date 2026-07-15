import type { SecretScanStatus } from "../bus/contract";

function indicatorCopy(status: SecretScanStatus, findings: number) {
  switch (status.state) {
    case "clean":
      return { label: "Gitleaks limpio", detail: status.version ?? null };
    case "findings":
      return {
        label: `${findings} ${findings === 1 ? "secreto posible" : "secretos posibles"}`,
        detail: "Gitleaks",
      };
    case "degraded":
      return {
        label:
          status.failure_category === "binary_unavailable"
            ? "Gitleaks no disponible"
            : "Gitleaks falló",
        detail: "Detector básico activo",
      };
    default:
      return { label: "Sin cambios por analizar", detail: null };
  }
}

export function SecretScanIndicator({
  status,
  findings = 0,
  compact = false,
}: {
  status: SecretScanStatus;
  findings?: number;
  compact?: boolean;
}) {
  const copy = indicatorCopy(status, findings);
  return (
    <div
      className={`secret-scan-indicator secret-scan-indicator--${status.state}${compact ? " secret-scan-indicator--compact" : ""}`}
      data-testid="secret-scan-status"
      role="status"
      aria-live="polite"
      title={status.message ?? undefined}
    >
      <span className="secret-scan-indicator__mark" aria-hidden="true" />
      <strong>{copy.label}</strong>
      {copy.detail && <span>{copy.detail}</span>}
    </div>
  );
}
