// What the user settles before a task's stages run: choices, user-facing
// texts and QA permissions, asked by the coordinator and answered here.
// Answers are final; changing one is a new contract version.

import { useState, type ReactNode } from "react";
import {
  acceptRecommendedDecisions,
  answerDeliveryDecision,
  changeDeliveryDecision,
  renewDeliveryPermissions,
  withdrawDeliveryDecision,
} from "../../delivery/client";
import { confirm } from "../../workbench/confirmDialog";
import type { DeliveryDecision } from "../../delivery/types";
import { CodeThreads } from "./DeliveryCodeThread";

type Run = (action: () => Promise<unknown>) => Promise<boolean>;

const ALLOWED = "allowed";
const DENIED = "denied";

function who(id: string | null): string {
  if (id === "auto") return "aceptación automática";
  return !id || id === "user" ? "ti" : id;
}

function TechnicalDetail({ detail }: { detail: string }) {
  if (!detail.trim()) return null;
  return (
    <details className="delivery-decision__detail">
      <summary>Ver detalle técnico</summary>
      <pre>{detail}</pre>
    </details>
  );
}

/** Takes back a question that stopped making sense, so it stops blocking the task. */
function WithdrawButton({
  decision,
  busy,
  run,
}: {
  decision: DeliveryDecision;
  busy: boolean;
  run: Run;
}) {
  return (
    <button
      type="button"
      disabled={busy}
      aria-label={`Retirar la decisión «${decision.question}»`}
      title="Si ya no tiene sentido. Se quita sin respuesta y deja de bloquear las etapas."
      onClick={() =>
        void (async () => {
          const ok = await confirm(
            `«${decision.question}» se retira sin respuesta y deja de bloquear las etapas. Si vuelve a hacer falta, hay que pedirla otra vez.`,
            { title: "Retirar la decisión", okLabel: "Retirar" },
          );
          if (ok) await run(() => withdrawDeliveryDecision(decision.id));
        })()
      }
    >
      Retirar
    </button>
  );
}

function ChoiceCard({
  decision,
  busy,
  run,
}: {
  decision: DeliveryDecision;
  busy: boolean;
  run: Run;
}) {
  const recommended = decision.options.find((option) => option.recommended)?.label ?? null;
  const [choice, setChoice] = useState<string | null>(recommended);
  return (
    <fieldset className="delivery-decision">
      <legend>{decision.question}</legend>
      <CodeThreads threads={decision.references} />
      {decision.options.map((option) => (
        <label key={option.label} className="delivery-decision__option">
          <input
            type="radio"
            name={`decision-${decision.id}`}
            checked={choice === option.label}
            onChange={() => setChoice(option.label)}
          />
          <span>
            <span className="delivery-decision__label">
              {option.label}
              {option.recommended && <span className="delivery-badge">Recomendada</span>}
            </span>
            {option.consequence && <small>{option.consequence}</small>}
          </span>
        </label>
      ))}
      <TechnicalDetail detail={decision.detail} />
      <div className="delivery-actions">
        <button
          type="button"
          disabled={busy || choice === null}
          onClick={() => choice && void run(() => answerDeliveryDecision(decision.id, choice))}
        >
          Decidir
        </button>
        <WithdrawButton decision={decision} busy={busy} run={run} />
      </div>
    </fieldset>
  );
}

function TextCard({
  decision,
  busy,
  run,
}: {
  decision: DeliveryDecision;
  busy: boolean;
  run: Run;
}) {
  const [text, setText] = useState(decision.text);
  return (
    <fieldset className="delivery-decision">
      <legend>{decision.question}</legend>
      <CodeThreads threads={decision.references} />
      <label className="delivery-request__field">
        <span>Texto exacto, tal como lo verá el usuario</span>
        <textarea value={text} onChange={(event) => setText(event.target.value)} rows={2} />
      </label>
      <TechnicalDetail detail={decision.detail} />
      <div className="delivery-actions">
        <button
          type="button"
          disabled={busy || !text.trim()}
          onClick={() => void run(() => answerDeliveryDecision(decision.id, text))}
        >
          {text === decision.text ? "Aprobar texto" : "Aprobar texto editado"}
        </button>
        <WithdrawButton decision={decision} busy={busy} run={run} />
      </div>
    </fieldset>
  );
}

/** Exactly what a permission lets QA do: the command, the folders it may
 *  read and the frozen text of every script it runs. */
function PermissionScope({ decision }: { decision: DeliveryDecision }) {
  const scripts = decision.scripts ?? [];
  const readDirs = decision.read_dirs ?? [];
  const writeDirs = decision.write_dirs ?? [];
  const clients = (decision.clients ?? []).map((client) =>
    client === "claude" ? "Claude Code" : client === "codex" ? "Codex" : client,
  );
  const warnings = decision.warnings ?? [];
  const unfrozen = decision.unfrozen ?? [];
  return (
    <>
      {decision.command && (
        <p>
          La QA podrá ejecutar <code>{decision.command}</code>, con cualquier argumento.
        </p>
      )}
      {decision.full_access && (
        <p className="delivery-tone--danger">
          <strong>Acceso completo para la QA de esta tarea.</strong> Si lo permites, la QA que lance
          el coordinador correrá sin sandbox: podrá ejecutar cualquier comando y leer, modificar o
          borrar archivos fuera del worktree con tu usuario, sin pedir aprobación. Lo que sigue
          describe lo que dice que hará, no un límite. Las demás etapas siguen con su acceso normal.
        </p>
      )}
      {clients.length > 0 && (
        <p>
          Ese comando arranca {clients.length === 1 ? "tu cliente real" : "tus clientes reales"}{" "}
          <strong>{clients.join(" y ")}</strong>, con tu sesión; para arrancar{" "}
          {clients.length === 1 ? "necesita" : "necesitan"} escribir en su propia carpeta, incluida
          abajo.
        </p>
      )}
      {writeDirs.length > 0 && (
        <p>
          La QA podrá <strong>escribir y borrar</strong> en estas carpetas fuera del worktree, y en
          ninguna otra:{" "}
          {writeDirs.map((dir) => (
            <code key={dir}>{dir} </code>
          ))}
        </p>
      )}
      {warnings.map((warning) => (
        <p key={warning} className="delivery-tone--danger">
          Aviso: {warning}
        </p>
      ))}
      {readDirs.length > 0 && (
        <p>
          La QA podrá leer y listar, sin modificar:{" "}
          {readDirs.map((dir) => (
            <code key={dir}>{dir} </code>
          ))}
        </p>
      )}
      {scripts.map((script) => (
        <details key={script.path} className="delivery-decision__detail">
          <summary>Script congelado: {script.path}</summary>
          <pre>{script.content}</pre>
        </details>
      ))}
      {scripts.length > 0 && (
        <p className="delivery-muted">
          Tinto copió {scripts.length === 1 ? "este script" : "estos scripts"} al pedirse el
          permiso. La QA ejecuta esa copia, aunque el archivo cambie después.
        </p>
      )}
      {unfrozen.length > 0 && (
        <p className="delivery-tone--danger">
          {scripts.length === 1 ? "El script usa" : "Los scripts usan"} archivos que los agentes
          pueden cambiar y que no están congelados:{" "}
          {unfrozen.map((path) => (
            <code key={path}>{path} </code>
          ))}
        </p>
      )}
      {decision.runs_candidate && (
        <p className="delivery-muted">
          Ejecuta el código del worktree de la tarea. El permiso vale para el código tal como esté
          cuando lo permitas: si cambia, la QA no arranca hasta que vuelvas a permitirlo. Los
          archivos que git ignora (como node_modules) no se comprueban.
        </p>
      )}
    </>
  );
}

function PermissionCard({
  decision,
  busy,
  run,
}: {
  decision: DeliveryDecision;
  busy: boolean;
  run: Run;
}) {
  return (
    <fieldset className="delivery-decision delivery-decision--permission">
      <legend>{decision.question}</legend>
      <CodeThreads threads={decision.references} />
      <PermissionScope decision={decision} />
      <p className="delivery-muted">Cómo se deshace: {decision.undo}</p>
      <TechnicalDetail detail={decision.detail} />
      <div className="delivery-actions">
        <button
          type="button"
          disabled={busy}
          onClick={() => void run(() => answerDeliveryDecision(decision.id, ALLOWED))}
        >
          Permitir
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void run(() => answerDeliveryDecision(decision.id, DENIED))}
        >
          No permitir
        </button>
        <WithdrawButton decision={decision} busy={busy} run={run} />
      </div>
    </fieldset>
  );
}

/** The pending decisions of a task, grouped by kind. */
export function PendingDecisions({
  taskId,
  decisions,
  busy,
  run,
}: {
  taskId: string;
  decisions: DeliveryDecision[];
  busy: boolean;
  run: Run;
}) {
  const choices = decisions.filter((decision) => decision.kind === "choice");
  const texts = decisions.filter((decision) => decision.kind === "text");
  const permissions = decisions.filter((decision) => decision.kind === "permission");
  const acceptable =
    texts.length + choices.filter((decision) => decision.options.some((o) => o.recommended)).length;
  const askers = [...new Set(decisions.map((decision) => who(decision.requested_by)))];
  return (
    <section className="delivery-banner delivery-decisions" aria-label="Decisiones pendientes">
      <p className="delivery-banner__title">
        <strong>Esperando tus decisiones</strong>
        <span className="delivery-muted">
          {decisions.length === 1 ? "1 pendiente" : `${decisions.length} pendientes`} · pedidas por{" "}
          {askers.join(", ")}
        </span>
      </p>
      <p className="delivery-muted">
        Hasta que las contestes no se lanza ninguna etapa. Tus respuestas llegan a todos los
        trabajos de la tarea.
      </p>
      {acceptable > 0 && (
        <div className="delivery-actions">
          <button
            type="button"
            className="delivery-button--primary"
            disabled={busy}
            onClick={() => void run(() => acceptRecommendedDecisions(taskId))}
          >
            Aceptar recomendadas
          </button>
          {permissions.length > 0 && (
            <span className="delivery-muted">Los permisos se deciden uno a uno.</span>
          )}
        </div>
      )}
      {choices.length > 0 && <h4 className="delivery-decisions__group">Decisiones</h4>}
      {choices.map((decision) => (
        <ChoiceCard key={decision.id} decision={decision} busy={busy} run={run} />
      ))}
      {texts.length > 0 && (
        <h4 className="delivery-decisions__group">Textos que verá el usuario</h4>
      )}
      {texts.map((decision) => (
        <TextCard key={decision.id} decision={decision} busy={busy} run={run} />
      ))}
      {permissions.length > 0 && (
        <h4 className="delivery-decisions__group">Permisos fuera del worktree</h4>
      )}
      {permissions.map((decision) => (
        <PermissionCard key={decision.id} decision={decision} busy={busy} run={run} />
      ))}
    </section>
  );
}

function decidedOn(ms: number | null): string {
  return ms ? new Date(ms).toLocaleString([], { dateStyle: "short", timeStyle: "short" }) : "";
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** Who decided when, said once when it is the same for every decision. */
function byLine(decisions: DeliveryDecision[]): string | null {
  const people = new Set(decisions.map((decision) => who(decision.decided_by)));
  const days = new Set(
    decisions.map((decision) =>
      decision.decided_at_ms ? new Date(decision.decided_at_ms).toDateString() : "",
    ),
  );
  if (people.size !== 1 || days.size !== 1) return null;
  const last = Math.max(...decisions.map((decision) => decision.decided_at_ms ?? 0));
  return `decididas por ${[...people][0]} el ${decidedOn(last)}`;
}

function LogRow({
  decision,
  shared,
  children,
}: {
  decision: DeliveryDecision;
  shared: boolean;
  children: ReactNode;
}) {
  return (
    <li>
      <span className="delivery-decision-log__question">{decision.question}</span>
      <span className="delivery-decision-log__answer">
        {children}
        {!shared && (
          <small className="delivery-muted">
            {who(decision.decided_by)} · {decidedOn(decision.decided_at_ms)}
          </small>
        )}
      </span>
    </li>
  );
}

/** One call the coordinator made on its own, with the way to change it. */
function CoordinatorDecision({
  decision,
  busy,
  run,
}: {
  decision: DeliveryDecision;
  busy: boolean;
  run: Run;
}) {
  const [changing, setChanging] = useState(false);
  const [answer, setAnswer] = useState(decision.answer ?? "");
  const changed = decision.decided_by === "user";
  return (
    <li className="delivery-coordinator-decision">
      <span className="delivery-decision-log__question">{decision.question}</span>
      {!changing ? (
        <span className="delivery-decision-log__answer">
          {decision.kind === "text" ? (
            <pre className="delivery-decision-log__text">{decision.answer}</pre>
          ) : (
            <strong>{decision.answer}</strong>
          )}
          {changed && <span className="delivery-badge delivery-badge--muted">cambiado por ti</span>}
          <button
            type="button"
            className="delivery-link-button"
            disabled={busy}
            aria-label={`Cambiar «${decision.question}»`}
            onClick={() => setChanging(true)}
          >
            Cambiar
          </button>
        </span>
      ) : (
        <span className="delivery-decision-log__answer">
          {decision.kind === "choice" ? (
            decision.options.map((option) => (
              <label key={option.label} className="delivery-decision__option">
                <input
                  type="radio"
                  name={`change-${decision.id}`}
                  checked={answer === option.label}
                  onChange={() => setAnswer(option.label)}
                />
                <span>
                  <span className="delivery-decision__label">{option.label}</span>
                  {option.consequence && <small>{option.consequence}</small>}
                </span>
              </label>
            ))
          ) : (
            <textarea
              aria-label="Texto"
              value={answer}
              onChange={(event) => setAnswer(event.target.value)}
              rows={2}
            />
          )}
          <span className="delivery-actions">
            <button
              type="button"
              disabled={busy || !answer.trim() || answer === decision.answer}
              onClick={() =>
                void run(() => changeDeliveryDecision(decision.id, answer)).then((ok) => {
                  if (ok) setChanging(false);
                })
              }
            >
              Guardar
            </button>
            <button type="button" onClick={() => setChanging(false)}>
              Cancelar
            </button>
          </span>
        </span>
      )}
      <TechnicalDetail detail={decision.detail} />
    </li>
  );
}

/** What the coordinator settled on its own: it does not hold the stages,
 *  and any of it can be changed; later stages get the new answer. */
export function CoordinatorDecisions({
  decisions,
  busy,
  run,
}: {
  decisions: DeliveryDecision[];
  busy: boolean;
  run: Run;
}) {
  if (decisions.length === 0) return null;
  return (
    <section className="delivery-section" aria-label="Decidido por el coordinador">
      <details className="delivery-decision-log">
        <summary>
          <h3>Decidido por el coordinador</h3>
          <span className="delivery-muted">
            {plural(decisions.length, "decisión", "decisiones")} · no frenan las etapas; revísalas
            cuando quieras
          </span>
        </summary>
        <p className="delivery-muted">
          Si cambias una, las etapas que se lancen después siguen tu respuesta. Lo ya hecho no se
          rehace solo.
        </p>
        <ul>
          {decisions.map((decision) => (
            <CoordinatorDecision key={decision.id} decision={decision} busy={busy} run={run} />
          ))}
        </ul>
      </details>
    </section>
  );
}

/** What was decided before the stages ran, folded by default: a summary
 *  line, then choices, approved texts and permissions apart. */
export function DecisionLog({
  taskId,
  decisions,
  candidate,
  busy,
  run,
}: {
  taskId: string;
  decisions: DeliveryDecision[];
  /** The task's code (tree id) as its latest stage saw it; null when unknown. */
  candidate: string | null;
  busy: boolean;
  run: Run;
}) {
  if (decisions.length === 0) return null;
  const choices = decisions.filter((decision) => decision.kind === "choice");
  const texts = decisions.filter((decision) => decision.kind === "text");
  const permissions = decisions.filter((decision) => decision.kind === "permission");
  const by = byLine(decisions);
  // Permissions that run the task's code and were allowed for other code.
  const outdated = permissions.filter(
    (decision) =>
      decision.runs_candidate && decision.answer === ALLOWED && decision.candidate !== candidate,
  );
  const counts = [
    choices.length > 0 && plural(choices.length, "elección", "elecciones"),
    texts.length > 0 && plural(texts.length, "texto", "textos"),
    permissions.length > 0 && plural(permissions.length, "permiso", "permisos"),
  ].filter(Boolean);
  return (
    <section className="delivery-section" aria-label="Decisiones">
      <details className="delivery-decision-log">
        <summary>
          <h3>Decisiones</h3>
          <span className="delivery-muted">
            {plural(decisions.length, "tomada", "tomadas")} · {counts.join(", ")}
            {by ? ` · ${by}` : ""}
          </span>
        </summary>
        {choices.length > 0 && (
          <>
            <h4 className="delivery-decisions__group">Elecciones</h4>
            <ul>
              {choices.map((decision) => {
                const recommended = decision.options.find((option) => option.recommended);
                return (
                  <LogRow key={decision.id} decision={decision} shared={by !== null}>
                    <strong>{decision.answer}</strong>
                    {recommended && recommended.label !== decision.answer && (
                      <span className="delivery-badge delivery-badge--muted">
                        no era la recomendada
                      </span>
                    )}
                  </LogRow>
                );
              })}
            </ul>
          </>
        )}
        {texts.length > 0 && (
          <>
            <h4 className="delivery-decisions__group">Textos aprobados</h4>
            <ul>
              {texts.map((decision) => (
                <LogRow key={decision.id} decision={decision} shared={by !== null}>
                  <pre className="delivery-decision-log__text">{decision.answer}</pre>
                  {decision.answer !== decision.text && (
                    <span className="delivery-badge delivery-badge--muted">editado</span>
                  )}
                </LogRow>
              ))}
            </ul>
          </>
        )}
        {permissions.length > 0 && (
          <>
            <h4 className="delivery-decisions__group">Permisos</h4>
            {outdated.length > 0 && (
              <p className="delivery-actions">
                <span className="delivery-tone--attention">
                  {outdated.length === 1
                    ? "El código cambió desde que permitiste 1 permiso que lo ejecuta"
                    : `El código cambió desde que permitiste ${outdated.length} permisos que lo ejecutan`}
                  : la QA no arranca hasta que vuelvas a permitirlos.
                </span>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void run(() => renewDeliveryPermissions(taskId))}
                >
                  Volver a permitir para el código actual
                </button>
              </p>
            )}
            <ul>
              {permissions.map((decision) => {
                const allowed = decision.answer === ALLOWED;
                return (
                  <LogRow key={decision.id} decision={decision} shared={by !== null}>
                    <strong className={allowed ? "delivery-tone--ok" : "delivery-tone--danger"}>
                      {allowed ? "✓ Permitido" : "✕ No permitido"}
                    </strong>
                    {allowed ? (
                      <PermissionScope decision={decision} />
                    ) : (
                      decision.command && <code>{decision.command}</code>
                    )}
                    {allowed && decision.runs_candidate && (
                      <small className="delivery-muted">
                        Permitido para el código {decision.candidate?.slice(0, 8)}
                        {outdated.includes(decision) ? ", que ya cambió." : ", el actual."}
                      </small>
                    )}
                  </LogRow>
                );
              })}
            </ul>
          </>
        )}
      </details>
    </section>
  );
}
