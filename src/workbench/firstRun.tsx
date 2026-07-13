// First-run empty state (R8): no workbench yet → create one inline. After
// creation the app shows the workspace (zero-repos state prompts adding repos).

import { useState } from "react";
import tintoWordmarkDark from "../assets/brand/tinto-wordmark-dark.png";
import { createAndActivate } from "./operations";

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

function Brand() {
  return (
    <div className="first-run__brand">
      <img src={tintoWordmarkDark} alt="Tinto" />
    </div>
  );
}

export function StartupLoading() {
  return (
    <main className="first-run first-run--status" data-testid="startup-loading">
      <Brand />
      <h1>Preparando Tinto</h1>
      <p role="status" aria-live="polite">
        Cargando workbenches y estado de los repositorios…
      </p>
    </main>
  );
}

export function StartupFailure({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <main className="first-run first-run--status" data-testid="startup-failure">
      <Brand />
      <h1>Tinto no pudo conectarse</h1>
      <p className="first-run__error" role="alert">
        {message}
      </p>
      <button type="button" className="first-run__retry" onClick={onRetry}>
        Reintentar conexión
      </button>
    </main>
  );
}

export function FirstRun() {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await createAndActivate(name.trim());
    } catch (cause) {
      setError(
        errorMessage(cause, "No se pudo crear la workbench. Revisa la conexión y reintenta."),
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="first-run" data-testid="first-run">
      <Brand />
      <h1>Bienvenido a Tinto</h1>
      <div className="first-run__form">
        <label id="workbench-name-label" htmlFor="workbench-name">
          Nombre de la workbench
        </label>
        <input
          id="workbench-name"
          data-testid="wb-name"
          placeholder="Ej. Trabajo"
          autoComplete="off"
          aria-invalid={Boolean(error)}
          aria-describedby={error ? "first-run-error" : undefined}
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => e.key === "Enter" && void create()}
        />
        <button
          type="button"
          data-testid="create-wb"
          onClick={() => void create()}
          disabled={busy || !name.trim()}
        >
          {busy ? "Creando…" : "Crear workbench"}
        </button>
      </div>
      {error && (
        <p id="first-run-error" className="first-run__error" role="alert">
          {error}
        </p>
      )}
    </main>
  );
}
