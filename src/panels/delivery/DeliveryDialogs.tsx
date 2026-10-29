// Delivery forms, rendered as in-app modals (same look as the confirm
// dialog, and reachable by UI automation), and the settings page.

import { useEffect, useRef, useState, type FormEvent, type ReactNode, type RefObject } from "react";
import { THEME_OPTIONS, themeStore, useThemeSetting, type ThemeSetting } from "../../qol/theme";
import { qualityStore, useQualityState } from "../../qol/state";
import { disableNotifications, enableNotifications } from "../../qol/notifications";
import { GuidesSection } from "./DeliveryGuides";
import {
  checkDeliveryJira,
  getArchivedDeliveryTasks,
  getDeliveryRepoSettings,
  setDeliveryJiraToken,
  setDeliveryRepoSettings,
} from "../../delivery/client";
import { isWindowsHost } from "../../workbench/platform";
import { useAccessibleDialog } from "../../workbench/useAccessibleDialog";
import {
  agentChoice,
  effortChoices,
  modelChoices,
  withModel,
  type AgentChoice,
} from "./agentChoice";
import { parseStatuses } from "./taskStatus";
import { ExternalLink } from "./ExternalLink";
import { ActionButton, ActionGroup } from "./ActionButton";
import type {
  DeliveryAutonomy,
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
import { UNLIMITED_CAPACITY } from "../../delivery/types";
import { GrowingTextarea } from "./GrowingTextarea";

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
  children,
  wide,
}: {
  title: string;
  submitLabel?: string;
  /** Resolves to false when the form is not ready; throwing shows why. */
  onSubmit?: () => Promise<unknown>;
  onCancel: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  const firstFieldRef = useRef<HTMLDivElement>(null);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({ onClose: onCancel });
  // The submit button sends it, also on Enter.
  const submit = (event: FormEvent) => event.preventDefault();
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
            <ActionButton
              type="submit"
              className="file-op-modal__button delivery-dialog__submit"
              onAction={onSubmit}
            >
              {submitLabel}
            </ActionButton>
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
  onSubmit,
  onCancel,
}: {
  repos: RepoChoice[];
  runs: DeliveryRun[];
  onSubmit: (values: NewTaskValues) => Promise<unknown>;
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
      onCancel={onCancel}
      onSubmit={async () => {
        if (!repo || !key.trim()) return false;
        await onSubmit({ repo, key, title, base, branch, runId, bootstrap });
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

/** A limit typed in a field: a whole number above zero, else none. */
function positive(text: string): number | null {
  const value = Number.parseInt(text, 10);
  return value > 0 ? value : null;
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

/** `NAME=value` lines as variables by name; lines without a name are dropped. */
function parseEnv(text: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const line of text.split("\n")) {
    const at = line.indexOf("=");
    const name = (at < 0 ? line : line.slice(0, at)).trim();
    if (name) env[name] = at < 0 ? "" : line.slice(at + 1).trim();
  }
  return env;
}

/** `folder: service service` lines as the services built from each folder. */
function parseServices(text: string): Record<string, string[]> {
  const services: Record<string, string[]> = {};
  for (const line of text.split("\n")) {
    const at = line.indexOf(":");
    if (at < 0) continue;
    const folder = line.slice(0, at).trim();
    const built = line
      .slice(at + 1)
      .split(/[\s,]+/)
      .filter(Boolean);
    if (folder && built.length > 0) services[folder] = built;
  }
  return services;
}

export function DispatchDialog({
  task,
  settings,
  lastAgent,
  initialRole,
  initialPrompt,
  promptSource,
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
  onSubmit: (values: DispatchValues) => Promise<unknown>;
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
      wide
      onCancel={onCancel}
      onSubmit={async () => {
        const finalRole = role === "custom" ? customRole.trim() : role;
        if (!finalRole || !prompt.trim()) return false;
        return onSubmit({
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
            <option value="muse">Muse Code</option>
            <option value="opencode">OpenCode</option>
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
            <option value="">En este equipo (sin Claude Code instalado, lo hace Codex)</option>
            <option value="windows">En este equipo, solo con Claude Code</option>
            <option value="wsl">En WSL</option>
          </select>
        </Field>
      )}
      {limitedClaude && (
        <Field
          label="Comprobaciones del repositorio"
          hint="Uno por línea: los comandos que verifican el trabajo. Claude puede ejecutar cualquier comando, también encadenado, salvo los pasos de Tinto (commit, push, merge, rebase, checkout…) y las instalaciones globales. Se guardan para este repositorio y también los usan los trabajos del coordinador."
        >
          <GrowingTextarea
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
          <GrowingTextarea
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
        <GrowingTextarea
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
  onSubmit,
  onCancel,
}: {
  name: string;
  note: string | null;
  onSubmit: (confirmation: string) => Promise<unknown>;
  onCancel: () => void;
}) {
  const [confirmation, setConfirmation] = useState("");
  return (
    <FormDialog
      title={`Liberar el recurso ${name}`}
      submitLabel="Liberar"
      onCancel={onCancel}
      onSubmit={async () => (confirmation.trim() ? onSubmit(confirmation) : false)}
    >
      {note && <p className="file-op-modal__body">{note}</p>}
      <p className="file-op-modal__body">
        Antes de liberarlo, comprueba que no queden procesos, navegadores ni servidores del trabajo
        anterior y que los datos estén como al principio.
      </p>
      <Field label="Qué comprobaste">
        <GrowingTextarea
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
  onUnarchive,
  onCancel,
}: {
  onUnarchive: (taskId: string) => Promise<unknown>;
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
      onCancel={onCancel}
      onSubmit={() => search(query)}
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
              <ActionButton className="file-op-modal__button" onAction={() => onUnarchive(task.id)}>
                Desarchivar
              </ActionButton>
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
  onSubmit,
  onCancel,
}: {
  steps: CorrectableStep[];
  initial: string;
  onSubmit: (approvalId: string, outcome: string, reason: string) => Promise<unknown>;
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
      onCancel={onCancel}
      onSubmit={async () =>
        step && outcome.trim() && reason.trim() ? onSubmit(step.id, outcome, reason) : false
      }
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
        <GrowingTextarea
          value={outcome}
          onChange={(event) => setOutcome(event.target.value)}
          rows={3}
          required
        />
      </Field>
      <Field label="Por qué se corrige">
        <GrowingTextarea
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

/** One stage's agent in Ajustes: not fixed, or an agent with its model, reasoning and time limit. */
export function StageAgentRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: DeliveryStageAgent | undefined;
  onChange: (next: DeliveryStageAgent | null) => void;
}) {
  const shown = value ? agentChoice(value) : null;
  const timeout = value?.timeout_minutes ?? null;
  const set = (next: AgentChoice, timeoutMinutes = timeout) =>
    onChange({
      agent: next.agent,
      model: next.model || null,
      reasoning: next.reasoning || null,
      timeout_minutes: timeoutMinutes,
    });
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
          <option value="muse">Muse Code</option>
          <option value="opencode">OpenCode</option>
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
          <Field label="Tiempo máximo (min)">
            {/* Saved when the field is left, not on every key; shows the saved limit again
                when it changes. */}
            <input
              key={timeout ?? ""}
              type="number"
              min={1}
              aria-label={`Tiempo máximo de ${label}`}
              placeholder="El del rol"
              defaultValue={timeout ?? ""}
              onBlur={(event) => {
                const next = positive(event.target.value);
                if (next !== timeout) set(shown, next);
              }}
            />
          </Field>
        </>
      )}
    </div>
  );
}

/** What QA can use in a repo and who runs each stage, set once so
 *  coordinators read it instead of asking on every task. */
function RepoSettings({
  repo,
  label,
  onBack,
  tabs,
  section,
  after,
}: {
  repo: string;
  label: string;
  onBack: () => void;
  tabs: ReactNode;
  /** The section to open on. */
  section?: string;
  /** Sections saved apart, after the repo's own (its guides). */
  after: ReactNode;
}) {
  const [loaded, setLoaded] = useState<DeliveryRepoSettings | null>(null);
  const [autonomy, setAutonomy] = useState<DeliveryAutonomy>(DEFAULT_AUTONOMY);
  // The section in sight, marked in the list; the list scrolls the page to one.
  const page = useRef<HTMLDivElement>(null);
  const [current, setCurrent] = useState(section ?? SETTINGS_SECTIONS[0].id);
  const sectionAt = (id: string) =>
    page.current?.querySelector<HTMLElement>(`[data-section="${id}"]`) ?? null;
  const spy = () => {
    const top = page.current?.scrollTop ?? 0;
    const seen = SETTINGS_SECTIONS.filter((section) => {
      const heading = sectionAt(section.id);
      return heading && heading.offsetTop - (page.current?.offsetTop ?? 0) <= top + 24;
    });
    setCurrent((seen.at(-1) ?? SETTINGS_SECTIONS[0]).id);
  };
  const [browser, setBrowser] = useState(false);
  const [commands, setCommands] = useState("");
  const [setup, setSetup] = useState("");
  // `NAME=value` lines.
  const [checkEnv, setCheckEnv] = useState("");
  const [environment, setEnvironment] = useState("");
  // The QA stack Tinto prepares; no stack while its name is empty.
  const [stackName, setStackName] = useState("");
  const [stackUp, setStackUp] = useState("");
  // `folder: service service` lines.
  const [stackServices, setStackServices] = useState("");
  const [stackReady, setStackReady] = useState("");
  const [stackWarm, setStackWarm] = useState("");
  const [stackSeed, setStackSeed] = useState("");
  const [stackPreview, setStackPreview] = useState("");
  const [verification, setVerification] = useState("");
  // "yes", "no", or "" while the user has not set it.
  const [paidCalls, setPaidCalls] = useState("");
  // "" keeps Tinto's default.
  const [maxFiles, setMaxFiles] = useState("");
  const [maxLines, setMaxLines] = useState("");
  const [stages, setStages] = useState<Record<string, DeliveryStageAgent>>({});
  const [retryOnRestart, setRetryOnRestart] = useState(false);
  const [prWithoutReviewer, setPrWithoutReviewer] = useState(false);
  const [probes, setProbes] = useState<string[]>([]);
  const [jiraHost, setJiraHost] = useState("");
  const [jiraProject, setJiraProject] = useState("");
  const [jiraToken, setJiraToken] = useState("");
  const [afterPr, setAfterPr] = useState("");
  const [afterMerge, setAfterMerge] = useState("");
  const [jiraCheck, setJiraCheck] = useState("");
  const [status, setStatus] = useState("");
  // Changed since it was read or saved: every field's change bubbles up to the section.
  const [edited, setEdited] = useState(false);
  useEffect(() => {
    let active = true;
    getDeliveryRepoSettings(repo).then(
      (settings) => {
        if (!active) return;
        setLoaded(settings);
        setBrowser(settings.qa_browser ?? false);
        setCommands((settings.qa_commands ?? []).join("\n"));
        setSetup((settings.setup ?? []).join("\n"));
        setCheckEnv(
          Object.entries(settings.check_env ?? {})
            .map(([name, value]) => `${name}=${value}`)
            .join("\n"),
        );
        setEnvironment(settings.qa_environment ?? "");
        const stack = settings.qa_stack;
        setStackName(stack?.name ?? "");
        setStackUp(stack?.up ?? "");
        setStackServices(
          Object.entries(stack?.services ?? {})
            .map(([folder, built]) => `${folder}: ${built.join(" ")}`)
            .join("\n"),
        );
        setStackReady(stack?.ready_url ?? "");
        setStackWarm((stack?.warm_paths ?? []).join("\n"));
        setStackSeed((stack?.seed ?? []).join("\n"));
        setStackPreview(stack?.preview ?? "");
        setVerification(settings.verification ?? "");
        setMaxFiles(settings.single_commit_max_files?.toString() ?? "");
        setMaxLines(settings.single_commit_max_lines?.toString() ?? "");
        setPaidCalls(
          settings.paid_model_calls === true
            ? "yes"
            : settings.paid_model_calls === false
              ? "no"
              : "",
        );
        setStages(settings.stages ?? {});
        setRetryOnRestart(settings.retry_on_restart ?? false);
        setPrWithoutReviewer(settings.pr_without_reviewer ?? false);
        setAutonomy({ ...DEFAULT_AUTONOMY, ...settings.autonomy });
        setProbes(settings.environment_probes ?? []);
        setJiraHost(settings.jira?.host ?? "");
        setJiraProject(settings.jira?.project ?? "");
        setAfterPr((settings.jira?.after_pr ?? []).join(", "));
        setAfterMerge((settings.jira?.after_merge ?? []).join(", "));
        setJiraToken("");
        setJiraCheck("");
        setStatus("");
        setEdited(false);
      },
      () => active && setStatus("No se pudieron leer los ajustes de este repositorio."),
    );
    return () => {
      active = false;
    };
  }, [repo]);
  const markEdited = () => {
    setEdited(true);
    setStatus("");
  };
  // Opened on a section: scrolled to it once its fields are in place.
  const ready = loaded !== null;
  useEffect(() => {
    if (section && ready) sectionAt(section)?.scrollIntoView?.({ block: "start" });
    // Only on opening: scrolling afterwards is the user's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);
  return (
    <SettingsFrame
      onBack={onBack}
      tabs={tabs}
      pageRef={page}
      onScroll={spy}
      rail={
        <nav className="delivery-settings__rail" aria-label="Secciones de los ajustes">
          {SETTINGS_GROUPS.map((group) => (
            <div key={group.label} className="delivery-settings__group">
              <span>{group.label}</span>
              {SETTINGS_SECTIONS.filter((item) => item.group === group.label).map((item) => (
                <button
                  key={item.id}
                  type="button"
                  aria-current={current === item.id ? "true" : undefined}
                  onClick={() => {
                    setCurrent(item.id);
                    sectionAt(item.id)?.scrollIntoView?.({ block: "start" });
                  }}
                >
                  {item.label}
                </button>
              ))}
            </div>
          ))}
        </nav>
      }
      actions={
        <div className="delivery-settings__save">
          <ActionButton
            className="delivery-button--primary"
            disabled={!loaded}
            onAction={async () =>
              loaded &&
              setDeliveryRepoSettings(repo, {
                ...loaded,
                qa_browser: browser,
                qa_commands: parseChecks(commands),
                setup: parseChecks(setup),
                check_env: parseEnv(checkEnv),
                qa_environment: environment.trim(),
                qa_stack: stackName.trim()
                  ? {
                      name: stackName.trim(),
                      up: stackUp.trim(),
                      services: parseServices(stackServices),
                      ready_url: stackReady.trim(),
                      warm_paths: parseChecks(stackWarm),
                      seed: parseChecks(stackSeed),
                      preview: stackPreview.trim(),
                    }
                  : null,
                verification: verification.trim(),
                paid_model_calls: paidCalls === "" ? null : paidCalls === "yes",
                single_commit_max_files: positive(maxFiles),
                single_commit_max_lines: positive(maxLines),
                stages,
                retry_on_restart: retryOnRestart,
                pr_without_reviewer: prWithoutReviewer,
                autonomy,
                environment_probes: probes,
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
                  setEdited(false);
                  setStatus(`Guardado para ${label}.`);
                  return saved;
                })
                .catch((cause) =>
                  setStatus(`No se pudo guardar: ${String(cause?.message ?? cause)}`),
                )
            }
          >
            Guardar
          </ActionButton>
          <span className="delivery-field__hint" role="status">
            {status || (edited ? "Cambios sin guardar." : "")}
          </span>
        </div>
      }
    >
      <section
        className="delivery-settings__section"
        aria-label={`Ajustes de ${label}`}
        onChange={markEdited}
      >
        <p className="delivery-settings__intro">
          Lo que la QA puede usar en este repositorio, quién corre cada etapa y qué hace Tinto por
          su cuenta. Los coordinadores lo leen aquí y no lo preguntan en cada tarea.
        </p>
        <h3 data-section="autonomy">Autonomía</h3>
        <p className="delivery-settings__intro">
          Qué pasos da Tinto por su cuenta cuando el lote termina su brainstorm, y cuáles te espera.
          Una sincronización, en cualquier momento.
        </p>
        <AutonomyTable
          value={autonomy}
          onChange={(next) => {
            setAutonomy(next);
            markEdited();
          }}
        />
        <h3 data-section="agents">Agente de cada etapa</h3>
        <p className="delivery-settings__intro">
          Con qué agente, modelo y razonamiento corre cada etapa. El diálogo de lanzar etapa parte
          de aquí y los coordinadores lo usan en lugar de elegir ellos.
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
        <h3 data-section="environment">Comprobaciones y entorno</h3>
        <Field
          label="Comandos de preparación"
          hint="Uno por línea: instalaciones y generadores que una tarea necesita antes de compilar y probar (cada paquete, los bindings…). El coordinador elige los de cada tarea y Tinto los corre en su worktree antes de las etapas, fuera de cualquier sandbox."
        >
          <GrowingTextarea
            value={setup}
            onChange={(event) => setSetup(event.target.value)}
            rows={3}
            placeholder="npm --prefix frontend ci"
          />
        </Field>
        <label className="delivery-check">
          <input
            type="checkbox"
            checked={retryOnRestart}
            onChange={(event) => setRetryOnRestart(event.target.checked)}
          />
          <span>
            Reintentar al reiniciar
            <small>
              Si Tinto se cierra con etapas en marcha, al abrirse las vuelve a poner en cola una
              vez, con el mismo encargo. Nunca las que pudieron dejar algo a medias fuera del
              worktree (acceso completo, comandos de shell, la QA con su recurso en cuarentena):
              esas quedan interrumpidas con el motivo.
            </small>
          </span>
        </label>
        <div className="delivery-field">
          <span className="delivery-field__label">Sondas de entorno</span>
          {(loaded?.checks ?? []).length === 0 ? (
            <small className="delivery-field__hint">
              Sin comandos de verificación todavía: se añaden al lanzar una etapa.
            </small>
          ) : (
            (loaded?.checks ?? []).map((check) => (
              <label key={check} className="delivery-check">
                <input
                  type="checkbox"
                  checked={probes.includes(check)}
                  onChange={(event) =>
                    setProbes((current) =>
                      event.target.checked
                        ? [...current, check]
                        : current.filter((probe) => probe !== check),
                    )
                  }
                />
                <code>{check}</code>
              </label>
            ))
          )}
          <small className="delivery-field__hint">
            Los comandos de verificación marcados se corren en «Entorno», en la barra del lote,
            junto con Docker si el repositorio lo usa. Marca solo los rápidos que dicen si el
            entorno responde (por ejemplo, un ping a la base de datos).
          </small>
        </div>
        <h3 data-section="qa">QA</h3>
        <Field
          label="Entorno de QA"
          hint="Dónde se prueba en esta máquina: plataforma, ámbito, clientes. Por ejemplo: Windows, ámbito personal; Claude Code y Codex con mis sesiones."
        >
          <GrowingTextarea
            value={environment}
            onChange={(event) => setEnvironment(event.target.value)}
            rows={2}
          />
        </Field>
        <h3 data-section="stack">Pila de QA</h3>
        <p className="delivery-settings__intro">
          Antes de cada QA con navegador, Tinto reconstruye solo los servicios cuyas carpetas cambió
          la tarea, espera a que la pila responda y abre las páginas de QA para que estén
          compiladas. El tiempo que tarda no cuenta en el límite del trabajo. Si la pila ya sirve el
          worktree de la tarea en el mismo commit, no reconstruye nada. Sin nombre, no hay pila.
        </p>
        <Field
          label="Nombre"
          hint="El proyecto de compose, por ejemplo agentos-dev. La QA con navegador ocupa el recurso qa:browser:<nombre>; una QA de línea de comandos en su propio proyecto corre a la vez."
        >
          <input value={stackName} onChange={(event) => setStackName(event.target.value)} />
        </Field>
        <Field
          label="Comando que la levanta"
          hint="Corre en el worktree de la tarea. {services} se cambia por los servicios a reconstruir."
        >
          <GrowingTextarea
            value={stackUp}
            onChange={(event) => setStackUp(event.target.value)}
            rows={2}
            placeholder="docker compose -p agentos-dev -f docker/compose/docker-compose.yml --env-file ../agentos/docker/compose/.env up -d --build --no-deps {services}"
          />
        </Field>
        <Field
          label="Servicios por carpeta"
          hint="Una carpeta por línea con los servicios que se construyen de ella: carpeta: servicio servicio."
        >
          <GrowingTextarea
            value={stackServices}
            onChange={(event) => setStackServices(event.target.value)}
            rows={3}
            placeholder={
              "frontend: frontend\napi-gateway: api-gateway\nservices/workflows: workflows"
            }
          />
        </Field>
        <Field label="URL de disponibilidad" hint="Responde cuando la pila está lista.">
          <input
            value={stackReady}
            onChange={(event) => setStackReady(event.target.value)}
            placeholder="http://localhost:8080/"
          />
        </Field>
        <Field
          label="Páginas a calentar"
          hint="Una ruta por línea; Tinto las pide con los módulos que cargan antes de que empiece la QA. Vacío: la URL de disponibilidad."
        >
          <GrowingTextarea
            value={stackWarm}
            onChange={(event) => setStackWarm(event.target.value)}
            rows={2}
            placeholder={"/login\n/app/workflows"}
          />
        </Field>
        <Field
          label="Datos de prueba"
          hint="Comandos, uno por línea, que corren cuando la pila responde: usuarios y datos de prueba."
        >
          <GrowingTextarea
            value={stackSeed}
            onChange={(event) => setStackSeed(event.target.value)}
            rows={2}
          />
        </Field>
        <Field
          label="Build de producción"
          hint="Opcional: un comando que sirve el frontend compilado para producción en lugar del servidor de desarrollo, sin compilar cada página al abrirla."
        >
          <GrowingTextarea
            value={stackPreview}
            onChange={(event) => setStackPreview(event.target.value)}
            rows={2}
          />
        </Field>
        <Field
          label="Comandos de QA"
          hint="Uno por línea: el comando real, nunca un script que los agentes puedan editar. Pueden escribir fuera del worktree."
        >
          <GrowingTextarea
            value={commands}
            onChange={(event) => setCommands(event.target.value)}
            rows={2}
            placeholder="agentos skills install"
          />
        </Field>
        <Field
          label="Variables de entorno de verificación y QA"
          hint="Una por línea, NOMBRE=valor. Los trabajos corren con ellas los comandos de verificación y de QA; por ejemplo, la base de datos de pruebas. {task} en un valor se cambia por la clave de la tarea (AGOS-474 → agos_474), así cada tarea tiene la suya; {job}, por el identificador corto del trabajo, así cada comprobación y cada sesión de agente tiene la suya. Son ajustes de prueba, no secretos: se ven tal cual."
        >
          <GrowingTextarea
            value={checkEnv}
            onChange={(event) => setCheckEnv(event.target.value)}
            rows={2}
            placeholder="TEST_DB_MONGO_URL=mongodb://localhost:27017/tinto_{task}"
          />
        </Field>
        <Field label="Navegador para la QA">
          <input
            type="checkbox"
            checked={browser}
            onChange={(event) => setBrowser(event.target.checked)}
          />
        </Field>
        <Field
          label="Cómo se verifica"
          hint="Además de las etapas: pruebas de punta a punta, QA en navegador cuando hay interfaz, en qué entorno… Los coordinadores lo leen y no lo preguntan en cada issue."
        >
          <GrowingTextarea
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
        <h3 data-section="commits">Commits</h3>
        <Field
          label="Un solo commit, como mucho"
          hint="Un coordinador que proponga un único commit más grande, o que mezcle código, tests y documentación, debe justificarlo."
        >
          <span className="delivery-field-row">
            <input
              aria-label="Archivos de un único commit"
              type="number"
              min={1}
              value={maxFiles}
              placeholder="5"
              onChange={(event) => setMaxFiles(event.target.value)}
            />
            archivos y
            <input
              aria-label="Líneas de un único commit"
              type="number"
              min={1}
              value={maxLines}
              placeholder="300"
              onChange={(event) => setMaxLines(event.target.value)}
            />
            líneas
          </span>
        </Field>
        <h3 data-section="pr">PR y fusión</h3>
        <label className="delivery-check">
          <input
            type="checkbox"
            checked={prWithoutReviewer}
            onChange={(event) => setPrWithoutReviewer(event.target.checked)}
          />
          <span>
            PR sin revisor
            <small>
              Puedes aprobar el paso PR sin elegir revisor y fusionarlo sin una revisión aprobada,
              para repositorios donde nadie más revisa. Sigue haciendo falta la CI en verde, la
              aprobación de los revisores que elijas y que nadie haya pedido cambios.
            </small>
          </span>
        </label>
        <h3 data-section="jira">Jira</h3>
        <p className="delivery-settings__intro">
          Con Jira configurado, Tinto hace los pasos Jira al aprobarlos: enlaza el PR, lleva la
          issue por estos estados y comprueba el estado final. Sin él, los hace el coordinador.
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
        <div className="delivery-actions delivery-settings__aside">
          <ActionButton
            disabled={!loaded?.jira?.has_token}
            title={loaded?.jira?.has_token ? undefined : "Guarda antes el host y el token."}
            onAction={() => {
              setJiraCheck("Comprobando…");
              return checkDeliveryJira(repo).then(
                (name) => setJiraCheck(`Conectado como ${name}.`),
                (cause) => setJiraCheck(`No se pudo: ${String(cause?.message ?? cause)}`),
              );
            }}
          >
            Comprobar Jira
          </ActionButton>
          <span className="delivery-field__hint" role="status">
            {jiraCheck}
          </span>
        </div>
      </section>
      {after}
    </SettingsFrame>
  );
}

/** The settings screen's frame: a bar that stays put (back, title, the
 *  scopes and the tab's save) over a list of sections and the page. */
function SettingsFrame({
  onBack,
  tabs,
  actions,
  rail,
  pageRef,
  onScroll,
  children,
}: {
  onBack: () => void;
  tabs: ReactNode;
  actions?: ReactNode;
  rail?: ReactNode;
  pageRef?: RefObject<HTMLDivElement | null>;
  onScroll?: () => void;
  children: ReactNode;
}) {
  return (
    <div className="delivery-settings">
      <header className="delivery-settings__bar">
        <button type="button" className="delivery-settings__back" onClick={onBack}>
          ← Volver
        </button>
        <h2>Ajustes</h2>
        {actions}
      </header>
      {tabs}
      <div className="delivery-settings__body">
        {rail}
        <div className="delivery-settings__page" ref={pageRef} onScroll={onScroll}>
          {children}
        </div>
      </div>
    </div>
  );
}

type SettingsScope = "tinto" | "delivery" | "repo";

/** Every setting, by where it applies: Tinto itself, all of Delivery, or one
 *  repository. */
export function SettingsPage({
  repos,
  repo,
  section,
  onBack,
  ...wide
}: {
  repos: RepoChoice[];
  repo: RepoChoice;
  /** The repo's section to open on. */
  section?: string;
  onBack: () => void;
} & DeliveryWideProps) {
  const [scope, setScope] = useState<SettingsScope>("repo");
  const [chosen, setChosen] = useState(repo);
  const choices = repos.some((candidate) => candidate.path === repo.path)
    ? repos
    : [repo, ...repos];
  const tab = (value: SettingsScope, label: ReactNode) => (
    <button
      type="button"
      role="tab"
      aria-selected={scope === value}
      onClick={() => setScope(value)}
    >
      {label}
    </button>
  );
  const tabs = (
    <div className="delivery-settings__tabs" role="tablist" aria-label="Ámbito de los ajustes">
      {tab("tinto", "Tinto")}
      {tab("delivery", "Delivery")}
      {tab("repo", `Repositorio: ${chosen.label}`)}
      {scope === "repo" && choices.length > 1 && (
        <select
          aria-label="Repositorio"
          value={chosen.path}
          onChange={(event) =>
            setChosen(choices.find((candidate) => candidate.path === event.target.value) ?? repo)
          }
        >
          {choices.map((candidate) => (
            <option key={candidate.path} value={candidate.path}>
              {candidate.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );
  if (scope === "tinto") {
    return (
      <SettingsFrame onBack={onBack} tabs={tabs}>
        <TintoSettings />
      </SettingsFrame>
    );
  }
  if (scope === "delivery") {
    return (
      <SettingsFrame onBack={onBack} tabs={tabs}>
        <section className="delivery-settings__section" aria-label="Ajustes de Delivery">
          <h3>Delivery</h3>
          <p className="delivery-settings__intro">
            Vale para todos los repositorios y se guarda al cambiarlo.
          </p>
          <DeliveryWideSettings {...wide} />
        </section>
      </SettingsFrame>
    );
  }
  return (
    <RepoSettings
      key={chosen.path}
      repo={chosen.path}
      label={chosen.label}
      onBack={onBack}
      tabs={tabs}
      section={chosen.path === repo.path ? section : undefined}
      after={<GuidesSection repo={chosen} />}
    />
  );
}

/** What applies to Tinto itself, saved as it changes. */
function TintoSettings() {
  const theme = useThemeSetting();
  const quality = useQualityState();
  return (
    <section className="delivery-settings__section" aria-label="Ajustes de Tinto">
      <h3>Tinto</h3>
      <p className="delivery-settings__intro">Se guarda al cambiarlo.</p>
      <Field label="Tema">
        <select
          value={theme}
          onChange={(event) => themeStore.set(event.target.value as ThemeSetting)}
        >
          {THEME_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </Field>
      <label className="delivery-check">
        <input
          type="checkbox"
          checked={quality.notificationsEnabled}
          onChange={(event) =>
            event.target.checked ? void enableNotifications() : disableNotifications()
          }
        />
        <span>
          Notificaciones
          <small>Avisos del sistema cuando algo te necesita y Tinto no está a la vista.</small>
        </span>
      </label>
      <label className="delivery-check">
        <input
          type="checkbox"
          checked={quality.glanceMode}
          onChange={(event) => qualityStore.setGlanceMode(event.target.checked)}
        />
        <span>
          Vista rápida
          <small>La misma opción que el menú Ver.</small>
        </span>
      </label>
    </section>
  );
}

/** The groups of a repo's sections, as its list shows them. */
const SETTINGS_GROUPS = [
  { label: "Cómo trabaja" },
  { label: "Verificación" },
  { label: "Entrega" },
  { label: "Aprendizaje" },
];

/** A repo's sections, in page order, each in its group. */
const SETTINGS_SECTIONS = [
  { id: "autonomy", label: "Autonomía", group: "Cómo trabaja" },
  { id: "agents", label: "Agentes por etapa", group: "Cómo trabaja" },
  { id: "environment", label: "Comprobaciones y entorno", group: "Verificación" },
  { id: "qa", label: "QA", group: "Verificación" },
  { id: "stack", label: "Pila de QA", group: "Verificación" },
  { id: "commits", label: "Commits", group: "Entrega" },
  { id: "pr", label: "PR y fusión", group: "Entrega" },
  { id: "jira", label: "Jira", group: "Entrega" },
  { id: "guides", label: "Guías", group: "Aprendizaje" },
];

/** Tinto's own defaults, for a repo saved before autonomy existed. */
const DEFAULT_AUTONOMY: DeliveryAutonomy = {
  decisions: true,
  permissions: true,
  full_access: true,
  settings_proposals: true,
  qa_problems: true,
  commit: true,
  sync: true,
  push: true,
  pr: false,
  merge: false,
  jira: true,
  pr_reply: false,
};

/** Every step Tinto can take on its own, in the order a task meets them. */
const AUTONOMY_STEPS: { key: keyof DeliveryAutonomy; label: string; hint: string }[] = [
  {
    key: "decisions",
    label: "Decisiones",
    hint: "La opción recomendada; los textos, como vienen.",
  },
  {
    key: "permissions",
    label: "Permisos",
    hint: "Correr un comando, leer o escribir fuera del worktree.",
  },
  { key: "full_access", label: "QA con acceso total", hint: "Una QA sin su sandbox." },
  { key: "settings_proposals", label: "Propuestas de ajustes", hint: "Se aplican enteras." },
  {
    key: "qa_problems",
    label: "Problemas de QA",
    hint: "Van al coordinador con la primera sugerencia de la QA.",
  },
  { key: "commit", label: "Commit", hint: "El plan de commits del coordinador." },
  { key: "sync", label: "Sincronizar", hint: "Traer la base a la rama local, sin publicar." },
  { key: "push", label: "Push", hint: "Publicar la rama." },
  { key: "pr", label: "Abrir el PR", hint: "Sin revisores, salvo los que elija el coordinador." },
  { key: "merge", label: "Fusionar", hint: "Cuando la CI y las revisiones lo permiten." },
  { key: "jira", label: "Jira", hint: "Los pasos que Tinto hace él mismo." },
  {
    key: "pr_reply",
    label: "Respuestas en el PR",
    hint: "Publicar en tu nombre una respuesta a un revisor.",
  },
];

/** One row per step: whether it waits for the user or Tinto takes it. */
function AutonomyTable({
  value,
  onChange,
}: {
  value: DeliveryAutonomy;
  onChange: (next: DeliveryAutonomy) => void;
}) {
  return (
    <div className="delivery-autonomy" role="group" aria-label="Autonomía por paso">
      {AUTONOMY_STEPS.map((step) => (
        <label key={step.key} className="delivery-autonomy__row">
          <span>
            {step.label}
            <small>{step.hint}</small>
          </span>
          <select
            aria-label={step.label}
            value={value[step.key] ? "auto" : "ask"}
            // The table saves with the page; a select's change must not
            // bubble to the section twice.
            onChange={(event) => {
              event.stopPropagation();
              onChange({ ...value, [step.key]: event.target.value === "auto" });
            }}
          >
            <option value="ask">Te pregunta</option>
            <option value="auto">Automático</option>
          </select>
        </label>
      ))}
    </div>
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

type DeliveryWideProps = {
  capacity: number;
  endpoint: DeliveryCoordinatorEndpoint | null;
  onCapacity: (capacity: number) => void;
};

/** What applies to Delivery in every repo: how many agents run at once and
 *  how an outside coordinator connects. */
export function DeliverySettingsDialog({
  onClose,
  ...wide
}: DeliveryWideProps & { onClose: () => void }) {
  return (
    <FormDialog title="Ajustes de Delivery" wide onCancel={onClose}>
      <DeliveryWideSettings {...wide} />
    </FormDialog>
  );
}

/** How many agents run at once and how an outside coordinator connects:
 *  the same in the dialog and on a repo's settings page. */
function DeliveryWideSettings({ capacity, endpoint, onCapacity }: DeliveryWideProps) {
  // The token is a credential: the page shows it masked and only the copy
  // buttons carry it.
  // Through Tinto's proxy as a stdio server, which updates leave running,
  // finds Tinto's server
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
  return (
    <>
      <Field
        label="Agentes en paralelo"
        hint="Automático: uno por cada tarea activa (cada issue), hasta uno por núcleo físico menos uno (para las comprobaciones de Tinto), que corren de una en una. Los demás esperan en cola, en orden de llegada. Las conversaciones de Agents no cuentan."
      >
        <select value={capacity} onChange={(event) => onCapacity(Number(event.target.value))}>
          <option value={0}>Automático: uno por issue</option>
          {[1, 2, 3, 4, 5, 6, 7, 8].map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
          <option value={UNLIMITED_CAPACITY}>Sin límite</option>
        </select>
      </Field>
      <h3 className="delivery-dialog__subtitle">Conectar un coordinador</h3>
      <p className="delivery-settings__intro">
        Un agente coordinador (por ejemplo la skill backlog-delivery) dirige Delivery mediante MCP:
        crea tareas, lanza etapas y pide aprobaciones.
      </p>
      {endpoint ? (
        <>
          <p className="delivery-settings__intro">
            Las conversaciones de Codex y Claude Code que abres en Agents ya tienen estas
            herramientas, sin pedir permiso para usarlas. Lo de abajo es para agentes que corren
            fuera de Tinto: arrancan el proxy de Tinto como servidor MCP, que sigue en marcha cuando
            Tinto se actualiza, abre Tinto si está cerrado y lo encuentra aunque cambie de puerto.
            Si lo añadiste antes con tinto.exe, vuelve a añadirlo con este comando: cada
            actualización de Tinto corta esa conexión.
          </p>
          <CopySnippet label="Claude Code" shown={claude(MASK)} copied={claude(endpoint.token)} />
          <CopySnippet
            label="Codex (config.toml)"
            shown={codex(MASK)}
            copied={codex(endpoint.token)}
          />
        </>
      ) : (
        <p className="delivery-settings__intro">El API de coordinador no está disponible.</p>
      )}
    </>
  );
}

/** The other batches the same conversation coordinates, after a batch's coordinator. */
function sharedWith(run: DeliveryRun, runs: DeliveryRun[]) {
  const others = runs.filter(
    (other) =>
      other.id !== run.id &&
      run.coordinator_session &&
      other.coordinator_session === run.coordinator_session,
  );
  return others.length > 0
    ? `, que coordina también ${others.map((other) => other.title || other.id).join(", ")}`
    : null;
}

/** The active batches, what can be done to each, and a new one: the
 *  header's batch menu. */
export function BatchList({
  runs,
  repos,
  onCreateRun,
  onQaJiraComment,
  onPause,
  onTakeover,
  onClose,
}: {
  runs: DeliveryRun[];
  repos: RepoChoice[];
  onCreateRun: (repo: string, title: string, qaJiraComment: boolean) => Promise<unknown>;
  onQaJiraComment: (run: DeliveryRun, post: boolean) => void;
  onPause: (run: DeliveryRun, paused: boolean) => Promise<unknown>;
  onTakeover: (run: DeliveryRun) => Promise<unknown>;
  onClose: (run: DeliveryRun) => Promise<unknown>;
}) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [title, setTitle] = useState("");
  const [jiraComment, setJiraComment] = useState(false);
  return (
    <div className="delivery-batches">
      <p className="delivery-muted">
        Un lote agrupa las tareas que dirige un coordinador. Las aprobaciones siempre las decides
        tú.
      </p>
      {runs.length === 0 && <p className="delivery-muted">Ningún lote activo.</p>}
      <ul className="delivery-run-list">
        {runs.map((run) => (
          <li key={run.id}>
            <span>
              <strong>{run.title || run.id}</strong>
              <small>
                {run.owner ? `coordina ${run.owner}` : "sin coordinador"}
                {run.coordinator_session &&
                  ` · conversación ${run.coordinator_session.slice(0, 8)}`}
                {sharedWith(run, runs)}
              </small>
            </span>
            <span className="delivery-run-list__actions">
              <select
                aria-label={`Resultado de QA en Jira del lote ${run.title || run.id}`}
                value={run.qa_jira_comment == null ? "" : run.qa_jira_comment ? "yes" : "no"}
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
              <ActionGroup>
                <ActionButton
                  aria-label={`${run.paused ? "Reanudar" : "Pausar"} el lote ${run.title || run.id}`}
                  onAction={() => onPause(run, !run.paused)}
                >
                  {run.paused ? "Reanudar" : "Pausar"}
                </ActionButton>
                {run.owner && run.owner !== "user" && (
                  <ActionButton
                    aria-label={`Tomar el control del lote ${run.title || run.id}`}
                    onAction={() => onTakeover(run)}
                  >
                    Tomar el control
                  </ActionButton>
                )}
                <ActionButton
                  aria-label={`Cerrar el lote ${run.title || run.id}`}
                  onAction={() => onClose(run)}
                >
                  Cerrar
                </ActionButton>
              </ActionGroup>
            </span>
          </li>
        ))}
      </ul>
      <form
        className="delivery-batches__new"
        aria-label="Nuevo lote"
        // The submit button sends it, also on Enter.
        onSubmit={(event) => event.preventDefault()}
      >
        <Field label="Nuevo lote">
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Lote de octubre"
          />
        </Field>
        <Field label="Repositorio">
          <select value={repo} onChange={(event) => setRepo(event.target.value)}>
            {repos.map((choice) => (
              <option key={choice.path} value={choice.path}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
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
        <ActionButton
          type="submit"
          className="delivery-button--primary"
          disabled={!repo || !title.trim()}
          onAction={async () => {
            await onCreateRun(repo, title.trim(), jiraComment);
            setTitle("");
          }}
        >
          Crear lote
        </ActionButton>
      </form>
    </div>
  );
}
