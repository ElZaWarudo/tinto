// A repo's guides, as a section of its settings: the reviewers whose
// comments become rules for its agents, the comments Tinto collected from
// them, the rules approved so far and the next version a coordinator
// proposed. Nothing changes until the user approves it here.

import { useEffect, useState } from "react";
import {
  collectDeliveryGuideComments,
  decideDeliveryGuide,
  getDeliveryGuide,
  getDeliveryGuideCandidates,
  getDeliveryRepoSettings,
  onDeliveryChanged,
  requestDeliveryGuide,
  setDeliveryRepoSettings,
} from "../../delivery/client";
import type {
  DeliveryGuideKind,
  DeliveryGuidePerson,
  DeliveryGuideRule,
  DeliveryGuideView,
} from "../../delivery/types";

import { ActionButton, ActionGroup } from "./ActionButton";
import type { RepoChoice } from "./DeliveryDialogs";
import { ExternalLink } from "./ExternalLink";
import { errorText } from "./errorText";

type Role = "" | DeliveryGuideKind | "both";

const ROLES: { value: Role; label: string }[] = [
  { value: "", label: "Ninguno" },
  { value: "visual", label: "Guía visual" },
  { value: "technical", label: "Guía técnica" },
  { value: "both", label: "Visual y técnica" },
];

const KINDS: { kind: DeliveryGuideKind; label: string }[] = [
  { kind: "visual", label: "Visual" },
  { kind: "technical", label: "Técnica" },
];

const STAGE_LABEL: Record<string, string> = {
  implementation: "implementación",
  review: "revisión",
  qa: "QA",
  tests: "tests",
};

const roleOf = (person: DeliveryGuidePerson | undefined): Role =>
  !person || person.kinds.length === 0 ? "" : person.kinds.length > 1 ? "both" : person.kinds[0];

const kindsOf = (role: Role): DeliveryGuideKind[] =>
  role === "both" ? ["visual", "technical"] : role ? [role] : [];

/** A repo's guides, as a section of its settings: saved apart from the rest. */
export function GuidesSection({ repo }: { repo: RepoChoice }) {
  const [view, setView] = useState<DeliveryGuideView | null>(null);
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [candidatesError, setCandidatesError] = useState<string | null>(null);
  // The roles as the user is choosing them, by login.
  const [roles, setRoles] = useState<Record<string, Role> | null>(null);
  const [status, setStatus] = useState("");

  // A coordinator's proposal shows as soon as it arrives.
  useEffect(() => {
    let live = true;
    const reload = () =>
      getDeliveryGuide(repo.path).then(
        (found) => live && setView(found),
        () => undefined,
      );
    const unlisten = onDeliveryChanged(() => void reload());
    return () => {
      live = false;
      void unlisten.then((stop) => stop());
    };
  }, [repo.path]);
  useEffect(() => {
    let live = true;
    getDeliveryGuide(repo.path).then(
      (found) => {
        if (!live) return;
        setView(found);
        setRoles(Object.fromEntries(found.people.map((person) => [person.login, roleOf(person)])));
      },
      (cause) => live && setStatus(errorText(cause)),
    );
    getDeliveryGuideCandidates(repo.path, repo.distro).then(
      (logins) => live && setCandidates(logins),
      (cause) => live && setCandidatesError(errorText(cause)),
    );
    return () => {
      live = false;
    };
  }, [repo.path, repo.distro]);

  // Everyone who can review, and the guides chosen before even if GitHub
  // no longer lists them.
  const logins = [
    ...new Set([...(view?.people.map((person) => person.login) ?? []), ...(candidates ?? [])]),
  ];
  const saveGuides = async () => {
    const settings = await getDeliveryRepoSettings(repo.path);
    const guides = Object.entries(roles ?? {})
      .map(([login, role]) => ({ login, kinds: kindsOf(role) }))
      .filter((person) => person.kinds.length > 0);
    await setDeliveryRepoSettings(repo.path, { ...settings, guides });
    setView(await getDeliveryGuide(repo.path));
    setStatus("Guías guardadas.");
  };

  return (
    <section className="delivery-settings__section" aria-label={`Guías de ${repo.label}`}>
      <h3 data-section="guides">Guías</h3>
      <p className="delivery-settings__intro">
        Lo que escriben en los PRs quienes elijas como guía (comentarios en líneas, resúmenes de sus
        revisiones y la conversación del PR) se convierte en reglas breves. Tinto lo lee solo cada
        media hora mientras sigue los PRs y, si hay comentarios nuevos, pide al coordinador del lote
        que proponga las reglas; tú las apruebas aquí.
      </p>

      <h4>Quién guía</h4>
      {candidatesError && (
        <p className="delivery-field__hint">
          No se pudo leer quién puede revisar en GitHub: {candidatesError}
        </p>
      )}
      {candidates === null && !candidatesError && (
        <p className="delivery-muted">Leyendo quién puede revisar en GitHub…</p>
      )}
      {logins.length > 0 && roles && (
        <ul className="delivery-guide-people">
          {logins.map((login) => (
            <li key={login}>
              <span>{login}</span>
              <select
                aria-label={`Papel de ${login}`}
                value={roles[login] ?? ""}
                onChange={(event) => setRoles({ ...roles, [login]: event.target.value as Role })}
              >
                {ROLES.map((role) => (
                  <option key={role.value} value={role.value}>
                    {role.label}
                  </option>
                ))}
              </select>
            </li>
          ))}
        </ul>
      )}
      <div className="delivery-actions">
        <ActionButton disabled={!roles} onAction={saveGuides}>
          Guardar guías
        </ActionButton>
      </div>

      <h4>Comentarios</h4>
      {view && (
        <p className="delivery-muted">
          {view.comments === 0
            ? "Todavía no hay comentarios de las guías."
            : `${view.comments} comentarios de las guías, ${view.pending} sin destilar.`}
          {view.collected_at_ms
            ? ` Leídos por última vez ${new Date(view.collected_at_ms).toLocaleString()}.`
            : ""}
        </p>
      )}
      <ActionGroup>
        <ActionButton
          disabled={!view || view.people.length === 0}
          title="Lee ahora lo último que escribieron las guías en los PRs, sin esperar a la próxima lectura"
          onAction={async () => {
            const found = await collectDeliveryGuideComments(repo.path, repo.distro);
            setView(found);
            setStatus(`${found.pending} comentarios sin destilar.`);
          }}
        >
          Leer de GitHub
        </ActionButton>
        <ActionButton
          disabled={!view || view.pending === 0}
          title="El coordinador del lote los lee y propone las reglas"
          onAction={async () => {
            const told = await requestDeliveryGuide(repo.path);
            setStatus(
              told === 1
                ? "Se lo pedimos al coordinador del lote."
                : `Se lo pedimos a los coordinadores de ${told} lotes.`,
            );
          }}
        >
          Destilar con el coordinador
        </ActionButton>
      </ActionGroup>
      <span className="delivery-field__hint" role="status">
        {status}
      </span>

      {view?.proposal && (
        <ProposalReview
          current={view.rules}
          proposed={view.proposal.rules}
          note={view.proposal.note}
          by={view.proposal.requested_by}
          onDecide={async (approve) => {
            setView(await decideDeliveryGuide(repo.path, approve));
            setStatus(approve ? "Reglas aprobadas." : "Propuesta rechazada.");
          }}
        />
      )}

      {KINDS.map(({ kind, label }) => {
        const rules = (view?.rules ?? []).filter((rule) => rule.kind === kind);
        return (
          <div key={kind} className="delivery-guide-kind">
            <h4>Guía {label.toLowerCase()}</h4>
            {rules.length === 0 ? (
              <p className="delivery-muted">Sin reglas todavía.</p>
            ) : (
              <ol
                className="delivery-guide-rules"
                aria-label={`Reglas de la guía ${label.toLowerCase()}`}
              >
                {rules.map((rule) => (
                  <RuleItem key={rule.id} rule={rule} />
                ))}
              </ol>
            )}
          </div>
        );
      })}
    </section>
  );
}

/** Where a rule applies and how often the guides asked for it. */
function scope(rule: DeliveryGuideRule): string {
  const parts = [
    rule.applies_to.length > 0 ? rule.applies_to.join(", ") : "todo el repositorio",
    rule.stages.length > 0
      ? rule.stages.map((stage) => STAGE_LABEL[stage] ?? stage).join(", ")
      : "todas las etapas",
  ];
  if (rule.seen > 0) parts.push(rule.seen === 1 ? "1 vez" : `${rule.seen} veces`);
  return parts.join(" · ");
}

function RuleItem({ rule, mark }: { rule: DeliveryGuideRule; mark?: string }) {
  return (
    <li className="delivery-guide-rule">
      <details>
        <summary>
          <strong>{rule.id}</strong> {rule.rule}
          {mark && <span className="delivery-guide-rule__mark">{mark}</span>}
          <small className="delivery-muted">{scope(rule)}</small>
        </summary>
        {rule.why && <p>{rule.why}</p>}
        {rule.example && <pre className="delivery-code">{rule.example}</pre>}
        {rule.sources.length > 0 && (
          <p className="delivery-guide-rule__sources">
            {rule.sources.map((source, index) => (
              <ExternalLink key={source} href={source}>
                comentario {index + 1}
              </ExternalLink>
            ))}
          </p>
        )}
      </details>
    </li>
  );
}

/** The proposed version against the current one: what is new, what changes
 *  and what goes, for the user to approve or reject whole. */
function ProposalReview({
  current,
  proposed,
  note,
  by,
  onDecide,
}: {
  current: DeliveryGuideRule[];
  proposed: DeliveryGuideRule[];
  note: string;
  by: string;
  onDecide: (approve: boolean) => Promise<void>;
}) {
  const before = new Map(current.map((rule) => [rule.id, rule]));
  const changed = proposed.filter((rule) => {
    const old = before.get(rule.id);
    return !old || JSON.stringify(old) !== JSON.stringify(rule);
  });
  const removed = current.filter((rule) => !proposed.some((next) => next.id === rule.id));
  return (
    <div
      className="delivery-banner delivery-banner--step"
      role="group"
      aria-label="Reglas propuestas"
    >
      <p className="delivery-banner__title">
        <strong>Reglas propuestas</strong>
        <span className="delivery-muted">por {by}</span>
      </p>
      {note && <p>{note}</p>}
      {changed.length + removed.length === 0 ? (
        <p className="delivery-muted">Las mismas reglas que ahora.</p>
      ) : (
        <ol className="delivery-guide-rules">
          {changed.map((rule) => (
            <RuleItem key={rule.id} rule={rule} mark={before.has(rule.id) ? "Cambia" : "Nueva"} />
          ))}
          {removed.map((rule) => (
            <RuleItem key={rule.id} rule={rule} mark="Se retira" />
          ))}
        </ol>
      )}
      <ActionGroup>
        <ActionButton className="delivery-button--primary" onAction={() => onDecide(true)}>
          Aprobar
        </ActionButton>
        <ActionButton onAction={() => onDecide(false)}>Rechazar</ActionButton>
      </ActionGroup>
    </div>
  );
}
