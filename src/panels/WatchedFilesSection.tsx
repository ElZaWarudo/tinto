import { useMemo, useState } from "react";
import type { FsEvent, WatchingState } from "../bus/contract";

interface Props {
  repo: string;
  activeWorkbench: string | null;
  patterns: string[];
  events: FsEvent[];
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
  const size = event.size == null ? "size unknown" : `${event.size} B`;
  if (event.size_delta == null || event.size_delta === 0) return size;
  const sign = event.size_delta > 0 ? "+" : "";
  return `${size} (${sign}${event.size_delta} B)`;
}

export function WatchedFilesSection({
  activeWorkbench,
  patterns,
  events,
  watching,
  onSave,
}: Props) {
  const [rows, setRows] = useState<string[]>(rowsFromPatterns(patterns));
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  const normalized = useMemo(() => normalizePatterns(rows), [rows]);

  const updateRow = (index: number, value: string) => {
    setRows((current) => current.map((row, i) => (i === index ? value : row)));
    setDirty(true);
    setError(null);
  };

  const addRow = () => {
    setRows((current) => [...current, ""]);
    setDirty(true);
    setError(null);
  };

  const removeRow = (index: number) => {
    setRows((current) => {
      const next = current.filter((_, i) => i !== index);
      return next.length ? next : [""];
    });
    setDirty(true);
    setError(null);
  };

  const cancel = () => {
    setRows(rowsFromPatterns(patterns));
    setDirty(false);
    setError(null);
  };

  const save = async () => {
    if (!activeWorkbench) {
      setError("No active workbench.");
      return;
    }
    if (rows.some((row) => row.trim() === "") && normalized.length > 0) {
      setError("Remove blank patterns before saving.");
      return;
    }
    const dup = duplicatePattern(normalized);
    if (dup) {
      setError(`Duplicate pattern: ${dup}`);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await onSave(normalized);
      setRows(normalized.length ? normalized : [""]);
      setDirty(false);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="watched-files" data-testid="watched-files">
      <header className="watched-files__head">
        <h3>Watched files</h3>
        {!watching.available && (
          <span className="watched-files__degraded" data-testid="watched-degraded">
            watcher degraded: {watching.reason ?? "unknown reason"}
          </span>
        )}
      </header>

      <div className="watch-patterns" data-testid="watch-patterns">
        <h4>Patterns</h4>
        {rows.map((row, index) => (
          <div className="watch-patterns__row" key={index}>
            <input
              aria-label={`watch pattern ${index + 1}`}
              value={row}
              placeholder=".env"
              onChange={(e) => updateRow(index, e.target.value)}
            />
            <button type="button" onClick={() => removeRow(index)}>
              Remove
            </button>
          </div>
        ))}
        <div className="watch-patterns__actions">
          <button type="button" onClick={addRow}>
            Add pattern
          </button>
          <button type="button" disabled={!dirty || saving} onClick={cancel}>
            Cancel
          </button>
          <button type="button" disabled={!dirty || saving} onClick={() => void save()}>
            {saving ? "Saving…" : "Save patterns"}
          </button>
        </div>
        {patterns.length === 0 && !dirty && (
          <p className="repo-panel__muted" data-testid="watch-no-patterns">
            No watched patterns configured.
          </p>
        )}
        {error && (
          <p className="watched-files__error" data-testid="watch-error">
            {error}
          </p>
        )}
      </div>

      <div className="watch-events" data-testid="watch-events">
        <h4>Recent events</h4>
        {events.length === 0 ? (
          <p className="repo-panel__muted" data-testid="watch-no-events">
            No watched file events yet.
          </p>
        ) : (
          <ul>
            {events.map((event, index) => (
              <li className="watch-event" key={`${event.timestamp_ms}:${event.path}:${index}`}>
                <span className={`watch-event__kind watch-event__kind--${event.kind}`}>
                  {event.kind}
                </span>
                <span className="watch-event__path">{event.path}</span>
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
