import { describe, it, expect, vi } from "vitest";

const client = vi.hoisted(() => ({ retryDeliveryJob: vi.fn() }));
vi.mock("../../delivery/client", () => client);

import { retryJobOnce } from "./retryJobOnce";

describe("retryJobOnce", () => {
  it("retries a job once from the whole window, and again only after a failed try", async () => {
    client.retryDeliveryJob.mockResolvedValueOnce({ id: "j2" });
    const [first, second] = await Promise.all([retryJobOnce("j1"), retryJobOnce("j1")]);
    expect(first).toEqual({ id: "j2" });
    expect(second).toBe(false);
    expect(client.retryDeliveryJob).toHaveBeenCalledTimes(1);

    client.retryDeliveryJob.mockRejectedValueOnce(new Error("busy"));
    await expect(retryJobOnce("j5")).rejects.toThrow("busy");
    client.retryDeliveryJob.mockResolvedValueOnce({ id: "j6" });
    await expect(retryJobOnce("j5")).resolves.toEqual({ id: "j6" });
  });
});
