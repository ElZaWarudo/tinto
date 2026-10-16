import { describe, expect, it } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import { confirm } from "./confirmDialog";

describe("confirm", () => {
  it("renders an in-app dialog and resolves true on the confirm button", async () => {
    const result = confirm("¿Revertir la sesión?\n\nNo se puede deshacer.", {
      title: "Revertir sesión de Agent",
      kind: "warning",
      okLabel: "Revertir sesión",
    });
    const dialog = await screen.findByRole("alertdialog", { name: "Revertir sesión de Agent" });
    expect(dialog).toHaveTextContent("¿Revertir la sesión?");
    fireEvent.click(screen.getByRole("button", { name: "Revertir sesión" }));
    await expect(result).resolves.toBe(true);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("resolves false on cancel and on Escape", async () => {
    const cancelled = confirm("¿Seguro?");
    fireEvent.click(await screen.findByRole("button", { name: "Cancelar" }));
    await expect(cancelled).resolves.toBe(false);

    const escaped = confirm("¿Seguro?");
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.keyDown(dialog, { key: "Escape" });
    await expect(escaped).resolves.toBe(false);
  });
});
