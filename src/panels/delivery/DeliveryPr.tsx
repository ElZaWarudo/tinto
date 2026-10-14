// The task's pull request as Tinto follows it: state, checks, reviews and
// what is new since the user last looked. GitHub content stays on GitHub;
// only counts and authors are shown here.

import { markDeliveryPrSeen, refreshDeliveryPr } from "../../delivery/client";
import type { DeliveryPr } from "../../delivery/types";
import { prSummary } from "./taskStatus";

type Run = (action: () => Promise<unknown>) => Promise<boolean>;

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

export function PrSection({ pr, busy, run }: { pr: DeliveryPr; busy: boolean; run: Run }) {
  const summary = prSummary(pr);
  const fresh = news(pr);
  const { passing, failing, pending } = pr.checks;
  return (
    <section className="delivery-section delivery-pr" aria-label="Pull request">
      <h3>Pull request</h3>
      <div className="delivery-pr__body">
        <p className="delivery-pr__head">
          <a href={pr.url} target="_blank" rel="noreferrer">
            {pr.repo}#{pr.number}
          </a>
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
        {fresh && (
          <div className="delivery-pr__news" role="status">
            <span>
              Nuevo desde que lo viste: <strong>{fresh}</strong>.
            </span>
            <button
              type="button"
              disabled={busy}
              onClick={() => void run(() => markDeliveryPrSeen(pr.task_id))}
            >
              Marcar como visto
            </button>
          </div>
        )}
        {pr.error && <p className="delivery-tone--danger">No se pudo leer GitHub: {pr.error}</p>}
        <p className="delivery-muted delivery-pr__meta">
          {checked(pr.checked_at_ms)} ·{" "}
          <button
            type="button"
            className="delivery-link-button"
            onClick={() => void run(() => refreshDeliveryPr(pr.task_id))}
          >
            Actualizar
          </button>
        </p>
      </div>
    </section>
  );
}
