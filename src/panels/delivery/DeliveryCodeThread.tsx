// The code a decision is about, shown as GitHub shows a review thread: the
// file, the lines (red removed, green added, the commented one framed) and
// the comment. Tinto read all of it from GitHub or the task's worktree.

import type { ReactNode } from "react";
import type { DeliveryCodeThread } from "../../delivery/types";
import { ExternalLink } from "./ExternalLink";

export function CodeThreads({ threads }: { threads?: DeliveryCodeThread[] }) {
  if (!threads?.length) return null;
  return (
    <div className="delivery-threads">
      {threads.map((thread, index) => (
        <CodeThread key={index} thread={thread} open={index === 0} />
      ))}
    </div>
  );
}

function CodeThread({ thread, open }: { thread: DeliveryCodeThread; open: boolean }) {
  const isComment = thread.kind === "pr_comment";
  const where = thread.path || (isComment ? "Comentario del PR" : "Código");
  return (
    <details className="delivery-thread" open={open}>
      <summary className="delivery-thread__head">
        <span className="delivery-thread__chevron" aria-hidden="true">
          ▶
        </span>
        <span className="delivery-thread__path">
          {where}
          {thread.line && !thread.error ? `:${thread.line}` : ""}
        </span>
        <span className="delivery-thread__tag">
          {isComment ? "comentario del PR" : "código del worktree"}
        </span>
        {thread.outdated && (
          <span className="delivery-thread__tag delivery-thread__tag--outdated">
            desactualizado
          </span>
        )}
        <span className="delivery-thread__spacer" />
        {thread.url && (
          <ExternalLink href={thread.url} className="delivery-thread__link" label="Ver en GitHub">
            Ver en GitHub ↗
          </ExternalLink>
        )}
      </summary>
      {thread.error ? (
        <p className="delivery-thread__missing">No disponible: {thread.error}</p>
      ) : (
        <table className="delivery-thread__diff" aria-label={`Líneas de ${where}`}>
          <tbody>
            {thread.lines.map((line, index) => (
              <tr
                key={index}
                className={`delivery-thread__line delivery-thread__line--${line.kind}${
                  line.target ? " delivery-thread__line--target" : ""
                }`}
              >
                <td className="delivery-thread__num">{line.old ?? ""}</td>
                <td className="delivery-thread__num">{line.new ?? ""}</td>
                <td className="delivery-thread__sign">
                  {line.kind === "add" ? "+" : line.kind === "del" ? "-" : " "}
                </td>
                <td className="delivery-thread__code">{line.text}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {thread.comment && (
        <div className="delivery-thread__comment">
          <span className="delivery-thread__avatar" aria-hidden="true">
            {thread.comment.author.slice(0, 1).toUpperCase()}
          </span>
          <div>
            <p className="delivery-thread__who">
              {thread.comment.author}
              <span> · {ago(thread.comment.created_at)}</span>
            </p>
            <p className="delivery-thread__body">{withInlineCode(thread.comment.body)}</p>
            <p className="delivery-thread__replies">
              {thread.comment.replies === 0
                ? "Sin respuestas"
                : thread.comment.replies === 1
                  ? "1 respuesta"
                  : `${thread.comment.replies} respuestas`}
            </p>
          </div>
        </div>
      )}
      {thread.note && <p className="delivery-thread__note">{thread.note}</p>}
    </details>
  );
}

/** `code` spans of a comment as code; the rest as written. */
function withInlineCode(text: string): ReactNode[] {
  return text
    .split("`")
    .map((part, index) => (index % 2 === 1 ? <code key={index}>{part}</code> : part));
}

function ago(iso: string): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return "";
  const minutes = Math.max(0, Math.round((Date.now() - at) / 60_000));
  if (minutes < 1) return "ahora";
  if (minutes < 60) return `hace ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `hace ${hours} h`;
  return `hace ${Math.round(hours / 24)} d`;
}
