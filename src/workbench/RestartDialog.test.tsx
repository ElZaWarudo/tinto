import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

const { invoke, open } = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

import { RestartDialog } from "./RestartDialog";

const plan = {
  jobs: [{ task_key: "TIN-7", role: "implementation", agent: "codex" }],
  conversations: [
    { session_id: "s1", repo: "C:\\repos\\tinto", agent_type: "claude", working: true },
    { session_id: "s2", repo: "C:\\repos\\api", agent_type: "codex", working: false },
  ],
};

describe("RestartDialog", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    invoke.mockImplementation((command: string) =>
      command === "restart_plan" ? Promise.resolve(plan) : Promise.resolve(),
    );
  });

  it("says what keeps running and what resumes before restarting", async () => {
    render(<RestartDialog onClose={() => {}} />);

    const running = await screen.findByRole("region", { name: "Siguen en marcha" });
    expect(running).toHaveTextContent("TIN-7 · implementation (codex)");
    const resumed = screen.getByRole("region", { name: "Se retoman al abrir" });
    expect(resumed).toHaveTextContent("claude · tinto — a mitad de turno: se le pide seguir");
    expect(resumed).toHaveTextContent("codex · api");

    fireEvent.click(screen.getByRole("button", { name: "Reiniciar" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("restart_tinto", { installer: null }));
  });

  it("runs the chosen installer instead of reopening itself", async () => {
    open.mockResolvedValue("C:\\Downloads\\Tinto_setup.exe");
    render(<RestartDialog onClose={() => {}} />);
    await screen.findByRole("region", { name: "Siguen en marcha" });

    fireEvent.click(screen.getByRole("button", { name: "Instalar actualización…" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("restart_tinto", {
        installer: "C:\\Downloads\\Tinto_setup.exe",
      }),
    );
  });

  it("does nothing when no installer is chosen, and shows a failed restart", async () => {
    open.mockResolvedValue(null);
    render(<RestartDialog onClose={() => {}} />);
    await screen.findByRole("region", { name: "Siguen en marcha" });
    fireEvent.click(screen.getByRole("button", { name: "Instalar actualización…" }));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalledWith("restart_tinto", expect.anything());

    invoke.mockImplementation((command: string) =>
      command === "restart_tinto"
        ? Promise.reject({ category: "x", message: "sin permiso" })
        : Promise.resolve(plan),
    );
    fireEvent.click(screen.getByRole("button", { name: "Reiniciar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("sin permiso");
  });

  it("says when nothing is running", async () => {
    invoke.mockResolvedValue({ jobs: [], conversations: [] });
    render(<RestartDialog onClose={() => {}} />);
    expect(await screen.findByText("Ningún trabajo de Delivery en marcha.")).toBeInTheDocument();
    expect(screen.getByText("Ninguna conversación abierta.")).toBeInTheDocument();
  });
});
