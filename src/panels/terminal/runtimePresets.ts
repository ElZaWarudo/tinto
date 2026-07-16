import type {
  CodexModelSelection,
  CodexReasoningSelection,
  CodexSpeedSelection,
} from "./agentRuntimeCatalog";

const STORAGE_KEY = "tinto:runtime-presets:v1";
const MAX_PRESETS = 24;

export type RuntimePresetIcon = "calendar" | "target" | "bolt" | "code" | "compass" | "spark";

export interface RuntimePreset {
  id: string;
  name: string;
  model: CodexModelSelection;
  reasoning: CodexReasoningSelection;
  speed: CodexSpeedSelection;
  icon: RuntimePresetIcon;
  color: string;
  favorite: boolean;
}

export const DEFAULT_RUNTIME_PRESETS: RuntimePreset[] = [
  {
    id: "everyday",
    name: "Diario",
    model: "auto",
    reasoning: "auto",
    speed: "standard",
    icon: "calendar",
    color: "#2dd4bf",
    favorite: true,
  },
  {
    id: "deep-work",
    name: "Trabajo profundo",
    model: "auto",
    reasoning: "high",
    speed: "standard",
    icon: "target",
    color: "#8b5cf6",
    favorite: false,
  },
  {
    id: "quick-tasks",
    name: "Tareas rápidas",
    model: "auto",
    reasoning: "medium",
    speed: "fast",
    icon: "bolt",
    color: "#f59e0b",
    favorite: false,
  },
];

export function loadRuntimePresets(): RuntimePreset[] {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return cloneDefaults();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return cloneDefaults();
    const presets = parsed.map(parseRuntimePreset).filter(isPresent).slice(0, MAX_PRESETS);
    return presets.length > 0 ? presets : cloneDefaults();
  } catch {
    return cloneDefaults();
  }
}

export function saveRuntimePresets(presets: RuntimePreset[]): RuntimePreset[] {
  const safe = normalizeFavorite(
    presets.map(parseRuntimePreset).filter(isPresent).slice(0, MAX_PRESETS),
  );
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(safe));
  } catch {
    // Presets remain usable for this session when storage is unavailable.
  }
  return safe;
}

export function loadFavoriteRuntimePreset(): RuntimePreset | null {
  return loadRuntimePresets().find((preset) => preset.favorite) ?? null;
}

export function runtimePresetMatches(
  preset: RuntimePreset,
  model: CodexModelSelection,
  reasoning: CodexReasoningSelection,
  speed: CodexSpeedSelection,
): boolean {
  return preset.model === model && preset.reasoning === reasoning && preset.speed === speed;
}

export function createRuntimePresetId(name: string): string {
  const slug = name
    .trim()
    .toLocaleLowerCase()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)/g, "")
    .slice(0, 24);
  const suffix = Date.now().toString(36);
  return `${slug || "preset"}-${suffix}`;
}

function cloneDefaults(): RuntimePreset[] {
  return DEFAULT_RUNTIME_PRESETS.map((preset) => ({ ...preset }));
}

function parseRuntimePreset(value: unknown): RuntimePreset | null {
  if (!value || typeof value !== "object") return null;
  const preset = value as Partial<RuntimePreset> & { accent?: string };
  if (
    !(
      typeof preset.id === "string" &&
      preset.id.length > 0 &&
      typeof preset.name === "string" &&
      preset.name.trim().length > 0 &&
      preset.name.length <= 40 &&
      typeof preset.model === "string" &&
      preset.model.length > 0 &&
      typeof preset.reasoning === "string" &&
      preset.reasoning.length > 0 &&
      (preset.speed === "standard" || preset.speed === "fast")
    )
  )
    return null;
  const icons: RuntimePresetIcon[] = ["calendar", "target", "bolt", "code", "compass", "spark"];
  const legacyColors: Record<string, string> = {
    mint: "#2dd4bf",
    violet: "#8b5cf6",
    amber: "#f59e0b",
    blue: "#3b82f6",
  };
  return {
    id: preset.id,
    name: preset.name.trim(),
    model: preset.model,
    reasoning: preset.reasoning,
    speed: preset.speed,
    icon: icons.includes(preset.icon as RuntimePresetIcon)
      ? (preset.icon as RuntimePresetIcon)
      : "spark",
    color:
      typeof preset.color === "string" && /^#[0-9a-f]{6}$/i.test(preset.color)
        ? preset.color.toLowerCase()
        : (legacyColors[preset.accent ?? ""] ?? "#3b82f6"),
    favorite: preset.favorite === true,
  };
}

function normalizeFavorite(presets: RuntimePreset[]): RuntimePreset[] {
  const favorite = presets.find((preset) => preset.favorite)?.id ?? null;
  return presets.map((preset) => ({ ...preset, favorite: preset.id === favorite }));
}

function isPresent<T>(value: T | null): value is T {
  return value !== null;
}
