import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, act } from "@testing-library/react";

// Avoid rendering dockview / hitting Tauri in jsdom.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./bus/connection", () => ({
  useBusConnection: () => {},
  reloadActiveWorkbench: vi.fn(),
}));

const captured = vi.hoisted(() => ({
  components: undefined as Record<string, unknown> | undefined,
}));
vi.mock("./workspace/DockWorkspace", () => ({
  DockWorkspace: (props: { components: Record<string, unknown> }) => {
    captured.components = props.components;
    return <div data-testid="workspace-stub" />;
  },
}));

import App from "./App";
import { busStore } from "./bus/store";
import { PANEL_DASHBOARD, PANEL_REPO, PANEL_TREE } from "./workspace/panels";
import type { WorkbenchConfig } from "./bus/contract";

describe("App", () => {
  beforeEach(() => {
    busStore.resetAll();
    captured.components = undefined;
  });

  it("shows the workspace shell before the snapshot loads", () => {
    render(<App />);
    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(screen.getByText("Tinto")).toBeInTheDocument(); // top bar brand
  });

  // Covers AE1 (first-run gate) + R8
  it("shows first-run when loaded with no active workbench", () => {
    act(() => busStore.loadSnapshot([], { available: true })); // loaded, no config.active
    render(<App />);
    expect(screen.getByTestId("first-run")).toBeInTheDocument();
    expect(screen.queryByTestId("workspace-stub")).not.toBeInTheDocument();
  });

  it("shows the workspace with all panel types registered when a workbench is active", () => {
    const config: WorkbenchConfig = {
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [] }],
    };
    act(() => {
      busStore.setConfig(config);
      busStore.loadSnapshot([], { available: true });
    });
    render(<App />);
    expect(screen.getByTestId("workspace-stub")).toBeInTheDocument();
    expect(Object.keys(captured.components ?? {})).toEqual(
      expect.arrayContaining([PANEL_DASHBOARD, PANEL_TREE, PANEL_REPO]),
    );
  });
});
