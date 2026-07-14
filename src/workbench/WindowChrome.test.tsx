import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const windowApi = vi.hoisted(() => ({
  close: vi.fn(() => Promise.resolve()),
  isMaximized: vi.fn(() => Promise.resolve(false)),
  minimize: vi.fn(() => Promise.resolve()),
  onResized: vi.fn(() => Promise.resolve(() => {})),
  toggleMaximize: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => windowApi,
}));

import { CompactWindowBar } from "./WindowChrome";

describe("WindowChrome", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.defineProperty(globalThis, "isTauri", {
      configurable: true,
      value: true,
    });
    windowApi.isMaximized.mockResolvedValue(false);
    windowApi.onResized.mockResolvedValue(() => {});
  });

  afterEach(() => {
    delete (globalThis as typeof globalThis & { isTauri?: boolean }).isTauri;
  });

  it("exposes a native drag region and working window controls", async () => {
    const { container } = render(<CompactWindowBar />);
    const dragRegion = container.querySelector<HTMLElement>(".menu-bar__drag-region");

    expect(dragRegion).toHaveAttribute("data-tauri-drag-region");
    fireEvent.click(screen.getByRole("button", { name: "Minimizar" }));
    expect(windowApi.minimize).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Maximizar" }));
    await waitFor(() => expect(windowApi.toggleMaximize).toHaveBeenCalledOnce());

    fireEvent.doubleClick(dragRegion!);
    await waitFor(() => expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(2));

    fireEvent.click(screen.getByRole("button", { name: "Cerrar Tinto" }));
    expect(windowApi.close).toHaveBeenCalledOnce();
  });

  it("does not show desktop controls in a browser preview", () => {
    Object.defineProperty(globalThis, "isTauri", {
      configurable: true,
      value: false,
    });
    render(<CompactWindowBar />);

    expect(screen.queryByLabelText("Controles de ventana")).not.toBeInTheDocument();
  });
});
