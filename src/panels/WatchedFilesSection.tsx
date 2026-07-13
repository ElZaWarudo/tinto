import { useMemo, useState } from "react";
import type { FsEvent, WatchingState } from "../bus/contract";
import { SignalBadges } from "./SignalBadges";

interface Props {
  repo: string;
  activeWorkbench: string | null;
  patterns: string[];
  events: FsEvent[];
  filtersActive?: boolean;
  watching: WatchingState;
  onSave: (patterns: string[]) => Promise<void>;
}

function normalizePatterns(rows: string[]): string[] {
  return rows.map((r) => r.trim()).filter(Boolean);
}

function rowsFromPatterns(patterns: string[]): string[] {
  return patterns.length ? patterns : [""];
}

function duplicatePattern(patterns: string[]): string | null {
  const seen = new Set<string>();
  for (const p of patterns) {
    if (seen.has(p)) return p;
    seen.add(p);
  }
  return null;
}

function formatSize(event: FsEvent): string {
  const size = event.size == null ? "tamaño desconocido" : `${event.size} B`;
  if (event.size_delta == null || event.size_delta === 0) return size;
  const sign = event.size_delta > 0 ? "+" : "";
  return `${size} (${sign}${event.size_delta} B)`;
}

function eventKindLabel(kind: FsEvent["kind"]): string {
  switch (kind) {
    case "created":
      return "Creado";
    case "modified":
      return "Modificado";
    case "removed":
      return "Eliminado";
  }
}

export function WatchedFilesSection({
  activeWorkbench,
  patterns,
  events,
  filtersActive = false,
  watching,
  onSave,
}: Props) {
  const patternsKey = patterns.join("\0");
  const [editor, setEditor] = useState(() => ({
    sourceKey: patternsKey,
    rows: rowsFromPatterns(patterns),
    dirty: false,
  }));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const rows =
    !editor.dirty && editor.sourceKey !== patternsKey ? rowsFromPatterns(patterns) : editor.rows;
  const dirty = editor.dirty;
  const normalized = useMemo(() => normalizePatterns(rows), [rows]);

  const updateRow = (index: number, value: string) => {
    if (saving) return;
    setEditor({
      sourceKey: patternsKey,
      rows: rows.map((row, i) => (i === index ? value : row)),
      dirty: true,
    });
    setError(null);
    setSaved(false);
  };

  const addRow = () => {
    if (saving) return;
    setEditor({ sourceKey: patternsKey, rows: [...rows, ""], dirty: true });
    setError(null);
    setSaved(false);
  };

  const removeRow = (index: number) => {
    if (saving) return;
    const next = rows.filter((_, i) => i !== index);
    setEditor({ sourceKey: patternsKey, rows: next.length ? next : [""], dirty: true });
    setError(null);
    setSaved(false);
  };

  const cancel = () => {
    setEditor({ sourceKey: patternsKey, rows: rowsFromPatterns(patterns), dirty: false });
    setError(null);
    setSaved(false);
  };

  const save = async () => {
    if (!activeWorkbench) {
      setError("No hay una workbench activa.");
      return;
    }
    if (rows.some((row) => row.trim() === "") && normalized.length > 0) {
      setError("Quita los patrones vacíos antes de guardar.");
      return;
    }
    const dup = duplicatePattern(normalized);
    if (dup) {
      setError(`Patrón duplicado: ${dup}`);
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await onSave(normalized);
      setEditor({
        sourceKey: patternsKey,
        rows: normalized.length ? normalized : [""],
        dirty: false,
      });
      setSaved(true);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="watched-files" data-testid="watched-files" aria-busy={saving}>
      <header className="watched-files__head">
        <h3>Archivos observados</h3>
        {!watching.available && (
          <span className="watched-files__degraded" data-testid="watched-degraded">
            supervisión degradada: {watching.reason ?? "motivo desconocido"}
          </span>
        )}
      </header>

      <div className="watch-patterns" data-testid="watch-patterns">
        <h4>Patrones</h4>
        {rows.map((row, index) => (
          <div className="watch-patterns__row" key={index}>
            <label className="watch-patterns__field">
              <span>Patrón {index + 1}</span>
              <input
                value={row}
                disabled={saving}
                placeholder=".env"
                aria-invalid={Boolean(error)}
                aria-describedby={error ? "watch-pattern-error" : undefined}
                onChange={(e) => updateRow(index, e.target.value)}
              />
            </label>
            <button type="button" disabled={saving} onClick={() => removeRow(index)}>
              Quitar
            </button>
          </div>
        ))}
        <div className="watch-patterns__actions">
          <button type="button" disabled={saving} onClick={addRow}>
            Añadir patrón
          </button>
          <button type="button" disabled={!dirty || saving} onClick={cancel}>
            Cancelar
          </button>
          <button type="button" disabled={!dirty || saving} onClick={() => void save()}>
            {saving ? "Guardando…" : "Guardar patrones"}
          </button>
        </div>
        <span className="watch-patterns__status" role="status" aria-live="polite">
          {saving ? "Guardando patrones." : saved ? "Patrones guardados." : ""}
        </span>
        {patterns.length === 0 && !dirty && (
          <p className="repo-panel__muted" data-testid="watch-no-patterns">
            No hay patrones configurados.
          </p>
        )}
        {error && (
          <p
            id="watch-pattern-error"
            className="watched-files__error"
            data-testid="watch-error"
            role="alert"
          >
            {error}
          </p>
        )}
      </div>

      <div className="watch-events" data-testid="watch-events">
        <h4>Eventos recientes</h4>
        {events.length === 0 ? (
          <p className="repo-panel__muted" data-testid="watch-no-events">
            {filtersActive
              ? "Ningún evento coincide con los filtros actuales."
              : "Todavía no hay eventos de archivos observados."}
          </p>
        ) : (
          <ul>
            {events.map((event, index) => (
              <li className="watch-event" key={`${event.timestamp_ms}:${event.path}:${index}`}>
                <span className={`watch-event__kind watch-event__kind--${event.kind}`}>
                  {eventKindLabel(event.kind)}
                </span>
                <span className="watch-event__path">{event.path}</span>
                <SignalBadges signals={event.signals ?? []} limit={2} compact />
                <span className="watch-event__meta">
                  {new Date(event.timestamp_ms).toLocaleString()} · {formatSize(event)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}
