// The problems QA found in a task (and the ones the user adds): the user
// picks a fix for each (QA's suggestion or their own), or says it is not a
// problem, and sends the chosen ones to the coordinator together.

import { useState } from "react";
import {
  dismissDeliveryQaProblem,
  reportDeliveryQaProblem,
  sendDeliveryQaProblems,
} from "../../delivery/client";
import type { DeliveryQaProblem, DeliveryQaProblemStatus } from "../../delivery/types";

import { ActionButton } from "./ActionButton";

const STATUS: Record<DeliveryQaProblemStatus, { label: string; tone: string }> = {
  open: { label: "nuevo", tone: "new" },
  sent: { label: "enviado al coordinador", tone: "sent" },
  fixing: { label: "en arreglo", tone: "fixing" },
  verified: { label: "comprobado por la QA", tone: "verified" },
  still_failing: { label: "sigue fallando", tone: "failing" },
  dismissed: { label: "no es un problema", tone: "dismissed" },
  superseded: { label: "sustituido por otra QA", tone: "dismissed" },
};

/** Problems the user still has something to do or watch about. */
function current(problem: DeliveryQaProblem): boolean {
  return !["verified", "dismissed", "superseded"].includes(problem.status);
}

/** What the user picked for one open problem: a suggestion's index, or their words. */
type Choice = { picked: number | "own"; own: string; included: boolean };

export function QaProblems({
  taskId,
  problems,
}: {
  taskId: string;
  problems: DeliveryQaProblem[];
}) {
  const [showAll, setShowAll] = useState(false);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [dismissing, setDismissing] = useState<string | null>(null);
  const [reason, setReason] = useState("");
  const [report, setReport] = useState("");
  const [reportPlace, setReportPlace] = useState("");
  if (problems.length === 0) return null;
  const shown = showAll ? problems : problems.filter(current);
  const choiceOf = (problem: DeliveryQaProblem): Choice =>
    choices[problem.id] ?? {
      picked: problem.suggestions.length > 0 ? 0 : "own",
      own: "",
      included: false,
    };
  const solutionOf = (problem: DeliveryQaProblem): string => {
    const choice = choiceOf(problem);
    return choice.picked === "own" ? choice.own.trim() : (problem.suggestions[choice.picked] ?? "");
  };
  const update = (problem: DeliveryQaProblem, change: Partial<Choice>) =>
    setChoices((all) => ({ ...all, [problem.id]: { ...choiceOf(problem), ...change } }));
  const ready = problems.filter(
    (problem) =>
      problem.status === "open" && choiceOf(problem).included && solutionOf(problem) !== "",
  );
  const send = async () => {
    await sendDeliveryQaProblems(
      taskId,
      ready.map((problem) => ({ problem_id: problem.id, solution: solutionOf(problem) })),
    );
    setChoices({});
  };

  return (
    <section className="delivery-qa-problems" aria-label="Problemas de la QA">
      <header className="delivery-qa-problems__head">
        <h3>Problemas de la QA</h3>
        <span className="delivery-muted">#n en Tinto · P1, P2… en el registro visual</span>
        <span className="delivery-qa-problems__spacer" />
        <div className="delivery-qa-problems__filters" role="group" aria-label="Mostrar">
          <button type="button" aria-pressed={!showAll} onClick={() => setShowAll(false)}>
            Pendientes ({problems.filter(current).length})
          </button>
          <button type="button" aria-pressed={showAll} onClick={() => setShowAll(true)}>
            Todos ({problems.length})
          </button>
        </div>
      </header>

      {shown.map((problem) => {
        const open = problem.status === "open";
        const choice = choiceOf(problem);
        const status = STATUS[problem.status] ?? STATUS.open;
        return (
          <article
            key={problem.id}
            className="delivery-qa-problem"
            aria-label={`Problema #${problem.number}`}
          >
            {open ? (
              <input
                type="checkbox"
                aria-label={`Enviar el problema #${problem.number}`}
                checked={choice.included}
                onChange={(event) => update(problem, { included: event.target.checked })}
              />
            ) : (
              <span />
            )}
            <div>
              <p className="delivery-qa-problem__title">
                <span className="delivery-qa-problem__number">#{problem.number}</span>
                <strong>{problem.text}</strong>
                <span className={`delivery-qa-chip delivery-qa-chip--${status.tone}`}>
                  {status.label}
                </span>
              </p>
              <p className="delivery-muted delivery-qa-problem__where">
                {[
                  problem.change,
                  problem.place,
                  problem.job_id
                    ? `${problem.label} de la QA intento ${problem.attempt ?? "?"}`
                    : "lo reportaste tú",
                ]
                  .filter(Boolean)
                  .join(" · ")}
              </p>

              {open ? (
                <>
                  <div
                    className="delivery-qa-problem__solutions"
                    role="radiogroup"
                    aria-label={`Solución del problema #${problem.number}`}
                  >
                    {problem.suggestions.map((suggestion, index) => (
                      <label key={index}>
                        <input
                          type="radio"
                          name={`qa-${problem.id}`}
                          checked={choice.picked === index}
                          onChange={() => update(problem, { picked: index, included: true })}
                        />
                        <span>
                          {suggestion}
                          {index === 0 && (
                            <span className="delivery-badge">Sugerida por la QA</span>
                          )}
                        </span>
                      </label>
                    ))}
                    <label>
                      <input
                        type="radio"
                        name={`qa-${problem.id}`}
                        checked={choice.picked === "own"}
                        onChange={() => update(problem, { picked: "own", included: true })}
                      />
                      <span>Otra solución…</span>
                    </label>
                  </div>
                  {choice.picked === "own" && (
                    <textarea
                      className="delivery-qa-problem__own"
                      aria-label={`Tu solución para el problema #${problem.number}`}
                      placeholder="Cómo debería quedar: el coordinador lo recibe tal cual y la QA comprueba exactamente esto."
                      value={choice.own}
                      onChange={(event) =>
                        update(problem, { own: event.target.value, included: true })
                      }
                      rows={2}
                    />
                  )}
                  <div className="delivery-actions">
                    {problem.suggestions.length > 0 && (
                      <button
                        type="button"
                        onClick={() => update(problem, { picked: 0, included: true })}
                      >
                        Aplicar solución sugerida
                      </button>
                    )}
                    {dismissing === problem.id ? (
                      <>
                        <input
                          aria-label={`Por qué el problema #${problem.number} no es un problema`}
                          placeholder="Por qué no es un problema"
                          value={reason}
                          onChange={(event) => setReason(event.target.value)}
                        />
                        <ActionButton
                          disabled={!reason.trim()}
                          onAction={async () => {
                            await dismissDeliveryQaProblem(problem.id, reason);
                            setDismissing(null);
                            setReason("");
                          }}
                        >
                          Descartar
                        </ActionButton>
                        <button type="button" onClick={() => setDismissing(null)}>
                          Cancelar
                        </button>
                      </>
                    ) : (
                      <button type="button" onClick={() => setDismissing(problem.id)}>
                        No es un problema…
                      </button>
                    )}
                  </div>
                </>
              ) : (
                <div className="delivery-qa-problem__history">
                  {problem.solution && <p>Solución elegida: {problem.solution}</p>}
                  {problem.reason && <p>Motivo: {problem.reason}</p>}
                  {problem.notes.map((note, index) => (
                    <p key={index}>Coordinador: {note}</p>
                  ))}
                </div>
              )}
            </div>
          </article>
        );
      })}

      <div className="delivery-qa-problems__report">
        <strong>Reportar otro problema</strong>
        <div className="delivery-qa-problems__report-row">
          <input
            aria-label="Problema que la QA no vio"
            placeholder="Qué falla (por ejemplo: en móvil el menú se corta)"
            value={report}
            onChange={(event) => setReport(event.target.value)}
          />
          <input
            aria-label="Dónde pasa"
            placeholder="Dónde (pantalla, captura…)"
            value={reportPlace}
            onChange={(event) => setReportPlace(event.target.value)}
          />
          <ActionButton
            disabled={!report.trim()}
            onAction={async () => {
              await reportDeliveryQaProblem(taskId, report, reportPlace);
              setReport("");
              setReportPlace("");
            }}
          >
            Añadir a la lista
          </ActionButton>
        </div>
      </div>

      <footer className="delivery-qa-problems__send">
        <span>
          {ready.length === 0
            ? "Elige una solución para los problemas que quieras arreglar."
            : `${ready.length === 1 ? "1 problema listo" : `${ready.length} problemas listos`} para el coordinador: se arreglan en una sola implementación y la QA vuelve a comprobar solo esos puntos.`}
        </span>
        <ActionButton
          className="delivery-button--primary"
          disabled={ready.length === 0}
          onAction={send}
        >
          Enviar al coordinador
        </ActionButton>
      </footer>
    </section>
  );
}
