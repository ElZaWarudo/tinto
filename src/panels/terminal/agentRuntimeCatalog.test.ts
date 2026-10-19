import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentRuntimeCatalog } from "../../bus/contract";

const setDeliveryCodexModel = vi.fn(() => Promise.resolve());
vi.mock("../../delivery/client", () => ({ setDeliveryCodexModel }));

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
    setDeliveryCodexModel.mockClear();
  });

  it("keeps Muse Code's catalog apart from Codex's", async () => {
    rememberRuntimeCatalog(catalog("codex_app_server", "gpt-6.1-sol"));
    rememberRuntimeCatalog(catalog(MUSE_CATALOG_SOURCE, "muse-spark-1.3"));
    await Promise.resolve();

    expect(lastRuntimeCatalog()?.default_model).toBe("gpt-6.1-sol");
    expect(lastRuntimeCatalog("muse")?.default_model).toBe("muse-spark-1.3");
    // Only Codex's default becomes Delivery's default Codex model.
    expect(setDeliveryCodexModel).toHaveBeenCalledTimes(1);
    expect(setDeliveryCodexModel).toHaveBeenCalledWith("gpt-6.1-sol");
  });
});
