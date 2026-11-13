// "Te necesita": what needs the user in a repo, above its task list. A step
// approved in one click and a broken stage are answered here; everything else
// goes to its task, on what it asks.

import { decideDeliveryApproval, retryDeliveryJob } from "../../delivery/client";
import { Icon } from "../../workbench/Icon";
import { ActionButton } from "./ActionButton";
import { ExternalLink } from "./ExternalLink";
import type { QueueItem } from "./queueItems";

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
  /** Selects the item's task, on what it asks. */
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
          const text = (
            <>
              <strong>{item.title}</strong>
              <span
                className={`delivery-queue__need${item.kind === "broke" ? " delivery-queue__need--broke" : ""}`}
              >
                {item.need}
              </span>
            </>
          );
          return (
            <li
              key={item.key}
              className={`delivery-queue__row${taskId && arrived.has(taskId) ? " delivery-queue__row--arrived" : ""}`}
              onAnimationEnd={(event) => {
                if (event.animationName === "delivery-arrive" && taskId) onArrived(taskId);
              }}
            >
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
                  onAction={() => retryDeliveryJob(answer.jobId)}
                >
                  Reintentar
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
