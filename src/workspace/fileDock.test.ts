import { describe, it, expect, vi, beforeEach } from "vitest";
import { fileDock, FILE_PREVIEW_ID, filePanelId } from "./fileDock";

const REPO = "/r/api";

// Minimal fake of the bits of DockviewApi that fileDock drives.
function makeFakeApi() {
  const addCbs: Array<(p: unknown) => void> = [];
  const removeCbs: Array<(p: { id: string }) => void> = [];
  const activeCbs: Array<() => void> = [];
  const layoutCbs: Array<() => void> = [];
  const panels: Array<{
    id: string;
    params: Record<string, unknown>;
    api: Record<string, ReturnType<typeof vi.fn>>;
  }> = [];
  let active: (typeof panels)[number] | null = null;

  const api = {
    panels,
    get activePanel() {
      return active;
    },
    getPanel: (id: string) => panels.find((p) => p.id === id),
    addPanel: vi.fn(
      (opts: {
        id: string;
        params?: Record<string, unknown>;
        position?: { direction: "right"; referencePanel: string };
      }) => {
      const panel = {
        id: opts.id,
        params: { ...(opts.params ?? {}) },
        api: {
          setActive: vi.fn(() => {
            active = panel;
            activeCbs.forEach((cb) => cb());
          }),
          close: vi.fn(() => {
            const i = panels.indexOf(panel);
            if (i >= 0) panels.splice(i, 1);
            if (active === panel) active = panels[panels.length - 1] ?? null;
            removeCbs.forEach((cb) => cb({ id: panel.id }));
          }),
          updateParameters: vi.fn((p: Record<string, unknown>) => {
            panel.params = { ...panel.params, ...p };
          }),
          setTitle: vi.fn(),
        },
      };
      panels.push(panel);
      active = panel;
      addCbs.forEach((cb) => cb(panel));
      activeCbs.forEach((cb) => cb());
      return panel;
      },
    ),
    onDidAddPanel: (cb: (p: unknown) => void) => (addCbs.push(cb), { dispose() {} }),
    onDidRemovePanel: (cb: (p: { id: string }) => void) => (removeCbs.push(cb), { dispose() {} }),
    onDidActivePanelChange: (cb: () => void) => (activeCbs.push(cb), { dispose() {} }),
    onDidLayoutChange: (cb: () => void) => (layoutCbs.push(cb), { dispose() {} }),
    toJSON: () => ({ panels: Object.fromEntries(panels.map((p) => [p.id, { params: p.params }])) }),
    fromJSON: vi.fn(),
  };
  return api;
}

describe("fileDock", () => {
  beforeEach(() => fileDock.drop(REPO));

  it("queues opens before the dock is registered, then drains them on register", () => {
    fileDock.openFile(REPO, "a.ts", false); // queued (no api yet)
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    expect(api.getPanel(FILE_PREVIEW_ID)).toBeTruthy();
    expect(fileDock.getState(REPO).preview).toBe("a.ts");
  });

  it("reuses a single preview slot across single clicks", () => {
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    fileDock.openFile(REPO, "a.ts", false);
    fileDock.openFile(REPO, "b.ts", false);
    expect(api.panels.filter((p) => p.id === FILE_PREVIEW_ID)).toHaveLength(1);
    expect(fileDock.getState(REPO).preview).toBe("b.ts");
    expect(fileDock.getState(REPO).active).toBe("b.ts");
  });

  it("pins a file as its own panel and clears the preview when promoting it", () => {
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    fileDock.openFile(REPO, "a.ts", false); // preview
    fileDock.openFile(REPO, "a.ts", true); // pin (promote)
    expect(api.getPanel(filePanelId("a.ts"))).toBeTruthy();
    expect(api.getPanel(FILE_PREVIEW_ID)).toBeFalsy();
    expect(fileDock.getState(REPO).preview).toBeNull();
    expect(fileDock.getState(REPO).open).toContain("a.ts");
  });

  it("focuses an already-open pinned file instead of duplicating it", () => {
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    fileDock.openFile(REPO, "a.ts", true);
    const panel = api.getPanel(filePanelId("a.ts"))!;
    fileDock.openFile(REPO, "a.ts", false); // single-click the open file
    expect(api.panels.filter((p) => p.id === filePanelId("a.ts"))).toHaveLength(1);
    expect(panel.api.setActive).toHaveBeenCalled();
  });

  it("opens the next pinned file as a right split of the active file", () => {
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    fileDock.openFile(REPO, "a.ts", true);
    fileDock.openFile(REPO, "b.ts", true);
    expect(fileDock.getState(REPO).open).toEqual(["a.ts", "b.ts"]);
    expect(api.addPanel).toHaveBeenLastCalledWith(
      expect.objectContaining({
        id: filePanelId("b.ts"),
        position: {
          direction: "right",
          referencePanel: filePanelId("a.ts"),
        },
      }),
    );
  });

  it("persists the layout on unmount and restores it on the next mount", () => {
    const api1 = makeFakeApi();
    fileDock.register(REPO, api1 as never);
    fileDock.openFile(REPO, "a.ts", true);
    fileDock.openFile(REPO, "b.ts", true);
    fileDock.unregister(REPO); // flushes a save
    expect(localStorage.getItem(`tinto:filedock:${REPO}`)).toBeTruthy();

    const api2 = makeFakeApi();
    fileDock.register(REPO, api2 as never);
    expect(api2.fromJSON).toHaveBeenCalled(); // restored from the saved layout
  });

  it("drop forgets the persisted layout", () => {
    const api = makeFakeApi();
    fileDock.register(REPO, api as never);
    fileDock.openFile(REPO, "a.ts", true);
    fileDock.unregister(REPO);
    expect(localStorage.getItem(`tinto:filedock:${REPO}`)).toBeTruthy();
    fileDock.drop(REPO);
    expect(localStorage.getItem(`tinto:filedock:${REPO}`)).toBeNull();
  });
});
