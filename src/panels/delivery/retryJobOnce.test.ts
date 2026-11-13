import { describe, it, expect, vi } from "vitest";

const client = vi.hoisted(() => ({ retryDeliveryJob: vi.fn() }));
vi.mock("../../delivery/client", () => client);

import { retryJobOnce } from "./retryJobOnce";

describe("retryJobOnce", () => {
  it("retries a job once while the call is in flight, and again once it settles", async () => {
    client.retryDeliveryJob.mockResolvedValueOnce({ id: "j2" });
    const [first, second] = await Promise.all([retryJobOnce("j1"), retryJobOnce("j1")]);
    expect(first).toEqual({ id: "j2" });
    expect(second).toBe(false);
    expect(client.retryDeliveryJob).toHaveBeenCalledTimes(1);
    // Settled: a later retry reaches Tinto, which says whether it may.
    client.retryDeliveryJob.mockRejectedValueOnce(new Error("ya hay un intento nuevo"));
    await expect(retryJobOnce("j1")).rejects.toThrow("ya hay un intento nuevo");
    client.retryDeliveryJob.mockResolvedValueOnce({ id: "j3" });
    await expect(retryJobOnce("j1")).resolves.toEqual({ id: "j3" });
  });
});
