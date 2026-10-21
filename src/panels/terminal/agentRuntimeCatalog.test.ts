import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentRuntimeCatalog } from "../../bus/contract";

const setDeliveryModelCatalog = vi.fn(() => Promise.resolve());
vi.mock("../../delivery/client", () => ({ setDeliveryModelCatalog }));

const { lastRuntimeCatalog, MUSE_CATALOG_SOURCE, rememberRuntimeCatalog } =
  await import("./agentRuntimeCatalog");

function catalog(source: string, defaultModel: string): AgentRuntimeCatalog {
  return {
    status: "ready",
    source,
    models: [
      {
        id: defaultModel,
        model: defaultModel,
        display_name: defaultModel,
        description: "",
        supported_reasoning_efforts: [{ value: "high", description: "" }],
        default_reasoning_effort: "high",
        is_default: true,
      },
    ],
    default_model: defaultModel,
    error: null,
    updated_at_ms: 1,
  };
}

describe("runtime catalogs", () => {
  beforeEach(() => {
    localStorage.clear();
    setDeliveryModelCatalog.mockClear();
  });

  it("keeps Muse Code's catalog apart from Codex's and tells Delivery each one", async () => {
    rememberRuntimeCatalog(catalog("codex_app_server", "gpt-6.1-sol"));
    rememberRuntimeCatalog(catalog(MUSE_CATALOG_SOURCE, "muse-spark-1.3"));
    await Promise.resolve();

    expect(lastRuntimeCatalog()?.default_model).toBe("gpt-6.1-sol");
    expect(lastRuntimeCatalog("muse")?.default_model).toBe("muse-spark-1.3");
    // Delivery checks the models stages and coordinators name against them.
    expect(setDeliveryModelCatalog).toHaveBeenCalledWith("codex", "gpt-6.1-sol", ["gpt-6.1-sol"]);
    expect(setDeliveryModelCatalog).toHaveBeenCalledWith("muse", "muse-spark-1.3", [
      "muse-spark-1.3",
    ]);
  });
});
