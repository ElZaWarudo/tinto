// The diff's line notes: the gutter "+" that opens one, each line's notes
// (edit, resolve, delete) and the bar that sends the repo's open notes to one
// of its Agents as a single prompt.

import { useState } from "react";
import type { AgentSession } from "../../bus/contract";
import { writeAgentSessionInput } from "../../bus/client";
import { sessionSignal, SIGNAL_LABEL } from "../../agent/sessionSignal";
import { useAgentSessionState } from "../../agent/sessionStore";
import {
  lineNoteStore,
  notesPrompt,
  sameAnchor,
  type LineNote,
  type LineNoteAnchor,
} from "./lineNoteStore";

/** What a diff needs to show and add notes: its repo's notes and the line
 *  being annotated. */
export interface DiffNotes {
  repo: string;
  path: string;
  notes: LineNote[];
  composing: string | null;
  setComposing: (key: string | null) => void;
}

function anchorKey(anchor: LineNoteAnchor): string {
  return `${anchor.removed ? "-" : "+"}${anchor.line}`;
}

function anchorLabel(anchor: LineNoteAnchor): string {
  return `${anchor.removed ? "línea eliminada" : "línea"} ${anchor.line}`;
}

export function NoteAddButton({ notes, anchor }: { notes: DiffNotes; anchor: LineNoteAnchor }) {
  return (
    <button
      type="button"
      className="diff-note-add"
      aria-label={`Añadir nota en la ${anchorLabel(anchor)}`}
      title="Añadir nota"
      onClick={() => notes.setComposing(anchorKey(anchor))}
    >
      +
    </button>
  );
}

export function LineNoteThread({ notes, anchor }: { notes: DiffNotes; anchor: LineNoteAnchor }) {
  const here = notes.notes.filter((note) => sameAnchor(note, anchor));
  const composing = notes.composing === anchorKey(anchor);
  if (here.length === 0 && !composing) return null;
  return (
    <div className="diff-notes" data-testid="diff-notes">
      {here.map((note) => (
        <LineNoteItem key={note.id} note={note} />
      ))}
      {composing && (
        <NoteEditor
          label={`Nota en la ${anchorLabel(anchor)}`}
          initial=""
          onSave={(text) => {
            lineNoteStore.add(anchor, text);
            notes.setComposing(null);
          }}
          onCancel={() => notes.setComposing(null)}
        />
      )}
    </div>
  );
}

function LineNoteItem({ note }: { note: LineNote }) {
  const [editing, setEditing] = useState(false);
  if (editing) {
    return (
      <NoteEditor
        label={`Editar nota de la línea ${note.line}`}
        initial={note.text}
        onSave={(text) => {
          lineNoteStore.update(note.id, { text });
          setEditing(false);
        }}
        onCancel={() => setEditing(false)}
      />
    );
  }
  return (
    <div className={`diff-note${note.resolved ? " diff-note--resolved" : ""}`}>
      <p className="diff-note__text">{note.text}</p>
      <div className="diff-note__actions">
        {note.resolved && <span className="diff-note__state">Resuelta</span>}
        <button type="button" onClick={() => setEditing(true)}>
          Editar
        </button>
        <button
          type="button"
          onClick={() => lineNoteStore.update(note.id, { resolved: !note.resolved })}
        >
          {note.resolved ? "Reabrir" : "Resolver"}
        </button>
        <button type="button" onClick={() => lineNoteStore.remove(note.id)}>
          Eliminar
        </button>
      </div>
    </div>
  );
}

function NoteEditor({
  label,
  initial,
  onSave,
  onCancel,
}: {
  label: string;
  initial: string;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const save = () => {
    if (text.trim()) onSave(text.trim());
  };
  return (
    <div className="diff-note diff-note--editing">
      <textarea
        aria-label={label}
        autoFocus
        rows={3}
        value={text}
        placeholder="¿Qué debería cambiar el Agent aquí?"
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            onCancel();
          } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            save();
          }
        }}
      />
      <div className="diff-note__actions">
        <button type="button" className="diff-note__save" disabled={!text.trim()} onClick={save}>
          Guardar
        </button>
        <button type="button" onClick={onCancel}>
          Cancelar
        </button>
      </div>
    </div>
  );
}

function agentOptionLabel(session: AgentSession): string {
  const signal = sessionSignal(session);
  return `${session.agent_type} · ${session.id.slice(0, 8)} · ${signal ? SIGNAL_LABEL[signal] : "terminal"}`;
}

/** The repo's open notes, the Agent to send them to and the button. */
export function LineNotesBar({ repo, notes }: { repo: string; notes: LineNote[] }) {
  const { sessions } = useAgentSessionState();
  const [chosen, setChosen] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const open = notes.filter((note) => note.repo === repo && !note.resolved);
  const agents = Object.values(sessions).filter(
    (session) =>
      session.repo === repo && (session.status === "running" || session.status === "starting"),
  );
  if (open.length === 0 && !message) return null;
  const target = agents.find((session) => session.id === chosen) ?? agents[0];
  const busy = target ? sessionSignal(target) === "working" : false;

  const send = async () => {
    if (!target) return;
    setSending(true);
    setMessage(null);
    try {
      await writeAgentSessionInput(target.id, `${notesPrompt(open)}\r`);
      for (const note of open) lineNoteStore.update(note.id, { resolved: true });
      setMessage(
        `${open.length === 1 ? "Nota enviada" : `${open.length} notas enviadas`} al Agent; quedan como resueltas.`,
      );
    } catch (error) {
      const detail =
        typeof error === "object" && error !== null && "message" in error
          ? String((error as { message: unknown }).message)
          : String(error);
      setMessage(`No se pudieron enviar las notas: ${detail}`);
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="diff-notes-bar" data-testid="diff-notes-bar">
      {open.length > 0 && (
        <>
          <span>
            {open.length === 1 ? "1 nota sin resolver" : `${open.length} notas sin resolver`} en
            este repo
          </span>
          {agents.length === 0 ? (
            <span className="diff-notes-bar__hint">Abre un Agent en este repo para enviarlas.</span>
          ) : (
            <>
              <select
                aria-label="Agent que recibe las notas"
                value={target?.id ?? ""}
                onChange={(event) => setChosen(event.target.value)}
              >
                {agents.map((session) => (
                  <option key={session.id} value={session.id}>
                    {agentOptionLabel(session)}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="diff-notes-bar__send"
                disabled={sending || busy}
                title={busy ? "Espera a que el Agent termine su turno" : undefined}
                onClick={() => void send()}
              >
                {sending ? "Enviando…" : "Enviar al agente"}
              </button>
            </>
          )}
        </>
      )}
      {message && (
        <span className="diff-notes-bar__message" role="status">
          {message}
        </span>
      )}
    </div>
  );
}
