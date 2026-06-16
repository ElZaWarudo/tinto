import { describe, it, expect, beforeEach } from "vitest";
import { clampZoom, handleZoomKey, zoomStore, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from "./zoom";

function key(k: string, mods: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return { key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods } as KeyboardEvent;
}

describe("zoom", () => {
  beforeEach(() => {
    localStorage.clear();
    zoomStore.set(1);
  });

  it("clampZoom bounds the range and rounds to 1 decimal", () => {
    expect(clampZoom(1.25)).toBe(1.3);
    expect(clampZoom(10)).toBe(ZOOM_MAX);
    expect(clampZoom(0.1)).toBe(ZOOM_MIN);
  });

  it("zoomIn/zoomOut/reset move the level, persist, and set the --file-zoom var", () => {
    zoomStore.zoomIn();
    expect(zoomStore.getZoom()).toBe(1 + ZOOM_STEP);
    expect(document.documentElement.style.getPropertyValue("--file-zoom")).toBe(
      String(1 + ZOOM_STEP),
    );
    expect(localStorage.getItem("tinto:zoom")).toBe(String(1 + ZOOM_STEP));

    zoomStore.reset();
    expect(zoomStore.getZoom()).toBe(1);

    zoomStore.zoomOut();
    expect(zoomStore.getZoom()).toBe(clampZoom(1 - ZOOM_STEP));
  });

  it("does not exceed the bounds on repeated steps", () => {
    for (let i = 0; i < 50; i++) zoomStore.zoomIn();
    expect(zoomStore.getZoom()).toBe(ZOOM_MAX);
    for (let i = 0; i < 50; i++) zoomStore.zoomOut();
    expect(zoomStore.getZoom()).toBe(ZOOM_MIN);
  });

  it("handleZoomKey responds to Ctrl/Cmd +/-/0 only", () => {
    expect(handleZoomKey(key("+", { ctrlKey: true }))).toBe(true);
    expect(zoomStore.getZoom()).toBe(1 + ZOOM_STEP);
    expect(handleZoomKey(key("=", { ctrlKey: true }))).toBe(true); // unshifted '+'
    expect(handleZoomKey(key("-", { metaKey: true }))).toBe(true);
    expect(handleZoomKey(key("0", { ctrlKey: true }))).toBe(true);
    expect(zoomStore.getZoom()).toBe(1);

    // Ignored: no modifier, alt held, or unrelated key.
    expect(handleZoomKey(key("+"))).toBe(false);
    expect(handleZoomKey(key("+", { ctrlKey: true, altKey: true }))).toBe(false);
    expect(handleZoomKey(key("a", { ctrlKey: true }))).toBe(false);
  });
});
