import { useRef, useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useAccessibleDialog } from "./useAccessibleDialog";

function TestDialog({ onClose }: { onClose: () => void }) {
  const initialFocusRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({ onClose, initialFocusRef });

  return (
    <div data-testid="test-backdrop">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-label="Diálogo de prueba">
        <button ref={initialFocusRef} type="button">
          Primera acción
        </button>
        <button type="button" onClick={onClose}>
          Cerrar
        </button>
      </div>
    </div>
  );
}

function DialogHarness() {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button type="button" data-testid="dialog-trigger" onClick={() => setOpen(true)}>
        Abrir diálogo
      </button>
      <main data-testid="dialog-background">Contenido de fondo</main>
      {open && <TestDialog onClose={() => setOpen(false)} />}
    </div>
  );
}

function MenuDialogHarness() {
  const [menuOpen, setMenuOpen] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);

  return (
    <div>
      <div>
        <button
          type="button"
          aria-haspopup="menu"
          data-testid="menu-trigger"
          onClick={() => setMenuOpen(true)}
        >
          Ayuda
        </button>
        {menuOpen && (
          <div role="menu">
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setDialogOpen(true);
                setMenuOpen(false);
              }}
            >
              Abrir modal
            </button>
          </div>
        )}
      </div>
      {dialogOpen && <TestDialog onClose={() => setDialogOpen(false)} />}
    </div>
  );
}

describe("useAccessibleDialog", () => {
  it("moves focus inside, traps Tab, isolates the background, and restores everything on close", async () => {
    const user = userEvent.setup();
    render(<DialogHarness />);

    const trigger = screen.getByTestId("dialog-trigger");
    const background = screen.getByTestId("dialog-background");
    await user.click(trigger);

    const first = screen.getByRole("button", { name: "Primera acción" });
    const close = screen.getByRole("button", { name: "Cerrar" });
    expect(first).toHaveFocus();
    expect(trigger).toHaveAttribute("inert");
    expect(trigger).toHaveAttribute("aria-hidden", "true");
    expect(background).toHaveAttribute("inert");
    expect(background).toHaveAttribute("aria-hidden", "true");

    await user.tab();
    expect(close).toHaveFocus();
    await user.tab();
    expect(first).toHaveFocus();
    await user.tab({ shift: true });
    expect(close).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).not.toHaveAttribute("inert");
    expect(trigger).not.toHaveAttribute("aria-hidden");
    expect(background).not.toHaveAttribute("inert");
    expect(background).not.toHaveAttribute("aria-hidden");
    expect(trigger).toHaveFocus();
  });

  it("restores focus to a menu trigger when the invoking menu item was removed", async () => {
    const user = userEvent.setup();
    render(<MenuDialogHarness />);

    const trigger = screen.getByTestId("menu-trigger");
    await user.click(trigger);
    await user.click(screen.getByRole("menuitem", { name: "Abrir modal" }));
    expect(screen.getByRole("button", { name: "Primera acción" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
  });
});
