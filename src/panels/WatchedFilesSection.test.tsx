import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { WatchedFilesSection } from "./WatchedFilesSection";
import type { FsEvent, WatchingState } from "../bus/contract";

const event = (over: Partial<FsEvent> = {}): FsEvent => ({
  path: ".env",
  kind: "modified",
  timestamp_ms: 1_700_000_000_000,
  size: 12,
  size_delta: 2,
  ...over,
});

function renderSection(over: Partial<Parameters<typeof WatchedFilesSection>[0]> = {}) {
  const onSave = over.onSave ?? vi.fn(() => Promise.resolve());
  const props = {
    repo: "/r/api",
    activeWorkbench: "Work",
    patterns: [".env"],
    events: [],
    watching: { available: true } satisfies WatchingState,
    onSave,
    ...over,
  };
  render(<WatchedFilesSection {...props} />);
  return { onSave };
}

describe("WatchedFilesSection", () => {
  it("renders configured patterns and recent watched-file events", () => {
    renderSection({
      patterns: [".env", "secrets/*.json"],
      events: [
        event({
          path: ".env",
          kind: "modified",
          size: 12,
          size_delta: 2,
          signals: [
            {
              kind: "sensitive_path",
              severity: "warning",
              path: ".env",
              message: "Sensitive watched file changed",
            },
          ],
        }),
        event({ path: "secret.json", kind: "created", size: 4, size_delta: 4 }),
      ],
    });

    expect(screen.getByDisplayValue(".env")).toBeInTheDocument();
    expect(screen.getByDisplayValue("secrets/*.json")).toBeInTheDocument();
    expect(screen.getByTestId("watch-events")).toHaveTextContent(".env");
    expect(screen.getByTestId("watch-events")).toHaveTextContent("Modificado");
    expect(screen.getByTestId("watch-events")).toHaveTextContent("12 B (+2 B)");
    expect(screen.getByTestId("watch-events")).toHaveTextContent("Archivo sensible");
    expect(screen.getByTestId("watch-events")).toHaveTextContent("secret.json");
  });

  it("shows empty and degraded states distinctly", () => {
    renderSection({ patterns: [], watching: { available: false, reason: "backend unavailable" } });

    expect(screen.getByTestId("watch-no-patterns")).toHaveTextContent(
      "No hay patrones configurados.",
    );
    expect(screen.getByTestId("watch-no-events")).toHaveTextContent(
      "Todavía no hay eventos de archivos observados.",
    );
    expect(screen.getByTestId("watched-degraded")).toHaveTextContent("backend unavailable");
  });

  it("validates blank and duplicate pattern rows before saving", async () => {
    const { onSave } = renderSection({ patterns: [".env"] });

    fireEvent.click(screen.getByText("Añadir patrón"));
    fireEvent.click(screen.getByText("Guardar patrones"));
    expect(await screen.findByTestId("watch-error")).toHaveTextContent("patrones vacíos");
    expect(onSave).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Patrón 2"), { target: { value: ".env" } });
    fireEvent.click(screen.getByText("Guardar patrones"));
    expect(await screen.findByTestId("watch-error")).toHaveTextContent("Patrón duplicado: .env");
    expect(onSave).not.toHaveBeenCalled();
  });

  it("saves normalized patterns and can clear the list", async () => {
    const { onSave } = renderSection({ patterns: [".env"] });

    fireEvent.change(screen.getByLabelText("Patrón 1"), {
      target: { value: " secrets/*.json " },
    });
    fireEvent.click(screen.getByText("Guardar patrones"));
    await waitFor(() => expect(onSave).toHaveBeenCalledWith(["secrets/*.json"]));
    expect(screen.getByRole("status")).toHaveTextContent("Patrones guardados.");

    fireEvent.click(screen.getByText("Quitar"));
    fireEvent.click(screen.getByText("Guardar patrones"));
    await waitFor(() => expect(onSave).toHaveBeenLastCalledWith([]));
  });

  it("keeps the draft visible when save fails", async () => {
    renderSection({
      patterns: [".env"],
      onSave: vi.fn(() => Promise.reject(new Error("invalid glob"))),
    });

    fireEvent.change(screen.getByLabelText("Patrón 1"), { target: { value: "[" } });
    fireEvent.click(screen.getByText("Guardar patrones"));

    expect(await screen.findByTestId("watch-error")).toHaveTextContent("invalid glob");
    expect(screen.getByDisplayValue("[")).toBeInTheDocument();
  });

  it("locks the editor while a save is pending so the saved draft cannot be overwritten", async () => {
    let resolveSave!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        }),
    );
    renderSection({ patterns: [".env"], onSave });

    const input = screen.getByLabelText("Patrón 1");
    fireEvent.change(input, { target: { value: "secrets/*.json" } });
    fireEvent.click(screen.getByText("Guardar patrones"));

    expect(screen.getByTestId("watched-files")).toHaveAttribute("aria-busy", "true");
    expect(input).toBeDisabled();
    expect(screen.getByText("Añadir patrón")).toBeDisabled();
    expect(screen.getByText("Quitar")).toBeDisabled();

    await act(async () => resolveSave());
    expect(screen.getByRole("status")).toHaveTextContent("Patrones guardados.");
    expect(screen.getByDisplayValue("secrets/*.json")).toBeEnabled();
  });
});
