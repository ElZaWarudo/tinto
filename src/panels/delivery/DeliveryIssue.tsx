// A task's Jira issue read inside Tinto: its fields, description and last
// comments. Jira's HTML is sanitized before it is shown, and its links open
// in the system's browser.

import { useEffect, useState, type MouseEvent } from "react";
import { getDeliveryIssue, openDeliveryUrl } from "../../delivery/client";
import type { DeliveryIssue } from "../../delivery/types";

import { ExternalLink } from "./ExternalLink";

/** Elements dropped with their content: they run code, embed pages or take input. */
const DROPPED =
  "script,style,iframe,frame,frameset,object,embed,applet,form,input,button,textarea,select,link,meta,base,template,svg,math";

/** Jira's HTML without scripts, event handlers, embedded pages or styles;
 *  links and images point at absolute http(s) addresses on `base`'s site. */
export function sanitizeIssueHtml(html: string, base: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  doc.body.querySelectorAll(DROPPED).forEach((element) => element.remove());
  for (const element of doc.body.querySelectorAll("*")) {
    for (const { name, value } of [...element.attributes]) {
      const lower = name.toLowerCase();
      if (lower === "href" || lower === "src") {
        const url = absolute(value, base);
        if (url) element.setAttribute(name, url);
        else element.removeAttribute(name);
      } else if (lower.startsWith("on") || ["style", "srcdoc", "srcset"].includes(lower)) {
        element.removeAttribute(name);
      }
    }
  }
  return doc.body.innerHTML;
}

function absolute(value: string, base: string): string | null {
  try {
    const url = new URL(value.trim(), base);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

/** The webview would not follow a link in Jira's HTML: the system's browser opens it. */
function openLink(event: MouseEvent) {
  const link = (event.target as Element).closest("a");
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute("href");
  if (href?.startsWith("http")) void openDeliveryUrl(href).catch(() => {});
}

function when(created: string): string {
  const date = new Date(created);
  return Number.isNaN(date.getTime())
    ? created
    : date.toLocaleString([], { dateStyle: "short", timeStyle: "short" });
}

export function JiraIssue({ taskId, issueUrl }: { taskId: string; issueUrl: string }) {
  const [issue, setIssue] = useState<DeliveryIssue | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    getDeliveryIssue(taskId).then(
      (read) => {
        if (live) setIssue(read);
      },
      (cause) => {
        if (live) setError(String(cause?.message ?? cause));
      },
    );
    return () => {
      live = false;
    };
  }, [taskId]);

  const page = (
    <ExternalLink className="delivery-issue__page" href={issue?.url ?? issueUrl}>
      Abrir en Jira ↗
    </ExternalLink>
  );
  if (error) {
    return (
      <section className="delivery-issue">
        <p role="alert" className="delivery-issue__error">
          No se pudo leer la incidencia: {error}
        </p>
        {page}
      </section>
    );
  }
  if (!issue) {
    return (
      <section className="delivery-issue" aria-busy="true">
        <p className="delivery-issue__loading">Leyendo la incidencia en Jira…</p>
      </section>
    );
  }
  const html = (source: string) => ({ __html: sanitizeIssueHtml(source, issue.url) });
  const facts: [string, string | null][] = [
    ["Tipo", issue.type],
    ["Estado", issue.status],
    ["Prioridad", issue.priority],
    ["Asignada a", issue.assignee],
    ["Informador", issue.reporter],
    ["Etiquetas", issue.labels.join(", ") || null],
    ["Padre", issue.parent && `${issue.parent.key} ${issue.parent.summary}`],
  ];
  return (
    <section className="delivery-issue" aria-label={`Incidencia ${issue.key}`}>
      <h3>
        {issue.key} {issue.summary}
      </h3>
      {page}
      <dl className="delivery-issue__facts">
        {facts
          .filter(([, value]) => value)
          .map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
      </dl>
      {issue.links.length > 0 && (
        <ul className="delivery-issue__links" aria-label="Incidencias enlazadas">
          {issue.links.map((link) => (
            <li key={`${link.type} ${link.key}`}>
              {link.type} <strong>{link.key}</strong> {link.summary}
              {link.status && <span className="delivery-issue__status"> {link.status}</span>}
            </li>
          ))}
        </ul>
      )}
      <h4>Descripción</h4>
      {issue.description_html ? (
        <div
          className="delivery-issue__html"
          onClick={openLink}
          dangerouslySetInnerHTML={html(issue.description_html)}
        />
      ) : (
        <p className="delivery-issue__empty">Sin descripción.</p>
      )}
      {issue.comments.length > 0 && (
        <>
          <h4>Últimos comentarios</h4>
          {issue.comments.map((comment, index) => (
            <article key={index} className="delivery-issue__comment">
              <header>
                <strong>{comment.author}</strong> {when(comment.created)}
              </header>
              <div
                className="delivery-issue__html"
                onClick={openLink}
                dangerouslySetInnerHTML={html(comment.body_html)}
              />
            </article>
          ))}
        </>
      )}
    </section>
  );
}
