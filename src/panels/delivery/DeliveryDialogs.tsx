// Delivery forms, rendered as in-app modals (same look as the confirm
// dialog, and reachable by UI automation).

import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { getDeliveryRepoSettings, setDeliveryRepoSettings } from "../../delivery/client";
import { useAccessibleDialog } from "../../workbench/useAccessibleDialog";
import { lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryCoordinatorEndpoint,
  DeliveryRepoSettings,
  DeliveryRun,
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

export interface DispatchValues {
  role: string;
  agent: DeliveryAgent;
  model: string;
  access: DeliveryAccess;
  prompt: string;
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
  initialRole,
  initialPrompt,
  promptSource,
  busy,
  onSubmit,
  onCancel,
}: {
  task: DeliveryTask;
  /** The repo's settings (checks and QA tools), once loaded. */
  settings: DeliveryRepoSettings | null;
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
  const defaultCodexModel = lastRuntimeCatalog()?.default_model ?? "";
  const builtIn = DELIVERY_ROLES.some((choice) => choice.value === initialRole);
  const [role, setRole] = useState<string>(builtIn ? initialRole : "custom");
  const [customRole, setCustomRole] = useState(builtIn ? "" : initialRole);
  const [agent, setAgent] = useState<DeliveryAgent>("codex");
  const [model, setModel] = useState(defaultCodexModel);
  const [access, setAccess] = useState<DeliveryAccess>("workspace");
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
          model: model.trim(),
          access,
          prompt,
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
          <select value={role} onChange={(event) => setRole(event.target.value)}>
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
            onChange={(event) => {
              const next = event.target.value as DeliveryAgent;
              setAgent(next);
              setModel(next === "codex" ? defaultCodexModel : "");
            }}
          >
            <option value="codex">Codex</option>
            <option value="claude">Claude Code</option>
          </select>
        </Field>
        <Field label="Modelo">
          <input
            value={model}
            onChange={(event) => setModel(event.target.value)}
            placeholder="predeterminado"
          />
        </Field>
      </div>
      <Field label="Acceso">
        <select
          value={access}
          onChange={(event) => setAccess(event.target.value as DeliveryAccess)}
        >
          <option value="workspace">Workspace</option>
          <option value="full">Acceso completo</option>
        </select>
      </Field>
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
          promptSource
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

/** What QA can use in a repo, set once so coordinators read it instead of
 *  asking on every task. */
function RepoQaSettings({ repos }: { repos: RepoChoice[] }) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [loaded, setLoaded] = useState<DeliveryRepoSettings | null>(null);
  const [browser, setBrowser] = useState(false);
  const [commands, setCommands] = useState("");
  const [environment, setEnvironment] = useState("");
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
    <section className="delivery-qa-settings" aria-label="QA del repositorio">
      <h3 className="delivery-dialog__subtitle">QA del repositorio</h3>
      <p className="file-op-modal__body">
        Lo que la QA puede usar en cada repositorio. Los coordinadores lo leen aquí y no lo
        preguntan en cada tarea.
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
            }).then(
              (saved) => {
                setLoaded(saved);
                setStatus(`Guardado para ${label}.`);
              },
              () => setStatus("No se pudo guardar."),
            )
          }
        >
          Guardar la QA de {label}
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
  onTakeover: (run: DeliveryRun) => void;
  onClose: (run: DeliveryRun) => void;
  onCancel: () => void;
}) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [title, setTitle] = useState("");
  const [jiraComment, setJiraComment] = useState(false);
  // The token is a credential: the dialog shows it masked and only the copy
  // buttons carry it.
  const claude = (token: string) =>
    endpoint
      ? `claude mcp add --transport http tinto-delivery ${endpoint.url} --header "Authorization: Bearer ${token}"`
      : "";
  const codex = (token: string) =>
    endpoint
      ? `[mcp_servers.tinto-delivery]\nurl = "${endpoint.url}"\nhttp_headers = { Authorization = "Bearer ${token}" }`
      : "";
  const activeRuns = runs.filter((run) => run.status === "active");
  return (
    <FormDialog title="Ajustes de Delivery" wide onCancel={onCancel}>
      <Field
        label="Agentes en paralelo"
        hint="Los demás esperan en cola, en orden de llegada. Las conversaciones de Agents no cuentan."
      >
        <select
          value={capacity}
          disabled={busy}
          onChange={(event) => onCapacity(Number(event.target.value))}
        >
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
            fuera de Tinto.
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
