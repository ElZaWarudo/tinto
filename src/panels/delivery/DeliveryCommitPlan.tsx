// The commit step as a plan: one or more commits, each with its exact
// message and files. The form prepares it; the view shows it to whoever
// approves.

import { useEffect, useState } from "react";
import { getDeliveryCommitDraft, requestDeliveryApproval } from "../../delivery/client";
import type { DeliveryApproval, DeliveryCommitDraft, DeliveryTask } from "../../delivery/types";

import { ActionButton } from "./ActionButton";

/** Where a changed file goes: a commit's position, or this to stay uncommitted. */
const LEFT_OUT = -1;

/** A list of files longer than this starts folded, its count in sight. */
const FILES_SHOWN = 5;

function fileCount(count: number): string {
  return count === 1 ? "1 archivo" : `${count} archivos`;
}

/** The plan of a commit approval, exactly as Tinto will run it. */
export function CommitPlanView({ approval }: { approval: DeliveryApproval }) {
  const commits = approval.commits ?? [];
  const leftOut = approval.left_out ?? [];
  if (commits.length === 0) {
    // A request from before plans existed: one commit with every change.
    return (
      <pre className="delivery-banner__text">
        {approval.title}
        {approval.body ? `\n\n${approval.body}` : ""}
      </pre>
    );
  }
  return (
    <ol className="delivery-commit-plan" aria-label="Plan de commits">
      {commits.map((commit, index) => (
        <li key={index}>
          <pre className="delivery-banner__text">{commit.message}</pre>
          <details
            className="delivery-commit-plan__files"
            open={commit.paths.length <= FILES_SHOWN}
          >
            <summary>{fileCount(commit.paths.length)}</summary>
            <ul aria-label={`Archivos del commit ${index + 1}`}>
              {commit.paths.map((path) => (
                <li key={path}>
                  <code>{path}</code>
                </li>
              ))}
            </ul>
          </details>
        </li>
      ))}
      <li className="delivery-commit-plan__rest">
        {leftOut.length === 0 ? (
          <span className="delivery-muted">No queda ningún cambio sin incluir.</span>
        ) : (
          <details className="delivery-commit-plan__files" open={leftOut.length <= FILES_SHOWN}>
            <summary>Quedan sin incluir, a propósito: {fileCount(leftOut.length)}</summary>
            <ul aria-label="Archivos que quedan fuera">
              {leftOut.map((path) => (
                <li key={path}>
                  <code>{path}</code>
                </li>
              ))}
            </ul>
          </details>
        )}
      </li>
    </ol>
  );
}

/** Prepares the commit step: messages, and which commit each file goes in. */
export function CommitPlanForm({ task, onClose }: { task: DeliveryTask; onClose: () => void }) {
  const [draft, setDraft] = useState<DeliveryCommitDraft | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [messages, setMessages] = useState<string[]>([""]);
  const [placed, setPlaced] = useState<Record<string, number>>({});

  useEffect(() => {
    let current = true;
    getDeliveryCommitDraft(task.id).then(
      (loaded) => current && setDraft(loaded),
      (error: unknown) =>
        current && setFailure(error instanceof Error ? error.message : String(error)),
    );
    return () => {
      current = false;
    };
  }, [task.id]);

  if (failure) {
    return (
      <div className="delivery-request" role="alert">
        <p>No se pudieron leer los cambios de la tarea: {failure}</p>
        <div className="delivery-actions">
          <button type="button" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </div>
    );
  }
  if (!draft) return <p className="delivery-muted">Leyendo los cambios de la tarea…</p>;

  const placeOf = (path: string) => placed[path] ?? 0;
  const commits = messages.map((message, index) => ({
    message: message.trim(),
    paths: draft.files.filter((path) => placeOf(path) === index),
  }));
  const leaveOut = draft.files.filter((path) => placeOf(path) === LEFT_OUT);
  const ready = commits.every((commit) => commit.message && commit.paths.length > 0);
  const removeCommit = (index: number) => {
    setMessages(messages.filter((_, at) => at !== index));
    // Its files go to the first commit; later commits move up one place.
    setPlaced(
      Object.fromEntries(
        draft.files.map((path) => {
          const at = placeOf(path);
          return [path, at === index ? 0 : at > index ? at - 1 : at];
        }),
      ),
    );
  };

  return (
    <form
      className="delivery-request"
      aria-label="Pedir aprobación de commit"
      // The submit button sends it, also on Enter.
      onSubmit={(event) => event.preventDefault()}
    >
      {draft.recent.length > 0 && (
        <details className="delivery-decision__detail">
          <summary>Últimos commits de la rama, para seguir su convención</summary>
          <pre>{draft.recent.join("\n")}</pre>
        </details>
      )}
      {messages.map((message, index) => (
        <label key={index} className="delivery-request__field">
          <span>
            Mensaje del commit {index + 1}
            {messages.length > 1 &&
              ` · ${
                commits[index].paths.length === 1
                  ? "1 archivo"
                  : `${commits[index].paths.length} archivos`
              }`}
          </span>
          <textarea
            value={message}
            onChange={(event) =>
              setMessages(messages.map((old, at) => (at === index ? event.target.value : old)))
            }
            rows={2}
            placeholder={
              draft.recent[0]
                ? `Como en este repositorio, p. ej.: ${draft.recent[0]}`
                : "Asunto; opcionalmente, una línea en blanco y el detalle"
            }
            required
          />
        </label>
      ))}
      <div className="delivery-actions">
        <button type="button" onClick={() => setMessages([...messages, ""])}>
          Añadir otro commit
        </button>
        {messages.length > 1 && (
          <button type="button" onClick={() => removeCommit(messages.length - 1)}>
            Quitar el último commit
          </button>
        )}
      </div>
      <fieldset className="delivery-commit-files">
        <legend>Qué archivos van en cada commit</legend>
        {draft.files.length === 0 && (
          <p className="delivery-muted">No hay cambios que confirmar.</p>
        )}
        {draft.files.map((path) => (
          <label key={path}>
            <code>{path}</code>
            <select
              aria-label={`Destino de ${path}`}
              value={placeOf(path)}
              onChange={(event) => setPlaced({ ...placed, [path]: Number(event.target.value) })}
            >
              {messages.map((_, index) => (
                <option key={index} value={index}>
                  Commit {index + 1}
                </option>
              ))}
              <option value={LEFT_OUT}>Dejar sin commit</option>
            </select>
          </label>
        ))}
      </fieldset>
      <p className="delivery-muted">
        Tinto creará {messages.length === 1 ? "el commit" : `los ${messages.length} commits`} en
        este orden, con estos textos exactos y sin añadir firmas.{" "}
        {leaveOut.length > 0
          ? `Quedarán sin confirmar: ${leaveOut.join(", ")}.`
          : "No quedará ningún cambio sin confirmar."}
      </p>
      <div className="delivery-actions">
        <ActionButton
          type="submit"
          className="delivery-button--primary"
          disabled={!ready}
          onAction={async () => {
            await requestDeliveryApproval(task.id, "commit", "", "", { commits, leaveOut });
            onClose();
          }}
        >
          Pedir aprobación
        </ActionButton>
        <button type="button" onClick={onClose}>
          Cancelar
        </button>
      </div>
    </form>
  );
}
