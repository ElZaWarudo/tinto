import { describe, expect, it } from "vitest";
import {
  detachedPanelLabel,
  detachedPanelTitle,
  detachedPanelUrl,
  readDetachedPanel,
} from "./detachPanelWindow";

describe("detached panel windows", () => {
  const file = {
    kind: "file" as const,
    repo: "C:\\work\\agentos",
    path: "documentation/plugins.md",
  };

  it("carries the panel in its URL", () => {
    expect(readDetachedPanel(detachedPanelUrl(file).slice(1))).toEqual(file);
    const delivery = { kind: "delivery" as const, repo: "/r/tinto" };
    expect(readDetachedPanel(detachedPanelUrl(delivery).slice(1))).toEqual(delivery);
    expect(readDetachedPanel("?tintoDetachedPanel=not-json")).toBeNull();
    expect(readDetachedPanel('?tintoDetachedPanel={"kind":"file","repo":"/r"}')).toBeNull();
    expect(readDetachedPanel("")).toBeNull();
  });

  it("gives the same panel the same window label, made of label-safe characters", () => {
    const label = detachedPanelLabel(file);
    expect(label).toBe(detachedPanelLabel({ ...file }));
    expect(label).not.toBe(detachedPanelLabel({ ...file, path: "documentation/sandbox-fs.md" }));
    expect(label).toMatch(/^panel-file-[0-9a-f]+$/);
  });

  it("names the window after the file and its repo", () => {
    expect(detachedPanelTitle(file)).toBe("plugins.md · agentos");
    expect(detachedPanelTitle({ kind: "delivery", repo: "/r/tinto" })).toBe("Delivery · tinto");
  });
});
