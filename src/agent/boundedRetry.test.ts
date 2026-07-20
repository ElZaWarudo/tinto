import { describe, expect, it, vi } from "vitest";
import { retryAgentRecoveryOperation } from "./boundedRetry";

describe("retryAgentRecoveryOperation", () => {
  it("recovers aggressively with bounded incremental waits", async () => {
    const operation = vi
      .fn<() => Promise<string>>()
      .mockRejectedValueOnce(new Error("host unavailable"))
      .mockRejectedValueOnce({ category: "session_not_running", message: "starting" })
      .mockResolvedValueOnce("ready");
    const waits: number[] = [];
    const retries: Array<[number, number]> = [];

    await expect(
      retryAgentRecoveryOperation(operation, {
        onRetry: (attempt, maxAttempts) => retries.push([attempt, maxAttempts]),
        wait: async (delayMs) => {
          waits.push(delayMs);
        },
      }),
    ).resolves.toBe("ready");

    expect(operation).toHaveBeenCalledTimes(3);
    expect(waits).toEqual([120, 300]);
    expect(retries).toEqual([
      [2, 5],
      [3, 5],
    ]);
  });

  it("stops after five attempts instead of scheduling an endless loop", async () => {
    const failure = { category: "session_not_running", message: "still starting" };
    const operation = vi.fn<() => Promise<void>>().mockRejectedValue(failure);
    const wait = vi.fn<(delayMs: number) => Promise<void>>().mockResolvedValue(undefined);

    await expect(retryAgentRecoveryOperation(operation, { wait })).rejects.toBe(failure);

    expect(operation).toHaveBeenCalledTimes(5);
    expect(wait).toHaveBeenCalledTimes(4);
    expect(wait.mock.calls.map(([delayMs]) => delayMs)).toEqual([120, 300, 700, 1_400]);
  });

  it("does not retry errors that require user action", async () => {
    const failure = { category: "session_not_found", message: "gone" };
    const operation = vi.fn<() => Promise<void>>().mockRejectedValue(failure);
    const wait = vi.fn<(delayMs: number) => Promise<void>>();

    await expect(retryAgentRecoveryOperation(operation, { wait })).rejects.toBe(failure);

    expect(operation).toHaveBeenCalledOnce();
    expect(wait).not.toHaveBeenCalled();
  });
});
