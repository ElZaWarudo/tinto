import { describe, it, expect, beforeEach } from "vitest";
import { tabsStore, visibleTabs } from "./tabsStore";

const R = "/r/a";

describe("tabsStore (preview/pin model)", () => {
  beforeEach(() => tabsStore.reset());

  it("starts empty: no files, no preview, overview active", () => {
    expect(tabsStore.getRepo(R)).toEqual({ files: [], preview: null, active: null });
  });

  it("previewFile uses one shared slot; previewing another reuses it", () => {
    tabsStore.previewFile(R, "a.ts");
    expect(tabsStore.getRepo(R)).toEqual({ files: [], preview: "a.ts", active: "a.ts" });
    tabsStore.previewFile(R, "b.ts"); // reuses the slot — a.ts is gone
    expect(tabsStore.getRepo(R)).toEqual({ files: [], preview: "b.ts", active: "b.ts" });
    expect(visibleTabs(tabsStore.getRepo(R))).toEqual(["b.ts"]);
  });

  it("pinFile promotes a file to a permanent tab and clears it from preview", () => {
    tabsStore.previewFile(R, "a.ts");
    tabsStore.pinFile(R, "a.ts");
    expect(tabsStore.getRepo(R)).toEqual({ files: ["a.ts"], preview: null, active: "a.ts" });
    // A new preview now coexists with the pinned tab.
    tabsStore.previewFile(R, "b.ts");
    expect(visibleTabs(tabsStore.getRepo(R))).toEqual(["a.ts", "b.ts"]);
  });

  it("previewing an already-pinned file just focuses it (no preview slot churn)", () => {
    tabsStore.pinFile(R, "a.ts");
    tabsStore.previewFile(R, "b.ts");
    tabsStore.previewFile(R, "a.ts"); // already pinned
    expect(tabsStore.getRepo(R)).toEqual({ files: ["a.ts"], preview: "b.ts", active: "a.ts" });
  });

  it("openFile routes to preview (pin=false) or pin (pin=true)", () => {
    tabsStore.openFile(R, "a.ts", false);
    expect(tabsStore.getRepo(R).preview).toBe("a.ts");
    tabsStore.openFile(R, "b.ts", true);
    expect(tabsStore.getRepo(R).files).toEqual(["b.ts"]);
  });

  it("setActive selects a pinned file, the preview, or the overview; ignores unknown", () => {
    tabsStore.pinFile(R, "a.ts");
    tabsStore.previewFile(R, "b.ts");
    tabsStore.setActive(R, null);
    expect(tabsStore.getRepo(R).active).toBeNull();
    tabsStore.setActive(R, "a.ts");
    expect(tabsStore.getRepo(R).active).toBe("a.ts");
    tabsStore.setActive(R, "b.ts"); // the preview is selectable
    expect(tabsStore.getRepo(R).active).toBe("b.ts");
    tabsStore.setActive(R, "ghost.ts"); // not open → no-op
    expect(tabsStore.getRepo(R).active).toBe("b.ts");
  });

  it("closeFile drops a pinned tab and moves active to a neighbour, then overview", () => {
    tabsStore.pinFile(R, "a.ts");
    tabsStore.pinFile(R, "b.ts");
    tabsStore.pinFile(R, "c.ts");
    tabsStore.setActive(R, "b.ts");
    tabsStore.closeFile(R, "b.ts"); // active b → falls to the tab now at its index (c)
    expect(tabsStore.getRepo(R)).toEqual({
      files: ["a.ts", "c.ts"],
      preview: null,
      active: "c.ts",
    });
    tabsStore.closeFile(R, "a.ts"); // not active → active unchanged
    expect(tabsStore.getRepo(R)).toEqual({ files: ["c.ts"], preview: null, active: "c.ts" });
    tabsStore.closeFile(R, "c.ts"); // last, was active → overview
    expect(tabsStore.getRepo(R)).toEqual({ files: [], preview: null, active: null });
  });

  it("closeFile can close the preview tab itself", () => {
    tabsStore.pinFile(R, "a.ts");
    tabsStore.previewFile(R, "b.ts");
    tabsStore.closeFile(R, "b.ts");
    expect(tabsStore.getRepo(R)).toEqual({ files: ["a.ts"], preview: null, active: "a.ts" });
  });

  it("closeRepo clears all tab state for the repo", () => {
    tabsStore.pinFile(R, "a.ts");
    tabsStore.pinFile("/r/b", "x.ts");
    tabsStore.closeRepo(R);
    expect(tabsStore.getRepo(R)).toEqual({ files: [], preview: null, active: null });
    expect(tabsStore.getRepo("/r/b").files).toEqual(["x.ts"]);
  });
});
