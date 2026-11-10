import { describe, expect, it, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  applyLayout,
  safeParseLayout,
  isUsableLayout,
  layoutReferencesTree,
  loadUiState,
  saveUiState,
} from "./layout";
import type { DockviewApi, SerializedDockview } from "dockview-react";

// Loose test fixtures; the runtime helpers only read `.panels`.
const asLayout = (o: object) => o as unknown as SerializedDockview;

describe("layout persistence helpers", () => {
  beforeEach(() => invokeMock.mockReset());

  it("safeParseLayout returns null for null/empty/corrupt input", () => {
    expect(safeParseLayout(null)).toBeNull();
    expect(safeParseLayout("")).toBeNull();
    expect(safeParseLayout("{not json")).toBeNull();
    expect(safeParseLayout("123")).toBeNull(); // not an object
  });

  it("safeParseLayout returns the object for valid JSON", () => {
    const parsed = safeParseLayout('{"panels":{"dashboard":{}},"grid":{}}');
    expect(parsed).toEqual({ panels: { dashboard: {} }, grid: {} });
  });

  it("isUsableLayout requires a non-empty panels map", () => {
    expect(isUsableLayout(null)).toBe(false);
    expect(isUsableLayout(asLayout({}))).toBe(false);
    expect(isUsableLayout(asLayout({ panels: {} }))).toBe(false);
    expect(isUsableLayout(asLayout({ panels: { dashboard: {} } }))).toBe(true);
  });

  it("layoutReferencesTree detects the legacy in-dock repo tree panel", () => {
    expect(layoutReferencesTree(null)).toBe(false);
    expect(layoutReferencesTree(asLayout({ panels: { dashboard: {} } }))).toBe(false);
    expect(
      layoutReferencesTree(asLayout({ panels: { dashboard: { contentComponent: "dashboard" } } })),
    ).toBe(false);
    expect(layoutReferencesTree(asLayout({ panels: { tree: { contentComponent: "tree" } } }))).toBe(
      true,
    );
  });

  it("applyLayout keeps the saved layout and drops only the agent consoles", () => {
    const layout = asLayout({
      panels: {
        dashboard: { contentComponent: "dashboard" },
        "repo:/r/a": { contentComponent: "repo" },
        "agent-consoles": { contentComponent: "agent-consoles" },
        "agent-terminal:sess-1": { contentComponent: "agent-terminal" },
      },
    });
    let open: string[] = [];
    const api = {
      get panels() {
        return open.map((id) => ({ id }));
      },
      fromJSON: () => {
        open = Object.keys(layout.panels);
      },
      getPanel: (id: string) => (open.includes(id) ? { id } : undefined),
      removePanel: (panel: { id: string }) => {
        open = open.filter((id) => id !== panel.id);
      },
      clear: vi.fn(),
      addPanel: vi.fn(),
    } as unknown as DockviewApi;

    applyLayout(api, layout);

    expect(open).toEqual(["dashboard", "repo:/r/a"]);
    expect(api.addPanel).not.toHaveBeenCalled();
  });

  it("loadUiState parses the backend string, tolerating errors", async () => {
    invokeMock.mockResolvedValueOnce('{"panels":{"dashboard":{}}}');
    expect(await loadUiState()).toEqual({ panels: { dashboard: {} } });

    invokeMock.mockResolvedValueOnce(null);
    expect(await loadUiState()).toBeNull();

    invokeMock.mockRejectedValueOnce(new Error("boom"));
    expect(await loadUiState()).toBeNull();
  });

  it("saveUiState invokes set_ui_state with a JSON string and swallows errors", async () => {
    invokeMock.mockResolvedValueOnce(undefined);
    await saveUiState(asLayout({ panels: { dashboard: {} } }));
    expect(invokeMock).toHaveBeenCalledWith("set_ui_state", {
      state: '{"panels":{"dashboard":{}}}',
    });

    invokeMock.mockRejectedValueOnce(new Error("disk full"));
    await expect(saveUiState(asLayout({ panels: {} }))).resolves.toBeUndefined(); // no throw
  });
});
