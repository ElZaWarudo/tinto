import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import App from "./App";

interface TickPayload {
  timestamp_ms: number;
}
type TickHandler = (event: { event: string; id: number; payload: TickPayload }) => void;

const tickHandlers: TickHandler[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: TickHandler) => {
    if (event === "tick") tickHandlers.push(handler);
    return Promise.resolve(() => {});
  }),
}));

describe("App (puente de humo)", () => {
  beforeEach(() => {
    tickHandlers.length = 0;
    mockIPC((cmd) => {
      if (cmd === "ping") {
        return { message: "pong desde el backend de Tinto", timestamp_ms: 1718000000000 };
      }
      return undefined;
    });
  });

  afterEach(() => {
    clearMocks();
    vi.clearAllMocks();
  });

  // Covers AE1
  it("muestra la respuesta del ping del backend al montar", async () => {
    render(<App />);
    await waitFor(() =>
      expect(screen.getByTestId("ping")).toHaveTextContent("pong desde el backend de Tinto"),
    );
  });

  // Covers AE2
  it("refleja el timestamp del ultimo tick recibido", async () => {
    render(<App />);
    expect(screen.getByTestId("tick")).toHaveTextContent("esperando...");

    await waitFor(() => expect(tickHandlers.length).toBeGreaterThan(0));
    const timestamp = Date.UTC(2026, 5, 10, 14, 30, 0);
    act(() => {
      tickHandlers.forEach((handler) =>
        handler({ event: "tick", id: 1, payload: { timestamp_ms: timestamp } }),
      );
    });

    expect(screen.getByTestId("tick")).not.toHaveTextContent("esperando...");
    // toLocaleTimeString: patrón hora con separadores (p. ej. 14:30:00 / 2:30:00 PM)
    expect(screen.getByTestId("tick")).toHaveTextContent(/\d{1,2}:\d{2}:\d{2}/);
  });

  it("muestra estado de error si el ping falla", async () => {
    // mockIPC re-registra el interceptor: reemplaza el de beforeEach sin limpiar a mitad de test
    mockIPC((cmd) => {
      if (cmd === "ping") {
        return Promise.reject(new Error("backend caido"));
      }
      return undefined;
    });

    render(<App />);
    await waitFor(() =>
      expect(screen.getByTestId("ping")).toHaveTextContent("error contactando el backend"),
    );
  });
});
