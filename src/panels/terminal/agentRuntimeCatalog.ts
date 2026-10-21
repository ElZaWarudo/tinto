import type { AgentRuntimeCatalog, AgentRuntimeModel } from "../../bus/contract";
import { setDeliveryModelCatalog } from "../../delivery/client";

export type CodexModelSelection = string;
export type CodexReasoningSelection = string;
export type CodexSpeedSelection = "standard" | "fast";

export function effectiveRuntimeModel(
  catalog: AgentRuntimeCatalog | null,
  selected: CodexModelSelection,
): AgentRuntimeModel | undefined {
  const models = catalog?.models ?? [];
  const id = selected === "auto" ? catalog?.default_model : selected;
  return models.find((model) => model.id === id || model.model === id);
}

export function runtimeFastTier(
  catalog: AgentRuntimeCatalog | null,
  selected: CodexModelSelection,
) {
  return effectiveRuntimeModel(catalog, selected)?.service_tiers?.find(
    (tier) => tier.id.toLocaleLowerCase() === "fast" || tier.name.toLocaleLowerCase() === "fast",
  );
}

export function reasoningSupportedByModel(
  catalog: AgentRuntimeCatalog | null,
  selectedModel: CodexModelSelection,
  reasoning: CodexReasoningSelection,
): boolean {
  if (reasoning === "auto" || catalog?.status !== "ready") return true;
  const model = effectiveRuntimeModel(catalog, selectedModel);
  if (!model) return true;
  return model.supported_reasoning_efforts.some((effort) => effort.value === reasoning);
}

export function speedSupportedByModel(
  catalog: AgentRuntimeCatalog | null,
  selectedModel: CodexModelSelection,
  speed: CodexSpeedSelection,
): boolean {
  if (speed === "standard" || catalog?.status !== "ready") return true;
  if (!effectiveRuntimeModel(catalog, selectedModel)) return true;
  return Boolean(runtimeFastTier(catalog, selectedModel));
}

export function codexModelLabel(
  catalog: AgentRuntimeCatalog | null,
  value: CodexModelSelection,
): string {
  if (value === "auto") return "Predeterminado";
  return effectiveRuntimeModel(catalog, value)?.display_name || value;
}

export function codexReasoningLabel(value: CodexReasoningSelection): string {
  const labels: Record<string, string> = {
    auto: "Predeterminado",
    minimal: "Mínimo",
    low: "Bajo",
    medium: "Medio",
    high: "Alto",
    xhigh: "Muy alto",
    max: "Máximo",
    ultra: "Ultra",
    none: "Ninguno",
  };
  return labels[value] ?? value;
}

export function codexSpeedLabel(value: CodexSpeedSelection): string {
  return value === "fast" ? "Rápido" : "Normal";
}

export type RuntimeCatalogProvider = "codex" | "muse";

/** The source Muse Code's `muse serve` runtime gives its catalog. */
export const MUSE_CATALOG_SOURCE = "muse_serve";

const LAST_CATALOG_KEYS: Record<RuntimeCatalogProvider, string> = {
  codex: "tinto.agents.lastRuntimeCatalog",
  muse: "tinto.agents.lastRuntimeCatalog.muse",
};

export function catalogProvider(catalog: AgentRuntimeCatalog): RuntimeCatalogProvider {
  return catalog.source === MUSE_CATALOG_SOURCE ? "muse" : "codex";
}

/**
 * Remembers each provider's last ready model catalog so archived sessions and Delivery can offer
 * the same choices, and tells Delivery about it.
 */
export function rememberRuntimeCatalog(catalog: AgentRuntimeCatalog): void {
  if (catalog.status !== "ready" || !catalog.models?.length) return;
  const provider = catalogProvider(catalog);
  try {
    localStorage.setItem(LAST_CATALOG_KEYS[provider], JSON.stringify(catalog));
  } catch {
    // Storage may be unavailable; the catalog is only a convenience here.
  }
  tellDelivery(provider, catalog);
}

/**
 * Tells Delivery a provider's catalog: the models its stages and coordinators may name, and the
 * default Codex jobs that name no model use.
 */
export function tellDelivery(provider: RuntimeCatalogProvider, catalog: AgentRuntimeCatalog): void {
  const models = (catalog.models ?? []).map((model) => model.model);
  Promise.resolve()
    .then(() => setDeliveryModelCatalog(provider, catalog.default_model ?? null, models))
    .catch(() => {});
}

export function lastRuntimeCatalog(
  provider: RuntimeCatalogProvider = "codex",
): AgentRuntimeCatalog | null {
  try {
    const stored = localStorage.getItem(LAST_CATALOG_KEYS[provider]);
    const catalog = stored ? (JSON.parse(stored) as AgentRuntimeCatalog) : null;
    return catalog?.status === "ready" && Array.isArray(catalog.models) ? catalog : null;
  } catch {
    return null;
  }
}
