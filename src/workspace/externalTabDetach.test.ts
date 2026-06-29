import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { armExternalTabDetach } from "./externalTabDetach";

describe("armExternalTabDetach", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", { value: 800, configurable: true });
    Object.defineProperty(window, "innerHeight", { value: 600, configurable: true });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("detaches when the active tab drag is released outside the viewport", () => {
    const detach = vi.fn();
    const start = new MouseEvent("dragstart", {
      clientX: 120,
      clientY: 40,
    }) as DragEvent;

    armExternalTabDetach(start, detach);
    document.dispatchEvent(
      new MouseEvent("mouseleave", {
        clientX: -4,
        clientY: 80,
        relatedTarget: null,
      }),
    );
    expect(detach).not.toHaveBeenCalled();

    window.dispatchEvent(
      new MouseEvent("dragend", {
        clientX: -4,
        clientY: 80,
      }),
    );

    expect(detach).toHaveBeenCalledOnce();
  });

  it("does not detach when the drag ends inside the viewport", () => {
    const detach = vi.fn();
    const start = new MouseEvent("dragstart", {
      clientX: 120,
      clientY: 40,
    }) as DragEvent;

    armExternalTabDetach(start, detach);
    window.dispatchEvent(
      new MouseEvent("dragend", {
        clientX: 180,
        clientY: 80,
      }),
    );

    expect(detach).not.toHaveBeenCalled();
  });
});
