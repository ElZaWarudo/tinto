// Notes the user leaves on changed lines of a diff, kept for this session only
// (they survive closing and reopening the file), and sent to an Agent as one
// prompt. Nothing here touches the monitoring pipeline: it is the user's tool.

import { useSyncExternalStore } from "react";
import type { DiffLine } from "../../bus/contract";

export interface LineNote {
  id: string;
  repo: string;
  path: string;
  /** The new-side line number, or the old one for a removed line. */
  line: number;
  removed: boolean;
  lineText: string;
  text: string;
  resolved: boolean;
}

export type LineNoteAnchor = Pick<LineNote, "repo" | "path" | "line" | "removed" | "lineText">;

const MAX_LINE_TEXT_CHARS = 200;

class LineNoteStore {
  private notes: LineNote[] = [];
  private nextId = 1;
  private listeners = new Set<() => void>();

  getNotes = (): LineNote[] => this.notes;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(notes: LineNote[]) {
    this.notes = notes;
    this.listeners.forEach((listener) => listener());
  }

  add(anchor: LineNoteAnchor, text: string): string {
    const id = `note-${this.nextId++}`;
    this.set([...this.notes, { ...anchor, id, text, resolved: false }]);
    return id;
  }

  update(id: string, changes: Partial<Pick<LineNote, "text" | "resolved">>) {
    this.set(this.notes.map((note) => (note.id === id ? { ...note, ...changes } : note)));
  }

  remove(id: string) {
    this.set(this.notes.filter((note) => note.id !== id));
  }

  reset() {
    this.set([]);
  }
}

export const lineNoteStore = new LineNoteStore();

export function useLineNotes(): LineNote[] {
  return useSyncExternalStore(lineNoteStore.subscribe, lineNoteStore.getNotes);
}

/** Where a note on this line goes; `null` for an unchanged line, which takes none. */
export function anchorFor(repo: string, path: string, line: DiffLine): LineNoteAnchor | null {
  const removed = line.kind === "Removed";
  const number = removed ? line.old_lineno : line.new_lineno;
  if (line.kind === "Context" || number == null) return null;
  return { repo, path, line: number, removed, lineText: line.content };
}

export function sameAnchor(note: LineNote, anchor: LineNoteAnchor): boolean {
  return (
    note.repo === anchor.repo &&
    note.path === anchor.path &&
    note.line === anchor.line &&
    note.removed === anchor.removed
  );
}

/** One prompt with every note, each anchored as `path:line` with the line's text. */
export function notesPrompt(notes: LineNote[]): string {
  const items = notes.map((note, index) => {
    const lineText = note.lineText.trim();
    const shown =
      lineText.length > MAX_LINE_TEXT_CHARS
        ? `${lineText.slice(0, MAX_LINE_TEXT_CHARS)}…`
        : lineText;
    return [
      `${index + 1}. ${note.path}:${note.line}${note.removed ? " (línea eliminada)" : ""}`,
      `   > ${shown}`,
      `   ${note.text.trim().replace(/\n/g, "\n   ")}`,
    ].join("\n");
  });
  return [
    "Revisé tus cambios y dejé estas notas sobre líneas concretas. Atiende cada una:",
    "",
    ...items,
  ].join("\n");
}
