// Changes to a repo's settings that a coordinator proposes. Nothing changes
// until the user approves them here, all of them or the ones left checked.

import { useEffect, useState } from "react";
import {
  approveDeliverySettingsProposal,
  getDeliveryRepoSettings,
  rejectDeliverySettingsProposal,
} from "../../delivery/client";
import type {
  DeliveryRepoSettings,
  DeliverySettingsChanges,
  DeliverySettingsProposal,
} from "../../delivery/types";

type Run = (action: () => Promise<unknown>) => Promise<boolean>;

type ListField = "checks_add" | "checks_remove" | "qa_commands_add";

const key = (field: string, value: string) => `${field}\u0000${value}`;

/** Every change of the proposal, by key. */
function changeKeys(changes: DeliverySettingsChanges): string[] {
  const keys: string[] = [];
  for (const field of ["checks_add", "checks_remove", "qa_commands_add"] as ListField[]) {
    for (const value of changes[field] ?? []) keys.push(key(field, value));
  }
  if (changes.bootstrap != null) keys.push(key("bootstrap", ""));
  if (changes.qa_browser != null) keys.push(key("qa_browser", ""));
  if (changes.verification != null) keys.push(key("verification", ""));
  return keys;
}

/** The part of the proposal left checked. */
function accepted(
  changes: DeliverySettingsChanges,
  selected: Set<string>,
): DeliverySettingsChanges {
  const pick = (field: ListField) =>
    (changes[field] ?? []).filter((value) => selected.has(key(field, value)));
  const part: DeliverySettingsChanges = {
    checks_add: pick("checks_add"),
    checks_remove: pick("checks_remove"),
    qa_commands_add: pick("qa_commands_add"),
  };
  if (changes.bootstrap != null && selected.has(key("bootstrap", ""))) {
    part.bootstrap = changes.bootstrap;
  }
  if (changes.qa_browser != null && selected.has(key("qa_browser", ""))) {
    part.qa_browser = changes.qa_browser;
  }
  if (changes.verification != null && selected.has(key("verification", ""))) {
    part.verification = changes.verification;
  }
  return part;
}

function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: () => void;
}) {
  return <input type="checkbox" aria-label={label} checked={checked} onChange={onChange} />;
}

/** One list before and after: what stays, what goes and what comes. */
function ListDiff({
  title,
  before,
  added,
  removed,
  addField,
  selected,
  toggle,
}: {
  title: string;
  before: string[];
  added: string[];
  removed: string[];
  addField: ListField;
  selected: Set<string>;
  toggle: (key: string) => void;
}) {
  if (added.length === 0 && removed.length === 0) return null;
  const goes = removed.filter((value) => selected.has(key("checks_remove", value)));
  const comes = added.filter((value) => selected.has(key(addField, value)));
  const after = before.filter((value) => !goes.includes(value)).length + comes.length;
  return (
    <div className="delivery-settings-diff">
      <p className="delivery-settings-diff__title">
        <strong>{title}</strong>
        <span className="delivery-muted">
          antes {before.length} · después {after}
        </span>
      </p>
      <ul>
        {before.map((value) =>
          removed.includes(value) ? (
            <li key={value} className="delivery-settings-diff__removed">
              <Toggle
                label={`Quitar ${value}`}
                checked={selected.has(key("checks_remove", value))}
                onChange={() => toggle(key("checks_remove", value))}
              />
              <span aria-hidden="true">−</span>
              <code>{value}</code>
            </li>
          ) : (
            <li key={value} className="delivery-settings-diff__kept">
              <span aria-hidden="true" />
              <span aria-hidden="true" />
              <code>{value}</code>
            </li>
          ),
        )}
        {added.map((value) => (
          <li key={value} className="delivery-settings-diff__added">
            <Toggle
              label={`Añadir ${value}`}
              checked={selected.has(key(addField, value))}
              onChange={() => toggle(key(addField, value))}
            />
            <span aria-hidden="true">+</span>
            <code>{value}</code>
          </li>
        ))}
      </ul>
    </div>
  );
}

function ValueChange({
  title,
  field,
  before,
  after,
  selected,
  toggle,
}: {
  title: string;
  field: string;
  before: string;
  after: string;
  selected: Set<string>;
  toggle: (key: string) => void;
}) {
  return (
    <label className="delivery-settings-diff__value">
      <Toggle
        label={`Cambiar ${title.toLowerCase()}`}
        checked={selected.has(key(field, ""))}
        onChange={() => toggle(key(field, ""))}
      />
      <span>
        <strong>{title}</strong>: {before || "(vacío)"} → {after || "(vacío)"}
      </span>
    </label>
  );
}

function ProposalCard({
  proposal,
  busy,
  run,
}: {
  proposal: DeliverySettingsProposal;
  busy: boolean;
  run: Run;
}) {
  const changes = proposal.changes;
  const [current, setCurrent] = useState<DeliveryRepoSettings | null>(null);
  const [selected, setSelected] = useState(() => new Set(changeKeys(changes)));
  useEffect(() => {
    let live = true;
    getDeliveryRepoSettings(proposal.repo)
      .then((settings) => live && setCurrent(settings))
      .catch(() => live && setCurrent(null));
    return () => {
      live = false;
    };
  }, [proposal.repo, proposal.id]);
  const toggle = (value: string) =>
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(value)) next.delete(value);
      else next.add(value);
      return next;
    });
  const all = changeKeys(changes).length;
  const part = accepted(changes, selected);
  return (
    <fieldset className="delivery-decision delivery-settings-proposal">
      <legend>{proposal.reason}</legend>
      <p className="delivery-muted">
        Lo pide {proposal.requested_by}. Los trabajos que ya están en marcha no cambian; los
        siguientes verán lo que apruebes.
      </p>
      <ListDiff
        title="Comandos de verificación"
        before={current?.checks ?? changes.checks_remove ?? []}
        added={changes.checks_add ?? []}
        removed={changes.checks_remove ?? []}
        addField="checks_add"
        selected={selected}
        toggle={toggle}
      />
      <ListDiff
        title="Comandos de QA"
        before={current?.qa_commands ?? []}
        added={changes.qa_commands_add ?? []}
        removed={[]}
        addField="qa_commands_add"
        selected={selected}
        toggle={toggle}
      />
      {changes.bootstrap != null && (
        <ValueChange
          title="Bootstrap"
          field="bootstrap"
          before={current?.bootstrap ?? ""}
          after={changes.bootstrap}
          selected={selected}
          toggle={toggle}
        />
      )}
      {changes.qa_browser != null && (
        <ValueChange
          title="Navegador para la QA"
          field="qa_browser"
          before={current?.qa_browser ? "sí" : "no"}
          after={changes.qa_browser ? "sí" : "no"}
          selected={selected}
          toggle={toggle}
        />
      )}
      {changes.verification != null && (
        <ValueChange
          title="Cómo se verifica"
          field="verification"
          before={current?.verification ?? ""}
          after={changes.verification}
          selected={selected}
          toggle={toggle}
        />
      )}
      <div className="delivery-actions">
        <button
          type="button"
          className="delivery-button--primary"
          disabled={busy || selected.size === 0}
          onClick={() => void run(() => approveDeliverySettingsProposal(proposal.id, part))}
        >
          {selected.size === all ? "Aprobar" : `Aprobar ${selected.size} de ${all}`}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={() => void run(() => rejectDeliverySettingsProposal(proposal.id))}
        >
          Rechazar
        </button>
      </div>
    </fieldset>
  );
}

/** The settings changes waiting for the user on a task's repo. */
export function PendingSettingsProposals({
  proposals,
  busy,
  run,
}: {
  proposals: DeliverySettingsProposal[];
  busy: boolean;
  run: Run;
}) {
  if (proposals.length === 0) return null;
  return (
    <section
      className="delivery-banner delivery-decisions"
      aria-label="Cambios de ajustes por aprobar"
    >
      <p className="delivery-banner__title">
        <strong>Cambios de ajustes por aprobar</strong>
        <span className="delivery-muted">
          {proposals.length === 1 ? "1 propuesta" : `${proposals.length} propuestas`}
        </span>
      </p>
      {proposals.map((proposal) => (
        <ProposalCard key={proposal.id} proposal={proposal} busy={busy} run={run} />
      ))}
    </section>
  );
}
