import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ActionButton, ActionGroup } from "./ActionButton";
import { DeliveryRefreshContext } from "./deliveryRefresh";

describe("ActionButton", () => {
  it("shows its own progress and check, refreshes, and keeps its label in place", async () => {
    const user = userEvent.setup();
    let finish: () => void = () => {};
    const refresh = vi.fn(() => Promise.resolve());
    render(
      <DeliveryRefreshContext.Provider value={refresh}>
        <ActionGroup>
          <ActionButton onAction={() => new Promise<void>((resolve) => (finish = resolve))}>
            Aprobar
          </ActionButton>
          <ActionButton onAction={() => Promise.resolve()}>Rechazar</ActionButton>
        </ActionGroup>
        <ActionButton onAction={() => Promise.resolve()}>Otra tarea</ActionButton>
      </DeliveryRefreshContext.Provider>,
    );
    const approve = screen.getByRole("button", { name: "Aprobar" });
    await user.click(approve);
    expect(approve).toHaveAttribute("aria-busy", "true");
    expect(approve).toHaveTextContent("Aprobar");
    expect(approve).toBeEnabled();
    // Its group waits; the rest of the panel does not.
    expect(screen.getByRole("button", { name: "Rechazar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Otra tarea" })).toBeEnabled();
    finish();
    await waitFor(() => expect(approve).toHaveClass("delivery-action--done"));
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Rechazar" })).toBeEnabled();
  });

  it("says next to it why it failed, and does nothing when the user cancels", async () => {
    const user = userEvent.setup();
    const refresh = vi.fn(() => Promise.resolve());
    render(
      <DeliveryRefreshContext.Provider value={refresh}>
        <ActionButton onAction={() => Promise.reject(new Error("GitHub no responde"))}>
          Abrir PR
        </ActionButton>
        <ActionButton onAction={() => Promise.resolve(false)}>Retirar</ActionButton>
      </DeliveryRefreshContext.Provider>,
    );
    await user.click(screen.getByRole("button", { name: "Abrir PR" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("GitHub no responde");
    expect(screen.getByRole("button", { name: "Abrir PR" })).toHaveClass("delivery-action--failed");
    await user.click(screen.getByRole("button", { name: "Retirar" }));
    expect(screen.getByRole("button", { name: "Retirar" })).toHaveClass("delivery-action--idle");
    expect(refresh).not.toHaveBeenCalled();
  });
});
