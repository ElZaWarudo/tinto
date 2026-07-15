import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_RUNTIME_PRESETS,
  createRuntimePresetId,
  loadRuntimePresets,
  runtimePresetMatches,
  saveRuntimePresets,
} from "./runtimePresets";

describe("runtime presets", () => {
  beforeEach(() => localStorage.clear());

  it("starts with useful built-in presets", () => {
    expect(loadRuntimePresets()).toEqual(DEFAULT_RUNTIME_PRESETS);
  });

  it("persists valid user edits and rejects malformed storage", () => {
    const edited = [{ ...DEFAULT_RUNTIME_PRESETS[0], name: "Mi diario" }];
    saveRuntimePresets(edited);
    expect(loadRuntimePresets()).toEqual(edited);

    localStorage.setItem("tinto:runtime-presets:v1", "{broken");
    expect(loadRuntimePresets()).toEqual(DEFAULT_RUNTIME_PRESETS);
  });

  it("upgrades stored presets with the previous accent shape", () => {
    localStorage.setItem(
      "tinto:runtime-presets:v1",
      JSON.stringify([
        {
          id: "legacy",
          name: "Anterior",
          model: "auto",
          reasoning: "auto",
          speed: "standard",
          accent: "violet",
        },
      ]),
    );

    expect(loadRuntimePresets()).toEqual([
      expect.objectContaining({ id: "legacy", icon: "spark", color: "#8b5cf6" }),
    ]);
  });

  it("matches all three runtime dimensions", () => {
    const preset = DEFAULT_RUNTIME_PRESETS[1];
    expect(runtimePresetMatches(preset, "auto", "high", "standard")).toBe(true);
    expect(runtimePresetMatches(preset, "auto", "medium", "standard")).toBe(false);
  });

  it("creates stable readable ids with a unique suffix", () => {
    vi.spyOn(Date, "now").mockReturnValue(1_760_000_000_000);
    expect(createRuntimePresetId("  Revisión rápida  ")).toMatch(/^revision-rapida-/);
  });
});
