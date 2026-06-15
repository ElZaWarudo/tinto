import { describe, it, expect, vi } from "vitest";
import { openDiffPanel } from "./openDiff";
import { diffPanelId } from "./panels";

// Covers AE9: opening the same (repo, path) focuses the existing panel, no dup.
function fakeApi() {
  const panels: Record<string, { api: { setActive: ReturnType<typeof vi.fn> } }> = {};
  return {
    addPanel: vi.fn((opts: { id: string }) => {
      panels[opts.id] = { api: { setActive: vi.fn() } };
      return panels[opts.id];
    }),
    getPanel: vi.fn((id: string) => panels[id]),
    _panels: panels,
  };
}

describe("openDiffPanel (dedup)", () => {
  it("adds a diff panel once and focuses it on the second open", () => {
    const api = fakeApi();
    openDiffPanel(api as never, "/r/api", "src/a.ts", "a.ts");
    expect(api.addPanel).toHaveBeenCalledTimes(1);
    const created = api._panels[diffPanelId("/r/api", "src/a.ts")];
    expect(created).toBeDefined();

    openDiffPanel(api as never, "/r/api", "src/a.ts", "a.ts");
    expect(api.addPanel).toHaveBeenCalledTimes(1); // no duplicate
    expect(created.api.setActive).toHaveBeenCalledOnce();
  });

  it("opens distinct panels for different files in the same repo", () => {
    const api = fakeApi();
    openDiffPanel(api as never, "/r/api", "src/a.ts", "a.ts");
    openDiffPanel(api as never, "/r/api", "src/b.ts", "b.ts");
    expect(api.addPanel).toHaveBeenCalledTimes(2);
  });
});
