// What the user settles before a task's stages run: choices, user-facing
// texts and QA permissions, asked by the coordinator and answered here.
// Answers are final; changing one is a new contract version.

import { useState, type ReactNode } from "react";
import { acceptRecommendedDecisions, answerDeliveryDecision } from "../../delivery/client";
import type { DeliveryDecision } from "../../delivery/types";

type Run = (action: () => Promise<unknown>) => Promise<boolean>;

const ALLOWED = "allowed";
const DENIED = "denied";

function who(id: string | null): string {
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
      </div>
    </fieldset>
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
      {decision.command && (
        <p>
          La QA podrá ejecutar <code>{decision.command}</code>.
        </p>
      )}
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

/** What was decided before the stages ran, folded by default: a summary
 *  line, then choices, approved texts and permissions apart. */
export function DecisionLog({ decisions }: { decisions: DeliveryDecision[] }) {
  if (decisions.length === 0) return null;
  const choices = decisions.filter((decision) => decision.kind === "choice");
  const texts = decisions.filter((decision) => decision.kind === "text");
  const permissions = decisions.filter((decision) => decision.kind === "permission");
  const by = byLine(decisions);
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
            <ul>
              {permissions.map((decision) => {
                const allowed = decision.answer === ALLOWED;
                return (
                  <LogRow key={decision.id} decision={decision} shared={by !== null}>
                    <strong className={allowed ? "delivery-tone--ok" : "delivery-tone--danger"}>
                      {allowed ? "✓ Permitido" : "✕ No permitido"}
                    </strong>
                    {decision.command && <code>{decision.command}</code>}
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
