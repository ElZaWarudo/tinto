import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { KeyboardShortcuts } from "./KeyboardShortcuts";

describe("KeyboardShortcuts", () => {
  it("renders the modal with title", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByTestId("shortcuts-modal")).toBeInTheDocument();
    expect(screen.getByText("Atajos de teclado")).toBeInTheDocument();
    expect(screen.getByTestId("shortcuts-close")).toHaveFocus();
  });

  it("displays zoom shortcuts in the Texto group", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Aumentar tamaño del texto")).toBeInTheDocument();
    expect(screen.getByText("Reducir tamaño del texto")).toBeInTheDocument();
    expect(screen.getByText("Restablecer tamaño del texto")).toBeInTheDocument();
  });

  it("displays navigation shortcuts", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Alternar árbol de archivos")).toBeInTheDocument();
    expect(screen.getByText("Siguiente proyecto")).toBeInTheDocument();
    expect(screen.getByText("Proyecto anterior")).toBeInTheDocument();
    expect(screen.getByText("Siguiente archivo")).toBeInTheDocument();
    expect(screen.getByText("Archivo anterior")).toBeInTheDocument();
  });

  it("displays close shortcuts", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Cerrar archivo")).toBeInTheDocument();
    expect(screen.getByText("Cerrar proyecto")).toBeInTheDocument();
  });

  it("displays view shortcuts", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Vista rápida")).toBeInTheDocument();
    expect(screen.getByText("Abrir resumen")).toBeInTheDocument();
    expect(screen.getByText("Abrir cronología")).toBeInTheDocument();
  });

  it("displays project shortcuts", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Refrescar proyecto")).toBeInTheDocument();
    expect(screen.getByText("Añadir proyecto")).toBeInTheDocument();
  });

  it("displays all group headings", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    expect(screen.getByText("Navegación")).toBeInTheDocument();
    expect(screen.getByText("Cerrar")).toBeInTheDocument();
    expect(screen.getByText("Vista")).toBeInTheDocument();
    expect(screen.getByText("Proyecto")).toBeInTheDocument();
    expect(screen.getByText("Texto")).toBeInTheDocument();
  });

  it("calls onClose when clicking the close button", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    fireEvent.click(screen.getByTestId("shortcuts-close"));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("calls onClose when clicking the backdrop", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    fireEvent.click(screen.getByTestId("shortcuts-backdrop"));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("does not call onClose when clicking inside the modal", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    fireEvent.click(screen.getByText("Navegación"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("calls onClose when pressing Escape", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("displays kbd elements for shortcut keys", () => {
    const onClose = vi.fn();
    render(<KeyboardShortcuts onClose={onClose} />);

    const kbdElements = document.querySelectorAll("kbd");
    expect(kbdElements.length).toBeGreaterThan(0);

    // At least one kbd should contain either ⌘ (Mac) or Ctrl (other)
    const kbdTexts = Array.from(kbdElements).map((el) => el.textContent);
    const hasModifier = kbdTexts.some((text) => text?.includes("⌘") || text?.includes("Ctrl"));
    expect(hasModifier).toBe(true);
  });
});
