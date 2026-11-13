// "Te necesita": what needs the user in a repo, above its task list. A step
// approved in one click, a broken stage and a paused batch are answered here;
// everything else goes to where it is answered. Each kind has its own mark,
// the stage line's: a flag for what holds, a cross for what broke.

import { decideDeliveryApproval, setDeliveryRunPaused } from "../../delivery/client";
import { Icon } from "../../workbench/Icon";
import { ActionButton } from "./ActionButton";
import { ExternalLink } from "./ExternalLink";
import type { QueueItem, QueueKind } from "./queueItems";
import { retryJobOnce } from "./retryJobOnce";

const KIND_MARK: Record<QueueKind, { icon: "flag" | "x" | "pull"; label: string }> = {
  holds: { icon: "flag", label: "Te espera" },
  broke: { icon: "x", label: "Se rompió" },
  review: { icon: "pull", label: "PR por revisar" },
};

export function DeliveryQueue({
  items,
  arrived,
  onArrived,
  onShow,
}: {
  items: QueueItem[];
  /** Tasks that just came to need the user: their rows tint once. */
  arrived: ReadonlySet<string>;
  onArrived: (taskId: string) => void;
  /** Goes to where the item is answered. */
  onShow: (item: QueueItem) => void;
}) {
  if (items.length === 0) {
    return (
      <section className="delivery-queue" aria-label="Te necesita">
        <p className="delivery-queue__none">Nada te necesita.</p>
      </section>
    );
  }
  return (
    <section className="delivery-queue" aria-label="Te necesita">
      <h2>
        <Icon name="flag" className="delivery-icon" />
        Te necesita <span>{items.length}</span>
      </h2>
      <ul>
        {items.map((item) => {
          const taskId = item.task?.id;
          const answer = item.inPlace;
          const mark = KIND_MARK[item.kind];
          const text = (
            <>
              <strong>{item.title}</strong>
              <span className="delivery-queue__need">{item.need}</span>
            </>
          );
          return (
            <li
              key={item.key}
              data-queue-key={item.key}
              className={`delivery-queue__row delivery-queue__row--${item.kind}${taskId && arrived.has(taskId) ? " delivery-queue__row--arrived" : ""}`}
              onAnimationEnd={(event) => {
                if (event.animationName === "delivery-arrive" && taskId) onArrived(taskId);
              }}
            >
              <span className="delivery-queue__mark" role="img" aria-label={mark.label}>
                <Icon name={mark.icon} className="delivery-icon" />
              </span>
              {item.review ? (
                <span className="delivery-queue__text">{text}</span>
              ) : (
                <button type="button" className="delivery-queue__text" onClick={() => onShow(item)}>
                  {text}
                </button>
              )}
              {item.review ? (
                <ExternalLink
                  href={item.review.url}
                  className="delivery-queue__control"
                  label={`Abrir el PR #${item.review.number} en GitHub`}
                >
                  Abrir
                </ExternalLink>
              ) : answer?.kind === "approve" ? (
                <ActionButton
                  className="delivery-queue__control"
                  aria-label={`${answer.label}: ${item.title}`}
                  onAction={() => decideDeliveryApproval(answer.approval.id, true)}
                >
                  {answer.label}
                </ActionButton>
              ) : answer?.kind === "retry" ? (
                <ActionButton
                  className="delivery-queue__control"
                  aria-label={`Reintentar ${item.title}: ${item.need}`}
                  onAction={() => retryJobOnce(answer.jobId)}
                >
                  Reintentar
                </ActionButton>
              ) : answer?.kind === "resume" ? (
                <ActionButton
                  className="delivery-queue__control"
                  aria-label={`Reanudar el lote ${item.title}`}
                  onAction={() => setDeliveryRunPaused(answer.runId, false)}
                >
                  Reanudar
                </ActionButton>
              ) : (
                <button
                  type="button"
                  className="delivery-queue__control delivery-button--quiet"
                  aria-label={`Ver ${item.title}: ${item.need}`}
                  onClick={() => onShow(item)}
                >
                  Ver
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
