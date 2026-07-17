import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openTerminal: vi.fn(),
  openTerminalParams: vi.fn(() => [{ sessionId: "session-1", repo: "/repo" }]),
  markTerminalDetached: vi.fn(),
  reattachDetachedConsoles: vi.fn(() => Promise.resolve(true)),
  closeCurrentDetachedWindow: vi.fn(() => Promise.resolve(true)),
  armExternalTabDetach: vi.fn(),
}));

vi.mock("../../bus/connection", () => ({ useBusConnection: vi.fn() }));
vi.mock("../../workspace/consoleDock", () => ({
  consoleDock: {
    openTerminal: mocks.openTerminal,
    openTerminalParams: mocks.openTerminalParams,
  },
}));
vi.mock("../../workspace/externalTabDetach", () => ({
  armExternalTabDetach: mocks.armExternalTabDetach,
}));
vi.mock("./ConsoleDockPanel", () => ({ ConsoleDockPanel: () => <div>Consolas</div> }));
vi.mock("./detachTerminalWindow", () => ({
  closeCurrentDetachedWindow: mocks.closeCurrentDetachedWindow,
  markTerminalDetached: mocks.markTerminalDetached,
  onDetachedConsolesOpenTerminal: vi.fn(() => Promise.resolve(() => {})),
  reattachDetachedConsoles: mocks.reattachDetachedConsoles,
}));

import { DetachedConsolesApp } from "./DetachedConsolesApp";

describe("DetachedConsolesApp", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.reattachDetachedConsoles.mockResolvedValue(true);
    mocks.closeCurrentDetachedWindow.mockResolvedValue(true);
  });

  it("offers an explicit reattach button in addition to pointer drag", async () => {
    const { container } = render(<DetachedConsolesApp />);
    const dragHandle = container.querySelector(".detached-terminal-window__drag-tab");
    const reattach = screen.getByRole("button", { name: "Reanexar Agents" });

    expect(dragHandle).not.toBeNull();
    expect(dragHandle).not.toContainElement(reattach);
    fireEvent.pointerDown(dragHandle!);
    expect(mocks.armExternalTabDetach).toHaveBeenCalledOnce();

    fireEvent.pointerDown(reattach);
    expect(mocks.armExternalTabDetach).toHaveBeenCalledOnce();
    fireEvent.click(reattach);
    await waitFor(() =>
      expect(mocks.reattachDetachedConsoles).toHaveBeenCalledWith([
        { sessionId: "session-1", repo: "/repo" },
      ]),
    );
    expect(mocks.markTerminalDetached).toHaveBeenCalledWith("session-1");
    expect(mocks.reattachDetachedConsoles.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.markTerminalDetached.mock.invocationCallOrder[0],
    );
    expect(mocks.closeCurrentDetachedWindow).toHaveBeenCalledOnce();
  });

  it("keeps the detached window recoverable when reattach fails", async () => {
    mocks.reattachDetachedConsoles.mockResolvedValueOnce(false);
    render(<DetachedConsolesApp />);

    fireEvent.click(screen.getByRole("button", { name: "Reanexar Agents" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudieron reanexar los Agents",
    );
    expect(screen.getByRole("alert")).not.toBe(screen.getByRole("button"));
    expect(mocks.markTerminalDetached).not.toHaveBeenCalled();
    expect(mocks.closeCurrentDetachedWindow).not.toHaveBeenCalled();

    mocks.reattachDetachedConsoles.mockResolvedValueOnce(true);
    fireEvent.click(screen.getByRole("button", { name: "Reintentar reanexado" }));
    await waitFor(() => expect(mocks.closeCurrentDetachedWindow).toHaveBeenCalledOnce());
  });

  it("blocks repeated reattach activations while the transfer is pending", async () => {
    let resolveReattach!: (value: boolean) => void;
    mocks.reattachDetachedConsoles.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveReattach = resolve;
      }),
    );
    render(<DetachedConsolesApp />);

    const reattach = screen.getByRole("button", { name: "Reanexar Agents" });
    fireEvent.click(reattach);
    fireEvent.click(reattach);

    expect(mocks.reattachDetachedConsoles).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Reanexando Agents…" })).toBeDisabled();

    resolveReattach(false);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Reintentar reanexado" })).toBeEnabled(),
    );
  });

  it("retries only the close after the main window acknowledged the transfer", async () => {
    mocks.closeCurrentDetachedWindow.mockResolvedValueOnce(false);
    render(<DetachedConsolesApp />);

    fireEvent.click(screen.getByRole("button", { name: "Reanexar Agents" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Los Agents ya están reanexados; no se pudo cerrar esta ventana",
    );
    expect(mocks.reattachDetachedConsoles).toHaveBeenCalledOnce();
    expect(mocks.markTerminalDetached).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Reintentar cierre" }));

    await waitFor(() => expect(mocks.closeCurrentDetachedWindow).toHaveBeenCalledTimes(2));
    expect(mocks.reattachDetachedConsoles).toHaveBeenCalledOnce();
    expect(mocks.markTerminalDetached).toHaveBeenCalledOnce();
  });
});
