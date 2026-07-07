import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import type { IDockviewPanelProps } from "dockview-react";
import { confirm } from "@tauri-apps/plugin-dialog";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  getAgentJournalSession,
  listAgentSessions,
  revertSession,
  revertSessionTurnFile,
  restoreSessionTurn,
  runAgentHostCommand,
  stopAgentSession,
  writeAgentSessionInput,
} from "../../bus/client";
import {
  agentSessionStore,
  useAgentSession,
  useAgentSessionOutput,
  useAgentSessionTimeline,
} from "../../agent/sessionStore";
import type {
  AgentReviewFinding,
  AgentReviewSummary,
  AgentSession,
  AgentSessionRuntimeOptions,
  AgentSessionOutput,
  AgentSessionTimelineItem,
  FileDiff,
  RepoStatus,
} from "../../bus/contract";
import { useBusState } from "../../bus/store";
import codexLogo from "../../assets/agents/codex.svg";
import claudeLogo from "../../assets/agents/claude.svg";
import opencodeLogo from "../../assets/agents/opencode.svg";
import { useWorkspaceActions } from "../../workspace/actions";
import { consoleDock } from "../../workspace/consoleDock";
import { consumeTerminalDetachedMarker } from "./detachTerminalWindow";

export interface TerminalPanelParams {
  sessionId: string;
  repo?: string;
  agentType?: string;
  mode?: "live" | "journal";
}

type TerminalPanelProps = IDockviewPanelProps<TerminalPanelParams>;

const panelCloseStopTimers = new Map<string, number>();
const PANEL_CLOSE_STOP_DELAY_MS = 250;
const DETACHED_TRANSFER_STOP_DELAY_MS = 5000;

const AGENT_SKILL_SHORTCUTS = [
  {
    id: "krt-interface-warden",
    label: "Interface Warden",
    title: "Design or revise a distinctive working-surface interface",
  },
  {
    id: "krt-interface-inquisitor",
    label: "Interface Inquisitor",
    title: "Run an adversarial visual critique of an implemented interface",
  },
  {
    id: "krt-repo-medic",
    label: "Repo Medic",
    title: "Diagnose repository health, test hygiene, and maintenance risks",
  },
  {
    id: "krt-ci-questor",
    label: "CI Questor",
    title: "Investigate CI failures and summarize likely causes",
  },
  {
    id: "krt-gitflow-knight",
    label: "Gitflow Knight",
    title: "Prepare scoped commits on a proper feature branch",
  },
  {
    id: "krt-release-marshal",
    label: "Release Marshal",
    title: "Prepare delivery flow, pull request, and release handoff",
  },
] as const;

type AgentLensScope = "focused" | "session";
type AgentLensTab = "files" | "commands" | "timeline";
type AgentComposerCommandScope = "Codex" | "Tinto" | "Skill";
type AgentComposerCommandTrigger = "/" | "$";
type CodexModelSelection = "auto" | "gpt-5.5" | "gpt-5.4" | "gpt-5.4-mini" | "gpt-5.3-codex";
type CodexReasoningSelection = "auto" | "minimal" | "low" | "medium" | "high" | "xhigh";
type CodexSpeedSelection = "standard" | "fast";
type CodexRuntimeMenu = "reasoning" | "model" | "speed" | null;
type AgentComposerHostCommand =
  | "branch"
  | "comments"
  | "compact"
  | "details"
  | "feedback"
  | "fork"
  | "goal"
  | "init"
  | "mcp"
  | "mascot"
  | "personality"
  | "plan"
  | "review"
  | "status";

interface AgentComposerCommand {
  id: string;
  command: string;
  description: string;
  disabled: boolean;
  label: string;
  aliases?: string[];
  prompt?: string;
  scope: AgentComposerCommandScope;
  trigger: AgentComposerCommandTrigger;
  hostCommand?: AgentComposerHostCommand;
  runtimeCommand?: "model" | "reasoning" | "fast";
}

const CODEX_HOST_COMMANDS: Array<{
  id: AgentComposerHostCommand;
  command: string;
  description: string;
  label: string;
  aliases?: string[];
  prompt?: string;
  scope: AgentComposerCommandScope;
}> = [
  {
    id: "branch",
    command: "branch",
    label: "Bifurcar",
    description: "Bifurca este chat en local o en un worktree nuevo",
    aliases: ["fork-worktree", "bifurcar-worktree", "worktree"],
    scope: "Tinto",
  },
  {
    id: "comments",
    command: "comments",
    label: "Comentarios",
    description: "Enviar comentarios sobre este chat",
    aliases: ["comment", "comentarios", "comentario"],
    scope: "Tinto",
  },
  {
    id: "compact",
    command: "compact",
    label: "Compactar",
    description: "Compactar el contexto de este hilo",
    aliases: ["compactar"],
    scope: "Tinto",
  },
  {
    id: "status",
    command: "status",
    label: "Estado",
    description: "Mostrar el ID del chat, estado, uso y runtime",
    aliases: ["estado"],
    scope: "Tinto",
  },
  {
    id: "init",
    command: "init",
    label: "Inicializar",
    description: "Crear o actualizar AGENTS.md para Tinto",
    aliases: ["initialize", "inicializar"],
    scope: "Tinto",
  },
  {
    id: "fork",
    command: "fork",
    label: "Lateral",
    aliases: ["lateral", "bifurcar", "parallel"],
    description: "Iniciar una conversación paralela aislada",
    scope: "Tinto",
  },
  {
    id: "mcp",
    command: "mcp",
    label: "MCP",
    description: "Mostrar estado del servidor MCP",
    scope: "Tinto",
  },
  {
    id: "mascot",
    command: "mascot",
    label: "Mascota",
    description: "Despertar u ocultar la mascota de escritorio",
    scope: "Tinto",
  },
  {
    id: "plan",
    command: "plan",
    label: "Modo plan",
    aliases: ["mode-plan", "plan-mode", "modo-plan"],
    description: "Activar modo plan para el siguiente turno",
    scope: "Tinto",
  },
  {
    id: "goal",
    command: "goal",
    label: "Objetivo",
    aliases: ["objective", "objetivo"],
    description: "Establecer un objetivo hacia el que Codex seguirá trabajando",
    scope: "Tinto",
  },
  {
    id: "personality",
    command: "personality",
    label: "Personalidad",
    aliases: ["personalidad"],
    description: "Elegir cómo responde Codex",
    scope: "Tinto",
  },
  {
    id: "review",
    command: "review",
    aliases: ["code-review", "revision", "revision-codigo", "revision-de-codigo"],
    label: "Revisión de código",
    description: "Revisar cambios no preparados o comparar cambios",
    scope: "Tinto",
  },
  {
    id: "feedback",
    command: "feedback",
    label: "Feedback",
    aliases: ["commentary", "comentarios-feedback"],
    description: "Enviar feedback sobre esta sesión",
    scope: "Tinto",
  },
];

const CODEX_MODEL_OPTIONS: Array<{
  value: CodexModelSelection;
  label: string;
  description: string;
}> = [
  { value: "auto", label: "Auto", description: "Use the configured Codex default model" },
  { value: "gpt-5.5", label: "GPT-5.5", description: "Best default for complex agent work" },
  { value: "gpt-5.4", label: "GPT-5.4", description: "Strong general coding and reasoning" },
  { value: "gpt-5.4-mini", label: "GPT-5.4 mini", description: "Faster scans and small edits" },
  {
    value: "gpt-5.3-codex",
    label: "GPT-5.3 Codex",
    description: "Codex-focused compatibility option",
  },
];

const CODEX_REASONING_OPTIONS: Array<{
  value: CodexReasoningSelection;
  label: string;
  description: string;
}> = [
  { value: "auto", label: "Auto", description: "Let Codex choose from the model default" },
  { value: "minimal", label: "Minimo", description: "Shortest reasoning budget" },
  { value: "low", label: "Bajo", description: "Fast, lightweight reasoning" },
  { value: "medium", label: "Medio", description: "Balanced default for implementation work" },
  { value: "high", label: "Alto", description: "Deeper reasoning for risky changes" },
  { value: "xhigh", label: "Extremadamente alto", description: "Maximum effort when supported" },
];

const CODEX_SPEED_OPTIONS: Array<{
  value: CodexSpeedSelection;
  label: string;
  description: string;
}> = [
  { value: "standard", label: "Normal", description: "Respect the selected model and reasoning" },
  {
    value: "fast",
    label: "Velocidad",
    description: "Prefer fast defaults when model or reasoning are not fixed",
  },
];

const AGENT_LENS_TAB_ORDER: AgentLensTab[] = ["files", "commands", "timeline"];
const COMPOSER_COMMAND_TRIGGER_RE = /(^|\n)([/$])([^\s/$]*)$/;
const COMPOSER_COMMAND_LINE_RE = /(^|\n)([/$])([^\s/$]*)(?:[^\n]*)$/;

interface AgentTurnView {
  id: string;
  index: number;
  startedAtMs: number | null;
  updatedAtMs: number | null;
  userText: string | null;
  agentText: string[];
  commandText: string[];
  systemText: string[];
  changes: Array<{ path: string; kind: string }>;
  restoreCheckpointId: string | null;
  restoreReady: boolean;
}

interface AgentReviewResponseView {
  turnIndex: number;
  text: string;
  excerpt: string;
}

interface AgentSessionOverviewView {
  turns: number;
  messages: number;
  commands: number;
  files: number;
  latest: string | null;
  turnMap: Array<{
    id: string;
    index: number;
    commands: number;
    commandSummary: string | null;
    files: number;
    active: boolean;
    timeLabel: string | null;
  }>;
}

export function TerminalPanel({ params }: TerminalPanelProps) {
  const sessionId = params?.sessionId ?? "";
  const repo = params?.repo;
  const agentType = params?.agentType ?? "agent";
  const mode = params?.mode ?? "live";
  const { openFile } = useWorkspaceActions();
  const readOnly = mode === "journal";
  const session = useAgentSession(sessionId);
  const { chunks: sessionOutput } = useAgentSessionOutput(sessionId);
  const timeline = useAgentSessionTimeline(sessionId);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [transcriptQuery, setTranscriptQuery] = useState("");
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [reverting, setReverting] = useState(false);
  const [revertingFile, setRevertingFile] = useState<string | null>(null);
  const [restoringTurnId, setRestoringTurnId] = useState<string | null>(null);
  const [focusedTurnIndex, setFocusedTurnIndex] = useState<number | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [slashMenuOpen, setSlashMenuOpen] = useState(false);
  const [activeSlashCommandIndex, setActiveSlashCommandIndex] = useState(0);
  const [runtimeMenu, setRuntimeMenu] = useState<CodexRuntimeMenu>(null);
  const [selectedModel, setSelectedModel] = useState<CodexModelSelection>("auto");
  const [selectedReasoning, setSelectedReasoning] = useState<CodexReasoningSelection>("auto");
  const [selectedSpeed, setSelectedSpeed] = useState<CodexSpeedSelection>("standard");
  const [runtimeNotice, setRuntimeNotice] = useState<string | null>(null);
  const [mascotAwake, setMascotAwake] = useState(false);
  const [reviewSummary, setReviewSummary] = useState<AgentReviewSummary | null>(null);
  const [reviewFindings, setReviewFindings] = useState<AgentReviewFinding[]>([]);
  const [reviewPromptDraft, setReviewPromptDraft] = useState<string | null>(null);
  const [reviewPromptState, setReviewPromptState] = useState<"drafted" | "sent" | null>(null);
  const composerInputRef = useRef<HTMLTextAreaElement | null>(null);
  const transcriptSearchRef = useRef<HTMLInputElement | null>(null);

  const turns = useMemo(
    () => agentTurns(timeline, sessionOutput, session),
    [session, sessionOutput, timeline],
  );
  const overview = useMemo(() => agentSessionOverview(turns), [turns]);
  const visibleTurns = useMemo(
    () => filterAgentTurns(turns, transcriptQuery),
    [transcriptQuery, turns],
  );
  const validFocusedTurnIndex =
    focusedTurnIndex != null && turns.some((turn) => turn.index === focusedTurnIndex)
      ? focusedTurnIndex
      : null;
  const focusedTurn = useMemo(
    () =>
      validFocusedTurnIndex != null
        ? (turns.find((turn) => turn.index === validFocusedTurnIndex) ?? null)
        : (turns[turns.length - 1] ?? null),
    [turns, validFocusedTurnIndex],
  );
  const reviewResponse = useMemo(
    () =>
      reviewPromptDraft && reviewPromptState === "sent"
        ? reviewResponseForPrompt(turns, reviewPromptDraft)
        : null,
    [reviewPromptDraft, reviewPromptState, turns],
  );
  const reviewPromptTurnIndex = useMemo(
    () =>
      reviewPromptDraft && reviewPromptState === "sent"
        ? reviewPromptTurnIndexForPrompt(turns, reviewPromptDraft)
        : null,
    [reviewPromptDraft, reviewPromptState, turns],
  );
  const sessionRepo = session?.repo ?? repo;
  const hasTranscriptQuery = transcriptQuery.trim().length > 0;
  const canNavigateSearchResults = hasTranscriptQuery && visibleTurns.length > 1;
  const activeSearchResultIndex = hasTranscriptQuery
    ? visibleTurns.findIndex((turn) => turn.index === focusedTurnIndex)
    : -1;
  const activeSearchResultLabel = hasTranscriptQuery
    ? activeSearchResultIndex >= 0
      ? `${activeSearchResultIndex + 1} / ${visibleTurns.length}`
      : `- / ${visibleTurns.length}`
    : null;
  const transcriptSearchCountDescription = hasTranscriptQuery
    ? `${visibleTurns.length} matching ${turnNoun(visibleTurns.length)} out of ${turns.length} total ${turnNoun(turns.length)}.`
    : `Showing all ${turns.length} transcript ${turnNoun(turns.length)}.`;
  const activeSearchResultDescription = activeSearchResultLabel
    ? activeSearchResultIndex >= 0
      ? `Focused search result ${activeSearchResultIndex + 1} of ${visibleTurns.length} matching ${turnNoun(visibleTurns.length)}.`
      : `No focused search result selected out of ${visibleTurns.length} matching ${turnNoun(visibleTurns.length)}.`
    : null;
  const previousSearchResultTitle = transcriptSearchNavigationTitle(
    "previous",
    hasTranscriptQuery,
    visibleTurns.length,
  );
  const nextSearchResultTitle = transcriptSearchNavigationTitle(
    "next",
    hasTranscriptQuery,
    visibleTurns.length,
  );
  const transcriptCopyTitle = transcriptCopyButtonTitle(
    hasTranscriptQuery,
    visibleTurns.length,
    turns.length,
  );
  const latestTurnTitle = latestTurnButtonTitle(
    hasTranscriptQuery,
    visibleTurns.length,
    turns.length,
  );
  const canCompose =
    !!sessionId &&
    !readOnly &&
    !sending &&
    session?.status !== "completed" &&
    session?.status !== "failed" &&
    session?.status !== "reverted" &&
    session?.status !== "error";
  const canSend = canCompose && draft.trim().length > 0;
  const canRestoreTurn =
    !!sessionId &&
    !readOnly &&
    session?.status !== "running" &&
    session?.status !== "starting" &&
    session?.status !== "reverted" &&
    session?.status !== "error";
  const isCodexSession = agentType === "codex";
  const composerCommandTrigger = readComposerCommandTrigger(draft);
  const composerCommandQuery = composerCommandTrigger?.query ?? "";
  const composerCommandItems = useMemo<AgentComposerCommand[]>(
    () => [
      ...CODEX_HOST_COMMANDS.map((command) => ({
        id: command.id,
        command: command.command,
        description: command.description,
        disabled: !canCompose || (command.id === "details" && !session),
        hostCommand: command.prompt ? undefined : command.id,
        label: command.label,
        aliases: command.aliases,
        prompt: command.prompt,
        scope: command.scope,
        trigger: "/" as const,
      })),
      {
        id: "model",
        command: "model",
        description: "Choose the active Codex model",
        disabled: !canCompose || !isCodexSession,
        label: "Model",
        aliases: ["modelo"],
        runtimeCommand: "model" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "reasoning",
        command: "reasoning",
        description: "Choose Codex reasoning effort",
        disabled: !canCompose || !isCodexSession,
        label: "Reasoning",
        aliases: ["razonamiento"],
        runtimeCommand: "reasoning" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "effort",
        command: "effort",
        description: "Alias for reasoning effort",
        disabled: !canCompose || !isCodexSession,
        label: "Effort",
        aliases: ["razonamiento"],
        runtimeCommand: "reasoning" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "fast",
        command: "fast",
        description: "Toggle the fast Codex preset",
        disabled: !canCompose || !isCodexSession,
        label: "Fast",
        aliases: ["speed", "velocidad", "rapido", "rapida"],
        runtimeCommand: "fast" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "test",
        command: "test",
        description: "Run the most relevant verification for this repo",
        disabled: !canCompose,
        label: "Test",
        prompt:
          "Run the most relevant verification for this repo and summarize failures before fixing them.",
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "handoff",
        command: "handoff",
        description: "Summarize session state, changes, verification, and next step",
        disabled: !canCompose,
        label: "Handoff",
        prompt:
          "Summarize the current session state, changed files, verification, and next recommended step.",
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "details",
        command: "details",
        description: "Open session details, files, commands, timeline, and restore points",
        disabled: !session,
        label: "Details",
        scope: "Tinto",
        trigger: "/" as const,
      },
      ...AGENT_SKILL_SHORTCUTS.map((skill) => ({
        id: skill.id,
        command: skill.id,
        description: skill.title,
        disabled: !canCompose,
        label: skill.label,
        scope: "Skill" as const,
        trigger: "$" as const,
      })),
    ],
    [canCompose, isCodexSession, session],
  );
  const filteredComposerCommandItems = useMemo(
    () => filterComposerCommands(composerCommandItems, composerCommandTrigger),
    [composerCommandItems, composerCommandTrigger],
  );
  const commandMenuVisible = slashMenuOpen && Boolean(composerCommandTrigger) && canCompose;
  const effectiveRuntimeOptions = useMemo(
    () => codexRuntimeOptions(selectedModel, selectedReasoning, selectedSpeed),
    [selectedModel, selectedReasoning, selectedSpeed],
  );

  useEffect(() => {
    if (!session?.runtime_options || !isCodexSession) return;
    const restored = runtimeSelectionsFromOptions(session.runtime_options);
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled) return;
      setSelectedModel(restored.model);
      setSelectedReasoning(restored.reasoning);
      setSelectedSpeed(restored.speed);
    });
    return () => {
      cancelled = true;
    };
  }, [isCodexSession, session?.id, session?.runtime_options]);

  const focusVisibleTurn = (direction: "previous" | "next") => {
    if (visibleTurns.length === 0) return;
    const currentIndex = visibleTurns.findIndex((turn) => turn.index === validFocusedTurnIndex);
    const nextIndex =
      direction === "next"
        ? (currentIndex + 1) % visibleTurns.length
        : currentIndex <= 0
          ? visibleTurns.length - 1
          : currentIndex - 1;
    const nextTurn = visibleTurns[nextIndex];
    if (!nextTurn) return;
    setFocusedTurnIndex(nextTurn.index);
    scrollToAgentTurn(sessionId, nextTurn.index, "center");
  };

  const resetTranscriptSearch = ({ focusSearch = false }: { focusSearch?: boolean } = {}) => {
    setFocusedTurnIndex(null);
    setTranscriptQuery("");
    if (focusSearch) {
      transcriptSearchRef.current?.focus();
    }
  };

  const onTranscriptSearchKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape" && hasTranscriptQuery) {
      event.preventDefault();
      resetTranscriptSearch();
      return;
    }
    if (event.key !== "Enter" || !canNavigateSearchResults) return;
    event.preventDefault();
    focusVisibleTurn(event.shiftKey ? "previous" : "next");
  };

  useEffect(() => {
    if (!sessionId) return;
    cancelPanelCloseStop(sessionId);
    let active = true;
    const loadSession =
      mode === "journal"
        ? getAgentJournalSession(sessionId).then((session) => (session ? [session] : []))
        : listAgentSessions();
    void loadSession
      .then((sessions) => {
        if (!active) return;
        if (mode === "journal") {
          const session = sessions[0];
          if (session) agentSessionStore.upsertSession(session);
          else setError("Session transcript was not found.");
        } else {
          agentSessionStore.setSessions(sessions);
        }
      })
      .catch((e) => {
        if (active) setError(commandMessage(e));
      });
    return () => {
      active = false;
      if (readOnly) return;
      schedulePanelCloseStop(
        sessionId,
        consumeTerminalDetachedMarker(sessionId)
          ? DETACHED_TRANSFER_STOP_DELAY_MS
          : PANEL_CLOSE_STOP_DELAY_MS,
      );
    };
  }, [mode, readOnly, sessionId]);

  const canRevert =
    !!session &&
    !readOnly &&
    !!session.checkpoint &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const canRevertTurnFile =
    !!session &&
    !readOnly &&
    session.status !== "running" &&
    session.status !== "starting" &&
    session.status !== "reverted";

  const canStop =
    !!session &&
    !readOnly &&
    !stopping &&
    (session.status === "running" || session.status === "starting");

  const sendDraft = async () => {
    if (!canSend) return;
    const text = draft.trimEnd();
    const slashCommandHandled =
      isCodexSession &&
      applyCodexRuntimeSlashCommand(text, {
        setModel: setSelectedModel,
        setReasoning: setSelectedReasoning,
        setSpeed: setSelectedSpeed,
        setNotice: setRuntimeNotice,
      });
    if (slashCommandHandled) {
      setDraft("");
      setSlashMenuOpen(false);
      return;
    }
    if (await applyComposerSlashCommand(text)) {
      return;
    }
    setSending(true);
    setError(null);
    try {
      await writeAgentSessionInput(sessionId, `${text}\r`, effectiveRuntimeOptions);
      if (reviewPromptDraft && text.trim() === reviewPromptDraft.trim()) {
        setReviewPromptState("sent");
      }
      setDraft("");
      setRuntimeNotice(null);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setSending(false);
    }
  };

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    void sendDraft();
  };

  const onDraftKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (commandMenuVisible) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setActiveSlashCommandIndex((current) =>
          nextComposerCommandIndex(
            current,
            filteredComposerCommandItems.length,
            event.key === "ArrowDown",
          ),
        );
        return;
      }
      if (
        (event.key === "Enter" || event.key === "Tab") &&
        filteredComposerCommandItems.length > 0
      ) {
        event.preventDefault();
        applyComposerCommand(
          filteredComposerCommandItems[activeSlashCommandIndex] ?? filteredComposerCommandItems[0],
        );
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setSlashMenuOpen(false);
        return;
      }
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    void sendDraft();
  };

  const onDraftChange = (value: string) => {
    setDraft(value);
    setSlashMenuOpen(canCompose && Boolean(readComposerCommandTrigger(value)));
  };

  const insertComposerPrompt = (prompt: string) => {
    if (!canCompose) return;
    setDraft((current) => replaceDraftComposerCommand(current, prompt));
    setSlashMenuOpen(false);
    window.setTimeout(() => composerInputRef.current?.focus(), 0);
  };

  const insertSkillMention = (skillName: string) => {
    if (!canCompose) return;
    setDraft((current) => replaceDraftComposerCommand(current, `$${skillName} `));
    setSlashMenuOpen(false);
    window.setTimeout(() => composerInputRef.current?.focus(), 0);
  };

  const clearComposerCommand = () => {
    setDraft((current) => clearDraftComposerCommand(current));
    setSlashMenuOpen(false);
    window.setTimeout(() => composerInputRef.current?.focus(), 0);
  };

  const draftRuntimeCommand = (command: "model" | "reasoning") => {
    if (!canCompose) return;
    setDraft((current) => replaceDraftComposerCommand(current, `/${command} `));
    setSlashMenuOpen(false);
    window.setTimeout(() => composerInputRef.current?.focus(), 0);
  };

  const toggleFastPreset = () => {
    setSelectedSpeed((current) => {
      const next = current === "fast" ? "standard" : "fast";
      setRuntimeNotice(next === "fast" ? "Fast preset enabled." : "Fast preset disabled.");
      return next;
    });
    clearComposerCommand();
  };

  const executeHostCommand = async (command: AgentComposerHostCommand, argument?: string) => {
    if (!sessionId || !canCompose) return;
    if (command === "mascot") {
      setMascotAwake((current) => {
        const next = !current;
        setRuntimeNotice(next ? "Mascot awake in this agent panel." : "Mascot hidden.");
        return next;
      });
      clearComposerCommand();
      return;
    }
    if (command === "details") {
      setDetailsOpen(true);
    }
    setSending(true);
    setError(null);
    try {
      const result = await runAgentHostCommand(sessionId, command, argument);
      setRuntimeNotice(result.message);
      if (result.review_summary) {
        setReviewSummary(result.review_summary);
        setReviewFindings(result.review_findings ?? []);
        setReviewPromptDraft(null);
        setReviewPromptState(null);
        setCopiedTarget((current) => (isReviewClipboardTarget(current) ? null : current));
        setDetailsOpen(true);
      } else if (command === "review") {
        setReviewSummary(null);
        setReviewFindings([]);
        setReviewPromptDraft(null);
        setReviewPromptState(null);
        setCopiedTarget((current) => (isReviewClipboardTarget(current) ? null : current));
      }
      if (result.status === "completed") {
        const sessions = await listAgentSessions();
        agentSessionStore.setSessions(sessions);
        if (result.session_id) {
          consoleDock.openTerminal({
            sessionId: result.session_id,
            repo: result.repo ?? sessionRepo,
            agentType: result.agent_type ?? agentType,
          });
        }
      }
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setSending(false);
      clearComposerCommand();
    }
  };

  const applyComposerSlashCommand = async (text: string): Promise<boolean> => {
    const match = text.trim().match(/^\/([^\s/]+)(?:\s+([\s\S]+))?$/);
    if (!match) return false;
    const commandName = normalizeComposerCommandToken(match[1]);
    const argument = match[2]?.trim();
    if (isDeferredMemoryCommand(commandName)) {
      setRuntimeNotice("Memory commands are deferred for the later Tinto memory plan.");
      clearComposerCommand();
      return true;
    }
    const command = composerCommandItems.find(
      (item) => item.trigger === "/" && composerCommandMatchesName(item, commandName),
    );
    if (!command || command.disabled) return false;
    if (command.prompt) {
      insertComposerPrompt(command.prompt);
      return true;
    }
    if (command.runtimeCommand || command.hostCommand || command.id === "details") {
      if (command.runtimeCommand === "model") {
        draftRuntimeCommand("model");
        return true;
      }
      if (command.runtimeCommand === "reasoning") {
        draftRuntimeCommand("reasoning");
        return true;
      }
      if (command.runtimeCommand === "fast") {
        toggleFastPreset();
        return true;
      }
      await executeHostCommand(command.hostCommand ?? "details", argument);
      return true;
    }
    return false;
  };

  const applyComposerCommand = (command: AgentComposerCommand | undefined) => {
    if (!command || command.disabled) return;
    if (command.trigger === "$") {
      insertSkillMention(command.command);
      return;
    }
    if (command.runtimeCommand === "model") {
      draftRuntimeCommand("model");
      return;
    }
    if (command.runtimeCommand === "reasoning") {
      draftRuntimeCommand("reasoning");
      return;
    }
    if (command.runtimeCommand === "fast") {
      toggleFastPreset();
      return;
    }
    if (command.prompt) {
      insertComposerPrompt(command.prompt);
      return;
    }
    if (command.hostCommand) {
      if (command.hostCommand === "plan") {
        void executeHostCommand("plan", "toggle");
        return;
      }
      if (
        command.hostCommand === "goal" ||
        command.hostCommand === "personality" ||
        command.hostCommand === "comments" ||
        command.hostCommand === "feedback"
      ) {
        setDraft((current) => replaceDraftComposerCommand(current, `/${command.hostCommand} `));
        setSlashMenuOpen(false);
        window.setTimeout(() => composerInputRef.current?.focus(), 0);
        return;
      }
      void executeHostCommand(command.hostCommand);
      return;
    }
    if (command.id === "details") {
      void executeHostCommand("details");
    }
  };

  useEffect(() => {
    let cancelled = false;
    queueMicrotask(() => {
      if (!cancelled) setActiveSlashCommandIndex(0);
    });
    return () => {
      cancelled = true;
    };
  }, [composerCommandQuery, composerCommandTrigger?.trigger, filteredComposerCommandItems.length]);

  const applyFileActionPrompt = (context: AgentLensFilePromptContext) => {
    if (!canCompose) return;
    setDraft((current) => {
      const trimmed = current.trimEnd();
      const prompt = fileActionPrompt(context);
      return trimmed ? `${trimmed}\n\n${prompt}` : prompt;
    });
  };

  const applyReviewPrompt = (summary: AgentReviewSummary, findings: AgentReviewFinding[]) => {
    if (!canCompose) return;
    const prompt = reviewActionPrompt(summary, findings);
    setDraft((current) => {
      const trimmed = current.trimEnd();
      return trimmed ? `${trimmed}\n\n${prompt}` : prompt;
    });
    setReviewPromptDraft(prompt);
    setReviewPromptState("drafted");
    setCopiedTarget((current) => (isReviewClipboardTarget(current) ? null : current));
    setSlashMenuOpen(false);
    window.setTimeout(() => composerInputRef.current?.focus(), 0);
  };

  const resetReviewPrompt = () => {
    setReviewPromptDraft(null);
    setReviewPromptState(null);
    setCopiedTarget((current) => (isReviewClipboardTarget(current) ? null : current));
  };

  const copyText = async (target: string, text: string) => {
    try {
      await writeClipboardText(text);
      setCopiedTarget(target);
      window.setTimeout(
        () => setCopiedTarget((current) => (current === target ? null : current)),
        1600,
      );
    } catch (e) {
      setError(commandMessage(e));
    }
  };

  const onRestoreTurn = async (turn: AgentTurnView) => {
    if (!sessionId || !turn.restoreCheckpointId || restoringTurnId) return;
    const ok = await confirm(
      `This will restore files and the chat view to turn ${turn.index}. Continue?`,
      {
        title: "Restore agent turn",
        kind: "warning",
        okLabel: "Restore",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    setRestoringTurnId(turn.restoreCheckpointId);
    setError(null);
    try {
      const updated = await restoreSessionTurn(sessionId, turn.restoreCheckpointId, true);
      agentSessionStore.upsertSession(updated);
      setFocusedTurnIndex(turn.index);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setRestoringTurnId(null);
    }
  };

  const onRevert = async () => {
    if (!sessionId || !canRevert || reverting) return;
    const ok = await confirm("This will undo all changes made by this session. Continue?", {
      title: "Revert agent session",
      kind: "warning",
      okLabel: "Revert",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setReverting(true);
    setError(null);
    try {
      const updated = await revertSession(sessionId, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setReverting(false);
    }
  };

  const onStop = async () => {
    if (!sessionId || !canStop) return;
    setStopping(true);
    setError(null);
    try {
      await stopAgentSession(sessionId);
      const sessions = await listAgentSessions();
      agentSessionStore.setSessions(sessions);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setStopping(false);
    }
  };

  const onRevertTurnFile = async (turnCheckpointId: string, path: string) => {
    if (!sessionId || !canRevertTurnFile || revertingFile) return;
    const ok = await confirm(`Revert ${path} from this turn checkpoint?`, {
      title: "Revert file from turn",
      kind: "warning",
      okLabel: "Revert file",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    setRevertingFile(`${turnCheckpointId}:${path}`);
    setError(null);
    try {
      const updated = await revertSessionTurnFile(sessionId, turnCheckpointId, path, true);
      agentSessionStore.upsertSession(updated);
    } catch (e) {
      setError(commandMessage(e));
    } finally {
      setRevertingFile(null);
    }
  };

  return (
    <div
      className="agent-panel"
      data-testid={`terminal-panel-${sessionId}`}
      title={agentPanelTitle(agentType, repo)}
    >
      <header className="agent-panel__header" title={agentHeaderTitle(agentType, repo)}>
        <span
          className={`agent-panel__logo agent-panel__logo--${agentLogoClass(agentType)}`}
          aria-hidden="true"
          title={agentLogoTitle(agentType, repo)}
        >
          {agentLogoSrc(agentType) ? (
            <img src={agentLogoSrc(agentType) ?? ""} alt="" />
          ) : (
            <span>{agentLogoText(agentType)}</span>
          )}
        </span>
        <div className="agent-panel__identity" title={agentIdentityTitle(agentType, repo)}>
          <span className="agent-panel__agent" title={agentNameLabelTitle(agentType, repo)}>
            {agentLabel(agentType)}
          </span>
          <span className="agent-panel__repo" title={agentRepoLabelTitle(agentType, repo)}>
            {repo ? repoName(repo) : "Agent session"}
          </span>
        </div>
        <SessionStatus session={session} />
        <div
          className="agent-panel__header-actions"
          title={agentHeaderActionsTitle(agentType, repo)}
        >
          <button
            className="agent-panel__stop"
            disabled={!canStop}
            onClick={onStop}
            title={agentStopControlTitle(agentType, repo, readOnly, canStop, stopping)}
            type="button"
          >
            <span title={agentStopControlLabelTitle(stopping)}>
              {stopping ? "Stopping" : "Stop"}
            </span>
          </button>
          <button
            className="agent-panel__revert"
            disabled={!canRevert || reverting}
            onClick={onRevert}
            title={agentRevertControlTitle(
              agentType,
              repo,
              readOnly,
              session ?? null,
              canRevert,
              reverting,
            )}
            type="button"
          >
            <span title={agentRevertControlLabelTitle(reverting)}>
              {reverting ? "Reverting" : "Revert"}
            </span>
          </button>
        </div>
      </header>

      {error && (
        <div
          className="agent-panel__error"
          data-testid="terminal-panel-error"
          title={agentErrorBannerTitle(error)}
        >
          {error}
        </div>
      )}

      <div
        className={`agent-panel__workspace${detailsOpen ? " agent-panel__workspace--details" : ""}`}
        title={agentWorkspaceTitle(agentType, repo)}
      >
        <section
          className={`agent-panel__chat-shell${
            !detailsOpen ? " agent-panel__chat-shell--active" : ""
          }`}
          title={agentChatShellTitle(agentType, repo)}
        >
          <div
            className="agent-panel__chat-tools"
            title={transcriptToolsContainerTitle(
              hasTranscriptQuery,
              visibleTurns.length,
              turns.length,
            )}
          >
            <label
              className="agent-panel__chat-search"
              title={transcriptSearchContainerTitle(
                hasTranscriptQuery,
                visibleTurns.length,
                turns.length,
              )}
            >
              <span title={transcriptSearchLabelTitle()}>Search transcript</span>
              <span className="sr-only" id="agent-transcript-search-hint">
                Press Enter to move through matching turns and Escape to clear the search.
              </span>
              <input
                aria-describedby="agent-transcript-search-hint"
                aria-label="Search transcript"
                ref={transcriptSearchRef}
                value={transcriptQuery}
                onChange={(event) => setTranscriptQuery(event.currentTarget.value)}
                onKeyDown={onTranscriptSearchKeyDown}
                placeholder="Find messages, commands, files..."
                title={transcriptSearchInputTitle()}
                type="search"
              />
            </label>
            <span
              aria-label={transcriptSearchCountDescription}
              aria-live="polite"
              className="agent-panel__chat-search-count"
              title={transcriptSearchCountTitle(
                hasTranscriptQuery,
                visibleTurns.length,
                turns.length,
              )}
            >
              {hasTranscriptQuery ? `${visibleTurns.length} of ${turns.length} turns` : "All turns"}
            </span>
            {activeSearchResultLabel && (
              <span
                aria-label={activeSearchResultDescription ?? "Active search result"}
                className="agent-panel__chat-search-position"
                title={activeSearchResultPositionTitle(
                  activeSearchResultIndex,
                  visibleTurns.length,
                )}
              >
                {activeSearchResultLabel}
              </span>
            )}
            <button
              aria-label="Previous result"
              className="agent-panel__chat-nav"
              disabled={!canNavigateSearchResults}
              onClick={() => focusVisibleTurn("previous")}
              title={previousSearchResultTitle}
              type="button"
            >
              <span title={transcriptSearchNavigationLabelTitle("previous")}>Prev</span>
            </button>
            <button
              aria-label="Next result"
              className="agent-panel__chat-nav"
              disabled={!canNavigateSearchResults}
              onClick={() => focusVisibleTurn("next")}
              title={nextSearchResultTitle}
              type="button"
            >
              <span title={transcriptSearchNavigationLabelTitle("next")}>Next</span>
            </button>
            <div
              aria-label="Transcript secondary actions"
              className="agent-panel__chat-secondary-actions"
              role="group"
              title={transcriptSecondaryActionsTitle(
                hasTranscriptQuery,
                visibleTurns.length,
                turns.length,
              )}
            >
              <button
                className="agent-panel__chat-nav agent-panel__chat-nav--secondary"
                disabled={visibleTurns.length === 0}
                onClick={() => {
                  const latest = visibleTurns[visibleTurns.length - 1];
                  if (latest) {
                    setFocusedTurnIndex(latest.index);
                    scrollToAgentTurn(sessionId, latest.index, "end");
                  }
                }}
                title={latestTurnTitle}
                type="button"
              >
                <span title={transcriptSecondaryActionLabelTitle("Latest")}>Latest</span>
              </button>
              <button
                className="agent-panel__chat-copy agent-panel__chat-copy--secondary"
                disabled={visibleTurns.length === 0}
                onClick={() => void copyText("transcript", transcriptText(visibleTurns))}
                title={transcriptCopyTitle}
                type="button"
              >
                <span
                  title={transcriptSecondaryActionLabelTitle(
                    copiedTarget === "transcript" ? "Copied" : "Copy visible",
                  )}
                >
                  {copiedTarget === "transcript" ? "Copied" : "Copy visible"}
                </span>
              </button>
              {session && (
                <button
                  aria-expanded={detailsOpen}
                  className="agent-panel__details-toggle"
                  onClick={() => setDetailsOpen((open) => !open)}
                  title={
                    detailsOpen
                      ? "Hide session details and return to the conversation."
                      : "Show session details, restore points, files, commands, and timeline."
                  }
                  type="button"
                >
                  <span>{detailsOpen ? "Hide details" : "Details"}</span>
                </button>
              )}
            </div>
          </div>
          <main
            className="agent-panel__chat"
            aria-label="Agent conversation"
            title={conversationContainerTitle(
              hasTranscriptQuery,
              visibleTurns.length,
              turns.length,
              readOnly,
            )}
          >
            {visibleTurns.length > 0 ? (
              visibleTurns.map((turn) => (
                <AgentTurn
                  key={turn.id}
                  copiedTarget={copiedTarget}
                  firstTurnAtMs={turns[0]?.startedAtMs ?? null}
                  focused={turn.index === focusedTurn?.index}
                  onCopyMessage={(target, text) => void copyText(target, text)}
                  onCopyTurn={(target, text) => void copyText(target, text)}
                  searchQuery={transcriptQuery}
                  turn={turn}
                  turnElementId={agentTurnElementId(sessionId, turn.index)}
                />
              ))
            ) : (
              <div
                className="agent-panel__empty-chat"
                title={emptyChatContainerTitle(hasTranscriptQuery, readOnly)}
              >
                <span title={emptyChatStateLabelTitle(hasTranscriptQuery, readOnly)}>
                  {hasTranscriptQuery ? "No matches" : readOnly ? "Transcript" : "Ready"}
                </span>
                <p title={emptyChatHelperTextTitle(hasTranscriptQuery, readOnly)}>
                  {hasTranscriptQuery
                    ? "Try another search across messages, commands, and touched files."
                    : readOnly
                      ? "No timeline items were saved for this session."
                      : "Start a turn from the composer below."}
                </p>
                {hasTranscriptQuery && (
                  <button
                    className="agent-panel__empty-chat-action"
                    onClick={() => resetTranscriptSearch({ focusSearch: true })}
                    title={emptyChatClearSearchActionTitle()}
                    type="button"
                  >
                    <span title={transcriptClearSearchLabelTitle()}>Clear search</span>
                  </button>
                )}
              </div>
            )}
          </main>
        </section>

        <aside
          className={`agent-panel__side-rail${
            detailsOpen ? " agent-panel__side-rail--active" : ""
          }`}
          aria-label="Agent inspection rail"
          title={agentSideRailTitle(agentType, repo)}
        >
          {detailsOpen && (
            <AgentDetailsHeader
              files={overview.files}
              focusedTurnIndex={focusedTurn?.index ?? null}
              onClose={() => setDetailsOpen(false)}
              turns={overview.turns}
            />
          )}
          <AgentSessionOverview
            overview={overview}
            focusedTurnIndex={focusedTurn?.index ?? null}
            onSelectTurn={(turnIndex) => {
              setFocusedTurnIndex(turnIndex);
              scrollToAgentTurn(sessionId, turnIndex, "start");
            }}
          />
          {session && <AgentHostContextStrip session={session} />}
          {mascotAwake && <AgentMascotPanel agentType={agentType} repo={repo} />}
          {reviewSummary && (
            <AgentReviewSummaryPanel
              canPrompt={canCompose}
              copiedExchange={copiedTarget === "review-exchange"}
              copiedFiles={copiedTarget === "review-files"}
              copiedFindings={copiedTarget === "review-findings"}
              copiedPrompt={copiedTarget === "review-prompt"}
              copiedResponse={copiedTarget === "review-response"}
              copiedSummary={copiedTarget === "review-summary"}
              findings={reviewFindings}
              onCopyExchange={(prompt, response) =>
                void copyText("review-exchange", reviewExchangeCopyText(prompt, response))
              }
              onCopyFindings={(findings) =>
                void copyText("review-findings", reviewFindingsCopyText(findings))
              }
              onCopyFiles={(summary) => void copyText("review-files", reviewFilesCopyText(summary))}
              onCopyPrompt={(prompt) => void copyText("review-prompt", prompt)}
              onCopyResponse={(response) => void copyText("review-response", response.text)}
              onCopySummary={(summary, findings) =>
                void copyText("review-summary", reviewSummaryCopyText(summary, findings))
              }
              onPromptReview={() => applyReviewPrompt(reviewSummary, reviewFindings)}
              onResetReview={resetReviewPrompt}
              onShowPromptRequest={(turnIndex) => {
                setFocusedTurnIndex(turnIndex);
                scrollToAgentTurn(sessionId, turnIndex, "center");
              }}
              onShowResponse={(response) => {
                setFocusedTurnIndex(response.turnIndex);
                scrollToAgentTurn(sessionId, response.turnIndex, "center");
              }}
              promptDraft={reviewPromptDraft}
              promptState={reviewPromptState}
              promptTurnIndex={reviewPromptTurnIndex}
              response={reviewResponse}
              summary={reviewSummary}
            />
          )}
          <AgentActivityStrip overview={overview} readOnly={readOnly} session={session} />
          {session && (
            <>
              <div className="agent-panel__focus-pane">
                <AgentTurnFocus
                  canRestore={canRestoreTurn}
                  firstTurnAtMs={turns[0]?.startedAtMs ?? null}
                  onRestoreTurn={(turn) => void onRestoreTurn(turn)}
                  restoringTurnId={restoringTurnId}
                  turn={focusedTurn}
                />
              </div>
              {session && (
                <div className="agent-panel__lens-pane">
                  <AgentLens
                    session={session}
                    turns={turns}
                    focusedTurn={focusedTurn}
                    repo={sessionRepo}
                    canRevertTurnFile={canRevertTurnFile}
                    canPromptForFile={canCompose}
                    revertingFile={revertingFile}
                    onOpenFile={(path) => {
                      if (sessionRepo) openFile(sessionRepo, path, true);
                    }}
                    onPromptFile={applyFileActionPrompt}
                    onRevertTurnFile={onRevertTurnFile}
                  />
                </div>
              )}
            </>
          )}
        </aside>
      </div>

      <form
        className="agent-panel__composer"
        onSubmit={onSubmit}
        title={agentComposerTitle(agentType, repo)}
      >
        <div
          className="agent-panel__composer-actions"
          aria-label="Agent command menu"
          title={agentComposerActionsTitle(agentType, repo, readOnly, canCompose)}
        >
          <span title={agentCommandHintTitle(canCompose, readOnly)}>
            {canCompose
              ? "Type / for commands or $ for skills"
              : readOnly
                ? "Archived transcript"
                : "Commands unavailable"}
          </span>
          <small title={agentCommandScopeHintTitle()}>Codex + Tinto + Skills</small>
        </div>
        {isCodexSession && (
          <CodexRuntimeControls
            menu={runtimeMenu}
            model={selectedModel}
            reasoning={selectedReasoning}
            speed={selectedSpeed}
            disabled={!canCompose}
            notice={runtimeNotice}
            onMenuChange={setRuntimeMenu}
            onModelChange={(value) => {
              setSelectedModel(value);
              setRuntimeNotice(`Model set to ${codexModelLabel(value)}.`);
            }}
            onReasoningChange={(value) => {
              setSelectedReasoning(value);
              setRuntimeNotice(`Reasoning set to ${codexReasoningLabel(value)}.`);
            }}
            onSpeedChange={(value) => {
              setSelectedSpeed(value);
              setRuntimeNotice(
                value === "fast" ? "Fast preset enabled." : "Standard speed enabled.",
              );
            }}
          />
        )}
        {commandMenuVisible && (
          <div
            aria-label="Composer commands"
            className="agent-panel__slash-menu"
            role="listbox"
            title={agentCommandMenuTitle(
              composerCommandTrigger,
              filteredComposerCommandItems.length,
            )}
          >
            {filteredComposerCommandItems.length > 0 ? (
              filteredComposerCommandItems.map((command, index) => (
                <button
                  aria-selected={index === activeSlashCommandIndex}
                  className={`agent-panel__slash-command${
                    index === activeSlashCommandIndex ? " agent-panel__slash-command--active" : ""
                  }`}
                  disabled={command.disabled}
                  key={command.id}
                  onClick={() => applyComposerCommand(command)}
                  onMouseDown={(event) => event.preventDefault()}
                  role="option"
                  title={agentComposerCommandTitle(command)}
                  type="button"
                >
                  <code title={agentComposerCommandCodeTitle(command)}>
                    {command.trigger}
                    {command.command}
                  </code>
                  <span title={agentComposerCommandLabelTitle(command.label)}>{command.label}</span>
                  <small title={agentComposerCommandDescriptionTitle(command)}>
                    {command.scope} / {command.description}
                    {agentComposerCommandAliasText(command)}
                  </small>
                </button>
              ))
            ) : (
              <div
                className="agent-panel__slash-empty"
                title={agentCommandEmptyTitle(composerCommandTrigger)}
              >
                No command matches {composerCommandTrigger?.trigger}
                {composerCommandQuery}
              </div>
            )}
          </div>
        )}
        <div className="agent-panel__composer-row" title={agentComposerRowTitle(agentType, repo)}>
          <textarea
            aria-label={`Message ${agentLabel(agentType)}`}
            ref={composerInputRef}
            value={draft}
            onChange={(event) => onDraftChange(event.currentTarget.value)}
            onKeyDown={onDraftKeyDown}
            title={agentComposerInputTitle(agentType, repo, readOnly, canCompose)}
            placeholder={
              readOnly
                ? "Archived transcript"
                : `Ask ${agentLabel(agentType)} what to do in this repo...`
            }
            disabled={!canCompose}
            rows={2}
          />
          <button
            className="agent-panel__send"
            type="submit"
            disabled={!canSend}
            title={agentComposerSendTitle(
              agentType,
              repo,
              canSend,
              sending,
              readOnly,
              canCompose,
              draft.trim().length > 0,
            )}
          >
            <span title={agentComposerSendLabelTitle(sending)}>{sending ? "Sending" : "Send"}</span>
          </button>
        </div>
      </form>
    </div>
  );
}

function AgentMascotPanel({ agentType, repo }: { agentType: string; repo?: string }) {
  const label = agentLabel(agentType);
  const repoLabel = repo ? repoName(repo) : "this session";
  return (
    <section
      aria-label="Tinto companion"
      className="agent-panel__mascot"
      title={`Tinto companion is awake for ${label} on ${repoLabel}.`}
    >
      <span aria-hidden="true" className="agent-panel__mascot-mark" title="Tinto companion mark.">
        T
      </span>
      <div className="agent-panel__mascot-copy">
        <strong title="Tinto companion status.">Awake</strong>
        <small title={`Tinto companion scope: ${repoLabel}.`}>Watching this turn surface</small>
      </div>
    </section>
  );
}

function AgentReviewSummaryPanel({
  canPrompt,
  copiedExchange,
  copiedFiles,
  copiedFindings,
  copiedPrompt,
  copiedResponse,
  copiedSummary,
  findings,
  onCopyExchange,
  onCopyFiles,
  onCopyFindings,
  onCopyPrompt,
  onCopyResponse,
  onCopySummary,
  onPromptReview,
  onResetReview,
  onShowPromptRequest,
  onShowResponse,
  promptDraft,
  promptState,
  promptTurnIndex,
  response,
  summary,
}: {
  canPrompt: boolean;
  copiedExchange: boolean;
  copiedFiles: boolean;
  copiedFindings: boolean;
  copiedPrompt: boolean;
  copiedResponse: boolean;
  copiedSummary: boolean;
  findings: AgentReviewFinding[];
  onCopyExchange: (prompt: string, response: AgentReviewResponseView) => void;
  onCopyFiles: (summary: AgentReviewSummary) => void;
  onCopyFindings: (findings: AgentReviewFinding[]) => void;
  onCopyPrompt: (prompt: string) => void;
  onCopyResponse: (response: AgentReviewResponseView) => void;
  onCopySummary: (summary: AgentReviewSummary, findings: AgentReviewFinding[]) => void;
  onPromptReview: () => void;
  onResetReview: () => void;
  onShowPromptRequest: (turnIndex: number) => void;
  onShowResponse: (response: AgentReviewResponseView) => void;
  promptDraft: string | null;
  promptState: "drafted" | "sent" | null;
  promptTurnIndex: number | null;
  response: AgentReviewResponseView | null;
  summary: AgentReviewSummary;
}) {
  const visibleFiles = summary.files.slice(0, 8);
  const visibleFindings = findings.slice(0, 4);
  const hiddenCount =
    summary.truncated_count + Math.max(0, summary.files.length - visibleFiles.length);
  const working = summary.working_shortstat ?? "no unstaged line diff";
  const staged = summary.staged_shortstat ?? "no staged line diff";
  return (
    <section
      className="agent-panel__review-summary"
      aria-label="Review summary"
      title={`Review summary for ${summary.branch}: ${summary.changed_files} changed files.`}
    >
      <div className="agent-panel__review-summary-head">
        <span title="Review summary branch.">{summary.branch}</span>
        <small title="Review summary changed file count.">
          {summary.changed_files} {summary.changed_files === 1 ? "file" : "files"}
        </small>
      </div>
      <button
        aria-label="Draft semantic review prompt"
        className="agent-panel__review-action"
        disabled={!canPrompt}
        onClick={onPromptReview}
        title={reviewPromptActionTitle(canPrompt, findings.length)}
        type="button"
      >
        <span title="Review semantic prompt action label.">Ask review</span>
      </button>
      <button
        aria-label="Copy structured review summary"
        className="agent-panel__review-action"
        onClick={() => onCopySummary(summary, findings)}
        title={reviewSummaryCopyButtonTitle(copiedSummary)}
        type="button"
      >
        <span title={reviewSummaryCopyLabelTitle(copiedSummary ? "Copied" : "Copy summary")}>
          {copiedSummary ? "Copied" : "Copy summary"}
        </span>
      </button>
      {summary.files.length > 0 && (
        <button
          aria-label="Copy review changed files"
          className="agent-panel__review-action"
          onClick={() => onCopyFiles(summary)}
          title={reviewFilesCopyButtonTitle(copiedFiles, summary.files.length)}
          type="button"
        >
          <span title={reviewFilesCopyLabelTitle(copiedFiles ? "Copied" : "Copy files")}>
            {copiedFiles ? "Copied" : "Copy files"}
          </span>
        </button>
      )}
      {findings.length > 0 && (
        <button
          aria-label="Copy deterministic review findings"
          className="agent-panel__review-action"
          onClick={() => onCopyFindings(findings)}
          title={reviewFindingsCopyButtonTitle(copiedFindings, findings.length)}
          type="button"
        >
          <span title={reviewFindingsCopyLabelTitle(copiedFindings ? "Copied" : "Copy findings")}>
            {copiedFindings ? "Copied" : "Copy findings"}
          </span>
        </button>
      )}
      {promptState && (
        <p
          className="agent-panel__review-request-state"
          title={reviewPromptStateTitle(promptState)}
        >
          {reviewPromptStateLabel(promptState)}
        </p>
      )}
      {promptDraft && promptState && (
        <button
          aria-label="Copy semantic review prompt"
          className="agent-panel__review-action"
          onClick={() => onCopyPrompt(promptDraft)}
          title={reviewPromptCopyButtonTitle(copiedPrompt, promptState)}
          type="button"
        >
          <span title={reviewPromptCopyLabelTitle(copiedPrompt ? "Copied" : "Copy prompt")}>
            {copiedPrompt ? "Copied" : "Copy prompt"}
          </span>
        </button>
      )}
      {promptTurnIndex != null && (
        <button
          aria-label="Show semantic review request turn"
          className="agent-panel__review-action"
          onClick={() => onShowPromptRequest(promptTurnIndex)}
          title={reviewPromptShowButtonTitle(promptTurnIndex)}
          type="button"
        >
          <span title="Semantic review request navigation label: Show request.">Show request</span>
        </button>
      )}
      {(promptState || response) && (
        <button
          aria-label="Reset semantic review workflow"
          className="agent-panel__review-action"
          onClick={onResetReview}
          title={reviewPromptResetButtonTitle(response != null)}
          type="button"
        >
          <span title="Semantic review reset label: Reset review.">Reset review</span>
        </button>
      )}
      {response && (
        <div
          className="agent-panel__review-response"
          title={reviewResponseTitle(response.turnIndex)}
        >
          <strong title="Semantic review response status.">Review response captured</strong>
          <span title={`Semantic review response excerpt: ${response.excerpt}`}>
            {response.excerpt}
          </span>
        </div>
      )}
      {response && (
        <button
          aria-label="Show semantic review response turn"
          className="agent-panel__review-action"
          onClick={() => onShowResponse(response)}
          title={reviewResponseShowButtonTitle(response.turnIndex)}
          type="button"
        >
          <span title="Semantic review response navigation label: Show response.">
            Show response
          </span>
        </button>
      )}
      {response && (
        <button
          aria-label="Copy semantic review response"
          className="agent-panel__review-action"
          onClick={() => onCopyResponse(response)}
          title={reviewResponseCopyButtonTitle(copiedResponse)}
          type="button"
        >
          <span title={reviewResponseCopyLabelTitle(copiedResponse ? "Copied" : "Copy response")}>
            {copiedResponse ? "Copied" : "Copy response"}
          </span>
        </button>
      )}
      {promptDraft && response && (
        <button
          aria-label="Copy semantic review exchange"
          className="agent-panel__review-action"
          onClick={() => onCopyExchange(promptDraft, response)}
          title={reviewExchangeCopyButtonTitle(copiedExchange)}
          type="button"
        >
          <span title={reviewExchangeCopyLabelTitle(copiedExchange ? "Copied" : "Copy exchange")}>
            {copiedExchange ? "Copied" : "Copy exchange"}
          </span>
        </button>
      )}
      <div className="agent-panel__review-summary-stats" aria-label="Review diff stats">
        <span title={`Working tree diff: ${working}`}>{working}</span>
        <span title={`Staged diff: ${staged}`}>{staged}</span>
      </div>
      {visibleFiles.length > 0 ? (
        <ul className="agent-panel__review-summary-files" aria-label="Review changed files">
          {visibleFiles.map((file) => (
            <li key={file} title={`Review changed file: ${file}`}>
              {file}
            </li>
          ))}
          {hiddenCount > 0 && (
            <li title={`Review summary has ${hiddenCount} more changed files.`}>
              +{hiddenCount} more
            </li>
          )}
        </ul>
      ) : (
        <p title="Review summary has no local changed files.">No local changes detected.</p>
      )}
      {visibleFindings.length > 0 && (
        <ul className="agent-panel__review-findings" aria-label="Review findings">
          {visibleFindings.map((finding, index) => {
            const location = reviewFindingLocation(finding);
            return (
              <li
                key={`${finding.title}:${finding.path ?? "session"}:${finding.line ?? index}`}
                title={`${finding.severity}: ${finding.title}. ${finding.detail}`}
              >
                <span title={`Review finding severity: ${finding.severity}.`}>
                  {finding.severity}
                </span>
                <strong title={`Review finding: ${finding.title}.`}>{finding.title}</strong>
                {location && (
                  <small title={`Review finding location: ${location}.`}>{location}</small>
                )}
              </li>
            );
          })}
          {findings.length > visibleFindings.length && (
            <li title={`Review has ${findings.length - visibleFindings.length} more findings.`}>
              <span title="Review finding overflow severity.">more</span>
              <strong title="Review finding overflow count.">
                +{findings.length - visibleFindings.length} findings
              </strong>
            </li>
          )}
        </ul>
      )}
    </section>
  );
}

function reviewFindingLocation(finding: AgentReviewFinding): string | null {
  if (!finding.path) return null;
  return finding.line ? `${finding.path}:${finding.line}` : finding.path;
}

function reviewSummaryCopyButtonTitle(copied: boolean): string {
  return copied
    ? "Copied structured review summary to clipboard."
    : "Copy the structured review summary to the clipboard.";
}

function reviewSummaryCopyLabelTitle(label: "Copy summary" | "Copied"): string {
  return `Structured review summary copy label: ${label}.`;
}

function isReviewClipboardTarget(target: string | null): boolean {
  return (
    target === "review-summary" ||
    target === "review-files" ||
    target === "review-findings" ||
    target === "review-prompt" ||
    target === "review-response" ||
    target === "review-exchange"
  );
}

function reviewFilesCopyButtonTitle(copied: boolean, fileCount: number): string {
  return copied
    ? "Copied review changed files to clipboard."
    : `Copy ${overviewMetricCount("Files", fileCount)} to the clipboard.`;
}

function reviewFilesCopyLabelTitle(label: "Copy files" | "Copied"): string {
  return `Review changed files copy label: ${label}.`;
}

function reviewFilesCopyText(summary: AgentReviewSummary): string {
  if (summary.files.length === 0) return "Review changed files: none";
  const lines = ["Review changed files:", ...summary.files.map((file) => `- ${file}`)];
  if (summary.truncated_count > 0) {
    lines.push(`- +${summary.truncated_count} more changed files`);
  }
  return lines.join("\n");
}

function reviewFindingsCopyButtonTitle(copied: boolean, findingCount: number): string {
  return copied
    ? "Copied deterministic review findings to clipboard."
    : `Copy ${overviewMetricCount("Findings", findingCount)} to the clipboard.`;
}

function reviewFindingsCopyLabelTitle(label: "Copy findings" | "Copied"): string {
  return `Deterministic review findings copy label: ${label}.`;
}

function reviewFindingsCopyText(findings: AgentReviewFinding[]): string {
  if (findings.length === 0) return "Host review findings: none";
  return [
    "Host review findings:",
    ...findings.map((finding) => {
      const location = reviewFindingLocation(finding);
      return `- ${finding.severity}: ${finding.title}${
        location ? ` (${location})` : ""
      } - ${finding.detail}`;
    }),
  ].join("\n");
}

function reviewSummaryCopyText(
  summary: AgentReviewSummary,
  findings: AgentReviewFinding[],
): string {
  const lines = [
    "Structured review summary:",
    `Branch: ${summary.branch}`,
    `Changed files: ${summary.changed_files}`,
    `Working tree diff: ${summary.working_shortstat ?? "no unstaged line diff"}`,
    `Staged diff: ${summary.staged_shortstat ?? "no staged line diff"}`,
  ];
  if (summary.files.length > 0) {
    lines.push("Files:", ...summary.files.map((file) => `- ${file}`));
    if (summary.truncated_count > 0) {
      lines.push(`- +${summary.truncated_count} more changed files`);
    }
  } else {
    lines.push("Files: none");
  }
  if (findings.length > 0) {
    lines.push("Host review findings:");
    for (const finding of findings) {
      const location = reviewFindingLocation(finding);
      lines.push(
        `- ${finding.severity}: ${finding.title}${location ? ` (${location})` : ""} - ${
          finding.detail
        }`,
      );
    }
  } else {
    lines.push("Host review findings: none");
  }
  return lines.join("\n");
}

function reviewPromptActionTitle(canPrompt: boolean, findingCount: number): string {
  const findingText =
    findingCount > 0
      ? `with ${overviewMetricCount("Findings", findingCount)}`
      : "with no deterministic findings";
  return canPrompt
    ? `Draft a semantic code-review prompt from this review summary ${findingText}.`
    : "Cannot draft a semantic code-review prompt because the session is archived or inactive.";
}

function reviewPromptStateLabel(state: "drafted" | "sent"): string {
  return state === "sent" ? "Review request sent" : "Review draft ready";
}

function reviewPromptStateTitle(state: "drafted" | "sent"): string {
  return state === "sent"
    ? "Semantic review prompt was sent as an agent turn."
    : "Semantic review prompt is drafted in the composer.";
}

function reviewPromptCopyButtonTitle(copied: boolean, state: "drafted" | "sent"): string {
  if (copied) return "Copied semantic review prompt to clipboard.";
  return state === "sent"
    ? "Copy the sent semantic review prompt to the clipboard."
    : "Copy the drafted semantic review prompt to the clipboard.";
}

function reviewPromptCopyLabelTitle(label: "Copy prompt" | "Copied"): string {
  return `Semantic review prompt copy label: ${label}.`;
}

function reviewPromptShowButtonTitle(turnIndex: number): string {
  return `Show the sent semantic review request in conversation turn ${turnIndex}.`;
}

function reviewPromptResetButtonTitle(hasResponse: boolean): string {
  return hasResponse
    ? "Reset the captured semantic review response and request state for this review summary."
    : "Reset the drafted semantic review prompt state for this review summary.";
}

function reviewResponseTitle(turnIndex: number): string {
  return `Semantic review response captured from turn ${turnIndex}; verify findings before acting.`;
}

function reviewResponseCopyButtonTitle(copied: boolean): string {
  return copied
    ? "Copied semantic review response to clipboard."
    : "Copy the captured semantic review response to the clipboard.";
}

function reviewResponseShowButtonTitle(turnIndex: number): string {
  return `Show the full semantic review response in conversation turn ${turnIndex}.`;
}

function reviewResponseCopyLabelTitle(label: "Copy response" | "Copied"): string {
  return `Semantic review response copy label: ${label}.`;
}

function reviewExchangeCopyButtonTitle(copied: boolean): string {
  return copied
    ? "Copied semantic review request and response to clipboard."
    : "Copy the semantic review request and captured response to the clipboard.";
}

function reviewExchangeCopyLabelTitle(label: "Copy exchange" | "Copied"): string {
  return `Semantic review exchange copy label: ${label}.`;
}

function reviewExchangeCopyText(prompt: string, response: AgentReviewResponseView): string {
  return [
    "Semantic review request:",
    prompt.trim(),
    "",
    "Semantic review response:",
    response.text.trim(),
  ].join("\n");
}

function reviewResponseForPrompt(
  turns: AgentTurnView[],
  prompt: string,
): AgentReviewResponseView | null {
  const promptText = prompt.trim();
  const turnIndex = reviewPromptTurnIndexForPrompt(turns, promptText);
  const turn = turnIndex == null ? null : turns.find((candidate) => candidate.index === turnIndex);
  const responseText = turn?.agentText.join("\n").trim();
  if (!turn || !responseText) return null;
  return {
    turnIndex: turn.index,
    text: responseText,
    excerpt: compactActivityText(responseText),
  };
}

function reviewPromptTurnIndexForPrompt(turns: AgentTurnView[], prompt: string): number | null {
  const promptText = prompt.trim();
  if (!promptText) return null;
  return turns.find((candidate) => candidate.userText?.trim() === promptText)?.index ?? null;
}

function reviewActionPrompt(summary: AgentReviewSummary, findings: AgentReviewFinding[]): string {
  const lines = [
    "Review the current Git changes for correctness, regressions, security risks, and missing tests.",
    `Branch: ${summary.branch}`,
    `Changed files: ${summary.changed_files}`,
  ];
  if (summary.working_shortstat) lines.push(`Working tree diff: ${summary.working_shortstat}`);
  if (summary.staged_shortstat) lines.push(`Staged diff: ${summary.staged_shortstat}`);
  if (summary.files.length > 0) {
    lines.push("Files:");
    for (const file of summary.files.slice(0, 12)) {
      lines.push(`- ${file}`);
    }
    if (summary.truncated_count > 0) {
      lines.push(`- plus ${summary.truncated_count} more changed files`);
    }
  }
  if (findings.length > 0) {
    lines.push("Host review findings to verify first:");
    for (const finding of findings.slice(0, 8)) {
      const location = reviewFindingLocation(finding);
      lines.push(
        `- ${finding.severity}: ${finding.title}${location ? ` (${location})` : ""} - ${finding.detail}`,
      );
    }
    if (findings.length > 8) {
      lines.push(`- plus ${findings.length - 8} more host findings`);
    }
  }
  lines.push(
    "Return findings first, ordered by severity, with file/line references when possible. If there are no issues, say that clearly and mention any residual test gaps.",
  );
  return lines.join("\n");
}

function CodexRuntimeControls({
  disabled,
  menu,
  model,
  notice,
  onMenuChange,
  onModelChange,
  onReasoningChange,
  onSpeedChange,
  reasoning,
  speed,
}: {
  disabled: boolean;
  menu: CodexRuntimeMenu;
  model: CodexModelSelection;
  notice: string | null;
  onMenuChange: (menu: CodexRuntimeMenu) => void;
  onModelChange: (value: CodexModelSelection) => void;
  onReasoningChange: (value: CodexReasoningSelection) => void;
  onSpeedChange: (value: CodexSpeedSelection) => void;
  reasoning: CodexReasoningSelection;
  speed: CodexSpeedSelection;
}) {
  const close = () => onMenuChange(null);
  return (
    <div className="agent-panel__runtime" aria-label="Codex runtime controls">
      <div className="agent-panel__runtime-buttons">
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "reasoning" ? null : "reasoning")}
          title={`Reasoning: ${codexReasoningLabel(reasoning)}.`}
        >
          <span aria-hidden="true">○</span>
          <span>{codexReasoningShortLabel(reasoning)}</span>
        </button>
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "model" ? null : "model")}
          title={`Model: ${codexModelLabel(model)}.`}
        >
          <span aria-hidden="true">⚡</span>
          <span>{codexModelShortLabel(model)}</span>
        </button>
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "speed" ? null : "speed")}
          title={`Speed: ${codexSpeedLabel(speed)}.`}
        >
          <span aria-hidden="true">↗</span>
          <span>{codexSpeedLabel(speed)}</span>
        </button>
      </div>
      {notice && (
        <span className="agent-panel__runtime-notice" title={`Codex runtime update: ${notice}`}>
          {notice}
        </span>
      )}
      {menu && (
        <div className="agent-panel__runtime-popover" role="menu" title="Codex runtime picker">
          {menu === "reasoning" &&
            CODEX_REASONING_OPTIONS.map((option) => (
              <RuntimeOptionButton
                active={reasoning === option.value}
                description={option.description}
                key={option.value}
                label={option.label}
                onClick={() => {
                  onReasoningChange(option.value);
                  close();
                }}
              />
            ))}
          {menu === "model" &&
            CODEX_MODEL_OPTIONS.map((option) => (
              <RuntimeOptionButton
                active={model === option.value}
                description={option.description}
                key={option.value}
                label={option.label}
                onClick={() => {
                  onModelChange(option.value);
                  close();
                }}
              />
            ))}
          {menu === "speed" &&
            CODEX_SPEED_OPTIONS.map((option) => (
              <RuntimeOptionButton
                active={speed === option.value}
                description={option.description}
                key={option.value}
                label={option.label}
                onClick={() => {
                  onSpeedChange(option.value);
                  close();
                }}
              />
            ))}
        </div>
      )}
    </div>
  );
}

function RuntimeOptionButton({
  active,
  description,
  label,
  onClick,
}: {
  active: boolean;
  description: string;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={active}
      className={`agent-panel__runtime-option${active ? " agent-panel__runtime-option--active" : ""}`}
      onClick={onClick}
      title={`${label}: ${description}.`}
    >
      <span>{label}</span>
      {active && <span aria-hidden="true">✓</span>}
    </button>
  );
}

function AgentDetailsHeader({
  files,
  focusedTurnIndex,
  onClose,
  turns,
}: {
  files: number;
  focusedTurnIndex: number | null;
  onClose: () => void;
  turns: number;
}) {
  return (
    <header
      className="agent-panel__details-head"
      title="Session details: turn map, current activity, restore points, and Agent Lens."
    >
      <div>
        <strong>Details</strong>
        <small>
          {turns} {turnNoun(turns)} / {files} {files === 1 ? "file" : "files"}
          {focusedTurnIndex ? ` / T${focusedTurnIndex}` : ""}
        </small>
      </div>
      <button
        className="agent-panel__details-close"
        onClick={onClose}
        title="Close session details."
        type="button"
      >
        Close
      </button>
    </header>
  );
}

function AgentSessionOverview({
  focusedTurnIndex,
  overview,
  onSelectTurn,
}: {
  focusedTurnIndex: number | null;
  overview: AgentSessionOverviewView;
  onSelectTurn: (turnIndex: number) => void;
}) {
  return (
    <section
      className="agent-panel__overview"
      aria-label="Agent session overview"
      title={overviewSectionTitle(overview)}
    >
      <div
        className="agent-panel__overview-metrics"
        title={overviewMetricsContainerTitle(overview)}
      >
        <OverviewMetric value={overview.turns} label="Turns" />
        <OverviewMetric value={overview.messages} label="Messages" />
        <OverviewMetric value={overview.commands} label="Commands" />
        <OverviewMetric value={overview.files} label="Files" />
      </div>
      <div
        className="agent-panel__overview-activity"
        title={overviewLatestActivityGroupTitle(overview.latest)}
      >
        <span title={overviewLatestActivityLabelTitle()}>Latest activity</span>
        <p title={overviewLatestActivityTextTitle(overview.latest)}>
          {overview.latest ?? "Waiting for the first turn."}
        </p>
      </div>
      {overview.turnMap.length > 0 && (
        <div
          className="agent-panel__overview-turns"
          aria-label="Turn map"
          title={turnMapContainerTitle(overview.turnMap.length)}
        >
          {overview.turnMap.map((turn) => (
            <button
              className={`agent-panel__overview-turn${
                turn.index === focusedTurnIndex ? " agent-panel__overview-turn--active" : ""
              }`}
              aria-pressed={turn.index === focusedTurnIndex}
              key={turn.id}
              onClick={() => onSelectTurn(turn.index)}
              title={turnMapTitle(turn)}
              type="button"
            >
              <strong title={turnMapIndexTitle(turn.index)}>T{turn.index}</strong>
              {turn.timeLabel && (
                <small title={turnMapTimeTitle(turn.index, turn.timeLabel)}>{turn.timeLabel}</small>
              )}
              {turn.commands > 0 && (
                <small title={turnMapCommandCountTitle(turn.index, turn.commands)}>
                  {turn.commands} cmd
                </small>
              )}
              {turn.commandSummary && (
                <small title={turnMapCommandSummaryTitle(turn.index, turn.commandSummary)}>
                  cmd {turn.commandSummary}
                </small>
              )}
              {turn.files > 0 && (
                <small title={turnMapFileCountTitle(turn.index, turn.files)}>
                  {turn.files} files
                </small>
              )}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

function turnMapContainerTitle(turnCount: number): string {
  return `Agent session overview turn map: ${overviewMetricCount("Turns", turnCount)}.`;
}

function overviewSectionTitle(overview: AgentSessionOverviewView): string {
  const latestSummary = overview.latest ?? "waiting for the first turn";
  const turnMapSummary =
    overview.turnMap.length > 0
      ? `turn map ${overviewMetricCount("Turns", overview.turnMap.length)}`
      : "turn map waiting for turns";
  return `Agent session overview: ${overviewMetricCount(
    "Turns",
    overview.turns,
  )}, ${overviewMetricCount("Messages", overview.messages)}, ${overviewMetricCount(
    "Commands",
    overview.commands,
  )}, ${overviewMetricCount("Files", overview.files)}; latest activity: ${latestSummary}; ${turnMapSummary}.`;
}

function overviewMetricsContainerTitle(overview: AgentSessionOverviewView): string {
  return `Agent session overview metrics: ${overviewMetricCount(
    "Turns",
    overview.turns,
  )}, ${overviewMetricCount("Messages", overview.messages)}, ${overviewMetricCount(
    "Commands",
    overview.commands,
  )}, ${overviewMetricCount("Files", overview.files)}.`;
}

function OverviewMetric({ value, label }: { value: number; label: string }) {
  return (
    <div
      className="agent-panel__overview-metric"
      aria-label={`${label}: ${value}`}
      title={overviewMetricTitle(label, value)}
    >
      <span title={overviewMetricValueTitle(label, value)}>{value}</span>
      <small title={overviewMetricLabelTitle(label)}>{label}</small>
    </div>
  );
}

function overviewMetricTitle(label: string, value: number): string {
  return `Agent session overview ${label.toLowerCase()} metric: ${overviewMetricCount(label, value)}.`;
}

function overviewMetricValueTitle(label: string, value: number): string {
  return `Agent session overview ${label.toLowerCase()} value: ${value}.`;
}

function overviewMetricLabelTitle(label: string): string {
  return `Agent session overview metric label: ${label}.`;
}

function overviewMetricCount(label: string, value: number): string {
  const singular = label.endsWith("s") ? label.slice(0, -1).toLowerCase() : label.toLowerCase();
  const unit = value === 1 ? singular : label.toLowerCase();
  return `${value} ${unit}`;
}

function overviewLatestActivityGroupTitle(latest: string | null): string {
  return latest
    ? "Agent session overview latest-activity area: latest captured activity."
    : "Agent session overview latest-activity area: waiting for the first turn.";
}

function overviewLatestActivityLabelTitle(): string {
  return "Agent session overview latest-activity label.";
}

function overviewLatestActivityTextTitle(latest: string | null): string {
  return latest
    ? `Agent session overview latest activity: ${latest}.`
    : "Agent session overview latest activity: waiting for the first turn.";
}

function AgentHostContextStrip({ session }: { session: AgentSession }) {
  const items = agentHostContextItems(session);
  if (items.length === 0) return null;
  return (
    <section
      aria-label="Turn context"
      className="agent-panel__context-strip"
      title={agentHostContextStripTitle(items)}
    >
      <span title={agentHostContextLabelTitle()}>Turn context</span>
      <div className="agent-panel__context-items" title={agentHostContextItemsTitle(items.length)}>
        {items.map((item) => (
          <div
            className="agent-panel__context-item"
            key={item.kind}
            title={agentHostContextItemTitle(item)}
          >
            <small title={agentHostContextItemLabelTitle(item.label)}>{item.label}</small>
            <strong title={agentHostContextItemValueTitle(item)}>{item.value}</strong>
          </div>
        ))}
      </div>
    </section>
  );
}

type AgentHostContextItem = {
  kind: "goal" | "personality" | "plan" | "compact";
  label: string;
  value: string;
};

function agentHostContextItems(session: AgentSession): AgentHostContextItem[] {
  const items: AgentHostContextItem[] = [];
  const goal = compactContextValue(session.goal?.text ?? null);
  if (goal) {
    items.push({ kind: "goal", label: "Goal", value: goal });
  }
  const personality = compactContextValue(session.personality?.name ?? null);
  if (personality) {
    items.push({ kind: "personality", label: "Style", value: personality });
  }
  if (session.plan_mode?.enabled) {
    items.push({ kind: "plan", label: "Plan", value: "On" });
  }
  const summary = compactContextValue(session.context_summary?.text ?? null);
  if (summary) {
    items.push({ kind: "compact", label: "Compact", value: summary });
  }
  return items;
}

function compactContextValue(value: string | null | undefined): string | null {
  const normalized = value?.trim().replace(/\s+/g, " ");
  if (!normalized) return null;
  return normalized.length > 80 ? `${normalized.slice(0, 77)}...` : normalized;
}

function agentHostContextStripTitle(items: AgentHostContextItem[]): string {
  return `Turn context strip: ${punctuatedTitleValue(
    items.map((item) => `${item.label} ${item.value}`).join("; "),
  )}`;
}

function agentHostContextLabelTitle(): string {
  return "Turn context label.";
}

function agentHostContextItemsTitle(count: number): string {
  return `Turn context items: ${overviewMetricCount("Items", count)}.`;
}

function agentHostContextItemTitle(item: AgentHostContextItem): string {
  return `Turn context ${item.label.toLowerCase()}: ${punctuatedTitleValue(item.value)}`;
}

function agentHostContextItemLabelTitle(label: string): string {
  return `Turn context item label: ${label}.`;
}

function agentHostContextItemValueTitle(item: AgentHostContextItem): string {
  return `Turn context ${item.label.toLowerCase()} value: ${punctuatedTitleValue(item.value)}`;
}

function punctuatedTitleValue(value: string): string {
  return /[.!?]$/.test(value) ? value : `${value}.`;
}

function AgentActivityStrip({
  overview,
  readOnly,
  session,
}: {
  overview: AgentSessionOverviewView;
  readOnly: boolean;
  session: AgentSession | undefined;
}) {
  const activity = agentActivitySummary(session, readOnly);
  return (
    <section
      className="agent-panel__activity"
      aria-label="Agent activity"
      title={agentActivityStripTitle(activity, overview)}
    >
      <div className="agent-panel__activity-main" title={agentActivityMainTitle(activity)}>
        <span
          className={`agent-panel__activity-dot agent-panel__activity-dot--${activity.tone}`}
          title={agentActivityDotTitle(activity)}
        />
        <div title={agentActivityTextGroupTitle(activity)}>
          <strong title={agentActivityTitleLabelTitle(activity)}>{activity.title}</strong>
          <p title={agentActivityDetailTitle(activity)}>{activity.detail}</p>
        </div>
      </div>
      <div className="agent-panel__activity-facts" title={agentActivityFactsTitle()}>
        <span title={agentActivityTurnsFactTitle(overview.turns)}>{overview.turns} turns</span>
        <span title={agentActivityFilesFactTitle(overview.files)}>{overview.files} files</span>
        <span title={agentActivityCheckpointFactTitle(activity.checkpoint)}>
          {activity.checkpoint}
        </span>
        <span title={agentActivityThroughputFactTitle(activity.throughput)}>
          {activity.throughput}
        </span>
      </div>
    </section>
  );
}

function AgentTurnFocus({
  canRestore,
  firstTurnAtMs,
  onRestoreTurn,
  restoringTurnId,
  turn,
}: {
  canRestore: boolean;
  firstTurnAtMs: number | null;
  onRestoreTurn: (turn: AgentTurnView) => void;
  restoringTurnId: string | null;
  turn: AgentTurnView | null;
}) {
  if (!turn) {
    return (
      <section
        className="agent-panel__turn-focus"
        aria-label="Focused turn"
        title={focusedTurnIdleContainerTitle()}
      >
        <div className="agent-panel__turn-focus-head">
          <span title={focusedTurnHeadingLabelTitle()}>Focused turn</span>
          <small title={focusedTurnIdleStatusLabelTitle()}>Idle</small>
        </div>
        <strong title={focusedTurnEmptyStateLabelTitle()}>No turn selected</strong>
        <p title={focusedTurnIdleHelperTitle()}>
          The next agent response will appear here as a navigable turn.
        </p>
      </section>
    );
  }

  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const latest = latestActivityText(turn);
  const visibleChanges = turn.changes.slice(0, 4);
  const hiddenChangeCount = Math.max(0, turn.changes.length - visibleChanges.length);
  const artifactSummary = turnArtifactSummary(turn.changes);
  const commandSummary = turnCommandSummaryText(turn);
  const restoreDisabled = !canRestore || !turn.restoreReady || restoringTurnId != null;
  const isRestoringThisTurn =
    turn.restoreCheckpointId != null && restoringTurnId === turn.restoreCheckpointId;
  return (
    <section
      className="agent-panel__turn-focus"
      aria-label="Focused turn"
      title={focusedTurnSelectedContainerTitle(turn.index)}
    >
      <div className="agent-panel__turn-focus-head">
        <span title={focusedTurnHeadingLabelTitle()}>Focused turn</span>
        {timeLabel && (
          <small title={focusedTurnTimeTitle(turn.index, timeLabel)}>{timeLabel}</small>
        )}
      </div>
      <div className="agent-panel__turn-focus-title">
        <strong title={focusedTurnIndexLabelTitle(turn.index)}>Turn {turn.index}</strong>
        <small title={focusedTurnSummaryTitle(turn)}>{turnSummaryLabel(turn)}</small>
      </div>
      <p
        title={
          latest
            ? focusedTurnLatestActivityTitle(turn.index, latest)
            : focusedTurnFallbackTextTitle(turn.index)
        }
      >
        {latest ? compactActivityText(latest) : "No text captured."}
      </p>
      <div
        className="agent-panel__turn-focus-facts"
        title={focusedTurnFactsContainerTitle(
          turn.index,
          turn.commandText.length,
          turn.changes.length,
        )}
      >
        <span title={focusedTurnFactTitle(turn.index, "commands", turn.commandText.length)}>
          {turn.commandText.length} commands
        </span>
        <span title={focusedTurnFactTitle(turn.index, "files", turn.changes.length)}>
          {turn.changes.length} files
        </span>
      </div>
      {artifactSummary.length > 0 && (
        <div
          className="agent-panel__turn-artifacts"
          aria-label="Focused turn artifact summary"
          title={focusedTurnArtifactSummaryContainerTitle(turn.index, artifactSummary.length)}
        >
          {artifactSummary.map((item) => (
            <span key={item.kind} title={turnArtifactSummaryChipTitle(turn.index, item)}>
              {item.kind} {item.count}
            </span>
          ))}
        </div>
      )}
      {commandSummary && (
        <div
          className="agent-panel__turn-commands"
          aria-label="Focused turn command summary"
          title={focusedTurnCommandSummaryContainerTitle(turn.index)}
        >
          <span title={turnCommandSummaryTitle(turn.index, commandSummary)}>
            Recent command {commandSummary}
          </span>
        </div>
      )}
      {visibleChanges.length > 0 && (
        <div
          className="agent-panel__turn-focus-files"
          aria-label="Focused turn files"
          title={focusedTurnFilesContainerTitle(
            turn.index,
            visibleChanges.length,
            hiddenChangeCount,
          )}
        >
          {visibleChanges.map((change) => (
            <span
              key={`${change.kind}:${change.path}`}
              title={focusedTurnFileRowTitle(turn.index, change)}
            >
              {change.kind} {change.path}
            </span>
          ))}
          {hiddenChangeCount > 0 && (
            <span title={focusedTurnHiddenFileOverflowTitle(turn.index, hiddenChangeCount)}>
              +{hiddenChangeCount} more
            </span>
          )}
        </div>
      )}
      <div
        className="agent-panel__turn-focus-actions"
        title={focusedTurnRestoreContainerTitle(turn.index, canRestore, turn.restoreReady)}
      >
        <button
          disabled={restoreDisabled}
          onClick={() => onRestoreTurn(turn)}
          title={focusedTurnRestoreButtonTitle(
            turn.index,
            canRestore,
            turn.restoreReady,
            isRestoringThisTurn,
          )}
          type="button"
        >
          <span title={focusedTurnRestoreLabelTitle(isRestoringThisTurn)}>
            {isRestoringThisTurn ? "Restoring" : "Restore here"}
          </span>
        </button>
      </div>
    </section>
  );
}

function AgentTurn({
  copiedTarget,
  firstTurnAtMs,
  focused,
  onCopyMessage,
  onCopyTurn,
  searchQuery,
  turn,
  turnElementId,
}: {
  copiedTarget: string | null;
  firstTurnAtMs: number | null;
  focused: boolean;
  onCopyMessage: (target: string, text: string) => void;
  onCopyTurn: (target: string, text: string) => void;
  searchQuery: string;
  turn: AgentTurnView;
  turnElementId: string;
}) {
  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const turnTarget = `${turn.id}:turn`;
  const artifactSummary = turnArtifactSummary(turn.changes);
  const commandSummary = turnCommandSummaryText(turn);
  const searchMatches = agentTurnSearchMatches(turn, searchQuery);
  const searchMatchesLabel = `Turn ${turn.index} search matches`;
  const turnCopied = copiedTarget === turnTarget;
  return (
    <article
      aria-current={focused ? "true" : undefined}
      className={`agent-panel__chat-turn${focused ? " agent-panel__chat-turn--focused" : ""}`}
      id={turnElementId}
      title={conversationTurnContainerTitle(turn, focused)}
    >
      <div
        className="agent-panel__chat-turn-head"
        title={conversationTurnHeaderContainerTitle(turn.index)}
      >
        <div
          className="agent-panel__chat-turn-title"
          title={conversationTurnTitleContainerTitle(turn.index)}
        >
          <span title={turnIndexLabelTitle(turn.index)}>Turn {turn.index}</span>
          <small title={turnSummaryTitle(turn)}>{turnSummaryLabel(turn)}</small>
        </div>
        <div
          className="agent-panel__chat-turn-meta"
          title={conversationTurnMetadataContainerTitle(turn.index)}
        >
          {timeLabel && <small title={turnTimeTitle(turn.index, timeLabel)}>{timeLabel}</small>}
          {turn.changes.length > 0 && (
            <small title={turnTouchedFilesTitle(turn.index, turn.changes.length)}>
              {turn.changes.length} files touched
            </small>
          )}
          <button
            className="agent-panel__turn-copy"
            onClick={() => onCopyTurn(turnTarget, turnTranscriptText(turn, firstTurnAtMs))}
            title={turnCopyButtonTitle(turn.index, turnCopied)}
            type="button"
          >
            <span title={turnCopyLabelTitle(turnCopied ? "Copied" : "Copy turn")}>
              {turnCopied ? "Copied" : "Copy turn"}
            </span>
          </button>
        </div>
      </div>
      {artifactSummary.length > 0 && (
        <div
          className="agent-panel__turn-artifacts"
          aria-label={`Turn ${turn.index} artifact summary`}
          title={turnArtifactSummaryContainerTitle(turn.index, artifactSummary.length)}
        >
          {artifactSummary.map((item) => (
            <span key={item.kind} title={turnArtifactSummaryChipTitle(turn.index, item)}>
              {item.kind} {item.count}
            </span>
          ))}
        </div>
      )}
      {commandSummary && (
        <div
          className="agent-panel__turn-commands"
          aria-label={`Turn ${turn.index} command summary`}
          title={turnCommandSummaryContainerTitle(turn.index)}
        >
          <span title={turnCommandSummaryTitle(turn.index, commandSummary)}>
            Recent command {commandSummary}
          </span>
        </div>
      )}
      {searchMatches.length > 0 && (
        <div
          className="agent-panel__turn-search-matches"
          aria-label={searchMatchesLabel}
          title={`${searchMatchesLabel}: why this visible turn matched the transcript search.`}
        >
          {searchMatches.map((match) => (
            <span key={match.key} title={match.title}>
              {match.label}
            </span>
          ))}
        </div>
      )}
      {turn.userText && (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:user`}
          copyTitle={messageCopyButtonTitle(turn.index, "You", copiedTarget === `${turn.id}:user`)}
          kind="user_message"
          label="You"
          onCopy={() => onCopyMessage(`${turn.id}:user`, turn.userText ?? "")}
          text={turn.userText}
          turnIndex={turn.index}
        />
      )}
      {turn.agentText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:agent:${index}`}
          copyTitle={messageCopyButtonTitle(
            turn.index,
            "Agent",
            copiedTarget === `${turn.id}:agent:${index}`,
          )}
          kind="agent_message"
          label="Agent"
          onCopy={() => onCopyMessage(`${turn.id}:agent:${index}`, text)}
          text={text}
          key={`a-${index}`}
          turnIndex={turn.index}
        />
      ))}
      {turn.commandText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:command:${index}`}
          copyTitle={messageCopyButtonTitle(
            turn.index,
            "Command",
            copiedTarget === `${turn.id}:command:${index}`,
          )}
          kind="command_output"
          label="Command"
          onCopy={() => onCopyMessage(`${turn.id}:command:${index}`, text)}
          text={text}
          key={`c-${index}`}
          turnIndex={turn.index}
        />
      ))}
      {turn.systemText.map((text, index) => (
        <AgentMessageBlock
          copied={copiedTarget === `${turn.id}:system:${index}`}
          copyTitle={messageCopyButtonTitle(
            turn.index,
            "System",
            copiedTarget === `${turn.id}:system:${index}`,
          )}
          kind="lifecycle"
          label="System"
          onCopy={() => onCopyMessage(`${turn.id}:system:${index}`, text)}
          text={text}
          key={`s-${index}`}
          turnIndex={turn.index}
        />
      ))}
      {turn.changes.length > 0 && (
        <div
          className="agent-panel__chat-turn-files"
          title={conversationTurnTouchedFilesContainerTitle(turn.index, turn.changes.length)}
        >
          {turn.changes.map((change) => (
            <span
              key={`${change.kind}:${change.path}`}
              title={turnTouchedFileTitle(turn.index, change.kind, change.path)}
            >
              {change.kind} {change.path}
            </span>
          ))}
        </div>
      )}
    </article>
  );
}

function AgentMessageBlock({
  copied,
  copyTitle,
  kind,
  label,
  onCopy,
  text,
  turnIndex,
}: {
  copied: boolean;
  copyTitle: string;
  kind: AgentSessionTimelineItem["kind"];
  label: string;
  onCopy: () => void;
  text: string;
  turnIndex: number;
}) {
  const technical = kind === "command_output";
  const commandSummary = technical ? commandOutputSummary(text) : null;
  const commandSummaryLabel = commandSummary ?? "Command output";
  const collapseCommand = technical && shouldCollapseCommandOutput(text);
  return (
    <div
      className={`agent-panel__message agent-panel__message--${kind}`}
      title={messageBlockContainerTitle(turnIndex, label)}
    >
      <div className="agent-panel__message-head" title={messageHeaderTitle(label)}>
        <div className="agent-panel__message-role" title={messageRoleLabelTitle(label)}>
          {label}
        </div>
        <button
          className="agent-panel__message-copy"
          aria-label={`Copy ${label} message`}
          onClick={onCopy}
          title={copyTitle}
          type="button"
        >
          <span title={messageCopyLabelTitle(copied ? "Copied" : "Copy")}>
            {copied ? "Copied" : "Copy"}
          </span>
        </button>
      </div>
      {technical ? (
        collapseCommand ? (
          <details
            className="agent-panel__command-block"
            title={collapsedCommandBlockTitle(turnIndex)}
          >
            <summary title={collapsedCommandSummaryRowTitle(turnIndex)}>
              <span title={collapsedCommandSummaryLabelTitle(commandSummaryLabel)}>
                {commandSummaryLabel}
              </span>
              <small title={collapsedCommandDisclosureLabelTitle()}>Show output</small>
            </summary>
            <pre
              className="agent-panel__message-terminal"
              title={messageTerminalContentTitle(turnIndex)}
            >
              {text}
            </pre>
          </details>
        ) : (
          <pre
            className="agent-panel__message-terminal"
            title={messageTerminalContentTitle(turnIndex)}
          >
            {text}
          </pre>
        )
      ) : (
        <div
          className="agent-panel__markdown"
          title={messageMarkdownContentTitle(turnIndex, label)}
        >
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
        </div>
      )}
    </div>
  );
}

function shouldCollapseCommandOutput(text: string): boolean {
  return text.length > 360 || text.split(/\r\n|\r|\n/).length > 8;
}

function commandOutputSummary(text: string): string {
  const firstLine = text
    .split(/\r\n|\r|\n/)
    .map((line) => line.trim())
    .find(Boolean);
  if (!firstLine) return "Command output";
  return firstLine.length > 96 ? `${firstLine.slice(0, 93)}...` : firstLine;
}

function agentErrorBannerTitle(error: string): string {
  return `Agent session error banner: ${error}`;
}

function emptyChatContainerTitle(hasTranscriptQuery: boolean, readOnly: boolean): string {
  if (hasTranscriptQuery) {
    return "Agent conversation empty state: no transcript matches for the current search.";
  }
  if (readOnly) {
    return "Agent conversation empty state: archived transcript has no saved timeline items.";
  }
  return "Agent conversation empty state: ready for the first turn.";
}

function emptyChatStateLabelTitle(hasTranscriptQuery: boolean, readOnly: boolean): string {
  if (hasTranscriptQuery) {
    return "Agent conversation empty-state label: No matches.";
  }
  if (readOnly) {
    return "Agent conversation empty-state label: Transcript.";
  }
  return "Agent conversation empty-state label: Ready.";
}

function emptyChatHelperTextTitle(hasTranscriptQuery: boolean, readOnly: boolean): string {
  if (hasTranscriptQuery) {
    return "Agent conversation empty-state helper: try another transcript search.";
  }
  if (readOnly) {
    return "Agent conversation empty-state helper: no timeline items were saved.";
  }
  return "Agent conversation empty-state helper: start a turn from the composer.";
}

function emptyChatClearSearchActionTitle(): string {
  return "Agent conversation empty-state action: clear search, restore all turns, and return focus to search.";
}

function agentHeaderTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent header for ${repoLabel}: identity, status, and session controls.`;
}

function agentPanelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent session surface for ${repoLabel}: header, status, conversation, Agent Lens, and composer.`;
}

function agentWorkspaceTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent workspace for ${repoLabel}: overview, activity, conversation, inspection rail, and composer.`;
}

function agentChatShellTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent chat shell for ${repoLabel}: transcript tools and conversation column.`;
}

function agentSideRailTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent side rail for ${repoLabel}: focused turn and Agent Lens column.`;
}

function agentComposerTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent composer for ${repoLabel}: command menu, skill mentions, and message input.`;
}

function agentComposerActionsTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  canCompose: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (readOnly) {
    return `${agentLabel(agentType)} command menu for ${repoLabel}: archived transcripts are read-only.`;
  }
  if (!canCompose) {
    return `${agentLabel(agentType)} command menu for ${repoLabel}: waiting for a writable session.`;
  }
  return `${agentLabel(agentType)} command menu for ${repoLabel}: type / for Codex and Tinto commands or $ for skills.`;
}

function agentCommandHintTitle(canCompose: boolean, readOnly: boolean): string {
  if (readOnly) return "Composer commands are disabled because this transcript is archived.";
  if (!canCompose) return "Composer commands are waiting for a writable session.";
  return "Composer command hint: type / for Codex and Tinto commands or $ for skills.";
}

function agentCommandScopeHintTitle(): string {
  return "Composer command scopes: Codex prompt commands, Tinto session commands, and skills.";
}

function agentCommandMenuTitle(
  trigger: { trigger: AgentComposerCommandTrigger; query: string } | null,
  count: number,
): string {
  const prefix = trigger?.trigger ?? "/";
  const query = trigger?.query ?? "";
  const suffix = query ? ` matching ${prefix}${query}` : "";
  const noun = count === 1 ? "command" : "commands";
  return `Composer command menu: ${count} ${noun}${suffix}.`;
}

function agentComposerCommandTitle(command: AgentComposerCommand): string {
  const state = command.disabled ? "Unavailable" : "Run";
  const aliases = agentComposerCommandAliasTitle(command);
  return `${state} ${command.trigger}${command.command}: ${command.description}.${aliases}`;
}

function agentComposerCommandCodeTitle(command: AgentComposerCommand): string {
  return `Composer command trigger: ${command.trigger}${command.command}.`;
}

function agentComposerCommandLabelTitle(label: string): string {
  return `Composer command label: ${label}.`;
}

function agentComposerCommandDescriptionTitle(command: AgentComposerCommand): string {
  const aliases = agentComposerCommandAliasTitle(command);
  return `${command.scope} composer command description for ${command.trigger}${command.command}: ${command.description}.${aliases}`;
}

function agentComposerCommandAliasText(command: AgentComposerCommand): string {
  const aliases = command.aliases?.slice(0, 3) ?? [];
  if (aliases.length === 0) return "";
  return ` · Also ${aliases.map((alias) => `${command.trigger}${alias}`).join(", ")}`;
}

function agentComposerCommandAliasTitle(command: AgentComposerCommand): string {
  const aliases = command.aliases ?? [];
  if (aliases.length === 0) return "";
  return ` Aliases: ${aliases.map((alias) => `${command.trigger}${alias}`).join(", ")}.`;
}

function agentCommandEmptyTitle(
  trigger: { trigger: AgentComposerCommandTrigger; query: string } | null,
): string {
  const prefix = trigger?.trigger ?? "/";
  return `No composer commands match ${prefix}${trigger?.query ?? ""}.`;
}

function readComposerCommandTrigger(
  value: string,
): { trigger: AgentComposerCommandTrigger; query: string } | null {
  const match = value.match(COMPOSER_COMMAND_TRIGGER_RE);
  const trigger = match?.[2];
  if (trigger !== "/" && trigger !== "$") return null;
  return { trigger, query: match?.[3] ?? "" };
}

function filterComposerCommands(
  commands: AgentComposerCommand[],
  trigger: { trigger: AgentComposerCommandTrigger; query: string } | null,
): AgentComposerCommand[] {
  if (!trigger) return [];
  const normalized = normalizeComposerCommandToken(trigger.query);
  const scopedCommands = commands.filter((command) => command.trigger === trigger.trigger);
  if (!normalized) return scopedCommands;
  return scopedCommands
    .map((command) => ({ command, score: composerCommandMatchScore(command, normalized) }))
    .filter((item) => item.score < Number.POSITIVE_INFINITY)
    .sort((left, right) => left.score - right.score)
    .map((item) => item.command);
}

function composerCommandMatchesName(command: AgentComposerCommand, name: string | undefined) {
  if (!name) return false;
  return (
    normalizeComposerCommandToken(command.command) === name ||
    (command.aliases ?? []).some((alias) => normalizeComposerCommandToken(alias) === name)
  );
}

function composerCommandMatchScore(command: AgentComposerCommand, normalizedQuery: string): number {
  const commandName = normalizeComposerCommandToken(command.command);
  const aliases = (command.aliases ?? []).map((alias) => normalizeComposerCommandToken(alias));
  if (commandName === normalizedQuery || aliases.includes(normalizedQuery)) return 0;
  if (commandName.startsWith(normalizedQuery)) return 1;
  if (aliases.some((alias) => alias.startsWith(normalizedQuery))) return 2;
  const haystack = [command.command, ...aliases, command.label, command.scope, command.description]
    .join(" ")
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .trim();
  return haystack.includes(normalizedQuery) ? 3 : Number.POSITIVE_INFINITY;
}

function normalizeComposerCommandToken(value: string | undefined): string {
  return (value ?? "")
    .trim()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/\s+/g, "-");
}

function isDeferredMemoryCommand(commandName: string): boolean {
  return commandName === "memory" || commandName === "memories" || commandName === "memorias";
}

function nextComposerCommandIndex(current: number, count: number, forward: boolean): number {
  if (count <= 0) return 0;
  if (forward) return (current + 1) % count;
  return current <= 0 ? count - 1 : current - 1;
}

function replaceDraftComposerCommand(current: string, replacement: string): string {
  const match = current.match(COMPOSER_COMMAND_TRIGGER_RE);
  if (!match || match.index == null) {
    const trimmed = current.trimEnd();
    return trimmed ? `${trimmed}\n\n${replacement}` : replacement;
  }
  const prefix = current.slice(0, match.index);
  const trimmedPrefix = prefix.trimEnd();
  return trimmedPrefix ? `${trimmedPrefix}\n\n${replacement}` : replacement;
}

function clearDraftComposerCommand(current: string): string {
  const match = current.match(COMPOSER_COMMAND_LINE_RE);
  if (!match || match.index == null) return current;
  const prefix = current.slice(0, match.index);
  return match[1] === "\n" ? prefix : prefix.trimEnd();
}

function codexRuntimeOptions(
  model: CodexModelSelection,
  reasoning: CodexReasoningSelection,
  speed: CodexSpeedSelection,
): AgentSessionRuntimeOptions {
  const resolvedModel = model === "auto" && speed === "fast" ? "gpt-5.4-mini" : model;
  const resolvedReasoning = reasoning === "auto" && speed === "fast" ? "low" : reasoning;
  const options: AgentSessionRuntimeOptions = { speed };
  if (resolvedModel !== "auto") options.model = resolvedModel;
  if (resolvedReasoning !== "auto") options.reasoning_effort = resolvedReasoning;
  return options;
}

function runtimeSelectionsFromOptions(options: AgentSessionRuntimeOptions): {
  model: CodexModelSelection;
  reasoning: CodexReasoningSelection;
  speed: CodexSpeedSelection;
} {
  return {
    model: isCodexModelSelection(options.model) ? options.model : "auto",
    reasoning: isCodexReasoningSelection(options.reasoning_effort)
      ? options.reasoning_effort
      : "auto",
    speed: options.speed === "fast" ? "fast" : "standard",
  };
}

function applyCodexRuntimeSlashCommand(
  text: string,
  setters: {
    setModel: (value: CodexModelSelection) => void;
    setNotice: (value: string) => void;
    setReasoning: (value: CodexReasoningSelection) => void;
    setSpeed: (value: CodexSpeedSelection) => void;
  },
): boolean {
  const match = text.trim().match(/^\/([^\s/]+)(?:\s+(.+))?$/);
  if (!match) return false;
  const command = normalizeComposerCommandToken(match[1]);
  const rawValue = normalizeComposerCommandToken(match[2]);
  if (
    command === "fast" ||
    command === "speed" ||
    command === "velocidad" ||
    command === "rapido" ||
    command === "rapida"
  ) {
    const next = normalizeCodexSpeed(rawValue);
    setters.setSpeed(next);
    setters.setNotice(next === "fast" ? "Fast preset enabled." : "Fast preset disabled.");
    return true;
  }
  if (command === "model" || command === "modelo") {
    const model = normalizeCodexModel(rawValue);
    setters.setModel(model);
    setters.setNotice(`Model set to ${codexModelLabel(model)}.`);
    return true;
  }
  if (command !== "reasoning" && command !== "razonamiento" && command !== "effort") {
    return false;
  }
  const reasoning = normalizeCodexReasoning(rawValue);
  setters.setReasoning(reasoning);
  setters.setNotice(`Reasoning set to ${codexReasoningLabel(reasoning)}.`);
  return true;
}

function normalizeCodexModel(value: string | undefined): CodexModelSelection {
  if (!value || value === "default") return "auto";
  const normalized = normalizeComposerCommandToken(value);
  return isCodexModelSelection(normalized) ? normalized : "auto";
}

function normalizeCodexReasoning(value: string | undefined): CodexReasoningSelection {
  if (!value || value === "default" || value === "predeterminado") return "auto";
  const normalized = normalizeComposerCommandToken(value)
    .replace(/^minimo$/, "minimal")
    .replace(/^mínimo$/, "minimal")
    .replace(/^bajo$/, "low")
    .replace(/^medio$/, "medium")
    .replace(/^alto$/, "high")
    .replace(/^extremadamente-alto$/, "xhigh")
    .replace(/^extra$/, "xhigh")
    .replace(/^extra-high$/, "xhigh");
  return isCodexReasoningSelection(normalized) ? normalized : "auto";
}

function normalizeCodexSpeed(value: string | undefined): CodexSpeedSelection {
  if (
    value === "off" ||
    value === "false" ||
    value === "standard" ||
    value === "normal" ||
    value === "estandar" ||
    value === "estándar"
  ) {
    return "standard";
  }
  return "fast";
}

function isCodexModelSelection(value: unknown): value is CodexModelSelection {
  return CODEX_MODEL_OPTIONS.some((option) => option.value === value);
}

function isCodexReasoningSelection(value: unknown): value is CodexReasoningSelection {
  return CODEX_REASONING_OPTIONS.some((option) => option.value === value);
}

function codexModelLabel(value: CodexModelSelection): string {
  return CODEX_MODEL_OPTIONS.find((option) => option.value === value)?.label ?? "Auto";
}

function codexModelShortLabel(value: CodexModelSelection): string {
  if (value === "auto") return "Model";
  return codexModelLabel(value).replace(/^GPT-/, "");
}

function codexReasoningLabel(value: CodexReasoningSelection): string {
  return CODEX_REASONING_OPTIONS.find((option) => option.value === value)?.label ?? "Auto";
}

function codexReasoningShortLabel(value: CodexReasoningSelection): string {
  if (value === "auto") return "Reasoning";
  return codexReasoningLabel(value);
}

function codexSpeedLabel(value: CodexSpeedSelection): string {
  return CODEX_SPEED_OPTIONS.find((option) => option.value === value)?.label ?? "Normal";
}

function agentComposerRowTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent composer input row for ${repoLabel}: message draft and send control.`;
}

function agentComposerInputTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  canCompose: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (readOnly) {
    return `${agentLabel(agentType)} agent message input for ${repoLabel}: archived transcript is read-only.`;
  }
  if (!canCompose) {
    return `${agentLabel(agentType)} agent message input for ${repoLabel}: waiting for a writable session.`;
  }
  return `${agentLabel(agentType)} agent message input for ${repoLabel}: draft the next instruction.`;
}

function agentComposerSendTitle(
  agentType: string,
  repo: string | undefined,
  canSend: boolean,
  sending: boolean,
  readOnly: boolean,
  canCompose: boolean,
  hasDraft: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (sending) {
    return `${agentLabel(agentType)} agent send control for ${repoLabel}: sending drafted message.`;
  }
  if (readOnly) {
    return `${agentLabel(agentType)} agent send control for ${repoLabel}: archived transcript is read-only.`;
  }
  if (!canCompose) {
    return `${agentLabel(agentType)} agent send control for ${repoLabel}: waiting for a writable session.`;
  }
  if (!hasDraft) {
    return `${agentLabel(agentType)} agent send control for ${repoLabel}: message input is empty.`;
  }
  if (!canSend) {
    return `${agentLabel(agentType)} agent send control for ${repoLabel}: message input is empty or unavailable.`;
  }
  return `${agentLabel(agentType)} agent send control for ${repoLabel}: send drafted message.`;
}

function agentComposerSendLabelTitle(sending: boolean): string {
  return `Composer send button label: ${sending ? "Sending" : "Send"}.`;
}

function agentIdentityTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent identity for ${repoLabel}: agent name and repo label.`;
}

function agentHeaderActionsTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent header actions for ${repoLabel}: Stop and Revert controls.`;
}

function agentStopControlTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  canStop: boolean,
  stopping: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (stopping) {
    return `${agentLabel(agentType)} stop control for ${repoLabel}: stopping session.`;
  }
  if (readOnly) {
    return `${agentLabel(agentType)} stop control for ${repoLabel}: archived transcripts are read-only.`;
  }
  if (canStop) {
    return `${agentLabel(agentType)} stop control for ${repoLabel}: stop the running session.`;
  }
  return `${agentLabel(agentType)} stop control for ${repoLabel}: session is not running.`;
}

function agentStopControlLabelTitle(stopping: boolean): string {
  return `Agent stop control label: ${stopping ? "Stopping" : "Stop"}.`;
}

function agentRevertControlTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  session: AgentSession | null,
  canRevert: boolean,
  reverting: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (reverting) {
    return `${agentLabel(agentType)} revert control for ${repoLabel}: reverting session changes.`;
  }
  if (readOnly) {
    return `${agentLabel(agentType)} revert control for ${repoLabel}: archived transcripts are read-only.`;
  }
  if (session?.status === "reverted") {
    return `${agentLabel(agentType)} revert control for ${repoLabel}: session already reverted.`;
  }
  if (session && !session.checkpoint) {
    return `${agentLabel(agentType)} revert control for ${repoLabel}: no reversible checkpoint.`;
  }
  if (canRevert) {
    return `${agentLabel(agentType)} revert control for ${repoLabel}: revert session changes.`;
  }
  return `${agentLabel(agentType)} revert control for ${repoLabel}: stop the session before reverting.`;
}

function agentRevertControlLabelTitle(reverting: boolean): string {
  return `Agent revert control label: ${reverting ? "Reverting" : "Revert"}.`;
}

function agentLogoTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent logo for ${repoLabel}: agent mark.`;
}

function agentRepoLabelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  if (!repo) {
    return `${agentLabel(agentType)} agent repo label for ${repoLabel}: no repo path.`;
  }
  return `${agentLabel(agentType)} agent repo label for ${repoLabel}: full path ${repo}.`;
}

function agentNameLabelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "Agent session";
  return `${agentLabel(agentType)} agent display-name label for ${repoLabel}.`;
}

function collapsedCommandSummaryLabelTitle(summary: string): string {
  return `Collapsed command output summary: ${summary}.`;
}

function collapsedCommandDisclosureLabelTitle(): string {
  return "Collapsed command output disclosure label: Show output.";
}

function collapsedCommandBlockTitle(turnIndex: number): string {
  return `Collapsed command output container for turn ${turnIndex}: summary disclosure and full output text.`;
}

function collapsedCommandSummaryRowTitle(turnIndex: number): string {
  return `Collapsed command output summary row for turn ${turnIndex}: click to show or hide full output.`;
}

function messageRoleLabelTitle(label: string): string {
  return `Agent message role label: ${label}.`;
}

function messageBlockContainerTitle(turnIndex: number, label: string): string {
  return `Agent message block for turn ${turnIndex}: ${label} content and copy control.`;
}

function messageMarkdownContentTitle(turnIndex: number, label: string): string {
  return `Rendered ${label} Markdown content for turn ${turnIndex}.`;
}

function messageTerminalContentTitle(turnIndex: number): string {
  return `Command output text for turn ${turnIndex}.`;
}

function messageHeaderTitle(label: string): string {
  return `Agent message header for ${label}: role label and copy control.`;
}

function SessionStatus({ session }: { session: AgentSession | undefined }) {
  if (!session) {
    return (
      <div className="agent-panel__status-strip" title={loadingSessionStatusTitle()}>
        <span title={loadingSessionStatusLabelTitle()}>Loading session</span>
      </div>
    );
  }
  return (
    <div className="agent-panel__status-strip" title={auditTitle(session)}>
      <span
        className={`agent-panel__status agent-panel__status--${session.status}`}
        title={sessionStatusFacetTitle(session.status)}
      >
        {sessionStatusLabel(session.status)}
      </span>
      <span title={turnStatusFacetTitle(session.turn_status ?? "waiting")}>
        {turnStatusLabel(session.turn_status ?? "waiting")}
      </span>
      <span title={checkpointStatusFacetTitle(session.checkpoint?.checkpoint_type)}>
        {session.checkpoint ? checkpointLabel(session.checkpoint.checkpoint_type) : "No checkpoint"}
      </span>
      {(session.change_log?.length ?? 0) > 0 && (
        <span title={changeLogStatusFacetTitle(session.change_log?.length ?? 0)}>
          {session.change_log?.length} changes
        </span>
      )}
    </div>
  );
}

function AgentLens({
  session,
  turns,
  focusedTurn,
  repo,
  canRevertTurnFile,
  canPromptForFile,
  revertingFile,
  onOpenFile,
  onPromptFile,
  onRevertTurnFile,
}: {
  session: AgentSession;
  turns: AgentTurnView[];
  focusedTurn: AgentTurnView | null;
  repo: string | undefined;
  canRevertTurnFile: boolean;
  canPromptForFile: boolean;
  revertingFile: string | null;
  onOpenFile: (path: string) => void;
  onPromptFile: (context: AgentLensFilePromptContext) => void;
  onRevertTurnFile: (turnCheckpointId: string, path: string) => void;
}) {
  const [activeTab, setActiveTab] = useState<AgentLensTab>("files");
  const [scope, setScope] = useState<AgentLensScope>("focused");
  const [fileQuery, setFileQuery] = useState("");
  const [commandQuery, setCommandQuery] = useState("");
  const [timelineQuery, setTimelineQuery] = useState("");
  const [previewFileId, setPreviewFileId] = useState<string | null>(null);
  const fileFilterRef = useRef<HTMLInputElement | null>(null);
  const commandFilterRef = useRef<HTMLInputElement | null>(null);
  const timelineFilterRef = useRef<HTMLInputElement | null>(null);
  const activeScope = focusedTurn ? scope : "session";
  const focusedTurnIndex = activeScope === "focused" ? (focusedTurn?.index ?? null) : null;
  const turnCheckpoints = session.turn_checkpoints ?? [];
  const restorableTurnCheckpoints = turnCheckpoints.filter((checkpoint) =>
    Boolean(checkpoint.restore_checkpoint),
  );
  const latestRestorableTurn = restorableTurnCheckpoints[restorableTurnCheckpoints.length - 1];
  const busState = useBusState();
  const liveRepo = repo ? busState.repos[repo] : undefined;
  const liveDiffs = repo ? busState.diffs[repo] : undefined;
  const fileItems = agentLensFileItems(
    turnCheckpoints,
    session.change_log ?? [],
    focusedTurnIndex,
  ).map((item) => ({
    ...item,
    artifactKind: agentLensArtifactKind(item.path),
    context: agentLensFileContext(item.path, liveRepo?.status, liveDiffs),
  }));
  const filteredFileItems = filterAgentLensFileItems(fileItems, fileQuery);
  const groupedFileItems = groupAgentLensFileItems(filteredFileItems);
  const previewItem =
    filteredFileItems.find((item) => item.id === previewFileId) ??
    filteredFileItems.find((item) => item.context?.preview) ??
    filteredFileItems[0] ??
    null;
  const previewIndex = previewItem
    ? filteredFileItems.findIndex((item) => item.id === previewItem.id)
    : -1;
  const previewCount = filteredFileItems.length;
  const previewPosition = previewIndex >= 0 ? previewIndex + 1 : 0;
  const previousPreviewItem =
    previewCount > 0 && previewIndex >= 0
      ? filteredFileItems[(previewIndex - 1 + previewCount) % previewCount]
      : null;
  const nextPreviewItem =
    previewCount > 0 && previewIndex >= 0
      ? filteredFileItems[(previewIndex + 1) % previewCount]
      : null;
  const previewRevertKey = previewItem?.turnCheckpointId
    ? `${previewItem.turnCheckpointId}:${previewItem.path}`
    : null;
  const isPreviewReverting = Boolean(previewRevertKey && revertingFile === previewRevertKey);
  const hasFileQuery = fileQuery.trim().length > 0;
  const commandItems = agentLensCommandItems(turns, focusedTurnIndex);
  const timelineItems = agentLensTimelineItems(turns, focusedTurnIndex);
  const filteredCommandItems = filterAgentLensCommandItems(commandItems, commandQuery);
  const filteredTimelineItems = filterAgentLensTimelineItems(timelineItems, timelineQuery);
  const hasCommandQuery = commandQuery.trim().length > 0;
  const hasTimelineQuery = timelineQuery.trim().length > 0;
  const clearFileFilter = () => {
    setFileQuery("");
    requestAnimationFrame(() => fileFilterRef.current?.focus());
  };
  const clearCommandFilter = () => {
    setCommandQuery("");
    requestAnimationFrame(() => commandFilterRef.current?.focus());
  };
  const clearTimelineFilter = () => {
    setTimelineQuery("");
    requestAnimationFrame(() => timelineFilterRef.current?.focus());
  };
  const scopeLabel =
    activeScope === "focused" && focusedTurn
      ? `Turn ${focusedTurn.index}`
      : `${turns.length} turns`;
  const activateTab = (tab: AgentLensTab) => setActiveTab(tab);
  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>, tab: AgentLensTab) => {
    const currentIndex = AGENT_LENS_TAB_ORDER.indexOf(tab);
    let nextTab: AgentLensTab | null = null;
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      nextTab = AGENT_LENS_TAB_ORDER[(currentIndex + 1) % AGENT_LENS_TAB_ORDER.length];
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      nextTab =
        AGENT_LENS_TAB_ORDER[
          (currentIndex - 1 + AGENT_LENS_TAB_ORDER.length) % AGENT_LENS_TAB_ORDER.length
        ];
    } else if (event.key === "Home") {
      nextTab = AGENT_LENS_TAB_ORDER[0];
    } else if (event.key === "End") {
      nextTab = AGENT_LENS_TAB_ORDER[AGENT_LENS_TAB_ORDER.length - 1];
    }
    if (!nextTab) return;
    event.preventDefault();
    setActiveTab(nextTab);
    requestAnimationFrame(() => {
      document.getElementById(agentLensTabId(session.id, nextTab))?.focus();
    });
  };
  const selectPreviewIndex = (index: number) => {
    if (previewCount === 0) return;
    const nextIndex = (index + previewCount) % previewCount;
    setPreviewFileId(filteredFileItems[nextIndex]?.id ?? null);
  };
  const selectPreviousPreview = () => {
    selectPreviewIndex(previewIndex <= 0 ? previewCount - 1 : previewIndex - 1);
  };
  const selectNextPreview = () => {
    selectPreviewIndex(previewIndex < 0 || previewIndex >= previewCount - 1 ? 0 : previewIndex + 1);
  };
  const handlePreviewKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (previewCount < 2) return;
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      selectPreviousPreview();
    } else if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      selectNextPreview();
    }
  };
  return (
    <aside
      className="agent-panel__lens"
      aria-label="Agent Lens"
      title={agentLensRootTitle(
        activeScope,
        activeTab,
        focusedTurn?.index ?? null,
        turns.length,
        fileItems.length,
        commandItems.length,
        timelineItems.length,
        session.turn_status ?? "waiting",
      )}
    >
      <div
        className="agent-panel__lens-head"
        title={agentLensHeadingTitle(activeScope, focusedTurn?.index ?? null, turns.length)}
      >
        <span title={agentLensHeadingLabelTitle()}>Agent Lens</span>
        <small
          title={agentLensScopeLabelTitle(activeScope, focusedTurn?.index ?? null, turns.length)}
        >
          {scopeLabel}
        </small>
      </div>
      <div
        className="agent-panel__lens-scope"
        aria-label="Agent Lens scope"
        title={agentLensScopeGroupTitle(activeScope, focusedTurn?.index ?? null)}
      >
        <button
          aria-pressed={activeScope === "focused"}
          disabled={!focusedTurn}
          onClick={() => setScope("focused")}
          title={agentLensScopeTitle("focused", focusedTurn?.index ?? null)}
          type="button"
        >
          Focus
        </button>
        <button
          aria-pressed={activeScope === "session"}
          onClick={() => setScope("session")}
          title={agentLensScopeTitle("session", focusedTurn?.index ?? null)}
          type="button"
        >
          Session
        </button>
      </div>
      <div
        className="agent-panel__lens-metrics"
        title={agentLensMetricsTitle(
          activeScope,
          fileItems.length,
          turnCheckpoints.length,
          restorableTurnCheckpoints.length,
          session.turn_status ?? "waiting",
        )}
      >
        <div>
          <span title={agentLensTurnStateValueTitle(session.turn_status ?? "waiting")}>
            {turnStatusLabel(session.turn_status ?? "waiting")}
          </span>
          <small title={agentLensTurnStateMetricTitle(session.turn_status ?? "waiting")}>
            Turn state
          </small>
        </div>
        <div>
          <span title={agentLensFileMetricValueTitle(activeScope, fileItems.length)}>
            {fileItems.length}
          </span>
          <small title={agentLensFileMetricTitle(activeScope, fileItems.length)}>
            {activeScope === "focused" ? "Focused files" : "Session files"}
          </small>
        </div>
        <div>
          <span
            title={agentLensRestoreMetricValueTitle(
              turnCheckpoints.length,
              restorableTurnCheckpoints.length,
              latestRestorableTurn?.index ?? null,
            )}
          >
            {restorableTurnCheckpoints.length}/{turnCheckpoints.length}
          </span>
          <small
            title={agentLensRestoreMetricTitle(
              turnCheckpoints.length,
              restorableTurnCheckpoints.length,
              latestRestorableTurn?.index ?? null,
            )}
          >
            Restore points
          </small>
        </div>
      </div>

      <div
        className="agent-panel__lens-tabs"
        role="tablist"
        aria-label="Agent Lens views"
        aria-orientation="horizontal"
        title={agentLensTabListTitle(fileItems.length, commandItems.length, timelineItems.length)}
      >
        <AgentLensTabButton
          active={activeTab === "files"}
          controlsId={agentLensPanelId(session.id, "files")}
          count={fileItems.length}
          id={agentLensTabId(session.id, "files")}
          label="Files"
          onClick={() => activateTab("files")}
          onKeyDown={(event) => handleTabKeyDown(event, "files")}
        />
        <AgentLensTabButton
          active={activeTab === "commands"}
          controlsId={agentLensPanelId(session.id, "commands")}
          count={commandItems.length}
          id={agentLensTabId(session.id, "commands")}
          label="Commands"
          onClick={() => activateTab("commands")}
          onKeyDown={(event) => handleTabKeyDown(event, "commands")}
        />
        <AgentLensTabButton
          active={activeTab === "timeline"}
          controlsId={agentLensPanelId(session.id, "timeline")}
          count={timelineItems.length}
          id={agentLensTabId(session.id, "timeline")}
          label="Timeline"
          onClick={() => activateTab("timeline")}
          onKeyDown={(event) => handleTabKeyDown(event, "timeline")}
        />
      </div>

      {activeTab === "files" && (
        <div
          className="agent-panel__lens-view"
          aria-label="Agent Lens Files view"
          aria-labelledby={agentLensTabId(session.id, "files")}
          id={agentLensPanelId(session.id, "files")}
          role="tabpanel"
          title={agentLensViewContainerTitle(
            "files",
            activeScope,
            fileItems.length,
            filteredFileItems.length,
            hasFileQuery,
          )}
        >
          {fileItems.length > 0 ? (
            <>
              <label
                className="agent-panel__lens-filter"
                title={agentLensFileFilterWrapperTitle(
                  fileItems.length,
                  filteredFileItems.length,
                  hasFileQuery,
                )}
              >
                <span title={agentLensFileFilterLabelTitle()}>Filter files</span>
                <input
                  aria-describedby={agentLensFileFilterStatusId(session.id)}
                  aria-label="Filter touched files"
                  ref={fileFilterRef}
                  value={fileQuery}
                  onChange={(event) => setFileQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasFileQuery) {
                      event.preventDefault();
                      clearFileFilter();
                    }
                  }}
                  placeholder="Path or change type..."
                  title={agentLensFileFilterTitle(fileItems.length, hasFileQuery)}
                  type="search"
                />
                <small
                  aria-live="polite"
                  id={agentLensFileFilterStatusId(session.id)}
                  title={agentLensFileFilterCountTitle(
                    filteredFileItems.length,
                    fileItems.length,
                    hasFileQuery,
                  )}
                >
                  {hasFileQuery
                    ? `${filteredFileItems.length} of ${fileItems.length} files`
                    : `${fileItems.length} files`}
                </small>
                {hasFileQuery && (
                  <button
                    className="agent-panel__lens-filter-clear"
                    onClick={clearFileFilter}
                    title={agentLensClearFileFilterTitle(
                      filteredFileItems.length,
                      fileItems.length,
                    )}
                    type="button"
                  >
                    <span title={agentLensClearFileFilterLabelTitle()}>Clear</span>
                  </button>
                )}
              </label>
              {filteredFileItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Touched files"
                  title={agentLensTouchedFilesListTitle(
                    activeScope,
                    filteredFileItems.length,
                    hasFileQuery,
                  )}
                >
                  <div
                    className="agent-panel__lens-preview"
                    aria-label="Selected file preview"
                    onKeyDown={handlePreviewKeyDown}
                    tabIndex={previewCount > 1 ? 0 : undefined}
                    title={agentLensPreviewContainerTitle(
                      previewItem?.path ?? null,
                      previewPosition,
                      previewCount,
                    )}
                  >
                    <div className="agent-panel__lens-preview-head">
                      <span title={agentLensPreviewLabelTitle()}>Preview</span>
                      <small title={agentLensPreviewPositionTitle(previewPosition, previewCount)}>
                        {previewPosition} / {previewCount}
                      </small>
                    </div>
                    <strong title={agentLensPreviewSelectionTitle(previewItem?.path ?? null)}>
                      {previewItem?.path ?? "No file selected"}
                    </strong>
                    {previewCount > 1 && (
                      <div
                        aria-label="Preview navigation"
                        className="agent-panel__lens-preview-nav"
                        title={agentLensPreviewNavigationTitle(
                          previewItem?.path ?? null,
                          previewCount,
                        )}
                      >
                        <button
                          onClick={selectPreviousPreview}
                          title={agentLensPreviewNavButtonTitle(
                            "previous",
                            previousPreviewItem?.path ?? null,
                          )}
                          type="button"
                        >
                          <span title={agentLensPreviewNavLabelTitle("Previous")}>Previous</span>
                        </button>
                        <button
                          onClick={selectNextPreview}
                          title={agentLensPreviewNavButtonTitle(
                            "next",
                            nextPreviewItem?.path ?? null,
                          )}
                          type="button"
                        >
                          <span title={agentLensPreviewNavLabelTitle("Next")}>Next</span>
                        </button>
                      </div>
                    )}
                    {previewItem && (
                      <div
                        aria-label={`Preview actions for ${previewItem.path}`}
                        className="agent-panel__lens-preview-actions"
                        title={agentLensPreviewActionsTitle(
                          previewItem.path,
                          Boolean(previewItem.turnCheckpointId),
                        )}
                      >
                        <button
                          aria-label="Open selected preview file"
                          disabled={!repo}
                          onClick={() => onOpenFile(previewItem.path)}
                          title={agentLensOpenActionTitle(previewItem.path, Boolean(repo))}
                          type="button"
                        >
                          <span title={agentLensPreviewActionLabelTitle("Open")}>Open</span>
                        </button>
                        <button
                          aria-label="Ask about selected preview file"
                          disabled={!canPromptForFile}
                          onClick={() =>
                            onPromptFile({
                              path: previewItem.path,
                              kind: previewItem.kind,
                              turnIndex: previewItem.turnIndex,
                              artifactKind: previewItem.artifactKind,
                              hunkSummary: previewItem.context?.preview?.summary ?? null,
                            })
                          }
                          title={agentLensAskActionTitle(previewItem.path, canPromptForFile)}
                          type="button"
                        >
                          <span title={agentLensPreviewActionLabelTitle("Ask")}>Ask</span>
                        </button>
                        {previewItem.turnCheckpointId && (
                          <button
                            aria-label="Revert selected preview file"
                            disabled={!canRevertTurnFile || isPreviewReverting}
                            onClick={() =>
                              onRevertTurnFile(previewItem.turnCheckpointId ?? "", previewItem.path)
                            }
                            title={agentLensPreviewRevertActionTitle(
                              previewItem.path,
                              previewItem.turnIndex,
                              canRevertTurnFile,
                              isPreviewReverting,
                            )}
                            type="button"
                          >
                            <span
                              title={agentLensPreviewActionLabelTitle(
                                isPreviewReverting ? "Reverting" : "Revert",
                              )}
                            >
                              {isPreviewReverting ? "Reverting" : "Revert"}
                            </span>
                          </button>
                        )}
                      </div>
                    )}
                    {previewItem?.context?.preview ? (
                      <div
                        aria-label={`Preview details for ${previewItem.path}`}
                        className="agent-panel__lens-preview-detail"
                        title={agentLensPreviewDetailGroupTitle(previewItem.path)}
                      >
                        <small title={previewItem.context.preview.summaryTitle}>
                          {previewItem.context.preview.summary}
                        </small>
                        <p title={previewItem.context.preview.detailTitle}>
                          {previewItem.context.preview.detail}
                        </p>
                      </div>
                    ) : (
                      <p title={agentLensNoLiveHunkTitle(previewItem?.path ?? null)}>
                        No live hunk data available for this file.
                      </p>
                    )}
                  </div>
                  {groupedFileItems.map((group) => (
                    <section
                      aria-label={`${group.kind} files`}
                      className="agent-panel__lens-file-group"
                      key={group.kind}
                      title={agentLensFileGroupTitle(group.kind, group.items.length)}
                    >
                      <div
                        className="agent-panel__lens-file-group-head"
                        title={agentLensFileGroupHeaderTitle(group.kind, group.items.length)}
                      >
                        <span title={agentLensFileGroupKindLabelTitle(group.kind)}>
                          {group.kind}
                        </span>
                        <small title={agentLensFileGroupCountTitle(group.kind, group.items.length)}>
                          {group.items.length}
                        </small>
                      </div>
                      {group.items.map((item) => {
                        const key = `${item.turnCheckpointId}:${item.path}`;
                        return (
                          <div
                            className={`agent-panel__lens-file${
                              item.turnCheckpointId ? "" : " agent-panel__lens-file--readonly"
                            }`}
                            key={item.id}
                            title={agentLensFileTitle(item.turnIndex, item.kind, item.path)}
                          >
                            <span
                              title={agentLensFileScopeMetaTitle(item.turnIndex, item.timeLabel)}
                            >
                              {item.turnIndex ? `Turn ${item.turnIndex}` : "Session"}
                              {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                            </span>
                            <strong title={agentLensFilePathMetaTitle(item.path)}>
                              {item.path}
                            </strong>
                            <small title={agentLensFileKindMetaTitle(item.artifactKind, item.kind)}>
                              {item.kind}
                            </small>
                            {item.context && (
                              <div
                                aria-label={`Live context for ${item.path}`}
                                className="agent-panel__lens-file-context"
                                title={agentLensLiveContextTitle(item.path)}
                              >
                                {item.context.statusChips.map((chip) => (
                                  <span key={chip.label} title={chip.title}>
                                    {chip.label}
                                  </span>
                                ))}
                                <span title={item.context.diffTitle}>{item.context.diffLabel}</span>
                                {item.context.renameLabel && (
                                  <span title={item.context.renameTitle}>
                                    {item.context.renameLabel}
                                  </span>
                                )}
                              </div>
                            )}
                            <div
                              aria-label={`File actions for ${item.path}`}
                              className="agent-panel__lens-file-actions"
                              title={agentLensFileActionsTitle(
                                item.path,
                                Boolean(item.turnCheckpointId),
                              )}
                            >
                              <button
                                aria-pressed={previewItem?.id === item.id}
                                onClick={() => setPreviewFileId(item.id)}
                                title={agentLensPreviewActionTitle(
                                  item.path,
                                  previewItem?.id === item.id,
                                )}
                                type="button"
                              >
                                <span title={agentLensFileActionLabelTitle("Preview")}>
                                  Preview
                                </span>
                              </button>
                              <button
                                disabled={!repo}
                                onClick={() => onOpenFile(item.path)}
                                title={agentLensOpenActionTitle(item.path, Boolean(repo))}
                                type="button"
                              >
                                <span title={agentLensFileActionLabelTitle("Open")}>Open</span>
                              </button>
                              <button
                                disabled={!canPromptForFile}
                                onClick={() =>
                                  onPromptFile({
                                    path: item.path,
                                    kind: item.kind,
                                    turnIndex: item.turnIndex,
                                    artifactKind: item.artifactKind,
                                    hunkSummary: item.context?.preview?.summary ?? null,
                                  })
                                }
                                title={agentLensAskActionTitle(item.path, canPromptForFile)}
                                type="button"
                              >
                                <span title={agentLensFileActionLabelTitle("Ask")}>Ask</span>
                              </button>
                              {item.turnCheckpointId && (
                                <button
                                  disabled={!canRevertTurnFile || revertingFile === key}
                                  onClick={() =>
                                    onRevertTurnFile(item.turnCheckpointId ?? "", item.path)
                                  }
                                  title={agentLensRevertActionTitle(
                                    item.path,
                                    item.turnIndex,
                                    canRevertTurnFile,
                                    revertingFile === key,
                                  )}
                                  type="button"
                                >
                                  <span
                                    title={agentLensFileActionLabelTitle(
                                      revertingFile === key ? "Reverting" : "Revert",
                                    )}
                                  >
                                    {revertingFile === key ? "Reverting" : "Revert"}
                                  </span>
                                </button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </section>
                  ))}
                </div>
              ) : (
                <div
                  className="agent-panel__empty-lens"
                  title={agentLensNoFilesMatchTitle(fileQuery)}
                >
                  <span title={agentLensNoFilesMatchLabelTitle(fileQuery)}>
                    No files match this filter.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearFileFilter}
                    title={agentLensNoFilesMatchClearTitle(fileQuery)}
                    type="button"
                  >
                    <span title={agentLensClearFileFilterLabelTitle()}>Clear</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div
              className="agent-panel__empty-lens"
              title={agentLensNoTouchedFilesTitle(activeScope)}
            >
              No touched files yet.
            </div>
          )}
        </div>
      )}

      {activeTab === "commands" && (
        <div
          className="agent-panel__lens-view"
          aria-label="Agent Lens Commands view"
          aria-labelledby={agentLensTabId(session.id, "commands")}
          id={agentLensPanelId(session.id, "commands")}
          role="tabpanel"
          title={agentLensViewContainerTitle(
            "commands",
            activeScope,
            commandItems.length,
            filteredCommandItems.length,
            hasCommandQuery,
          )}
        >
          {commandItems.length > 0 ? (
            <>
              <label
                className="agent-panel__lens-filter"
                title={agentLensEventFilterWrapperTitle(
                  "commands",
                  commandItems.length,
                  filteredCommandItems.length,
                  hasCommandQuery,
                )}
              >
                <span title={agentLensEventFilterLabelTitle("commands")}>Filter commands</span>
                <input
                  aria-describedby={agentLensEventFilterStatusId(session.id, "commands")}
                  aria-label="Filter command output"
                  ref={commandFilterRef}
                  value={commandQuery}
                  onChange={(event) => setCommandQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasCommandQuery) {
                      event.preventDefault();
                      clearCommandFilter();
                    }
                  }}
                  placeholder="Command text..."
                  title={agentLensEventFilterTitle(
                    "commands",
                    commandItems.length,
                    hasCommandQuery,
                  )}
                  type="search"
                />
                <small
                  aria-live="polite"
                  id={agentLensEventFilterStatusId(session.id, "commands")}
                  title={agentLensEventFilterCountTitle(
                    "commands",
                    filteredCommandItems.length,
                    commandItems.length,
                    hasCommandQuery,
                  )}
                >
                  {agentLensEventFilterCountText(
                    "commands",
                    filteredCommandItems.length,
                    commandItems.length,
                    hasCommandQuery,
                  )}
                </small>
                {hasCommandQuery && (
                  <button
                    className="agent-panel__lens-filter-clear"
                    onClick={clearCommandFilter}
                    title={agentLensClearEventFilterTitle(
                      "commands",
                      filteredCommandItems.length,
                      commandItems.length,
                    )}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("commands")}>Clear</span>
                  </button>
                )}
              </label>
              {filteredCommandItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Command output"
                  title={agentLensCommandListTitle(
                    activeScope,
                    filteredCommandItems.length,
                    hasCommandQuery,
                  )}
                >
                  {filteredCommandItems.map((item) => (
                    <div
                      className="agent-panel__lens-event"
                      key={item.id}
                      title={agentLensCommandEventTitle(item)}
                    >
                      <span title={agentLensCommandEventMetaTitle(item)}>
                        Turn {item.turnIndex} command
                        {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                      </span>
                      <pre title={agentLensCommandEventTextTitle(item)}>{item.text}</pre>
                    </div>
                  ))}
                </div>
              ) : (
                <div
                  className="agent-panel__empty-lens"
                  title={agentLensNoEventsMatchTitle("commands", commandQuery)}
                >
                  <span title={agentLensNoEventsMatchLabelTitle("commands", commandQuery)}>
                    No commands match this filter.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearCommandFilter}
                    title={agentLensNoEventsMatchClearTitle("commands", commandQuery)}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("commands")}>Clear</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="agent-panel__empty-lens" title={agentLensNoCommandsTitle(activeScope)}>
              No commands captured yet.
            </div>
          )}
        </div>
      )}

      {activeTab === "timeline" && (
        <div
          className="agent-panel__lens-view"
          aria-label="Agent Lens Timeline view"
          aria-labelledby={agentLensTabId(session.id, "timeline")}
          id={agentLensPanelId(session.id, "timeline")}
          role="tabpanel"
          title={agentLensViewContainerTitle(
            "timeline",
            activeScope,
            timelineItems.length,
            filteredTimelineItems.length,
            hasTimelineQuery,
          )}
        >
          {timelineItems.length > 0 ? (
            <>
              <label
                className="agent-panel__lens-filter"
                title={agentLensEventFilterWrapperTitle(
                  "timeline",
                  timelineItems.length,
                  filteredTimelineItems.length,
                  hasTimelineQuery,
                )}
              >
                <span title={agentLensEventFilterLabelTitle("timeline")}>Filter timeline</span>
                <input
                  aria-describedby={agentLensEventFilterStatusId(session.id, "timeline")}
                  aria-label="Filter timeline events"
                  ref={timelineFilterRef}
                  value={timelineQuery}
                  onChange={(event) => setTimelineQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasTimelineQuery) {
                      event.preventDefault();
                      clearTimelineFilter();
                    }
                  }}
                  placeholder="Event text or type..."
                  title={agentLensEventFilterTitle(
                    "timeline",
                    timelineItems.length,
                    hasTimelineQuery,
                  )}
                  type="search"
                />
                <small
                  aria-live="polite"
                  id={agentLensEventFilterStatusId(session.id, "timeline")}
                  title={agentLensEventFilterCountTitle(
                    "timeline",
                    filteredTimelineItems.length,
                    timelineItems.length,
                    hasTimelineQuery,
                  )}
                >
                  {agentLensEventFilterCountText(
                    "timeline",
                    filteredTimelineItems.length,
                    timelineItems.length,
                    hasTimelineQuery,
                  )}
                </small>
                {hasTimelineQuery && (
                  <button
                    className="agent-panel__lens-filter-clear"
                    onClick={clearTimelineFilter}
                    title={agentLensClearEventFilterTitle(
                      "timeline",
                      filteredTimelineItems.length,
                      timelineItems.length,
                    )}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("timeline")}>Clear</span>
                  </button>
                )}
              </label>
              {filteredTimelineItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Recent timeline"
                  title={agentLensTimelineListTitle(
                    activeScope,
                    filteredTimelineItems.length,
                    hasTimelineQuery,
                  )}
                >
                  {filteredTimelineItems.map((item) => (
                    <div
                      className="agent-panel__lens-event"
                      key={item.id}
                      title={agentLensTimelineEventTitle(item)}
                    >
                      <span title={agentLensTimelineEventMetaTitle(item)}>
                        Turn {item.turnIndex} - {item.label}
                        {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                      </span>
                      <pre title={agentLensTimelineEventTextTitle(item)}>{item.text}</pre>
                    </div>
                  ))}
                </div>
              ) : (
                <div
                  className="agent-panel__empty-lens"
                  title={agentLensNoEventsMatchTitle("timeline", timelineQuery)}
                >
                  <span title={agentLensNoEventsMatchLabelTitle("timeline", timelineQuery)}>
                    No timeline events match this filter.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearTimelineFilter}
                    title={agentLensNoEventsMatchClearTitle("timeline", timelineQuery)}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("timeline")}>Clear</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="agent-panel__empty-lens" title={agentLensNoTimelineTitle(activeScope)}>
              No timeline captured yet.
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function AgentLensTabButton({
  active,
  controlsId,
  count,
  id,
  label,
  onClick,
  onKeyDown,
}: {
  active: boolean;
  controlsId: string;
  count: number;
  id: string;
  label: string;
  onClick: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
}) {
  return (
    <button
      className={`agent-panel__lens-tab${active ? " agent-panel__lens-tab--active" : ""}`}
      aria-controls={controlsId}
      aria-selected={active}
      id={id}
      onClick={onClick}
      onKeyDown={onKeyDown}
      role="tab"
      tabIndex={active ? 0 : -1}
      title={agentLensTabTitle(label, count)}
      type="button"
    >
      <span title={agentLensTabLabelTitle(label)}>{label}</span>
      <small title={agentLensTabCountTitle(label, count)}>{count}</small>
    </button>
  );
}

function agentLensTabTitle(label: string, count: number): string {
  if (label === "Files") {
    const fileLabel = count === 1 ? "file" : "files";
    return `Show Agent Lens Files view with ${count} touched ${fileLabel}.`;
  }
  if (label === "Commands") {
    const commandLabel = count === 1 ? "command output" : "command outputs";
    return `Show Agent Lens Commands view with ${count} ${commandLabel}.`;
  }
  if (label === "Timeline") {
    const itemLabel = count === 1 ? "timeline item" : "timeline items";
    return `Show Agent Lens Timeline view with ${count} recent ${itemLabel}.`;
  }
  return `Show Agent Lens ${label} view.`;
}

function agentLensTabLabelTitle(label: string): string {
  return `Agent Lens tab name: ${label} view.`;
}

function agentLensTabCountTitle(label: string, count: number): string {
  if (label === "Files") {
    const fileLabel = count === 1 ? "file" : "files";
    return `Agent Lens Files tab count: ${count} touched ${fileLabel}.`;
  }
  if (label === "Commands") {
    const commandLabel = count === 1 ? "command output" : "command outputs";
    return `Agent Lens Commands tab count: ${count} ${commandLabel}.`;
  }
  if (label === "Timeline") {
    const itemLabel = count === 1 ? "timeline item" : "timeline items";
    return `Agent Lens Timeline tab count: ${count} ${itemLabel}.`;
  }
  return `Agent Lens ${label} tab count: ${count}.`;
}

function agentLensTabId(sessionId: string, tab: AgentLensTab): string {
  return `agent-lens-${domIdPart(sessionId)}-${tab}-tab`;
}

function agentLensPanelId(sessionId: string, tab: AgentLensTab): string {
  return `agent-lens-${domIdPart(sessionId)}-${tab}-panel`;
}

function domIdPart(value: string): string {
  return value.replace(/[^a-zA-Z0-9_-]/g, "-") || "session";
}

function agentLensRootTitle(
  activeScope: AgentLensScope,
  activeTab: AgentLensTab,
  focusedTurnIndex: number | null,
  turnCount: number,
  fileCount: number,
  commandCount: number,
  timelineCount: number,
  turnStatus: string,
): string {
  const fileLabel = fileCount === 1 ? "file" : "files";
  const commandLabel = commandCount === 1 ? "command output" : "command outputs";
  const timelineLabel = timelineCount === 1 ? "timeline item" : "timeline items";
  return `Agent Lens inspector: ${agentLensScopeSummary(
    activeScope,
    focusedTurnIndex,
    turnCount,
  )}; ${agentLensTabName(activeTab)} view active; ${fileCount} ${fileLabel}, ${commandCount} ${commandLabel}, ${timelineCount} ${timelineLabel}; turn state ${turnStatusLabel(turnStatus)}.`;
}

function agentLensViewContainerTitle(
  tab: AgentLensTab,
  activeScope: AgentLensScope,
  totalCount: number,
  visibleCount: number,
  hasQuery: boolean,
): string {
  const scope = activeScope === "focused" ? "focused turn" : "current session";
  if (tab === "files") {
    const totalLabel = totalCount === 1 ? "file" : "files";
    if (hasQuery) {
      const visibleLabel = visibleCount === 1 ? "file" : "files";
      return `Agent Lens Files view for the ${scope}: showing ${visibleCount} ${visibleLabel} from ${totalCount} touched ${totalLabel}.`;
    }
    return `Agent Lens Files view for the ${scope}: ${totalCount} touched ${totalLabel}.`;
  }
  if (tab === "commands") {
    const commandLabel = totalCount === 1 ? "command output" : "command outputs";
    if (hasQuery) {
      const visibleLabel = visibleCount === 1 ? "command output" : "command outputs";
      return `Agent Lens Commands view for the ${scope}: showing ${visibleCount} ${visibleLabel} from ${totalCount} ${commandLabel}.`;
    }
    return `Agent Lens Commands view for the ${scope}: ${totalCount} ${commandLabel}.`;
  }
  const itemLabel = totalCount === 1 ? "timeline item" : "timeline items";
  if (hasQuery) {
    const visibleLabel = visibleCount === 1 ? "timeline item" : "timeline items";
    return `Agent Lens Timeline view for the ${scope}: showing ${visibleCount} ${visibleLabel} from ${totalCount} ${itemLabel}.`;
  }
  return `Agent Lens Timeline view for the ${scope}: ${totalCount} ${itemLabel}.`;
}

function agentLensTabName(tab: AgentLensTab): string {
  switch (tab) {
    case "commands":
      return "Commands";
    case "timeline":
      return "Timeline";
    case "files":
    default:
      return "Files";
  }
}

function agentLensScopeSummary(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused") {
    return focusedTurnIndex
      ? `focused turn ${focusedTurnIndex}`
      : "focused scope waiting for a turn";
  }
  const turnLabel = turnCount === 1 ? "turn" : "turns";
  return `current session with ${turnCount} ${turnLabel}`;
}

function agentLensHeadingTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Agent Lens inspector for focused turn ${focusedTurnIndex}.`;
  }
  const turnLabel = turnCount === 1 ? "turn" : "turns";
  return `Agent Lens inspector for the full session with ${turnCount} ${turnLabel}.`;
}

function agentLensHeadingLabelTitle(): string {
  return "Agent Lens heading label.";
}

function agentLensTabListTitle(
  fileCount: number,
  commandCount: number,
  timelineCount: number,
): string {
  const fileLabel = fileCount === 1 ? "file" : "files";
  const commandLabel = commandCount === 1 ? "command output" : "command outputs";
  const timelineLabel = timelineCount === 1 ? "timeline item" : "timeline items";
  return `Agent Lens view tabs: ${fileCount} ${fileLabel}, ${commandCount} ${commandLabel}, ${timelineCount} ${timelineLabel}.`;
}

function agentLensScopeTitle(scope: AgentLensScope, focusedTurnIndex: number | null): string {
  if (scope === "focused") {
    return focusedTurnIndex
      ? `Scope Agent Lens to focused turn ${focusedTurnIndex}.`
      : "Focus a turn to scope Agent Lens to that turn.";
  }
  return "Scope Agent Lens to the full session.";
}

function agentLensScopeLabelTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Agent Lens is showing focused turn ${focusedTurnIndex}.`;
  }
  const turnLabel = turnCount === 1 ? "turn" : "turns";
  return `Agent Lens is showing the full session with ${turnCount} ${turnLabel}.`;
}

function agentLensScopeGroupTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Agent Lens scope controls are focused on turn ${focusedTurnIndex}.`;
  }
  return "Agent Lens scope controls switch between the focused turn and the full session.";
}

function agentLensMetricsTitle(
  activeScope: AgentLensScope,
  fileCount: number,
  turnCheckpointCount: number,
  restorableCount: number,
  turnStatus: string,
): string {
  const fileLabel = fileCount === 1 ? "file" : "files";
  const checkpointLabel = turnCheckpointCount === 1 ? "turn checkpoint" : "turn checkpoints";
  const scopeLabel = activeScope === "focused" ? "focused turn" : "current session";
  return `Agent Lens metrics summarize ${turnStatusLabel(turnStatus)} state, ${fileCount} ${fileLabel}, and ${restorableCount} restorable ${checkpointLabel} for the ${scopeLabel}.`;
}

function agentLensTurnStateMetricTitle(turnStatus: string): string {
  return `Current Agent Lens turn state: ${turnStatusLabel(turnStatus)}.`;
}

function agentLensTurnStateValueTitle(turnStatus: string): string {
  return `Agent Lens turn state value: ${turnStatusLabel(turnStatus)}.`;
}

function agentLensFileMetricTitle(activeScope: AgentLensScope, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  if (activeScope === "focused") {
    return `Agent Lens focused scope includes ${count} ${fileLabel}.`;
  }
  return `Agent Lens session scope includes ${count} ${fileLabel}.`;
}

function agentLensFileMetricValueTitle(activeScope: AgentLensScope, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  const scopeLabel = activeScope === "focused" ? "focused scope" : "session scope";
  return `Agent Lens ${scopeLabel} file count value: ${count} ${fileLabel}.`;
}

function agentLensRestoreMetricTitle(
  turnCheckpointCount: number,
  restorableCount: number,
  latestRestorableTurnIndex: number | null,
): string {
  if (turnCheckpointCount === 0) {
    return "Agent Lens restore-point metric: no completed turn checkpoints yet.";
  }
  const checkpointLabel = turnCheckpointCount === 1 ? "turn checkpoint" : "turn checkpoints";
  const restorableLabel = restorableCount === 1 ? "restore point" : "restore points";
  const latest = latestRestorableTurnIndex
    ? ` Latest restorable turn is ${latestRestorableTurnIndex}.`
    : "";
  return `Agent Lens restore-point metric: ${restorableCount} ${restorableLabel} from ${turnCheckpointCount} ${checkpointLabel}.${latest}`;
}

function agentLensRestoreMetricValueTitle(
  turnCheckpointCount: number,
  restorableCount: number,
  latestRestorableTurnIndex: number | null,
): string {
  if (turnCheckpointCount === 0) {
    return "Agent Lens restore-point value: 0 of 0 turn checkpoints are restorable.";
  }
  const latest = latestRestorableTurnIndex
    ? ` Latest restorable turn: ${latestRestorableTurnIndex}.`
    : "";
  return `Agent Lens restore-point value: ${restorableCount} of ${turnCheckpointCount} turn checkpoints are restorable.${latest}`;
}

function agentLensFileFilterStatusId(sessionId: string): string {
  return `agent-lens-${domIdPart(sessionId)}-file-filter-status`;
}

function agentLensFileFilterTitle(count: number, hasQuery: boolean): string {
  const fileLabel = count === 1 ? "file" : "files";
  const escapeHint = hasQuery ? " Press Escape to clear the filter." : "";
  return `Filter ${count} Agent Lens touched ${fileLabel} by path, change type, status, or artifact category.${escapeHint}`;
}

function agentLensFileFilterLabelTitle(): string {
  return "Agent Lens file filter label.";
}

function agentLensFileFilterWrapperTitle(
  totalCount: number,
  visibleCount: number,
  hasQuery: boolean,
): string {
  const totalLabel = totalCount === 1 ? "file" : "files";
  if (hasQuery) {
    const visibleLabel = visibleCount === 1 ? "file" : "files";
    return `Agent Lens file filter is showing ${visibleCount} ${visibleLabel} from ${totalCount} touched ${totalLabel}.`;
  }
  return `Agent Lens file filter controls ${totalCount} touched ${totalLabel}.`;
}

function agentLensFileFilterCountTitle(
  visibleCount: number,
  totalCount: number,
  hasQuery: boolean,
): string {
  const totalLabel = totalCount === 1 ? "file" : "files";
  if (hasQuery) {
    return `Showing ${visibleCount} of ${totalCount} Agent Lens touched ${totalLabel} after filtering.`;
  }
  return `Showing all ${totalCount} Agent Lens touched ${totalLabel}.`;
}

function agentLensClearFileFilterTitle(visibleCount: number, totalCount: number): string {
  const visibleLabel = visibleCount === 1 ? "file" : "files";
  const totalLabel = totalCount === 1 ? "file" : "files";
  return `Clear Agent Lens file filter and show all ${totalCount} touched ${totalLabel}; currently showing ${visibleCount} ${visibleLabel}.`;
}

function agentLensClearFileFilterLabelTitle(): string {
  return "Agent Lens clear-file-filter label: Clear.";
}

function agentLensPreviewLabelTitle(): string {
  return "Selected-file preview area for the active Agent Lens file.";
}

function agentLensPreviewContainerTitle(
  path: string | null,
  position: number,
  count: number,
): string {
  const keyboardHint = count > 1 ? " Use arrow keys to move between previewed files." : "";
  return path
    ? `Selected Agent Lens file preview for ${path}; item ${position} of ${count}.${keyboardHint}`
    : "Agent Lens selected-file preview placeholder is waiting for a touched file.";
}

function agentLensPreviewSelectionTitle(path: string | null): string {
  return path
    ? `Agent Lens preview is showing ${path}.`
    : "Agent Lens selected-file preview placeholder: no file selected.";
}

function agentLensPreviewPositionTitle(position: number, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return `Agent Lens preview position: ${position} of ${count} visible ${fileLabel}.`;
}

function agentLensPreviewNavigationTitle(path: string | null, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return path
    ? `Agent Lens preview navigation for ${path}: move through ${count} visible ${fileLabel}.`
    : `Agent Lens preview navigation: move through ${count} visible ${fileLabel}.`;
}

function agentLensPreviewNavButtonTitle(
  direction: "previous" | "next",
  path: string | null,
): string {
  const label = direction === "previous" ? "previous" : "next";
  return path
    ? `Show the ${label} Agent Lens preview file: ${path}.`
    : `Show the ${label} Agent Lens preview file.`;
}

function agentLensPreviewNavLabelTitle(label: string): string {
  return `Agent Lens preview navigation label: ${label}.`;
}

function agentLensPreviewActionsTitle(path: string, canShowRevert: boolean): string {
  const controls = canShowRevert
    ? "open, ask, and revert controls for the selected preview file"
    : "open and ask controls for the selected preview file";
  return `Agent Lens preview actions for ${path}: ${controls}.`;
}

function agentLensPreviewActionLabelTitle(label: string): string {
  return `Agent Lens preview action label: ${label}.`;
}

function agentLensPreviewRevertActionTitle(
  path: string,
  turnIndex: number | null,
  canRevert: boolean,
  isReverting: boolean,
): string {
  return `Selected preview file: ${agentLensRevertActionTitle(
    path,
    turnIndex,
    canRevert,
    isReverting,
  )}`;
}

function agentLensPreviewDetailGroupTitle(path: string): string {
  return `Agent Lens preview details for ${path}: hunk summary and first-hunk location.`;
}

function agentLensNoLiveHunkTitle(path: string | null): string {
  return path
    ? `Selected-file preview placeholder for ${path}: no live hunk data available.`
    : "Selected-file preview placeholder: no live hunk data available because no file is selected.";
}

function agentLensNoFilesMatchTitle(query: string): string {
  return `No Agent Lens files match "${query.trim()}". Clear or change the filter to show touched files.`;
}

function agentLensNoFilesMatchLabelTitle(query: string): string {
  return `Agent Lens file filter empty result for "${query.trim()}".`;
}

function agentLensNoFilesMatchClearTitle(query: string): string {
  return `Clear Agent Lens file filter "${query.trim()}" and restore touched files.`;
}

function agentLensNoTouchedFilesTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens has no touched files in the focused turn.";
  }
  return "Agent Lens has no touched files in the current session.";
}

function agentLensTouchedFilesListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const fileLabel = count === 1 ? "file" : "files";
  const scope = activeScope === "focused" ? "focused turn" : "current session";
  const filterPrefix = hasQuery ? "Filtered " : "";
  return `${filterPrefix}Agent Lens touched files for the ${scope}: ${count} ${fileLabel}.`;
}

function agentLensCommandListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const commandLabel = count === 1 ? "command output" : "command outputs";
  const scope = activeScope === "focused" ? "focused turn" : "current session";
  const filterPrefix = hasQuery ? "Filtered " : "";
  return `${filterPrefix}Agent Lens command output for the ${scope}: ${count} ${commandLabel}.`;
}

function agentLensTimelineListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const itemLabel = count === 1 ? "timeline item" : "timeline items";
  const scope = activeScope === "focused" ? "focused turn" : "current session";
  const filterPrefix = hasQuery ? "Filtered " : "";
  return `${filterPrefix}Agent Lens recent timeline for the ${scope}: ${count} ${itemLabel}.`;
}

function agentLensEventFilterStatusId(sessionId: string, kind: "commands" | "timeline"): string {
  return `agent-lens-${domIdPart(sessionId)}-${kind}-filter-status`;
}

function agentLensEventFilterTitle(
  kind: "commands" | "timeline",
  count: number,
  hasQuery: boolean,
): string {
  const noun = kind === "commands" ? "command output" : "timeline events";
  const escapeHint = hasQuery ? " Press Escape to clear the filter." : "";
  return `Filter ${count} Agent Lens ${noun} by text${kind === "timeline" ? " or event type" : ""}.${escapeHint}`;
}

function agentLensEventFilterLabelTitle(kind: "commands" | "timeline"): string {
  return kind === "commands"
    ? "Agent Lens command filter label."
    : "Agent Lens timeline filter label.";
}

function agentLensEventFilterWrapperTitle(
  kind: "commands" | "timeline",
  totalCount: number,
  visibleCount: number,
  hasQuery: boolean,
): string {
  const noun =
    kind === "commands"
      ? totalCount === 1
        ? "command output"
        : "command outputs"
      : totalCount === 1
        ? "timeline event"
        : "timeline events";
  if (hasQuery) {
    return `Agent Lens ${kind} filter is showing ${visibleCount} from ${totalCount} ${noun}.`;
  }
  return `Agent Lens ${kind} filter controls ${totalCount} ${noun}.`;
}

function agentLensEventFilterCountTitle(
  kind: "commands" | "timeline",
  visibleCount: number,
  totalCount: number,
  hasQuery: boolean,
): string {
  const noun =
    kind === "commands"
      ? totalCount === 1
        ? "command output"
        : "command outputs"
      : totalCount === 1
        ? "timeline event"
        : "timeline events";
  if (hasQuery) {
    return `Showing ${visibleCount} of ${totalCount} Agent Lens ${noun} after filtering.`;
  }
  return `Showing all ${totalCount} Agent Lens ${noun}.`;
}

function agentLensEventFilterCountText(
  kind: "commands" | "timeline",
  visibleCount: number,
  totalCount: number,
  hasQuery: boolean,
): string {
  const noun =
    kind === "commands"
      ? totalCount === 1
        ? "command"
        : "commands"
      : totalCount === 1
        ? "event"
        : "events";
  return hasQuery ? `${visibleCount} of ${totalCount} ${noun}` : `${totalCount} ${noun}`;
}

function agentLensClearEventFilterTitle(
  kind: "commands" | "timeline",
  visibleCount: number,
  totalCount: number,
): string {
  const noun =
    kind === "commands"
      ? totalCount === 1
        ? "command output"
        : "command outputs"
      : totalCount === 1
        ? "timeline event"
        : "timeline events";
  return `Clear Agent Lens ${kind} filter and show all ${totalCount} ${noun}; currently showing ${visibleCount}.`;
}

function agentLensClearEventFilterLabelTitle(kind: "commands" | "timeline"): string {
  return kind === "commands"
    ? "Agent Lens clear-command-filter label: Clear."
    : "Agent Lens clear-timeline-filter label: Clear.";
}

function agentLensNoEventsMatchTitle(kind: "commands" | "timeline", query: string): string {
  const noun = kind === "commands" ? "commands" : "timeline events";
  return `No Agent Lens ${noun} match "${query.trim()}". Clear or change the filter to show captured items.`;
}

function agentLensNoEventsMatchLabelTitle(kind: "commands" | "timeline", query: string): string {
  const noun = kind === "commands" ? "command output" : "timeline event";
  return `Agent Lens ${noun} filter empty result for "${query.trim()}".`;
}

function agentLensNoEventsMatchClearTitle(kind: "commands" | "timeline", query: string): string {
  return `Clear Agent Lens ${kind} filter "${query.trim()}" and restore captured items.`;
}

function agentLensLiveContextTitle(path: string): string {
  return `Live Agent Lens context for ${path}: repo status and diff chips.`;
}

function agentLensCommandEventTitle(item: {
  turnIndex: number;
  timeLabel: string | null;
  text: string;
}): string {
  const timing = item.timeLabel ? ` at ${item.timeLabel}` : "";
  return `Command output captured in Agent Lens for turn ${item.turnIndex}${timing}: ${item.text}`;
}

function agentLensCommandEventMetaTitle(item: {
  turnIndex: number;
  timeLabel: string | null;
}): string {
  const timing = item.timeLabel ? ` at ${item.timeLabel}` : "";
  return `Agent Lens command event metadata: turn ${item.turnIndex} command${timing}.`;
}

function agentLensCommandEventTextTitle(item: { turnIndex: number; text: string }): string {
  return `Captured Agent Lens command output for turn ${item.turnIndex}: ${item.text}`;
}

function agentLensTimelineEventTitle(item: {
  turnIndex: number;
  label: string;
  timeLabel: string | null;
  text: string;
}): string {
  const timing = item.timeLabel ? ` at ${item.timeLabel}` : "";
  return `Timeline ${item.label.toLowerCase()} event captured in Agent Lens for turn ${item.turnIndex}${timing}: ${item.text}`;
}

function agentLensTimelineEventMetaTitle(item: {
  turnIndex: number;
  label: string;
  timeLabel: string | null;
}): string {
  const timing = item.timeLabel ? ` at ${item.timeLabel}` : "";
  return `Agent Lens timeline event metadata: turn ${item.turnIndex} ${item.label} event${timing}.`;
}

function agentLensTimelineEventTextTitle(item: {
  turnIndex: number;
  label: string;
  text: string;
}): string {
  return `Captured Agent Lens timeline text for turn ${item.turnIndex} ${item.label} event: ${item.text}`;
}

function agentLensNoCommandsTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens has no command output in the focused turn.";
  }
  return "Agent Lens has no command output in the current session.";
}

function agentLensNoTimelineTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens has no timeline events in the focused turn.";
  }
  return "Agent Lens has no timeline events in the current session.";
}

function agentLensFileItems(
  turnCheckpoints: NonNullable<AgentSession["turn_checkpoints"]>,
  changeLog: NonNullable<AgentSession["change_log"]>,
  focusedTurnIndex: number | null,
) {
  const firstCheckpointAtMs = turnCheckpoints[0]?.started_at_ms ?? null;
  const checkpointItems = turnCheckpoints
    .slice()
    .reverse()
    .filter((turn) => focusedTurnIndex == null || turn.index === focusedTurnIndex)
    .flatMap((turn) =>
      turn.changes.map((change) => ({
        id: `${turn.id}:${change.kind}:${change.path}`,
        turnCheckpointId: turn.id,
        turnIndex: turn.index,
        timeLabel: timeOffsetLabel(turn.started_at_ms, firstCheckpointAtMs),
        path: change.path,
        kind: change.kind,
      })),
    );
  if (focusedTurnIndex != null && turnCheckpoints.length > 0) return checkpointItems;
  const checkpointPaths = new Set(checkpointItems.map((item) => item.path));
  const firstChangeAtMs = changeLog[0]?.timestamp_ms ?? null;
  const sessionItems = changeLog
    .filter((change) => !checkpointPaths.has(change.path))
    .slice()
    .reverse()
    .map((change) => ({
      id: `session:${change.kind}:${change.path}:${change.timestamp_ms}`,
      turnCheckpointId: null,
      turnIndex: null,
      timeLabel: timeOffsetLabel(change.timestamp_ms, firstChangeAtMs),
      path: change.path,
      kind: change.kind,
    }));
  return [...checkpointItems, ...sessionItems];
}

const AGENT_LENS_ARTIFACT_ORDER = ["Code", "Tests", "Docs", "Config", "Other"] as const;

type AgentLensArtifactKind = (typeof AGENT_LENS_ARTIFACT_ORDER)[number];

interface AgentLensFileContext {
  statusChips: Array<{ label: string; title: string }>;
  diffLabel: string;
  diffTitle: string;
  renameLabel: string | null;
  renameTitle: string | undefined;
  preview: AgentLensFilePreview | null;
  searchText: string;
}

interface AgentLensFilePromptContext {
  path: string;
  kind: string;
  turnIndex: number | null;
  artifactKind: AgentLensArtifactKind;
  hunkSummary: string | null;
}

interface AgentLensFilePreview {
  summary: string;
  summaryTitle: string;
  detail: string;
  detailTitle: string;
}

function groupAgentLensFileItems<T extends { artifactKind: AgentLensArtifactKind }>(
  items: T[],
): Array<{ kind: AgentLensArtifactKind; items: T[] }> {
  return AGENT_LENS_ARTIFACT_ORDER.map((kind) => ({
    kind,
    items: items.filter((item) => item.artifactKind === kind),
  })).filter((group) => group.items.length > 0);
}

function agentLensArtifactKind(path: string): AgentLensArtifactKind {
  const normalized = normalizeAgentPath(path);
  const basename = normalized.split("/").pop() ?? normalized;
  if (
    normalized.includes("/test/") ||
    normalized.includes("/tests/") ||
    normalized.includes("__tests__/") ||
    /\.(test|spec)\.[^.]+$/.test(basename)
  ) {
    return "Tests";
  }
  if (
    normalized.startsWith("docs/") ||
    normalized.endsWith(".md") ||
    normalized.endsWith(".mdx") ||
    normalized.endsWith(".rst")
  ) {
    return "Docs";
  }
  if (
    normalized.includes("/config/") ||
    normalized.endsWith(".json") ||
    normalized.endsWith(".toml") ||
    normalized.endsWith(".yaml") ||
    normalized.endsWith(".yml") ||
    normalized.endsWith(".env") ||
    basename.startsWith(".")
  ) {
    return "Config";
  }
  if (
    /\.(ts|tsx|js|jsx|rs|py|go|java|kt|swift|cs|c|cc|cpp|h|hpp|css|scss|html|svelte|vue)$/.test(
      basename,
    )
  ) {
    return "Code";
  }
  return "Other";
}

function agentLensFileContext(
  path: string,
  status: RepoStatus | undefined,
  diffs: Record<string, FileDiff> | undefined,
): AgentLensFileContext | null {
  const statusChips = fileStatusChips(path, status);
  const diff = findAgentLensDiff(path, diffs);
  if (statusChips.length === 0 && !diff) return null;
  const diffSummary = diff
    ? fileDiffSummary(path, diff)
    : { label: "No live diff", title: `No live diff is currently available for ${path}.` };
  const renameLabel = diff?.old_path ? `from ${diff.old_path}` : null;
  return {
    statusChips,
    diffLabel: diffSummary.label,
    diffTitle: diffSummary.title,
    renameLabel,
    renameTitle: diff?.old_path
      ? `Live diff rename source for ${path}: ${diff.old_path}.`
      : undefined,
    preview: diff ? fileDiffPreview(diff) : null,
    searchText: [
      ...statusChips.map((chip) => chip.label),
      diffSummary.label,
      renameLabel ?? "",
    ].join(" "),
  };
}

function fileStatusChips(
  path: string,
  status: RepoStatus | undefined,
): Array<{ label: string; title: string }> {
  if (!status) return [];
  const chips: Array<{ label: string; title: string }> = [];
  if (pathListIncludes(status.staged, path)) chips.push(fileStatusChip(path, "staged"));
  if (pathListIncludes(status.modified, path)) chips.push(fileStatusChip(path, "modified"));
  if (pathListIncludes(status.untracked, path)) chips.push(fileStatusChip(path, "untracked"));
  return chips;
}

function fileStatusChip(path: string, label: string): { label: string; title: string } {
  return { label, title: `Live repo status for ${path}: ${label}.` };
}

function pathListIncludes(paths: string[], path: string): boolean {
  const target = normalizeAgentPath(path);
  return paths.some((candidate) => normalizeAgentPath(candidate) === target);
}

function findAgentLensDiff(
  path: string,
  diffs: Record<string, FileDiff> | undefined,
): FileDiff | undefined {
  if (!diffs) return undefined;
  const direct = diffs[path];
  if (direct) return direct;
  const target = normalizeAgentPath(path);
  return Object.values(diffs).find(
    (diff) =>
      normalizeAgentPath(diff.path) === target ||
      (diff.old_path ? normalizeAgentPath(diff.old_path) === target : false),
  );
}

function fileDiffSummary(path: string, diff: FileDiff): { label: string; title: string } {
  if (diff.is_binary) {
    return { label: "Binary diff", title: `Live diff summary for ${path}: binary file diff.` };
  }
  const totals = diff.hunks.reduce(
    (acc, hunk) => {
      for (const line of hunk.lines) {
        if (line.kind === "Added") acc.added += 1;
        if (line.kind === "Removed") acc.removed += 1;
      }
      return acc;
    },
    { added: 0, removed: 0 },
  );
  return {
    label: `+${totals.added} / -${totals.removed}`,
    title: `Live diff summary for ${path}: ${totals.added} added, ${totals.removed} removed.`,
  };
}

function fileDiffPreview(diff: FileDiff): AgentLensFilePreview {
  if (diff.is_binary) {
    return {
      summary: "Binary diff",
      summaryTitle: `Selected-file preview summary for ${diff.path}: binary diff.`,
      detail: "Binary file; hunk preview is unavailable.",
      detailTitle: `Selected-file preview detail for ${diff.path}: binary file; hunk preview is unavailable.`,
    };
  }
  const totals = diff.hunks.reduce(
    (acc, hunk) => {
      for (const line of hunk.lines) {
        if (line.kind === "Added") acc.added += 1;
        if (line.kind === "Removed") acc.removed += 1;
      }
      return acc;
    },
    { added: 0, removed: 0 },
  );
  const firstHunk = diff.hunks[0];
  const hunkLabel = diff.hunks.length === 1 ? "1 hunk" : `${diff.hunks.length} hunks`;
  const rangeLabel = firstHunk
    ? `First hunk @@ -${firstHunk.old_start} +${firstHunk.new_start}`
    : "No hunks";
  return {
    summary: `${hunkLabel} - +${totals.added} / -${totals.removed}`,
    summaryTitle: `Selected-file preview summary for ${diff.path}: ${hunkLabel}, ${totals.added} added, ${totals.removed} removed.`,
    detail: diff.old_path ? `${rangeLabel}; renamed from ${diff.old_path}.` : `${rangeLabel}.`,
    detailTitle: diff.old_path
      ? `Selected-file preview detail for ${diff.path}: ${rangeLabel}; renamed from ${diff.old_path}.`
      : `Selected-file preview detail for ${diff.path}: ${rangeLabel}.`,
  };
}

function normalizeAgentPath(path: string): string {
  return path.replace(/\\/g, "/").toLocaleLowerCase();
}

function filterAgentLensFileItems<
  T extends {
    path: string;
    kind: string;
    artifactKind?: AgentLensArtifactKind;
    context?: AgentLensFileContext | null;
  },
>(items: T[], query: string): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return items;
  return items.filter((item) =>
    `${item.path}\n${item.kind}\n${item.artifactKind ?? ""}\n${item.context?.searchText ?? ""}`
      .toLocaleLowerCase()
      .includes(normalized),
  );
}

function filterAgentLensCommandItems<
  T extends { turnIndex: number; timeLabel: string | null; text: string },
>(items: T[], query: string): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return items;
  return items.filter((item) =>
    [`turn ${item.turnIndex}`, item.timeLabel ?? "", item.text]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized),
  );
}

function filterAgentLensTimelineItems<
  T extends { turnIndex: number; label: string; timeLabel: string | null; text: string },
>(items: T[], query: string): T[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return items;
  return items.filter((item) =>
    [`turn ${item.turnIndex}`, item.label, item.timeLabel ?? "", item.text]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized),
  );
}

function agentLensCommandItems(turns: AgentTurnView[], focusedTurnIndex: number | null) {
  return turns
    .filter((turn) => focusedTurnIndex == null || turn.index === focusedTurnIndex)
    .flatMap((turn) =>
      turn.commandText.map((text, index) => ({
        id: `${turn.id}:command:${index}`,
        turnIndex: turn.index,
        timeLabel: turnTimeLabel(turn, turns[0]?.startedAtMs ?? null),
        text: compactActivityText(text),
      })),
    )
    .reverse();
}

function agentLensTimelineItems(turns: AgentTurnView[], focusedTurnIndex: number | null) {
  return turns
    .filter((turn) => focusedTurnIndex == null || turn.index === focusedTurnIndex)
    .flatMap((turn) => {
      const items: Array<{
        id: string;
        turnIndex: number;
        label: string;
        text: string;
        timeLabel: string | null;
      }> = [];
      const timeLabel = turnTimeLabel(turn, turns[0]?.startedAtMs ?? null);
      if (turn.userText) {
        items.push({
          id: `${turn.id}:user`,
          turnIndex: turn.index,
          label: "You",
          timeLabel,
          text: compactActivityText(turn.userText),
        });
      }
      turn.agentText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:agent:${index}`,
          turnIndex: turn.index,
          label: "Agent",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      turn.commandText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:command:${index}`,
          turnIndex: turn.index,
          label: "Command",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      turn.systemText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:system:${index}`,
          turnIndex: turn.index,
          label: "System",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      return items;
    })
    .reverse()
    .slice(0, 12);
}

function agentSessionOverview(turns: AgentTurnView[]): AgentSessionOverviewView {
  const files = new Set<string>();
  let messages = 0;
  let commands = 0;
  let latest: string | null = null;

  for (const turn of turns) {
    messages += (turn.userText ? 1 : 0) + turn.agentText.length + turn.systemText.length;
    commands += turn.commandText.length;
    for (const change of turn.changes) files.add(change.path);
    latest = latestActivityText(turn) ?? latest;
  }

  const activeTurnId = turns[turns.length - 1]?.id ?? null;
  const firstTurnAtMs = turns[0]?.startedAtMs ?? null;
  return {
    turns: turns.length,
    messages,
    commands,
    files: files.size,
    latest: latest ? compactActivityText(latest) : null,
    turnMap: turns.map((turn) => ({
      id: turn.id,
      index: turn.index,
      commands: turn.commandText.length,
      commandSummary: turnCommandSummaryText(turn),
      files: turn.changes.length,
      active: turn.id === activeTurnId,
      timeLabel: turnTimeLabel(turn, firstTurnAtMs),
    })),
  };
}

function turnMapTitle(turn: AgentSessionOverviewView["turnMap"][number]): string {
  const parts = [`Turn ${turn.index}: ${turn.commands} commands, ${turn.files} files`];
  if (turn.commandSummary) parts.push(`Recent command: ${turn.commandSummary}`);
  return parts.join(" - ");
}

function turnMapIndexTitle(turnIndex: number): string {
  return `Agent session overview turn-map label: turn ${turnIndex}.`;
}

function turnMapTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Agent session overview turn-map timing for turn ${turnIndex}: ${timeLabel}.`;
}

function turnMapCommandCountTitle(turnIndex: number, commands: number): string {
  return `Agent session overview turn-map command count for turn ${turnIndex}: ${overviewMetricCount(
    "Commands",
    commands,
  )}.`;
}

function turnMapCommandSummaryTitle(turnIndex: number, commandSummary: string): string {
  return `Agent session overview turn-map command summary for turn ${turnIndex}: ${commandSummary}.`;
}

function turnMapFileCountTitle(turnIndex: number, files: number): string {
  return `Agent session overview turn-map file count for turn ${turnIndex}: ${overviewMetricCount(
    "Files",
    files,
  )}.`;
}

function agentActivitySummary(
  session: AgentSession | undefined,
  readOnly: boolean,
): {
  title: string;
  detail: string;
  checkpoint: string;
  throughput: string;
  tone: "idle" | "working" | "done" | "failed";
} {
  if (!session) {
    return {
      title: "Loading agent session",
      detail: "Connecting the workspace view to the agent runtime.",
      checkpoint: "Checkpoint unknown",
      throughput: "No stream yet",
      tone: "idle",
    };
  }
  const turnState = turnStatusLabel(session.turn_status ?? "waiting");
  const status = sessionStatusLabel(session.status);
  const changeCount = session.change_log?.length ?? 0;
  const checkpoint = session.checkpoint
    ? checkpointLabel(session.checkpoint.checkpoint_type)
    : "No checkpoint";
  const throughput =
    session.output_bytes_per_second != null
      ? `${Math.round(session.output_bytes_per_second)} B/s`
      : "Stream quiet";
  if (readOnly) {
    return {
      title: "Archived transcript",
      detail: `${changeCount} recorded changes. Session is read-only.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.status === "failed" || session.status === "error") {
    return {
      title: "Needs attention",
      detail: session.error
        ? commandMessage(session.error)
        : `${status}. Review the transcript and recent command output.`,
      checkpoint,
      throughput,
      tone: "failed",
    };
  }
  if (session.status === "completed" || session.status === "reverted") {
    return {
      title: session.status === "reverted" ? "Changes reverted" : "Session complete",
      detail: `${changeCount} changes tracked. ${turnState} turn state.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.turn_status === "working" || session.status === "running") {
    return {
      title: "Agent is working",
      detail: `${turnState}. ${changeCount} changes tracked so far.`,
      checkpoint,
      throughput,
      tone: "working",
    };
  }
  return {
    title: "Ready for the next turn",
    detail: `${turnState}. ${changeCount} changes tracked so far.`,
    checkpoint,
    throughput,
    tone: "idle",
  };
}

function agentActivityFactsTitle(): string {
  return "Agent activity facts: turns, files, checkpoint, and stream throughput.";
}

function agentActivityStripTitle(
  activity: ReturnType<typeof agentActivitySummary>,
  overview: AgentSessionOverviewView,
): string {
  return `Agent activity strip: ${activity.title}; ${activity.detail}; ${overviewMetricCount(
    "Turns",
    overview.turns,
  )}, ${overviewMetricCount("Files", overview.files)}.`;
}

function agentActivityMainTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Agent activity main status: ${activity.title}.`;
}

function agentActivityDotTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Agent activity pulse: ${activity.tone} state.`;
}

function agentActivityTextGroupTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Agent activity status text: ${activity.title}; ${activity.detail}`;
}

function agentActivityTitleLabelTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Agent activity headline: ${activity.title}.`;
}

function agentActivityDetailTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Agent activity detail: ${activity.detail}`;
}

function agentActivityTurnsFactTitle(turns: number): string {
  return `Agent activity turn count: ${turns} ${turns === 1 ? "turn" : "turns"}.`;
}

function agentActivityFilesFactTitle(files: number): string {
  return `Agent activity touched-file count: ${files} ${files === 1 ? "file" : "files"}.`;
}

function agentActivityCheckpointFactTitle(checkpoint: string): string {
  return `Agent activity checkpoint fact: ${checkpoint}.`;
}

function agentActivityThroughputFactTitle(throughput: string): string {
  return `Agent activity stream throughput fact: ${throughput}.`;
}

function latestActivityText(turn: AgentTurnView): string | null {
  const candidates = [
    turn.userText ?? "",
    ...turn.agentText,
    ...turn.commandText,
    ...turn.systemText,
  ].filter(Boolean);
  return candidates[candidates.length - 1] ?? null;
}

function turnTimeLabel(turn: AgentTurnView, firstTurnAtMs: number | null): string | null {
  return timeOffsetLabel(turn.startedAtMs, firstTurnAtMs);
}

function timeOffsetLabel(
  timestampMs: number | null | undefined,
  baseMs: number | null | undefined,
): string | null {
  if (timestampMs == null || baseMs == null) return null;
  const deltaSeconds = Math.max(0, Math.round((timestampMs - baseMs) / 1000));
  if (deltaSeconds < 60) return `+${deltaSeconds}s`;
  const minutes = Math.floor(deltaSeconds / 60);
  const seconds = deltaSeconds % 60;
  if (minutes < 60) return seconds === 0 ? `+${minutes}m` : `+${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes === 0 ? `+${hours}h` : `+${hours}h ${remainingMinutes}m`;
}

function compactActivityText(text: string): string {
  const oneLine = text.replace(/\s+/g, " ").trim();
  return oneLine.length > 140 ? `${oneLine.slice(0, 137)}...` : oneLine;
}

function agentTurnElementId(sessionId: string, turnIndex: number): string {
  return `agent-turn-${sessionId}-${turnIndex}`;
}

function scrollToAgentTurn(sessionId: string, turnIndex: number, block: ScrollLogicalPosition) {
  document
    .getElementById(agentTurnElementId(sessionId, turnIndex))
    ?.scrollIntoView({ block, behavior: "smooth" });
}

async function writeClipboardText(text: string): Promise<void> {
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard is not available in this window.");
  }
  await navigator.clipboard.writeText(text);
}

function transcriptText(turns: AgentTurnView[]): string {
  const firstTurnAtMs = turns[0]?.startedAtMs ?? null;
  return turns.map((turn) => turnTranscriptText(turn, firstTurnAtMs)).join("\n\n");
}

function turnTranscriptText(turn: AgentTurnView, firstTurnAtMs: number | null): string {
  const timeLabel = turnTimeLabel(turn, firstTurnAtMs);
  const artifactSummary = turnArtifactSummaryText(turn.changes);
  const parts: string[] = [`Turn ${turn.index}${timeLabel ? ` (${timeLabel})` : ""}`];
  if (artifactSummary) parts.push(`Artifacts: ${artifactSummary}.`);
  if (turn.userText) parts.push(`You:\n${turn.userText}`);
  turn.agentText.forEach((text) => parts.push(`Agent:\n${text}`));
  turn.commandText.forEach((text) => parts.push(`Command:\n${text}`));
  turn.systemText.forEach((text) => parts.push(`System:\n${text}`));
  if (turn.changes.length > 0) {
    parts.push(
      `Files:\n${turn.changes.map((change) => `- ${change.kind} ${change.path}`).join("\n")}`,
    );
  }
  return parts.join("\n\n");
}

function focusedTurnHeadingLabelTitle(): string {
  return "Focused turn heading label: Focused turn.";
}

function focusedTurnIdleContainerTitle(): string {
  return "Focused turn card container: no turn selected.";
}

function focusedTurnSelectedContainerTitle(turnIndex: number): string {
  return `Focused turn card container: selected turn ${turnIndex}.`;
}

function focusedTurnIndexLabelTitle(turnIndex: number): string {
  return `Focused turn index label: Turn ${turnIndex}.`;
}

function focusedTurnIdleStatusLabelTitle(): string {
  return "Focused turn idle status label: Idle.";
}

function focusedTurnEmptyStateLabelTitle(): string {
  return "Focused turn empty-state label: No turn selected.";
}

function focusedTurnIdleHelperTitle(): string {
  return "Focused turn idle helper text: the next agent response will appear as a navigable turn.";
}

function focusedTurnFallbackTextTitle(turnIndex: number): string {
  return `Focused turn selected fallback text: No text captured for turn ${turnIndex}.`;
}

function focusedTurnHiddenFileOverflowTitle(turnIndex: number, count: number): string {
  const label = count === 1 ? "additional touched file" : "additional touched files";
  return `Focused turn hidden file overflow: ${count} ${label} for turn ${turnIndex}.`;
}

function focusedTurnFileRowTitle(
  turnIndex: number,
  change: { kind: string; path: string },
): string {
  return `Focused turn touched file row for turn ${turnIndex}: ${change.kind} ${change.path}.`;
}

function focusedTurnFilesContainerTitle(
  turnIndex: number,
  visibleCount: number,
  hiddenCount: number,
): string {
  const visibleLabel = visibleCount === 1 ? "visible touched file" : "visible touched files";
  const hiddenLabel = hiddenCount === 1 ? "hidden touched file" : "hidden touched files";
  const hiddenText = hiddenCount > 0 ? `, plus ${hiddenCount} ${hiddenLabel}` : "";
  return `Focused turn files container for turn ${turnIndex}: ${visibleCount} ${visibleLabel}${hiddenText}.`;
}

function focusedTurnRestoreContainerTitle(
  turnIndex: number,
  canRestore: boolean,
  restoreReady: boolean,
): string {
  if (!canRestore) {
    return `Focused turn restore container for turn ${turnIndex}: stop the session before restoring.`;
  }
  return restoreReady
    ? `Focused turn restore container for turn ${turnIndex}: restore files and chat view to this turn.`
    : `Focused turn restore container for turn ${turnIndex}: no completed restore checkpoint is available.`;
}

function focusedTurnRestoreButtonTitle(
  turnIndex: number,
  canRestore: boolean,
  restoreReady: boolean,
  restoring: boolean,
): string {
  if (restoring) return `Restoring files and chat view to turn ${turnIndex}.`;
  if (!canRestore) return `Restore turn ${turnIndex}: stop the session before restoring.`;
  if (!restoreReady) {
    return `Restore turn ${turnIndex}: unavailable because this turn has no completed checkpoint.`;
  }
  return `Restore turn ${turnIndex}: return files and chat view to this turn.`;
}

function focusedTurnRestoreLabelTitle(restoring: boolean): string {
  return restoring
    ? "Focused turn restore label: Restoring."
    : "Focused turn restore label: Restore here.";
}

function focusedTurnTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Focused turn ${turnIndex} timing relative to the first turn: ${timeLabel}.`;
}

function turnTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Turn ${turnIndex} timing relative to the first turn: ${timeLabel}.`;
}

function focusedTurnFactTitle(
  turnIndex: number,
  kind: "commands" | "files",
  count: number,
): string {
  const label = count === 1 ? kind.slice(0, -1) : kind;
  return `Focused turn ${turnIndex} has ${count} ${label}.`;
}

function focusedTurnFactsContainerTitle(
  turnIndex: number,
  commandCount: number,
  fileCount: number,
): string {
  const commandLabel = commandCount === 1 ? "command" : "commands";
  const fileLabel = fileCount === 1 ? "file" : "files";
  return `Focused turn facts container for turn ${turnIndex}: ${commandCount} ${commandLabel}, ${fileCount} ${fileLabel}.`;
}

function focusedTurnCommandSummaryContainerTitle(turnIndex: number): string {
  return `Focused turn command-summary container for turn ${turnIndex}: 1 recent command summary.`;
}

function focusedTurnArtifactSummaryContainerTitle(
  turnIndex: number,
  categoryCount: number,
): string {
  const categoryLabel = categoryCount === 1 ? "artifact category chip" : "artifact category chips";
  return `Focused turn artifact-summary container for turn ${turnIndex}: ${categoryCount} ${categoryLabel}.`;
}

function focusedTurnLatestActivityTitle(turnIndex: number, latest: string): string {
  return `Most recent captured activity for focused turn ${turnIndex}: ${compactActivityText(latest)}`;
}

function focusedTurnSummaryTitle(turn: AgentTurnView): string {
  return `Focused turn ${turn.index} transcript, command, and file counts: ${turnSummaryLabel(turn)}.`;
}

function turnSummaryTitle(turn: AgentTurnView): string {
  return `Turn ${turn.index} transcript, command, and file counts: ${turnSummaryLabel(turn)}.`;
}

function conversationTurnContainerTitle(turn: AgentTurnView, focused: boolean): string {
  const focusState = focused ? "focused" : "not focused";
  return `Conversation turn card container for turn ${turn.index}: ${focusState}; ${turnSummaryLabel(turn)}.`;
}

function conversationTurnHeaderContainerTitle(turnIndex: number): string {
  return `Conversation turn header container for turn ${turnIndex}: title, metadata, and copy control.`;
}

function conversationTurnTitleContainerTitle(turnIndex: number): string {
  return `Conversation turn title container for turn ${turnIndex}: Turn label and transcript summary.`;
}

function conversationTurnMetadataContainerTitle(turnIndex: number): string {
  return `Conversation turn metadata container for turn ${turnIndex}: timing, touched-file count, and copy control.`;
}

function turnIndexLabelTitle(turnIndex: number): string {
  return `Conversation turn index label: Turn ${turnIndex}.`;
}

function turnTouchedFilesTitle(turnIndex: number, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return `Turn ${turnIndex} touched ${count} ${fileLabel}.`;
}

function conversationTurnTouchedFilesContainerTitle(turnIndex: number, count: number): string {
  const fileLabel = count === 1 ? "touched-file chip" : "touched-file chips";
  return `Conversation turn touched-files container for turn ${turnIndex}: ${count} ${fileLabel}.`;
}

function turnTouchedFileTitle(turnIndex: number, kind: string, path: string): string {
  return `Touched file in turn ${turnIndex}: ${kind} ${path}.`;
}

function agentLensFileTitle(turnIndex: number | null, kind: string, path: string): string {
  const scope = turnIndex ? `turn ${turnIndex}` : "the session";
  return `Agent Lens touched file for ${scope}: ${kind} ${path}.`;
}

function agentLensFileScopeMetaTitle(turnIndex: number | null, timeLabel: string | null): string {
  const timing = timeLabel ? ` at ${timeLabel}` : "";
  if (turnIndex) {
    return `Agent Lens file row timing: turn ${turnIndex}${timing}.`;
  }
  return `Agent Lens file row scope: session change log${timing}.`;
}

function agentLensFilePathMetaTitle(path: string): string {
  return `Agent Lens file row path: ${path}.`;
}

function agentLensFileKindMetaTitle(kind: AgentLensArtifactKind, changeKind: string): string {
  return `${kind} Agent Lens file row change type: ${changeKind}.`;
}

function agentLensFileActionsTitle(path: string, canShowRevert: boolean): string {
  const controls = canShowRevert
    ? "preview, open, ask, and revert controls"
    : "preview, open, and ask controls";
  return `Agent Lens file actions for ${path}: ${controls}.`;
}

function agentLensFileActionLabelTitle(label: string): string {
  return `Agent Lens file action label: ${label}.`;
}

function agentLensPreviewActionTitle(path: string, isSelected: boolean): string {
  return isSelected
    ? `Previewing Agent Lens details for ${path}.`
    : `Preview Agent Lens details for ${path}.`;
}

function agentLensOpenActionTitle(path: string, hasRepo: boolean): string {
  return hasRepo
    ? `Open ${path} from Agent Lens in the workspace.`
    : `Cannot open ${path} because the session repo is unavailable.`;
}

function agentLensAskActionTitle(path: string, canPrompt: boolean): string {
  return canPrompt
    ? `Draft an Agent Lens follow-up prompt for ${path}.`
    : `Cannot draft an Agent Lens prompt for ${path} because the session is archived or inactive.`;
}

function agentLensRevertActionTitle(
  path: string,
  turnIndex: number | null,
  canRevert: boolean,
  isReverting: boolean,
): string {
  const scope = turnIndex ? `turn ${turnIndex}` : "this turn";
  if (isReverting) return `Reverting ${path} from ${scope}.`;
  return canRevert
    ? `Revert ${path} from ${scope} checkpoint.`
    : `Stop the session before reverting ${path}.`;
}

function agentLensFileGroupCountTitle(kind: AgentLensArtifactKind, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return `${kind} artifact group contains ${count} ${fileLabel}.`;
}

function agentLensFileGroupTitle(kind: AgentLensArtifactKind, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return `Agent Lens ${kind} file group contains ${count} touched ${fileLabel}.`;
}

function agentLensFileGroupHeaderTitle(kind: AgentLensArtifactKind, count: number): string {
  const fileLabel = count === 1 ? "file" : "files";
  return `${kind} Agent Lens group heading for ${count} touched ${fileLabel}.`;
}

function agentLensFileGroupKindLabelTitle(kind: AgentLensArtifactKind): string {
  return `Agent Lens file group kind label: ${kind}.`;
}

function turnCommandSummaryTitle(turnIndex: number, commandSummary: string): string {
  return `Compact recent command output summary for turn ${turnIndex}: ${commandSummary}`;
}

function turnCommandSummaryContainerTitle(turnIndex: number): string {
  return `Conversation turn command-summary container for turn ${turnIndex}: 1 recent command summary.`;
}

function turnArtifactSummaryContainerTitle(turnIndex: number, categoryCount: number): string {
  const categoryLabel = categoryCount === 1 ? "artifact category chip" : "artifact category chips";
  return `Conversation turn artifact-summary container for turn ${turnIndex}: ${categoryCount} ${categoryLabel}.`;
}

function turnArtifactSummaryChipTitle(
  turnIndex: number,
  item: { kind: AgentLensArtifactKind; count: number },
): string {
  const fileLabel = item.count === 1 ? "file" : "files";
  return `${item.kind} artifacts touched in turn ${turnIndex}: ${item.count} ${fileLabel}.`;
}

function fileActionPrompt({
  path,
  kind,
  turnIndex,
  artifactKind,
  hunkSummary,
}: AgentLensFilePromptContext): string {
  const scope = turnIndex ? `turn ${turnIndex}` : "this session";
  return [
    `Focus on ${path}.`,
    `It was ${kind} in ${scope}.`,
    `Artifact category: ${artifactKind}.`,
    ...(hunkSummary ? [`Diff summary: ${hunkSummary}.`] : []),
    "Inspect the relevant file context, explain what still needs attention, and propose the next concrete edit or verification step.",
  ].join("\n");
}

function turnSummaryLabel(turn: AgentTurnView): string {
  const messages =
    (turn.userText ? 1 : 0) +
    turn.agentText.length +
    turn.systemText.length +
    turn.commandText.length;
  const parts = [`${messages} messages`];
  if (turn.commandText.length > 0) parts.push(`${turn.commandText.length} commands`);
  if (turn.changes.length > 0) parts.push(`${turn.changes.length} files`);
  return parts.join(" / ");
}

function turnArtifactSummary(
  changes: Array<{ path: string }>,
): Array<{ kind: AgentLensArtifactKind; count: number }> {
  if (changes.length === 0) return [];
  const counts = new Map<AgentLensArtifactKind, number>();
  for (const change of changes) {
    const kind = agentLensArtifactKind(change.path);
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  return AGENT_LENS_ARTIFACT_ORDER.map((kind) => ({
    kind,
    count: counts.get(kind) ?? 0,
  })).filter((item) => item.count > 0);
}

function turnArtifactSummaryText(changes: Array<{ path: string }>): string | null {
  const summary = turnArtifactSummary(changes);
  return summary.length > 0 ? summary.map((item) => `${item.kind} ${item.count}`).join(", ") : null;
}

function turnCommandSummaryText(turn: AgentTurnView): string | null {
  const commands = turn.commandText
    .map((text) => compactActivityText(text))
    .filter(Boolean)
    .slice(-2);
  return commands.length > 0 ? commands.join(" | ") : null;
}

function filterAgentTurns(turns: AgentTurnView[], query: string): AgentTurnView[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return turns;
  return turns.filter((turn) => agentTurnSearchText(turn).includes(normalized));
}

function turnNoun(count: number): string {
  return count === 1 ? "turn" : "turns";
}

function transcriptSearchNavigationTitle(
  direction: "previous" | "next",
  hasQuery: boolean,
  matchCount: number,
): string {
  const label = direction === "previous" ? "Previous" : "Next";
  if (!hasQuery) return `Search transcript to enable ${label.toLowerCase()} result navigation.`;
  if (matchCount < 2) return `${label} result navigation needs at least two matching turns.`;
  return `${label} search result`;
}

function transcriptSearchNavigationLabelTitle(direction: "previous" | "next"): string {
  return `Transcript search navigation label: ${direction} result.`;
}

function transcriptCopyButtonTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) return "No visible transcript turns to copy.";
  if (hasQuery) {
    return `Copy ${visibleCount} filtered transcript ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Copy all ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function latestTurnButtonTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) return "No visible transcript turns to jump to.";
  if (hasQuery) {
    return `Jump to the latest filtered transcript turn out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Jump to the latest of ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function transcriptSecondaryActionLabelTitle(label: "Latest" | "Copy visible" | "Copied"): string {
  return `Transcript secondary action label: ${label}.`;
}

function transcriptClearSearchLabelTitle(): string {
  return "Transcript clear-search label: Clear search.";
}

function transcriptToolsContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    if (visibleCount === 0) {
      return `Agent transcript tools: search active with no matching turns out of ${totalCount} total ${turnNoun(totalCount)}.`;
    }
    return `Agent transcript tools: search active with ${visibleCount} matching ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  if (totalCount === 0) {
    return "Agent transcript tools: search, result navigation, latest, and copy controls waiting for transcript turns.";
  }
  return `Agent transcript tools: search, result navigation, latest, and copy controls for all ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function transcriptSecondaryActionsTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) {
    if (hasQuery) {
      return `Transcript secondary actions: no matching transcript turns to jump to or copy out of ${totalCount} total ${turnNoun(totalCount)}.`;
    }
    return "Transcript secondary actions: latest-turn jump and copy-visible controls waiting for transcript turns.";
  }
  if (hasQuery) {
    return `Transcript secondary actions: latest filtered-turn jump and copy ${visibleCount} filtered transcript ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Transcript secondary actions: latest-turn jump and copy all ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function transcriptSearchContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    if (visibleCount === 0) {
      return `Transcript search: no matching turns out of ${totalCount} total ${turnNoun(totalCount)}.`;
    }
    return `Transcript search: ${visibleCount} matching ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Transcript search: find messages, commands, and files across ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function transcriptSearchLabelTitle(): string {
  return "Transcript search label: Search transcript.";
}

function transcriptSearchInputTitle(): string {
  return "Transcript search input placeholder: Find messages, commands, files. Press Escape to clear the search.";
}

function transcriptSearchCountTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    return `Transcript search count: ${visibleCount} matching ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Transcript search count: showing all ${totalCount} transcript ${turnNoun(totalCount)}.`;
}

function activeSearchResultPositionTitle(activeIndex: number, matchCount: number): string {
  if (activeIndex >= 0) {
    return `Active transcript search position: result ${activeIndex + 1} of ${matchCount} matching ${turnNoun(matchCount)}.`;
  }
  return `Active transcript search position: no result selected out of ${matchCount} matching ${turnNoun(matchCount)}.`;
}

function conversationContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
  readOnly: boolean,
): string {
  if (visibleCount === 0) {
    if (hasQuery) {
      return `Agent conversation transcript: no matching turns out of ${totalCount} total ${turnNoun(totalCount)}.`;
    }
    return readOnly
      ? "Agent conversation transcript: no saved turns."
      : "Agent conversation transcript: ready for the first turn.";
  }
  if (hasQuery) {
    return `Agent conversation transcript: ${visibleCount} matching ${turnNoun(visibleCount)} out of ${totalCount} total ${turnNoun(totalCount)}.`;
  }
  return `Agent conversation transcript: showing all ${totalCount} ${turnNoun(totalCount)}.`;
}

function turnCopyButtonTitle(turnIndex: number, copied: boolean): string {
  return copied
    ? `Copied full transcript for turn ${turnIndex} to clipboard.`
    : `Copy full transcript for turn ${turnIndex}, including messages, commands, and touched files.`;
}

function turnCopyLabelTitle(label: "Copy turn" | "Copied"): string {
  return `Conversation turn copy label: ${label}.`;
}

function messageCopyButtonTitle(turnIndex: number, label: string, copied: boolean): string {
  return copied
    ? `Copied ${label} message from turn ${turnIndex} to clipboard.`
    : `Copy ${label} message from turn ${turnIndex}.`;
}

function messageCopyLabelTitle(label: "Copy" | "Copied"): string {
  return `Message block copy label: ${label}.`;
}

function agentTurnSearchText(turn: AgentTurnView): string {
  return [
    agentTurnMessageSearchText(turn),
    agentTurnCommandSearchText(turn),
    agentTurnFileSearchText(turn),
  ]
    .join("\n")
    .toLocaleLowerCase();
}

function agentTurnSearchMatches(
  turn: AgentTurnView,
  query: string,
): Array<{ key: string; label: string; title: string }> {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return [];
  return [
    {
      key: "message",
      label: "Message match",
      title: "Search matched message text in this turn",
      text: agentTurnMessageSearchText(turn),
    },
    {
      key: "command",
      label: "Command match",
      title: "Search matched command output or its compact summary",
      text: agentTurnCommandSearchText(turn),
    },
    {
      key: "file",
      label: "File match",
      title: "Search matched a touched file, file status, or artifact category",
      text: agentTurnFileSearchText(turn),
    },
  ].filter((match) => match.text.toLocaleLowerCase().includes(normalized));
}

function agentTurnMessageSearchText(turn: AgentTurnView): string {
  return [turn.userText ?? "", ...turn.agentText, ...turn.systemText].join("\n");
}

function agentTurnCommandSearchText(turn: AgentTurnView): string {
  const commandSummary = turnCommandSummaryText(turn);
  return [
    ...turn.commandText,
    commandSummary ?? "",
    commandSummary ? `cmd ${commandSummary}` : "",
    commandSummary ? `recent command ${commandSummary}` : "",
  ].join("\n");
}

function agentTurnFileSearchText(turn: AgentTurnView): string {
  return [
    ...turn.changes.flatMap((change) => [
      change.path,
      change.kind,
      agentLensArtifactKind(change.path),
    ]),
    turnArtifactSummaryText(turn.changes) ?? "",
  ].join("\n");
}

function agentTurns(
  timeline: AgentSessionTimelineItem[],
  chunks: AgentSessionOutput[],
  session: AgentSession | undefined,
): AgentTurnView[] {
  if (timeline.length > 0) {
    return limitRestoredTurns(
      attachCheckpointChanges(buildTurnsFromTimeline(timeline), session),
      session,
    );
  }
  const text = chunks.map((chunk) => decodeBase64Text(chunk.chunk_base64)).join("");
  const cleaned = stripAnsi(text).replace(/\r\n/g, "\n").replace(/\r/g, "\n").trim();
  if (!cleaned) return [];
  const fallbackBlocks = cleaned
    .split(/\n{3,}/)
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => part);
  if (fallbackBlocks.length === 0) return [];
  return limitRestoredTurns(
    attachCheckpointChanges(
      [
        {
          id: "fallback-output",
          index: 1,
          startedAtMs: chunks[0]?.timestamp_ms ?? null,
          updatedAtMs: chunks[chunks.length - 1]?.timestamp_ms ?? chunks[0]?.timestamp_ms ?? null,
          userText: null,
          agentText: fallbackBlocks,
          commandText: [],
          systemText: [],
          changes: [],
          restoreCheckpointId: null,
          restoreReady: false,
        },
      ],
      session,
    ),
    session,
  );
}

function buildTurnsFromTimeline(timeline: AgentSessionTimelineItem[]): AgentTurnView[] {
  const turns: AgentTurnView[] = [];
  let current: AgentTurnView | null = null;
  for (const item of timeline) {
    const text = item.text.trim();
    if (!text) continue;
    if (item.kind === "user_message") {
      current = {
        id: item.id,
        index: turns.length + 1,
        startedAtMs: item.timestamp_ms,
        updatedAtMs: item.timestamp_ms,
        userText: text,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
        restoreCheckpointId: null,
        restoreReady: false,
      };
      turns.push(current);
      continue;
    }
    if (!current) {
      current = {
        id: item.id,
        index: turns.length + 1,
        startedAtMs: item.timestamp_ms,
        updatedAtMs: item.timestamp_ms,
        userText: null,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
        restoreCheckpointId: null,
        restoreReady: false,
      };
      turns.push(current);
    }
    current.updatedAtMs = item.timestamp_ms;
    appendTimelineText(current, item.kind, text);
  }
  return turns;
}

function appendTimelineText(
  turn: AgentTurnView,
  kind: AgentSessionTimelineItem["kind"],
  text: string,
) {
  const target =
    kind === "command_output"
      ? turn.commandText
      : kind === "lifecycle"
        ? turn.systemText
        : turn.agentText;
  const previous = target[target.length - 1];
  if (previous) {
    target[target.length - 1] = `${previous}${needsLineBreak(previous, text) ? "\n" : ""}${text}`;
  } else {
    target.push(text);
  }
}

function attachCheckpointChanges(
  turns: AgentTurnView[],
  session: AgentSession | undefined,
): AgentTurnView[] {
  if (!session?.turn_checkpoints?.length) return turns;
  const next = turns.map((turn) => ({ ...turn, changes: [...turn.changes] }));
  for (const checkpoint of session.turn_checkpoints) {
    let turn = next.find((candidate) => candidate.index === checkpoint.index);
    if (!turn) {
      turn = {
        id: checkpoint.id,
        index: checkpoint.index,
        startedAtMs: checkpoint.started_at_ms,
        updatedAtMs: checkpoint.ended_at_ms ?? checkpoint.started_at_ms,
        userText: null,
        agentText: [],
        commandText: [],
        systemText: [],
        changes: [],
        restoreCheckpointId: null,
        restoreReady: false,
      };
      next.push(turn);
    }
    turn.startedAtMs = turn.startedAtMs ?? checkpoint.started_at_ms;
    turn.updatedAtMs = Math.max(
      turn.updatedAtMs ?? checkpoint.started_at_ms,
      checkpoint.ended_at_ms ?? checkpoint.started_at_ms,
    );
    turn.changes = checkpoint.changes.map((change) => ({
      path: change.path,
      kind: change.kind,
    }));
    turn.restoreCheckpointId = checkpoint.id;
    turn.restoreReady = Boolean(checkpoint.restore_checkpoint);
  }
  return next.sort((a, b) => a.index - b.index);
}

function limitRestoredTurns(
  turns: AgentTurnView[],
  session: AgentSession | undefined,
): AgentTurnView[] {
  const restoredToTurnIndex = session?.restored_to_turn_index;
  if (!restoredToTurnIndex) return turns;
  return turns.filter((turn) => turn.index <= restoredToTurnIndex);
}

function needsLineBreak(left: string, right: string): boolean {
  if (!left || !right) return false;
  return !left.endsWith("\n") && !right.startsWith("\n");
}

function decodeBase64Text(chunk: string): string {
  const binary = atob(chunk);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}

const ansiEscapePattern = new RegExp(`${String.fromCharCode(27)}\\[[0-?]*[ -/]*[@-~]`, "g");

function stripAnsi(text: string): string {
  return text.replace(ansiEscapePattern, "");
}

function commandMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message?: unknown }).message ?? "Agent command failed.");
  }
  return String(error || "Agent command failed.");
}

function turnStatusLabel(status: string): string {
  switch (status) {
    case "working":
      return "Working";
    case "settling":
      return "Settling";
    default:
      return "Waiting";
  }
}

function sessionStatusLabel(status: string): string {
  switch (status) {
    case "starting":
      return "Starting";
    case "running":
      return "Running";
    case "completed":
      return "Done";
    case "failed":
      return "Failed";
    case "reverted":
      return "Reverted";
    case "error":
      return "Error";
    default:
      return status;
  }
}

function checkpointLabel(type: string): string {
  return type === "git_ref" ? "git checkpoint" : "filesystem checkpoint";
}

function agentLabel(agentType: string): string {
  switch (agentType) {
    case "codex":
      return "Codex";
    case "claude":
      return "Claude Code";
    case "opencode":
      return "OpenCode";
    default:
      return agentType;
  }
}

function agentLogoText(agentType: string): string {
  switch (agentType) {
    case "codex":
      return "Cx";
    case "claude":
      return "Cl";
    case "opencode":
      return "OC";
    default:
      return agentType.slice(0, 2).toUpperCase();
  }
}

function agentLogoSrc(agentType: string): string | null {
  switch (agentType) {
    case "codex":
      return codexLogo;
    case "claude":
      return claudeLogo;
    case "opencode":
      return opencodeLogo;
    default:
      return null;
  }
}

function agentLogoClass(agentType: string): string {
  return agentType.replace(/[^a-z0-9_-]/gi, "").toLowerCase() || "agent";
}

function repoName(repo: string): string {
  const parts = repo.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? repo;
}

function auditTitle(session: AgentSession): string {
  const pieces = [
    `Status: ${session.status}`,
    session.checkpoint
      ? `Checkpoint: ${checkpointLabel(session.checkpoint.checkpoint_type)}`
      : null,
    session.checkpoint?.git_hash ? `Git: ${session.checkpoint.git_hash.slice(0, 12)}` : null,
    `Changes: ${session.change_log?.length ?? 0}`,
    `Age: ${Math.round(session.age_ms / 1000)}s`,
    `Active sessions: ${session.active_sessions}`,
  ];
  return pieces.filter(Boolean).join(" / ");
}

function sessionStatusFacetTitle(status: AgentSession["status"]): string {
  return `Agent session status facet: ${sessionStatusLabel(status)}.`;
}

function turnStatusFacetTitle(turnStatus: string): string {
  return `Agent turn status facet: ${turnStatusLabel(turnStatus)}.`;
}

function checkpointStatusFacetTitle(checkpointType: string | null | undefined): string {
  return checkpointType
    ? `Agent checkpoint status facet: ${checkpointLabel(checkpointType)}.`
    : "Agent checkpoint status facet: no checkpoint.";
}

function changeLogStatusFacetTitle(count: number): string {
  const changeLabel = count === 1 ? "change" : "changes";
  return `Agent change-log status facet: ${count} ${changeLabel}.`;
}

function loadingSessionStatusTitle(): string {
  return "Agent session status strip: loading session.";
}

function loadingSessionStatusLabelTitle(): string {
  return "Agent session status-strip label: Loading session.";
}

function cancelPanelCloseStop(sessionId: string) {
  const timer = panelCloseStopTimers.get(sessionId);
  if (timer === undefined) return;
  window.clearTimeout(timer);
  panelCloseStopTimers.delete(sessionId);
}

function schedulePanelCloseStop(sessionId: string, delayMs = PANEL_CLOSE_STOP_DELAY_MS) {
  cancelPanelCloseStop(sessionId);
  const timer = window.setTimeout(() => {
    panelCloseStopTimers.delete(sessionId);
    void stopAgentSession(sessionId)
      .then(() => listAgentSessions())
      .then((sessions) => agentSessionStore.setSessions(sessions))
      .catch(() => {});
  }, delayMs);
  panelCloseStopTimers.set(sessionId, timer);
}
