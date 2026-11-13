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
  useSyncExternalStore,
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
import { Icon as AppIcon, type IconName } from "../../workbench/Icon";
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
  regenerateDeliveryVisualRecord,
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
  approveDeliveryReplies,
  changeDeliveryPrReviewers,
  decideDeliveryApproval,
  dispatchDeliveryJob,
  findDeliveryPr,
  getDeliveryMergeMethod,
  getDeliveryPrReviewers,
  getDeliveryQaScreenshots,
  getDeliveryOverview,
  getOpencodeModels,
  getDeliveryRepoSettings,
  getDeliveryReviewRequests,
  REVIEW_REQUESTS_EVERY_MS,
  onDeliveryChanged,
  openDeliveryJobInAgents,
  openDeliveryTaskInAgents,
  releaseDeliveryLease,
  removeDeliveryTask,
  requestDeliveryApproval,
  resolveDeliveryJob,
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
  DeliveryQaScreenshot,
  DeliveryReviewRequest,
  DeliveryReplyImage,
  DeliveryFixCommit,
  DeliveryEnvironmentCheck,
  DeliveryJob,
  DeliveryJobVerdict,
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
import { QUOTA_STOP_PERCENT, UNLIMITED_CAPACITY } from "../../delivery/types";
import {
  DispatchDialog,
  ArchivedTasksDialog,
  BatchList,
  CorrectResultDialog,
  NewTaskDialog,
  ReleaseLeaseDialog,
  SettingsPage,
  StageAgentRow,
  type CorrectableStep,
  type RepoChoice,
} from "./DeliveryDialogs";
import { CommitPlanForm, CommitPlanView } from "./DeliveryCommitPlan";
import { JiraIssue } from "./DeliveryIssue";
import { QaProblems } from "./DeliveryQaProblems";
import { AddBlocker, BlockedBar, BlockerActions, BlockerList } from "./DeliveryBlockers";
import { CoordinatorDock } from "./DeliveryCoordinator";
import { Overlay } from "./DeliveryOverlay";
import { useOneOverlay } from "./useOneOverlay";
import {
  coordinatorStalls,
  guideProposals,
  repoKey,
  stallKey,
  type CoordinatorStall,
  type GuideProposalWait,
} from "./DeliveryNotifications";
import { CoordinatorDecisions, DecisionLog, PendingDecisions } from "./DeliveryDecisions";
import { PendingSettingsProposals } from "./DeliverySettingsProposals";
import { PrSection } from "./DeliveryPr";
import { GitHubLog, MergeChecks, MergeFailed, MergeRunning } from "./DeliveryMerge";
import {
  STEPS,
  arrivedInAttention,
  attemptOutcome,
  batchName,
  byCommand,
  costOf,
  dollars,
  isOpen,
  jobsOf,
  lastHandoff,
  latestApproval,
  latestOfRole,
  SCOPE_ROLE,
  lower,
  nextStageOf,
  phaseOf,
  latestStep,
  nextStepOf,
  openBlockers,
  parseStatuses,
  pausedJob,
  pendingSettingsProposals,
  prMood,
  prSummary,
  ciLine,
  waitsOnLabel,
  waitsOnPhrase,
  quietFor,
  stepKey,
  taskLabel,
  entryTasks,
  issueRowId,
  packageState,
  splitIssueOf,
  taskListGroups,
  type TaskListEntry,
  stepName,
  stepOf,
  taskAsksOf,
  brokenJobOf,
  approvalSummary,
  approveLabel,
  approvesInOneClick,
  files,
  type DeliveryStep,
  jobBrokePhrase,
  nowLine,
  roundOf,
  reviewerBlock,
  stageLabel,
  taskStatus,
  taskTrack,
  type TaskTrack,
  type Station,
  qaProblemsToDecide as problemsToDecide,
  verdictsOf,
  type ListGroup,
  listGroup,
  searchTasks,
  type StatusGroup,
  type StatusIcon,
  type TaskStatus,
  type Tone,
} from "./taskStatus";
import "./delivery.css";
import { CodeThreads } from "./DeliveryCodeThread";
import { MAX_LOG_ENTRIES, useJobLog } from "./useJobLog";
import { ScopeSection } from "./DeliveryScope";
import { ExternalLink } from "./ExternalLink";
import { ActionButton, ActionGroup } from "./ActionButton";
import { DeliveryQueue } from "./DeliveryQueue";
import { deliveryQueue, type BatchWaits, type QueueItem } from "./queueItems";
import { retryJobOnce } from "./retryJobOnce";
import { DeliveryRefreshContext } from "./deliveryRefresh";
import { GrowingTextarea } from "./GrowingTextarea";
import { rememberOpencodeModels } from "./agentChoice";
import {
  CHECKS_ROW,
  checksOutcome,
  spanLabel,
  taskHistory,
  whenLabel,
  type HistoryEntry,
  type HistoryRow,
} from "./taskHistory";

type Dialog =
  | { kind: "new-task" }
  | { kind: "dispatch"; task: DeliveryTask; role: string; prompt: string; source: string | null }
  | { kind: "release"; name: string; note: string | null }
  | { kind: "archived" }
  | null;

/** What the task list shows: one batch (null: all of them), the children
 *  of one Jira parent (null: all tasks), the groups folded to their count,
 *  and the split issues opened to their packages. Delivered tasks and split
 *  issues start folded. */
interface ListView {
  lot: string | null;
  parent: string | null;
  folded: ListGroup[];
  openIssues: string[];
}

const LIST_VIEW_KEY = "tinto:delivery-list-view";

function savedListView(): ListView {
  try {
    const saved = JSON.parse(localStorage.getItem(LIST_VIEW_KEY) ?? "null") as ListView | null;
    if (saved)
      return { ...saved, parent: saved.parent ?? null, openIssues: saved.openIssues ?? [] };
  } catch {
    // Without storage the list opens as it does the first time.
  }
  return { lot: null, parent: null, folded: ["delivered"], openIssues: [] };
}

/** The app's icons, sized as Delivery draws them. */
function Icon({ name }: { name: IconName }) {
  return <AppIcon name={name} className="delivery-icon" />;
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

/** The task's state; `who` it waits on follows it, unless the state already
 *  asks the user or an agent is at work. `oneLine` (a row not selected): who
 *  it waits on follows the state in the same line, cut to fit. */
function StatusLine({
  status,
  who,
  oneLine = false,
}: {
  status: TaskStatus;
  who?: string | null;
  oneLine?: boolean;
}) {
  const shown =
    status.group !== "attention" && status.group !== "running" && who
      ? waitsOnPhrase(who, status.label)
      : null;
  // Amber says the task waits on the user; a PR in trouble the coordinator handles does not.
  const tone =
    status.tone === "attention" && status.group !== "attention" ? "waiting" : status.tone;
  // How long it has run ("1:04"), one of the label's parts, closes the line.
  const parts = status.label.split(" · ");
  const at = parts.findIndex((part) => /^\d+:\d{2}$/.test(part));
  const label = at < 0 ? status.label : parts.filter((_, index) => index !== at).join(" · ");
  const whole = shown ? `${label} · ${shown}` : label;
  return (
    <span
      className={`delivery-status-line delivery-tone--${tone}${oneLine ? " delivery-status-line--one" : ""}`}
      title={whole}
    >
      <StatusMark icon={status.icon} />
      <span className="delivery-status-line__text">{oneLine ? whole : label}</span>
      {!oneLine && shown && <small className="delivery-status-line__who">{shown}</small>}
      {at >= 0 && <span className="delivery-status-line__time">{parts[at]}</span>}
    </span>
  );
}

/** Arrow keys move between a menu's entries, round from the last to the first. */
function moveAmongEntries(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  const entries = [
    ...event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)'),
  ];
  const at = entries.indexOf(document.activeElement as HTMLElement);
  const step = event.key === "ArrowDown" ? 1 : -1;
  entries[(at + step + entries.length) % entries.length]?.focus();
}

/** A button that opens a short list of actions in an overlay, pressed while
 *  it is open. */
function Menu({
  label,
  trigger,
  disabled,
  className,
  listClassName,
  children,
}: {
  label?: string;
  trigger: ReactNode;
  disabled?: boolean;
  className?: string;
  listClassName?: string;
  children: (close: () => void) => ReactNode;
}) {
  // The button it opened from, while it is open.
  const [from, setFrom] = useState<HTMLButtonElement | null>(null);
  const close = () => setFrom(null);
  return (
    <span className="delivery-menu">
      <button
        type="button"
        className={className}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={from !== null}
        disabled={disabled}
        onClick={(event) => setFrom(from ? null : event.currentTarget)}
      >
        {trigger}
      </button>
      {from && (
        <Overlay
          anchor={from}
          role="menu"
          className={`delivery-menu__list${listClassName ? ` ${listClassName}` : ""}`}
          onClose={close}
          onKeyDown={moveAmongEntries}
        >
          {children(close)}
        </Overlay>
      )}
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
  useOneOverlay(listRef, null, onClose);
  useEffect(() => {
    window.addEventListener("blur", onClose);
    return () => window.removeEventListener("blur", onClose);
  }, [onClose]);
  const move = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      onClose();
      return;
    }
    moveAmongEntries(event);
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
  return clockSpan((job.ended_at_ms ?? now) - job.started_at_ms);
}

/** "36 s", "4 min 42 s": how long a stage ran. */
function clockSpan(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
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

/** QA's resources: `qa`, or one per environment (`qa:browser:agentos-dev`,
 *  `qa:cli:qa580`). */
function isQaLease(name: string): boolean {
  return name === "qa" || name.startsWith("qa:");
}

/** The environment a QA resource names, for its chip: " (agentos-dev)". */
function qaPlace(name: string): string {
  const place = name.split(":").slice(2).join(":");
  return place ? ` (${place})` : "";
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
    timing.prepare_ms == null ? null : `Preparación del entorno ${lasted(timing.prepare_ms)}`,
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

/** Why a task's or batch's cost reads "≥": some of its agents did not say theirs. */
const COST_PARTIAL = "Suma de los agentes que informan su costo; algunos no lo informan";

/** What a job used: tokens in, read from cache and out, and the cost when known. */
function usageLabel(usage: DeliveryUsage): string {
  const cost = usage.cost_micro_usd === null ? "" : ` · ${dollars(usage.cost_micro_usd)}`;
  return `Consumo: ${tokens(usage.input_tokens)} de entrada · ${tokens(usage.cached_tokens)} en caché · ${tokens(usage.output_tokens)} de salida${cost}`;
}

function changeSymbol(kind: string): string {
  if (kind === "created") return "+";
  if (kind === "removed") return "×";
  return "−";
}

function agentName(agent: string): string {
  return agent === "codex"
    ? "Codex"
    : agent === "claude"
      ? "Claude Code"
      : agent === "muse"
        ? "Muse Code"
        : agent === "opencode"
          ? "OpenCode"
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
  /** Coordinators that stopped with changes unread, and the stops the user hid. */
  stalls: CoordinatorStall[];
  hiddenStalls: string[];
  hideStall: (stall: CoordinatorStall) => void;
  /** The repos with guide rules proposed and not decided. */
  guides: GuideProposalWait[];
  /** What the user chose from «te necesita», until the view that holds it opens it. */
  asked: Asked | null;
  settleAsked: () => void;
  /** Goes to what waits on the user, in this window's view that holds it. */
  goToAsk: (what: Asked) => void;
  /** The PRs that wait for the user's review, in any repo. */
  reviewRequests: DeliveryReviewRequest[];
}

/** One thing that waits on the user, as «te necesita» goes to it. */
type Asked =
  /** `ask`: one of the task's asks (its card's id suffix); without it, the first. */
  | { kind: "task"; taskId: string; ask?: string }
  | { kind: "stall"; runId: string }
  | { kind: "guides"; repo: string }
  /** A queue row answered in place (a paused batch): its control. */
  | { kind: "queue"; repo: string; key: string }
  /** The header's gear: the settings page, on its Delivery tab. */
  | { kind: "delivery-settings"; repo: string };

/** A repo's queue, its tasks in the list's order: the one list of what
 *  waits on the user there, which its tab and the header count. */
function repoQueue(
  overview: DeliveryOverview,
  repo: string,
  now: number,
  reviews: DeliveryReviewRequest[],
  waits: Omit<BatchWaits, "repo">,
  edited: (approvalId: string) => boolean,
): QueueItem[] {
  const key = repoKey(repo);
  const tasks = overview.tasks.filter((task) => repoKey(task.repo) === key);
  const groups = new Map(tasks.map((task) => [task.id, taskStatus(task, overview, now).group]));
  const ordered = entryTasks(
    taskListGroups(tasks, (task) => groups.get(task.id) ?? "idle").flatMap(
      (group) => group.entries,
    ),
  );
  return deliveryQueue(overview, ordered, reviews, now, edited, { repo, ...waits });
}

/** Where a queue item is answered, as «te necesita» goes to it. */
function queueAsk(repo: string, item: QueueItem): Asked {
  switch (item.batch?.kind) {
    case "stall":
      return { kind: "stall", runId: item.batch.runId };
    case "guides":
      return { kind: "guides", repo: item.batch.repo };
    case "paused":
      return { kind: "queue", repo, key: item.key };
  }
  return { kind: "task", taskId: item.task!.id, ask: item.ask ?? undefined };
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
  // Why the overview could not be read: the next good read clears it.
  const [readError, setReadError] = useState<string | null>(null);
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

  const [guides, setGuides] = useState<GuideProposalWait[]>([]);
  const [asked, setAsked] = useState<Asked | null>(null);
  const settleAsked = useCallback(() => setAsked(null), []);
  // The overview, then the guides of its batches' repos.
  const show = useCallback((next: DeliveryOverview) => {
    setOverview(next);
    setReadError(null);
    void guideProposals(next).then(setGuides);
  }, []);

  const refresh = useCallback(
    () => getDeliveryOverview().then(show, (cause: unknown) => setReadError(errorText(cause))),
    [show],
  );

  useEffect(() => {
    // The last model catalogs Tinto loaded: Codex jobs that name no model
    // (e.g. from a coordinator) use the account's default, and the models a
    // stage or a coordinator names are checked against them.
    for (const provider of ["codex", "muse"] as const) {
      const catalog = lastRuntimeCatalog(provider);
      if (catalog) tellDelivery(provider, catalog);
    }
    // OpenCode lists its own; without it installed the pickers keep its default.
    getOpencodeModels().then(rememberOpencodeModels, () => undefined);
  }, []);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    let pending: ReturnType<typeof setTimeout> | null = null;
    getDeliveryOverview().then(
      (next) => {
        if (active) show(next);
      },
      (cause: unknown) => {
        if (active) setReadError(errorText(cause));
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
    // Changes arrive as events; this only catches one that got lost.
    const fallback = setInterval(reload, 30_000);
    return () => {
      active = false;
      unlisten?.();
      if (pending) clearTimeout(pending);
      clearInterval(fallback);
    };
  }, [refresh, show]);

  // Every second redraws the whole panel: only while a job's clock runs.
  const ticking =
    overview?.jobs.some((job) => ["queued", "pending", "started"].includes(job.status)) ?? false;
  useEffect(() => {
    const clock = setInterval(
      () => {
        if (!document.hidden) setNow(Date.now());
      },
      ticking ? 1000 : 30_000,
    );
    return () => clearInterval(clock);
  }, [ticking]);

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

  // Delivery's settings open in the repo in front (else the first one), on
  // its settings page's Delivery tab.
  const hasSettingsRepo = Boolean(only) || repos.length > 0;
  const openDeliverySettings = () => {
    const settingsRepo =
      only ??
      (dockRef.current?.activePanel?.params as { repo?: string } | undefined)?.repo ??
      repos[0]?.path;
    if (!settingsRepo) return;
    if (!only) openRepo(settingsRepo);
    setAsked({ kind: "delivery-settings", repo: settingsRepo });
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
  const wideSettings = overview?.settings ?? {
    capacity: 0,
    quota_stop_percent: QUOTA_STOP_PERCENT,
  };
  const capacitySetting = wideSettings.capacity;
  const capacity = overview?.capacity ?? Math.max(1, capacitySetting);
  // QA's resources, one per environment it tests on: the one in quarantine
  // first, else the busy ones.
  const qaLeases = (overview?.leases ?? []).filter((lease) => isQaLease(lease.name));
  const qaActive = qaLeases.filter((lease) => lease.state === "active");
  const qa = qaLeases.find((lease) => lease.state === "quarantined") ?? qaActive[0] ?? qaLeases[0];
  const qaHolder = tasks.find((task) => task.id === qa?.holder_task_id);
  // The QA jobs a quarantine holds back, wherever the scheduler put them,
  // and whether only the user can release it for them.
  const qaBlocked = (overview?.jobs ?? []).filter(
    (job) =>
      job.status === "queued" &&
      job.lease === qa?.name &&
      overview?.phases?.[job.task_id]?.kind === "waits_quarantine",
  );
  const qaWaitsOnUser = qaBlocked.some(
    (job) => overview?.phases?.[job.task_id]?.waits_on === "user",
  );
  const stalls = overview ? coordinatorStalls(overview, now) : [];
  const [hiddenStalls, setHiddenStalls] = useState(savedHiddenStalls);
  // Hidden until the coordinator stops again; keys of stalls gone are dropped.
  const hideStall = (stall: CoordinatorStall) => {
    const live = stalls.map(stallKey);
    const next = [...hiddenStalls.filter((key) => live.includes(key)), stallKey(stall)];
    setHiddenStalls(next);
    try {
      localStorage.setItem(HIDDEN_STALLS_KEY, JSON.stringify(next));
    } catch {
      // Hidden until Tinto closes.
    }
  };
  // Goes to what waits on the user, in the view that holds it; a repo in a
  // window of its own only comes forward. A repo's own window is that view.
  const goTo = (repo: string, what: Asked) => {
    const here = only ? repoKey(only) === repoKey(repo) : !(repoKey(repo) in detached);
    if (here) setAsked(what);
  };
  // The PRs that wait for the user's review, wherever they are: read on
  // opening and every so often, and left out while GitHub cannot be read.
  const [reviewRequests, setReviewRequests] = useState<DeliveryReviewRequest[]>([]);
  useEffect(() => {
    let live = true;
    const read = () =>
      getDeliveryReviewRequests().then(
        (found) => live && setReviewRequests(found),
        () => {},
      );
    void read();
    const clock = setInterval(() => void read(), REVIEW_REQUESTS_EVERY_MS);
    return () => {
      live = false;
      clearInterval(clock);
    };
  }, []);
  // Everything that waits on the user: every repo's queue, one after another,
  // each item going where its queue's "Ver" goes.
  const drafted = useSyncExternalStore(subscribeDrafts, draftedIds);
  const queueRepos = [
    ...new Map(
      [
        ...tasks.map((task) => task.repo),
        ...stalls.map((stall) => stall.run.repo),
        ...guides.map((guide) => guide.repo),
      ].map((repo) => [repoKey(repo), repo] as const),
    ).values(),
  ];
  const asks = overview
    ? queueRepos.flatMap((repo) =>
        repoQueue(overview, repo, now, reviewRequests, { stalls, hiddenStalls, guides }, (id) =>
          drafted.split("\n").includes(id),
        ).map((item) => ({ repo, item })),
      )
    : [];
  const manyRepos = new Set(asks.map((ask) => repoKey(ask.repo))).size > 1;
  const goToItem = (repo: string, item: QueueItem) => {
    if (item.review) {
      void openDeliveryUrl(item.review.url).catch(() => {});
      return;
    }
    if (item.task) reveal(item.task);
    else showRepo(repo);
    goTo(repo, queueAsk(repo, item));
  };
  const needsTrigger = (
    <>
      <Icon name="flag" />
      {asks.length === 1 ? "1 cosa te necesita" : `${asks.length} cosas te necesitan`}
    </>
  );

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
    hiddenStalls,
    hideStall,
    guides,
    asked,
    settleAsked,
    goToAsk: setAsked,
    reviewRequests,
  };

  return (
    <DeliveryRefreshContext.Provider value={refresh}>
      <DeliveryContext.Provider value={shared}>
        <div className="delivery-panel" data-testid="delivery-panel">
          <header className="delivery-panel__header">
            <h1>Delivery</h1>
            {only && <span className="delivery-muted">{busStore.displayName(only)}</span>}
            <span className="delivery-panel__spacer" />
            {reviewRequests.length > 0 && (
              <Menu
                className="delivery-button--quiet"
                listClassName="delivery-needs"
                trigger={
                  <>
                    <Icon name="pull" />
                    {reviewRequests.length} PR por revisar
                  </>
                }
              >
                {(close) =>
                  reviewRequests.map((request) => (
                    <button
                      key={request.url}
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        close();
                        void openDeliveryUrl(request.url).catch(() => {});
                      }}
                    >
                      <span className="delivery-needs__title">{request.title}</span>
                      <small>
                        {request.repo} #{request.number} · {request.author}
                        {request.draft && " · borrador"}
                      </small>
                    </button>
                  ))
                }
              </Menu>
            )}
            {/* One thing waiting: straight to it; several: the list of them. */}
            {asks.length === 1 && (
              <button
                type="button"
                className="delivery-attention"
                onClick={() => goToItem(asks[0].repo, asks[0].item)}
              >
                {needsTrigger}
              </button>
            )}
            {asks.length > 1 && (
              <Menu
                className="delivery-attention"
                listClassName="delivery-needs"
                trigger={needsTrigger}
              >
                {(close) =>
                  asks.map(({ repo, item }) => (
                    <button
                      key={`${repoKey(repo)}:${item.key}`}
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        close();
                        goToItem(repo, item);
                      }}
                    >
                      <span className="delivery-needs__title">{item.title}</span>
                      <small>
                        {item.need}
                        {manyRepos && ` · ${busStore.displayName(repo)}`}
                      </small>
                    </button>
                  ))
                }
              </Menu>
            )}
            {/* Shown while it says something: an idle queue and a free QA do not. */}
            {(running.length > 0 || queued.length > 0) && (
              <span className="delivery-load">
                Agentes
                {capacity === UNLIMITED_CAPACITY ? (
                  <>{running.length} en marcha, sin límite</>
                ) : (
                  <>
                    <span className="delivery-meter" aria-hidden>
                      {Array.from({ length: capacity }, (_, index) => (
                        <i key={index} className={index < running.length ? "on" : undefined} />
                      ))}
                    </span>
                    {running.length} de {capacity}
                  </>
                )}
                {queued.length > 0 && (
                  <span className="delivery-muted">· {queued.length} en cola</span>
                )}
              </span>
            )}
            {qa?.state === "quarantined" ? (
              <button
                type="button"
                className="delivery-lease delivery-lease--quarantined"
                title={qa.note ?? undefined}
                onClick={() => setDialog({ kind: "release", name: qa.name, note: qa.note })}
              >
                QA en cuarentena{qaPlace(qa.name)}
                {qaBlocked.length > 0 && (
                  <span className="delivery-muted">· {qaBlocked.length} esperando</span>
                )}
              </button>
            ) : (
              (qaActive.length > 0 ? qaActive : qa && qa.queue.length > 0 ? [qa] : []).map(
                (lease) => {
                  const holder = tasks.find((task) => task.id === lease.holder_task_id);
                  return (
                    <span
                      key={lease.name}
                      className={`delivery-lease delivery-lease--${lease.state}`}
                      title={
                        lease.state === "active" && holder
                          ? `En QA${qaPlace(lease.name)}: ${taskLabel(holder)} · ${holder.title}`
                          : (lease.note ?? undefined)
                      }
                    >
                      {lease.state === "active" && <Icon name="photo" />}
                      {lease.state === "active"
                        ? `En QA${qaPlace(lease.name)}: ${holder ? taskLabel(holder) : "ocupado"}`
                        : "QA libre"}
                      {lease.queue.length > 0 && (
                        <span className="delivery-muted">· {lease.queue.length} esperando</span>
                      )}
                    </span>
                  );
                },
              )
            )}
            <button
              type="button"
              className="delivery-button--quiet"
              onClick={() => setDialog({ kind: "archived" })}
            >
              Archivadas…
            </button>
            <button
              type="button"
              className="delivery-icon-button delivery-button--quiet"
              aria-label="Ajustes de Delivery"
              title="Agentes en paralelo y conexión de coordinadores"
              disabled={!hasSettingsRepo}
              onClick={openDeliverySettings}
            >
              <Icon name="gear" />
            </button>
            <button
              type="button"
              className={asks.length === 0 ? "delivery-button--primary" : undefined}
              disabled={repos.length === 0}
              onClick={() => setDialog({ kind: "new-task" })}
            >
              Nueva tarea
            </button>
          </header>

          {(error ?? readError) && (
            <div className="delivery-panel__error" role="alert">
              <span>{error ?? readError}</span>
              <button
                type="button"
                aria-label="Descartar error"
                onClick={() => {
                  setError(null);
                  setReadError(null);
                }}
              >
                ×
              </button>
            </div>
          )}

          {/* Only when it holds work back; otherwise the header and the task say it. */}
          {qa?.state === "quarantined" && qaBlocked.length > 0 && (
            <div className="delivery-panel__warning" role="status">
              <span title={qa.note ?? undefined}>
                <strong>QA bloqueada:</strong>{" "}
                {qaBlocked.length === 1 ? "una QA espera" : `${qaBlocked.length} QA esperan`} a que
                {qaWaitsOnUser ? " liberes" : " el coordinador libere"} el recurso QA
                {qaHolder ? `, en cuarentena tras ${qaHolder.key}` : ""}.
                {qa.note ? ` ${qa.note}` : ""}
              </span>
              <button
                type="button"
                onClick={() => setDialog({ kind: "release", name: qa.name, note: qa.note })}
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
                leftHeaderActionsComponent={DeliveryAddRepo}
                dndStrategy="pointer"
                onReady={onDockReady}
              />
            </div>
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
  const { overview, now, stalls, hiddenStalls, guides, reviewRequests } = useDelivery();
  const tasks = (overview?.tasks ?? []).filter(
    (task) => repoKey(task.repo) === repoKey(params.repo),
  );
  // Its queue holds something: what the header counts for it.
  const needs =
    overview !== null &&
    repoQueue(
      overview,
      params.repo,
      now,
      reviewRequests,
      { stalls, hiddenStalls, guides },
      () => false,
    ).length > 0;
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

/** "+" beside the tabs: a repo of the workbench without its tab. */
function DeliveryAddRepo({ containerApi }: IDockviewHeaderActionsProps) {
  const { repos, openRepo } = useDelivery();
  const missing = repos.filter((repo) => !containerApi.getPanel(repoPanelId(repo.path)));
  if (missing.length === 0) return null;
  return (
    <Menu
      label="Abrir un repositorio"
      className="delivery-button--quiet delivery-add-repo"
      trigger={<Icon name="plus" />}
    >
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
            (item) =>
              `${item.ok ? "Responde" : "Falla"} · ${item.name}${item.output ? `\n${item.output}` : ""}`,
          )
          .join("\n\n");
  // With nothing to check there is nothing to say.
  if (!checking && !failure && checks?.length === 0) return null;
  return (
    <button
      type="button"
      className={`delivery-button--quiet delivery-env${tone ? ` delivery-env--${tone}` : ""}`}
      title={`${details}\n\nClic para comprobar de nuevo.`.trim()}
      aria-label={`Entorno: ${state}. Comprobar de nuevo`}
      disabled={checking}
      onClick={check}
    >
      <span className="delivery-dot" />
      <span>
        Entorno<span className="delivery-env__state">: {state}</span>
      </span>
    </button>
  );
}

/** One repo's delivery: its batches, its tasks and its settings. */
function DeliveryRepoView({ repo }: { repo: string }) {
  const shared = useDelivery();
  const { overview, now, repos, listView, setListView, selected, select, reveal, run } = shared;
  // Settings in place of the tasks, opened on a section or at the top;
  // "proposal": the guides, on the rules proposed; "delivery": its Delivery tab.
  const [settingsAt, setSettingsAt] = useState<string | null>(null);
  const settingsOpen = settingsAt !== null;
  const guidesOpen = settingsAt === "guides" || settingsAt === "proposal";
  const key = repoKey(repo);
  const { asked, settleAsked } = shared;
  const guidesWait = shared.guides.some((guide) => repoKey(guide.repo) === key);
  // «te necesita» chose this repo's proposed rules: the guides open on them.
  // The header's gear: the settings, on the Delivery tab.
  const askedGuides =
    (asked?.kind === "guides" || asked?.kind === "delivery-settings") && repoKey(asked.repo) === key
      ? asked
      : null;
  const [seenAsk, setSeenAsk] = useState<Asked | null>(null);
  if (askedGuides && askedGuides !== seenAsk) {
    setSeenAsk(askedGuides);
    setSettingsAt(askedGuides.kind === "guides" ? "proposal" : "delivery");
  }
  // Or one of its stopped coordinators: its sheet opens on that batch, with
  // what to tell it.
  const askedStall =
    asked?.kind === "stall" &&
    shared.stalls.some((stall) => stall.run.id === asked.runId && repoKey(stall.run.repo) === key)
      ? asked
      : null;
  const [coordinatorOpen, setCoordinatorOpen] = useState(false);
  // The request that opened the sheet on a stopped batch: each one, even for
  // the same batch, shows that batch over one picked in the sheet since.
  const [coordinatorAsk, setCoordinatorAsk] = useState<Extract<Asked, { kind: "stall" }> | null>(
    null,
  );
  if (askedStall && askedStall !== seenAsk) {
    setSeenAsk(askedStall);
    setCoordinatorAsk(askedStall);
    setCoordinatorOpen(true);
  }
  // Or a row of its queue answered in place: the queue shows again (settings
  // closed), and its control gets the focus once drawn.
  const askedRow = asked?.kind === "queue" && repoKey(asked.repo) === key ? asked : null;
  if (askedRow && askedRow !== seenAsk) {
    setSeenAsk(askedRow);
    setSettingsAt(null);
  }
  useEffect(() => {
    if (askedGuides || askedStall) settleAsked();
  }, [askedGuides, askedStall, settleAsked]);
  const choice = repos.find((candidate) => repoKey(candidate.path) === key) ?? {
    path: repo,
    distro: null,
    label: busStore.displayName(repo),
  };
  const tasks = useMemo(
    () => (overview?.tasks ?? []).filter((task) => repoKey(task.repo) === key),
    [overview, key],
  );
  const statuses = useMemo(
    () =>
      new Map(
        overview ? tasks.map((task) => [task.id, taskStatus(task, overview, now)] as const) : [],
      ),
    [overview, tasks, now],
  );
  // Each row ends with its stages as a small line: read once per overview,
  // not on every tick of the clock.
  const tracks = useMemo(
    () =>
      new Map(overview ? tasks.map((task) => [task.id, taskTrack(overview, task)] as const) : []),
    [overview, tasks],
  );
  // The queue's rows of a task that just came to need the user, tinted once:
  // never on the first load, a filter or an unfolding, only on a task that
  // changed group.
  const lastGroups = useRef(new Map<string, StatusGroup>());
  const [arrived, setArrived] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    const groups = new Map([...statuses].map(([id, status]) => [id, status.group]));
    const fresh = arrivedInAttention(lastGroups.current, groups);
    lastGroups.current = groups;
    if (fresh.length > 0) setArrived((current) => new Set([...current, ...fresh]));
  }, [statuses]);
  const activeRuns = (overview?.runs ?? []).filter(
    (candidate) => candidate.status === "active" && repoKey(candidate.repo) === key,
  );
  // The repo's stopped coordinators the user has not hidden.
  const repoStalls = shared.stalls.filter(
    (stall) => repoKey(stall.run.repo) === key && !shared.hiddenStalls.includes(stallKey(stall)),
  );
  // A task row's right-click menu, drawn by the task it selects.
  const [contextAt, setContextAt] = useState<(ContextPoint & { taskId: string }) | null>(null);
  const closeContext = useCallback(() => setContextAt(null), []);
  // A batch that closed since, or the only one left, shows every task.
  const lot =
    activeRuns.length > 1 && activeRuns.some((candidate) => candidate.id === listView.lot)
      ? listView.lot
      : null;
  // The Jira parents of the repo's tasks, and the one the list is narrowed to.
  const parents = [
    ...new Map(
      tasks.flatMap((task) => (task.parent ? [[task.parent.key, task.parent] as const] : [])),
    ).values(),
  ];
  const parent = parents.find((candidate) => candidate.key === listView.parent) ?? null;
  const listed = tasks.filter(
    (task) => (!lot || task.run_id === lot) && (!parent || task.parent?.key === parent.key),
  );
  const chooseParent = (next: string | null) => setListView((view) => ({ ...view, parent: next }));
  const isFolded = (group: ListGroup) => listView.folded.includes(group);
  const needsYou = (task: DeliveryTask) => statuses.get(task.id)?.group === "attention";
  const issueFolded = (entry: Extract<TaskListEntry, { kind: "issue" }>) =>
    !listView.openIssues.includes(entry.id) && !entry.packages.some(needsYou);
  // A split issue is one row, with its packages nested under it in order.
  const groups = taskListGroups(listed, (task) => statuses.get(task.id)?.group ?? "idle").map(
    (group) => ({ ...group, tasks: entryTasks(group.entries) }),
  );
  // What needs the user in the whole repo, whatever the list is narrowed to,
  // in the order its tasks have in the list.
  const drafted = useSyncExternalStore(subscribeDrafts, draftedIds);
  const queue = overview
    ? repoQueue(
        overview,
        repo,
        now,
        shared.reviewRequests,
        { stalls: shared.stalls, hiddenStalls: shared.hiddenStalls, guides: shared.guides },
        (id) => drafted.split("\n").includes(id),
      )
    : [];
  // The asked row's control, focused once drawn; a row gone meanwhile (the
  // batch resumed elsewhere) settles the request too.
  const queueKeys = queue.map((item) => item.key).join("\n");
  useEffect(() => {
    if (!askedRow) return;
    if (!queueKeys.split("\n").includes(askedRow.key)) {
      settleAsked();
      return;
    }
    const row = document.querySelector(`[data-queue-key="${CSS.escape(askedRow.key)}"]`);
    const control = row?.querySelector<HTMLElement>(".delivery-queue__control");
    if (!control) return;
    row?.scrollIntoView?.({ block: "nearest" });
    control.focus();
    settleAsked();
  }, [askedRow, queueKeys, settingsAt, settleAsked]);
  // "Ver": the task, on what it asks; a batch's item, where it is answered.
  const showAsk = (item: QueueItem) => {
    if (item.task) reveal(item.task);
    shared.goToAsk(queueAsk(repo, item));
  };
  const ordered = groups.flatMap((group) => (isFolded(group.group) ? [] : group.tasks));
  // The search finds a task by its issue or its PR among all the repo's
  // tasks, whatever the list is narrowed to or has folded.
  const [query, setQuery] = useState("");
  const found = searchTasks(tasks, overview?.prs ?? [], query);
  // The row the list scrolls to once it shows it.
  const [scrollTo, setScrollTo] = useState<string | null>(null);
  const listRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!scrollTo) return;
    const row = [...(listRef.current?.querySelectorAll<HTMLElement>("[data-task-id]") ?? [])].find(
      (candidate) => candidate.dataset.taskId === scrollTo,
    );
    const list = listRef.current;
    if (!row || !list) return;
    // Only the list scrolls, to leave the row in its middle.
    const [at, within] = [row.getBoundingClientRect(), list.getBoundingClientRect()];
    list.scrollTop += at.top - within.top - (list.clientHeight - at.height) / 2;
    row.focus({ preventScroll: true });
    setScrollTo(null);
    // After the list shows it: the same change unfolds its group.
  }, [scrollTo, listView]);
  // Goes to a found task: the list stops hiding it (its batch or parent
  // filter, its folded group, its folded issue) and scrolls to its row.
  const goToFound = (task: DeliveryTask) => {
    const group = taskListGroups(tasks, (one) => statuses.get(one.id)?.group ?? "idle").find(
      (candidate) => entryTasks(candidate.entries).includes(task),
    )?.group;
    setListView((view) => ({
      ...view,
      lot: view.lot && view.lot !== task.run_id ? null : view.lot,
      parent: view.parent && view.parent !== task.parent?.key ? null : view.parent,
      folded: view.folded.filter((candidate) => candidate !== group),
      openIssues:
        task.package && !view.openIssues.includes(issueRowId(task))
          ? [...view.openIssues, issueRowId(task)]
          : view.openIssues,
    }));
    select(repo, task.id);
    setQuery("");
    setScrollTo(task.id);
  };
  // A split issue's row shows its plan.
  const selectedIssue =
    groups
      .flatMap((group) => group.entries)
      .find(
        (entry): entry is Extract<TaskListEntry, { kind: "issue" }> =>
          entry.kind === "issue" && entry.id === selected[key],
      ) ?? null;
  // With every group folded, the detail still shows a task.
  const selectedTask = selectedIssue
    ? null
    : (tasks.find((task) => task.id === selected[key]) ??
      ordered[0] ??
      groups[0]?.tasks[0] ??
      null);
  const shownId = selectedIssue?.id ?? selectedTask?.id ?? null;
  // Keep showing the same task when its group changes (e.g. after answering
  // its decisions) instead of jumping to whatever is first in the list.
  useEffect(() => {
    if (shownId && shownId !== selected[key]) select(repo, shownId);
  }, [shownId, selected, key, repo, select]);
  const chooseLot = (next: string | null) => {
    setListView((view) => ({ ...view, lot: next }));
    if (next && selectedTask && selectedTask.run_id !== next) select(repo, null);
  };
  // The delivered tasks the list shows go to "Archivadas…", as one by one.
  // One after another, as they remove worktrees and branches of the same
  // repository; one that fails does not stop the rest, and the error names it.
  const archiveDelivered = async (delivered: DeliveryTask[]) => {
    const ok = await confirm(
      `${delivered.length === 1 ? "Se archiva 1 tarea entregada" : `Se archivan ${delivered.length} tareas entregadas`}: se borran sus worktrees y ramas locales, todo está en GitHub. Puedes recuperarlas en "Archivadas…".`,
      { title: "Archivar entregadas", okLabel: "Archivar" },
    );
    if (!ok) return false;
    const failed: string[] = [];
    for (const task of delivered) {
      try {
        await archiveDeliveryTask(task.id);
      } catch (cause) {
        failed.push(`${task.key}: ${errorText(cause)}`);
      }
    }
    if (selectedTask && delivered.includes(selectedTask)) select(repo, null);
    if (failed.length > 0) throw new Error(`No se archivaron ${failed.join("; ")}`);
  };
  const toggleGroup = (group: ListGroup) => {
    const folding = !listView.folded.includes(group);
    setListView((view) => ({
      ...view,
      folded: folding
        ? [...view.folded, group]
        : view.folded.filter((candidate) => candidate !== group),
    }));
    if (
      folding &&
      selectedTask &&
      listGroup(statuses.get(selectedTask.id)?.group ?? "idle") === group
    ) {
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
      <Menu trigger={trigger} listClassName="delivery-batches__list">
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

  // The task's Jira parent; a click narrows the list to its children.
  const parentTag = (task: DeliveryTask) =>
    task.parent && (
      <span
        className="delivery-task-row__parent"
        title={`Mostrar solo las hijas de ${task.parent.key}`}
        onClick={(event) => {
          event.stopPropagation();
          chooseParent(task.parent!.key);
        }}
      >
        ↳ {task.parent.key}
        {task.parent.summary && ` · ${task.parent.summary}`}
      </span>
    );
  // A task's row; a package's, nested under its issue, says only which one it is.
  const taskRow = (task: DeliveryTask) => {
    const pr = (overview?.prs ?? []).find((candidate) => candidate.task_id === task.id);
    const status = statuses.get(task.id)!;
    // The task QA is on now, or the one that left it in quarantine.
    const qaLease = overview?.leases.find(
      (lease) => isQaLease(lease.name) && lease.holder_task_id === task.id,
    );
    const qaHere =
      qaLease?.state === "active" || qaLease?.state === "quarantined" ? qaLease.state : null;
    const track = tracks.get(task.id);
    const selected = selectedTask?.id === task.id;
    return (
      <button
        key={task.id}
        type="button"
        className={`delivery-task-row${task.package ? " delivery-task-row--package" : ""}${qaHere === "active" ? " delivery-task-row--qa" : ""}`}
        data-task-id={task.id}
        aria-current={selected ? "true" : undefined}
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
          if (!(event.key === "ContextMenu" || (event.shiftKey && event.key === "F10"))) return;
          const host = event.currentTarget.closest(".delivery-panel");
          if (!host) return;
          event.preventDefault();
          const row = event.currentTarget.getBoundingClientRect();
          select(repo, task.id);
          setContextAt({ taskId: task.id, host, x: row.left + 24, y: row.bottom });
        }}
      >
        <span className="delivery-task-row__key">
          {task.package ? `Paquete ${task.package.index}/${task.package.of}` : taskLabel(task)}
          {qaHere && (
            <span
              className={`delivery-qa-badge delivery-qa-badge--${qaHere}`}
              title={qaLease?.note ?? undefined}
            >
              {qaHere === "active" ? "En QA" : "QA en cuarentena"}
            </span>
          )}
          {pr && (
            <span className="delivery-task-row__pr" title={`PR #${pr.number}`}>
              #{pr.number}
            </span>
          )}
          {track && <MiniLine track={track} />}
        </span>
        <span className="delivery-task-row__title">
          {task.package ? task.package.title || task.title : task.title || task.branch}
        </span>
        {!task.package && parentTag(task)}
        {/* One status line; the selected row also says why. */}
        <StatusLine
          status={status}
          who={overview ? waitsOnLabel(overview, task.id) : null}
          oneLine={!selected}
        />
        {selected && status.detail && (
          <span className="delivery-task-row__blocker" title={status.detail}>
            {status.detail}
          </span>
        )}
      </button>
    );
  };
  // A split issue's row: what it is and how far its packages got; a click shows its plan.
  // Folded, it also says which package is under way and where it stands.
  const issueRow = (entry: Extract<TaskListEntry, { kind: "issue" }>, folded: boolean) => {
    const first = entry.packages[0];
    const issue = overview ? splitIssueOf(overview, first) : null;
    const of = issue?.of ?? first.package!.of;
    const merged = issue?.packages.filter((pkg) => pkg.merged).length ?? 0;
    const done = (task: DeliveryTask) =>
      issue?.packages.some((pkg) => pkg.task_id === task.id && (pkg.merged || pkg.delivered));
    const current = entry.packages.find((task) => !done(task)) ?? entry.packages.at(-1)!;
    // A hidden package that is selected shows as its issue.
    const holdsSelected = folded && entry.packages.some((task) => task.id === selectedTask?.id);
    return (
      <button
        type="button"
        className="delivery-task-row delivery-task-row--issue"
        aria-current={selectedIssue?.id === entry.id || holdsSelected ? "true" : undefined}
        onClick={() => select(repo, entry.id)}
      >
        <span className="delivery-task-row__key">
          {entry.key}
          <span className="delivery-task-row__pr">
            {merged}/{of} fusionados
          </span>
        </span>
        <span className="delivery-task-row__title">{first.title || `${of} paquetes`}</span>
        {parentTag(first)}
        {folded && (
          <>
            <span className="delivery-task-row__package">
              Paquete {current.package!.index}/{current.package!.of}
              {current.package!.title && ` · ${current.package!.title}`}
            </span>
            <StatusLine
              status={statuses.get(current.id)!}
              who={overview ? waitsOnLabel(overview, current.id) : null}
              oneLine
            />
          </>
        )}
      </button>
    );
  };
  // The bar's batch, when the repo has one, and what acts on it.
  const active = activeRuns.length === 1 ? activeRuns[0] : null;
  const activeCost =
    active &&
    costOf(
      (overview?.jobs ?? []).filter((job) =>
        tasks.some((task) => task.id === job.task_id && task.run_id === active.id),
      ),
    );
  const takeable = !!active?.owner && active.owner !== "user";
  const pauseTitle = active?.paused
    ? "Lo que está en cola vuelve a arrancar"
    : "Lo que está en cola espera; lo que corre sigue (puedes pausar cada etapa)";
  const togglePause = () => setDeliveryRunPaused(active!.id, !active!.paused);
  const handOverItems = (target: DeliveryRun, close: () => void) => {
    const targets = handOverTargets(target);
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
            void run(() => handOverDeliveryRun([target.id], session.id));
          }}
        >
          {agentName(session.agent_type)} · {session.id.slice(0, 8)}
          {session.turn_status === "working" ? " (trabajando)" : ""}
        </button>
      ))
    );
  };
  const toggleGuides = () => setSettingsAt(guidesOpen ? null : "guides");
  // Rules proposed and not decided: marked as a repo's tab marks what needs the user.
  const guidesMark = guidesWait && (
    <span className="delivery-repo-tab__needs" aria-label="reglas propuestas" />
  );
  const toggleSettings = () => setSettingsAt(settingsOpen && !guidesOpen ? null : "");
  // The GitHub log, opened from the bar's menu and shown under it.
  const moreRef = useRef<HTMLSpanElement>(null);
  const [githubLogAt, setGithubLogAt] = useState<Element | null>(null);

  const toggleIssue = (id: string) =>
    setListView((view) => ({
      ...view,
      openIssues: view.openIssues.includes(id)
        ? view.openIssues.filter((candidate) => candidate !== id)
        : [...view.openIssues, id],
    }));

  return (
    <div className="delivery-repo" aria-label={`Delivery de ${choice.label}`}>
      {/* One bar: the batch on the left; on the right one menu with what acts on
          the batch and the repo, the environment's state and the coordinator. */}
      <div className="delivery-repo__bar">
        <div className="delivery-run">
          {active ? (
            <>
              {batches(
                <>
                  <strong>{batchName(active.title || active.id)}</strong>
                  <Icon name="down" />
                </>,
              )}
              <span className="delivery-run__meta">
                <span className="delivery-muted" title={active.repo}>
                  {active.owner === "user"
                    ? "lo diriges tú"
                    : active.owner
                      ? `coordina ${active.owner}`
                      : "sin coordinador"}
                </span>
                {active.notified_at_ms && (
                  <span
                    className="delivery-muted"
                    title="Tinto le escribe en Agents lo que espera de él"
                  >
                    · coordinador avisado {ago(active.notified_at_ms, now)}
                  </span>
                )}
                {activeCost && (
                  <span
                    className="delivery-muted"
                    title={activeCost.partial ? COST_PARTIAL : undefined}
                  >
                    · costo {activeCost.label}
                  </span>
                )}
              </span>
            </>
          ) : (
            <span className="delivery-run__meta">
              {batches(
                <>
                  {activeRuns.length > 1 ? `${activeRuns.length} lotes activos` : "Lotes"}
                  <Icon name="down" />
                </>,
              )}
            </span>
          )}
        </div>
        {/* "Lote" while the bar shows one batch; with none or several (each
            acted on from its list) the menu holds the repo's own entries. */}
        <span ref={moreRef} className="delivery-repo__more">
          <Menu
            className="delivery-button--quiet"
            label={active ? "Acciones del lote" : `Más de ${choice.label}`}
            trigger={
              active ? (
                <>
                  Lote{guidesMark} <Icon name="down" />
                </>
              ) : (
                <>⋯{guidesMark}</>
              )
            }
          >
            {(close) => (
              <>
                {active && (
                  <>
                    {takeable && (
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          close();
                          void run(() => shared.takeover(active));
                        }}
                      >
                        Tomar el control
                      </button>
                    )}
                    <button
                      type="button"
                      role="menuitem"
                      title={pauseTitle}
                      onClick={() => {
                        close();
                        void run(togglePause);
                      }}
                    >
                      {active.paused ? "Reanudar lote" : "Pausar lote"}
                    </button>
                    <hr />
                    <small className="delivery-menu__heading">Cambiar coordinador</small>
                    {handOverItems(active, close)}
                    <hr />
                  </>
                )}
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    close();
                    setGithubLogAt(moreRef.current);
                  }}
                >
                  Registro de GitHub
                </button>
                <button
                  type="button"
                  role="menuitem"
                  title={`Guías de ${choice.label}: reglas que salen de sus revisores${guidesWait ? ". Hay reglas propuestas por aprobar" : ""}`}
                  onClick={() => {
                    close();
                    toggleGuides();
                  }}
                >
                  Guías
                  {guidesMark}
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    close();
                    toggleSettings();
                  }}
                >
                  Ajustes de {choice.label}
                </button>
              </>
            )}
          </Menu>
        </span>
        {githubLogAt && <GitHubLog anchor={githubLogAt} onClose={() => setGithubLogAt(null)} />}
        <EnvironmentStatus repo={choice.path} distro={choice.distro} />
        {overview && activeRuns.length > 0 && (
          <CoordinatorDock
            runs={activeRuns}
            focus={coordinatorAsk?.runId ?? lot}
            request={coordinatorAsk}
            overview={overview}
            stalls={repoStalls}
            onHideStall={shared.hideStall}
            open={coordinatorOpen}
            onOpenChange={(open) => {
              setCoordinatorOpen(open);
              // Closed, it shows the batch the list does again.
              if (!open) setCoordinatorAsk(null);
            }}
          />
        )}
      </div>

      {settingsOpen ? (
        <SettingsPage
          key={settingsAt}
          repos={repos}
          repo={choice}
          section={
            settingsAt === "proposal" || settingsAt === "delivery"
              ? undefined
              : settingsAt || undefined
          }
          focusProposal={settingsAt === "proposal"}
          scope={settingsAt === "delivery" ? "delivery" : undefined}
          onBack={() => setSettingsAt(null)}
          settings={overview?.settings ?? { capacity: 0, quota_stop_percent: QUOTA_STOP_PERCENT }}
          endpoint={overview?.coordinator ?? null}
          onSettings={(value) => void run(() => updateDeliverySettings(value))}
        />
      ) : (
        <div className="delivery-panel__body">
          <nav className="delivery-panel__tasks" aria-label="Tareas de Delivery" ref={listRef}>
            {overview && tasks.length === 0 && (
              <p className="delivery-panel__empty">
                Crea una tarea para trabajar un issue en su propio worktree y rama.
              </p>
            )}
            {tasks.length > 0 && (
              <div className="delivery-search">
                <input
                  type="search"
                  aria-label="Buscar por issue o número de PR"
                  placeholder="Buscar issue o #PR"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && found[0]) goToFound(found[0]);
                    if (event.key === "Escape") setQuery("");
                  }}
                />
                {query.trim() && (
                  <ul className="delivery-search__results" aria-label="Resultados de la búsqueda">
                    {found.length === 0 && (
                      <li className="delivery-muted">Ninguna tarea coincide.</li>
                    )}
                    {found.map((task) => {
                      const pr = (overview?.prs ?? []).find(
                        (candidate) => candidate.task_id === task.id,
                      );
                      return (
                        <li key={task.id}>
                          <button type="button" onClick={() => goToFound(task)}>
                            <span className="delivery-task-row__key">
                              {taskLabel(task)}
                              {task.package &&
                                ` · paquete ${task.package.index}/${task.package.of}`}
                              {pr && (
                                <span className="delivery-task-row__pr">
                                  PR #{pr.number} · {lower(prSummary(pr).label)}
                                </span>
                              )}
                            </span>
                            <span className="delivery-search__title">
                              {task.package?.title || task.title || task.branch}
                            </span>
                            <StatusLine
                              status={statuses.get(task.id)!}
                              who={overview ? waitsOnLabel(overview, task.id) : null}
                            />
                            <span className="delivery-muted">
                              <code>{task.branch}</code>
                              {task.parent && ` · ↳ ${task.parent.key}`}
                            </span>
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </div>
            )}
            {(tasks.length > 0 || queue.length > 0) && (
              <DeliveryQueue
                items={queue}
                arrived={arrived}
                onArrived={(taskId) =>
                  setArrived((current) => new Set([...current].filter((id) => id !== taskId)))
                }
                onShow={showAsk}
              />
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
            {parents.length > 0 && (
              <label className="delivery-lot-filter">
                <span>Padre</span>
                <select
                  value={parent?.key ?? ""}
                  onChange={(event) => chooseParent(event.target.value || null)}
                >
                  <option value="">Todos los padres</option>
                  {parents.map((candidate) => (
                    <option key={candidate.key} value={candidate.key}>
                      {`${candidate.key}${candidate.summary ? ` · ${candidate.summary}` : ""} (${
                        tasks.filter((task) => task.parent?.key === candidate.key).length
                      })`}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {parent && (
              <p className="delivery-parent-progress">
                {(() => {
                  const children = tasks.filter((task) => task.parent?.key === parent.key);
                  const delivered = children.filter(
                    (task) => statuses.get(task.id)?.group === "delivered",
                  ).length;
                  return `${children.length} ${children.length === 1 ? "tarea" : "tareas"} en Delivery · ${delivered} ${delivered === 1 ? "entregada" : "entregadas"}`;
                })()}
              </p>
            )}
            {groups.map((group) => (
              <section key={group.group} className="delivery-task-group" aria-label={group.label}>
                <h2>
                  <button
                    type="button"
                    aria-expanded={!isFolded(group.group)}
                    onClick={() => toggleGroup(group.group)}
                  >
                    <Icon name={isFolded(group.group) ? "right" : "down"} />
                    {group.label} <span>{group.tasks.length}</span>
                  </button>
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
                  group.entries.map((entry) =>
                    entry.kind === "task" ? (
                      taskRow(entry.task)
                    ) : (
                      <div key={entry.id} className="delivery-issue-rows">
                        {/* A package that needs the user keeps its issue open. */}
                        {!entry.packages.some(needsYou) && (
                          <button
                            type="button"
                            className="delivery-issue-rows__fold"
                            aria-expanded={!issueFolded(entry)}
                            aria-label={`${issueFolded(entry) ? "Mostrar" : "Ocultar"} los paquetes de ${entry.key}`}
                            onClick={() => toggleIssue(entry.id)}
                          >
                            <Icon name={issueFolded(entry) ? "right" : "down"} />
                          </button>
                        )}
                        {issueRow(entry, issueFolded(entry))}
                        {!issueFolded(entry) && entry.packages.map(taskRow)}
                      </div>
                    ),
                  )}
              </section>
            ))}
          </nav>

          <main className="delivery-panel__detail">
            {selectedIssue && overview ? (
              <IssuePlan
                packages={selectedIssue.packages}
                overview={overview}
                now={now}
                onSelectTask={(task) => select(repo, task.id)}
              />
            ) : selectedTask && overview ? (
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
              overview && <p className="delivery-panel__empty">Sin tareas.</p>
            )}
          </main>
        </div>
      )}
    </div>
  );
}

/** A split issue's plan: its packages in order, where each stands, what it
 *  builds on and what it waits for. A package opens its task. */
function IssuePlan({
  packages,
  overview,
  now,
  onSelectTask,
}: {
  packages: DeliveryTask[];
  overview: DeliveryOverview;
  now: number;
  onSelectTask: (task: DeliveryTask) => void;
}) {
  const first = packages[0];
  const issue = splitIssueOf(overview, first);
  const of = issue?.of ?? first.package!.of;
  const merged = issue?.packages.filter((pkg) => pkg.merged).length ?? 0;
  return (
    <article className="delivery-task delivery-issue-plan" aria-label={`Plan de ${first.key}`}>
      <header className="delivery-task__header">
        <div className="delivery-task__heading">
          <h2>
            {first.key}
            {first.title && <span> {first.title}</span>}
          </h2>
        </div>
        <p className="delivery-muted">
          {of} paquetes, un PR cada uno · {merged} de {of} fusionados
        </p>
      </header>
      <ol className="delivery-issue-plan__packages" aria-label="Paquetes">
        {Array.from({ length: of }, (_, at) => at + 1).map((index) => {
          const task = packages.find((one) => one.package!.index === index);
          const summary = issue?.packages.find((pkg) => pkg.index === index);
          const title = task?.package?.title || summary?.title || `Paquete ${index}`;
          const state = issue ? packageState(overview, issue, index) : "sin crear";
          const pr =
            task && (overview.prs ?? []).find((candidate) => candidate.task_id === task.id);
          const base = packages.find((one) => one.id === task?.package?.builds_on)?.package?.index;
          const detail = task && !summary?.merged ? taskStatus(task, overview, now).detail : null;
          return (
            <li key={index}>
              <span className="delivery-issue-plan__index">{index}</span>
              <div>
                {task ? (
                  <button
                    type="button"
                    className="delivery-link-button"
                    onClick={() => onSelectTask(task)}
                  >
                    {title}
                  </button>
                ) : (
                  <strong>{title}</strong>
                )}
                <span className="delivery-muted">
                  {" "}
                  · {state}
                  {pr && ` · PR #${pr.number}`}
                  {base && ` · sobre el paquete ${base}`}
                </span>
                {detail && <p className="delivery-issue-plan__waits">{detail}</p>}
              </div>
            </li>
          );
        })}
      </ol>
    </article>
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
  const verdicts = verdictsOf(overview, task.id);
  const cost = costOf(jobs);
  // The latest QA's visual changes page: what the user reviews before the PR.
  const qaRecord = latestOfRole(jobs, "qa");
  const qaProblems = (overview.qa_problems ?? []).filter((problem) => problem.task_id === task.id);
  const qaProblemsToDecide = problemsToDecide(overview, task.id);
  const newestFirst = [...jobs].reverse();
  const approvals = overview.approvals.filter((approval) => approval.task_id === task.id);
  const [expandedJobId, setExpandedJobId] = useState<string | null>(newestFirst[0]?.id ?? null);
  // A stage launched while the task is open shows its log as it runs.
  const newestStage = newestFirst.find((job) => job.agent !== "shell")?.id ?? null;
  const [seenStage, setSeenStage] = useState(newestStage);
  if (newestStage !== seenStage) {
    setSeenStage(newestStage);
    if (newestStage) setExpandedJobId(newestStage);
  }
  const latestUndoable = newestFirst.find(
    (job) => job.agent !== "shell" && !job.undone_at_ms && job.changes.length > 0,
  );
  const hasOpenJob = jobs.some(isOpen);
  const next = nextStageOf(overview, task.id);
  const round = roundOf(overview, task.id);
  const track = useMemo(() => taskTrack(overview, task), [overview, task]);
  // The history by rounds, newest first: regrouped when the overview changes, not each second.
  const history = useMemo(
    () =>
      taskHistory(
        jobsOf(overview, task.id),
        (overview.rounds ?? []).filter((candidate) => candidate.task_id === task.id),
      ),
    [overview, task.id],
  );
  const handoff = lastHandoff(jobs);
  const broken = brokenJobOf(overview, task.id);
  // A merge Tinto still runs stays in sight while it cleans up after itself.
  const openApprovals = approvals.filter(
    (approval) =>
      approval.status === "pending" || approval.status === "approved" || approval.progress,
  );
  // A reply to a reviewer holds nothing: the next step can still be asked.
  const ladderStep = nextStepOf(overview, task.id);
  const step = openApprovals.every((approval) => approval.rung === "pr_reply") ? ladderStep : null;
  const [requesting, setRequesting] = useState(false);
  // What opens on top of the task: its pending decisions, QA's problems, its
  // Jira issue or a blocker to add; next to the button that opened it.
  const [float, setFloatKind] = useState<"decisions" | "qa" | "blocker" | "issue" | null>(null);
  const [floatAt, setFloatAt] = useState<Element | null>(null);
  const setFloat = (kind: typeof float, at: Element | null = null) => {
    setFloatKind(kind);
    setFloatAt(at);
  };
  // The open step shown in full in its place: one approval or the replies.
  const [expanded, setExpanded] = useState<string | null>(null);
  const toggleStep = (id: string) => setExpanded(expanded === id ? null : id);
  // Once the task is shown, what it asks for later fades in; not what it asks
  // for already when it opens.
  const article = useRef<HTMLElement>(null);
  useEffect(() => {
    // Two frames: after a click this effect runs before the first paint, and
    // the asks already there would fade in with every task opened.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => article.current?.setAttribute("data-settled", ""));
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  // Replies to reviewers are answered together, in one place.
  const replies = openApprovals.filter(
    (approval) => approval.rung === "pr_reply" && approval.status === "pending",
  );
  // Approved replies Tinto posts once GitHub has their fix.
  const heldReplies = openApprovals.filter(
    (approval) => approval.rung === "pr_reply" && approval.status === "approved",
  );
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
  // The latest merge failed and the PR is still open: why, until it is asked again.
  const lastMerge = approvals
    .filter((approval) => approval.rung === "merge")
    .reduce<DeliveryApproval | null>(
      (latest, approval) =>
        !latest || approval.requested_at_ms > latest.requested_at_ms ? approval : latest,
      null,
    );
  const failedMerge = lastMerge?.status === "failed" && pr?.state === "open" ? lastMerge : null;
  // Opening the task checks its steps against GitHub: the PR is found by its
  // branch if needed and read, unless it was read moments ago.
  // A check that failed says so: what the task shows may be out of date.
  const [checkFailed, setCheckFailed] = useState<{ taskId: string; why: string } | null>(null);
  useEffect(() => {
    let live = true;
    checkDeliveryTask(task.id).then(
      () => live && setCheckFailed(null),
      (cause) => live && setCheckFailed({ taskId: task.id, why: errorText(cause) }),
    );
    return () => {
      live = false;
    };
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
  const settingsProposals = pendingSettingsProposals(overview, task);

  // What «te necesita» goes to here, in the order the task's status names it:
  // the card that holds it, opened with its own control (`-open`) and focused.
  const askId = (what: string) => `delivery-ask-${task.id}-${what}`;
  const asks = taskAsksOf(overview, task);
  // The one thing to do first: the only primary button among the task's cards.
  const askTarget = asks[0]?.id ?? "now";
  // Tinto says whose turn it is; with no ask and not the user's, the task goes on alone.
  const waitsOn = phaseOf(overview, task.id)?.waits_on;
  const yours = waitsOn === "user";
  // The PR is what the user has to move (ask for its review…).
  const prYours = yours && phaseOf(overview, task.id)?.kind === "pr";
  // The queue's answer for this task, so the two never disagree: in it, the
  // task does not "need nothing"; held there for something with no card of
  // its own here (failing checks…), the "Ahora" line opens with it.
  const queued = deliveryQueue(overview, [task], [], now, () => false, null);
  // Not when it is the task's own phrase: "Ahora" already says that one.
  const said = taskStatus(task, overview, now).label;
  const queueNeed = queued.find(
    (item) => item.kind === "broke" && item.ask === null && item.need !== said,
  )?.need;
  const nothingYours = queued.length === 0 && asks.length === 0 && !yours;
  // It goes on alone only while someone is working on it: a task that waits
  // on another one or on something outside does not.
  const goesOn = waitsOn === "agent" || waitsOn === "queue" || waitsOn === "coordinator";
  // The history is folded; a stage picked on the track opens it on the
  // stage's most recent row, and the round that holds it. `exact`: that very
  // job opened, in the row and round that hold it (an earlier attempt too).
  const historyFold = useRef<HTMLDetailsElement>(null);
  const focusStage = (job: DeliveryJob, exact = false) => {
    const row = history
      .flatMap((entry) => entry.rows)
      .find((candidate) =>
        exact
          ? candidate.attempts.some((attempt) => attempt.id === job.id)
          : candidate.latest && candidate.job.role === job.role,
      );
    setExpandedJobId(exact ? job.id : (row?.job.id ?? job.id));
    if (historyFold.current) historyFold.current.open = true;
    const shown = row && document.querySelector(`[data-job="${row.job.id}"]`);
    for (
      let fold = shown?.closest("details");
      fold;
      fold = fold.parentElement?.closest("details")
    ) {
      fold.open = true;
    }
    shown?.scrollIntoView?.({ block: "nearest" });
  };
  // The job a queue jump focuses once its row is drawn open.
  const focusJob = useRef<string | null>(null);
  useEffect(() => {
    const toggle =
      focusJob.current &&
      document.querySelector<HTMLElement>(
        `[data-job="${focusJob.current}"] > button[aria-expanded]`,
      );
    if (!toggle) return;
    focusJob.current = null;
    toggle.focus();
  });
  const { asked, settleAsked } = useDelivery();
  const askedHere = asked?.kind === "task" && asked.taskId === task.id ? asked : null;
  useEffect(() => {
    if (!askedHere) return;
    settleAsked();
    // A job (a paused stage): that attempt opened, even with a newer one of
    // its stage, its toggle focused so what it offers ("Reanudar") comes next.
    const jobAsked = askedHere.ask?.startsWith("job:")
      ? jobs.find((job) => job.id === askedHere.ask!.slice("job:".length))
      : undefined;
    if (jobAsked) {
      // A one-shot jump the queue asked for (settled above).
      // eslint-disable-next-line react-hooks/set-state-in-effect
      focusStage(jobAsked, true);
      focusJob.current = jobAsked.id;
      return;
    }
    // The ask chosen in the queue, while the task still asks it (the PR: while
    // it waits on the user; the blockers' block: while it has any).
    const target =
      asks.some((ask) => ask.id === askedHere.ask) ||
      (askedHere.ask === "pr" && prYours) ||
      (askedHere.ask === "broken" && openBlockers(task).length > 0)
        ? askedHere.ask!
        : askTarget;
    const opener = document.getElementById(`${askId(target)}-open`);
    const card = document.getElementById(askId(target)) ?? opener;
    // A card, or its opener, inside a folded section opens it.
    for (const at of [card, opener]) {
      for (let fold = at?.closest("details"); fold; fold = fold.parentElement?.closest("details")) {
        fold.open = true;
      }
    }
    card?.scrollIntoView?.({ block: "nearest" });
    card?.focus();
    // A fold's toggle (the PR's reviewers) is the control to go to, now open.
    if (opener?.tagName === "SUMMARY") opener.focus();
    // Pressed as the user would; a card that opens on top takes the focus itself.
    if (opener?.getAttribute("aria-expanded") === "false") opener.click();
    // Once per choice; askId only depends on the task.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [askedHere]);
  // The coordinator's label stops at the stages; the delivery is Tinto's to know.
  const delivered = !!phaseOf(overview, task.id) && !ladderStep;
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

  // The right-click menu: what the task can do now, what waits on the user first.
  const running = jobs.find((job) => job.status === "started" || job.status === "pending");
  const paused = pausedJob(jobs);
  const pendingStep = openApprovals.find((approval) => approval.status === "pending");
  const open = openBlockers(task);
  const blockerProps = {
    task,
    tasks: overview.tasks,
    onSelectTask,
    onAdd: (at: Element) => setFloat("blocker", at),
  };
  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});
  const menuEntries: (ContextItem | null | false | undefined)[] = [
    pendingStep && {
      label: `Aprobar ${stepName(stepOf(pendingStep), pr)}…`,
      onSelect: () => setExpanded(pendingStep.id),
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
          onSelect: () => void run(() => retryJobOnce(broken.id)),
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
    <article ref={article} className="delivery-task" aria-label={`Tarea ${taskLabel(task)}`}>
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
          {/* Two lines at most; the whole title on hover. */}
          <h2
            title={[taskLabel(task), task.package?.title, task.title].filter(Boolean).join(" · ")}
          >
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
                  onClick={(event) =>
                    setFloat(float === "issue" ? null : "issue", event.currentTarget)
                  }
                >
                  <Icon name="ticket" />
                  Jira
                </button>
              )}
              {pr && (
                <ExternalLink
                  // Amber only while the PR waits on the user.
                  className={`delivery-link-chip delivery-link-chip--pr delivery-link-chip--pr-${prMood(pr) === "attention" && !yours ? "neutral" : prMood(pr)}`}
                  href={pr.url}
                  label={`Abrir el PR #${pr.number} en GitHub`}
                >
                  <Icon name={pr.state === "merged" ? "merge" : "pull"} />
                  PR #{pr.number}
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
                  id={`${askId("qa")}-open`}
                  className={`delivery-link-chip delivery-link-chip--problems${qaProblemsToDecide > 0 ? " delivery-link-chip--pending" : ""}`}
                  aria-expanded={float === "qa"}
                  aria-label={
                    qaProblemsToDecide > 0
                      ? `Problemas de la QA, ${qaProblemsToDecide} por decidir`
                      : "Problemas de la QA"
                  }
                  onClick={(event) => setFloat(float === "qa" ? null : "qa", event.currentTarget)}
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
            {/* The batch and the sibling packages are in the bar and the list. */}
            {cost && (
              <span title={cost.partial ? COST_PARTIAL : undefined}>Costo: {cost.label}</span>
            )}
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

      {asks.length > 1 && (
        <p className="delivery-asks" role="status">
          <strong>Te tocan {asks.length} cosas aquí</strong>, en este orden:{" "}
          {asks.map((ask) => ask.label).join(", ")}.
        </p>
      )}
      {/* One line in the task; the cards open on top only when asked for. */}
      {blocked && (
        <div className="delivery-pending-bar" role="status" id={askId("decisions")} tabIndex={-1}>
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
            id={`${askId("decisions")}-open`}
            className="delivery-button--primary"
            aria-expanded={float === "decisions"}
            onClick={(event) => setFloat("decisions", event.currentTarget)}
          >
            Responder
          </button>
        </div>
      )}
      {blocked && float === "decisions" && (
        <TaskFloat
          anchor={floatAt}
          label="Responder decisiones"
          closeLabel="Cerrar decisiones"
          onClose={() => setFloat(null)}
        >
          <PendingDecisions taskId={task.id} decisions={pendingDecisions} />
        </TaskFloat>
      )}
      {float === "blocker" && (
        <TaskFloat
          anchor={floatAt}
          label="Añadir bloqueo"
          closeLabel="Cerrar añadir bloqueo"
          onClose={() => setFloat(null)}
        >
          <AddBlocker task={task} onDone={() => setFloat(null)} />
        </TaskFloat>
      )}
      {float === "qa" && (
        <TaskFloat
          anchor={floatAt}
          label="Problemas de la QA"
          closeLabel="Cerrar problemas de la QA"
          onClose={() => setFloat(null)}
        >
          <QaProblems taskId={task.id} problems={qaProblems} />
        </TaskFloat>
      )}
      {float === "issue" && issueUrl && (
        <TaskFloat
          anchor={floatAt}
          label={`Incidencia ${task.key} de Jira`}
          closeLabel="Cerrar la incidencia"
          onClose={() => setFloat(null)}
        >
          <JiraIssue key={task.id} taskId={task.id} issueUrl={issueUrl} />
        </TaskFloat>
      )}

      {openApprovals
        .filter((approval) => approval.rung !== "pr_reply")
        .map((approval) => (
          <ApprovalBar
            key={approval.id}
            id={askId(approval.id)}
            approval={approval}
            pr={pr}
            primary={askTarget === approval.id}
            open={expanded === approval.id}
            onToggle={() => toggleStep(approval.id)}
          >
            {approval.rung === "merge" && approval.progress ? (
              <MergeRunning approval={approval} now={now} />
            ) : (
              <ApprovalBanner task={task} approval={approval} pr={pr} now={now} />
            )}
          </ApprovalBar>
        ))}
      {failedMerge && pr && <MergeFailed approval={failedMerge} pr={pr} />}
      {checkFailed?.taskId === task.id && (
        <p className="delivery-job__note" role="status">
          No se pudo comprobar la tarea en GitHub: {checkFailed.why}
        </p>
      )}
      {replies.length > 0 && (
        <StepBar
          id={askId("replies")}
          title={
            replies.length === 1
              ? "Aprobar la respuesta al revisor"
              : `Aprobar ${replies.length} respuestas a los revisores`
          }
          button="Revisar"
          buttonLabel="Revisar las respuestas a los revisores"
          primary={askTarget === "replies"}
          open={expanded === "replies"}
          onToggle={() => toggleStep("replies")}
        >
          <RepliesBanner replies={replies} />
        </StepBar>
      )}
      {heldReplies.length > 0 && (
        <ul className="delivery-pr__facts" aria-label="Respuestas aprobadas sin publicar">
          {heldReplies.map((reply) => (
            <li key={reply.id} role="status">
              <strong>{reply.title}</strong>: {reply.notice ?? "Respuesta aprobada · publicándose"}
            </li>
          ))}
        </ul>
      )}

      <PendingSettingsProposals id={askId("settings")} proposals={settingsProposals} />

      {rejected && openApprovals.length === 0 && (
        <RejectedStep task={task} approval={rejected} pr={pr} />
      )}

      {broken && (
        <BrokenStage
          key={broken.id}
          id={askId("broken")}
          job={broken}
          primary={askTarget === "broken"}
          undoable={latestUndoable?.id === broken.id}
          onContinue={() => onContinueJob(broken)}
          onUndo={() => undo(broken)}
          blockers={open.length > 0 && <BlockerList {...blockerProps} />}
          blockerActions={
            open.length > 0 && (
              <BlockerActions
                task={task}
                onAdd={blockerProps.onAdd}
                className="delivery-button--quiet"
              />
            )
          }
        />
      )}

      {/* What happens now, and in which round: one line. With nothing for the
          user, it opens by saying so, unless a blocker may be the user's to lift. */}
      <div
        className="delivery-task__now"
        role="group"
        aria-label="Ahora"
        id={askId("now")}
        tabIndex={-1}
      >
        <p>
          <span className="delivery-task__now-label">Ahora</span>
          <span>
            {queueNeed
              ? `${queueNeed} · `
              : nothingYours &&
                !delivered &&
                (goesOn
                  ? "Nada que hacer: sigue solo · "
                  : open.length === 0 && "Nada que hacer por tu parte · ")}
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
      </div>

      {!broken && <BlockedBar id={askId("broken")} {...blockerProps} />}

      <StageTrack
        track={track}
        repo={task.repo}
        approvals={approvals}
        next={next}
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
        {/* Not in a round with stages to do again, nor while something blocks it. */}
        {!next && !hasOpenJob && !needsYou && !round && open.length === 0 && (
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
      <PausedRestack approvals={approvals} />

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

      {qaRecord && <QaRecordNotice job={qaRecord} />}

      {/* Folded sections, one line each; the PR's opens by itself while it
          waits on the user: replies to its reviewers, what it needs, or the
          task waiting on the user for it. */}
      {pr && (
        <PrSection
          id={askId("pr")}
          pr={pr}
          yours={yours}
          open={replies.length > 0 || prYours || (yours && prMood(pr) === "attention")}
        />
      )}

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

      <ScopeSection
        task={task}
        jobs={jobs}
        decisions={taskDecisions}
        blocked={blocked}
        onLaunch={() => dispatch(SCOPE_ROLE, false)}
      />

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

      {/* Folded: what happened, by rounds, newest first. */}
      <section className="delivery-section" aria-label="Historial">
        <details ref={historyFold} className="delivery-fold">
          <summary>
            <h3>Historial</h3>
            <span className="delivery-fold__gist">{historyGist(history)}</span>
          </summary>
          {jobs.length === 0 && (
            <p className="delivery-panel__empty">Todavía no se ha lanzado ninguna etapa.</p>
          )}
          {jobs.length > 0 && (
            <TaskHistory
              taskId={task.id}
              entries={history}
              now={now}
              renderRow={(row, id) => {
                // Opened, or one of the row's attempts is: they show inside it.
                const holdsOpen = row.attempts.some((attempt) => attempt.id === expandedJobId);
                const renderJob = (job: DeliveryJob, main?: ReactNode) => {
                  // The row closes on one click whatever it shows open; an
                  // earlier attempt inside it toggles by its own id.
                  const expanded = expandedJobId === job.id || (main !== undefined && holdsOpen);
                  return (
                    <JobRow
                      key={job.id}
                      id={main === undefined ? undefined : id}
                      job={job}
                      jobs={jobs}
                      verdicts={verdicts}
                      wait={overview.queued?.[job.id]}
                      now={now}
                      attempts={main === undefined ? undefined : row.attempts.length}
                      expanded={expanded}
                      undoable={latestUndoable?.id === job.id && !hasOpenJob}
                      onToggle={() => setExpandedJobId(expanded ? null : job.id)}
                      onUndo={() => undo(job)}
                      onContinue={() => onContinueJob(job)}
                    >
                      {main}
                    </JobRow>
                  );
                };
                const older = row.attempts.filter((attempt) => attempt !== row.job).reverse();
                if (row.key === CHECKS_ROW) {
                  const key = `${CHECKS_ROW}:${row.attempts[0].id}`;
                  const expanded = expandedJobId === key || holdsOpen;
                  return (
                    <CheckRow
                      key={key}
                      checks={row.checks!}
                      jobs={jobs}
                      verdicts={verdicts}
                      now={now}
                      expanded={expanded}
                      onToggle={() => setExpandedJobId(expanded ? null : key)}
                    >
                      {[...row.attempts].reverse().map((job) => renderJob(job))}
                    </CheckRow>
                  );
                }
                return renderJob(
                  row.job,
                  older.length > 0 ? (
                    <>
                      <h4 className="delivery-job__subtitle">Intentos anteriores</h4>
                      <ul className="delivery-jobs">{older.map((job) => renderJob(job))}</ul>
                    </>
                  ) : null,
                );
              }}
            />
          )}
        </details>
      </section>
    </article>
  );
}

/** Past this, why a stage broke shows its first lines until asked for all. */
const BROKEN_WHY_CHARS = 240;

/** The task's latest stage failed or was blocked: why, in a few lines, and
 *  what to do about it. When its work got done another way, the user settles
 *  it with a note instead of running it again. A blocked task's blockers go
 *  in the same block, under the stage's own reason: one fact, one surface. */
function BrokenStage({
  id,
  job,
  primary,
  undoable,
  onContinue,
  onUndo,
  blockers,
  blockerActions,
}: {
  id: string;
  job: DeliveryJob;
  /** It is what the task asks for first. */
  primary: boolean;
  undoable: boolean;
  onContinue: () => Promise<unknown>;
  onUndo: () => Promise<unknown>;
  /** The task's open blockers, when it has any. */
  blockers?: ReactNode;
  /** Their actions, after the stage's own. */
  blockerActions?: ReactNode;
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
      id={id}
      tabIndex={-1}
      className="delivery-banner delivery-banner--danger delivery-broken"
      role="group"
      aria-label="Etapa fallida"
    >
      <p className="delivery-banner__title">
        <Icon name="x" />
        {/* The task's phrase for it, word for word as its row and the queue say it. */}
        <strong>{jobBrokePhrase(job)}</strong>
        {brokenHow(job) && <span className="delivery-muted">· {brokenHow(job)}</span>}
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
      {blockers}
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
              className={primary ? "delivery-button--primary" : undefined}
              onAction={() => retryJobOnce(job.id)}
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
            {blockerActions}
          </div>
        </ActionGroup>
      )}
    </div>
  );
}

/** How a stage broke, after its phrase, when the phrase does not say it. */
function brokenHow(job: DeliveryJob): string | null {
  if (job.status === "interrupted") return "se interrumpió";
  if (job.status !== "failed" && job.result_state === "invalid") {
    return "terminó sin un resultado válido";
  }
  return null;
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

/** How a stop that holds the line reads in the mini line's name, worst first. */
const MINI_LINE_STOP: [Station, string][] = [
  ["broke", "falló"],
  ["yours", "te necesita"],
  ["attention", "requiere atención"],
];

/** A task's stages and steps as small stations on its list row: where it
 *  is, at a glance. Its name says how far it got and the stop that holds it. */
function MiniLine({ track }: { track: TaskTrack }) {
  const stops = [
    ...track.agents.map((stop) => ({ name: stageLabel(stop.role), station: stop.station })),
    ...track.steps.map((stop) => ({ name: stop.step.label, station: stop.station })),
  ].filter((stop) => stop.station !== "skipped");
  const passed = stops.filter((stop) => stop.station === "passed").length;
  const held = MINI_LINE_STOP.map(([station, words]) => {
    const stop = stops.find((candidate) => candidate.station === station);
    return stop && `${stop.name}: ${words}`;
  }).find(Boolean);
  const name = `${passed} de ${stops.length} pasos${held ? ` · ${held}` : ""}`;
  return (
    <span className="delivery-mini-line" role="img" aria-label={name} title={name}>
      {track.agents.map((stop) => (
        <i key={`agent-${stop.role}`} data-station={stop.station} />
      ))}
      {track.steps.map((stop) => (
        <i
          key={`step-${stepKey(stop.step)}`}
          className="delivery-mini-line__step"
          data-station={stop.station}
        />
      ))}
    </span>
  );
}

function StageTrack({
  track,
  repo,
  approvals,
  next,
  onFocus,
}: {
  /** The task's stages and steps, as `taskTrack` reads them. */
  track: TaskTrack;
  /** The task's repo, whose settings say who runs each stage. */
  repo: string;
  approvals: DeliveryApproval[];
  next: string | null;
  onFocus: (job: DeliveryJob) => void;
}) {
  return (
    <div className="delivery-track">
      <div className="delivery-track__part">
        <h3 id="delivery-track-agents">Agentes</h3>
        <ol aria-labelledby="delivery-track-agents">
          {track.agents.map((outcome) => {
            const { role, job, inRound, tone } = outcome;
            const content = (
              <>
                <span className="delivery-node__name">
                  <StatusMark icon={tone === "todo" ? "ring" : TONE_ICON[tone]} />
                  {stageLabel(role)}
                </span>
                {/* A line cut short keeps its full text on hover. */}
                <small title={outcome.title ?? outcome.label}>{outcome.label}</small>
                {job?.visual_record && !inRound && <small>Con registro visual ↓</small>}
                {job && isOpen(job) && job.activity && (
                  <small className="delivery-node__activity" title={job.activity}>
                    {job.activity}
                  </small>
                )}
                {job && job.agent !== "shell" && (
                  <small className="delivery-node__runtime" title={runtimeLabel(job)}>
                    {runtimeLabel(job)}
                  </small>
                )}
              </>
            );
            return (
              <li
                key={role}
                className={`delivery-node delivery-node--${tone}`}
                data-station={outcome.station}
                data-joined={outcome.joined || undefined}
                aria-current={role === next ? "step" : undefined}
              >
                {job ? (
                  <button
                    type="button"
                    title={`${stageLabel(role)} · intento ${job.attempt} · ${agentName(job.agent)} · ${runtimeLabel(job)}`}
                    onClick={() => onFocus(job)}
                  >
                    {content}
                    {/* What pointing at it offers; its own words say the rest. */}
                    <span className="delivery-node__hint" aria-hidden="true">
                      Ver en el historial
                    </span>
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
                {/* What runs now and is no station (Tinto's checks, a setup):
                    a mark on the stretch after this stage. */}
                {track.between?.after === role && (
                  <span
                    className="delivery-node__now"
                    role="img"
                    aria-label={track.between.label}
                    title={track.between.label}
                  />
                )}
              </li>
            );
          })}
        </ol>
      </div>
      <div className="delivery-track__part delivery-track__part--rungs">
        <h3 id="delivery-track-rungs">Entrega</h3>
        <ol aria-labelledby="delivery-track-rungs">
          {track.steps.map((step) => {
            const { step: ladder, approval } = step;
            const { rung, label } = ladder;
            const body = (
              <>
                <span className="delivery-node__name">
                  <StatusMark icon={step.icon} />
                  {label}
                </span>
                <small title={step.title ? undefined : step.label}>{step.label}</small>
              </>
            );
            // A paused restack opens on its step, where it can be abandoned.
            const paused =
              rung === "sync" && !!approval && pausedRestack(approvals)?.id === approval.id;
            return (
              <li
                key={stepKey(ladder)}
                className={`delivery-node delivery-node--${step.tone}`}
                data-rung={rung}
                data-station={step.station}
                data-joined={step.joined || undefined}
              >
                {paused ? (
                  <button
                    type="button"
                    title={step.title}
                    onClick={() =>
                      document
                        .querySelector<HTMLButtonElement>(`#${PAUSED_RESTACK_ID} button`)
                        ?.focus()
                    }
                  >
                    {body}
                  </button>
                ) : (
                  <div title={step.title}>{body}</div>
                )}
              </li>
            );
          })}
        </ol>
      </div>
    </div>
  );
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
  anchor,
  label,
  closeLabel,
  onClose,
  children,
}: {
  anchor: Element | null;
  label: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <Overlay anchor={anchor} label={label} className="delivery-float" onClose={onClose}>
      <button
        type="button"
        className="delivery-float__close"
        aria-label={closeLabel}
        onClick={onClose}
      >
        ×
      </button>
      {children}
    </Overlay>
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
  // Replies to reviewers, posted or refused by GitHub, oldest first.
  const replies = approvals.filter(
    (approval) =>
      approval.rung === "pr_reply" &&
      (approval.status === "executed" || approval.status === "failed"),
  );
  if (done.length === 0 && replies.length === 0) return null;
  // Folded: the delivery line already says each step; the gist counts them.
  const gist = [
    done.length > 0 && (done.length === 1 ? "1 paso" : `${done.length} pasos`),
    replies.length > 0 &&
      (replies.length === 1
        ? "1 respuesta al revisor"
        : `${replies.length} respuestas a revisores`),
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <section className="delivery-section" aria-label="Resultados de la entrega">
      <details className="delivery-fold">
        <summary>
          <h3>Resultados de la entrega</h3>
          <span className="delivery-fold__gist">{gist}</span>
        </summary>
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
          {replies.map((reply) => (
            <li key={reply.id} aria-label={reply.title}>
              <strong>{reply.title}</strong>{" "}
              {reply.status === "executed" && reply.outcome ? (
                <ExternalLink href={reply.outcome}>{reply.outcome}</ExternalLink>
              ) : (
                <span className="delivery-job__note">No se publicó: {reply.outcome}</span>
              )}{" "}
              <small className="delivery-muted">{when(reply.executed_at_ms)}</small>
              {reply.notice && <p className="delivery-job__note">{reply.notice}</p>}
            </li>
          ))}
        </ul>
      </details>
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

const PAUSED_RESTACK_ID = "delivery-paused-restack";

/** The task's latest sync when it is a restack paused on conflicts. */
function pausedRestack(approvals: DeliveryApproval[]): DeliveryApproval | undefined {
  const sync = latestApproval(approvals, "sync");
  const paused =
    sync?.restack &&
    sync.status === "executed" &&
    (sync.conflicts?.length ?? 0) > 0 &&
    !(sync.created?.length ?? 0);
  return paused ? sync : undefined;
}

/** A restack paused on conflicts: Tinto continues it once they are
 *  resolved, and the user can abandon it, keeping what was resolved. */
function PausedRestack({ approvals }: { approvals: DeliveryApproval[] }) {
  const sync = pausedRestack(approvals);
  if (!sync) return null;
  return (
    <div
      id={PAUSED_RESTACK_ID}
      className="delivery-commit-step"
      role="group"
      aria-label="Restack en pausa"
    >
      {sync.notice && <p className="delivery-job__note">{sync.notice}</p>}
      <ActionButton
        title="La rama vuelve a donde estaba; lo resuelto se guarda en una referencia de recuperación."
        onAction={async () => {
          const ok = await confirm(
            "La rama volverá a donde estaba antes del restack. Lo resuelto hasta ahora queda guardado en una referencia de recuperación; la tarea sigue pendiente de restack y Tinto no lo vuelve a empezar solo.",
            { title: "Abandonar el restack", okLabel: "Abandonar" },
          );
          if (!ok) return false;
          await undoDeliveryCommit(sync.id);
        }}
      >
        Abandonar el restack
      </ActionButton>
    </div>
  );
}

/** The people who could review a repo's PRs, as GitHub last listed them:
 *  shown while it is asked again. Per viewer; it may be empty or gone. */
const REVIEWERS_KEY = "tinto:pr-reviewers";

function cachedReviewers(repo: string): string[] | null {
  try {
    const saved = JSON.parse(localStorage.getItem(REVIEWERS_KEY) ?? "{}") as Record<
      string,
      unknown
    >;
    const logins = saved[repo];
    return Array.isArray(logins) && logins.every((login) => typeof login === "string")
      ? logins
      : null;
  } catch {
    return null;
  }
}

function cacheReviewers(repo: string, logins: string[]) {
  try {
    const saved = JSON.parse(localStorage.getItem(REVIEWERS_KEY) ?? "{}") as Record<
      string,
      unknown
    >;
    localStorage.setItem(REVIEWERS_KEY, JSON.stringify({ ...saved, [repo]: logins }));
  } catch {
    /* unavailable storage: the list is read from GitHub each time */
  }
}

/** The latest step, when the user rejected it and nothing was asked since. */
function rejectedStep(approvals: DeliveryApproval[]): DeliveryApproval | null {
  const latest = approvals
    .filter((approval) => approval.rung !== "pr_reply")
    .sort((a, b) => b.requested_at_ms - a.requested_at_ms)[0];
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
/** The file name of a path Tinto gave, with either separator. */
function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/**
 * What the user has to know about the latest QA's record: why there is
 * none, what it lacks of the QA record guide, the other files QA wrote.
 */
function QaRecordNotice({ job }: { job: DeliveryJob }) {
  const warnings = job.visual_record_warnings ?? [];
  const extras = job.visual_record_extras ?? [];
  if (!job.visual_record_error && warnings.length === 0 && extras.length === 0) {
    return null;
  }
  const missing = !job.visual_record;
  const title = missing
    ? job.visual_record_source
      ? "No se pudo generar el registro visual de la QA"
      : "La QA no tiene registro visual"
    : warnings.length > 0
      ? "El registro de QA no sigue la guía"
      : "La QA escribió más de un registro";
  return (
    <div
      className={`delivery-banner delivery-banner--${missing ? "danger" : "note"}`}
      role="group"
      aria-label="Registro visual de QA"
    >
      <p className="delivery-banner__title">
        <Icon name={missing ? "x" : "photo"} />
        <strong>{title}</strong>
      </p>
      {job.visual_record_error && <p>{job.visual_record_error}</p>}
      {warnings.map((warning) => (
        <p key={warning}>{warning}.</p>
      ))}
      {job.visual_record_source && !missing && (
        <p className="delivery-muted">Se muestra {fileName(job.visual_record_source)}.</p>
      )}
      {extras.length > 0 && (
        <p className="delivery-muted">También escribió: {extras.join(", ")}.</p>
      )}
      <ActionGroup>
        <div className="delivery-actions">
          <ActionButton
            title="Busca otra vez el registro en la carpeta de QA del trabajo y lo vuelve a generar"
            onAction={() => regenerateDeliveryVisualRecord(job.id)}
          >
            Regenerar registro
          </ActionButton>
        </div>
      </ActionGroup>
    </div>
  );
}

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

/** An open step as one line in the task; its text and buttons open in place. */
function ApprovalBar({
  id,
  approval,
  pr,
  primary,
  open,
  onToggle,
  children,
}: {
  id: string;
  approval: DeliveryApproval;
  pr: DeliveryPr | null | undefined;
  /** It is what the task asks for first. */
  primary: boolean;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const name = stepName(stepOf(approval), pr);
  const pending = approval.status === "pending";
  const running = approval.progress ? `Fusionando · ${lower(approval.progress.step)}` : null;
  // Opened, the step is approved from its card.
  const direct = !open && approvesInOneClick(approval, stepDrafts.has(approval.id));
  return (
    <StepBar
      id={id}
      title={pending ? `Aprobar ${name}` : (running ?? `Esperando al coordinador: ${name}`)}
      summary={approvalSummary(approval)}
      button={pending ? "Revisar" : "Ver"}
      buttonLabel={`${pending ? "Revisar" : "Ver"} ${name}`}
      primary={primary && !direct}
      waiting={!pending}
      action={
        direct && (
          <ActionGroup>
            <ActionButton
              className={primary ? "delivery-button--primary" : undefined}
              onAction={() => decideDeliveryApproval(approval.id, true)}
            >
              {approveLabel(approval, pr)}
            </ActionButton>
          </ActionGroup>
        )
      }
      open={open}
      onToggle={onToggle}
    >
      {children}
    </StepBar>
  );
}

/** A step's line, which opens into its card below it; the chevron folds it. */
function StepBar({
  id,
  title,
  summary,
  button,
  buttonLabel,
  primary,
  waiting,
  action,
  open,
  onToggle,
  children,
}: {
  /** Its card's id, focused by «te necesita»; its button's is `${id}-open`. */
  id: string;
  title: string;
  summary?: string;
  button: string;
  buttonLabel: string;
  primary?: boolean;
  /** Approved, it waits on the coordinator: neutral, not the step's colour. */
  waiting?: boolean;
  /** What the step can do without opening, beside the button that opens it. */
  action?: ReactNode;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <div
      id={id}
      tabIndex={-1}
      className={`delivery-step${open ? " delivery-step--open" : ""}${waiting ? " delivery-step--waiting" : ""}`}
    >
      <div className="delivery-pending-bar delivery-pending-bar--step" role="status">
        <span className="delivery-pending-bar__text">
          <strong>{title}</strong>
          {summary && <span className="delivery-muted"> · {summary}</span>}
        </span>
        <span className="delivery-pending-bar__actions">
          {action}
          <button
            type="button"
            id={`${id}-open`}
            className={primary && !open ? "delivery-button--primary" : undefined}
            aria-label={buttonLabel}
            aria-expanded={open}
            onClick={onToggle}
          >
            {open ? "Ocultar" : button}
            <Icon name="down" />
          </button>
        </span>
      </div>
      {open && children}
    </div>
  );
}

/** The commits of the branch that changed the commented file since the
 *  review comment: the fix a reply is about. */
function FixCommits({ commits, path }: { commits: DeliveryFixCommit[]; path: string }) {
  if (commits.length === 0) {
    return (
      <p className="delivery-muted">Ningún commit de la rama cambió {path} desde el comentario.</p>
    );
  }
  return (
    <ul className="delivery-replies__fix" aria-label="Commits con el arreglo">
      {commits.map((commit) => (
        <li key={commit.sha}>
          <ExternalLink
            href={commit.url}
            label={`Ver el commit ${commit.sha.slice(0, 7)} en GitHub`}
          >
            <code>{commit.sha.slice(0, 7)}</code>
          </ExternalLink>{" "}
          {commit.subject}
        </li>
      ))}
    </ul>
  );
}

/** QA screenshots a reply or the PR's description can show, ticked on or
 *  off. Once published, Tinto pushes the chosen ones to the task's evidence
 *  branch and shows them after the text. */
function QaImagePicker({
  shots,
  chosen,
  onChange,
  where = "en la respuesta",
}: {
  shots: DeliveryQaScreenshot[];
  chosen: DeliveryReplyImage[];
  onChange: (images: DeliveryReplyImage[]) => void;
  /** Where the chosen ones show: "en la respuesta", "en la descripción del PR". */
  where?: string;
}) {
  if (shots.length === 0) return null;
  const same = (a: DeliveryReplyImage, b: DeliveryReplyImage) =>
    a.job_id === b.job_id && a.file === b.file;
  return (
    <details className="delivery-replies__images" open={chosen.length > 0}>
      <summary>Capturas de la QA{chosen.length > 0 ? ` · ${chosen.length} ${where}` : ""}</summary>
      <ul aria-label="Capturas de la QA">
        {shots.map((shot) => {
          const on = chosen.some((image) => same(image, shot));
          return (
            <li key={`${shot.job_id}/${shot.file}`}>
              <label>
                <input
                  type="checkbox"
                  checked={on}
                  aria-label={`Mostrar ${shot.file} ${where}`}
                  onChange={() =>
                    onChange(
                      on
                        ? chosen.filter((image) => !same(image, shot))
                        : [...chosen, { job_id: shot.job_id, file: shot.file }],
                    )
                  }
                />
                <img src={shot.data_uri} alt={shot.file} />
                <small>
                  {shot.file} · QA intento {shot.attempt}
                </small>
              </label>
            </li>
          );
        })}
      </ul>
      <p className="delivery-muted">
        Se suben a una rama tinto/qa-evidence de la tarea, que nunca se fusiona y Tinto borra cuando
        el PR se cierra o se fusiona.
      </p>
    </details>
  );
}

/** The task's pending replies to reviewers, all in one place: each comment
 *  with its diff and the editable text Tinto posts as the user once
 *  approved, next to how the user replies in this repository. */
function RepliesBanner({ replies }: { replies: DeliveryApproval[] }) {
  const [bodies, setBodies] = useState<Record<string, string>>(() =>
    Object.fromEntries(replies.map((reply) => [reply.id, reply.body])),
  );
  const bodyOf = (reply: DeliveryApproval) => bodies[reply.id] ?? reply.body;
  const [images, setImages] = useState<Record<string, DeliveryReplyImage[]>>(() =>
    Object.fromEntries(replies.map((reply) => [reply.id, reply.images ?? []])),
  );
  const imagesOf = (reply: DeliveryApproval) => images[reply.id] ?? reply.images ?? [];
  const [shots, setShots] = useState<DeliveryQaScreenshot[]>([]);
  const taskId = replies[0]?.task_id;
  useEffect(() => {
    if (!taskId) return;
    let live = true;
    getDeliveryQaScreenshots(taskId).then(
      (found) => live && setShots(found),
      () => live && setShots([]),
    );
    return () => {
      live = false;
    };
  }, [taskId]);
  const examples = replies.find((reply) => reply.style_examples?.length)?.style_examples ?? [];
  const authorOf = (reply: DeliveryApproval) => reply.thread?.comment?.author;
  const authors = [...new Set(replies.map(authorOf).filter((login) => login !== undefined))];
  // Who is asked to review again once answered: every reviewer, unless unticked.
  const [skipped, setSkipped] = useState<string[]>([]);
  const askAgain = async (logins: (string | undefined)[]) => {
    const chosen = logins.filter(
      (login): login is string => login !== undefined && !skipped.includes(login),
    );
    if (chosen.length > 0) await changeDeliveryPrReviewers(replies[0].task_id, chosen, []);
  };
  const approve = async (reply: DeliveryApproval) => {
    await decideDeliveryApproval(
      reply.id,
      true,
      undefined,
      undefined,
      bodyOf(reply),
      imagesOf(reply),
    );
    await askAgain([authorOf(reply)]);
  };
  // Approved together, the replies are posted in the background, a few at
  // a time: until each one is, it stays here without its buttons. One that
  // could not go out gets a new notice saying why, and its buttons back.
  const [posting, setPosting] = useState<Record<string, string | null>>({});
  const isPosting = (reply: DeliveryApproval) =>
    reply.id in posting && posting[reply.id] === (reply.notice ?? null);
  const approveAll = async () => {
    await approveDeliveryReplies(
      replies.map((reply) => ({
        approval_id: reply.id,
        body: bodyOf(reply),
        images: imagesOf(reply),
      })),
      authors.filter((login) => !skipped.includes(login)),
    );
    setPosting(Object.fromEntries(replies.map((reply) => [reply.id, reply.notice ?? null])));
  };
  return (
    <div
      className="delivery-banner delivery-banner--step"
      role="group"
      aria-label="Aprobación pr_reply"
    >
      {examples.length > 0 && (
        <details className="delivery-replies__examples">
          <summary>Cómo respondes tú: tus últimas respuestas en este repositorio</summary>
          <ul>
            {examples.map((example, index) => (
              <li key={index}>{example}</li>
            ))}
          </ul>
        </details>
      )}
      {replies.map((reply) => {
        const body = bodyOf(reply);
        return (
          <section key={reply.id} className="delivery-replies__item" aria-label={reply.title}>
            <p className="delivery-banner__title">
              <Icon name="flag" />
              <strong>{reply.title}</strong>
              <span className="delivery-muted">{preparedBy(reply)}</span>
            </p>
            {reply.thread ? (
              <>
                <CodeThreads threads={[reply.thread]} />
                <FixCommits commits={reply.fix_commits ?? []} path={reply.thread.path} />
              </>
            ) : (
              <p className="delivery-muted">
                Comentario general en el{" "}
                <ExternalLink href={reply.comment_url ?? ""} label="Ver el PR en GitHub">
                  PR ↗
                </ExternalLink>
              </p>
            )}
            <label className="delivery-request__field">
              <span>Respuesta, tal como se publicará</span>
              <GrowingTextarea
                value={body}
                onChange={(event) => setBodies({ ...bodies, [reply.id]: event.target.value })}
                rows={2}
              />
            </label>
            <QaImagePicker
              shots={shots}
              chosen={imagesOf(reply)}
              onChange={(chosen) => setImages({ ...images, [reply.id]: chosen })}
            />
            {reply.notice && <p className="delivery-job__note">{reply.notice}</p>}
            <p className="delivery-muted">
              Al aprobar, Tinto la publica con tu cuenta de GitHub
              {reply.thread ? " en el hilo del comentario" : " como comentario del PR"}
              {reply.resolve_thread ? " y resuelve el hilo" : ""}
              {reply.interim ? "" : " en cuanto el PR en GitHub tenga la corrección"}. Rechazar no
              publica nada.
            </p>
            {isPosting(reply) ? (
              <p className="delivery-muted" role="status">
                Publicando en GitHub…
              </p>
            ) : (
              <ActionGroup>
                <div className="delivery-actions">
                  <ActionButton
                    className="delivery-button--primary"
                    disabled={!body.trim()}
                    onAction={() => approve(reply)}
                  >
                    {body === reply.body
                      ? "Aprobar y responder"
                      : "Aprobar y responder con tu texto"}
                  </ActionButton>
                  <ActionButton onAction={() => decideDeliveryApproval(reply.id, false)}>
                    Rechazar
                  </ActionButton>
                </div>
              </ActionGroup>
            )}
          </section>
        );
      })}
      {authors.length > 0 && (
        <fieldset className="delivery-reviewers">
          <legend>Al responder, volver a pedir la revisión a</legend>
          {authors.map((login) => (
            <label key={login}>
              <input
                type="checkbox"
                checked={!skipped.includes(login)}
                onChange={(event) =>
                  setSkipped(
                    event.target.checked
                      ? skipped.filter((other) => other !== login)
                      : [...skipped, login],
                  )
                }
              />{" "}
              {login}
            </label>
          ))}
        </fieldset>
      )}
      {replies.length > 1 && !replies.some(isPosting) && (
        <ActionGroup>
          <div className="delivery-actions">
            <ActionButton
              className="delivery-button--primary"
              disabled={replies.some((reply) => !bodyOf(reply).trim())}
              onAction={approveAll}
            >
              {`Aprobar y responder las ${replies.length}`}
            </ActionButton>
          </div>
        </ActionGroup>
      )}
    </div>
  );
}

/** The commit, PR or merge texts the user edited and has not approved yet,
 *  by approval: hiding the step or opening another task does not lose them. */
const stepDrafts = new Map<string, { title: string; body: string; messages?: string[] }>();
/** Who shows a one-click approval, told when a step gains or loses its draft. */
const draftListeners = new Set<() => void>();

function setStepDraft(
  id: string,
  draft: { title: string; body: string; messages?: string[] } | null,
) {
  const had = stepDrafts.has(id);
  if (draft) stepDrafts.set(id, draft);
  else stepDrafts.delete(id);
  if (had !== !!draft) draftListeners.forEach((listener) => listener());
}

function subscribeDrafts(listener: () => void) {
  draftListeners.add(listener);
  return () => void draftListeners.delete(listener);
}

/** The approvals with a draft, as a value that changes when they do. */
const draftedIds = () => [...stepDrafts.keys()].sort().join("\n");

function ApprovalBanner({
  task,
  approval,
  pr,
  now,
}: {
  task: DeliveryTask;
  approval: DeliveryApproval;
  pr: DeliveryPr | undefined;
  now: number;
}) {
  const [outcome, setOutcome] = useState("");
  const name = stepName(stepOf(approval), pr);
  const updatesPr = approval.rung === "pr" && pr?.state === "open" ? pr : null;
  const pending = approval.status === "pending";
  const choosesReviewers = pending && approval.rung === "pr";
  const [reviewers, setReviewers] = useState<string[]>(approval.reviewers ?? []);
  // The repo's people who can review: the list last read shows at once, and
  // `fresh` says GitHub answered this time.
  const [candidates, setCandidates] = useState<string[] | null>(() => cachedReviewers(task.repo));
  const [fresh, setFresh] = useState(false);
  const [candidatesError, setCandidatesError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  // "PR sin revisor" in the repo's settings lets the step go without one.
  const [reviewerOptional, setReviewerOptional] = useState(false);
  const [settingsRead, setSettingsRead] = useState(false);
  useEffect(() => {
    if (!choosesReviewers) return;
    let live = true;
    getDeliveryPrReviewers(task.id).then(
      (logins) => {
        if (!live) return;
        setCandidatesError(null);
        setCandidates(logins);
        setFresh(true);
        cacheReviewers(task.repo, logins);
      },
      (cause) => live && setCandidatesError(errorText(cause)),
    );
    getDeliveryRepoSettings(task.repo).then(
      (settings) => {
        if (!live) return;
        setReviewerOptional(settings.pr_without_reviewer ?? false);
        setSettingsRead(true);
      },
      () => live && setSettingsRead(true),
    );
    return () => {
      live = false;
    };
  }, [choosesReviewers, task.id, task.repo, attempt]);
  // Without a reviewer nobody can approve the PR, and it cannot be merged:
  // the step goes without one only when the repo allows it or GitHub said
  // nobody else can review. Until then it says what it waits for.
  const reviewerHold = reviewerBlock({
    choosesReviewers,
    chosen: reviewers.length,
    settingsRead,
    reviewerOptional,
    candidates,
    fresh,
    failed: candidatesError !== null,
  });
  // A commit, PR or merge step's texts are the user's to edit before approving.
  const planned = approval.rung === "commit" && (approval.commits ?? []).length > 0;
  const editsText =
    pending && !approval.record && ["commit", "pr", "merge"].includes(approval.rung);
  const draft = stepDrafts.get(approval.id);
  const [title, setTitle] = useState(draft?.title ?? approval.title);
  const [body, setBody] = useState(draft?.body ?? approval.body);
  const [messages, setMessages] = useState<string[] | undefined>(draft?.messages);
  const edited = !editsText
    ? undefined
    : planned
      ? messages && { messages }
      : title !== approval.title || body !== approval.body
        ? { title, body }
        : undefined;
  useEffect(() => {
    setStepDraft(approval.id, edited ? { title, body, messages } : null);
  });
  // The PR's description can show the task's QA screenshots.
  const [images, setImages] = useState<DeliveryReplyImage[]>(approval.images ?? []);
  const [shots, setShots] = useState<DeliveryQaScreenshot[]>([]);
  useEffect(() => {
    if (!choosesReviewers) return;
    let live = true;
    getDeliveryQaScreenshots(task.id).then(
      (found) => live && setShots(found),
      () => live && setShots([]),
    );
    return () => {
      live = false;
    };
  }, [choosesReviewers, task.id]);
  const consequence = approval.record
    ? `Al aprobar, Tinto toma estos commits, que ya están en ${task.branch}, como los de la tarea. No crea ni mueve nada, y sus comprobaciones, revisión y QA no se dan por hechas.`
    : approval.rung === "commit"
      ? `Al aprobar, Tinto crea ${(approval.commits?.length ?? 1) > 1 ? "estos commits, en este orden," : "el commit"} en ${task.branch} con estos textos exactos y sin añadir firmas. Si uno falla, se detiene. No publica nada: el push se aprueba aparte, y hasta entonces puedes deshacerlo.`
      : approval.rung === "sync"
        ? `Al aprobar, Tinto trae la base y la fusiona en ${task.branch} con este mensaje: un commit de merge, sin publicar. Si hay conflictos, el merge queda en curso, un job de implementación los resuelve y el paso commit lo concluye.`
        : approval.rung === "push"
          ? `Al aprobar, Tinto publica la rama ${task.branch}.`
          : updatesPr
            ? `El PR #${updatesPr.number} ya está abierto: al aprobar, Tinto le pone este título y cuerpo exactos y pide la revisión a quienes elijas. No abre otro PR.`
            : approval.rung === "pr"
              ? `Al aprobar, Tinto abre en GitHub el PR de ${task.branch} con este título y cuerpo exactos. Desde entonces lo sigue y pide la revisión a quienes elijas.${reviewerOptional ? "" : " Sin una revisión aprobada no se podrá fusionar."}`
              : approval.rung === "merge"
                ? `Al aprobar, Tinto comprueba que la CI pasó en el último commit del PR y que la base no avanzó, fusiona el PR en GitHub (${approval.merge_method ?? "squash"}) con este mensaje y borra allí la rama ${task.branch}. La rama local sigue hasta que elimines la tarea.`
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
      {pending && approval.rung === "merge" && pr && <MergeChecks pr={pr} now={now} />}
      {approval.rung === "pr" && pr && ciLine(pr) && (
        <p
          className={pr.ci === "passing" ? "delivery-muted" : "delivery-tone--danger"}
          aria-label="CI del PR"
        >
          {ciLine(pr)}
        </p>
      )}
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
      {editsText && !planned ? (
        <div className="delivery-step-text">
          <input
            aria-label="Título"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
          />
          <GrowingTextarea
            aria-label="Cuerpo"
            rows={3}
            value={body}
            onChange={(event) => setBody(event.target.value)}
          />
        </div>
      ) : approval.record ? (
        <ol className="delivery-commit-plan" aria-label="Commits a registrar">
          {approval.body.split("\n").map((line) => (
            <li key={line}>
              <code>{line}</code>
            </li>
          ))}
        </ol>
      ) : approval.rung === "commit" ? (
        <>
          <CommitPlanView
            approval={approval}
            messages={messages}
            onMessages={editsText ? setMessages : undefined}
          />
          {approval.single_commit_reason && (
            <p className="delivery-job__note">
              Un solo commit porque: {approval.single_commit_reason}
            </p>
          )}
          {approval.author && (
            <p className="delivery-job__note" aria-label="Autor de los commits">
              Autor: {approval.author}
            </p>
          )}
          {pending && approval.notice && <p className="delivery-tone--danger">{approval.notice}</p>}
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
          {!fresh && !candidatesError && (
            <p className="delivery-muted">
              {candidates === null
                ? "Leyendo quién puede revisar en GitHub…"
                : "La última lista leída; comprobándola en GitHub…"}
            </p>
          )}
          {candidatesError && (
            <p className="delivery-job__note">
              No se pudo leer quién puede revisar: {candidatesError}.
              {candidates !== null && " Es la última lista leída."}{" "}
              <button
                type="button"
                onClick={() => {
                  setCandidatesError(null);
                  setAttempt((count) => count + 1);
                }}
              >
                Reintentar
              </button>
            </p>
          )}
          {fresh && candidates?.length === 0 && (
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
      {choosesReviewers && (
        <QaImagePicker
          shots={shots}
          chosen={images}
          onChange={setImages}
          where="en la descripción del PR"
        />
      )}
      {pending && (
        <details className="delivery-step-consequence">
          <summary>Qué pasa al aprobar o al rechazar</summary>
          <p>{consequence}</p>
          <p>{rejectionScope(approval, task, pr)}</p>
        </details>
      )}
      {pending ? (
        <ActionGroup>
          <div className="delivery-actions">
            <ActionButton
              className="delivery-button--primary"
              disabled={reviewerHold !== null}
              title={reviewerHold ?? undefined}
              onAction={() =>
                choosesReviewers
                  ? decideDeliveryApproval(
                      approval.id,
                      true,
                      undefined,
                      reviewers,
                      undefined,
                      images,
                      ...(edited ? [edited] : []),
                    )
                  : edited
                    ? decideDeliveryApproval(
                        approval.id,
                        true,
                        undefined,
                        undefined,
                        undefined,
                        undefined,
                        edited,
                      )
                    : decideDeliveryApproval(approval.id, true)
              }
            >
              {approveLabel(approval, pr)}
            </ActionButton>
            <ActionButton
              title={rejectionScope(approval, task, pr)}
              onAction={() => decideDeliveryApproval(approval.id, false)}
            >
              Rechazar
            </ActionButton>
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
        <GrowingTextarea value={body} onChange={(event) => setBody(event.target.value)} rows={3} />
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

/** The history's line beside its title: how many rounds and the latest's cause. */
function historyGist(entries: HistoryEntry[]): string {
  // The last one opened (a resumed round comes back on top); counted to the highest number.
  const last = entries[0].round;
  const count = Math.max(...entries.map((entry) => entry.round?.number ?? 1));
  return last ? `${count} rondas · última: ${last.short}` : "Entrega inicial";
}

/** Rounds shown open, then as one line each, before the rest fold away. */
const ROUNDS_OPEN = 2;
const ROUNDS_LISTED = 4;

/** The task's history by rounds, newest first: the current round and the one
 *  before it open, the next two on a line each, older ones behind one
 *  disclosure, the initial delivery last (open when it is all there is). */
function TaskHistory({
  taskId,
  entries,
  now,
  renderRow,
}: {
  taskId: string;
  /** Newest first, the initial delivery last. */
  entries: HistoryEntry[];
  now: number;
  /** A row; `id` is the stage's anchor on its most recent row. */
  renderRow: (row: HistoryRow, id: string | undefined) => ReactNode;
}) {
  const rounds = entries.slice(0, -1);
  const initial = entries[entries.length - 1];
  // The stage track scrolls to the row of the stage's latest job.
  const entry = (shown: HistoryEntry, open: boolean) => (
    <HistoryRound
      key={shown.round?.id ?? "initial"}
      entry={shown}
      open={open}
      now={now}
      rows={shown.rows.map((row) =>
        renderRow(row, row.latest ? `delivery-stage-${taskId}-${row.job.role}` : undefined),
      )}
    />
  );
  const older = rounds.slice(ROUNDS_LISTED);
  return (
    <div className="delivery-rounds">
      {rounds.slice(0, ROUNDS_LISTED).map((round, at) => entry(round, at < ROUNDS_OPEN))}
      {older.length > 0 && (
        <details className="delivery-fold delivery-round">
          <summary>
            {older.length === 1 ? "1 ronda anterior" : `${older.length} rondas anteriores`}
          </summary>
          <div className="delivery-rounds">{older.map((round) => entry(round, false))}</div>
        </details>
      )}
      {entry(initial, rounds.length === 0)}
    </div>
  );
}

/** One round (or the initial delivery): its heading on one line, and when
 *  open its cause, what it had to address, its stages and how it ended. */
function HistoryRound({
  entry,
  open,
  now,
  rows,
}: {
  entry: HistoryEntry;
  open: boolean;
  now: number;
  rows: ReactNode[];
}) {
  const { round, from, to } = entry;
  const going = to === null;
  let when: string | null = null;
  if (from !== null) {
    const at = whenLabel(from, now);
    when = going
      ? `en marcha · desde ${at.startsWith("hoy ") ? `las ${at.slice(4)}` : at}`
      : `${at} · ${spanLabel(to - from)}`;
  }
  const meta = [round?.short, when].filter(Boolean).join(" · ");
  return (
    <details className="delivery-fold delivery-round" open={open}>
      <summary>
        <span className="delivery-round__title">
          {round ? `Ronda ${round.number}` : "Entrega inicial"}
        </span>
        {meta && <span className="delivery-round__meta">{meta}</span>}
      </summary>
      <div className="delivery-round__body">
        {round && round.cause !== round.short && (
          <p className="delivery-round__cause">{round.cause}</p>
        )}
        {round && round.items.length > 0 && (
          <details className="delivery-task__fold">
            <summary>
              {going ? "Qué atiende" : "Qué tenía que atender"} · {round.items.length}
            </summary>
            <ol>
              {round.items.map((item, at) => (
                <li key={at}>{item}</li>
              ))}
            </ol>
          </details>
        )}
        {rows.length > 0 ? (
          <ul className="delivery-jobs">{rows}</ul>
        ) : (
          <p className="delivery-muted">Todavía no ha corrido ninguna etapa.</p>
        )}
        {round?.outcome && <p className="delivery-round__outcome">→ {round.outcome}</p>}
      </div>
    </details>
  );
}

/** The CSS tone of an outcome in the history: findings still open read in
 *  the text's colour (the coordinator's to answer, not amber). */
function historyTone(tone: Tone): string {
  return tone === "attention" ? "open" : tone;
}

/** A round's checks on one line, "2 de 2 pasan", opening into each command. */
function CheckRow({
  checks,
  jobs,
  verdicts,
  now,
  expanded,
  onToggle,
  children,
}: {
  /** Each command's last run in the round. */
  checks: DeliveryJob[];
  jobs: DeliveryJob[];
  verdicts: Record<string, DeliveryJobVerdict>;
  now: number;
  expanded: boolean;
  onToggle: () => void;
  /** Every run of the round's checks, as rows. */
  children: ReactNode;
}) {
  const outcome = checksOutcome(checks, jobs, verdicts);
  const started = checks.flatMap((job) => (job.started_at_ms ? [job] : []));
  const from = Math.min(...started.map((job) => job.started_at_ms!));
  const to = Math.max(...started.map((job) => job.ended_at_ms ?? now));
  const tone = historyTone(outcome.tone);
  return (
    <li className="delivery-job">
      <button
        type="button"
        className="delivery-job__summary"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <Icon name={expanded ? "down" : "right"} />
        <span className={`delivery-job__mark delivery-tone--${tone}`}>
          <StatusMark icon={TONE_ICON[outcome.tone]} />
        </span>
        <span className="delivery-job__role">Comprobaciones</span>
        <span className={`delivery-outcome delivery-tone--${tone}`}>{outcome.label}</span>
        <span className="delivery-job__time">{started.length > 0 ? clockSpan(to - from) : ""}</span>
      </button>
      {expanded && (
        <div className="delivery-job__detail">
          <ul className="delivery-jobs">{children}</ul>
        </div>
      )}
    </li>
  );
}

function JobRow({
  id,
  job,
  jobs,
  verdicts,
  wait,
  now,
  attempts,
  expanded,
  undoable,
  onToggle,
  onUndo,
  onContinue,
  children,
}: {
  /** The stage's anchor, on its most recent row. */
  id?: string;
  /** A stage's row: how many attempts it had in the round. Without it, one
   *  attempt among others, named by its number. */
  attempts?: number;
  /** Shown at the end of the opened row: its earlier attempts. */
  children?: ReactNode;
  job: DeliveryJob;
  /** Every job of the task, oldest first. */
  jobs: DeliveryJob[];
  verdicts: Record<string, DeliveryJobVerdict>;
  /** Why it has not started, while it is queued. */
  wait?: DeliveryQueueWait;
  now: number;
  expanded: boolean;
  undoable: boolean;
  onToggle: () => void;
  onUndo: () => Promise<unknown>;
  onContinue: () => Promise<unknown>;
}) {
  const outcome = attemptOutcome(job, jobs, verdicts);
  const quiet = quietFor(job, now);
  const held = pausedJob(jobs)?.id === job.id;
  const scope =
    (job.access === "full" ? " · acceso completo" : job.writes ? "" : " · solo lectura") +
    (job.qa_browser ? " · navegador" : "") +
    (job.ran_on ? ` · en ${job.ran_on}` : "");
  const tone = historyTone(outcome.tone);
  return (
    <li id={id} data-job={job.id} className="delivery-job">
      <button
        type="button"
        className="delivery-job__summary"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <Icon name={expanded ? "down" : "right"} />
        <span className={`delivery-job__mark delivery-tone--${tone}`}>
          <StatusMark icon={TONE_ICON[outcome.tone]} />
        </span>
        <span className="delivery-job__role">
          {stageLabel(job.role)} {byCommand(job) && <code>{job.prompt}</code>}{" "}
          <small>
            {attempts === undefined
              ? `· intento ${job.attempt}`
              : attempts > 1
                ? `· ${attempts} intentos`
                : ""}
          </small>
        </span>
        <span className={`delivery-outcome delivery-tone--${tone}`}>{outcome.label}</span>
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
              <small className="delivery-muted">{quiet ?? ago(job.activity_at_ms, now)}</small>
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
              <small className="delivery-muted">· {wait.warning}</small>
            </>
          )}
        </p>
      )}
      {expanded && (
        <div className="delivery-job__detail">
          {/* Who ran it, and where its time went. */}
          <p className="delivery-job__who">
            {agentName(job.agent)}
            {job.agent === "shell" ? "" : ` · ${runtimeLabel(job)}`}
            {scope}
            {` · intento ${job.attempt}`}
            {job.started_at_ms ? ` · empezó ${whenLabel(job.started_at_ms, now)}` : ""}
          </p>
          {job.timing && (job.timing.model_ms !== null || job.timing.commands_ms > 0) && (
            <p className="delivery-job__timing" aria-label={`Tiempo de ${stageLabel(job.role)}`}>
              {timingLabel(job.timing)}
            </p>
          )}
          {job.error && <p className="delivery-job__error">{job.error}</p>}
          {job.prepare && (
            <p className="delivery-job__note">
              Entorno de QA preparado en {lasted(job.prepare.ms)}: {job.prepare.note}
            </p>
          )}
          {(job.reverted ?? []).length > 0 && (
            <p className="delivery-job__note">
              Tinto deshizo lo que {job.role === "qa" ? "la QA" : "el comando"} cambió en el
              worktree: {job.reverted!.join(", ")}.
            </p>
          )}
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
            <p className="delivery-job__note">Sin registro visual: {job.visual_record_error}</p>
          )}
          {job.visual_record_source && (
            <p className="delivery-muted">
              Registro de QA tomado de {fileName(job.visual_record_source)}
            </p>
          )}
          {verdicts[job.id]?.superseded && (
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
                    <Icon
                      name={
                        item.status === "completed"
                          ? "check"
                          : item.status === "in_progress"
                            ? "right"
                            : "ring"
                      }
                    />
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
              {job.role === "qa" && !isOpen(job) && job.status !== "paused" && (
                <ActionButton
                  title="Busca otra vez el registro en la carpeta de QA del trabajo y lo vuelve a generar"
                  onAction={() => regenerateDeliveryVisualRecord(job.id)}
                >
                  Regenerar registro
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
                <ActionButton onAction={() => retryJobOnce(job.id)}>Nuevo intento</ActionButton>
              )}
              {undoable && <ActionButton onAction={onUndo}>Deshacer estos cambios</ActionButton>}
              {!isOpen(job) && !held && job.agent !== "shell" && job.provider_session_id && (
                <ActionButton onAction={onContinue}>Continuar en Agents</ActionButton>
              )}
            </div>
          </ActionGroup>
          {children}
        </div>
      )}
    </li>
  );
}

function JobLog({ job }: { job: DeliveryJob }) {
  const log = useJobLog(job);
  const live = isOpen(job);
  if (!log || (log.entries.length === 0 && !log.stderr_tail.trim())) return null;
  return (
    <details className="delivery-log" open={live}>
      <summary>Registro</summary>
      {log.skipped > 0 && (
        <p className="delivery-job__note">
          {log.skipped} líneas anteriores no se muestran; las últimas {MAX_LOG_ENTRIES} sí.
        </p>
      )}
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
