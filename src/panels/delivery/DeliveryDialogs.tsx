// Delivery forms, rendered as in-app modals (same look as the confirm
// dialog, and reachable by UI automation).

import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { useAccessibleDialog } from "../../workbench/useAccessibleDialog";
import { lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import type {
  DeliveryAccess,
  DeliveryAgent,
  DeliveryCoordinatorEndpoint,
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
  const [runId, setRunId] = useState("");
  const [bootstrap, setBootstrap] = useState("");
  const repo = repos.find((choice) => choice.path === repoPath);
  const activeRuns = runs.filter((run) => run.status === "active" && run.repo === repoPath);
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
        <select value={repoPath} onChange={(event) => setRepoPath(event.target.value)}>
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
          <select value={runId} onChange={(event) => setRunId(event.target.value)}>
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
  { value: "qa", label: "QA", hint: "Solo lectura y exclusivo: usa el recurso QA mientras corre." },
] as const;

export interface DispatchValues {
  role: string;
  agent: DeliveryAgent;
  model: string;
  access: DeliveryAccess;
  prompt: string;
  /** The repo's verification commands when they were edited, else null. */
  checks: string[] | null;
}

function parseChecks(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function DispatchDialog({
  task,
  checks,
  initialRole,
  initialPrompt,
  promptSource,
  busy,
  onSubmit,
  onCancel,
}: {
  task: DeliveryTask;
  /** The repo's verification commands, once loaded. */
  checks: string[] | null;
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
  const shownChecks = checksText ?? (checks ?? []).join("\n");
  const limitedClaude = agent === "claude" && access === "workspace";
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
          checks: checksText === null ? null : parseChecks(checksText),
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

export function SettingsDialog({
  capacity,
  endpoint,
  runs,
  repos,
  busy,
  onCapacity,
  onCreateRun,
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
  onCreateRun: (repo: string, title: string) => void;
  onTakeover: (run: DeliveryRun) => void;
  onClose: (run: DeliveryRun) => void;
  onCancel: () => void;
}) {
  const [repo, setRepo] = useState(repos[0]?.path ?? "");
  const [title, setTitle] = useState("");
  const claude = endpoint
    ? `claude mcp add --transport http tinto-delivery ${endpoint.url} --header "Authorization: Bearer ${endpoint.token}"`
    : "";
  const codex = endpoint
    ? `[mcp_servers.tinto-delivery]\nurl = "${endpoint.url}"\nhttp_headers = { Authorization = "Bearer ${endpoint.token}" }`
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
              {run.owner && run.owner !== "user" && (
                <button type="button" disabled={busy} onClick={() => onTakeover(run)}>
                  Tomar el control
                </button>
              )}
              <button type="button" disabled={busy} onClick={() => onClose(run)}>
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
      <button
        type="button"
        className="file-op-modal__button"
        disabled={busy || !repo || !title.trim()}
        onClick={() => {
          onCreateRun(repo, title.trim());
          setTitle("");
        }}
      >
        Crear lote
      </button>

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
          <Field label="Claude Code">
            <textarea readOnly rows={3} value={claude} className="delivery-code" />
          </Field>
          <Field label="Codex (config.toml)">
            <textarea readOnly rows={3} value={codex} className="delivery-code" />
          </Field>
        </>
      ) : (
        <p className="file-op-modal__body">El API de coordinador no está disponible.</p>
      )}
    </FormDialog>
  );
}
