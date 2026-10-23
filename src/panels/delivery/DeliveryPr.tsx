// The task's pull request as Tinto follows it: state, checks, reviews and
// what is new since the user last looked. GitHub content stays on GitHub;
// only counts and authors are shown here.

import { useEffect, useState } from "react";
import {
  changeDeliveryPrReviewers,
  getDeliveryPrReviewers,
  markDeliveryPrSeen,
  refreshDeliveryPr,
} from "../../delivery/client";
import type { DeliveryPr } from "../../delivery/types";
import { prSummary } from "./taskStatus";
import { ExternalLink } from "./ExternalLink";
import { ActionButton } from "./ActionButton";

function checked(ms: number | null): string {
  if (!ms) return "todavía sin leer";
  const minutes = Math.round((Date.now() - ms) / 60000);
  return minutes < 1 ? "leído ahora" : `leído hace ${minutes} min`;
}

function news(pr: DeliveryPr): string | null {
  const parts = [
    pr.new_reviews > 0 &&
      `${pr.new_reviews} ${pr.new_reviews === 1 ? "revisión nueva" : "revisiones nuevas"}`,
    pr.new_comments > 0 &&
      `${pr.new_comments} ${pr.new_comments === 1 ? "comentario nuevo" : "comentarios nuevos"}`,
  ].filter(Boolean);
  if (parts.length === 0) return null;
  return `${parts.join(" y ")} de ${pr.new_authors.join(", ")}`;
}

const DECISION: Record<string, string> = {
  approved: "Aprobado",
  changes_requested: "Cambios pedidos",
  review_required: "Pendiente de revisión",
};

/** Who reviews the open PR. The user ticks people and asks them for a
 *  review (again, when they already gave one) or takes their request back. */
function PrReviewers({ pr }: { pr: DeliveryPr }) {
  const requested = pr.requested_reviewers ?? [];
  const approved = pr.approved_by ?? [];
  const [open, setOpen] = useState(false);
  // The repo's people who can review, read from GitHub the first time it opens.
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [chosen, setChosen] = useState<string[]>([]);
  useEffect(() => {
    if (!open || candidates) return;
    let live = true;
    getDeliveryPrReviewers(pr.task_id).then(
      (logins) => live && setCandidates(logins),
      (cause) =>
        live &&
        setError(
          cause && typeof cause === "object" && "message" in cause
            ? String(cause.message)
            : String(cause),
        ),
    );
    return () => {
      live = false;
    };
  }, [open, candidates, pr.task_id]);
  const people = [...new Set([...requested, ...approved, ...(candidates ?? [])])];
  const removable = chosen.filter((login) => requested.includes(login));
  const change = async (request: string[], remove: string[]) => {
    await changeDeliveryPrReviewers(pr.task_id, request, remove);
    setChosen([]);
  };
  return (
    <details className="delivery-pr__reviewers" onToggle={(event) => setOpen(event.currentTarget.open)}>
      <summary>
        Revisores:{" "}
        {requested.length > 0 ? `esperando a ${requested.join(", ")}` : "ninguna revisión pedida"}
      </summary>
      <fieldset className="delivery-reviewers">
        <legend>Elige a quién</legend>
        {candidates === null && !error && (
          <p className="delivery-muted">Leyendo quién puede revisar en GitHub…</p>
        )}
        {error && <p className="delivery-job__note">No se pudo leer quién puede revisar: {error}</p>}
        {people.map((login) => (
          <label key={login}>
            <input
              type="checkbox"
              checked={chosen.includes(login)}
              onChange={(event) =>
                setChosen(
                  event.target.checked
                    ? [...chosen, login]
                    : chosen.filter((other) => other !== login),
                )
              }
            />
            {login}
            {requested.includes(login) && <small className="delivery-muted">pedida</small>}
            {approved.includes(login) && <small className="delivery-muted">aprobó</small>}
          </label>
        ))}
      </fieldset>
      <div className="delivery-actions">
        <ActionButton disabled={chosen.length === 0} onAction={() => change(chosen, [])}>
          Pedir revisión
        </ActionButton>
        <ActionButton
          disabled={removable.length === 0}
          title="Solo a quien tiene la revisión pedida."
          onAction={() => change([], removable)}
        >
          Quitar petición
        </ActionButton>
        <span className="delivery-muted">
          A quien ya revisó, pedírsela otra vez le avisa de nuevo en GitHub.
        </span>
      </div>
    </details>
  );
}

export function PrSection({ pr }: { pr: DeliveryPr }) {
  const summary = prSummary(pr);
  const fresh = news(pr);
  const { passing, failing, pending } = pr.checks;
  return (
    <section className="delivery-section delivery-pr" aria-label="Pull request">
      <h3>Pull request</h3>
      <div className="delivery-pr__body">
        <p className="delivery-pr__head">
          <ExternalLink href={pr.url}>
            {pr.repo}#{pr.number}
          </ExternalLink>
          <span className={`delivery-status-line delivery-tone--${summary.tone}`}>
            {summary.label}
          </span>
          {pr.draft && <span className="delivery-badge delivery-badge--muted">borrador</span>}
        </p>
        {pr.state === "open" && (
          <ul className="delivery-pr__facts">
            <li>
              Revisión:{" "}
              <strong>
                {DECISION[pr.review_decision ?? ""] ??
                  (pr.approved_by?.length ? "Aprobado" : "Sin revisión aprobada")}
              </strong>
              {pr.approved_by?.length ? ` por ${pr.approved_by.join(", ")}` : ""}
            </li>
            <li>
              Checks:{" "}
              {passing + failing + pending === 0 ? (
                <strong>ninguno</strong>
              ) : (
                <>
                  <strong className="delivery-tone--ok">✓ {passing}</strong>
                  {failing > 0 && <strong className="delivery-tone--danger"> ✕ {failing}</strong>}
                  {pending > 0 && <strong className="delivery-tone--waiting"> ⏳ {pending}</strong>}
                </>
              )}
            </li>
            {pr.conflicts && (
              <li className="delivery-tone--attention">Tiene conflictos con su rama base.</li>
            )}
          </ul>
        )}
        {pr.state === "open" && <PrReviewers pr={pr} />}
        {fresh && (
          <div className="delivery-pr__news" role="status">
            <span>
              Nuevo desde que lo viste: <strong>{fresh}</strong>.
            </span>
            <ActionButton onAction={() => markDeliveryPrSeen(pr.task_id)}>
              Marcar como visto
            </ActionButton>
          </div>
        )}
        {pr.error && <p className="delivery-tone--danger">No se pudo leer GitHub: {pr.error}</p>}
        <p className="delivery-muted delivery-pr__meta">
          {checked(pr.checked_at_ms)} ·{" "}
          <ActionButton
            className="delivery-link-button"
            onAction={() => refreshDeliveryPr(pr.task_id)}
          >
            Actualizar
          </ActionButton>
        </p>
      </div>
    </section>
  );
}
