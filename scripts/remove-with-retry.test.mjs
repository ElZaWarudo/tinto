import { expect, test } from "vitest";

import { removeWithRetry } from "./remove-with-retry.mjs";

test("retries a transient Windows removal error", async () => {
  let removals = 0;
  let delays = 0;
  await removeWithRetry({
    attempts: 3,
    delay: async () => {
      delays += 1;
    },
    remove: () => {
      removals += 1;
      if (removals === 1) throw Object.assign(new Error("busy"), { code: "EBUSY" });
    },
  });
  expect(removals).toBe(2);
  expect(delays).toBe(1);
});

test("propagates non-retryable failures immediately", async () => {
  let removals = 0;
  const failure = Object.assign(new Error("denied"), { code: "EACCES" });
  await expect(
    removeWithRetry({
      attempts: 3,
      delay: async () => {},
      remove: () => {
        removals += 1;
        throw failure;
      },
    }),
  ).rejects.toBe(failure);
  expect(removals).toBe(1);
});

test("propagates the final retryable failure", async () => {
  let removals = 0;
  const failure = Object.assign(new Error("locked"), { code: "EPERM" });
  await expect(
    removeWithRetry({
      attempts: 2,
      delay: async () => {},
      remove: () => {
        removals += 1;
        throw failure;
      },
    }),
  ).rejects.toBe(failure);
  expect(removals).toBe(2);
});
