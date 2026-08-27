import assert from "node:assert/strict";
import test from "node:test";

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
  assert.equal(removals, 2);
  assert.equal(delays, 1);
});

test("propagates non-retryable failures immediately", async () => {
  let removals = 0;
  const failure = Object.assign(new Error("denied"), { code: "EACCES" });
  await assert.rejects(
    removeWithRetry({
      attempts: 3,
      delay: async () => {},
      remove: () => {
        removals += 1;
        throw failure;
      },
    }),
    failure,
  );
  assert.equal(removals, 1);
});

test("propagates the final retryable failure", async () => {
  let removals = 0;
  const failure = Object.assign(new Error("locked"), { code: "EPERM" });
  await assert.rejects(
    removeWithRetry({
      attempts: 2,
      delay: async () => {},
      remove: () => {
        removals += 1;
        throw failure;
      },
    }),
    failure,
  );
  assert.equal(removals, 2);
});
