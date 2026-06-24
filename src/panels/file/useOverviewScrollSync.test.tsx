import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createRef } from "react";
import { useOverviewScrollSync } from "./useOverviewScrollSync";

function makeBody(rows: { line: number; top: number }[]): {
  ref: React.RefObject<HTMLDivElement | null>;
  node: HTMLDivElement;
  setTops: (tops: number[]) => void;
  fireScroll: () => void;
} {
  const ref = createRef<HTMLDivElement>();
  const node = document.createElement("div");
  let currentTops = rows.map((row) => row.top);
  rows.forEach((row, index) => {
    const el = document.createElement("div");
    el.setAttribute("data-line", String(index + 1));
    Object.defineProperty(el, "getBoundingClientRect", {
      configurable: true,
      get: () => () => ({
        top: currentTops[index] ?? row.top,
        bottom: (currentTops[index] ?? row.top) + 18,
        left: 0,
        right: 0,
        width: 100,
        height: 18,
        x: 0,
        y: currentTops[index] ?? row.top,
        toJSON: () => "",
      }),
    });
    node.appendChild(el);
  });
  Object.defineProperty(node, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      top: 0,
      bottom: 200,
      left: 0,
      right: 0,
      width: 100,
      height: 200,
      x: 0,
      y: 0,
      toJSON: () => "",
    }),
  });
  Object.defineProperty(node, "clientHeight", {
    configurable: true,
    value: 200,
  });
  Object.defineProperty(node, "scrollHeight", {
    configurable: true,
    value: 500,
  });
  document.body.appendChild(node);
  (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
  return {
    ref,
    node,
    setTops: (tops: number[]) => {
      currentTops = tops;
    },
    fireScroll: () => node.dispatchEvent(new Event("scroll")),
  };
}

describe("useOverviewScrollSync", () => {
  let raf: (cb: FrameRequestCallback) => number;
  let caf: (id: number) => void;

  beforeEach(() => {
    raf = globalThis.requestAnimationFrame;
    caf = globalThis.cancelAnimationFrame;
    let nextId = 1;
    const pending = new Map<number, FrameRequestCallback>();
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      const id = nextId++;
      pending.set(id, cb);
      return id;
    }) as typeof globalThis.requestAnimationFrame;
    globalThis.cancelAnimationFrame = ((id: number) => {
      pending.delete(id);
    }) as typeof globalThis.cancelAnimationFrame;
    (globalThis as unknown as { __flushRaf: () => void }).__flushRaf = () => {
      const callbacks = Array.from(pending.entries());
      pending.clear();
      for (const [, callback] of callbacks) callback(performance.now());
    };
  });

  afterEach(() => {
    globalThis.requestAnimationFrame = raf;
    globalThis.cancelAnimationFrame = caf;
    document.body.innerHTML = "";
  });

  function flush() {
    (globalThis as unknown as { __flushRaf: () => void }).__flushRaf();
  }

  it("returns 1 when totalLines is zero", () => {
    const { ref } = makeBody([]);
    const { result } = renderHook(() => useOverviewScrollSync(ref, 0));
    act(() => {
      flush();
    });
    expect(result.current.topLine).toBe(1);
  });

  it("reports the first visible line on scroll", () => {
    const { ref, fireScroll, setTops } = makeBody([
      { line: 1, top: 0 },
      { line: 2, top: 18 },
      { line: 3, top: 36 },
      { line: 4, top: 54 },
      { line: 5, top: 72 },
    ]);
    const { result } = renderHook(() => useOverviewScrollSync(ref, 5));
    act(() => {
      flush();
    });
    expect(result.current.topLine).toBe(1);
    expect(result.current.visibleLineCount).toBe(5);
    expect(result.current.viewportHeight).toBe(200);
    expect(result.current.scrollProgress).toBe(0);
    setTops([-100, -50, 0, 50, 100]);
    act(() => {
      fireScroll();
      flush();
    });
    expect(result.current.topLine).toBe(3);
    expect(result.current.visibleLineCount).toBe(3);
  });

  it("ignores overview ruler descendants when finding the first visible line", () => {
    const { ref, node } = makeBody([
      { line: 1, top: 0 },
      { line: 2, top: 18 },
      { line: 3, top: 36 },
    ]);
    const ruler = document.createElement("div");
    ruler.className = "file-overview-ruler";
    const marker = document.createElement("button");
    marker.setAttribute("data-line", "99");
    Object.defineProperty(marker, "getBoundingClientRect", {
      configurable: true,
      value: () => ({
        top: 0,
        bottom: 18,
        left: 0,
        right: 0,
        width: 24,
        height: 18,
        x: 0,
        y: 0,
        toJSON: () => "",
      }),
    });
    ruler.appendChild(marker);
    node.prepend(ruler);

    const { result } = renderHook(() => useOverviewScrollSync(ref, 3));
    act(() => {
      flush();
    });
    expect(result.current.topLine).toBe(1);
    expect(result.current.visibleLineCount).toBe(3);
  });
});
