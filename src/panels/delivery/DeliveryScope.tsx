// Alcance, the task's scoping stage: what its agent is doing while it runs,
// then the task's plan in fixed sections, its earlier versions, and the way
// to launch or skip it.

import { useState } from "react";
import { getDeliveryTaskChanges, skipDeliveryScope } from "../../delivery/client";
import type {
  DeliveryDecision,
  DeliveryJob,
  DeliveryScopePlan,
  DeliveryTask,
} from "../../delivery/types";
import { useWorkspaceActions } from "../../workspace/actions";
import { ActionButton } from "./ActionButton";
import { errorText } from "./errorText";
import { isOpen, latestOfRole, SCOPE_ROLE, scopeActivity } from "./taskStatus";
import { useJobLog } from "./useJobLog";

function when(ms: number): string {
  return new Date(ms).toLocaleString([], { dateStyle: "short", timeStyle: "short" });
}

/** Who decided a question, as the plan names it. */
function decider(decision: DeliveryDecision): string {
  if (decision.status !== "answered") return "sin contestar";
  if (decision.decided_by === "auto") return "automática";
  if (decision.decided_by === "user" || !decision.decided_by) return "tú";
  return decision.blocking === false ? "coordinador" : decision.decided_by;
}

/** Scrolls to the task's decisions and opens them. */
function showDecisions(from: HTMLElement) {
  const article = from.closest("article") ?? document;
  const section = article.querySelector<HTMLElement>(
    'section[aria-label="Decisiones"], section[aria-label="Decidido por el coordinador"]',
  );
  section?.querySelector("details")?.setAttribute("open", "");
  section?.scrollIntoView?.({ block: "nearest" });
}

/** What the running Alcance job does, from its log. */
function ScopeLive({ task, job }: { task: DeliveryTask; job: DeliveryJob }) {
  const log = useJobLog(job);
  const seen = scopeActivity(log?.entries ?? [], task.worktree);
  const read = seen.files.reduce((sum, group) => sum + group.paths.length, 0);
  return (
    <div className="delivery-scope__live" aria-label="Actividad del alcance">
      <p className="delivery-job__activity">
        {job.current_command ?? job.activity ?? "Empezando a leer el código…"}
      </p>
      {read > 0 && (
        <details open>
          <summary>
            {read === 1 ? "1 archivo leído" : `${read} archivos leídos`} ·{" "}
            {seen.files.map((group) => group.group).join(", ")}
          </summary>
          <ul>
            {seen.files.map((group) => (
              <li key={group.group}>
                <strong>{group.group}</strong>: {group.paths.join(", ")}
              </li>
            ))}
          </ul>
        </details>
      )}
      {seen.searches.length > 0 && (
        <ul aria-label="Búsquedas">
          {seen.searches.map((search) => (
            <li key={search}>{search}</li>
          ))}
        </ul>
      )}
      {seen.commands.length > 0 && (
        <ul aria-label="Comandos">
          {seen.commands.map((command, index) => (
            <li key={index}>
              <code>{command.command}</code>
              {command.outcome && ` → ${command.outcome}`}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const RISK_LABEL: Record<string, string> = { bajo: "bajo", medio: "medio", alto: "alto" };

/** One version of the plan, in its seven sections. */
function PlanSections({
  task,
  plan,
  decisions,
}: {
  task: DeliveryTask;
  plan: DeliveryScopePlan;
  decisions: DeliveryDecision[];
}) {
  const { openFile } = useWorkspaceActions();
  const [error, setError] = useState<string | null>(null);
  const open = async (path: string) => {
    setError(null);
    try {
      // The worktree as a repo of the files view.
      const changes = await getDeliveryTaskChanges(task.id);
      openFile(changes.repo, path);
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  const asked = decisions.filter((decision) => decision.kind !== "permission");
  const none = <p className="delivery-muted">—</p>;
  return (
    <div className="delivery-scope__plan">
      <h4>Qué cambia</h4>
      <p className="delivery-scope__what">{plan.what}</p>
      <h4>Dónde</h4>
      {plan.places?.length ? (
        <ul>
          {plan.places.map((place) => (
            <li key={place.path}>
              <button
                type="button"
                className="delivery-link-button"
                title={`Abrir ${place.path} en el worktree de la tarea`}
                onClick={() => void open(place.path)}
              >
                {place.path}
              </button>
              {place.note && <span className="delivery-muted"> — {place.note}</span>}
            </li>
          ))}
        </ul>
      ) : (
        none
      )}
      {error && <p className="delivery-tone--danger">{error}</p>}
      <h4>
        Riesgos
        {plan.risk_level && (
          <small> · nivel {RISK_LABEL[plan.risk_level] ?? plan.risk_level}</small>
        )}
      </h4>
      {plan.risks?.length ? (
        <ul>
          {plan.risks.map((risk) => (
            <li key={risk}>{risk}</li>
          ))}
        </ul>
      ) : (
        none
      )}
      <h4>Pruebas</h4>
      {plan.tests?.length ? (
        <table className="delivery-scope__tests">
          <thead>
            <tr>
              <th>Requisito</th>
              <th>Capa</th>
              <th>Archivo</th>
            </tr>
          </thead>
          <tbody>
            {plan.tests.map((test, index) => (
              <tr key={index}>
                <td>{test.requirement}</td>
                <td>{test.layer || "—"}</td>
                <td>{test.file ? <code>{test.file}</code> : "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        none
      )}
      <h4>Comprobaciones</h4>
      {task.checks?.length ? (
        <ul>
          {task.checks.map((check) => (
            <li key={check}>
              <code>{check}</code>
            </li>
          ))}
        </ul>
      ) : (
        <p className="delivery-muted">Ninguna fijada en la tarea.</p>
      )}
      <h4>Preguntas</h4>
      {asked.length ? (
        <ul>
          {asked.map((decision) => (
            <li key={decision.id}>
              {decision.question} <strong>{decision.answer ?? ""}</strong>{" "}
              <small className="delivery-muted">· {decider(decision)}</small>{" "}
              <button
                type="button"
                className="delivery-link-button"
                onClick={(event) => showDecisions(event.currentTarget)}
              >
                Cambiar en Decisiones
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="delivery-muted">Sin preguntas.</p>
      )}
      <h4>Fuera de alcance</h4>
      {plan.out_of_scope?.length ? (
        <ul>
          {plan.out_of_scope.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      ) : (
        none
      )}
    </div>
  );
}

function authorLine(plan: DeliveryScopePlan): string {
  return plan.author === "agent"
    ? "Escrito tras explorar el código"
    : "Escrito por el coordinador sin explorar";
}

function SkipScope({ task, onDone }: { task: DeliveryTask; onDone: () => void }) {
  const [reason, setReason] = useState("");
  return (
    <form
      className="delivery-scope__skip"
      onSubmit={(event) => event.preventDefault()}
      aria-label="Omitir alcance"
    >
      <label>
        Motivo
        <input
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          placeholder="cambio de una línea, el issue ya dice qué tocar"
        />
      </label>
      <ActionButton
        disabled={!reason.trim()}
        onAction={async () => {
          await skipDeliveryScope(task.id, reason);
          onDone();
        }}
      >
        Omitir
      </ActionButton>
      <button type="button" onClick={onDone}>
        Cancelar
      </button>
    </form>
  );
}

/** The task's Alcance: live activity, the plan and its versions, or why it
 *  was skipped; with the way to launch or skip it. */
export function ScopeSection({
  task,
  jobs,
  decisions,
  blocked,
  onLaunch,
}: {
  task: DeliveryTask;
  jobs: DeliveryJob[];
  /** The task's decisions: its plan's questions. */
  decisions: DeliveryDecision[];
  /** Stages cannot be launched now (pending decisions). */
  blocked: boolean;
  onLaunch: () => void;
}) {
  const [skipping, setSkipping] = useState(false);
  const job = latestOfRole(jobs, SCOPE_ROLE);
  const running = job && isOpen(job) ? job : null;
  const plans = task.plans ?? [];
  const plan = plans.at(-1);
  const earlier = plans.slice(0, -1).reverse();
  // Before any other stage, it can still run or be skipped.
  const started = jobs.some((other) => other.agent !== "shell");
  const offer = !running && !plan && !task.scope_skip && !started;
  return (
    <section className="delivery-section delivery-scope" aria-label="Alcance">
      <h3>
        Alcance{" "}
        {plan && (
          <small className="delivery-muted">
            v{plan.version} · {authorLine(plan)} · {when(plan.at_ms)}
          </small>
        )}
      </h3>
      {task.scope_skip && !running && (
        <p className="delivery-scope__skipped">
          Omitido: {task.scope_skip.reason}{" "}
          <small className="delivery-muted">
            · por {task.scope_skip.by === "user" ? "ti" : "el coordinador"}
          </small>
        </p>
      )}
      {running && <ScopeLive task={task} job={running} />}
      {plan ? (
        <>
          {plan.reason && (
            <p className="delivery-muted">
              Cambió el {when(plan.at_ms)}: {plan.reason}
              {plan.changed ? ` · ${plan.changed}` : ""}
            </p>
          )}
          <PlanSections task={task} plan={plan} decisions={decisions} />
        </>
      ) : (
        !running &&
        (job || task.scope_skip || started) && (
          <div className="delivery-scope__missing">
            <p className="delivery-muted">Plan no registrado en Tinto.</p>
            {job?.result?.summary && <p>{job.result.summary}</p>}
          </div>
        )
      )}
      {earlier.length > 0 && (
        <details className="delivery-scope__versions">
          <summary>Versiones anteriores ({earlier.length})</summary>
          {earlier.map((version) => (
            <details key={`${version.version}-${version.at_ms}`}>
              <summary>
                v{version.version} · {when(version.at_ms)}
                {version.reason ? ` · ${version.reason}` : ""} · {authorLine(version)}
              </summary>
              <PlanSections task={task} plan={version} decisions={decisions} />
            </details>
          ))}
        </details>
      )}
      {offer && !skipping && (
        <div className="delivery-actions">
          <button type="button" disabled={blocked} onClick={onLaunch}>
            Lanzar alcance
          </button>
          <button type="button" onClick={() => setSkipping(true)}>
            Omitir alcance
          </button>
        </div>
      )}
      {offer && skipping && <SkipScope task={task} onDone={() => setSkipping(false)} />}
    </section>
  );
}
