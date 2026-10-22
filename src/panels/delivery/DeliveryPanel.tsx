// Delivery mode: tasks in their own worktree and branch, advanced by
// background agent stages. A separate way to work from Agents; the two only
// meet through "Abrir en Agents".

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type SetStateAction,
} from "react";
import { createPortal } from "react-dom";
import { DockviewReact, themeVisualStudio } from "dockview-react";
import type {
  DockviewApi,
  DockviewReadyEvent,
  IDockviewHeaderActionsProps,
  IDockviewPanelHeaderProps,
  IDockviewPanelProps,
  SerializedDockview,
} from "dockview-react";
import { busStore, useBusState } from "../../bus/store";
import { useAgentSessionState } from "../../agent/sessionStore";
import { useWorkspaceActions } from "../../workspace/actions";
import { setBaseView } from "../../workspace/baseView";
import {
  openDetachedPanelWindow,
  watchDetachedPanelWindow,
} from "../../workspace/detachPanelWindow";
import { armExternalTabDetach } from "../../workspace/externalTabDetach";
import { isWslRepoSource } from "../repoSource";
import { confirm } from "../../workbench/confirmDialog";
import {
  codexReasoningLabel,
  lastRuntimeCatalog,
  tellDelivery,
} from "../terminal/agentRuntimeCatalog";
import {
  archiveDeliveryTask,
  cancelDeliveryJob,
  clearDeliveryBlockers,
  openDeliveryUrl,
  getDeliveryIssueUrl,
  getDeliveryTaskChanges,
  openDeliveryVisualRecord,
  pauseDeliveryJob,
  resumeDeliveryJob,
  setDeliveryRunPaused,
  checkDeliveryTask,
  checkDeliveryEnvironment,
  closeDeliveryRun,
  closeDeliveryPr,
  handOverDeliveryRun,
  completeDeliveryApproval,
  deliveryPrToDraft,
  openDeliveryPr,
  correctDeliveryApproval,
  createDeliveryRun,
  createDeliveryTask,
  decideDeliveryApproval,
  dispatchDeliveryJob,
  findDeliveryPr,
  getDeliveryMergeMethod,
  getDeliveryPrReviewers,
  getDeliveryJobLog,
  getDeliveryOverview,
  getDeliveryRepoSettings,
  onDeliveryChanged,
  openDeliveryJobInAgents,
  openDeliveryTaskInAgents,
  releaseDeliveryLease,
  removeDeliveryTask,
  requestDeliveryApproval,
  resolveDeliveryJob,
  retryDeliveryJob,
  setDeliveryRepoSettings,
  setDeliveryRunQaJiraComment,
  takeoverDeliveryRun,
  undoDeliveryCommit,
  unarchiveDeliveryTask,
  undoDeliveryJob,
  updateDeliverySettings,
  updateDeliveryTask,
} from "../../delivery/client";
import type {
  DeliveryApproval,
  DeliveryEnvironmentCheck,
  DeliveryJob,
  DeliveryJobLog,
  DeliveryOverview,
  DeliveryPr,
  DeliveryQueueWait,
  DeliveryRepoSettings,
  DeliveryRun,
  DeliveryStageAgent,
  DeliveryTask,
  DeliveryTiming,
  DeliveryUsage,
} from "../../delivery/types";
import {
  DispatchDialog,
  ArchivedTasksDialog,
  BatchList,
  CorrectResultDialog,
  NewTaskDialog,
  ReleaseLeaseDialog,
  DeliverySettingsDialog,
  RepoSettingsPage,
  StageAgentRow,
  type CorrectableStep,
  type RepoChoice,
} from "./DeliveryDialogs";
import { CommitPlanForm, CommitPlanView } from "./DeliveryCommitPlan";
import { JiraIssue } from "./DeliveryIssue";
import { QaProblems } from "./DeliveryQaProblems";
import { AddBlocker, BlockedBar } from "./DeliveryBlockers";
import { CoordinatorDock } from "./DeliveryCoordinator";
import { coordinatorStalls, type CoordinatorStall } from "./DeliveryNotifications";
import { CoordinatorDecisions, DecisionLog, PendingDecisions } from "./DeliveryDecisions";
import { PendingSettingsProposals } from "./DeliverySettingsProposals";
import { PrSection } from "./DeliveryPr";
import {
  GROUPS,
  STEPS,
  TASK_STATE_LABEL,
  attemptOutcome,
  batchName,
  byCommand,
  historyKey,
  isBroken,
  isDelivered,
  isOpen,
  isSuperseded,
  jobsOf,
  lastHandoff,
  latestApproval,
  latestOfRole,
  lower,
  nextStageOf,
  latestStep,
  nextStep,
  openBlockers,
  parseStatuses,
  pausedJob,
  pendingSettingsProposals,
  prMood,
  prSummary,
  quietFor,
  rolesOf,
  stepKey,
  stepsOf,
  taskLabel,
  issueProgress,
  clusterPackages,
  stepOf,
  type DeliveryCurrentRound,
  type DeliveryStep,
  nowLine,
  roundHistory,
  roundOf,
  roundRung,
  roundStage,
  stageLabel,
  taskStatus,
  trackSteps,
  type StatusGroup,
  type StatusIcon,
  type TaskStatus,
  type Tone,
} from "./taskStatus";
import "./delivery.css";
import { ExternalLink } from "./ExternalLink";
import { ActionButton, ActionGroup } from "./ActionButton";
import { DeliveryRefreshContext } from "./deliveryRefresh";

type Dialog =
  | { kind: "new-task" }
  | { kind: "dispatch"; task: DeliveryTask; role: string; prompt: string; source: string | null }
  | { kind: "release"; name: string; note: string | null }
  | { kind: "archived" }
  | { kind: "settings" }
  | null;

/** What the task list shows: one batch (null: all of them) and the groups
 *  folded to their count. Delivered tasks start folded. */
interface ListView {
  lot: string | null;
  folded: StatusGroup[];
}

const LIST_VIEW_KEY = "tinto:delivery-list-view";

function savedListView(): ListView {
  try {
    const saved = JSON.parse(localStorage.getItem(LIST_VIEW_KEY) ?? "null") as ListView | null;
    if (saved) return saved;
  } catch {
    // Without storage the list opens as it does the first time.
  }
  return { lot: null, folded: ["delivered"] };
}

type IconName =
  | Exclude<StatusIcon, "live">
  | "more"
  | "down"
  | "right"
  | "gear"
  | "play"
  | "ticket"
  | "pull"
  | "merge"
  | "photo"
  | "bug";

const ICONS: Record<IconName, ReactNode> = {
  flag: <path d="M4 14V2.5M4 3h7.5l-1.8 2.6L11.5 8.2H4" />,
  x: <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />,
  alert: <path d="M8 2.5l6 11H2zM8 6.6v3.1M8 11.7v.1" />,
  clock: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <path d="M8 5v3.2l2.2 1.4" />
    </>
  ),
  lock: (
    <>
      <path d="M3.5 7.5h9v6h-9z" />
      <path d="M5.5 7.5V5a2.5 2.5 0 0 1 5 0v2.5M8 10v1.2" />
    </>
  ),
  ring: <circle cx="8" cy="8" r="4" />,
  check: <path d="M3.5 8.5l3 3 6-7" />,
  more: <path d="M3.5 8h.01M8 8h.01M12.5 8h.01" strokeWidth="2.6" />,
  down: <path d="M4 6l4 4 4-4" />,
  right: <path d="M6 4l4 4-4 4" />,
  gear: (
    <>
      <circle cx="8" cy="8" r="2.3" />
      <path d="M8 1.8v2M8 12.2v2M1.8 8h2M12.2 8h2M3.6 3.6L5 5M11 11l1.4 1.4M3.6 12.4L5 11M11 5l1.4-1.4" />
    </>
  ),
  play: <path d="M5 3.5l7 4.5-7 4.5z" />,
  ticket: (
    <path d="M2.5 5V3.5h11V5a1.5 1.5 0 0 0 0 3v0a1.5 1.5 0 0 0 0 3v1.5h-11V11a1.5 1.5 0 0 0 0-3 1.5 1.5 0 0 0 0-3zM9.5 4v8" />
  ),
  pull: (
    <>
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="4.5" cy="12.5" r="1.5" />
      <circle cx="11.5" cy="12.5" r="1.5" />
      <path d="M4.5 5v6M11.5 11V6.5A2 2 0 0 0 9.5 4.5H8" />
    </>
  ),
  merge: (
    <>
      <circle cx="4.5" cy="3.5" r="1.5" />
      <circle cx="4.5" cy="12.5" r="1.5" />
      <circle cx="11.5" cy="8" r="1.5" />
      <path d="M4.5 5v6M4.5 5c0 2.5 2 3 5.5 3" />
    </>
  ),
  photo: (
    <>
      <rect x="2.5" y="3.5" width="11" height="9" rx="1" />
      <path d="M2.5 11l3.5-3.5 3 3 2-2 2.5 2.5" />
    </>
  ),
  bug: (
    <>
      <rect x="5" y="5" width="6" height="8" rx="3" />
      <path d="M6 5a2 2 0 0 1 4 0M2.5 9H5M11 9h2.5M3 5.5l2 1.5M13 5.5l-2 1.5M3 12.5l2-1.5M13 12.5l-2-1.5" />
    </>
  ),
};

function Icon({ name }: { name: IconName }) {
  return (
    <svg className="delivery-icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      {ICONS[name]}
    </svg>
  );
}

function StatusMark({ icon }: { icon: StatusIcon }) {
  return icon === "live" ? (
    <span className="delivery-dot delivery-dot--live" aria-hidden />
  ) : (
    <Icon name={icon} />
  );
}

const TONE_ICON: Record<Tone, StatusIcon> = {
  attention: "alert",
  step: "flag",
  danger: "x",
  live: "live",
  waiting: "clock",
  blocked: "lock",
  idle: "ring",
  ok: "check",
};

function StatusLine({ status }: { status: TaskStatus }) {
  return (
    <span className={`delivery-status-line delivery-tone--${status.tone}`}>
      <StatusMark icon={status.icon} />
      {status.label}
    </span>
  );
}

/** A button that opens a short list of actions; closes on outside click or Escape.
 *  `floating` draws the list over the whole Delivery panel, for a button in a
 *  strip that would cut it off (the repo tabs'). */
function Menu({
  label,
  trigger,
  disabled,
  floating,
  children,
}: {
  label?: string;
  trigger: ReactNode;
  disabled?: boolean;
  floating?: boolean;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ host: Element; top: number; right: number } | null>(null);
  const toggle = () => {
    const host = floating ? ref.current?.closest(".delivery-panel") : null;
    if (!open && host && ref.current) {
      const button = ref.current.getBoundingClientRect();
      const panel = host.getBoundingClientRect();
      setPlace({ host, top: button.bottom - panel.top + 2, right: panel.right - button.right });
    } else {
      setPlace(null);
    }
    setOpen(!open);
  };
  useEffect(() => {
    if (!open) return;
    (listRef.current ?? ref.current)
      ?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')
      ?.focus();
    const onPointer = (event: MouseEvent) => {
      const target = event.target as Node;
      if (!ref.current?.contains(target) && !listRef.current?.contains(target)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <span className="delivery-menu" ref={ref}>
      <button
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={toggle}
      >
        {trigger}
      </button>
      {open &&
        (place ? (
          createPortal(
            <div
              ref={listRef}
              className="delivery-menu__list"
              role="menu"
              style={{ top: place.top, right: place.right }}
            >
              {children(() => setOpen(false))}
            </div>,
            place.host,
          )
        ) : (
          <div ref={listRef} className="delivery-menu__list" role="menu">
            {children(() => setOpen(false))}
          </div>
        ))}
    </span>
  );
}

/** Where a right click on a task asked for its menu: inside the Delivery panel. */
export interface ContextPoint {
  host: Element;
  x: number;
  y: number;
}

/** One entry of a task's context menu; `null` draws a separator. */
interface ContextItem {
  label: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
  title?: string;
  danger?: boolean;
}

/** The list a right click opens at the pointer. Arrow keys move, Escape or a
 *  click elsewhere closes it, and choosing an entry closes it first. */
function ContextMenu({
  at,
  label,
  items,
  onClose,
}: {
  at: ContextPoint;
  label: string;
  items: (ContextItem | null)[];
  onClose: () => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const list = listRef.current;
    if (!list) return;
    // Kept inside the panel: it opens up or left when the pointer is near an edge.
    const panel = at.host.getBoundingClientRect();
    const left = Math.max(0, Math.min(at.x - panel.left, panel.width - list.offsetWidth - 4));
    const top = Math.max(0, Math.min(at.y - panel.top, panel.height - list.offsetHeight - 4));
    setPlace({ left, top });
    list.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
  }, [at]);
  useEffect(() => {
    const onPointer = (event: MouseEvent) => {
      if (!listRef.current?.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    window.addEventListener("blur", onClose);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);
  const move = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const entries = [
      ...(listRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? []),
    ];
    const at = entries.indexOf(document.activeElement as HTMLElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    entries[(at + step + entries.length) % entries.length]?.focus();
  };
  // Separators only between entries: none first, last or twice in a row.
  const shown: (ContextItem | null)[] = [];
  for (const item of items) {
    if (item !== null || (shown.length > 0 && shown[shown.length - 1] !== null)) shown.push(item);
  }
  if (shown[shown.length - 1] === null) shown.pop();
  return createPortal(
    <div
      ref={listRef}
      className="delivery-menu__list delivery-menu__list--context"
      role="menu"
      aria-label={label}
      style={place ?? { visibility: "hidden" }}
      onKeyDown={move}
      onContextMenu={(event) => event.preventDefault()}
    >
      {shown.map((item, at) =>
        item === null ? (
          <hr key={`separator-${at}`} />
        ) : (
          <button
            key={at}
            type="button"
            role="menuitem"
            className={item.danger ? "delivery-menu__danger" : undefined}
            disabled={item.disabled}
            title={item.title}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            {item.label}
          </button>
        ),
      )}
    </div>,
    at.host,
  );
}

function errorText(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function shortCommit(value: string | null | undefined): string {
  return value ? value.slice(0, 8) : "";
}

function duration(job: DeliveryJob, now: number): string {
  if (!job.started_at_ms) return "";
  const end = job.ended_at_ms ?? now;
  const seconds = Math.max(0, Math.round((end - job.started_at_ms) / 1000));
  if (seconds < 60) return `${seconds} s`;
  return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
}

/** Two paths of the same folder, whatever their slashes and case. */
function samePath(a: string, b: string): boolean {
  const clean = (path: string) => path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  return clean(a) === clean(b);
}

/** "hace 12 s", "hace 3 min": when the agent last did something. */
function ago(ms: number | null | undefined, now: number): string {
  if (!ms) return "";
  const seconds = Math.max(0, Math.round((now - ms) / 1000));
  return seconds < 60 ? `hace ${seconds} s` : `hace ${Math.floor(seconds / 60)} min`;
}

/** "45 s", "31 min": how long something took. */
function lasted(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return seconds < 60 ? `${seconds} s` : `${Math.round(seconds / 60)} min`;
}

/** Where a job's time went: "Modelo 5 min · Comandos 78 min · 37 turnos ·
 *  más lento: npm run preflight ×5 (31 min, 2 al límite de 10 min)". */
function timingLabel(timing: DeliveryTiming): string {
  const slowest = timing.slowest.map((slow) => {
    const runs = slow.runs > 1 ? ` ×${slow.runs}` : "";
    const ceiling =
      slow.at_ceiling === 0
        ? ""
        : slow.runs === 1
          ? ", al límite de 10 min"
          : `, ${slow.at_ceiling} al límite de 10 min`;
    return `${slow.command}${runs} (${lasted(slow.total_ms)}${ceiling})`;
  });
  return [
    timing.model_ms === null ? null : `Modelo ${lasted(timing.model_ms)}`,
    `Comandos ${lasted(timing.commands_ms)}`,
    timing.turns === null ? null : timing.turns === 1 ? "1 turno" : `${timing.turns} turnos`,
    slowest.length === 0
      ? null
      : `${slowest.length === 1 ? "más lento" : "más lentos"}: ${slowest.join(", ")}`,
  ]
    .filter(Boolean)
    .join(" · ");
}

function tokens(count: number): string {
  return count >= 1000 ? `${(count / 1000).toFixed(1)}k` : String(count);
}

/** What a job used: tokens in, read from cache and out, and the cost when known. */
function usageLabel(usage: DeliveryUsage): string {
  const cost =
    usage.cost_micro_usd === null ? "" : ` · $${(usage.cost_micro_usd / 1_000_000).toFixed(2)}`;
  return `Consumo: ${tokens(usage.input_tokens)} de entrada · ${tokens(usage.cached_tokens)} en caché · ${tokens(usage.output_tokens)} de salida${cost}`;
}

function changeSymbol(kind: string): string {
  if (kind === "created") return "+";
  if (kind === "removed") return "×";
  return "−";
}

function files(count: number): string {
  return count === 1 ? "1 archivo" : `${count} archivos`;
}

function agentName(agent: string): string {
  return agent === "codex"
    ? "Codex"
    : agent === "claude"
      ? "Claude Code"
      : agent === "muse"
        ? "Muse Code"
        : "Shell";
}

/** The model and reasoning a stage runs with, named even when they are the defaults. */
function runtimeLabel(job: DeliveryJob): string {
  if (job.agent === "shell") return "";
  const model = job.model ?? job.session_model ?? "modelo predeterminado";
  if (job.reasoning) return `${model} · ${codexReasoningLabel(job.reasoning).toLowerCase()}`;
  // Codex and Muse Code fall back to the model's default effort, as the last catalog has it.
  const fallback =
    job.agent === "codex" || job.agent === "muse"
      ? lastRuntimeCatalog(job.agent)?.models?.find((entry) => entry.model === job.model)
          ?.default_reasoning_effort
      : undefined;
  return fallback
    ? `${model} · ${codexReasoningLabel(fallback).toLowerCase()} (predeterminado)`
    : `${model} · razonamiento predeterminado`;
}

/** Who ran the task's latest stage, for one with no agent of its own to start from. */
function lastStageAgent(jobs: DeliveryJob[], taskId: string): DeliveryStageAgent | null {
  const last = [...jobs].reverse().find((job) => job.task_id === taskId && job.agent !== "shell");
  return last ? { agent: last.agent, model: last.model, reasoning: last.reasoning ?? null } : null;
}

/** The same repo whatever Windows prefix (`\\?\`), slashes or case its path carries. */
function repoKey(path: string): string {
  return path
    .replace(/^\\\\\?\\/, "")
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .toLowerCase();
}

const repoPanelId = (repo: string) => `delivery:${repoKey(repo)}`;

/** What every repo's tab reads from the panel that holds them. */
interface DeliveryShared {
  overview: DeliveryOverview | null;
  now: number;
  repos: RepoChoice[];
  listView: ListView;
  setListView: Dispatch<SetStateAction<ListView>>;
  /** Each repo's shown task, by repoKey. */
  selected: Record<string, string>;
  select: (repo: string, taskId: string | null) => void;
  reveal: (task: DeliveryTask) => void;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  dispatch: (task: DeliveryTask, role: string, prompt: string, source: string | null) => void;
  openTaskInAgents: (task: DeliveryTask, agentType: string) => Promise<void>;
  continueJob: (job: DeliveryJob) => Promise<void>;
  takeover: (run: DeliveryRun) => Promise<false | undefined>;
  /** Opens a repo's tab in the dock (from "+"). */
  openRepo: (repo: string) => void;
  /** Coordinators that stopped with changes unread, minus the ones the user hid. */
  stalls: CoordinatorStall[];
  hideStall: (stall: CoordinatorStall) => void;
}

const DeliveryContext = createContext<DeliveryShared | null>(null);

function useDelivery(): DeliveryShared {
  const shared = useContext(DeliveryContext);
  if (!shared) throw new Error("Delivery context missing");
  return shared;
}

const DOCK_KEY = "tinto:delivery-dock";
const DETACHED_KEY = "tinto:delivery-detached";
const HIDDEN_STALLS_KEY = "tinto:delivery-hidden-stalls";

/** A stall is one stop: the same coordinator stopping again is a new one. */
function stallKey(stall: CoordinatorStall): string {
  return `${stall.run.id}:${stall.since}`;
}

function savedHiddenStalls(): string[] {
  try {
    const saved = JSON.parse(localStorage.getItem(HIDDEN_STALLS_KEY) ?? "[]") as unknown;
    return Array.isArray(saved) ? saved.filter((key) => typeof key === "string") : [];
  } catch {
    return [];
  }
}

function savedDetached(): Record<string, string> {
  try {
    const saved = JSON.parse(localStorage.getItem(DETACHED_KEY) ?? "null") as unknown;
    if (saved && typeof saved === "object" && !Array.isArray(saved)) {
      return saved as Record<string, string>;
    }
  } catch {
    // Nothing was out.
  }
  return {};
}

/** Delivery: one tab per repo, which can be reordered, split and taken out
 *  to a window of its own. `only` shows a single repo, as a detached window
 *  does. */
export function DeliveryPanel({ only }: { only?: string } = {}) {
  const bus = useBusState();
  const { openAgentTerminal } = useWorkspaceActions();
  const [overview, setOverview] = useState<DeliveryOverview | null>(null);
  const [selected, setSelected] = useState<Record<string, string>>({});
  const [dialog, setDialog] = useState<Dialog>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [repoSettings, setRepoSettings] = useState<DeliveryRepoSettings | null>(null);
  const [listView, setListView] = useState(savedListView);
  const dockRef = useRef<DockviewApi | null>(null);
  const [dockReady, setDockReady] = useState(false);
  // Repos whose tab is in a window of its own (repoKey → the path it opened
  // with); it comes back when that closes. Saved, so a reload of this window
  // leaves the tab out while its window is still open.
  const [detached, setDetached] = useState(savedDetached);
  useEffect(() => {
    if (only) return;
    try {
      localStorage.setItem(DETACHED_KEY, JSON.stringify(detached));
    } catch {
      // A reload brings the tab back beside its window.
    }
  }, [only, detached]);
  const attach = useCallback(
    (key: string) =>
      setDetached((current) => {
        if (!(key in current)) return current;
        const next = { ...current };
        delete next[key];
        return next;
      }),
    [],
  );
  // The windows that were out before a reload: the closed ones come back
  // now, the open ones when they close.
  useEffect(() => {
    if (only) return;
    for (const [key, repo] of Object.entries(savedDetached())) {
      void watchDetachedPanelWindow({ kind: "delivery", repo }, () => attach(key)).then((open) => {
        if (!open) attach(key);
      });
    }
  }, [only, attach]);
  useEffect(() => {
    try {
      localStorage.setItem(LIST_VIEW_KEY, JSON.stringify(listView));
    } catch {
      // It lasts until Tinto closes.
    }
  }, [listView]);

  const refresh = useCallback(
    () =>
      getDeliveryOverview().then(
        (next) => setOverview(next),
        (cause: unknown) => setError(errorText(cause)),
      ),
    [],
  );

  useEffect(() => {
    // The last model catalogs Tinto loaded: Codex jobs that name no model
    // (e.g. from a coordinator) use the account's default, and the models a
    // stage or a coordinator names are checked against them.
    for (const provider of ["codex", "muse"] as const) {
      const catalog = lastRuntimeCatalog(provider);
      if (catalog) tellDelivery(provider, catalog);
    }
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    let pending: ReturnType<typeof setTimeout> | null = null;
    getDeliveryOverview().then(
      (next) => {
        if (active) setOverview(next);
      },
      (cause: unknown) => {
        if (active) setError(errorText(cause));
      },
    );
    // Running agents change things several times a second: read the
    // overview at most once a second, one read at a time, so the panel
    // keeps answering clicks.
    let reading = false;
    let again = false;
    const reload = () => {
      if (reading) {
        again = true;
        return;
      }
      reading = true;
      void refresh().finally(() => {
        reading = false;
        if (again && active) {
          again = false;
          schedule();
        }
      });
    };
    const schedule = () => {
      if (pending) return;
      pending = setTimeout(() => {
        pending = null;
        if (active) reload();
      }, 1000);
    };
    void onDeliveryChanged(schedule).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    });
    const clock = setInterval(() => setNow(Date.now()), 1000);
    const fallback = setInterval(reload, 5000);
    return () => {
      active = false;
      unlisten?.();
      if (pending) clearTimeout(pending);
      clearInterval(clock);
      clearInterval(fallback);
    };
  }, [refresh]);

  const repos: RepoChoice[] = useMemo(() => {
    const config = bus.config;
    const active = config?.workbenches.find((workbench) => workbench.name === config.active);
    return (active?.repos ?? []).map((entry) => ({
      path: entry.path,
      distro: isWslRepoSource(entry.path, entry.source, entry.distro)
        ? (entry.distro ?? null)
        : null,
      label: busStore.displayName(entry.path),
    }));
  }, [bus.config]);

  const tasks = useMemo(() => overview?.tasks ?? [], [overview]);
  const statuses = new Map(
    overview ? tasks.map((task) => [task.id, taskStatus(task, overview, now)] as const) : [],
  );
  // Across every repo, whichever tab is in front.
  const needYou = tasks.filter((task) => statuses.get(task.id)?.group === "attention");
  // One path per repo with tasks, as its tasks name it.
  const taskRepos = [
    ...new Map(tasks.map((task) => [repoKey(task.repo), task.repo] as const)).values(),
  ];

  const select = useCallback((repo: string, taskId: string | null) => {
    setSelected((current) => {
      const key = repoKey(repo);
      if ((current[key] ?? null) === taskId) return current;
      const next = { ...current };
      if (taskId) next[key] = taskId;
      else delete next[key];
      return next;
    });
  }, []);

  const addRepoPanel = (api: DockviewApi, repo: string) => {
    if (api.getPanel(repoPanelId(repo))) return;
    api.addPanel({
      id: repoPanelId(repo),
      component: "deliveryRepo",
      tabComponent: "deliveryRepoTab",
      title: busStore.displayName(repo),
      params: { repo },
      // A tab that comes with new tasks leaves the one in front where it is.
      inactive: api.panels.length > 0,
    });
  };

  const openRepo = (repo: string) => {
    const api = dockRef.current;
    if (!api) return;
    addRepoPanel(api, repo);
    api.getPanel(repoPanelId(repo))?.api.setActive();
  };

  // Brings the repo forward: its tab, or its window when it has one.
  const showRepo = (repo: string) => {
    if (only) return;
    const out = detached[repoKey(repo)];
    if (out) {
      void openDetachedPanelWindow({ kind: "delivery", repo: out });
      return;
    }
    openRepo(repo);
  };

  // Shows the task in its repo's tab, leaving a batch filter that hides it.
  const reveal = (task: DeliveryTask) => {
    setListView((view) => (view.lot && view.lot !== task.run_id ? { ...view, lot: null } : view));
    select(task.repo, task.id);
    showRepo(task.repo);
  };

  // Every repo with tasks has its tab, unless it is in a window of its own.
  // A tab the saved layout brings for a repo that is out, or that is neither
  // in the workbench nor has tasks, closes.
  const taskRepoKeys = taskRepos.map(repoKey).join("\n");
  const repoKeys = bus.config ? repos.map((repo) => repoKey(repo.path)).join("\n") : null;
  useEffect(() => {
    const api = dockRef.current;
    if (!dockReady || !api) return;
    const withTasks = taskRepos.map(repoKey);
    for (const panel of [...api.panels]) {
      const repo = (panel.params as { repo?: string } | undefined)?.repo;
      if (!repo) continue;
      const key = repoKey(repo);
      const stray =
        repoKeys !== null && !repoKeys.split("\n").includes(key) && !withTasks.includes(key);
      if (key in detached || stray) panel.api.close();
    }
    for (const repo of taskRepos) {
      if (!(repoKey(repo) in detached)) addRepoPanel(api, repo);
    }
    // The paths only matter through their keys.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dockReady, taskRepoKeys, repoKeys, detached]);

  const onDockReady = (event: DockviewReadyEvent) => {
    const api = event.api;
    dockRef.current = api;
    try {
      const saved = JSON.parse(
        localStorage.getItem(DOCK_KEY) ?? "null",
      ) as SerializedDockview | null;
      if (saved) api.fromJSON(saved);
    } catch {
      // A layout from another version: the tabs open again in order.
    }
    api.onDidLayoutChange(() => {
      try {
        localStorage.setItem(DOCK_KEY, JSON.stringify(api.toJSON()));
      } catch {
        // The layout lasts until Tinto closes.
      }
    });
    // A repo's tab dragged out of the window and released there opens in its
    // own window; closing that window brings the tab back.
    api.onWillDragPanel(({ panel, nativeEvent }) => {
      const repo = (panel.params as { repo?: string } | undefined)?.repo;
      if (!repo) return;
      armExternalTabDetach(nativeEvent, async () => {
        const key = repoKey(repo);
        const opened = await openDetachedPanelWindow({ kind: "delivery", repo }, () => attach(key));
        if (!opened) return;
        setDetached((current) => ({ ...current, [key]: repo }));
        panel.api.close();
      });
    });
    setDockReady(true);
  };

  const jobs = overview?.jobs ?? [];
  const running = jobs.filter((job) => job.status === "pending" || job.status === "started");
  const queued = jobs.filter((job) => job.status === "queued");
  // The setting, and the agents it allows now (automatic: one per active task).
  const capacitySetting = overview?.settings.capacity ?? 0;
  const capacity = overview?.capacity ?? Math.max(1, capacitySetting);
  const qa = overview?.leases.find((lease) => lease.name === "qa");
  const qaHolder = tasks.find((task) => task.id === qa?.holder_task_id);
  const allStalls = overview ? coordinatorStalls(overview, now) : [];
  const [hiddenStalls, setHiddenStalls] = useState(savedHiddenStalls);
  const stalls = allStalls.filter((stall) => !hiddenStalls.includes(stallKey(stall)));
  // Hidden until the coordinator stops again; keys of stalls gone are dropped.
  const hideStall = (stall: CoordinatorStall) => {
    const live = allStalls.map(stallKey);
    const next = [...hiddenStalls.filter((key) => live.includes(key)), stallKey(stall)];
    setHiddenStalls(next);
    try {
      localStorage.setItem(HIDDEN_STALLS_KEY, JSON.stringify(next));
    } catch {
      // Hidden until Tinto closes.
    }
  };
  const needsCount = needYou.length + stalls.length;
  const needsLabel = [
    needYou.length === 1 ? "1 tarea" : needYou.length > 1 ? `${needYou.length} tareas` : "",
    stalls.length === 1
      ? "1 coordinador"
      : stalls.length > 1
        ? `${stalls.length} coordinadores`
        : "",
  ]
    .filter(Boolean)
    .join(" y ");

  // For what is not a button (a select, a menu item): its error goes to the
  // panel's banner. Buttons are ActionButtons and say it themselves.
  const run = async (action: () => Promise<unknown>) => {
    setError(null);
    try {
      await action();
      await refresh();
      return true;
    } catch (cause) {
      setError(errorText(cause));
      return false;
    }
  };

  const takeover = async (target: DeliveryRun) => {
    const ok = await confirm(
      `${target.owner} dejará de coordinar "${target.title}". Lo que envíe a partir de ahora se descarta.`,
      { title: "Tomar el control", kind: "warning", okLabel: "Tomar el control" },
    );
    if (!ok) return false;
    await takeoverDeliveryRun(target.id);
  };

  const shared: DeliveryShared = {
    overview,
    now,
    repos,
    listView,
    setListView,
    selected,
    select,
    reveal,
    run,
    dispatch: (task, role, prompt, source) => {
      setRepoSettings(null);
      getDeliveryRepoSettings(task.repo).then(setRepoSettings, () =>
        setRepoSettings({ worktree_root: null, bootstrap: null, default_base: null }),
      );
      setDialog({ kind: "dispatch", task, role, prompt, source });
    },
    openTaskInAgents: async (task, agentType) => {
      await run(async () => {
        const conversation = await openDeliveryTaskInAgents(task.id, agentType);
        openAgentTerminal({
          sessionId: conversation.session_id,
          repo: conversation.repo,
          agentType: conversation.agent_type,
        });
      });
    },
    continueJob: async (job) => {
      const conversation = await openDeliveryJobInAgents(job.id);
      openAgentTerminal({
        sessionId: conversation.session_id,
        repo: conversation.repo,
        agentType: conversation.agent_type,
      });
    },
    takeover,
    openRepo,
    stalls,
    hideStall,
  };

  return (
    <DeliveryRefreshContext.Provider value={refresh}>
      <DeliveryContext.Provider value={shared}>
        <div className="delivery-panel" data-testid="delivery-panel">
          <header className="delivery-panel__header">
            <h1>Delivery</h1>
            {only && <span className="delivery-muted">{busStore.displayName(only)}</span>}
            <span className="delivery-panel__spacer" />
            {needsCount > 0 && (
              <button
                type="button"
                className="delivery-attention"
                onClick={() =>
                  needYou.length > 0 ? reveal(needYou[0]) : showRepo(stalls[0].run.repo)
                }
              >
                <Icon name="flag" />
                {needsLabel} {needsCount === 1 ? "te necesita" : "te necesitan"}
              </button>
            )}
            <span className="delivery-load">
              Agentes
              <span className="delivery-meter" aria-hidden>
                {Array.from({ length: capacity }, (_, index) => (
                  <i key={index} className={index < running.length ? "on" : undefined} />
                ))}
              </span>
              {running.length} de {capacity}
              {queued.length > 0 && (
                <span className="delivery-muted">· {queued.length} en cola</span>
              )}
            </span>
            {qa?.state === "quarantined" ? (
              <button
                type="button"
                className="delivery-lease delivery-lease--quarantined"
                title={qa.note ?? undefined}
                onClick={() => setDialog({ kind: "release", name: "qa", note: qa.note })}
              >
                QA en cuarentena
                {qa.queue.length > 0 && (
                  <span className="delivery-muted">· {qa.queue.length} esperando</span>
                )}
              </button>
            ) : (
              qa && (
                <span
                  className={`delivery-lease delivery-lease--${qa.state}`}
                  title={qa.note ?? undefined}
                >
                  {qa.state === "active" ? `QA: ${qaHolder?.key ?? "ocupado"}` : "QA libre"}
                  {qa.queue.length > 0 && (
                    <span className="delivery-muted">· {qa.queue.length} esperando</span>
                  )}
                </span>
              )
            )}
            <button type="button" onClick={() => setDialog({ kind: "archived" })}>
              Archivadas…
            </button>
            <button
              type="button"
              className="delivery-icon-button"
              aria-label="Ajustes de Delivery"
              title="Agentes en paralelo y conexión de coordinadores"
              onClick={() => setDialog({ kind: "settings" })}
            >
              <Icon name="gear" />
            </button>
            <button
              type="button"
              className="delivery-button--primary"
              disabled={repos.length === 0}
              onClick={() => setDialog({ kind: "new-task" })}
            >
              Nueva tarea
            </button>
          </header>

          {error && (
            <div className="delivery-panel__error" role="alert">
              <span>{error}</span>
              <button type="button" aria-label="Descartar error" onClick={() => setError(null)}>
                ×
              </button>
            </div>
          )}

          {/* Only when it holds work back; otherwise the header and the task say it. */}
          {qa?.state === "quarantined" && qa.queue.length > 0 && (
            <div className="delivery-panel__warning" role="status">
              <span title={qa.note ?? undefined}>
                {qa.queue.length === 1 ? "Una QA espera" : `${qa.queue.length} QA esperan`} a que
                liberes el recurso QA{qaHolder ? `, en cuarentena tras ${qaHolder.key}` : ""}.
              </span>
              <button
                type="button"
                onClick={() => setDialog({ kind: "release", name: "qa", note: qa.note })}
              >
                Liberar…
              </button>
            </div>
          )}

          {only ? (
            <DeliveryRepoView repo={only} />
          ) : (
            <div className="delivery-dock">
              {overview && tasks.length === 0 && (
                <p className="delivery-panel__empty">
                  Crea una tarea para trabajar un issue en su propio worktree y rama.
                </p>
              )}
              <DockviewReact
                theme={themeVisualStudio}
                components={deliveryDockComponents}
                tabComponents={deliveryDockTabs}
                rightHeaderActionsComponent={DeliveryAddRepo}
                dndStrategy="pointer"
                onReady={onDockReady}
              />
            </div>
          )}

          {dialog?.kind === "settings" && (
            <DeliverySettingsDialog
              capacity={capacitySetting}
              endpoint={overview?.coordinator ?? null}
              onCapacity={(value) => void run(() => updateDeliverySettings(value))}
              onClose={() => setDialog(null)}
            />
          )}
          {dialog?.kind === "new-task" && (
            <NewTaskDialog
              repos={repos}
              runs={overview?.runs ?? []}
              onCancel={() => setDialog(null)}
              onSubmit={async (values) => {
                if (values.bootstrap.trim()) {
                  const current = await getDeliveryRepoSettings(values.repo.path);
                  await setDeliveryRepoSettings(values.repo.path, {
                    ...current,
                    bootstrap: values.bootstrap.trim(),
                  });
                }
                const task = await createDeliveryTask({
                  repo: values.repo.path,
                  distro: values.repo.distro,
                  key: values.key.trim(),
                  title: values.title.trim(),
                  base: values.base.trim(),
                  branch: values.branch.trim(),
                  runId: values.runId,
                });
                reveal(task);
                setDialog(null);
              }}
            />
          )}
          {dialog?.kind === "dispatch" && (
            <DispatchDialog
              task={dialog.task}
              settings={repoSettings}
              lastAgent={lastStageAgent(jobs, dialog.task.id)}
              initialRole={dialog.role}
              initialPrompt={dialog.prompt}
              promptSource={dialog.source}
              onCancel={() => setDialog(null)}
              onSubmit={async (values) => {
                if (values.access === "full") {
                  const ok = await confirm(
                    `El agente podrá ejecutar comandos y modificar archivos fuera del worktree sin pedir aprobación.\n\nWorktree: ${dialog.task.worktree}`,
                    {
                      title: "Dar acceso completo",
                      kind: "warning",
                      okLabel: "Dar acceso completo",
                    },
                  );
                  if (!ok) return false;
                }
                {
                  // The stage keeps the agent it was launched with.
                  const stage = values.role.toLowerCase();
                  const current = await getDeliveryRepoSettings(dialog.task.repo);
                  const fixed = current.stages?.[stage];
                  const same =
                    fixed?.agent === values.agent &&
                    (fixed.model ?? "") === values.model &&
                    (fixed.reasoning ?? "") === values.reasoning;
                  if (values.settings || !same) {
                    await setDeliveryRepoSettings(dialog.task.repo, {
                      ...current,
                      ...values.settings,
                      stages: {
                        ...current.stages,
                        [stage]: {
                          agent: values.agent,
                          model: values.model || null,
                          reasoning: values.reasoning || null,
                          // Its time limit stays.
                          ...(fixed?.timeout_minutes
                            ? { timeout_minutes: fixed.timeout_minutes }
                            : {}),
                        },
                      },
                    });
                  }
                  await dispatchDeliveryJob({
                    taskId: dialog.task.id,
                    role: values.role,
                    agent: values.agent,
                    model: values.model,
                    reasoning: values.reasoning,
                    access: values.access,
                    prompt: values.prompt,
                    platform: values.platform,
                  });
                }
                setDialog(null);
              }}
            />
          )}
          {dialog?.kind === "release" && (
            <ReleaseLeaseDialog
              name={dialog.name}
              note={dialog.note}
              onCancel={() => setDialog(null)}
              onSubmit={async (confirmation) => {
                await releaseDeliveryLease(dialog.name, confirmation);
                setDialog(null);
              }}
            />
          )}
          {dialog?.kind === "archived" && (
            <ArchivedTasksDialog
              onCancel={() => setDialog(null)}
              onUnarchive={async (taskId) => {
                const task = await unarchiveDeliveryTask(taskId);
                setDialog(null);
                reveal(task);
              }}
            />
          )}
        </div>
      </DeliveryContext.Provider>
    </DeliveryRefreshContext.Provider>
  );
}

const deliveryDockComponents = {
  deliveryRepo: ({ params }: IDockviewPanelProps<{ repo: string }>) => (
    <DeliveryRepoView repo={params.repo} />
  ),
};

/** A repo's tab: its name, how many tasks it has and whether one needs you.
 *  A repo with no tasks left can be closed. */
function DeliveryRepoTab({ params, api }: IDockviewPanelHeaderProps<{ repo: string }>) {
  const { overview, now, stalls } = useDelivery();
  const tasks = (overview?.tasks ?? []).filter(
    (task) => repoKey(task.repo) === repoKey(params.repo),
  );
  const needs =
    (overview !== null &&
      tasks.some((task) => taskStatus(task, overview, now).group === "attention")) ||
    stalls.some((stall) => repoKey(stall.run.repo) === repoKey(params.repo));
  return (
    <span className="delivery-repo-tab" title={params.repo}>
      {busStore.displayName(params.repo)}
      <span className="delivery-repo-tab__count">{tasks.length}</span>
      {needs && <span className="delivery-repo-tab__needs" aria-label="te necesita" />}
      {tasks.length === 0 && (
        <button
          type="button"
          className="delivery-repo-tab__close"
          aria-label={`Cerrar ${busStore.displayName(params.repo)}`}
          onClick={(event) => {
            event.stopPropagation();
            api.close();
          }}
        >
          ×
        </button>
      )}
    </span>
  );
}

const deliveryDockTabs = { deliveryRepoTab: DeliveryRepoTab };

/** A batch whose coordinator stopped with changes unread: a chip that opens
 *  what to tell it, its conversation when it runs in Tinto, and a way to hide
 *  this stop. */
function CoordinatorStallChip({
  stall,
  named,
  onOpenCoordinator,
}: {
  stall: CoordinatorStall;
  named?: boolean;
  onOpenCoordinator: () => void;
}) {
  const { hideStall } = useDelivery();
  const { run, unread, since } = stall;
  const batch = batchName(run.title || run.id);
  const changes = unread === 1 ? "1 cambio" : `${unread} cambios`;
  const at = new Date(since).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const message = `Sigue con wait_run de Tinto Delivery: tienes ${changes} sin leer desde las ${at}. Si las herramientas de Delivery fallan, reconéctalas.`;
  return (
    <span
      className="delivery-stall"
      title={`${named ? `${batch}: c` : "C"}oordinador parado, ${changes} sin leer desde las ${at}`}
    >
      <Menu
        label={`Coordinador parado${named ? ` en ${batch}` : ""}, ${changes} sin leer`}
        trigger={
          <>
            <span className="delivery-dot delivery-dot--warn" />
            {/* The batch's own name: "Lote" is already said by the bar. */}
            {named && (
              <span className="delivery-stall__batch">{batch.replace(/^lote\s+/i, "")}</span>
            )}
            <span className="delivery-muted">
              {named ? "coordinador parado" : "Coordinador parado"}
            </span>
            <span className="delivery-stall__count">{unread}</span>
          </>
        }
        floating
      >
        {(close) => (
          <div className="delivery-stall__card">
            <p>
              <strong>El coordinador no está esperando</strong>
            </p>
            <p className="delivery-muted">
              {run.owner && run.owner !== "user" ? run.owner : "Su coordinador"} tiene {changes} sin
              leer desde las {at}. Pídele que siga con wait_run; si sus herramientas de Delivery
              fallan, que las reconecte.
            </p>
            <div className="delivery-actions">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void navigator.clipboard?.writeText(message).catch(() => {});
                }}
              >
                Copiar mensaje
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  onOpenCoordinator();
                }}
              >
                Abrir coordinador
              </button>
              <button
                type="button"
                role="menuitem"
                title="Vuelve a avisar si se para otra vez"
                onClick={() => {
                  close();
                  hideStall(stall);
                }}
              >
                Ocultar
              </button>
            </div>
          </div>
        )}
      </Menu>
    </span>
  );
}

/** "+" beside the tabs: a repo of the workbench without its tab. */
function DeliveryAddRepo({ containerApi }: IDockviewHeaderActionsProps) {
  const { repos, openRepo } = useDelivery();
  const missing = repos.filter((repo) => !containerApi.getPanel(repoPanelId(repo.path)));
  if (missing.length === 0) return null;
  return (
    <Menu label="Abrir un repositorio" trigger="+" floating>
      {(close) =>
        missing.map((repo) => (
          <button
            key={repo.path}
            type="button"
            role="menuitem"
            onClick={() => {
              close();
              openRepo(repo.path);
            }}
          >
            {repo.label}
          </button>
        ))
      }
    </Menu>
  );
}

/** "Entorno": whether Docker (when the repo uses it) and the repo's
 *  environment probes answer, checked when the view opens and on click;
 *  each failure's output on hover. */
function EnvironmentStatus({ repo, distro }: { repo: string; distro: string | null }) {
  // Each click asks for a new round; a round's answer is shown once it comes.
  const [round, setRound] = useState(0);
  const asked = `${repo}|${distro ?? ""}|${round}`;
  const [answer, setAnswer] = useState<{
    asked: string;
    checks: DeliveryEnvironmentCheck[] | null;
    failure: string;
  } | null>(null);
  useEffect(() => {
    let current = true;
    checkDeliveryEnvironment(repo, distro).then(
      (checks) => current && setAnswer({ asked, checks, failure: "" }),
      (cause) =>
        current && setAnswer({ asked, checks: null, failure: String(cause?.message ?? cause) }),
    );
    return () => {
      current = false;
    };
  }, [repo, distro, asked]);
  const checking = answer?.asked !== asked;
  const checks = checking ? null : (answer?.checks ?? null);
  const failure = checking ? "" : (answer?.failure ?? "");
  const check = () => setRound((value) => value + 1);
  const failed = (checks ?? []).filter((item) => !item.ok);
  const state = checking
    ? "comprobando…"
    : failure
      ? "no se pudo comprobar"
      : checks?.length === 0
        ? "nada que comprobar"
        : failed.length === 0
          ? "bien"
          : failed.length === 1
            ? `falla ${failed[0].name}`
            : `${failed.length} fallos`;
  const tone = checking || checks?.length === 0 ? "" : failure || failed.length ? "fail" : "ok";
  const details = failure
    ? failure
    : checks?.length === 0
      ? "Docker no se usa en este repositorio y no hay sondas marcadas (en los ajustes del repositorio)."
      : (checks ?? [])
          .map(
            (item) => `${item.ok ? "✓" : "✗"} ${item.name}${item.output ? `\n${item.output}` : ""}`,
          )
          .join("\n\n");
  return (
    <button
      type="button"
      className={`delivery-env${tone ? ` delivery-env--${tone}` : ""}`}
      title={`${details}\n\nClic para comprobar de nuevo.`.trim()}
      aria-label={`Entorno: ${state}. Comprobar de nuevo`}
      disabled={checking}
      onClick={check}
    >
      <span className="delivery-dot" />
      Entorno: {state}
    </button>
  );
}

/** One repo's delivery: its batches, its tasks and its settings. */
function DeliveryRepoView({ repo }: { repo: string }) {
  const shared = useDelivery();
  const { overview, now, repos, listView, setListView, selected, select, reveal, run } = shared;
  const [settingsOpen, setSettingsOpen] = useState(false);
  const key = repoKey(repo);
  const choice = repos.find((candidate) => repoKey(candidate.path) === key) ?? {
    path: repo,
    distro: null,
    label: busStore.displayName(repo),
  };
  const tasks = (overview?.tasks ?? []).filter((task) => repoKey(task.repo) === key);
  const statuses = new Map(
    overview ? tasks.map((task) => [task.id, taskStatus(task, overview, now)] as const) : [],
  );
  const activeRuns = (overview?.runs ?? []).filter(
    (candidate) => candidate.status === "active" && repoKey(candidate.repo) === key,
  );
  const repoStalls = shared.stalls.filter((stall) => repoKey(stall.run.repo) === key);
  const [coordinatorOpen, setCoordinatorOpen] = useState(false);
  // A task row's right-click menu, drawn by the task it selects.
  const [contextAt, setContextAt] = useState<(ContextPoint & { taskId: string }) | null>(null);
  const closeContext = useCallback(() => setContextAt(null), []);
  // A batch that closed since, or the only one left, shows every task.
  const lot =
    activeRuns.length > 1 && activeRuns.some((candidate) => candidate.id === listView.lot)
      ? listView.lot
      : null;
  const listed = lot ? tasks.filter((task) => task.run_id === lot) : tasks;
  // What needs the user is never folded.
  const isFolded = (group: StatusGroup) => group !== "attention" && listView.folded.includes(group);
  // A split issue's packages sit together within each group, in order.
  const groups = GROUPS.map((group) => ({
    ...group,
    tasks: clusterPackages(listed.filter((task) => statuses.get(task.id)?.group === group.group)),
  })).filter((group) => group.tasks.length > 0);
  const ordered = groups.flatMap((group) => (isFolded(group.group) ? [] : group.tasks));
  // With every group folded, the detail still shows a task.
  const selectedTask =
    tasks.find((task) => task.id === selected[key]) ?? ordered[0] ?? groups[0]?.tasks[0] ?? null;
  const shownTaskId = selectedTask?.id ?? null;
  // Keep showing the same task when its group changes (e.g. after answering
  // its decisions) instead of jumping to whatever is first in the list.
  useEffect(() => {
    if (shownTaskId && shownTaskId !== selected[key]) select(repo, shownTaskId);
  }, [shownTaskId, selected, key, repo, select]);
  const chooseLot = (next: string | null) => {
    setListView((view) => ({ ...view, lot: next }));
    if (next && selectedTask && selectedTask.run_id !== next) select(repo, null);
  };
  // The delivered tasks the list shows go to "Archivadas…", as one by one.
  const archiveDelivered = async (delivered: DeliveryTask[]) => {
    const ok = await confirm(
      `${delivered.length === 1 ? "Se archiva 1 tarea entregada" : `Se archivan ${delivered.length} tareas entregadas`}: se borran sus worktrees y ramas locales, todo está en GitHub. Puedes recuperarlas en "Archivadas…".`,
      { title: "Archivar entregadas", okLabel: "Archivar" },
    );
    if (!ok) return false;
    for (const task of delivered) await archiveDeliveryTask(task.id);
    if (selectedTask && delivered.includes(selectedTask)) select(repo, null);
  };
  const toggleGroup = (group: StatusGroup) => {
    const folding = !listView.folded.includes(group);
    setListView((view) => ({
      ...view,
      folded: folding
        ? [...view.folded, group]
        : view.folded.filter((candidate) => candidate !== group),
    }));
    if (folding && selectedTask && statuses.get(selectedTask.id)?.group === group) {
      select(repo, null);
    }
  };

  // Running Agents conversations of the batch's repo, to hand it to one.
  const agentSessions = Object.values(useAgentSessionState().sessions);
  const handOverTargets = (target: DeliveryRun) =>
    agentSessions.filter(
      (session) =>
        session.status === "running" &&
        session.id !== target.coordinator_session &&
        samePath(session.repo, target.repo),
    );

  // The repo's active batches with what can be done to each, and a new one.
  const batches = (trigger: ReactNode) => (
    <span className="delivery-batches-menu">
      <Menu trigger={trigger}>
        {() => (
          <BatchList
            runs={activeRuns}
            repos={[choice]}
            onCreateRun={(path, title, qaJiraComment) =>
              createDeliveryRun(path, title, qaJiraComment)
            }
            onQaJiraComment={(target, post) =>
              void run(() => setDeliveryRunQaJiraComment(target.id, post))
            }
            onPause={(target, paused) => setDeliveryRunPaused(target.id, paused)}
            onTakeover={shared.takeover}
            onClose={async (target) => {
              const ok = await confirm(
                `Se cerrará el lote "${target.title}". Sus tareas siguen disponibles.`,
                { title: "Cerrar lote", okLabel: "Cerrar" },
              );
              if (!ok) return false;
              await closeDeliveryRun(target.id);
            }}
          />
        )}
      </Menu>
    </span>
  );

  return (
    <div className="delivery-repo" aria-label={`Delivery de ${choice.label}`}>
      <div className="delivery-repo__bar">
        {activeRuns.length === 1 ? (
          <span className="delivery-run">
            {batches(
              <>
                <strong>{batchName(activeRuns[0].title || activeRuns[0].id)}</strong>
                <Icon name="down" />
              </>,
            )}
            <span className="delivery-muted" title={activeRuns[0].repo}>
              {activeRuns[0].owner === "user"
                ? "lo diriges tú"
                : activeRuns[0].owner
                  ? `coordina ${activeRuns[0].owner}`
                  : "sin coordinador"}
            </span>
            {activeRuns[0].notified_at_ms && (
              <span
                className="delivery-muted"
                title="Tinto le escribe en Agents lo que espera de él"
              >
                · coordinador avisado {ago(activeRuns[0].notified_at_ms, now)}
              </span>
            )}
            {repoStalls.map((stall) => (
              <CoordinatorStallChip
                key={stall.run.id}
                stall={stall}
                onOpenCoordinator={() => setCoordinatorOpen(true)}
              />
            ))}
            {activeRuns[0].owner && activeRuns[0].owner !== "user" && (
              <ActionButton onAction={() => shared.takeover(activeRuns[0])}>
                Tomar el control
              </ActionButton>
            )}
            <Menu
              trigger={
                <>
                  Cambiar coordinador <Icon name="down" />
                </>
              }
            >
              {(close) => {
                const targets = handOverTargets(activeRuns[0]);
                return targets.length === 0 ? (
                  <span className="delivery-muted" role="menuitem" aria-disabled="true">
                    Abre en Agents una conversación de este repositorio
                  </span>
                ) : (
                  targets.map((session) => (
                    <button
                      key={session.id}
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        close();
                        void run(() => handOverDeliveryRun([activeRuns[0].id], session.id));
                      }}
                    >
                      {agentName(session.agent_type)} · {session.id.slice(0, 8)}
                      {session.turn_status === "working" ? " (trabajando)" : ""}
                    </button>
                  ))
                );
              }}
            </Menu>
            <ActionButton
              title={
                activeRuns[0].paused
                  ? "Lo que está en cola vuelve a arrancar"
                  : "Lo que está en cola espera; lo que corre sigue (puedes pausar cada etapa)"
              }
              onAction={() => setDeliveryRunPaused(activeRuns[0].id, !activeRuns[0].paused)}
            >
              {activeRuns[0].paused ? "Reanudar lote" : "Pausar lote"}
            </ActionButton>
          </span>
        ) : (
          <>
            {batches(
              <>
                {activeRuns.length > 1 ? `${activeRuns.length} lotes activos` : "Lotes"}
                <Icon name="down" />
              </>,
            )}
            {repoStalls.map((stall) => (
              <CoordinatorStallChip
                key={stall.run.id}
                stall={stall}
                named
                onOpenCoordinator={() => setCoordinatorOpen(true)}
              />
            ))}
          </>
        )}
        <span className="delivery-panel__spacer" />
        <EnvironmentStatus repo={choice.path} distro={choice.distro} />
        <button
          type="button"
          className="delivery-icon-button"
          aria-label={`Ajustes de ${choice.label}`}
          aria-pressed={settingsOpen}
          title={`Ajustes de ${choice.label}`}
          onClick={() => setSettingsOpen(!settingsOpen)}
        >
          <Icon name="gear" />
        </button>
      </div>

      {settingsOpen ? (
        <RepoSettingsPage repo={choice} onBack={() => setSettingsOpen(false)} />
      ) : (
        <div className="delivery-panel__body">
          <nav className="delivery-panel__tasks" aria-label="Tareas de Delivery">
            {tasks.length === 0 && (
              <p className="delivery-panel__empty">
                Crea una tarea para trabajar un issue en su propio worktree y rama.
              </p>
            )}
            {activeRuns.length > 1 && (
              <label className="delivery-lot-filter">
                <span>Lote</span>
                <select
                  value={lot ?? ""}
                  onChange={(event) => chooseLot(event.target.value || null)}
                >
                  <option value="">Todos los lotes ({tasks.length})</option>
                  {activeRuns.map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {`${batchName(candidate.title || candidate.id)} (${
                        tasks.filter((task) => task.run_id === candidate.id).length
                      })`}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {groups.map((group) => (
              <section key={group.group} className="delivery-task-group" aria-label={group.label}>
                <h2>
                  {group.group === "attention" ? (
                    <>
                      {group.label} <span>{group.tasks.length}</span>
                    </>
                  ) : (
                    <button
                      type="button"
                      aria-expanded={!isFolded(group.group)}
                      onClick={() => toggleGroup(group.group)}
                    >
                      <Icon name={isFolded(group.group) ? "right" : "down"} />
                      {group.label} <span>{group.tasks.length}</span>
                    </button>
                  )}
                  {group.group === "delivered" && (
                    <ActionButton
                      className="delivery-task-group__action"
                      onAction={() => archiveDelivered(group.tasks)}
                    >
                      Archivar todas
                    </ActionButton>
                  )}
                </h2>
                {!isFolded(group.group) &&
                  group.tasks.map((task) => {
                    const pr = (overview?.prs ?? []).find(
                      (candidate) => candidate.task_id === task.id,
                    );
                    const progress = overview ? issueProgress(overview, task) : null;
                    const status = statuses.get(task.id)!;
                    return (
                      <button
                        key={task.id}
                        type="button"
                        className={`delivery-task-row${status.tone === "blocked" ? " delivery-task-row--blocked" : ""}`}
                        aria-current={selectedTask?.id === task.id ? "true" : undefined}
                        onClick={() => select(repo, task.id)}
                        onContextMenu={(event) => {
                          const host = event.currentTarget.closest(".delivery-panel");
                          if (!host) return;
                          event.preventDefault();
                          select(repo, task.id);
                          setContextAt({
                            taskId: task.id,
                            host,
                            x: event.clientX,
                            y: event.clientY,
                          });
                        }}
                        onKeyDown={(event) => {
                          // The keyboard's way to the same menu: Shift+F10 or the menu key.
                          if (!(
                            event.key === "ContextMenu" ||
                            (event.shiftKey && event.key === "F10")
                          ))
                            return;
                          const host = event.currentTarget.closest(".delivery-panel");
                          if (!host) return;
                          event.preventDefault();
                          const row = event.currentTarget.getBoundingClientRect();
                          select(repo, task.id);
                          setContextAt({ taskId: task.id, host, x: row.left + 24, y: row.bottom });
                        }}
                      >
                        <span className="delivery-task-row__key">
                          {taskLabel(task)}
                          {pr && (
                            <span className="delivery-task-row__pr" title={`PR #${pr.number}`}>
                              #{pr.number}
                            </span>
                          )}
                        </span>
                        <span className="delivery-task-row__title">
                          {[task.package?.title, task.title || task.branch]
                            .filter(Boolean)
                            .join(" · ")}
                        </span>
                        {progress && (
                          <span className="delivery-task-row__title" title="El issue, por paquetes">
                            {progress}
                          </span>
                        )}
                        <StatusLine status={status} />
                        {status.detail && (
                          <span className="delivery-task-row__blocker" title={status.detail}>
                            {status.detail}
                          </span>
                        )}
                      </button>
                    );
                  })}
              </section>
            ))}
          </nav>

          <main className="delivery-panel__detail">
            {selectedTask && overview ? (
              <TaskDetail
                key={selectedTask.id}
                task={selectedTask}
                overview={overview}
                now={now}
                run={run}
                onDispatch={(role, prompt, source) =>
                  shared.dispatch(selectedTask, role, prompt, source)
                }
                onOpenConversation={(agentType) => shared.openTaskInAgents(selectedTask, agentType)}
                onContinueJob={shared.continueJob}
                onRemoved={() => select(repo, null)}
                onSelectTask={reveal}
                contextAt={contextAt?.taskId === selectedTask.id ? contextAt : null}
                onCloseContext={closeContext}
              />
            ) : (
              <p className="delivery-panel__empty">Sin tareas.</p>
            )}
          </main>
        </div>
      )}
      {overview && activeRuns.length > 0 && (
        <CoordinatorDock
          runs={activeRuns}
          focus={lot}
          overview={overview}
          stalled={repoStalls.map((stall) => stall.run.id)}
          open={coordinatorOpen}
          onOpenChange={setCoordinatorOpen}
        />
      )}
    </div>
  );
}

function TaskDetail({
  task,
  overview,
  now,
  run,
  onDispatch,
  onOpenConversation,
  onContinueJob,
  onRemoved,
  onSelectTask,
  contextAt,
  onCloseContext,
}: {
  task: DeliveryTask;
  overview: DeliveryOverview;
  now: number;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  onDispatch: (role: string, prompt: string, source: string | null) => void;
  onOpenConversation: (agentType: string) => Promise<void>;
  onContinueJob: (job: DeliveryJob) => Promise<void>;
  onRemoved: () => void;
  onSelectTask: (task: DeliveryTask) => void;
  /** Where the task's row was right-clicked: its menu opens there. */
  contextAt: ContextPoint | null;
  onCloseContext: () => void;
}) {
  const jobs = jobsOf(overview, task.id);
  // The latest QA's visual changes page: what the user reviews before the PR.
  const qaRecord = latestOfRole(jobs, "qa");
  const qaProblems = (overview.qa_problems ?? []).filter((problem) => problem.task_id === task.id);
  // The QA's problems that wait on the user: new ones, and fixes that did not hold.
  const qaProblemsToDecide = qaProblems.filter(
    (problem) => problem.status === "open" || problem.status === "still_failing",
  ).length;
  const newestFirst = [...jobs].reverse();
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const taskRun = overview.runs.find((candidate) => candidate.id === task.run_id);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(newestFirst[0]?.id ?? null);
  const latestUndoable = newestFirst.find(
    (job) => job.agent !== "shell" && !job.undone_at_ms && job.changes.length > 0,
  );
  const hasOpenJob = jobs.some(isOpen);
  const next = nextStageOf(overview, task.id);
  const round = roundOf(overview, task.id);
  const pastRounds = roundHistory(overview, task.id);
  const handoff = lastHandoff(jobs);
  const latest = newestFirst.find(
    (job) => job.agent !== "shell" && job.result_state !== "stale" && !job.undone_at_ms,
  );
  const broken = latest && !hasOpenJob && isBroken(latest) ? latest : null;
  const openApprovals = approvals.filter(
    (approval) => approval.status === "pending" || approval.status === "approved",
  );
  // A middle package of a split issue ends at its merge.
  const ladder = stepsOf(overview, task.id);
  const progress = issueProgress(overview, task);
  const step = openApprovals.length === 0 ? nextStep(approvals, ladder) : null;
  const [requesting, setRequesting] = useState(false);
  // What opens on top of the task: its pending decisions, QA's problems, its
  // Jira issue or an open step.
  const [float, setFloat] = useState<
    "decisions" | "qa" | "blocker" | "issue" | `approval:${string}` | null
  >(null);
  const floating = openApprovals.find((approval) => float === `approval:${approval.id}`);
  // The worktree in the files view, compared with the commit it started from.
  const { openRepo } = useWorkspaceActions();
  const openChanges = async () => {
    const changes = await getDeliveryTaskChanges(task.id);
    setBaseView(changes.repo, {
      label: `${task.base_ref} (${shortCommit(changes.base_commit)})`,
      taskId: task.id,
      files: changes.files,
    });
    openRepo(changes.repo);
  };
  // The issue's page, when the repo's Jira is set in Tinto.
  const [issueUrl, setIssueUrl] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    getDeliveryIssueUrl(task.id).then(
      (url) => {
        if (live) setIssueUrl(url);
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [task.id]);
  // The approval whose result is being corrected.
  const [correcting, setCorrecting] = useState<string | null>(null);
  const correctable = correctableSteps(approvals);
  const pr = (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
  // Opening the task checks its steps against GitHub: the PR is found by its
  // branch if needed and read, unless it was read moments ago.
  useEffect(() => {
    void checkDeliveryTask(task.id).catch(() => {});
  }, [task.id]);
  const taskDecisions = (overview.decisions ?? []).filter(
    (decision) => decision.task_id === task.id,
  );
  const pendingDecisions = taskDecisions.filter((decision) => decision.status === "pending");
  const blocked = pendingDecisions.length > 0;
  // An approved step waits on the coordinator, not on the user.
  const needsYou =
    blocked || openApprovals.some((approval) => approval.status === "pending") || broken !== null;
  const rejected = rejectedStep(approvals);
  // The coordinator's label stops at the stages; the delivery is Tinto's to know.
  const delivered = isDelivered(approvals, ladder);
  const stateLabel = delivered ? "Entregada" : (TASK_STATE_LABEL[task.state] ?? task.state);
  // The task's code as its latest stage found or left it.
  const withCode = newestFirst.find((job) => job.end_candidate ?? job.start_candidate);
  const candidate = (withCode?.end_candidate ?? withCode?.start_candidate)?.split(":")[1] ?? null;

  const dispatch = (role: string, withHandoff: boolean) => {
    const use = withHandoff && handoff !== null;
    onDispatch(
      role,
      use ? handoff.text : "",
      use
        ? `«Para la siguiente etapa» de ${stageLabel(handoff.from.role)} · intento ${handoff.from.attempt}`
        : null,
    );
  };

  const focusStage = (job: DeliveryJob) => {
    setExpandedJobId(job.id);
    document
      .getElementById(`delivery-stage-${task.id}-${job.role}`)
      ?.scrollIntoView?.({ block: "nearest" });
  };

  const undo = async (job: DeliveryJob) => {
    const ok = await confirm(
      `Se restaurarán ${files(job.changes.length)} al estado en que estaban cuando empezó este intento.`,
      { title: "Deshacer intento", kind: "warning", okLabel: "Deshacer" },
    );
    if (!ok) return false;
    await undoDeliveryJob(job.id);
  };

  const remove = async () => {
    const ok = await confirm(
      `Se eliminarán el worktree ${task.worktree} y la rama local ${task.branch}. Lo publicado sigue en GitHub.`,
      { title: "Eliminar tarea", kind: "warning", okLabel: "Eliminar" },
    );
    if (!ok) return false;
    {
      try {
        await removeDeliveryTask(task.id, false);
      } catch (cause) {
        const category =
          cause && typeof cause === "object" && "category" in cause
            ? String((cause as { category: unknown }).category)
            : "";
        if (category !== "task_unsaved") throw cause;
        const force = await confirm(
          `${errorText(cause)}. Si eliminas la tarea, se pierden con el worktree y la rama local ${task.branch}.`,
          { title: "Hay trabajo sin guardar", kind: "warning", okLabel: "Eliminar igualmente" },
        );
        if (!force) return false;
        await removeDeliveryTask(task.id, true);
      }
    }
    onRemoved();
  };

  const archive = async () => {
    const ok = await confirm(
      `Se borran el worktree ${task.worktree} y la rama local ${task.branch}: todo está en GitHub. La tarea sale de la lista y puedes recuperarla en "Archivadas…" por ${task.key} o por el número de su PR.`,
      { title: "Archivar tarea", okLabel: "Archivar" },
    );
    if (!ok) return false;
    await archiveDeliveryTask(task.id);
    onRemoved();
  };

  const bumpContract = async () => {
    const version = task.contract_version + 1;
    const ok = await confirm(
      `La tarea pasa a la versión ${version} del contrato. Los resultados que están en curso quedarán obsoletos y no harán avanzar la tarea.`,
      { title: "Nueva versión del contrato", kind: "warning", okLabel: `Subir a v${version}` },
    );
    if (!ok) return false;
    await updateDeliveryTask(task.id, { contractVersion: version });
  };

  // Stages with the most recent activity first; older attempts fold under the latest.
  const historyKeys = [...new Set(newestFirst.map(historyKey))];

  // The right-click menu: what the task can do now, what waits on the user first.
  const running = jobs.find((job) => job.status === "started" || job.status === "pending");
  const paused = pausedJob(jobs);
  const pendingStep = openApprovals.find((approval) => approval.status === "pending");
  const open = openBlockers(task);
  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});
  const menuEntries: (ContextItem | null | false | undefined)[] = [
    pendingStep && {
      label: `Aprobar ${lower(stepOf(pendingStep).label)}…`,
      onSelect: () => setFloat(`approval:${pendingStep.id}`),
    },
    blocked && {
      label:
        pendingDecisions.length === 1
          ? "Contestar 1 decisión…"
          : `Contestar ${pendingDecisions.length} decisiones…`,
      onSelect: () => setFloat("decisions"),
    },
    qaProblemsToDecide > 0 && {
      label: `Decidir ${qaProblemsToDecide} problema${qaProblemsToDecide === 1 ? "" : "s"} de QA…`,
      onSelect: () => setFloat("qa"),
    },
    broken
      ? {
          label: `Reintentar ${lower(stageLabel(broken.role))}`,
          onSelect: () => void run(() => retryDeliveryJob(broken.id)),
        }
      : undefined,
    broken && broken.agent !== "shell" && broken.provider_session_id
      ? {
          label: "Continuar en Agents",
          onSelect: () => void run(() => onContinueJob(broken)),
        }
      : undefined,
    running &&
      running.status === "started" &&
      running.agent !== "shell" && {
        label: `Pausar ${lower(stageLabel(running.role))}`,
        onSelect: () => void run(() => pauseDeliveryJob(running.id)),
      },
    paused && {
      label: `Reanudar ${lower(stageLabel(paused.role))}`,
      onSelect: () => void run(() => resumeDeliveryJob(paused.id)),
    },
    (running ?? paused) && {
      label: `Cancelar ${lower(stageLabel((running ?? paused)!.role))}`,
      danger: true,
      onSelect: () => void run(() => cancelDeliveryJob((running ?? paused)!.id)),
    },
    null,
    next && !broken
      ? {
          label: `Lanzar ${lower(stageLabel(next))}`,
          disabled: blocked,
          onSelect: () => dispatch(next, true),
        }
      : undefined,
    {
      label: "Lanzar etapa…",
      disabled: blocked,
      onSelect: () => dispatch(next ?? "tests", false),
    },
    step && !requesting
      ? {
          label:
            latestStep(approvals, step)?.status === "drifted"
              ? `Volver a pedir el ${lower(step.label)}…`
              : `Preparar ${lower(step.label)}…`,
          onSelect: () => setRequesting(true),
        }
      : undefined,
    null,
    !task.distro && { label: "Ver cambios", onSelect: () => void run(openChanges) },
    {
      label: `Abrir en Agents con ${agentName(lastStageAgent(jobs, task.id)?.agent ?? "claude")}`,
      onSelect: () =>
        void run(() => onOpenConversation(lastStageAgent(jobs, task.id)?.agent ?? "claude")),
    },
    issueUrl ? { label: "Ver issue en Jira", onSelect: () => setFloat("issue") } : undefined,
    pr && {
      label: `Abrir PR #${pr.number} en GitHub`,
      onSelect: () => void openDeliveryUrl(pr.url).catch(() => {}),
    },
    qaRecord?.visual_record
      ? {
          label: "Abrir registro de QA",
          onSelect: () => void run(() => openDeliveryVisualRecord(qaRecord.id)),
        }
      : undefined,
    null,
    open.length > 0 && {
      label: open.length === 1 ? "Quitar bloqueo" : `Quitar ${open.length} bloqueos`,
      onSelect: () => void run(() => clearDeliveryBlockers(task.id)),
    },
    !delivered && { label: "Bloquear por…", onSelect: () => setFloat("blocker") },
    {
      label: (
        <>
          Nueva versión del contrato… <small>v{task.contract_version}</small>
        </>
      ),
      onSelect: () => void run(bumpContract),
    },
    correctable.length > 0 && {
      label: "Corregir resultado…",
      onSelect: () => setCorrecting(correctable[0].id),
    },
    null,
    { label: `Copiar clave (${task.key})`, onSelect: () => copy(task.key) },
    { label: "Copiar rama", onSelect: () => copy(task.branch) },
    { label: "Copiar ruta del worktree", onSelect: () => copy(task.worktree) },
    pr && { label: "Copiar enlace del PR", onSelect: () => copy(pr.url) },
    null,
    delivered && {
      label: "Archivar tarea…",
      disabled: hasOpenJob,
      onSelect: () => void run(archive),
    },
    {
      label: "Eliminar tarea…",
      danger: true,
      disabled: hasOpenJob,
      title: hasOpenJob ? "Cancela o espera a que terminen sus etapas." : undefined,
      onSelect: () => void run(remove),
    },
  ];
  const contextItems = menuEntries.filter(
    (item): item is ContextItem | null => item !== false && item !== undefined,
  );

  return (
    <article className="delivery-task" aria-label={`Tarea ${taskLabel(task)}`}>
      {contextAt && (
        <ContextMenu
          at={contextAt}
          label={`Acciones de ${taskLabel(task)}`}
          items={contextItems}
          onClose={onCloseContext}
        />
      )}
      <header className="delivery-task__header">
        <div className="delivery-task__heading">
          <h2>
            {taskLabel(task)}
            {task.package?.title && <span> {task.package.title} ·</span>}
            {task.title && <span> {task.title}</span>}
          </h2>
          {(issueUrl || pr || qaRecord?.visual_record || qaProblems.length > 0) && (
            // Where the task goes outside its card: one quiet look for each.
            <div className="delivery-task__links">
              {issueUrl && (
                <button
                  type="button"
                  className="delivery-link-chip delivery-link-chip--jira"
                  aria-expanded={float === "issue"}
                  aria-label={`Ver ${task.key} de Jira`}
                  onClick={() => setFloat(float === "issue" ? null : "issue")}
                >
                  <Icon name="ticket" />
                  Jira
                </button>
              )}
              {pr && (
                <ExternalLink
                  className={`delivery-link-chip delivery-link-chip--pr delivery-link-chip--pr-${prMood(pr)}`}
                  href={pr.url}
                  label={`Abrir el PR #${pr.number} en GitHub`}
                >
                  <Icon name={pr.state === "merged" ? "merge" : "pull"} />
                  PR #{pr.number}
                  <span className="delivery-link-chip__state">· {lower(prSummary(pr).label)}</span>
                </ExternalLink>
              )}
              {qaRecord?.visual_record && (
                <ActionButton
                  className="delivery-link-chip delivery-link-chip--qa"
                  aria-label="Abrir registro visual de QA"
                  title={`Los cambios visuales que revisó la QA (intento ${qaRecord.attempt}), con capturas de escritorio y móvil. Se abre en tu navegador.`}
                  onAction={() => openDeliveryVisualRecord(qaRecord.id)}
                >
                  <Icon name="photo" />
                  Registro QA
                </ActionButton>
              )}
              {qaProblems.length > 0 && (
                <button
                  type="button"
                  className={`delivery-link-chip delivery-link-chip--problems${qaProblemsToDecide > 0 ? " delivery-link-chip--pending" : ""}`}
                  aria-expanded={float === "qa"}
                  aria-label={
                    qaProblemsToDecide > 0
                      ? `Problemas de la QA, ${qaProblemsToDecide} por decidir`
                      : "Problemas de la QA"
                  }
                  onClick={() => setFloat(float === "qa" ? null : "qa")}
                >
                  <Icon name="bug" />
                  Problemas
                  {qaProblemsToDecide > 0 && (
                    <span className="delivery-task__record-count">{qaProblemsToDecide}</span>
                  )}
                </button>
              )}
            </div>
          )}
          <p className="delivery-task__meta">
            <span>
              <code title={task.worktree}>{task.branch}</code> desde {task.base_ref} (
              {shortCommit(task.base_commit)})
            </span>
            {taskRun && <span>{batchName(taskRun.title || taskRun.id)}</span>}
            {progress && (
              <span title="El issue, por paquetes">
                {task.key}: {progress}
              </span>
            )}
            <span title={delivered ? undefined : "Lo fija quien coordina la tarea"}>
              Estado: {stateLabel}
            </span>
            {!task.distro && (
              <ActionButton
                className="delivery-link-button"
                title={`Abre el worktree en Archivos con solo lo que cambió desde ${task.base_ref}`}
                onAction={openChanges}
              >
                Ver cambios
              </ActionButton>
            )}
          </p>
          {/* What happens now, and in which round: one line. */}
          <div className="delivery-task__now" role="group" aria-label="Ahora">
            <p>
              <span className="delivery-task__now-label">Ahora</span>
              <span>
                {round && "↺ "}
                {nowLine(task, overview, now)}
              </span>
            </p>
            {round && round.items.length > 0 && (
              <details className="delivery-task__fold">
                <summary>Qué atiende la ronda {round.number}</summary>
                <ol>
                  {round.items.map((item, at) => (
                    <li key={at}>{item}</li>
                  ))}
                </ol>
              </details>
            )}
            {pastRounds.length > 0 && (
              <details className="delivery-task__fold">
                <summary>Rondas anteriores</summary>
                <ul>
                  {pastRounds.map((line) => (
                    <li key={line}>{line}</li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        </div>
        <Menu label="Más acciones de la tarea" trigger={<Icon name="more" />}>
          {(close) => (
            <>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void navigator.clipboard?.writeText(task.worktree).catch(() => {});
                }}
              >
                Copiar ruta del worktree
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void run(bumpContract);
                }}
              >
                Nueva versión del contrato… <small>v{task.contract_version}</small>
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={delivered}
                onClick={() => {
                  close();
                  setFloat("blocker");
                }}
              >
                Añadir bloqueo…
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={correctable.length === 0}
                title={
                  correctable.length === 0
                    ? "Aún no hay un PR, merge o Jira hecho que corregir."
                    : undefined
                }
                onClick={() => {
                  close();
                  setCorrecting(correctable[0].id);
                }}
              >
                Corregir resultado…
              </button>
              <button
                type="button"
                role="menuitem"
                disabled={!delivered || hasOpenJob}
                title={delivered ? undefined : "Se archiva una vez entregada."}
                onClick={() => {
                  close();
                  void run(archive);
                }}
              >
                Archivar tarea…
              </button>
              <hr />
              <button
                type="button"
                role="menuitem"
                className="delivery-menu__danger"
                disabled={hasOpenJob}
                title={hasOpenJob ? "Cancela o espera a que terminen sus etapas." : undefined}
                onClick={() => {
                  close();
                  void run(remove);
                }}
              >
                Eliminar tarea…
              </button>
            </>
          )}
        </Menu>
      </header>

      {/* One line in the task; the cards open on top only when asked for. */}
      {blocked && (
        <div className="delivery-pending-bar" role="status">
          <span>
            <strong>
              {pendingDecisions.length === 1
                ? "1 decisión pendiente"
                : `${pendingDecisions.length} decisiones pendientes`}
            </strong>{" "}
            <span className="delivery-muted">· las etapas de esta tarea esperan tu respuesta</span>
          </span>
          <button
            type="button"
            className="delivery-button--primary"
            onClick={() => setFloat("decisions")}
          >
            Responder
          </button>
        </div>
      )}
      {blocked && float === "decisions" && (
        <TaskFloat
          label="Responder decisiones"
          closeLabel="Cerrar decisiones"
          onClose={() => setFloat(null)}
        >
          <PendingDecisions taskId={task.id} decisions={pendingDecisions} />
        </TaskFloat>
      )}
      <BlockedBar
        task={task}
        tasks={overview.tasks}
        onSelectTask={onSelectTask}
        onAdd={() => setFloat("blocker")}
      />
      {float === "blocker" && (
        <TaskFloat
          label="Añadir bloqueo"
          closeLabel="Cerrar añadir bloqueo"
          onClose={() => setFloat(null)}
        >
          <AddBlocker task={task} onDone={() => setFloat(null)} />
        </TaskFloat>
      )}
      {float === "qa" && (
        <TaskFloat
          label="Problemas de la QA"
          closeLabel="Cerrar problemas de la QA"
          onClose={() => setFloat(null)}
        >
          <QaProblems taskId={task.id} problems={qaProblems} />
        </TaskFloat>
      )}
      {float === "issue" && issueUrl && (
        <TaskFloat
          label={`Incidencia ${task.key} de Jira`}
          closeLabel="Cerrar la incidencia"
          onClose={() => setFloat(null)}
        >
          <JiraIssue key={task.id} taskId={task.id} issueUrl={issueUrl} />
        </TaskFloat>
      )}

      <PendingSettingsProposals proposals={pendingSettingsProposals(overview, task)} />

      {openApprovals.map((approval) => (
        <ApprovalBar
          key={approval.id}
          approval={approval}
          onOpen={() => setFloat(`approval:${approval.id}`)}
        />
      ))}
      {floating && (
        <TaskFloat
          label={`Paso ${lower(stepOf(floating).label)}`}
          closeLabel="Cerrar el paso"
          onClose={() => setFloat(null)}
        >
          <ApprovalBanner task={task} approval={floating} pr={pr} />
        </TaskFloat>
      )}

      {rejected && openApprovals.length === 0 && (
        <RejectedStep task={task} approval={rejected} pr={pr} />
      )}

      {broken && (
        <BrokenStage
          key={broken.id}
          job={broken}
          undoable={latestUndoable?.id === broken.id}
          onContinue={() => onContinueJob(broken)}
          onUndo={() => undo(broken)}
        />
      )}

      {qaRecord?.visual_record_error && !qaRecord.visual_record && (
        <div
          className="delivery-banner delivery-banner--danger"
          role="group"
          aria-label="Registro visual de QA"
        >
          <p className="delivery-banner__title">
            <Icon name="x" />
            <strong>No se pudo generar el registro visual de la QA</strong>
          </p>
          <p>{qaRecord.visual_record_error}</p>
        </div>
      )}

      <StageTrack
        repo={task.repo}
        jobs={jobs}
        approvals={approvals}
        steps={trackSteps(ladder, approvals)}
        pr={pr}
        next={next}
        round={round}
        onFocus={focusStage}
      />

      <div className="delivery-actions">
        {next && !broken && (
          <button
            type="button"
            className={needsYou ? undefined : "delivery-button--primary"}
            disabled={blocked}
            onClick={() => dispatch(next, true)}
          >
            <Icon name="play" />
            Lanzar {lower(stageLabel(next))}
          </button>
        )}
        <button type="button" disabled={blocked} onClick={() => dispatch(next ?? "tests", false)}>
          {next && !broken ? "Otra etapa…" : "Lanzar etapa…"}
        </button>
        <Menu
          trigger={
            <>
              Abrir en Agents <Icon name="down" />
            </>
          }
        >
          {(close) =>
            (["codex", "claude", "muse"] as const).map((agent) => (
              <button
                key={agent}
                type="button"
                role="menuitem"
                onClick={() => {
                  close();
                  void onOpenConversation(agent);
                }}
              >
                Nueva conversación con {agentName(agent)}
              </button>
            ))
          }
        </Menu>
        {step && !requesting && (
          <button type="button" onClick={() => setRequesting(true)}>
            {latestStep(approvals, step)?.status === "drifted"
              ? `Volver a pedir el ${lower(step.label)}…`
              : `Preparar ${lower(step.label)}…`}
          </button>
        )}
        {blocked && (
          <span className="delivery-muted">
            Contesta las decisiones pendientes para lanzar etapas.
          </span>
        )}
        {!next && !hasOpenJob && !needsYou && (
          <span className="delivery-muted">
            {step
              ? `Todas las etapas pasaron. Lo siguiente es ${nextStepName(step)}.`
              : "Tarea entregada."}
          </span>
        )}
        {delivered && !hasOpenJob && (
          <ActionButton onAction={archive}>Archivar tarea…</ActionButton>
        )}
      </div>

      <StepChecks task={task} approvals={approvals} pr={pr} />

      <CommitStep approvals={approvals} />

      {step?.rung === "commit" && requesting && (
        <CommitPlanForm task={task} onClose={() => setRequesting(false)} />
      )}
      {step && step.rung !== "commit" && requesting && (
        <DeliveryRequest
          key={stepKey(step)}
          task={task}
          step={step}
          pr={pr}
          onClose={() => setRequesting(false)}
        />
      )}

      {pr && <PrSection pr={pr} />}

      <DeliveryResults approvals={approvals} onCorrect={setCorrecting} />
      {correcting && (
        <CorrectResultDialog
          steps={correctable}
          initial={correcting}
          onCancel={() => setCorrecting(null)}
          onSubmit={async (approvalId, outcome, reason) => {
            await correctDeliveryApproval(approvalId, outcome, reason);
            setCorrecting(null);
          }}
        />
      )}

      <CoordinatorDecisions
        decisions={taskDecisions.filter((decision) => decision.blocking === false)}
      />

      <DecisionLog
        taskId={task.id}
        candidate={candidate}
        decisions={taskDecisions.filter(
          (decision) => decision.status === "answered" && decision.blocking !== false,
        )}
      />

      <section className="delivery-section" aria-label="Historial">
        <h3>Historial</h3>
        {jobs.length === 0 && (
          <p className="delivery-panel__empty">Todavía no se ha lanzado ninguna etapa.</p>
        )}
        <ul className="delivery-jobs">
          {historyKeys.map((key, index) => {
            const ofKey = jobs.filter((job) => historyKey(job) === key);
            const current = latestOfRole(ofKey, ofKey[0].role)!;
            const older = newestFirst.filter((job) => historyKey(job) === key && job !== current);
            return (
              <StageHistory
                key={key}
                // The stage track scrolls to a stage; a check has no node there.
                id={`delivery-stage-${task.id}-${key === current.role ? key : `check-${index}`}`}
                current={current}
                older={older}
                jobs={jobs}
                renderJob={(job) => (
                  <JobRow
                    key={job.id}
                    job={job}
                    jobs={jobs}
                    wait={overview.queued?.[job.id]}
                    now={now}
                    expanded={expandedJobId === job.id}
                    undoable={latestUndoable?.id === job.id && !hasOpenJob}
                    onToggle={() => setExpandedJobId(expandedJobId === job.id ? null : job.id)}
                    onUndo={() => undo(job)}
                    onContinue={() => onContinueJob(job)}
                  />
                )}
              />
            );
          })}
        </ul>
      </section>
    </article>
  );
}

/** Past this, why a stage broke shows its first lines until asked for all. */
const BROKEN_WHY_CHARS = 240;

/** The task's latest stage failed or was blocked: why, in a few lines, and
 *  what to do about it. When its work got done another way, the user settles
 *  it with a note instead of running it again. */
function BrokenStage({
  job,
  undoable,
  onContinue,
  onUndo,
}: {
  job: DeliveryJob;
  undoable: boolean;
  onContinue: () => Promise<unknown>;
  onUndo: () => Promise<unknown>;
}) {
  const [whole, setWhole] = useState(false);
  const [settling, setSettling] = useState(false);
  const [note, setNote] = useState("");
  const why = [
    job.error,
    job.result_note,
    job.result?.status === "blocked" ? job.result.summary : null,
  ].filter((text): text is string => !!text?.trim());
  const long = why.join(" ").length > BROKEN_WHY_CHARS || why.some((text) => text.includes("\n"));
  return (
    <div
      className="delivery-banner delivery-banner--danger delivery-broken"
      role="group"
      aria-label="Etapa fallida"
    >
      <p className="delivery-banner__title">
        <Icon name="x" />
        <strong>
          {stageLabel(job.role)} {brokenVerb(job)}
        </strong>
        <span className="delivery-muted">intento {job.attempt}</span>
        {job.changes.length > 0 && !job.undone_at_ms && (
          <span className="delivery-muted">· dejó {files(job.changes.length)} cambiados</span>
        )}
      </p>
      {why.length > 0 && (
        <div
          className={`delivery-broken__why${long && !whole ? " delivery-broken__why--clamped" : ""}`}
        >
          {why.map((text, at) => (
            <p key={at}>{text}</p>
          ))}
        </div>
      )}
      {long && (
        <button
          type="button"
          className="delivery-link-button delivery-broken__more"
          aria-expanded={whole}
          onClick={() => setWhole(!whole)}
        >
          {whole ? "Ver menos" : "Ver todo"}
        </button>
      )}
      {settling ? (
        <form className="delivery-broken__settle" onSubmit={(event) => event.preventDefault()}>
          <label className="delivery-request__field">
            <span>Cómo quedó resuelta, en una línea (queda en su historial)</span>
            <input
              value={note}
              onChange={(event) => setNote(event.target.value)}
              placeholder="Otra sesión terminó el trabajo y la rama ya lo tiene"
              maxLength={300}
              required
              autoFocus
            />
          </label>
          <ActionGroup>
            <div className="delivery-actions">
              <ActionButton
                type="submit"
                className="delivery-button--primary"
                onAction={() => resolveDeliveryJob(job.id, note)}
              >
                Dar por resuelta
              </ActionButton>
              <button type="button" onClick={() => setSettling(false)}>
                Cancelar
              </button>
            </div>
          </ActionGroup>
        </form>
      ) : (
        <ActionGroup>
          <div className="delivery-actions">
            <ActionButton
              className="delivery-button--primary"
              onAction={() => retryDeliveryJob(job.id)}
            >
              Reintentar
            </ActionButton>
            {job.agent !== "shell" && job.provider_session_id && (
              <ActionButton onAction={onContinue}>Continuar en Agents</ActionButton>
            )}
            {undoable && <ActionButton onAction={onUndo}>Deshacer cambios</ActionButton>}
            <button
              type="button"
              title="Su trabajo se hizo de otra forma: la etapa cuenta como hecha sin repetirla"
              onClick={() => setSettling(true)}
            >
              Ya está resuelta…
            </button>
          </div>
        </ActionGroup>
      )}
    </div>
  );
}

function brokenVerb(job: DeliveryJob): string {
  if (job.status === "interrupted") return "se interrumpió";
  if (job.status === "failed") return "falló";
  if (job.result_state === "invalid") return "terminó sin un resultado válido";
  return "está bloqueada";
}

/** Who runs a stage from now on in the task's repo: the same setting as
 *  Ajustes › Agente de cada etapa, saved as soon as it changes. */
function StageAgentEditor({ repo, role }: { repo: string; role: string }) {
  const [settings, setSettings] = useState<DeliveryRepoSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const stage = role.toLowerCase();
  useEffect(() => {
    let live = true;
    getDeliveryRepoSettings(repo).then(
      (loaded) => live && setSettings(loaded),
      (cause) => live && setError(errorText(cause)),
    );
    return () => {
      live = false;
    };
  }, [repo]);
  const save = async (next: DeliveryStageAgent | null) => {
    setError(null);
    try {
      // The latest settings, so nothing changed elsewhere is lost.
      const current = await getDeliveryRepoSettings(repo);
      const stages = { ...current.stages };
      if (next) stages[stage] = next;
      else delete stages[stage];
      await setDeliveryRepoSettings(repo, { ...current, stages });
      setSettings({ ...current, stages });
    } catch (cause) {
      setError(errorText(cause));
    }
  };
  return (
    <div className="delivery-stage-agent">
      <p className="delivery-muted">
        Para los próximos lanzamientos de {lower(stageLabel(role))} en este repositorio, tuyos o del
        coordinador. Lo que ya corre no cambia.
      </p>
      {settings ? (
        <StageAgentRow
          label={stageLabel(role)}
          value={settings.stages?.[stage]}
          onChange={(next) => void save(next)}
        />
      ) : (
        !error && <p className="delivery-muted">Leyendo los ajustes…</p>
      )}
      {error && <p className="delivery-tone--danger">{error}</p>}
    </div>
  );
}

function StageTrack({
  repo,
  jobs,
  approvals,
  steps,
  pr,
  next,
  round,
  onFocus,
}: {
  /** The task's repo, whose settings say who runs each stage. */
  repo: string;
  jobs: DeliveryJob[];
  approvals: DeliveryApproval[];
  /** The delivery steps the task climbs. */
  steps: DeliveryStep[];
  /** The PR step shows the followed PR's live state once it exists. */
  pr: DeliveryPr | undefined;
  next: string | null;
  /** The round the task is in: the track shows that round's stages and steps. */
  round: DeliveryCurrentRound | null;
  onFocus: (job: DeliveryJob) => void;
}) {
  return (
    <div className="delivery-track">
      <div className="delivery-track__part">
        <h3 id="delivery-track-agents">Agentes</h3>
        <ol aria-labelledby="delivery-track-agents">
          {rolesOf(jobs).map((role) => {
            const job = latestOfRole(jobs, role);
            const inRound = roundStage(role, job, jobs, round);
            const outcome = inRound
              ? { ...inRound, label: role === next ? "Siguiente" : inRound.label }
              : job
                ? attemptOutcome(job, jobs)
                : { tone: "todo" as const, label: role === next ? "Siguiente" : "—" };
            const tone = outcome.tone;
            const content = (
              <>
                <span className="delivery-node__name">
                  <StatusMark icon={tone === "todo" ? "ring" : TONE_ICON[tone]} />
                  {stageLabel(role)}
                </span>
                <small>{outcome.label}</small>
                {job?.visual_record && !inRound && <small>Con registro visual ↓</small>}
                {job && job.agent !== "shell" && (
                  <small className="delivery-node__runtime">{runtimeLabel(job)}</small>
                )}
              </>
            );
            return (
              <li
                key={role}
                className={`delivery-node delivery-node--${tone}`}
                aria-current={role === next ? "step" : undefined}
              >
                {job ? (
                  <button
                    type="button"
                    title={`${stageLabel(role)} · intento ${job.attempt} · ${agentName(job.agent)} · ${runtimeLabel(job)}`}
                    onClick={() => onFocus(job)}
                  >
                    {content}
                  </button>
                ) : (
                  <div>{content}</div>
                )}
                <Menu
                  label={`Agente, modelo y razonamiento de ${stageLabel(role)}`}
                  trigger={<Icon name="down" />}
                >
                  {() => <StageAgentEditor repo={repo} role={role} />}
                </Menu>
              </li>
            );
          })}
        </ol>
      </div>
      <div className="delivery-track__part delivery-track__part--rungs">
        <h3 id="delivery-track-rungs">Entrega</h3>
        <ol aria-labelledby="delivery-track-rungs">
          {steps.map((ladder) => {
            const { rung, label } = ladder;
            const live = rung === "pr" && pr ? prSummary(pr) : null;
            const approval = latestStep(approvals, ladder);
            const unlinked = rung === "pr" && !pr && approval?.status === "executed";
            const before = rungStep(approval);
            const step = roundRung(ladder, approval, round)
              ? {
                  tone: "todo" as const,
                  icon: "ring" as const,
                  label: "Se repetirá",
                  // What it was before the round, for who asks.
                  title: `Se repetirá en esta ronda${approval ? `. Antes: ${before.title ?? before.label}` : ""}`,
                }
              : live
                ? { ...live, title: `${pr?.repo}#${pr?.number}` }
                : unlinked
                  ? {
                      tone: "step" as const,
                      icon: "alert" as const,
                      label: "Hecho · sin PR vinculado",
                      title: approval.outcome ?? undefined,
                    }
                  : before;
            return (
              <li key={stepKey(ladder)} className={`delivery-node delivery-node--${step.tone}`}>
                <div title={step.title}>
                  <span className="delivery-node__name">
                    <StatusMark icon={step.icon} />
                    {label}
                  </span>
                  <small>{step.label}</small>
                </div>
              </li>
            );
          })}
        </ol>
      </div>
    </div>
  );
}

function rungStep(approval: DeliveryApproval | undefined): {
  tone: Tone | "todo";
  icon: StatusIcon;
  label: string;
  title?: string;
} {
  if (!approval) return { tone: "todo", icon: "ring", label: "—" };
  switch (approval.status) {
    case "pending":
      return { tone: "step", icon: "flag", label: "Tu aprobación" };
    case "approved":
      return { tone: "waiting", icon: "clock", label: "Lo hace el coordinador" };
    case "executed":
      if (approval.conflicts?.length)
        return {
          tone: "attention",
          icon: "alert",
          label: `Conflictos en ${approval.conflicts.join(", ")}`,
          title: approval.outcome ?? undefined,
        };
      return {
        tone: "ok",
        icon: "check",
        label: approval.outcome || "Hecho",
        title: approval.outcome ?? undefined,
      };
    case "rejected":
      return { tone: "danger", icon: "x", label: "Rechazado" };
    case "undone":
      return {
        tone: "todo",
        icon: "ring",
        label: "Deshecho",
        title: approval.outcome ?? undefined,
      };
    case "drifted":
      return {
        tone: "step",
        icon: "alert",
        label: "Ya no coincide",
        title: approval.notice ?? undefined,
      };
    default:
      return { tone: "danger", icon: "x", label: "Falló", title: approval.outcome ?? undefined };
  }
}

/** Who prepared a request. One made in Tinto's own view may come from the
 *  user or from whoever drives the view for them; the API names its caller. */
function preparedBy(approval: DeliveryApproval): string {
  return approval.requested_by === "user"
    ? "preparado desde la interfaz de Tinto"
    : `preparado por el coordinador ${approval.requested_by}`;
}

/** What Tinto found when it checked the delivery steps against git and
 *  GitHub, with the way to put each one right. */
function StepChecks({
  task,
  approvals,
  pr,
}: {
  task: DeliveryTask;
  approvals: DeliveryApproval[];
  pr: DeliveryPr | undefined;
}) {
  type Check = {
    text: string;
    action: { label: string; run: () => Promise<unknown> } | null;
  };
  const checks = STEPS.flatMap((step): Check[] => {
    const { rung, label } = step;
    const approval = latestStep(approvals, step);
    if (!approval) return [];
    if (rung === "pr" && approval.status === "executed" && !pr) {
      return [
        {
          text: "PR: el paso está hecho, pero la tarea no tiene un PR vinculado.",
          action: { label: "Buscar el PR de la rama", run: () => findDeliveryPr(task.id) },
        },
      ];
    }
    if (!approval.notice) return [];
    const drifted = approval.status === "drifted";
    return [
      {
        text: `${label}: ${drifted ? "ya no coincide. " : ""}${approval.notice}.`,
        action:
          drifted && rung === "pr"
            ? { label: "Buscar otro PR de la rama", run: () => findDeliveryPr(task.id) }
            : null,
      },
    ];
  });
  if (checks.length === 0) return null;
  return (
    <div className="delivery-step-checks" role="group" aria-label="Comprobaciones de la entrega">
      {checks.map(({ text, action }) => (
        <p key={text}>
          <span className="delivery-job__note">{text}</span>
          {action && <ActionButton onAction={action.run}>{action.label}</ActionButton>}
        </p>
      ))}
    </div>
  );
}

/** A panel on top of the task, opened by the user: it takes the focus once
 *  (later refreshes must not take it from a card) and closes on Escape. */
function TaskFloat({
  label,
  closeLabel,
  onClose,
  children,
}: {
  label: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    panel.current?.focus();
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
    // Once, when it opens; onClose only closes it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <div className="delivery-float" role="dialog" aria-label={label} tabIndex={-1} ref={panel}>
      <button
        type="button"
        className="delivery-float__close"
        aria-label={closeLabel}
        onClick={onClose}
      >
        ×
      </button>
      {children}
    </div>
  );
}

/** "el merge", "el PR", "Jira (merge)": the step as a sentence names it. */
function nextStepName(step: DeliveryStep): string {
  return step.rung === "jira" ? step.label : `el ${lower(step.label)}`;
}

function when(ms: number | null | undefined): string {
  return ms ? new Date(ms).toLocaleString([], { dateStyle: "short", timeStyle: "short" }) : "";
}

const isDone = (approval: DeliveryApproval | undefined): approval is DeliveryApproval =>
  approval?.status === "executed" || approval?.status === "drifted";

/** Done steps whose result Tinto cannot check against git: the user's to
 *  correct. Commit and push are checked, so they are not here. */
function correctableSteps(approvals: DeliveryApproval[]): CorrectableStep[] {
  return STEPS.filter(({ rung }) => rung !== "commit" && rung !== "push").flatMap((step) => {
    const approval = latestStep(approvals, step);
    return isDone(approval)
      ? [{ id: approval.id, label: step.label, outcome: approval.outcome ?? "" }]
      : [];
  });
}

/** Each done step's result, with the corrections the user made and when. */
function DeliveryResults({
  approvals,
  onCorrect,
}: {
  approvals: DeliveryApproval[];
  onCorrect: (approvalId: string) => void;
}) {
  const done = STEPS.flatMap((step) => {
    const approval = latestStep(approvals, step);
    return isDone(approval) ? [{ step, approval }] : [];
  });
  if (done.length === 0) return null;
  return (
    <section className="delivery-section" aria-label="Resultados de la entrega">
      <h3>Resultados de la entrega</h3>
      <ul className="delivery-results">
        {done.map(({ step: { rung, label, stage }, approval }) => {
          const corrections = approval.corrections ?? [];
          // Oldest first: what was recorded, then each correction.
          const history = [
            {
              text: corrections[0]?.previous ?? approval.outcome ?? "—",
              at: approval.executed_at_ms,
              reason: null as string | null,
            },
            ...corrections.map((c) => ({ text: c.outcome, at: c.at_ms, reason: c.reason })),
          ];
          return (
            <li key={stage ?? rung} aria-label={label}>
              <strong>{label}</strong>
              <ol className="delivery-results__history">
                {history.map((entry, index) => (
                  <li
                    key={index}
                    className={index < history.length - 1 ? "delivery-muted" : undefined}
                  >
                    <span>{entry.text}</span>{" "}
                    <small className="delivery-muted">
                      {index === 0 ? when(entry.at) : `Corregido el ${when(entry.at)}`}
                      {entry.reason && ` · Por qué: ${entry.reason}`}
                    </small>
                  </li>
                ))}
              </ol>
              {rung !== "commit" && rung !== "push" && (
                <button
                  type="button"
                  className="delivery-link-button"
                  onClick={() => onCorrect(approval.id)}
                >
                  Corregir resultado…
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** What happened to the task's last commit step when it is not simply
 *  done, and the way to take back commits that are not pushed yet. */
function CommitStep({ approvals }: { approvals: DeliveryApproval[] }) {
  const commit = latestApproval(approvals, "commit");
  if (!commit) return null;
  const pushed = latestApproval(approvals, "push")?.status === "executed";
  // Steps recorded before plans existed only have "Commit <hash>" as outcome.
  const made =
    commit.created?.length ||
    (commit.status === "executed" && commit.outcome?.startsWith("Commit ") ? 1 : 0);
  const undoable =
    !pushed && made > 0 && (commit.status === "executed" || commit.status === "failed");
  const notice = commit.status === "undone" || commit.status === "failed";
  if (!undoable && !notice) return null;
  return (
    <div className="delivery-commit-step" role="group" aria-label="Paso de commit">
      {notice && commit.outcome && <p className="delivery-job__note">{commit.outcome}</p>}
      {undoable && (
        <ActionButton
          title="Los cambios vuelven al worktree, sin confirmar; no se pierde nada."
          onAction={async () => {
            const ok = await confirm(
              `La rama volverá a donde estaba antes de ${made === 1 ? "el commit" : `los ${made} commits`} y sus cambios quedarán en el worktree, sin confirmar. No se pierde nada.`,
              { title: "Deshacer el commit", okLabel: "Deshacer" },
            );
            if (!ok) return false;
            await undoDeliveryCommit(commit.id);
          }}
        >
          {made === 1 ? "Deshacer el commit" : `Deshacer los ${made} commits`}
        </ActionButton>
      )}
    </div>
  );
}

/** The latest step, when the user rejected it and nothing was asked since. */
function rejectedStep(approvals: DeliveryApproval[]): DeliveryApproval | null {
  const latest = [...approvals].sort((a, b) => b.requested_at_ms - a.requested_at_ms)[0];
  return latest?.status === "rejected" ? latest : null;
}

/** What stays as it is when the user rejects a step: a rejection only stops
 *  that step, it undoes none before it. */
function rejectionScope(
  approval: DeliveryApproval,
  task: DeliveryTask,
  pr: DeliveryPr | undefined,
) {
  const openPr = pr && pr.state === "open" ? `El PR #${pr.number} sigue abierto en GitHub` : null;
  switch (approval.rung) {
    case "commit":
      return `Rechazar no toca los cambios: siguen sin commit en ${task.branch}.`;
    case "sync":
      return `Rechazar no toca la rama: ${task.branch} sigue sin la base.`;
    case "push":
      return `Rechazar no deshace los commits: siguen en ${task.branch}, sin publicar.`;
    case "pr":
      return `Rechazar no borra nada: la rama ${task.branch} sigue publicada en GitHub, sin PR.`;
    case "merge":
      return openPr
        ? `${openPr}; rechazar este paso solo evita fusionarlo.`
        : "Rechazar este paso solo evita fusionar el PR.";
    default:
      if (approval.stage === "after_merge")
        return "El PR ya está fusionado; rechazar este paso solo evita los cambios en Jira.";
      return openPr
        ? `${openPr}; rechazar este paso solo evita los cambios en Jira.`
        : "Rechazar este paso solo evita los cambios en Jira.";
  }
}

/** A rejected step: what still stands, and how to undo the PR when it is open. */
function RejectedStep({
  task,
  approval,
  pr,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  pr: DeliveryPr | undefined;
}) {
  const openPr = pr?.state === "open";
  return (
    <div className="delivery-banner delivery-banner--step" role="group" aria-label="Paso rechazado">
      <p className="delivery-banner__title">
        <Icon name="x" />
        <strong>Rechazaste {lower(stepOf(approval).label)}</strong>
      </p>
      <p className="delivery-muted">{rejectionScope(approval, task, pr)}</p>
      {openPr && (
        <ActionGroup>
          <div className="delivery-actions">
            {!pr.draft && (
              <ActionButton onAction={() => deliveryPrToDraft(task.id)}>
                Pasar el PR a borrador
              </ActionButton>
            )}
            <ActionButton onAction={() => closeDeliveryPr(task.id)}>
              Cerrar el PR #{pr.number}
            </ActionButton>
          </div>
        </ActionGroup>
      )}
    </div>
  );
}

/** An open step as one line in the task; its text and buttons open on top. */
function ApprovalBar({ approval, onOpen }: { approval: DeliveryApproval; onOpen: () => void }) {
  const name = lower(stepOf(approval).label);
  const pending = approval.status === "pending";
  const commits = approval.commits ?? [];
  const summary =
    commits.length > 0
      ? `${commits.length === 1 ? "1 commit" : `${commits.length} commits`} · ${files(commits.reduce((sum, commit) => sum + commit.paths.length, 0))}`
      : approval.title;
  return (
    <div className="delivery-pending-bar delivery-pending-bar--step" role="status">
      <span className="delivery-pending-bar__text">
        <strong>{pending ? `Aprobar ${name}` : `Esperando al coordinador: ${name}`}</strong>{" "}
        <span className="delivery-muted">· {summary}</span>
      </span>
      <button
        type="button"
        className={pending ? "delivery-button--primary" : undefined}
        aria-label={`${pending ? "Revisar" : "Ver"} ${name}`}
        onClick={onOpen}
      >
        {pending ? "Revisar" : "Ver"}
      </button>
    </div>
  );
}

function ApprovalBanner({
  task,
  approval,
  pr,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  pr: DeliveryPr | undefined;
}) {
  const [outcome, setOutcome] = useState("");
  const name = lower(stepOf(approval).label);
  const pending = approval.status === "pending";
  const choosesReviewers = pending && approval.rung === "pr";
  const [reviewers, setReviewers] = useState<string[]>(approval.reviewers ?? []);
  // The repo's people who can review, or why they could not be read.
  const [candidates, setCandidates] = useState<string[] | null>(null);
  const [candidatesError, setCandidatesError] = useState<string | null>(null);
  useEffect(() => {
    if (!choosesReviewers) return;
    let live = true;
    getDeliveryPrReviewers(task.id).then(
      (logins) => live && setCandidates(logins),
      (cause) => live && setCandidatesError(errorText(cause)),
    );
    return () => {
      live = false;
    };
  }, [choosesReviewers, task.id]);
  // Without a reviewer nobody can approve the PR, and it cannot be merged.
  const needsReviewer = choosesReviewers && (candidates?.length ?? 0) > 0 && reviewers.length === 0;
  const consequence =
    approval.rung === "commit"
      ? `Al aprobar, Tinto crea ${(approval.commits?.length ?? 1) > 1 ? "estos commits, en este orden," : "el commit"} en ${task.branch} con estos textos exactos y sin añadir firmas. Si uno falla, se detiene. No publica nada: el push se aprueba aparte, y hasta entonces puedes deshacerlo.`
      : approval.rung === "sync"
        ? `Al aprobar, Tinto trae la base y la fusiona en ${task.branch} con este mensaje: un commit de merge, sin publicar. Si hay conflictos, el merge queda en curso, un job de implementación los resuelve y el paso commit lo concluye.`
        : approval.rung === "push"
          ? `Al aprobar, Tinto publica la rama ${task.branch}.`
          : approval.rung === "pr"
            ? `Al aprobar, Tinto abre en GitHub el PR de ${task.branch} con este título y cuerpo exactos (o se los pone al PR abierto de la rama). Desde entonces lo sigue y pide la revisión a quienes elijas. Sin una revisión aprobada no se podrá fusionar.`
            : approval.rung === "merge"
              ? `Al aprobar, Tinto fusiona el PR en GitHub (${approval.merge_method ?? "squash"}) con este mensaje y borra allí la rama ${task.branch}. La rama local sigue hasta que elimines la tarea.`
              : approval.jira_link_only
                ? `Al aprobar, Tinto enlaza este PR en ${task.key} sin cambiar su estado: otro paquete del issue ya lo movió.`
                : approval.jira_statuses?.length
                  ? `Al aprobar, Tinto ${approval.stage === "after_merge" ? "" : "enlaza el PR en la issue y "}lleva ${task.key} a ${approval.jira_statuses.join(" → ")} en Jira, y comprueba el estado final.`
                  : "Al aprobar, el coordinador hace estas operaciones en Jira y las informa; no tienes que hacer nada más.";
  return (
    <div
      className="delivery-banner delivery-banner--step"
      role="group"
      aria-label={`Aprobación ${approval.rung}`}
    >
      <p className="delivery-banner__title">
        <Icon name="flag" />
        <strong>{pending ? `Aprobar ${name}` : `Esperando al coordinador: ${name}`}</strong>
        <span className="delivery-muted">
          {pending ? preparedBy(approval) : "aprobado; el coordinador lo hace y lo informa"}
        </span>
      </p>
      {!pending && approval.notice && <p className="delivery-job__note">{approval.notice}</p>}
      {!pending && approval.rung === "pr" && (
        <div className="delivery-actions">
          <ActionButton
            className="delivery-button--primary"
            onAction={() => openDeliveryPr(approval.id)}
          >
            Abrir el PR en GitHub
          </ActionButton>
          <span className="delivery-muted">
            Tinto lo abre desde {task.branch} con el título y el cuerpo aprobados.
          </span>
        </div>
      )}
      {approval.rung === "commit" ? (
        <>
          <CommitPlanView approval={approval} />
          {approval.single_commit_reason && (
            <p className="delivery-job__note">
              Un solo commit porque: {approval.single_commit_reason}
            </p>
          )}
        </>
      ) : (
        <pre className="delivery-banner__text">
          {approval.title}
          {approval.body ? `\n\n${approval.body}` : ""}
        </pre>
      )}
      {choosesReviewers && (
        <fieldset className="delivery-reviewers">
          <legend>Revisores</legend>
          {candidates === null && !candidatesError && (
            <p className="delivery-muted">Leyendo quién puede revisar en GitHub…</p>
          )}
          {candidatesError && (
            <p className="delivery-job__note">
              No se pudo leer quién puede revisar: {candidatesError}. Pídelo en GitHub.
            </p>
          )}
          {candidates?.length === 0 && (
            <p className="delivery-job__note">
              El repositorio no tiene a nadie más que pueda revisar.
            </p>
          )}
          {candidates?.map((login) => (
            <label key={login}>
              <input
                type="checkbox"
                checked={reviewers.includes(login)}
                onChange={(event) =>
                  setReviewers(
                    event.target.checked
                      ? [...reviewers, login]
                      : reviewers.filter((chosen) => chosen !== login),
                  )
                }
              />
              {login}
            </label>
          ))}
        </fieldset>
      )}
      {pending && <p className="delivery-muted">{consequence}</p>}
      {pending ? (
        <ActionGroup>
          <div className="delivery-actions">
            <ActionButton
              className="delivery-button--primary"
              disabled={needsReviewer}
              title={needsReviewer ? "Elige al menos un revisor." : undefined}
              onAction={() =>
                choosesReviewers
                  ? decideDeliveryApproval(approval.id, true, undefined, reviewers)
                  : decideDeliveryApproval(approval.id, true)
              }
            >
              {approval.rung === "commit"
                ? "Aprobar y hacer commit"
                : approval.rung === "sync"
                  ? "Aprobar y sincronizar"
                  : approval.rung === "push"
                    ? "Aprobar y publicar"
                    : approval.rung === "merge"
                      ? "Aprobar y fusionar"
                      : approval.rung === "pr"
                        ? "Aprobar y abrir el PR"
                        : "Aprobar"}
            </ActionButton>
            <ActionButton
              title={rejectionScope(approval, task, pr)}
              onAction={() => decideDeliveryApproval(approval.id, false)}
            >
              Rechazar
            </ActionButton>
            <span className="delivery-muted">{rejectionScope(approval, task, pr)}</span>
          </div>
        </ActionGroup>
      ) : (
        // Only a correction: the coordinator reports the step. Nothing here
        // waits on it.
        <details className="delivery-manual-record">
          <summary>Lo hice fuera de Tinto: registrar el paso hecho</summary>
          <div className="delivery-actions">
            <input
              aria-label="Resultado del paso"
              placeholder={
                approval.rung === "pr"
                  ? "URL del PR (si no, Tinto lo busca por la rama)"
                  : "Qué se hizo"
              }
              value={outcome}
              onChange={(event) => setOutcome(event.target.value)}
            />
            <ActionButton
              disabled={approval.rung === "jira" && !outcome.trim()}
              title={
                approval.rung === "jira" && !outcome.trim()
                  ? "Escribe qué se hizo en Jira: transiciones y estado final."
                  : undefined
              }
              onAction={() => completeDeliveryApproval(approval.id, true, outcome)}
            >
              Marcar como hecho
            </ActionButton>
          </div>
        </details>
      )}
    </div>
  );
}

function DeliveryRequest({
  task,
  step,
  pr,
  onClose,
}: {
  task: DeliveryTask;
  step: DeliveryStep;
  /** The followed PR, for GitHub's own merge commit message. */
  pr: DeliveryPr | undefined;
  onClose: () => void;
}) {
  const named = task.title ? `${task.key}: ${task.title}` : task.key;
  const { rung } = step;
  // With a merge commit, GitHub's own message; otherwise the task's name.
  const mergeTitle = (method: string) =>
    method === "merge" && pr
      ? `Merge pull request #${pr.number} from ${pr.repo.split("/")[0]}/${task.branch}`
      : named;
  const [title, setTitle] = useState(rung === "push" ? `Publicar ${task.branch}` : named);
  // The commit step has its own form (CommitPlanForm).
  const [body, setBody] = useState("");
  const [method, setMethod] = useState("squash");
  // Once the user picks a method or writes the message, the defaults stop.
  const methodEdited = useRef(false);
  const titleEdited = useRef(false);
  const merge = rung === "merge";
  const name = lower(step.label);
  const chooseMethod = (next: string) => {
    setMethod(next);
    if (!titleEdited.current) setTitle(mergeTitle(next));
  };
  // The method the base branch's latest PRs were merged with.
  useEffect(() => {
    if (!merge) return;
    let live = true;
    getDeliveryMergeMethod(task.id).then(
      (proposed) => {
        if (live && !methodEdited.current) chooseMethod(proposed);
      },
      () => {},
    );
    return () => {
      live = false;
    };
    // Proposed once, when the form opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // A Jira step Tinto runs itself when the repo's Jira is set in Tinto.
  const [jiraByTinto, setJiraByTinto] = useState(false);
  const [statuses, setStatuses] = useState("");
  useEffect(() => {
    if (rung !== "jira") return;
    let live = true;
    getDeliveryRepoSettings(task.repo).then(
      (settings) => {
        const jira = settings.jira;
        if (!live || !jira?.has_token) return;
        setJiraByTinto(true);
        setStatuses((step.stage === "after_merge" ? jira.after_merge : jira.after_pr).join(", "));
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [rung, step.stage, task.repo]);
  return (
    <form
      className="delivery-request"
      aria-label={`Pedir aprobación de ${name}`}
      // The submit button sends it, also on Enter.
      onSubmit={(event) => event.preventDefault()}
    >
      {merge && (
        <label className="delivery-request__field">
          <span>Cómo fusionar</span>
          <select
            value={method}
            onChange={(event) => {
              methodEdited.current = true;
              chooseMethod(event.target.value);
            }}
          >
            <option value="squash">Squash: un solo commit</option>
            <option value="merge">Merge commit</option>
            <option value="rebase">Rebase</option>
          </select>
        </label>
      )}
      <label className="delivery-request__field">
        <span>
          {merge
            ? method === "rebase"
              ? "Descripción (rebase no crea un commit propio)"
              : "Mensaje del commit de fusión"
            : "Texto exacto a aprobar"}
        </span>
        <input
          value={title}
          onChange={(event) => {
            titleEdited.current = true;
            setTitle(event.target.value);
          }}
          required
        />
      </label>
      {jiraByTinto && (
        <label className="delivery-request__field">
          <span>Estados de {task.key} en Jira, en orden</span>
          <input
            value={statuses}
            onChange={(event) => setStatuses(event.target.value)}
            placeholder="En ejecución, In Review"
            required
          />
        </label>
      )}
      <label className="delivery-request__field">
        <span>
          {rung === "jira" && !jiraByTinto ? "Operaciones exactas en Jira" : "Detalle (opcional)"}
        </span>
        <textarea value={body} onChange={(event) => setBody(event.target.value)} rows={3} />
      </label>
      {jiraByTinto && (
        <p className="delivery-muted">
          Al aprobarlo, Tinto
          {step.stage === "after_pr" ? " enlaza el PR en la issue y la" : " la"} lleva por estos
          estados en Jira, y comprueba el estado final.
        </p>
      )}
      <div className="delivery-actions">
        <ActionButton
          type="submit"
          className="delivery-button--primary"
          onAction={async () => {
            await (merge
              ? requestDeliveryApproval(task.id, rung, title, body, undefined, method)
              : step.stage
                ? requestDeliveryApproval(
                    task.id,
                    rung,
                    title,
                    body,
                    undefined,
                    undefined,
                    step.stage,
                    jiraByTinto ? parseStatuses(statuses) : undefined,
                  )
                : requestDeliveryApproval(task.id, rung, title, body));
            onClose();
          }}
        >
          Pedir aprobación
        </ActionButton>
        <button type="button" onClick={onClose}>
          Cancelar
        </button>
      </div>
    </form>
  );
}

function StageHistory({
  id,
  current,
  older,
  jobs,
  renderJob,
}: {
  id: string;
  current: DeliveryJob;
  older: DeliveryJob[];
  /** Every job of the task, oldest first. */
  jobs: DeliveryJob[];
  renderJob: (job: DeliveryJob) => ReactNode;
}) {
  const [showOlder, setShowOlder] = useState(false);
  return (
    <>
      <li id={id} className="delivery-stage">
        <ul>{renderJob(current)}</ul>
      </li>
      {older.length > 0 && (
        <li className="delivery-older">
          <button type="button" aria-expanded={showOlder} onClick={() => setShowOlder(!showOlder)}>
            <Icon name={showOlder ? "down" : "right"} />
            {older.length === 1 ? "1 intento anterior" : `${older.length} intentos anteriores`}
            <span className="delivery-muted">
              · {older.map((job) => lower(attemptOutcome(job, jobs).label)).join(", ")}
            </span>
          </button>
          {showOlder && <ul>{older.map(renderJob)}</ul>}
        </li>
      )}
    </>
  );
}

function JobRow({
  job,
  jobs,
  wait,
  now,
  expanded,
  undoable,
  onToggle,
  onUndo,
  onContinue,
}: {
  job: DeliveryJob;
  /** Every job of the task, oldest first. */
  jobs: DeliveryJob[];
  /** Why it has not started, while it is queued. */
  wait?: DeliveryQueueWait;
  now: number;
  expanded: boolean;
  undoable: boolean;
  onToggle: () => void;
  onUndo: () => Promise<unknown>;
  onContinue: () => Promise<unknown>;
}) {
  const outcome = attemptOutcome(job, jobs);
  const quiet = quietFor(job, now);
  const held = pausedJob(jobs)?.id === job.id;
  const scope =
    (job.access === "full" ? " · acceso completo" : job.writes ? "" : " · solo lectura") +
    (job.qa_browser ? " · navegador" : "") +
    (job.ran_on ? ` · en ${job.ran_on}` : "");
  return (
    <li className="delivery-job">
      <button
        type="button"
        className="delivery-job__summary"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <Icon name={expanded ? "down" : "right"} />
        <span className="delivery-job__role">
          {stageLabel(job.role)} {byCommand(job) && <code>{job.prompt}</code>}{" "}
          <small>· intento {job.attempt}</small>
        </span>
        <span className="delivery-job__agent">
          {agentName(job.agent)}
          {job.agent === "shell" ? "" : ` · ${runtimeLabel(job)}`}
          {scope}
        </span>
        <span className={`delivery-outcome delivery-tone--${outcome.tone}`}>
          <StatusMark icon={TONE_ICON[outcome.tone]} />
          {outcome.label}
        </span>
        <span className="delivery-job__changes">
          {job.changes.length > 0 ? files(job.changes.length) : ""}
        </span>
        <span className="delivery-job__time">
          {duration(job, now)}
          {job.status === "started" && (
            <small className="delivery-muted"> de {job.timeout_minutes} min</small>
          )}
        </span>
      </button>
      {job.status === "started" && (job.current_command || job.activity) && (
        <p className="delivery-job__activity" aria-label={`Actividad de ${stageLabel(job.role)}`}>
          {job.current_command ? (
            <>
              <small>ejecutando</small>{" "}
              <span title={job.current_command}>{job.current_command}</span>{" "}
              <small className="delivery-muted">
                {lasted(now - (job.current_command_at_ms ?? now))}
              </small>
            </>
          ) : (
            <>
              <span>{job.activity}</span>
              <small className={quiet ? "delivery-tone--attention" : "delivery-muted"}>
                {quiet ?? ago(job.activity_at_ms, now)}
              </small>
            </>
          )}
        </p>
      )}
      {job.status === "queued" && wait && (
        <p className="delivery-job__activity" aria-label={`Por qué espera ${stageLabel(job.role)}`}>
          <span>{wait.reason}</span>{" "}
          <small className="delivery-muted">en cola {ago(wait.since_ms, now)}</small>
          {wait.warning && (
            <>
              {" "}
              <small className="delivery-tone--attention">· {wait.warning}</small>
            </>
          )}
        </p>
      )}
      {job.timing && (job.timing.model_ms !== null || job.timing.commands_ms > 0) && (
        <p
          className="delivery-job__timing"
          aria-label={`Tiempo de ${stageLabel(job.role)}`}
          title={timingLabel(job.timing)}
        >
          {timingLabel(job.timing)}
        </p>
      )}
      {expanded && (
        <div className="delivery-job__detail">
          {job.error && <p className="delivery-job__error">{job.error}</p>}
          {job.status === "queued" && job.waits_environment && (
            <p className="delivery-job__note">
              Espera al entorno; Tinto lo comprueba cada minuto y la lanza cuando responda:{" "}
              {job.waits_environment}
            </p>
          )}
          {job.result_note && <p className="delivery-job__note">{job.result_note}</p>}
          {job.resolved_note && (
            <p className="delivery-job__note">
              {job.result?.status === "findings"
                ? "Hallazgos aceptados sin corregirlos: "
                : "Dada por resuelta sin repetirla: "}
              {job.resolved_note}
            </p>
          )}
          {job.substituted && <p className="delivery-muted">{job.substituted}</p>}
          {job.retry_of && (
            <p className="delivery-muted">
              Reintento automático: Tinto se cerró mientras corría el intento anterior.
            </p>
          )}
          {job.visual_record_error && (
            <p className="delivery-job__note">
              No se pudo generar el registro visual: {job.visual_record_error}
            </p>
          )}
          {isSuperseded(job, jobs) && (
            <p className="delivery-job__note">
              Una etapa anterior cambió el código después de este resultado, así que ya no cuenta:
              hay que repetir esta etapa.
            </p>
          )}
          {job.result && (
            <div className="delivery-result">
              <p>{job.result.summary}</p>
              {job.result.findings.length > 0 && (
                <>
                  <h4>Hallazgos</h4>
                  <ul>
                    {job.result.findings.map((finding, index) => (
                      <li key={index}>{finding}</li>
                    ))}
                  </ul>
                </>
              )}
              {job.result.checks.length > 0 && (
                <>
                  <h4>Comprobaciones</h4>
                  <ul>
                    {job.result.checks.map((check, index) => (
                      <li key={index}>
                        <code>{check.command}</code> — {check.result}
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {job.result.handoff && (
                <>
                  <h4>Para la siguiente etapa</h4>
                  <p>{job.result.handoff}</p>
                </>
              )}
            </div>
          )}
          {(job.plan?.length ?? 0) > 0 && (
            <>
              <h4 className="delivery-job__subtitle">Plan del agente</h4>
              <ul className="delivery-plan" aria-label="Plan del agente">
                {job.plan?.map((item, index) => (
                  <li key={index} data-status={item.status}>
                    <span aria-hidden>
                      {item.status === "completed"
                        ? "✓"
                        : item.status === "in_progress"
                          ? "▸"
                          : "○"}
                    </span>
                    {item.text}
                  </li>
                ))}
              </ul>
            </>
          )}
          {job.usage && <p className="delivery-job__usage">{usageLabel(job.usage)}</p>}
          {job.changes.length > 0 && (
            <>
              <h4 className="delivery-job__subtitle">
                Archivos{job.undone_at_ms ? " (deshechos)" : ""}
              </h4>
              <ul className="delivery-changes" aria-label="Archivos cambiados">
                {job.changes.map((change) => (
                  <li key={change.path}>
                    <span
                      className={`agent-panel__chat-turn-file-kind`}
                      data-change-kind={change.kind}
                      aria-hidden
                    >
                      {changeSymbol(change.kind)}
                    </span>
                    <code>{change.path}</code>
                  </li>
                ))}
              </ul>
            </>
          )}
          <JobLog key={job.id} job={job} />
          <details className="delivery-job__fold">
            <summary>Instrucciones</summary>
            <pre>{job.prompt}</pre>
          </details>
          {(job.task_checks?.length ?? 0) > 0 && (
            <details className="delivery-job__fold">
              <summary>Comprobaciones de Tinto al empezar</summary>
              <pre>{job.task_checks?.join("\n")}</pre>
            </details>
          )}
          <details className="delivery-job__fold">
            <summary>Detalles técnicos</summary>
            <dl className="delivery-job__facts">
              {(job.start_candidate || job.end_candidate) && (
                <>
                  <dt>Instantánea</dt>
                  <dd>
                    <code>
                      {shortCommit(job.start_candidate?.split(":")[1])}
                      {job.end_candidate
                        ? ` → ${shortCommit(job.end_candidate.split(":")[1])}`
                        : ""}
                    </code>
                  </dd>
                </>
              )}
              <dt>Contrato</dt>
              <dd>v{job.contract_version}</dd>
              <dt>Tiempo máximo</dt>
              <dd>{job.timeout_minutes} min</dd>
              {job.exit_code !== null && (
                <>
                  <dt>Código de salida</dt>
                  <dd>{job.exit_code}</dd>
                </>
              )}
              {job.provider_session_id && (
                <>
                  <dt>Sesión</dt>
                  <dd>
                    <code>{job.provider_session_id}</code>
                  </dd>
                </>
              )}
            </dl>
          </details>
          <ActionGroup>
            <div className="delivery-actions">
              {job.visual_record && (
                <ActionButton
                  className="delivery-button--primary"
                  title="Los cambios visuales que revisó la QA, con sus capturas de escritorio y móvil"
                  onAction={() => openDeliveryVisualRecord(job.id)}
                >
                  Abrir registro visual
                </ActionButton>
              )}
              {job.status === "started" && job.agent !== "shell" && (
                <ActionButton
                  title="Detiene el agente; al reanudar sigue en la misma sesión, con lo que ya hizo"
                  onAction={() => pauseDeliveryJob(job.id)}
                >
                  Pausar
                </ActionButton>
              )}
              {held && (
                <ActionButton
                  className="delivery-button--primary"
                  onAction={() => resumeDeliveryJob(job.id)}
                >
                  Reanudar
                </ActionButton>
              )}
              {(isOpen(job) || held) && (
                <ActionButton onAction={() => cancelDeliveryJob(job.id)}>Cancelar</ActionButton>
              )}
              {!isOpen(job) && job.status !== "paused" && job.agent !== "shell" && (
                <ActionButton onAction={() => retryDeliveryJob(job.id)}>Nuevo intento</ActionButton>
              )}
              {undoable && <ActionButton onAction={onUndo}>Deshacer estos cambios</ActionButton>}
              {!isOpen(job) && !held && job.agent !== "shell" && job.provider_session_id && (
                <ActionButton onAction={onContinue}>Continuar en Agents</ActionButton>
              )}
            </div>
          </ActionGroup>
        </div>
      )}
    </li>
  );
}

function JobLog({ job }: { job: DeliveryJob }) {
  const [log, setLog] = useState<DeliveryJobLog | null>(null);
  const live = isOpen(job);
  const nextLine = useRef(0);
  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const chunk = await getDeliveryJobLog(job.id, nextLine.current);
        if (!active) return;
        nextLine.current = chunk.next_line;
        setLog((previous) => ({
          entries: [...(previous?.entries ?? []), ...chunk.entries],
          next_line: chunk.next_line,
          stderr_tail: chunk.stderr_tail,
        }));
      } catch {
        // The log appears once the job has started.
      }
    };
    void load();
    const timer = live ? setInterval(() => void load(), 1500) : null;
    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  }, [job.id, live]);
  if (!log || (log.entries.length === 0 && !log.stderr_tail.trim())) return null;
  return (
    <details className="delivery-log" open={live}>
      <summary>Registro</summary>
      <ol>
        {log.entries.map((entry, index) => (
          <li key={index} className={`delivery-log__entry delivery-log__entry--${entry.kind}`}>
            {entry.text}
          </li>
        ))}
      </ol>
      {log.stderr_tail.trim() && !live && job.status !== "finished" && (
        <pre className="delivery-log__stderr">{log.stderr_tail.trim()}</pre>
      )}
    </details>
  );
}
