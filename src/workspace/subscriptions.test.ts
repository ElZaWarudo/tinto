import { describe, it, expect, beforeEach, vi } from "vitest";
import { SubscriptionReconciler } from "./subscriptions";
import type { SubscriptionTarget } from "../bus/contract";

// Let the microtask coalescer flush.
const tick = () => Promise.resolve();

function setup(cap?: number) {
  const pushed: SubscriptionTarget[][] = [];
  const push = vi.fn((t: SubscriptionTarget[]) => {
    pushed.push(t);
    return Promise.resolve();
  });
  const r = new SubscriptionReconciler(push, cap);
  return { r, push, pushed };
}

const keys = (targets: SubscriptionTarget[]) => targets.map((t) => `${t.repo} ${t.path}`).sort();

describe("SubscriptionReconciler", () => {
  let env: ReturnType<typeof setup>;
  beforeEach(() => {
    env = setup();
  });

  it("pushes the open set once after a coalesced burst (R6a)", async () => {
    env.r.add("/r/a", "x.ts");
    env.r.add("/r/a", "y.ts");
    expect(env.push).not.toHaveBeenCalled(); // coalesced, not yet flushed
    await tick();
    expect(env.push).toHaveBeenCalledTimes(1);
    expect(keys(env.pushed[0])).toEqual(["/r/a x.ts", "/r/a y.ts"]);
  });

  it("is idempotent: re-adding the same target does not push again", async () => {
    env.r.add("/r/a", "x.ts");
    await tick();
    expect(env.push).toHaveBeenCalledTimes(1);
    env.r.add("/r/a", "x.ts"); // same set
    await tick();
    expect(env.push).toHaveBeenCalledTimes(1); // no second push
  });

  it("coalesces add→remove→add to a single push (StrictMode churn)", async () => {
    env.r.add("/r/a", "x.ts");
    env.r.remove("/r/a", "x.ts");
    env.r.add("/r/a", "x.ts");
    await tick();
    expect(env.push).toHaveBeenCalledTimes(1);
    expect(keys(env.pushed[0])).toEqual(["/r/a x.ts"]);
  });

  it("bounds the global set to the cap, MRU-wins, across repos (R12/AE12)", async () => {
    const { r, pushed } = setup(8);
    // 5 in repo A + 4 in repo B = 9 targets; the 9th (oldest) is paused.
    for (let i = 0; i < 5; i++) r.add("/r/a", `a${i}.ts`);
    for (let i = 0; i < 4; i++) r.add("/r/b", `b${i}.ts`);
    await tick();
    expect(pushed[pushed.length - 1]).toHaveLength(8);
    expect(keys(pushed[pushed.length - 1])).toEqual([
      "/r/a a1.ts",
      "/r/a a2.ts",
      "/r/a a3.ts",
      "/r/a a4.ts",
      "/r/b b0.ts",
      "/r/b b1.ts",
      "/r/b b2.ts",
      "/r/b b3.ts",
    ]);
    // The first-opened target (/r/a a0.ts) is the one evicted (least recent).
    expect(r.isLive("/r/a", "a0.ts")).toBe(false);
    expect(r.isLive("/r/a", "a1.ts")).toBe(true);
    expect(r.isLive("/r/b", "b3.ts")).toBe(true);
  });

  it("a paused target becomes live when an earlier one closes", async () => {
    const { r } = setup(2);
    r.add("/r", "1");
    r.add("/r", "2");
    r.add("/r", "3"); // "1" is now paused (oldest beyond cap 2)
    await tick();
    expect(r.isLive("/r", "1")).toBe(false);
    r.remove("/r", "3");
    expect(r.isLive("/r", "1")).toBe(true); // back within the bound
  });

  it("remove of an unknown target does not push", async () => {
    env.r.remove("/r/a", "nope");
    await tick();
    expect(env.push).not.toHaveBeenCalled();
  });
});
