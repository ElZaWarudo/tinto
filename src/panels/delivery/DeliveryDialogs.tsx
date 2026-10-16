// Delivery forms, rendered as in-app modals (same look as the confirm
// dialog, and reachable by UI automation).

import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import {
  checkDeliveryJira,
  getArchivedDeliveryTasks,
  getDeliveryRepoSettings,
  setDeliveryJiraToken,
  setDeliveryRepoSettings,
} from "../../delivery/client";
import { isWindowsHost } from "../../workbench/platform";
import { useAccessibleDialog } from "../../workbench/useAccessibleDialog";
import { codexReasoningLabel, lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import { parseStatuses } from "./taskStatus";
import { ExternalLink } from "./ExternalLink";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryArchivedTask,
  DeliveryCoordinatorEndpoint,
  DeliveryPlatform,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryStageAgent,
  DeliveryTask,
} from "../../delivery/types";

export interface RepoChoice {
  path: string;
  distro: string | null;
  label: string;
}

function FormDialog({
  title,
  submitLabel,
  onSubmit,
  onCancel,
  busy,
  children,
  wide,
}: {
  title: string;
  submitLabel?: string;
  onSubmit?: () => void;
  onCancel: () => void;
  busy?: boolean;
  children: ReactNode;
  wide?: boolean;
}) {
  const firstFieldRef = useRef<HTMLDivElement>(null);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({ onClose: onCancel });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!busy) onSubmit?.();
  };
  return (
    <div
      ref={dialogRef}
      className="file-op-modal-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onClick={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <form
        className={`file-op-modal delivery-dialog${wide ? " delivery-dialog--wide" : ""}`}
        onSubmit={submit}
      >
        <h2 className="file-op-modal__title">{title}</h2>
        <div className="delivery-dialog__fields" ref={firstFieldRef}>
          {children}
        </div>
        <div className="file-op-modal__actions">
          <button type="button" className="file-op-modal__button" onClick={onCancel}>
            {onSubmit ? "Cancelar" : "Cerrar"}
          </button>
          {onSubmit && (
            <button
              type="submit"
              className="file-op-modal__button delivery-dialog__submit"
              disabled={busy}
            >
              {busy ? "Trabajando…" : submitLabel}
            </button>
          )}
        </div>
      </form>
    </div>
  );
}

function Field({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return (
    <label className="delivery-field">
      <span className="delivery-field__label">{label}</span>
      {children}
      {hint && <small className="delivery-field__hint">{hint}</small>}
    </label>
  );
}

/** The same repo, whether or not the path carries Windows' `\\?\` prefix. */
function samePath(a: string, b: string): boolean {
  const plain = (path: string) =>
    path
      .replace(/^\\\\\?\\/, "")
      .replace(/\//g, "\\")
      .toLowerCase();
  return plain(a) === plain(b);
}

export interface NewTaskValues {
  repo: RepoChoice;
  key: string;
  title: string;
  base: string;
  branch: string;
  runId: string;
  bootstrap: string;
}

export function NewTaskDialog({
  repos,
  runs,
  busy,
  onSubmit,
  onCancel,
}: {
  repos: RepoChoice[];
  runs: DeliveryRun[];
  busy: boolean;
  onSubmit: (values: NewTaskValues) => void;
  onCancel: () => void;
}) {
  const [repoPath, setRepoPath] = useState(repos[0]?.path ?? "");
  const [key, setKey] = useState("");
  const [title, setTitle] = useState("");
  const [base, setBase] = useState("");
  const [branch, setBranch] = useState("");
  const [runChoice, setRunChoice] = useState<string | null>(null);
  const [bootstrap, setBootstrap] = useState("");
  const repo = repos.find((choice) => choice.path === repoPath);
  const activeRuns = runs
    .filter((run) => run.status === "active" && samePath(run.repo, repoPath))
    .sort((a, b) => b.created_at_ms - a.created_at_ms);
  // The repo's most recent active batch, until the user picks another or none.
  const runId = runChoice ?? activeRuns[0]?.id ?? "";
  const branchPlaceholder = `delivery/${key.trim().toLowerCase() || "clave"}`;
  return (
    <FormDialog
      title="Nueva tarea"
      submitLabel="Crear tarea"
      busy={busy}
      onCancel={onCancel}
      onSubmit={() => {
        if (!repo || !key.trim()) return;
        onSubmit({ repo, key, title, base, branch, runId, bootstrap });
      }}
    >
      <Field label="Repositorio">
        <select
          value={repoPath}
          onChange={(event) => {
            setRepoPath(event.target.value);
            setRunChoice(null);
          }}
        >
          {repos.map((choice) => (
            <option key={choice.path} value={choice.path}>
              {choice.label}
            </option>
          ))}
        </select>
      </Field>
      <div className="delivery-field-row">
        <Field label="Clave">
          <input
            value={key}
            onChange={(event) => setKey(event.target.value)}
            placeholder="AGOS-501"
            required
            autoFocus
          />
        </Field>
        <Field label="Rama" hint="Si ya existe, la tarea la retoma.">
          <input
            value={branch}
            onChange={(event) => setBranch(event.target.value)}
            placeholder={branchPlaceholder}
          />
        </Field>
      </div>
      <Field label="Título">
        <input
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder="Qué hay que entregar"
        />
      </Field>
      <div className="delivery-field-row">
        <Field label="Base" hint="Por defecto: develop, main o master.">
          <input
            value={base}
            onChange={(event) => setBase(event.target.value)}
            placeholder="automática"
          />
        </Field>
        <Field label="Lote">
          <select value={runId} onChange={(event) => setRunChoice(event.target.value)}>
            <option value="">Sin lote</option>
            {activeRuns.map((run) => (
              <option key={run.id} value={run.id}>
                {run.title || run.id}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <Field
        label="Preparación del worktree"
        hint="Comando que se ejecuta en cada worktree nuevo de este repo, por ejemplo npm ci. Se recuerda por repo."
      >
        <input
          value={bootstrap}
          onChange={(event) => setBootstrap(event.target.value)}
          placeholder="opcional"
        />
      </Field>
    </FormDialog>
  );
}

const DELIVERY_ROLES = [
  { value: "tests", label: "Tests", hint: "Escribe las pruebas que fallan antes del cambio." },
  {
    value: "implementation",
    label: "Implementación",
    hint: "El cambio mínimo que las hace pasar.",
  },
  {
    value: "review",
    label: "Revisión",
    hint: "Solo lectura: revisa alcance, estilo y corrección.",
  },
  {
    value: "qa",
    label: "QA",
    hint: "Exclusiva y de solo lectura en el worktree. Tiene red y puede leer el checkout principal.",
  },
] as const;

interface Choice {
  value: string;
  label: string;
}

const DEFAULT_EFFORT = "medium";
const CLAUDE_MODELS: Choice[] = [
  { value: "", label: "Predeterminado" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "fable", label: "Fable" },
];
const CLAUDE_EFFORTS = ["low", "medium", "high", "xhigh", "max"];
const CODEX_FALLBACK_EFFORTS = ["low", "medium", "high"];

/** The models an agent offers. Codex's come from the catalog Agents keeps;
 *  Sol 6.1 is the default when the account has it. */
function modelChoices(agent: DeliveryAgent): { choices: Choice[]; preferred: string } {
  if (agent !== "codex") return { choices: CLAUDE_MODELS, preferred: "" };
  const catalog = lastRuntimeCatalog();
  const models = catalog?.models ?? [];
  if (models.length === 0)
    return { choices: [{ value: "", label: "Predeterminado" }], preferred: "" };
  const sol = models.find(
    (model) =>
      /sol/i.test(`${model.display_name} ${model.model}`) &&
      /6\.1/.test(`${model.display_name} ${model.model}`),
  );
  const fallback = models.find(
    (model) => model.model === catalog?.default_model || model.id === catalog?.default_model,
  );
  return {
    choices: models.map((model) => ({
      value: model.model,
      label: model.display_name || model.model,
    })),
    preferred: (sol ?? fallback ?? models[0]).model,
  };
}

/** Reasoning levels for an agent and model, and the one to preselect: medium
 *  when the model supports it. */
function effortChoices(
  agent: DeliveryAgent,
  model: string,
): { choices: Choice[]; preferred: string } {
  const values =
    agent === "codex"
      ? (lastRuntimeCatalog()
          ?.models?.find((option) => option.model === model)
          ?.supported_reasoning_efforts.map((effort) => effort.value) ?? CODEX_FALLBACK_EFFORTS)
      : CLAUDE_EFFORTS;
  const preferred = values.includes(DEFAULT_EFFORT) ? DEFAULT_EFFORT : (values[0] ?? "");
  return {
    choices: values.map((value) => ({ value, label: codexReasoningLabel(value) })),
    preferred,
  };
}

/** Who runs a stage, as the selectors hold it ("" is the CLI's default). */
interface AgentChoice {
  agent: DeliveryAgent;
  model: string;
  reasoning: string;
}

/** What the selectors show for a stage's agent: its own model and reasoning
 *  where this machine offers them, the preferred ones otherwise. */
function agentChoice(from: DeliveryStageAgent | null | undefined): AgentChoice {
  const agent = from?.agent === "claude" ? "claude" : "codex";
  const models = modelChoices(agent);
  const wanted = from?.agent === agent ? (from.model ?? "") : null;
  const model = models.choices.some((choice) => choice.value === wanted)
    ? (wanted as string)
    : models.preferred;
  return withModel({ agent, model, reasoning: from?.reasoning ?? "" }, model);
}

/** The choice with another model, on a reasoning level that model supports. */
function withModel(choice: AgentChoice, model: string): AgentChoice {
  const efforts = effortChoices(choice.agent, model);
  const keeps = efforts.choices.some((effort) => effort.value === choice.reasoning);
  return { ...choice, model, reasoning: keeps ? choice.reasoning : efforts.preferred };
}

export interface DispatchValues {
  role: string;
  agent: DeliveryAgent;
  model: string;
  reasoning: string;
  access: DeliveryAccess;
  prompt: string;
  /** Where a Claude job on a Windows repo runs; null lets Tinto choose. */
  platform: DeliveryPlatform | null;
  /** Repo settings edited in the dialog, to save before queueing; null when untouched. */
  settings: Pick<DeliveryRepoSettings, "checks" | "qa_commands" | "qa_browser"> | null;
}

function parseChecks(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function DispatchDialog({
  task,
  settings,
  lastAgent,
  initialRole,
  initialPrompt,
  promptSource,
  busy,
  onSubmit,
  onCancel,
}: {
  task: DeliveryTask;
  /** The repo's settings (checks, QA tools and each stage's agent), once loaded. */
  settings: DeliveryRepoSettings | null;
  /** Who ran the task's latest stage: where a stage with no agent of its own starts. */
  lastAgent: DeliveryStageAgent | null;
  /** The stage to preselect, usually the task's next one. */
  initialRole: string;
  /** Instructions to start from, usually the previous stage's handoff. */
  initialPrompt: string;
  /** Where initialPrompt came from, shown under the field. */
  promptSource: string | null;
  busy: boolean;
  onSubmit: (values: DispatchValues) => void;
  onCancel: () => void;
}) {
  const builtIn = DELIVERY_ROLES.some((choice) => choice.value === initialRole);
  const [role, setRole] = useState<string>(builtIn ? initialRole : "custom");
  const [customRole, setCustomRole] = useState(builtIn ? "" : initialRole);
  // What the user picked here; until then, the stage's own agent.
  const [picked, setPicked] = useState<AgentChoice | null>(null);
  const stages = settings?.stages ?? {};
  const stageKey = (role === "custom" ? customRole : role).trim().toLowerCase();
  const choice = picked ?? agentChoice(stages[stageKey] ?? lastAgent);
  const { agent, model, reasoning } = choice;
  const models = modelChoices(agent);
  const efforts = effortChoices(agent, model);
  const [access, setAccess] = useState<DeliveryAccess>("workspace");
  const [platform, setPlatform] = useState<DeliveryPlatform | "">("");
  // Only Claude Code has two places to run a Windows repo's jobs.
  const choosesPlatform = agent === "claude" && task.distro === null && isWindowsHost();
  const [prompt, setPrompt] = useState(initialPrompt);
  const [checksText, setChecksText] = useState<string | null>(null);
  const [qaCommandsText, setQaCommandsText] = useState<string | null>(null);
  const [qaBrowser, setQaBrowser] = useState<boolean | null>(null);
  const shownChecks = checksText ?? (settings?.checks ?? []).join("\n");
  const shownQaCommands = qaCommandsText ?? (settings?.qa_commands ?? []).join("\n");
  const shownQaBrowser = qaBrowser ?? settings?.qa_browser ?? false;
  const limitedClaude = agent === "claude" && access === "workspace";
  const qa = role === "qa";
  const roleHint = DELIVERY_ROLES.find((choice) => choice.value === role)?.hint;
  return (
    <FormDialog
      title={`Lanzar etapa en ${task.key}`}
      submitLabel="Poner en cola"
      busy={busy}
      wide
      onCancel={onCancel}
      onSubmit={() => {
        const finalRole = role === "custom" ? customRole.trim() : role;
        if (!finalRole || !prompt.trim()) return;
        onSubmit({
          role: finalRole,
          agent,
          model,
          reasoning,
          access,
          prompt,
          platform: choosesPlatform && platform ? platform : null,
          settings:
            checksText === null && qaCommandsText === null && qaBrowser === null
              ? null
              : {
                  checks: parseChecks(shownChecks),
                  qa_commands: parseChecks(shownQaCommands),
                  qa_browser: shownQaBrowser,
                },
        });
      }}
    >
      <div className="delivery-field-row">
        <Field label="Etapa" hint={roleHint}>
          <select
            value={role}
            onChange={(event) => {
              setRole(event.target.value);
              if (stages[event.target.value]) setPicked(null);
            }}
          >
            {DELIVERY_ROLES.map((choice) => (
              <option key={choice.value} value={choice.value}>
                {choice.label}
              </option>
            ))}
            <option value="custom">Otro…</option>
          </select>
        </Field>
        {role === "custom" && (
          <Field label="Nombre de la etapa">
            <input
              value={customRole}
              onChange={(event) => setCustomRole(event.target.value)}
              placeholder="docs"
            />
          </Field>
        )}
        <Field label="Agente">
          <select
            value={agent}
            onChange={(event) =>
              setPicked(
                agentChoice({
                  agent: event.target.value as DeliveryAgent,
                  model: null,
                  reasoning: null,
                }),
              )
            }
          >
            <option value="codex">Codex</option>
            <option value="claude">Claude Code</option>
          </select>
        </Field>
      </div>
      <div className="delivery-field-row">
        <Field label="Modelo">
          <select
            value={model}
            onChange={(event) => setPicked(withModel(choice, event.target.value))}
          >
            {models.choices.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Razonamiento">
          <select
            value={reasoning}
            onChange={(event) => setPicked({ ...choice, reasoning: event.target.value })}
          >
            {efforts.choices.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Acceso">
          <select
            value={access}
            onChange={(event) => setAccess(event.target.value as DeliveryAccess)}
          >
            <option value="workspace">Workspace</option>
            <option value="full">Acceso completo</option>
          </select>
        </Field>
      </div>
      <p className="delivery-field__hint">
        Tinto recuerda el agente, el modelo y el razonamiento de cada etapa para este repositorio, y
        los coordinadores los usan. También se fijan en Ajustes.
      </p>
      {choosesPlatform && (
        <Field
          label="Dónde se ejecuta"
          hint="Los comandos y las rutas del encargo deben ser de esa plataforma. En el historial, cada intento dice dónde se ejecutó."
        >
          <select
            value={platform}
            onChange={(event) => setPlatform(event.target.value as DeliveryPlatform | "")}
          >
            <option value="">Windows si Claude Code está instalado allí; si no, WSL</option>
            <option value="windows">Windows</option>
            <option value="wsl">WSL</option>
          </select>
        </Field>
      )}
      {limitedClaude && (
        <Field
          label="Comandos que puede ejecutar"
          hint="Uno por línea, con cualquier argumento. Además puede ejecutar comandos de solo lectura (ls, grep, git status…); el resto se le deniega. Se guardan para este repositorio y también los usan los trabajos del coordinador."
        >
          <textarea
            value={shownChecks}
            onChange={(event) => setChecksText(event.target.value)}
            rows={3}
            placeholder={"npm test\nnpm run lint"}
          />
        </Field>
      )}
      {qa && (
        <label className="delivery-check">
          <input
            type="checkbox"
            checked={shownQaBrowser}
            onChange={(event) => setQaBrowser(event.target.checked)}
          />
          <span>
            Navegador para la QA
            <small>
              Un navegador sin interfaz (Playwright) en Windows, con un perfil limpio; ve los
              servicios publicados en localhost. Las capturas se guardan con el trabajo, fuera del
              worktree. Se recuerda para este repositorio.
            </small>
          </span>
        </label>
      )}
      {qa && limitedClaude && (
        <Field
          label="Comandos de QA"
          hint="Uno por línea, con cualquier argumento: lo que la QA necesita además de los comandos de arriba, por ejemplo la CLI que cambia el issue. Pueden escribir fuera del worktree, aunque Claude Code sigue bloqueando sus propios comandos de archivos (cat, touch, rm…) fuera del worktree y del checkout principal. Se guardan para este repositorio."
        >
          <textarea
            value={shownQaCommands}
            onChange={(event) => setQaCommandsText(event.target.value)}
            rows={2}
            placeholder={"agentos skills install"}
          />
        </Field>
      )}
      <Field
        label="Instrucciones"
        hint={
          promptSource && prompt === initialPrompt
            ? `Tomado de ${promptSource}. Puedes editarlo. Tinto añade la tarea, el worktree, la etapa y el formato del resultado.`
            : "Tinto añade la tarea, el worktree, la etapa y el formato del resultado."
        }
      >
        <textarea
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
          rows={8}
          placeholder="Qué debe hacer esta etapa y cómo comprobarlo."
          required
        />
      </Field>
    </FormDialog>
  );
}

export function ReleaseLeaseDialog({
  name,
  note,
  busy,
  onSubmit,
  onCancel,
}: {
  name: string;
  note: string | null;
  busy: boolean;
  onSubmit: (confirmation: string) => void;
  onCancel: () => void;
}) {
  const [confirmation, setConfirmation] = useState("");
  return (
    <FormDialog
      title={`Liberar el recurso ${name}`}
      submitLabel="Liberar"
      busy={busy}
      onCancel={onCancel}
      onSubmit={() => {
        if (confirmation.trim()) onSubmit(confirmation);
      }}
    >
      {note && <p className="file-op-modal__body">{note}</p>}
      <p className="file-op-modal__body">
        Antes de liberarlo, comprueba que no queden procesos, navegadores ni servidores del trabajo
        anterior y que los datos estén como al principio.
      </p>
      <Field label="Qué comprobaste">
        <textarea
          value={confirmation}
          onChange={(event) => setConfirmation(event.target.value)}
          rows={3}
          placeholder="Navegador cerrado, contenedores detenidos, base de datos restaurada."
          required
          autoFocus
        />
      </Field>
    </FormDialog>
  );
}

/** Finds archived tasks by issue key or PR number and brings one back. */
export function ArchivedTasksDialog({
  busy,
  onUnarchive,
  onCancel,
}: {
  busy: boolean;
  onUnarchive: (taskId: string) => void;
  onCancel: () => void;
}) {
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<DeliveryArchivedTask[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const search = (text: string) =>
    getArchivedDeliveryTasks(text).then(
      (tasks) => {
        setFound(tasks);
        setError(null);
      },
      (cause) => setError(String(cause?.message ?? cause)),
    );
  useEffect(() => {
    void search("");
  }, []);
  return (
    <FormDialog
      title="Tareas archivadas"
      submitLabel="Buscar"
      busy={busy}
      onCancel={onCancel}
      onSubmit={() => void search(query)}
    >
      <Field label="Issue o número de PR" hint="Por ejemplo: AGOS-455 o #279">
        <input value={query} onChange={(event) => setQuery(event.target.value)} autoFocus />
      </Field>
      {error && <p className="delivery-tone--danger">{error}</p>}
      {found?.length === 0 && <p className="file-op-modal__body">No hay ninguna archivada así.</p>}
      {found && found.length > 0 && (
        <ul className="delivery-archived" aria-label="Archivadas encontradas">
          {found.map(({ task, pr_number, pr_url }) => (
            <li key={task.id} aria-label={task.key}>
              <span>
                <strong>{task.key}</strong> {task.title}
                {pr_number !== null && (
                  <>
                    {" · "}
                    {pr_url ? (
                      <ExternalLink href={pr_url}>PR #{pr_number}</ExternalLink>
                    ) : (
                      <>PR #{pr_number}</>
                    )}
                  </>
                )}
              </span>
              <button
                type="button"
                className="file-op-modal__button"
                disabled={busy}
                onClick={() => onUnarchive(task.id)}
              >
                Desarchivar
              </button>
            </li>
          ))}
        </ul>
      )}
    </FormDialog>
  );
}

/** A done delivery step whose result the user can correct. */
export interface CorrectableStep {
  id: string;
  label: string;
  outcome: string;
}

/** Corrects by hand the result of a step Tinto cannot check, saying why. */
export function CorrectResultDialog({
  steps,
  initial,
  busy,
  onSubmit,
  onCancel,
}: {
  steps: CorrectableStep[];
  initial: string;
  busy: boolean;
  onSubmit: (approvalId: string, outcome: string, reason: string) => void;
  onCancel: () => void;
}) {
  const [id, setId] = useState(initial);
  const step = steps.find((candidate) => candidate.id === id) ?? steps[0];
  const [outcome, setOutcome] = useState(step?.outcome ?? "");
  const [reason, setReason] = useState("");
  return (
    <FormDialog
      title="Corregir resultado"
      submitLabel="Guardar corrección"
      busy={busy}
      onCancel={onCancel}
      onSubmit={() => {
        if (step && outcome.trim() && reason.trim()) onSubmit(step.id, outcome, reason);
      }}
    >
      <p className="file-op-modal__body">
        El resultado anterior se conserva en el historial del paso, con su fecha.
      </p>
      {steps.length > 1 && (
        <Field label="Paso">
          <select
            value={step?.id}
            onChange={(event) => {
              setId(event.target.value);
              setOutcome(steps.find((s) => s.id === event.target.value)?.outcome ?? "");
            }}
          >
            {steps.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>
                {candidate.label}
              </option>
            ))}
          </select>
        </Field>
      )}
      <Field label="Resultado">
        <textarea
          value={outcome}
          onChange={(event) => setOutcome(event.target.value)}
          rows={3}
          required
        />
      </Field>
      <Field label="Por qué se corrige">
        <textarea
          value={reason}
          onChange={(event) => setReason(event.target.value)}
          rows={2}
          required
          autoFocus
        />
      </Field>
    </FormDialog>
  );
}

/** One stage's agent in Ajustes: not fixed, or an agent with its model and reasoning. */
function StageAgentRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: DeliveryStageAgent | undefined;
  onChange: (next: DeliveryStageAgent | null) => void;
}) {
  const shown = value ? agentChoice(value) : null;
  const set = (next: AgentChoice) =>
    onChange({ agent: next.agent, model: next.model || null, reasoning: next.reasoning || null });
  return (
    <div className="delivery-field-row">
      <Field label={label}>
        <select
          aria-label={`Agente de ${label}`}
          value={shown?.agent ?? ""}
          onChange={(event) =>
            event.target.value
              ? set(
                  agentChoice({
                    agent: event.target.value as DeliveryAgent,
                    model: null,
                    reasoning: null,
                  }),
                )
              : onChange(null)
          }
        >
          <option value="">Sin fijar</option>
          <option value="codex">Codex</option>
          <option value="claude">Claude Code</option>
        </select>
      </Field>
      {shown && (
        <>
          <Field label="Modelo">
            <select
              aria-label={`Modelo de ${label}`}
              value={shown.model}
              onChange={(event) => set(withModel(shown, event.target.value))}
            >
              {modelChoices(shown.agent).choices.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Razonamiento">
            <select
              aria-label={`Razonamiento de ${label}`}
              value={shown.reasoning}
              onChange={(event) => set({ ...shown, reasoning: event.target.value })}
            >
              {effortChoices(shown.agent, shown.model).choices.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </Field>
        </>
      )}
    </div>
  );
}

/** What QA can use in a repo and who runs each stage, set once so
 *  coordinators read it instead of asking on every task. */
function RepoQaSettings({ repos }: { repos: RepoChoice[] }) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [loaded, setLoaded] = useState<DeliveryRepoSettings | null>(null);
  const [browser, setBrowser] = useState(false);
  const [commands, setCommands] = useState("");
  const [environment, setEnvironment] = useState("");
  const [verification, setVerification] = useState("");
  // "yes", "no", or "" while the user has not set it.
  const [paidCalls, setPaidCalls] = useState("");
  const [stages, setStages] = useState<Record<string, DeliveryStageAgent>>({});
  const [jiraHost, setJiraHost] = useState("");
  const [jiraProject, setJiraProject] = useState("");
  const [jiraToken, setJiraToken] = useState("");
  const [afterPr, setAfterPr] = useState("");
  const [afterMerge, setAfterMerge] = useState("");
  const [jiraCheck, setJiraCheck] = useState("");
  const [status, setStatus] = useState("");
  useEffect(() => {
    if (!repo) return;
    let active = true;
    getDeliveryRepoSettings(repo).then(
      (settings) => {
        if (!active) return;
        setLoaded(settings);
        setBrowser(settings.qa_browser ?? false);
        setCommands((settings.qa_commands ?? []).join("\n"));
        setEnvironment(settings.qa_environment ?? "");
        setVerification(settings.verification ?? "");
        setPaidCalls(
          settings.paid_model_calls === true
            ? "yes"
            : settings.paid_model_calls === false
              ? "no"
              : "",
        );
        setStages(settings.stages ?? {});
        setJiraHost(settings.jira?.host ?? "");
        setJiraProject(settings.jira?.project ?? "");
        setAfterPr((settings.jira?.after_pr ?? []).join(", "));
        setAfterMerge((settings.jira?.after_merge ?? []).join(", "));
        setJiraToken("");
        setJiraCheck("");
        setStatus("");
      },
      () => active && setStatus("No se pudieron leer los ajustes de este repositorio."),
    );
    return () => {
      active = false;
    };
  }, [repo]);
  const label = repos.find((choice) => choice.path === repo)?.label ?? repo;
  return (
    <section className="delivery-qa-settings" aria-label="Ajustes del repositorio">
      <h3 className="delivery-dialog__subtitle">QA del repositorio</h3>
      <p className="file-op-modal__body">
        Lo que la QA puede usar en cada repositorio y quién corre cada etapa. Los coordinadores lo
        leen aquí y no lo preguntan en cada tarea.
      </p>
      <Field label="Repositorio">
        <select
          aria-label="Repositorio de la QA"
          value={repo}
          onChange={(event) => setRepo(event.target.value)}
        >
          {repos.map((choice) => (
            <option key={choice.path} value={choice.path}>
              {choice.label}
            </option>
          ))}
        </select>
      </Field>
      <Field
        label="Entorno de QA"
        hint="Dónde se prueba en esta máquina: plataforma, ámbito, clientes. Por ejemplo: Windows, ámbito personal; Claude Code y Codex con mis sesiones."
      >
        <textarea
          value={environment}
          onChange={(event) => setEnvironment(event.target.value)}
          rows={2}
        />
      </Field>
      <Field
        label="Comandos de QA"
        hint="Uno por línea: el comando real, nunca un script que los agentes puedan editar. Pueden escribir fuera del worktree."
      >
        <textarea
          value={commands}
          onChange={(event) => setCommands(event.target.value)}
          rows={2}
          placeholder="agentos skills install"
        />
      </Field>
      <label className="delivery-check">
        <input
          type="checkbox"
          checked={browser}
          onChange={(event) => setBrowser(event.target.checked)}
        />
        <span>Navegador para la QA</span>
      </label>
      <Field
        label="Cómo se verifica"
        hint="Además de las etapas: pruebas de punta a punta, QA en navegador cuando hay interfaz, en qué entorno… Los coordinadores lo leen y no lo preguntan en cada issue."
      >
        <textarea
          value={verification}
          onChange={(event) => setVerification(event.target.value)}
          rows={2}
          placeholder="Tests automáticos y una prueba de punta a punta en el stack local; QA en navegador solo si cambia la interfaz."
        />
      </Field>
      <Field label="Llamadas de pago a modelos en la QA">
        <select value={paidCalls} onChange={(event) => setPaidCalls(event.target.value)}>
          <option value="">Sin fijar (el coordinador pregunta)</option>
          <option value="yes">Permitidas, con un modelo barato</option>
          <option value="no">No permitidas</option>
        </select>
      </Field>
      <h3 className="delivery-dialog__subtitle">Jira</h3>
      <p className="file-op-modal__body">
        Con Jira configurado, Tinto hace los pasos Jira al aprobarlos: enlaza el PR, lleva la issue
        por estos estados y comprueba el estado final. Sin él, los hace el coordinador.
      </p>
      <Field label="Host de Jira" hint="Por ejemplo: https://jira.example.com">
        <input value={jiraHost} onChange={(event) => setJiraHost(event.target.value)} />
      </Field>
      <Field label="Proyecto">
        <input value={jiraProject} onChange={(event) => setJiraProject(event.target.value)} />
      </Field>
      <Field
        label="Token de Jira"
        hint={
          loaded?.jira?.has_token
            ? "Hay un token guardado. Escribe otro solo para cambiarlo."
            : "Un token de acceso personal. Se guarda aparte y no se muestra."
        }
      >
        <input
          type="password"
          autoComplete="off"
          value={jiraToken}
          onChange={(event) => setJiraToken(event.target.value)}
          placeholder={loaded?.jira?.has_token ? MASK : ""}
        />
      </Field>
      <Field
        label="Estados al abrir el PR"
        hint="En orden, separados por comas: En ejecución, In Review"
      >
        <input value={afterPr} onChange={(event) => setAfterPr(event.target.value)} />
      </Field>
      <Field label="Estados al fusionar el PR" hint="Por ejemplo: Cerrada">
        <input value={afterMerge} onChange={(event) => setAfterMerge(event.target.value)} />
      </Field>
      <div className="delivery-actions">
        <button
          type="button"
          className="file-op-modal__button"
          disabled={!loaded?.jira?.has_token}
          title={loaded?.jira?.has_token ? undefined : "Guarda antes el host y el token."}
          onClick={() => {
            setJiraCheck("Comprobando…");
            checkDeliveryJira(repo).then(
              (name) => setJiraCheck(`Conectado como ${name}.`),
              (cause) => setJiraCheck(`No se pudo: ${String(cause?.message ?? cause)}`),
            );
          }}
        >
          Comprobar Jira
        </button>
        <span className="delivery-field__hint" role="status">
          {jiraCheck}
        </span>
      </div>
      <h3 className="delivery-dialog__subtitle">Agente de cada etapa</h3>
      <p className="file-op-modal__body">
        Con qué agente, modelo y razonamiento corre cada etapa en este repositorio. El diálogo de
        lanzar etapa parte de aquí y los coordinadores lo usan en lugar de elegir ellos.
      </p>
      {DELIVERY_ROLES.map((stage) => (
        <StageAgentRow
          key={stage.value}
          label={stage.label}
          value={stages[stage.value]}
          onChange={(next) =>
            setStages((current) => {
              const rest = { ...current };
              delete rest[stage.value];
              return next ? { ...rest, [stage.value]: next } : rest;
            })
          }
        />
      ))}
      <div className="delivery-actions">
        <button
          type="button"
          className="file-op-modal__button"
          disabled={!loaded}
          onClick={() =>
            loaded &&
            setDeliveryRepoSettings(repo, {
              ...loaded,
              qa_browser: browser,
              qa_commands: parseChecks(commands),
              qa_environment: environment.trim(),
              verification: verification.trim(),
              paid_model_calls: paidCalls === "" ? null : paidCalls === "yes",
              stages,
              jira: jiraHost.trim()
                ? {
                    host: jiraHost.trim(),
                    project: jiraProject.trim(),
                    after_pr: parseStatuses(afterPr),
                    after_merge: parseStatuses(afterMerge),
                  }
                : null,
            })
              .then(async (saved) => {
                // The token goes apart and only when typed: empty keeps it.
                if (jiraToken.trim()) await setDeliveryJiraToken(repo, jiraToken);
                setJiraToken("");
                setLoaded(await getDeliveryRepoSettings(repo));
                setStatus(`Guardado para ${label}.`);
                return saved;
              })
              .catch((cause) => setStatus(`No se pudo guardar: ${String(cause?.message ?? cause)}`))
          }
        >
          Guardar los ajustes de {label}
        </button>
        <span className="delivery-field__hint" role="status">
          {status}
        </span>
      </div>
    </section>
  );
}

const MASK = "••••••••";

/** A setup snippet whose secret stays hidden; the button copies it whole. */
function CopySnippet({ label, shown, copied }: { label: string; shown: string; copied: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  return (
    <div className="delivery-field">
      <span className="delivery-field__label">{label}</span>
      <pre className="delivery-code delivery-snippet">{shown}</pre>
      <div className="delivery-actions">
        <button
          type="button"
          className="file-op-modal__button"
          aria-label={`Copiar la configuración de ${label}`}
          onClick={() =>
            navigator.clipboard
              .writeText(copied)
              .then(() => setState("copied"))
              .catch(() => setState("failed"))
          }
        >
          Copiar
        </button>
        <span className="delivery-field__hint" role="status">
          {state === "copied"
            ? "Copiado, con el token."
            : state === "failed"
              ? "No se pudo copiar."
              : "El token no se muestra; se copia con el botón."}
        </span>
      </div>
    </div>
  );
}

export function SettingsDialog({
  capacity,
  endpoint,
  runs,
  repos,
  busy,
  onCapacity,
  onCreateRun,
  onQaJiraComment,
  onPause,
  onTakeover,
  onClose,
  onCancel,
}: {
  capacity: number;
  endpoint: DeliveryCoordinatorEndpoint | null;
  runs: DeliveryRun[];
  repos: RepoChoice[];
  busy: boolean;
  onCapacity: (capacity: number) => void;
  onCreateRun: (repo: string, title: string, qaJiraComment: boolean) => void;
  onQaJiraComment: (run: DeliveryRun, post: boolean) => void;
  onPause: (run: DeliveryRun, paused: boolean) => void;
  onTakeover: (run: DeliveryRun) => void;
  onClose: (run: DeliveryRun) => void;
  onCancel: () => void;
}) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [title, setTitle] = useState("");
  const [jiraComment, setJiraComment] = useState(false);
  // The token is a credential: the dialog shows it masked and only the copy
  // buttons carry it.
  // Through tinto.exe as a stdio server, which finds Tinto's server
  // wherever it listens (its port can change between launches) and opens
  // Tinto when it is closed; over HTTP only when the executable is unknown.
  const command = endpoint?.command;
  // Claude Code starts the server through a shell that drops backslashes.
  const claudeCommand = command?.replace(/\\/g, "/");
  const claude = (token: string) =>
    !endpoint
      ? ""
      : command
        ? `claude mcp add tinto-delivery -- "${claudeCommand}" --delivery-mcp-proxy`
        : `claude mcp add --transport http tinto-delivery ${endpoint.url} --header "Authorization: Bearer ${token}"`;
  const codex = (token: string) =>
    !endpoint
      ? ""
      : command
        ? `[mcp_servers.tinto-delivery]\ncommand = '${command}'\nargs = ["--delivery-mcp-proxy"]`
        : `[mcp_servers.tinto-delivery]\nurl = "${endpoint.url}"\nhttp_headers = { Authorization = "Bearer ${token}" }`;
  const activeRuns = runs.filter((run) => run.status === "active");
  return (
    <FormDialog title="Ajustes de Delivery" wide onCancel={onCancel}>
      <Field
        label="Agentes en paralelo"
        hint="Automático: uno por cada tarea activa (cada issue), hasta 8. Los demás esperan en cola, en orden de llegada. Las conversaciones de Agents no cuentan."
      >
        <select
          value={capacity}
          disabled={busy}
          onChange={(event) => onCapacity(Number(event.target.value))}
        >
          <option value={0}>Automático: uno por issue</option>
          {[1, 2, 3, 4, 5, 6, 7, 8].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </Field>

      <h3 className="delivery-dialog__subtitle">Lotes activos</h3>
      <p className="file-op-modal__body">
        Un lote agrupa las tareas que dirige un coordinador. Las aprobaciones siempre las decides
        tú.
      </p>
      {activeRuns.length === 0 && <p className="file-op-modal__body">Ninguno.</p>}
      <ul className="delivery-run-list">
        {activeRuns.map((run) => (
          <li key={run.id}>
            <span>
              <strong>{run.title || run.id}</strong>
              <small>{run.owner ? `coordina ${run.owner}` : "sin coordinador"}</small>
            </span>
            <span className="delivery-run-list__actions">
              <select
                aria-label={`Resultado de QA en Jira del lote ${run.title || run.id}`}
                value={run.qa_jira_comment == null ? "" : run.qa_jira_comment ? "yes" : "no"}
                disabled={busy}
                onChange={(event) => onQaJiraComment(run, event.target.value === "yes")}
              >
                {run.qa_jira_comment == null && <option value="">QA en Jira: sin decidir</option>}
                <option value="yes">QA en Jira: comentar</option>
                <option value="no">QA en Jira: no publicar</option>
              </select>
              {run.unattended ? (
                <span
                  className="delivery-badge"
                  title="El brainstorm terminó: Tinto responde las decisiones con lo recomendado, concede los permisos y hace commit y push. Abrir el PR sigue esperándote."
                >
                  Sin paradas hasta el PR
                </span>
              ) : (
                <span
                  className="delivery-badge delivery-badge--muted"
                  title="El coordinador pregunta lo de todas las tareas; cuando lo respondas, cierra el brainstorm y el lote sigue sin paradas hasta el PR."
                >
                  Brainstorm
                </span>
              )}
              <button
                type="button"
                aria-label={`${run.paused ? "Reanudar" : "Pausar"} el lote ${run.title || run.id}`}
                disabled={busy}
                onClick={() => onPause(run, !run.paused)}
              >
                {run.paused ? "Reanudar" : "Pausar"}
              </button>
              {run.owner && run.owner !== "user" && (
                <button
                  type="button"
                  aria-label={`Tomar el control del lote ${run.title || run.id}`}
                  disabled={busy}
                  onClick={() => onTakeover(run)}
                >
                  Tomar el control
                </button>
              )}
              <button
                type="button"
                aria-label={`Cerrar el lote ${run.title || run.id}`}
                disabled={busy}
                onClick={() => onClose(run)}
              >
                Cerrar
              </button>
            </span>
          </li>
        ))}
      </ul>
      <div className="delivery-field-row">
        <Field label="Repositorio">
          <select value={repo} onChange={(event) => setRepo(event.target.value)}>
            {repos.map((choice) => (
              <option key={choice.path} value={choice.path}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Nuevo lote">
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Lote de octubre"
          />
        </Field>
      </div>
      <Field
        label="Resultado de QA en Jira"
        hint="El coordinador lo lee aquí y no lo pregunta en cada tarea."
      >
        <select
          value={jiraComment ? "yes" : "no"}
          onChange={(event) => setJiraComment(event.target.value === "yes")}
        >
          <option value="no">No publicar</option>
          <option value="yes">Comentar en cada issue</option>
        </select>
      </Field>
      <button
        type="button"
        className="file-op-modal__button"
        disabled={busy || !repo || !title.trim()}
        onClick={() => {
          onCreateRun(repo, title.trim(), jiraComment);
          setTitle("");
        }}
      >
        Crear lote
      </button>

      <RepoQaSettings repos={repos} />

      <h3 className="delivery-dialog__subtitle">Conectar un coordinador</h3>
      <p className="file-op-modal__body">
        Un agente coordinador (por ejemplo la skill backlog-delivery) dirige Delivery mediante MCP:
        crea tareas, lanza etapas y pide aprobaciones.
      </p>
      {endpoint ? (
        <>
          <p className="file-op-modal__body">
            Las conversaciones de Codex y Claude Code que abres en Agents ya tienen estas
            herramientas, sin pedir permiso para usarlas. Lo de abajo es para agentes que corren
            fuera de Tinto: arrancan tinto.exe como servidor MCP, que abre Tinto si está cerrado y
            lo encuentra aunque cambie de puerto. No hace falta abrirlo de otra forma ni
            configurarlo de nuevo.
          </p>
          <CopySnippet label="Claude Code" shown={claude(MASK)} copied={claude(endpoint.token)} />
          <CopySnippet
            label="Codex (config.toml)"
            shown={codex(MASK)}
            copied={codex(endpoint.token)}
          />
        </>
      ) : (
        <p className="file-op-modal__body">El API de coordinador no está disponible.</p>
      )}
    </FormDialog>
  );
}
