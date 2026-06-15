import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// dockview cannot render in jsdom; capture the onReady callback and drive a
// fake api so we can test the shell's restore/guard/save wiring directly.
let capturedOnReady: ((e: { api: FakeApi }) => void) | null = null;
vi.mock("dockview-react", () => ({
  DockviewReact: (props: { onReady: (e: { api: FakeApi }) => void }) => {
    capturedOnReady = props.onReady;
    return null;
  },
  themeVisualStudio: {},
}));

import { DockWorkspace } from "./DockWorkspace";
import { PANEL_DASHBOARD, PANEL_TREE } from "./panels";

interface FakeApi {
  panels: { id: string }[];
  clear: ReturnType<typeof vi.fn>;
  addPanel: ReturnType<typeof vi.fn>;
  fromJSON: ReturnType<typeof vi.fn>;
  toJSON: ReturnType<typeof vi.fn>;
  onDidLayoutChange: ReturnType<typeof vi.fn>;
  onDidRemovePanel: ReturnType<typeof vi.fn>;
  fireLayoutChange: () => void;
  fireRemovePanel: () => void;
}

function makeFakeApi(): FakeApi {
  const layoutCbs: Array<() => void> = [];
  const removeCbs: Array<() => void> = [];
  const api: FakeApi = {
    panels: [],
    clear: vi.fn(() => {
      api.panels = [];
    }),
    addPanel: vi.fn((opts: { id: string }) => {
      api.panels.push({ id: opts.id });
    }),
    fromJSON: vi.fn(),
    toJSON: vi.fn(() => ({ panels: { dashboard: {} } })),
    onDidLayoutChange: vi.fn((cb: () => void) => {
      layoutCbs.push(cb);
      return { dispose() {} };
    }),
    onDidRemovePanel: vi.fn((cb: () => void) => {
      removeCbs.push(cb);
      return { dispose() {} };
    }),
    fireLayoutChange: () => layoutCbs.forEach((cb) => cb()),
    fireRemovePanel: () => removeCbs.forEach((cb) => cb()),
  };
  return api;
}

const dummy = () => null;
const components = { [PANEL_DASHBOARD]: dummy, [PANEL_TREE]: dummy };

describe("DockWorkspace", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    capturedOnReady = null;
  });
  afterEach(() => vi.useRealTimers());

  it("builds the default layout when there is no persisted state", async () => {
    invokeMock.mockResolvedValue(null); // get_ui_state -> null
    render(<DockWorkspace components={components} />);
    const api = makeFakeApi();
    capturedOnReady!({ api });

    await waitFor(() => expect(api.addPanel).toHaveBeenCalled());
    const addedIds = api.addPanel.mock.calls.map((c) => c[0].id);
    expect(addedIds).toContain(PANEL_DASHBOARD);
    expect(addedIds).toContain(PANEL_TREE);
    expect(api.fromJSON).not.toHaveBeenCalled();
  });

  it("restores a usable persisted layout instead of the default", async () => {
    invokeMock.mockResolvedValue('{"panels":{"dashboard":{}},"grid":{}}');
    render(<DockWorkspace components={components} />);
    const api = makeFakeApi();
    capturedOnReady!({ api });

    await waitFor(() => expect(api.fromJSON).toHaveBeenCalled());
    expect(api.addPanel).not.toHaveBeenCalled();
  });

  it("guards against an empty workspace by reopening the Dashboard", async () => {
    invokeMock.mockResolvedValue(null);
    render(<DockWorkspace components={components} />);
    const api = makeFakeApi();
    capturedOnReady!({ api });
    await waitFor(() => expect(api.onDidRemovePanel).toHaveBeenCalled());

    api.addPanel.mockClear();
    api.panels = []; // simulate all panels removed
    api.fireRemovePanel();
    expect(api.addPanel).toHaveBeenCalledWith(expect.objectContaining({ id: PANEL_DASHBOARD }));
  });

  it("debounce-saves the layout on change", async () => {
    vi.useFakeTimers();
    invokeMock.mockResolvedValue(null);
    render(<DockWorkspace components={components} />);
    const api = makeFakeApi();
    capturedOnReady!({ api });
    // let the async restore + listener attach run
    await vi.waitFor(() => expect(api.onDidLayoutChange).toHaveBeenCalled());

    invokeMock.mockClear();
    api.fireLayoutChange();
    vi.advanceTimersByTime(500);
    expect(invokeMock).toHaveBeenCalledWith(
      "set_ui_state",
      expect.objectContaining({ state: expect.any(String) }),
    );
  });
});
