import { describe, it, expect, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import type { DeliveryJob, DeliveryJobLog } from "../../delivery/types";

const client = vi.hoisted(() => ({ getDeliveryJobLog: vi.fn() }));
vi.mock("../../delivery/client", () => client);

import { useJobLog } from "./useJobLog";

describe("useJobLog", () => {
  it("adds a read overtaken by another from the same line only once", async () => {
    vi.useFakeTimers();
    const reads: ((log: DeliveryJobLog) => void)[] = [];
    client.getDeliveryJobLog.mockImplementation(
      () => new Promise<DeliveryJobLog>((resolve) => reads.push(resolve)),
    );
    const job = { id: "j1", status: "started" } as DeliveryJob;
    const { result } = renderHook(() => useJobLog(job));
    // The first read takes longer than the next poll.
    act(() => void vi.advanceTimersByTime(1500));
    expect(client.getDeliveryJobLog.mock.calls).toEqual([
      ["j1", 0],
      ["j1", 0],
    ]);
    const chunk = { entries: [{ kind: "info", text: "npm test" }], next_line: 1, stderr_tail: "" };
    await act(async () => reads.forEach((resolve) => resolve(chunk)));
    expect(result.current?.entries).toEqual(chunk.entries);
    vi.useRealTimers();
  });
});
