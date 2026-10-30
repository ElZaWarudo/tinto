// What a blocked task waits for (another task, a PR, a decision, a note).
// Tinto follows each one and clears it when it resolves; the user can also
// remove one, override the block, or add a blocker of their own.

import { useState } from "react";
import { addDeliveryBlocker, clearDeliveryBlockers } from "../../delivery/client";
import type {
  DeliveryBlocker,
  DeliveryTask,
  DeliveryTaskMilestone,
  NewDeliveryBlocker,
} from "../../delivery/types";
import { confirm } from "../../workbench/confirmDialog";

import { ActionButton } from "./ActionButton";
import { ExternalLink } from "./ExternalLink";
import { blockerLabel, openBlockers } from "./taskStatus";

/** The blocker's name, linked to what it waits for when Tinto can open it. */
function BlockerName({
  blocker,
  tasks,
  onSelectTask,
}: {
  blocker: DeliveryBlocker;
  tasks: DeliveryTask[];
  onSelectTask: (task: DeliveryTask) => void;
}) {
  const label = blockerLabel(blocker);
  if (blocker.type === "pr") {
    return (
      <ExternalLink href={blocker.url} label={`Abrir ${label} en GitHub`}>
        {label} ↗
      </ExternalLink>
    );
  }
  const target =
    blocker.type === "task" ? tasks.find((task) => task.id === blocker.task_id) : undefined;
  if (target) {
    return (
      <button type="button" className="delivery-link-button" onClick={() => onSelectTask(target)}>
        {label}
      </button>
    );
  }
  return <span>{label}</span>;
}

/** The line on a blocked task: each open blocker, and the user's ways out. */
export function BlockedBar({
  task,
  tasks,
  onSelectTask,
  onAdd,
}: {
  task: DeliveryTask;
  tasks: DeliveryTask[];
  onSelectTask: (task: DeliveryTask) => void;
  onAdd: (from: Element) => void;
}) {
  const open = openBlockers(task);
  if (open.length === 0) return null;
  const override = async () => {
    const ok = await confirm(
      `${task.key} deja de esperar ${open.length === 1 ? "su bloqueo" : `sus ${open.length} bloqueos`} y vuelve a donde estaba. Se avisa al coordinador.`,
      { title: "Desbloquear tarea", kind: "warning", okLabel: "Desbloquear" },
    );
    if (!ok) return false;
    await clearDeliveryBlockers(task.id);
  };
  return (
    <div
      className="delivery-pending-bar delivery-blocked"
      role="group"
      aria-label="Bloqueos de la tarea"
    >
      <div className="delivery-blocked__list">
        <strong>Bloqueada por</strong>
        <ul>
          {open.map((blocker) => (
            <li key={blocker.id}>
              <BlockerName blocker={blocker} tasks={tasks} onSelectTask={onSelectTask} />
              {blocker.status && <span className="delivery-muted"> · {blocker.status}</span>}
              <ActionButton
                className="delivery-link-button"
                aria-label={`Quitar el bloqueo ${blockerLabel(blocker)}`}
                onAction={() => clearDeliveryBlockers(task.id, [blocker.id])}
              >
                Quitar
              </ActionButton>
            </li>
          ))}
        </ul>
        {task.blocked_reason && <p className="delivery-muted">{task.blocked_reason}</p>}
      </div>
      <div className="delivery-actions">
        <button type="button" onClick={(event) => onAdd(event.currentTarget)}>
          Añadir bloqueo
        </button>
        <ActionButton onAction={override}>Desbloquear</ActionButton>
      </div>
    </div>
  );
}

type Kind = "task" | "pr" | "note";

/** Adds a blocker: a task by key, a PR by URL, or a note. */
export function AddBlocker({ task, onDone }: { task: DeliveryTask; onDone: () => void }) {
  const [kind, setKind] = useState<Kind>("task");
  const [text, setText] = useState("");
  const [until, setUntil] = useState<DeliveryTaskMilestone>("merged");
  const value = text.trim();
  const blocker = (): NewDeliveryBlocker =>
    kind === "task"
      ? { type: "task", task: value, until }
      : kind === "pr"
        ? { type: "pr", url: value }
        : { type: "note", text: value };
  return (
    <form
      className="delivery-add-blocker"
      aria-label="Añadir bloqueo"
      onSubmit={(event) => event.preventDefault()}
    >
      <h3>Bloquear {task.key}</h3>
      <label>
        Espera a
        <select value={kind} onChange={(event) => setKind(event.target.value as Kind)}>
          <option value="task">otra tarea</option>
          <option value="pr">un PR de GitHub</option>
          <option value="note">algo fuera (nota)</option>
        </select>
      </label>
      <label>
        {kind === "task" ? "Tarea (clave)" : kind === "pr" ? "URL del PR" : "Qué espera"}
        <input
          value={text}
          maxLength={kind === "note" ? 200 : undefined}
          placeholder={
            kind === "task"
              ? "AGOS-580"
              : kind === "pr"
                ? "https://github.com/owner/repo/pull/305"
                : "El equipo de plataforma despliega el bucket"
          }
          onChange={(event) => setText(event.target.value)}
        />
      </label>
      {kind === "task" && (
        <label>
          Hasta que
          <select
            value={until}
            onChange={(event) => setUntil(event.target.value as DeliveryTaskMilestone)}
          >
            <option value="pr_opened">abra su PR</option>
            <option value="merged">se fusione</option>
            <option value="delivered">se entregue</option>
          </select>
        </label>
      )}
      <div className="delivery-actions">
        <ActionButton
          className="delivery-button--primary"
          disabled={!value}
          onAction={async () => {
            await addDeliveryBlocker(task.id, blocker());
            onDone();
          }}
        >
          Bloquear
        </ActionButton>
      </div>
    </form>
  );
}
