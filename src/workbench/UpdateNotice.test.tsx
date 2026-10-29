import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const updater = vi.hoisted(() => ({ check: vi.fn(), relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: updater.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: updater.relaunch }));

import { UpdateNotice } from "./UpdateNotice";

describe("UpdateNotice", () => {
  beforeEach(() => {
    updater.check.mockReset();
    updater.relaunch.mockReset();
  });

  it("installs a newer release and restarts Tinto", async () => {
    const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
    updater.check.mockResolvedValue({ version: "1.0.1", downloadAndInstall });
    render(<UpdateNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Actualizar y reiniciar" }));

    expect(screen.getByText("Tinto 1.0.1 está disponible.")).toBeInTheDocument();
    expect(downloadAndInstall).toHaveBeenCalled();
    expect(updater.relaunch).toHaveBeenCalled();
  });

  it("says why an install failed and lets the user try again", async () => {
    const downloadAndInstall = vi.fn().mockRejectedValue({ message: "firma no válida" });
    updater.check.mockResolvedValue({ version: "1.0.1", downloadAndInstall });
    render(<UpdateNotice />);

    await userEvent.click(await screen.findByRole("button", { name: "Actualizar y reiniciar" }));

    expect(await screen.findByText("No se pudo instalar: firma no válida")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Actualizar y reiniciar" })).toBeEnabled();
    expect(updater.relaunch).not.toHaveBeenCalled();
  });
});
