import type { AgentRuntimeCatalog, AgentRuntimeModel } from "../../bus/contract";

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
  };
  return labels[value] ?? value;
}

export function codexSpeedLabel(value: CodexSpeedSelection): string {
  return value === "fast" ? "Rápido" : "Normal";
}
