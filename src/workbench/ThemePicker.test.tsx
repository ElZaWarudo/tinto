import { useState } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { THEME_STORAGE_KEY, themeStore } from "../qol/theme";
import { ThemePicker } from "./ThemePicker";

function Harness() {
  const [open, setOpen] = useState(false);
  return <ThemePicker open={open} onOpenChange={setOpen} />;
}

const trigger = () => screen.getByTestId("theme-picker-trigger");
const rootTheme = () => document.documentElement.dataset.theme;

describe("ThemePicker", () => {
  beforeEach(() => {
    localStorage.clear();
    themeStore.set("dark");
  });

  it("names the theme in effect and lists every theme as a swatch painted with its tokens", async () => {
    render(<Harness />);
    expect(trigger()).toHaveTextContent("Oscuro");
    expect(trigger()).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(trigger());
    const group = screen.getByRole("radiogroup", { name: "Tema" });
    const options = within(group).getAllByRole("radio");
    expect(options.map((option) => option.textContent)).toEqual([
      "Oscuro",
      "Morado oscuro",
      "Ciruela",
      "Claro",
      "Según el sistema",
    ]);
    expect(within(group).getByRole("radio", { name: "Oscuro" })).toBeChecked();
    expect(within(group).getByRole("radio", { name: "Oscuro" })).toHaveFocus();

    const swatchThemes = (name: string) =>
      Array.from(
        within(group).getByRole("radio", { name }).querySelectorAll<HTMLElement>("[data-theme]"),
      ).map((swatch) => swatch.dataset.theme);
    expect(swatchThemes("Ciruela")).toEqual(["plum"]);
    expect(swatchThemes("Según el sistema")).toEqual(["dark", "light"]);
  });

  it("applies and saves a theme as soon as it is chosen, and keeps the panel open", async () => {
    render(<Harness />);
    await userEvent.click(trigger());
    await userEvent.click(screen.getByRole("radio", { name: "Ciruela" }));

    expect(rootTheme()).toBe("plum");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("plum");
    expect(screen.getByRole("radio", { name: "Ciruela" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Oscuro" })).not.toBeChecked();
    expect(trigger()).toHaveTextContent("Ciruela");
    expect(screen.getByRole("dialog", { name: "Tema" })).toBeInTheDocument();
  });

  it("moves the choice with the arrow keys and closes on Escape, back on the trigger", async () => {
    render(<Harness />);
    await userEvent.click(trigger());

    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    expect(rootTheme()).toBe("plum");
    expect(screen.getByRole("radio", { name: "Ciruela" })).toHaveFocus();

    await userEvent.keyboard("{ArrowLeft}");
    expect(rootTheme()).toBe("purple");

    // Wraps from the first theme to the last.
    await userEvent.keyboard("{ArrowUp}{ArrowUp}");
    expect(themeStore.getSetting()).toBe("system");

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger()).toHaveFocus();
  });

  it("closes when the user clicks outside", async () => {
    render(<Harness />);
    await userEvent.click(trigger());
    fireEvent.click(screen.getByTestId("theme-picker-backdrop"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(rootTheme()).toBe("dark");
  });
});
