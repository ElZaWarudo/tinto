// The merge step as the user sees it: what Tinto already checked before they
// approve, what it is doing once approved (it runs apart from the click),
// why it failed, and the GitHub calls behind all of it.

import { useEffect, useState } from "react";
import { getDeliveryGithubLog } from "../../delivery/client";
import type { DeliveryApproval, DeliveryGhCall, DeliveryPr } from "../../delivery/types";
import { Icon } from "../../workbench/Icon";
import { Overlay } from "./DeliveryOverlay";
import { elapsed, mergeChecks } from "./taskStatus";

/** What Tinto checked of the PR before the merge is approved, and how long
 *  ago it read GitHub. Approving checks the PR again. */
export function MergeChecks({ pr, now }: { pr: DeliveryPr; now: number }) {
  return (
    <div className="delivery-merge" aria-label="Comprobaciones del merge">
      <ul className="delivery-merge__checks">
        {mergeChecks(pr).map((check) => (
          <li
            key={check.label}
            className={check.ok ? "delivery-tone--ok" : "delivery-tone--danger"}
          >
            <Icon name={check.ok ? "check" : "x"} />
            <span>{check.label}</span>
          </li>
        ))}
      </ul>
      {pr.checked_at_ms && (
        <p className="delivery-muted">
          Leído de GitHub hace {elapsed(pr.checked_at_ms, now)}; al aprobar, Tinto lo comprueba de
          nuevo.
        </p>
      )}
    </div>
  );
}

/** An approved merge Tinto is running: the step it is on and for how long.
 *  Nothing waits on the user. */
export function MergeRunning({ approval, now }: { approval: DeliveryApproval; now: number }) {
  const progress = approval.progress;
  if (!progress) return null;
  return (
    <div className="delivery-merge" role="status" aria-label="Merge en marcha">
      <p>
        <Icon name="clock" /> {progress.step} · {elapsed(progress.since_ms, now)}
      </p>
      <p className="delivery-muted">
        Tinto lo hace aparte: puedes seguir trabajando. Si GitHub no responde en 90 s, el paso falla
        y dice por qué.
      </p>
    </div>
  );
}

/** A merge that failed, while its PR is still open: what went wrong. */
export function MergeFailed({ approval, pr }: { approval: DeliveryApproval; pr: DeliveryPr }) {
  return (
    <div className="delivery-banner delivery-merge--failed" role="alert">
      <p className="delivery-banner__title">
        <Icon name="x" />
        <strong>No se fusionó el PR #{pr.number}</strong>
      </p>
      <p>{approval.outcome ?? "Sin detalle del error."}</p>
      <p className="delivery-muted">
        El PR sigue abierto. Prepara el merge de nuevo cuando lo que lo impidió esté resuelto; el
        registro de GitHub muestra cada llamada.
      </p>
    </div>
  );
}

/** The latest GitHub calls Tinto made, newest first, read while open. */
export function GitHubLog() {
  const [from, setFrom] = useState<HTMLElement | null>(null);
  const [calls, setCalls] = useState<DeliveryGhCall[] | null>(null);
  useEffect(() => {
    if (!from) return;
    let live = true;
    const read = () =>
      getDeliveryGithubLog().then(
        (log) => live && setCalls(log),
        () => live && setCalls([]),
      );
    read();
    const timer = window.setInterval(read, 3000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [from]);
  const failed = (calls ?? []).filter(
    (call) => call.outcome.kind === "failed" || call.outcome.kind === "timed_out",
  ).length;
  return (
    <>
      <button
        type="button"
        className="delivery-env"
        aria-expanded={from !== null}
        onClick={(event) => setFrom(from ? null : event.currentTarget)}
      >
        Registro de GitHub
      </button>
      {from && (
        <Overlay
          anchor={from}
          label="Registro de GitHub"
          className="delivery-float delivery-ghlog"
          onClose={() => setFrom(null)}
        >
          <p className="delivery-muted">
            Las últimas llamadas de Tinto a GitHub, la más reciente primero
            {failed > 0 ? ` · ${failed} con error` : ""}.
          </p>
          {calls === null ? (
            <p className="delivery-muted">Leyendo…</p>
          ) : calls.length === 0 ? (
            <p className="delivery-muted">Tinto no ha llamado a GitHub desde que se abrió.</p>
          ) : (
            <ol className="delivery-ghlog__list">
              {calls.map((call, index) => (
                <li key={`${call.at_ms}-${index}`}>
                  <span className="delivery-muted">
                    {new Date(call.at_ms).toLocaleTimeString()}
                  </span>
                  <span className={callTone(call)}>{callResult(call)}</span>
                  <code>{call.command}</code>
                </li>
              ))}
            </ol>
          )}
        </Overlay>
      )}
    </>
  );
}

function callTone(call: DeliveryGhCall): string {
  switch (call.outcome.kind) {
    case "ok":
      return "delivery-tone--ok";
    case "waited":
      return "delivery-tone--attention";
    default:
      return "delivery-tone--danger";
  }
}

function callResult(call: DeliveryGhCall): string {
  const took = `${(call.duration_ms / 1000).toFixed(1)} s`;
  switch (call.outcome.kind) {
    case "ok":
      return took;
    case "waited":
      return `esperó ${took}`;
    case "timed_out":
      return `sin respuesta en ${took}`;
    case "failed":
      return `${took} · ${call.outcome.error}`;
  }
}
