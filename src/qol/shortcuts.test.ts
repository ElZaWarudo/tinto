import { describe, it, expect, vi, beforeEach } from "vitest";
import { installShortcuts, SHORTCUTS } from "./shortcuts";
import { deleteUndoManager } from "../panels/file/deleteUndo";
import type { DockviewApi } from "dockview-react";

describe("shortcuts", () => {
  describe("SHORTCUTS", () => {
    it("defines all expected shortcuts", () => {
      expect(SHORTCUTS).toHaveLength(14);

      const actions = SHORTCUTS.map((s) => s.action);
      expect(actions).toContain("Alternar árbol de archivos");
      expect(actions).toContain("Siguiente proyecto");
      expect(actions).toContain("Proyecto anterior");
      expect(actions).toContain("Siguiente archivo");
      expect(actions).toContain("Archivo anterior");
      expect(actions).toContain("Cerrar archivo");
      expect(actions).toContain("Cerrar proyecto");
      expect(actions).toContain("Vista rápida");
      expect(actions).toContain("Abrir resumen");
      expect(actions).toContain("Abrir cronología");
      expect(actions).toContain("Refrescar proyecto");
      expect(actions).toContain("Añadir proyecto");
      expect(actions).toContain("Restaurar archivo eliminado");
      expect(actions).toContain("Rehacer eliminación de archivo");
    });

    it("groups shortcuts correctly", () => {
      const groups = new Set(SHORTCUTS.map((s) => s.group));
      expect(groups).toEqual(new Set(["Navegación", "Cerrar", "Vista", "Proyecto", "Archivos"]));
    });
  });

  describe("installShortcuts", () => {
    let mockApi: DockviewApi;
    let mockActions: { openDashboard: () => void; openTimeline: () => void; addRepo: () => void };

    beforeEach(() => {
      mockApi = {
        activePanel: null,
        panels: [],
      } as unknown as DockviewApi;

      mockActions = {
        openDashboard: vi.fn(),
        openTimeline: vi.fn(),
        addRepo: vi.fn(),
      };

      vi.clearAllMocks();
      deleteUndoManager.reset();
    });

    it("returns a cleanup function", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);
      expect(typeof cleanup).toBe("function");
      cleanup();
    });

    it("ignores events without modifier key", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      const event = new KeyboardEvent("keydown", { key: "b" });
      window.dispatchEvent(event);

      expect(mockActions.openDashboard).not.toHaveBeenCalled();
      cleanup();
    });

    it("ignores events in input elements", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      const input = document.createElement("input");
      document.body.appendChild(input);
      input.focus();

      const event = new KeyboardEvent("keydown", {
        key: "b",
        ctrlKey: true,
        bubbles: true,
      });
      input.dispatchEvent(event);

      expect(mockActions.openDashboard).not.toHaveBeenCalled();
      document.body.removeChild(input);
      cleanup();
    });

    it("suspends workspace shortcuts while an aria-modal dialog is open", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);
      const dialog = document.createElement("div");
      dialog.setAttribute("role", "dialog");
      dialog.setAttribute("aria-modal", "true");
      document.body.appendChild(dialog);

      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "D", ctrlKey: true, shiftKey: true }),
      );

      expect(mockActions.openDashboard).not.toHaveBeenCalled();
      dialog.remove();
      cleanup();
    });

    it("calls openDashboard on Ctrl+Shift+D", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      const event = new KeyboardEvent("keydown", {
        key: "D",
        ctrlKey: true,
        shiftKey: true,
      });
      window.dispatchEvent(event);

      expect(mockActions.openDashboard).toHaveBeenCalled();
      cleanup();
    });

    it("calls openTimeline on Ctrl+Shift+H", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      const event = new KeyboardEvent("keydown", {
        key: "H",
        ctrlKey: true,
        shiftKey: true,
      });
      window.dispatchEvent(event);

      expect(mockActions.openTimeline).toHaveBeenCalled();
      cleanup();
    });

    it("calls addRepo on Ctrl+Shift+A", () => {
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      const event = new KeyboardEvent("keydown", {
        key: "A",
        ctrlKey: true,
        shiftKey: true,
      });
      window.dispatchEvent(event);

      expect(mockActions.addRepo).toHaveBeenCalled();
      cleanup();
    });

    it("calls file undo and redo on Ctrl+Z and Ctrl+Shift+Z", async () => {
      const undoSpy = vi.spyOn(deleteUndoManager, "undo").mockResolvedValue(null);
      const redoSpy = vi.spyOn(deleteUndoManager, "redo").mockResolvedValue(null);
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, mockActions);

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true }));
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Z", ctrlKey: true, shiftKey: true }),
      );

      expect(undoSpy).toHaveBeenCalled();
      expect(redoSpy).toHaveBeenCalled();
      undoSpy.mockRestore();
      redoSpy.mockRestore();
      cleanup();
    });

    it("surfaces a failed file undo with a retry that reuses the recoverable operation", async () => {
      const undoSpy = vi
        .spyOn(deleteUndoManager, "undo")
        .mockResolvedValueOnce({
          copied: [],
          conflicts: [],
          fatalError: "No se pudo restaurar",
        })
        .mockResolvedValueOnce({ copied: [], conflicts: [] });
      const onFileMutationError = vi.fn();
      const apiRef = { current: mockApi };
      const cleanup = installShortcuts(apiRef, { ...mockActions, onFileMutationError });

      window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true }));
      await vi.waitFor(() => expect(onFileMutationError).toHaveBeenCalledOnce());

      const retry = onFileMutationError.mock.calls[0][1];
      retry();
      await vi.waitFor(() => expect(undoSpy).toHaveBeenCalledTimes(2));

      undoSpy.mockRestore();
      cleanup();
    });

    it("does nothing when api is null", () => {
      const apiRef = { current: null };
      const cleanup = installShortcuts(apiRef, mockActions);

      const event = new KeyboardEvent("keydown", {
        key: "D",
        ctrlKey: true,
        shiftKey: true,
      });
      window.dispatchEvent(event);

      expect(mockActions.openDashboard).not.toHaveBeenCalled();
      cleanup();
    });
  });
});
