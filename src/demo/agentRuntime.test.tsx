import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { AgentRuntimeFixture } from "./agentRuntime";

describe("AgentRuntimeFixture", () => {
  it("uses the production composer column order", () => {
    const { container } = render(<AgentRuntimeFixture />);
    const row = container.querySelector(".agent-panel__composer-row");

    expect(row).not.toBeNull();
    const controls = Array.from(row!.children);
    expect(controls).toHaveLength(3);
    expect(controls[0]).toBe(
      within(row as HTMLElement).getByRole("button", {
        name: "Adjuntar archivos (no disponible en esta fixture)",
      }),
    );
    expect(controls[1]).toBe(screen.getByRole("textbox", { name: "Mensaje para Codex" }));
    expect(controls[2]).toBe(screen.getByRole("button", { name: "Enviar" }));
  });

  it("simulates pointer submission without launching a real Agent", async () => {
    const user = userEvent.setup();
    render(<AgentRuntimeFixture />);

    const message = screen.getByRole("textbox", { name: "Mensaje para Codex" });
    const send = screen.getByRole("button", { name: "Enviar" });
    expect(send).toBeDisabled();

    await user.type(message, "Revisa el cambio");
    await user.click(send);

    expect(message).toHaveValue("");
    expect(send).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Envío simulado: «Revisa el cambio». No se ejecutó ningún Agent real.",
    );
  });

  it("simulates keyboard submission and reports the synthetic result", async () => {
    const user = userEvent.setup();
    render(<AgentRuntimeFixture />);

    const message = screen.getByRole("textbox", { name: "Mensaje para Codex" });
    await user.type(message, "Comprueba el runtime");
    await user.tab();
    expect(screen.getByRole("button", { name: "Enviar" })).toHaveFocus();
    await user.keyboard("{Enter}");

    expect(message).toHaveValue("");
    expect(screen.getByRole("status")).toHaveTextContent(
      "Envío simulado: «Comprueba el runtime». No se ejecutó ningún Agent real.",
    );
  });
});
