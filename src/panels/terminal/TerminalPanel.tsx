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
    title: "Diseñar o revisar una interfaz de trabajo con identidad propia",
  },
  {
    id: "krt-interface-inquisitor",
    label: "Interface Inquisitor",
    title: "Hacer una crítica visual exigente de una interfaz ya implementada",
  },
  {
    id: "krt-repo-medic",
    label: "Repo Medic",
    title: "Diagnosticar la salud del repositorio, las pruebas y los riesgos de mantenimiento",
  },
  {
    id: "krt-ci-questor",
    label: "CI Questor",
    title: "Investigar fallos de CI y resumir sus causas probables",
  },
  {
    id: "krt-gitflow-knight",
    label: "Gitflow Knight",
    title: "Preparar commits acotados en una rama de funcionalidad adecuada",
  },
  {
    id: "krt-release-marshal",
    label: "Release Marshal",
    title: "Preparar el flujo de entrega, el pull request y el relevo de la publicación",
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
  { value: "auto", label: "Automático", description: "Usar el modelo predeterminado de Codex" },
  {
    value: "gpt-5.5",
    label: "GPT-5.5",
    description: "La mejor opción general para trabajo complejo con agentes",
  },
  {
    value: "gpt-5.4",
    label: "GPT-5.4",
    description: "Buen rendimiento general en programación y razonamiento",
  },
  {
    value: "gpt-5.4-mini",
    label: "GPT-5.4 mini",
    description: "Más rápido para revisiones y cambios pequeños",
  },
  {
    value: "gpt-5.3-codex",
    label: "GPT-5.3 Codex",
    description: "Opción de compatibilidad centrada en Codex",
  },
];

const CODEX_REASONING_OPTIONS: Array<{
  value: CodexReasoningSelection;
  label: string;
  description: string;
}> = [
  {
    value: "auto",
    label: "Automático",
    description: "Dejar que Codex use el nivel predeterminado del modelo",
  },
  { value: "minimal", label: "Mínimo", description: "El presupuesto de razonamiento más corto" },
  { value: "low", label: "Bajo", description: "Razonamiento rápido y ligero" },
  { value: "medium", label: "Medio", description: "Equilibrado para tareas de implementación" },
  { value: "high", label: "Alto", description: "Razonamiento más profundo para cambios delicados" },
  {
    value: "xhigh",
    label: "Muy alto",
    description: "El máximo esfuerzo cuando el modelo lo admite",
  },
];

const CODEX_SPEED_OPTIONS: Array<{
  value: CodexSpeedSelection;
  label: string;
  description: string;
}> = [
  {
    value: "standard",
    label: "Normal",
    description: "Respetar el modelo y el razonamiento seleccionados",
  },
  {
    value: "fast",
    label: "Rápido",
    description: "Priorizar opciones rápidas si no se han fijado el modelo ni el razonamiento",
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
    ? `${visibleTurns.length} ${turnNoun(visibleTurns.length)} coincidentes de ${turns.length} en total.`
    : `Se muestran los ${turns.length} ${turnNoun(turns.length)} de la transcripción.`;
  const activeSearchResultDescription = activeSearchResultLabel
    ? activeSearchResultIndex >= 0
      ? `Resultado de búsqueda seleccionado: ${activeSearchResultIndex + 1} de ${visibleTurns.length}.`
      : `No hay ningún resultado seleccionado entre los ${visibleTurns.length} coincidentes.`
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
        description: "Elegir el modelo activo de Codex",
        disabled: !canCompose || !isCodexSession,
        label: "Modelo",
        aliases: ["modelo"],
        runtimeCommand: "model" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "reasoning",
        command: "reasoning",
        description: "Elegir el nivel de razonamiento de Codex",
        disabled: !canCompose || !isCodexSession,
        label: "Razonamiento",
        aliases: ["razonamiento"],
        runtimeCommand: "reasoning" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "effort",
        command: "effort",
        description: "Alias del nivel de razonamiento",
        disabled: !canCompose || !isCodexSession,
        label: "Esfuerzo",
        aliases: ["razonamiento"],
        runtimeCommand: "reasoning" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "fast",
        command: "fast",
        description: "Activar o desactivar el ajuste rápido de Codex",
        disabled: !canCompose || !isCodexSession,
        label: "Rápido",
        aliases: ["speed", "velocidad", "rapido", "rapida"],
        runtimeCommand: "fast" as const,
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "test",
        command: "test",
        description: "Ejecutar la verificación más relevante para este repositorio",
        disabled: !canCompose,
        label: "Probar",
        prompt:
          "Ejecuta la verificación más relevante para este repositorio y resume los fallos antes de corregirlos.",
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "handoff",
        command: "handoff",
        description:
          "Resumir el estado de la sesión, los cambios, la verificación y el siguiente paso",
        disabled: !canCompose,
        label: "Relevo",
        prompt:
          "Resume el estado actual de la sesión, los archivos modificados, la verificación y el siguiente paso recomendado.",
        scope: "Codex" as const,
        trigger: "/" as const,
      },
      {
        id: "details",
        command: "details",
        description:
          "Abrir los detalles, archivos, comandos, Timeline y puntos de restauración de la sesión",
        disabled: !session,
        label: "Detalles",
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
  const composerCommandListboxId = `composer-command-menu-${sessionId}`;
  const composerHintId = `agent-composer-hint-${sessionId}`;
  const activeComposerCommand = commandMenuVisible
    ? (filteredComposerCommandItems[activeSlashCommandIndex] ?? filteredComposerCommandItems[0])
    : null;
  const activeComposerCommandId = activeComposerCommand
    ? composerCommandOptionId(sessionId, activeComposerCommand.id)
    : undefined;
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
          else setError("No se encontró la transcripción de la sesión.");
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
      if (event.key === "Home" && filteredComposerCommandItems.length > 0) {
        event.preventDefault();
        setActiveSlashCommandIndex(0);
        return;
      }
      if (event.key === "End" && filteredComposerCommandItems.length > 0) {
        event.preventDefault();
        setActiveSlashCommandIndex(filteredComposerCommandItems.length - 1);
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
      setRuntimeNotice(next === "fast" ? "Modo rápido activado." : "Modo rápido desactivado.");
      return next;
    });
    clearComposerCommand();
  };

  const executeHostCommand = async (command: AgentComposerHostCommand, argument?: string) => {
    if (!sessionId || !canCompose) return;
    if (command === "mascot") {
      setMascotAwake((current) => {
        const next = !current;
        setRuntimeNotice(
          next ? "La mascota está activa en este panel de Agent." : "Mascota oculta.",
        );
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
      setRuntimeNotice("Los comandos de memoria quedan pendientes para una futura fase de Tinto.");
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
      `Se restaurarán los archivos y la conversación al turno ${turn.index}. ¿Quieres continuar?`,
      {
        title: "Restaurar turno de Agent",
        kind: "warning",
        okLabel: "Restaurar",
        cancelLabel: "Cancelar",
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
    const ok = await confirm(
      "Se desharán todos los cambios hechos por esta sesión. ¿Quieres continuar?",
      {
        title: "Revertir sesión de Agent",
        kind: "warning",
        okLabel: "Revertir",
        cancelLabel: "Cancelar",
      },
    );
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
    const ok = await confirm(`¿Revertir ${path} al punto de control de este turno?`, {
      title: "Revertir archivo desde el turno",
      kind: "warning",
      okLabel: "Revertir archivo",
      cancelLabel: "Cancelar",
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
            {repo ? repoName(repo) : "Sesión de Agent"}
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
              {stopping ? "Deteniendo" : "Detener"}
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
              {reverting ? "Revirtiendo" : "Revertir"}
            </span>
          </button>
        </div>
      </header>

      {error && (
        <div
          className="agent-panel__error"
          data-testid="terminal-panel-error"
          role="alert"
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
              <span title={transcriptSearchLabelTitle()}>Buscar en la transcripción</span>
              <span className="sr-only" id="agent-transcript-search-hint">
                Pulsa Intro para recorrer los turnos coincidentes y Escape para borrar la búsqueda.
              </span>
              <input
                aria-describedby="agent-transcript-search-hint"
                aria-label="Buscar en la transcripción"
                ref={transcriptSearchRef}
                value={transcriptQuery}
                onChange={(event) => setTranscriptQuery(event.currentTarget.value)}
                onKeyDown={onTranscriptSearchKeyDown}
                placeholder="Buscar mensajes, comandos o archivos..."
                title={transcriptSearchInputTitle()}
                type="search"
              />
            </label>
            <div
              aria-label="Navegación por los resultados de la transcripción"
              className="agent-panel__chat-result-actions"
              role="group"
            >
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
                {hasTranscriptQuery
                  ? `${visibleTurns.length} de ${turns.length} turnos`
                  : "Todos los turnos"}
              </span>
              {activeSearchResultLabel && (
                <span
                  aria-label={activeSearchResultDescription ?? "Resultado de búsqueda activo"}
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
                aria-label="Resultado anterior"
                className="agent-panel__chat-nav"
                disabled={!canNavigateSearchResults}
                onClick={() => focusVisibleTurn("previous")}
                title={previousSearchResultTitle}
                type="button"
              >
                <span title={transcriptSearchNavigationLabelTitle("previous")}>Anterior</span>
              </button>
              <button
                aria-label="Resultado siguiente"
                className="agent-panel__chat-nav"
                disabled={!canNavigateSearchResults}
                onClick={() => focusVisibleTurn("next")}
                title={nextSearchResultTitle}
                type="button"
              >
                <span title={transcriptSearchNavigationLabelTitle("next")}>Siguiente</span>
              </button>
            </div>
            <div
              aria-label="Acciones de la transcripción"
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
                <span title={transcriptSecondaryActionLabelTitle("Último")}>Último</span>
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
                    copiedTarget === "transcript" ? "Copiado" : "Copiar lo visible",
                  )}
                >
                  {copiedTarget === "transcript" ? "Copiado" : "Copiar lo visible"}
                </span>
              </button>
              {session && (
                <button
                  aria-expanded={detailsOpen}
                  className="agent-panel__details-toggle"
                  onClick={() => setDetailsOpen((open) => !open)}
                  title={
                    detailsOpen
                      ? "Ocultar los detalles y volver a la conversación."
                      : "Mostrar los detalles, puntos de restauración, archivos, comandos y Timeline."
                  }
                  type="button"
                >
                  <span>{detailsOpen ? "Ocultar detalles" : "Detalles"}</span>
                </button>
              )}
            </div>
          </div>
          <main
            className="agent-panel__chat"
            aria-label="Conversación con Agent"
            aria-live="polite"
            aria-relevant="additions text"
            aria-atomic="false"
            role="log"
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
                  {hasTranscriptQuery ? "Sin coincidencias" : readOnly ? "Transcripción" : "Listo"}
                </span>
                {!hasTranscriptQuery && readOnly && (
                  <p title={emptyChatHelperTextTitle()}>
                    No se guardaron eventos de Timeline para esta sesión.
                  </p>
                )}
                {hasTranscriptQuery && (
                  <button
                    className="agent-panel__empty-chat-action"
                    onClick={() => resetTranscriptSearch({ focusSearch: true })}
                    title={emptyChatClearSearchActionTitle()}
                    type="button"
                  >
                    <span title={transcriptClearSearchLabelTitle()}>Borrar búsqueda</span>
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
          aria-label="Panel de inspección de Agent"
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
        <span className="sr-only" id={composerHintId}>
          {canCompose
            ? "Escribe / para ver comandos o $ para ver skills."
            : readOnly
              ? "La transcripción está archivada y es de solo lectura."
              : "El compositor no está disponible."}
        </span>
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
              setRuntimeNotice(`Modelo cambiado a ${codexModelLabel(value)}.`);
            }}
            onReasoningChange={(value) => {
              setSelectedReasoning(value);
              setRuntimeNotice(`Razonamiento cambiado a ${codexReasoningLabel(value)}.`);
            }}
            onSpeedChange={(value) => {
              setSelectedSpeed(value);
              setRuntimeNotice(
                value === "fast" ? "Modo rápido activado." : "Velocidad normal activada.",
              );
            }}
          />
        )}
        {commandMenuVisible && (
          <div
            aria-label="Comandos del compositor"
            className="agent-panel__slash-menu"
            id={composerCommandListboxId}
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
                  id={composerCommandOptionId(sessionId, command.id)}
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
                Ningún comando coincide con {composerCommandTrigger?.trigger}
                {composerCommandQuery}
              </div>
            )}
          </div>
        )}
        <div className="agent-panel__composer-row" title={agentComposerRowTitle(agentType, repo)}>
          <textarea
            aria-activedescendant={activeComposerCommandId}
            aria-controls={commandMenuVisible ? composerCommandListboxId : undefined}
            aria-describedby={composerHintId}
            aria-expanded={commandMenuVisible}
            aria-haspopup="listbox"
            aria-label={`Mensaje para ${agentLabel(agentType)}`}
            ref={composerInputRef}
            value={draft}
            onChange={(event) => onDraftChange(event.currentTarget.value)}
            onKeyDown={onDraftKeyDown}
            title={agentComposerInputTitle(agentType, repo, readOnly, canCompose)}
            placeholder={
              readOnly ? "Transcripción archivada" : `Mensaje para ${agentLabel(agentType)}`
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
            <span title={agentComposerSendLabelTitle(sending)}>
              {sending ? "Enviando" : "Enviar"}
            </span>
          </button>
        </div>
      </form>
    </div>
  );
}

function AgentMascotPanel({ agentType, repo }: { agentType: string; repo?: string }) {
  const label = agentLabel(agentType);
  const repoLabel = repo ? repoName(repo) : "esta sesión";
  return (
    <section
      aria-label="Asistente Tinto"
      className="agent-panel__mascot"
      title={`El asistente Tinto está activo con ${label} en ${repoLabel}.`}
    >
      <span
        aria-hidden="true"
        className="agent-panel__mascot-mark"
        title="Marca del asistente Tinto."
      >
        T
      </span>
      <div className="agent-panel__mascot-copy">
        <strong title="Estado del asistente Tinto.">Activo</strong>
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
  const working = summary.working_shortstat ?? "sin cambios de líneas sin preparar";
  const staged = summary.staged_shortstat ?? "sin cambios de líneas preparados";
  return (
    <section
      className="agent-panel__review-summary"
      aria-label="Resumen de la revisión"
      title={`Resumen de la revisión de ${summary.branch}: ${summary.changed_files} archivos modificados.`}
    >
      <div className="agent-panel__review-summary-head">
        <span title="Rama revisada.">{summary.branch}</span>
        <small title="Cantidad de archivos modificados en la revisión.">
          {summary.changed_files} {summary.changed_files === 1 ? "archivo" : "archivos"}
        </small>
      </div>
      <button
        aria-label="Preparar prompt de revisión semántica"
        className="agent-panel__review-action"
        disabled={!canPrompt}
        onClick={onPromptReview}
        title={reviewPromptActionTitle(canPrompt, findings.length)}
        type="button"
      >
        <span title="Acción para solicitar una revisión semántica.">Pedir revisión</span>
      </button>
      <button
        aria-label="Copiar resumen estructurado de la revisión"
        className="agent-panel__review-action"
        onClick={() => onCopySummary(summary, findings)}
        title={reviewSummaryCopyButtonTitle(copiedSummary)}
        type="button"
      >
        <span title={reviewSummaryCopyLabelTitle(copiedSummary ? "Copiado" : "Copiar resumen")}>
          {copiedSummary ? "Copiado" : "Copiar resumen"}
        </span>
      </button>
      {summary.files.length > 0 && (
        <button
          aria-label="Copiar archivos modificados de la revisión"
          className="agent-panel__review-action"
          onClick={() => onCopyFiles(summary)}
          title={reviewFilesCopyButtonTitle(copiedFiles, summary.files.length)}
          type="button"
        >
          <span title={reviewFilesCopyLabelTitle(copiedFiles ? "Copiado" : "Copiar archivos")}>
            {copiedFiles ? "Copiado" : "Copiar archivos"}
          </span>
        </button>
      )}
      {findings.length > 0 && (
        <button
          aria-label="Copiar hallazgos automáticos de la revisión"
          className="agent-panel__review-action"
          onClick={() => onCopyFindings(findings)}
          title={reviewFindingsCopyButtonTitle(copiedFindings, findings.length)}
          type="button"
        >
          <span
            title={reviewFindingsCopyLabelTitle(copiedFindings ? "Copiados" : "Copiar hallazgos")}
          >
            {copiedFindings ? "Copiados" : "Copiar hallazgos"}
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
          aria-label="Copiar prompt de revisión semántica"
          className="agent-panel__review-action"
          onClick={() => onCopyPrompt(promptDraft)}
          title={reviewPromptCopyButtonTitle(copiedPrompt, promptState)}
          type="button"
        >
          <span title={reviewPromptCopyLabelTitle(copiedPrompt ? "Copiado" : "Copiar prompt")}>
            {copiedPrompt ? "Copiado" : "Copiar prompt"}
          </span>
        </button>
      )}
      {promptTurnIndex != null && (
        <button
          aria-label="Mostrar el turno de la solicitud de revisión semántica"
          className="agent-panel__review-action"
          onClick={() => onShowPromptRequest(promptTurnIndex)}
          title={reviewPromptShowButtonTitle(promptTurnIndex)}
          type="button"
        >
          <span title="Mostrar la solicitud de revisión semántica.">Ver solicitud</span>
        </button>
      )}
      {(promptState || response) && (
        <button
          aria-label="Reiniciar el flujo de revisión semántica"
          className="agent-panel__review-action"
          onClick={onResetReview}
          title={reviewPromptResetButtonTitle(response != null)}
          type="button"
        >
          <span title="Reiniciar la revisión semántica.">Reiniciar revisión</span>
        </button>
      )}
      {response && (
        <div
          className="agent-panel__review-response"
          title={reviewResponseTitle(response.turnIndex)}
        >
          <strong title="Estado de la respuesta de revisión semántica.">
            Respuesta de revisión recibida
          </strong>
          <span title={`Extracto de la respuesta de revisión semántica: ${response.excerpt}`}>
            {response.excerpt}
          </span>
        </div>
      )}
      {response && (
        <button
          aria-label="Mostrar el turno de la respuesta de revisión semántica"
          className="agent-panel__review-action"
          onClick={() => onShowResponse(response)}
          title={reviewResponseShowButtonTitle(response.turnIndex)}
          type="button"
        >
          <span title="Mostrar la respuesta de revisión semántica.">Ver respuesta</span>
        </button>
      )}
      {response && (
        <button
          aria-label="Copiar respuesta de revisión semántica"
          className="agent-panel__review-action"
          onClick={() => onCopyResponse(response)}
          title={reviewResponseCopyButtonTitle(copiedResponse)}
          type="button"
        >
          <span
            title={reviewResponseCopyLabelTitle(copiedResponse ? "Copiada" : "Copiar respuesta")}
          >
            {copiedResponse ? "Copiada" : "Copiar respuesta"}
          </span>
        </button>
      )}
      {promptDraft && response && (
        <button
          aria-label="Copiar intercambio de revisión semántica"
          className="agent-panel__review-action"
          onClick={() => onCopyExchange(promptDraft, response)}
          title={reviewExchangeCopyButtonTitle(copiedExchange)}
          type="button"
        >
          <span
            title={reviewExchangeCopyLabelTitle(copiedExchange ? "Copiado" : "Copiar intercambio")}
          >
            {copiedExchange ? "Copiado" : "Copiar intercambio"}
          </span>
        </button>
      )}
      <div
        className="agent-panel__review-summary-stats"
        aria-label="Estadísticas del diff revisado"
      >
        <span title={`Diff del árbol de trabajo: ${working}`}>{working}</span>
        <span title={`Diff preparado: ${staged}`}>{staged}</span>
      </div>
      {visibleFiles.length > 0 ? (
        <ul
          className="agent-panel__review-summary-files"
          aria-label="Archivos modificados de la revisión"
        >
          {visibleFiles.map((file) => (
            <li key={file} title={`Archivo modificado de la revisión: ${file}`}>
              {file}
            </li>
          ))}
          {hiddenCount > 0 && (
            <li
              title={`El resumen de la revisión incluye ${hiddenCount} archivos modificados más.`}
            >
              +{hiddenCount} más
            </li>
          )}
        </ul>
      ) : (
        <p title="El resumen de la revisión no incluye cambios locales.">
          No se detectaron cambios locales.
        </p>
      )}
      {visibleFindings.length > 0 && (
        <ul className="agent-panel__review-findings" aria-label="Hallazgos de la revisión">
          {visibleFindings.map((finding, index) => {
            const location = reviewFindingLocation(finding);
            return (
              <li
                key={`${finding.title}:${finding.path ?? "session"}:${finding.line ?? index}`}
                title={`${finding.severity}: ${finding.title}. ${finding.detail}`}
              >
                <span title={`Gravedad del hallazgo: ${reviewSeverityLabel(finding.severity)}.`}>
                  {reviewSeverityLabel(finding.severity)}
                </span>
                <strong title={`Hallazgo de la revisión: ${finding.title}.`}>
                  {finding.title}
                </strong>
                {location && (
                  <small title={`Review finding location: ${location}.`}>{location}</small>
                )}
              </li>
            );
          })}
          {findings.length > visibleFindings.length && (
            <li
              title={`La revisión incluye ${findings.length - visibleFindings.length} hallazgos más.`}
            >
              <span title="Indicador de hallazgos adicionales.">más</span>
              <strong title="Cantidad de hallazgos adicionales.">
                +{findings.length - visibleFindings.length} hallazgos
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
    ? "Resumen estructurado de la revisión copiado al portapapeles."
    : "Copiar el resumen estructurado de la revisión al portapapeles.";
}

function reviewSeverityLabel(severity: string): string {
  switch (severity.toLocaleLowerCase()) {
    case "critical":
      return "crítico";
    case "high":
      return "alto";
    case "medium":
      return "medio";
    case "low":
      return "bajo";
    case "warning":
      return "advertencia";
    case "info":
      return "información";
    default:
      return severity;
  }
}

function reviewSummaryCopyLabelTitle(label: "Copiar resumen" | "Copiado"): string {
  return `Acción para copiar el resumen estructurado de la revisión: ${label}.`;
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
    ? "Archivos modificados de la revisión copiados al portapapeles."
    : `Copiar ${countLabel(fileCount, "archivo", "archivos")} al portapapeles.`;
}

function reviewFilesCopyLabelTitle(label: "Copiar archivos" | "Copiado"): string {
  return `Acción para copiar los archivos modificados: ${label}.`;
}

function reviewFilesCopyText(summary: AgentReviewSummary): string {
  if (summary.files.length === 0) return "Archivos modificados de la revisión: ninguno";
  const lines = [
    "Archivos modificados de la revisión:",
    ...summary.files.map((file) => `- ${file}`),
  ];
  if (summary.truncated_count > 0) {
    lines.push(`- +${summary.truncated_count} archivos modificados más`);
  }
  return lines.join("\n");
}

function reviewFindingsCopyButtonTitle(copied: boolean, findingCount: number): string {
  return copied
    ? "Hallazgos automáticos de la revisión copiados al portapapeles."
    : `Copiar ${countLabel(findingCount, "hallazgo", "hallazgos")} al portapapeles.`;
}

function reviewFindingsCopyLabelTitle(label: "Copiar hallazgos" | "Copiados"): string {
  return `Acción para copiar los hallazgos automáticos: ${label}.`;
}

function reviewFindingsCopyText(findings: AgentReviewFinding[]): string {
  if (findings.length === 0) return "Hallazgos de la revisión local: ninguno";
  return [
    "Hallazgos de la revisión local:",
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
    "Resumen estructurado de la revisión:",
    `Rama: ${summary.branch}`,
    `Archivos modificados: ${summary.changed_files}`,
    `Diff del árbol de trabajo: ${summary.working_shortstat ?? "sin cambios de líneas sin preparar"}`,
    `Diff preparado: ${summary.staged_shortstat ?? "sin cambios de líneas preparados"}`,
  ];
  if (summary.files.length > 0) {
    lines.push("Archivos:", ...summary.files.map((file) => `- ${file}`));
    if (summary.truncated_count > 0) {
      lines.push(`- +${summary.truncated_count} archivos modificados más`);
    }
  } else {
    lines.push("Archivos: ninguno");
  }
  if (findings.length > 0) {
    lines.push("Hallazgos de la revisión local:");
    for (const finding of findings) {
      const location = reviewFindingLocation(finding);
      lines.push(
        `- ${finding.severity}: ${finding.title}${location ? ` (${location})` : ""} - ${
          finding.detail
        }`,
      );
    }
  } else {
    lines.push("Hallazgos de la revisión local: ninguno");
  }
  return lines.join("\n");
}

function reviewPromptActionTitle(canPrompt: boolean, findingCount: number): string {
  const findingText =
    findingCount > 0
      ? `con ${countLabel(findingCount, "hallazgo", "hallazgos")}`
      : "sin hallazgos automáticos";
  return canPrompt
    ? `Preparar un prompt de revisión semántica del código a partir de este resumen ${findingText}.`
    : "No se puede preparar el prompt porque la sesión está archivada o inactiva.";
}

function reviewPromptStateLabel(state: "drafted" | "sent"): string {
  return state === "sent" ? "Solicitud de revisión enviada" : "Borrador de revisión listo";
}

function reviewPromptStateTitle(state: "drafted" | "sent"): string {
  return state === "sent"
    ? "El prompt de revisión semántica se envió como un turno de Agent."
    : "El prompt de revisión semántica está preparado en el compositor.";
}

function reviewPromptCopyButtonTitle(copied: boolean, state: "drafted" | "sent"): string {
  if (copied) return "Prompt de revisión semántica copiado al portapapeles.";
  return state === "sent"
    ? "Copiar el prompt de revisión semántica enviado al portapapeles."
    : "Copiar el borrador del prompt de revisión semántica al portapapeles.";
}

function reviewPromptCopyLabelTitle(label: "Copiar prompt" | "Copiado"): string {
  return `Acción para copiar el prompt de revisión semántica: ${label}.`;
}

function reviewPromptShowButtonTitle(turnIndex: number): string {
  return `Mostrar la solicitud de revisión semántica enviada en el turno ${turnIndex}.`;
}

function reviewPromptResetButtonTitle(hasResponse: boolean): string {
  return hasResponse
    ? "Reiniciar la respuesta capturada y la solicitud de revisión semántica de este resumen."
    : "Reiniciar el borrador del prompt de revisión semántica de este resumen.";
}

function reviewResponseTitle(turnIndex: number): string {
  return `Respuesta de revisión semántica recibida en el turno ${turnIndex}; verifica los hallazgos antes de actuar.`;
}

function reviewResponseCopyButtonTitle(copied: boolean): string {
  return copied
    ? "Respuesta de revisión semántica copiada al portapapeles."
    : "Copiar la respuesta de revisión semántica al portapapeles.";
}

function reviewResponseShowButtonTitle(turnIndex: number): string {
  return `Mostrar la respuesta completa de la revisión semántica en el turno ${turnIndex}.`;
}

function reviewResponseCopyLabelTitle(label: "Copiar respuesta" | "Copiada"): string {
  return `Acción para copiar la respuesta de revisión semántica: ${label}.`;
}

function reviewExchangeCopyButtonTitle(copied: boolean): string {
  return copied
    ? "Solicitud y respuesta de revisión semántica copiadas al portapapeles."
    : "Copiar la solicitud y la respuesta de revisión semántica al portapapeles.";
}

function reviewExchangeCopyLabelTitle(label: "Copiar intercambio" | "Copiado"): string {
  return `Acción para copiar el intercambio de revisión semántica: ${label}.`;
}

function reviewExchangeCopyText(prompt: string, response: AgentReviewResponseView): string {
  return [
    "Solicitud de revisión semántica:",
    prompt.trim(),
    "",
    "Respuesta de revisión semántica:",
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
    "Revisa los cambios actuales de Git en busca de errores, regresiones, riesgos de seguridad y pruebas ausentes.",
    `Rama: ${summary.branch}`,
    `Archivos modificados: ${summary.changed_files}`,
  ];
  if (summary.working_shortstat)
    lines.push(`Diff del árbol de trabajo: ${summary.working_shortstat}`);
  if (summary.staged_shortstat) lines.push(`Diff preparado: ${summary.staged_shortstat}`);
  if (summary.files.length > 0) {
    lines.push("Archivos:");
    for (const file of summary.files.slice(0, 12)) {
      lines.push(`- ${file}`);
    }
    if (summary.truncated_count > 0) {
      lines.push(`- y ${summary.truncated_count} archivos modificados más`);
    }
  }
  if (findings.length > 0) {
    lines.push("Hallazgos de la revisión local que debes comprobar primero:");
    for (const finding of findings.slice(0, 8)) {
      const location = reviewFindingLocation(finding);
      lines.push(
        `- ${finding.severity}: ${finding.title}${location ? ` (${location})` : ""} - ${finding.detail}`,
      );
    }
    if (findings.length > 8) {
      lines.push(`- y ${findings.length - 8} hallazgos locales más`);
    }
  }
  lines.push(
    "Presenta primero los hallazgos, ordenados por gravedad y con referencias a archivo y línea cuando sea posible. Si no hay problemas, indícalo con claridad y menciona cualquier carencia que quede en las pruebas.",
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
    <div className="agent-panel__runtime" aria-label="Controles de ejecución de Codex">
      <div className="agent-panel__runtime-buttons">
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "reasoning" ? null : "reasoning")}
          title={`Razonamiento: ${codexReasoningLabel(reasoning)}.`}
        >
          <span aria-hidden="true">○</span>
          <span>{codexReasoningShortLabel(reasoning)}</span>
        </button>
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "model" ? null : "model")}
          title={`Modelo: ${codexModelLabel(model)}.`}
        >
          <span aria-hidden="true">⚡</span>
          <span>{codexModelShortLabel(model)}</span>
        </button>
        <button
          type="button"
          className="agent-panel__runtime-button"
          disabled={disabled}
          onClick={() => onMenuChange(menu === "speed" ? null : "speed")}
          title={`Velocidad: ${codexSpeedLabel(speed)}.`}
        >
          <span aria-hidden="true">↗</span>
          <span>{codexSpeedLabel(speed)}</span>
        </button>
      </div>
      {notice && (
        <span
          className="agent-panel__runtime-notice"
          title={`Cambio de ejecución de Codex: ${notice}`}
        >
          {notice}
        </span>
      )}
      {menu && (
        <div
          className="agent-panel__runtime-popover"
          role="menu"
          title="Selector de ejecución de Codex"
        >
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
      title="Detalles de la sesión: mapa de turnos, actividad actual, puntos de restauración y Agent Lens."
    >
      <div>
        <strong>Detalles</strong>
        <small>
          {turns} {turnNoun(turns)} / {files} {files === 1 ? "archivo" : "archivos"}
          {focusedTurnIndex ? ` / T${focusedTurnIndex}` : ""}
        </small>
      </div>
      <button
        className="agent-panel__details-close"
        onClick={onClose}
        title="Cerrar los detalles de la sesión."
        type="button"
      >
        Cerrar
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
      aria-label="Resumen de la sesión de Agent"
      title={overviewSectionTitle(overview)}
    >
      <div
        className="agent-panel__overview-metrics"
        title={overviewMetricsContainerTitle(overview)}
      >
        <OverviewMetric value={overview.turns} label="Turnos" />
        <OverviewMetric value={overview.messages} label="Mensajes" />
        <OverviewMetric value={overview.commands} label="Comandos" />
        <OverviewMetric value={overview.files} label="Archivos" />
      </div>
      <div
        className="agent-panel__overview-activity"
        title={overviewLatestActivityGroupTitle(overview.latest)}
      >
        <span title={overviewLatestActivityLabelTitle()}>Actividad reciente</span>
        <p title={overviewLatestActivityTextTitle(overview.latest)}>
          {overview.latest ?? "Esperando el primer turno."}
        </p>
      </div>
      {overview.turnMap.length > 0 && (
        <div
          className="agent-panel__overview-turns"
          aria-label="Mapa de turnos"
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
                  {turn.files} {turn.files === 1 ? "archivo" : "archivos"}
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
  return `Mapa de turnos del resumen de la sesión de Agent: ${overviewMetricCount("Turnos", turnCount)}.`;
}

function overviewSectionTitle(overview: AgentSessionOverviewView): string {
  const latestSummary = overview.latest ?? "esperando el primer turno";
  const turnMapSummary =
    overview.turnMap.length > 0
      ? `mapa de turnos: ${overviewMetricCount("Turnos", overview.turnMap.length)}`
      : "mapa de turnos a la espera de actividad";
  return `Resumen de la sesión de Agent: ${overviewMetricCount(
    "Turnos",
    overview.turns,
  )}, ${overviewMetricCount("Mensajes", overview.messages)}, ${overviewMetricCount(
    "Comandos",
    overview.commands,
  )}, ${overviewMetricCount("Archivos", overview.files)}; actividad reciente: ${latestSummary}; ${turnMapSummary}.`;
}

function overviewMetricsContainerTitle(overview: AgentSessionOverviewView): string {
  return `Métricas del resumen de la sesión de Agent: ${overviewMetricCount(
    "Turnos",
    overview.turns,
  )}, ${overviewMetricCount("Mensajes", overview.messages)}, ${overviewMetricCount(
    "Comandos",
    overview.commands,
  )}, ${overviewMetricCount("Archivos", overview.files)}.`;
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
  return `Métrica ${label.toLowerCase()} del resumen de la sesión de Agent: ${overviewMetricCount(label, value)}.`;
}

function overviewMetricValueTitle(label: string, value: number): string {
  return `Valor de ${label.toLowerCase()} del resumen de la sesión de Agent: ${value}.`;
}

function overviewMetricLabelTitle(label: string): string {
  return `Etiqueta de métrica del resumen de la sesión de Agent: ${label}.`;
}

function overviewMetricCount(label: string, value: number): string {
  const singular = label.endsWith("s") ? label.slice(0, -1).toLowerCase() : label.toLowerCase();
  const unit = value === 1 ? singular : label.toLowerCase();
  return `${value} ${unit}`;
}

function countLabel(value: number, singular: string, plural: string): string {
  return `${value} ${value === 1 ? singular : plural}`;
}

function overviewLatestActivityGroupTitle(latest: string | null): string {
  return latest
    ? "Área de actividad reciente del resumen de la sesión de Agent: última actividad registrada."
    : "Área de actividad reciente del resumen de la sesión de Agent: esperando el primer turno.";
}

function overviewLatestActivityLabelTitle(): string {
  return "Etiqueta de actividad reciente del resumen de la sesión de Agent.";
}

function overviewLatestActivityTextTitle(latest: string | null): string {
  return latest
    ? `Actividad reciente del resumen de la sesión de Agent: ${latest}.`
    : "Actividad reciente del resumen de la sesión de Agent: esperando el primer turno.";
}

function AgentHostContextStrip({ session }: { session: AgentSession }) {
  const items = agentHostContextItems(session);
  if (items.length === 0) return null;
  return (
    <section
      aria-label="Contexto del turno"
      className="agent-panel__context-strip"
      title={agentHostContextStripTitle(items)}
    >
      <span title={agentHostContextLabelTitle()}>Contexto del turno</span>
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
    items.push({ kind: "goal", label: "Objetivo", value: goal });
  }
  const personality = compactContextValue(session.personality?.name ?? null);
  if (personality) {
    items.push({ kind: "personality", label: "Estilo", value: personality });
  }
  if (session.plan_mode?.enabled) {
    items.push({ kind: "plan", label: "Plan", value: "Activo" });
  }
  const summary = compactContextValue(session.context_summary?.text ?? null);
  if (summary) {
    items.push({ kind: "compact", label: "Resumen", value: summary });
  }
  return items;
}

function compactContextValue(value: string | null | undefined): string | null {
  const normalized = value?.trim().replace(/\s+/g, " ");
  if (!normalized) return null;
  return normalized.length > 80 ? `${normalized.slice(0, 77)}...` : normalized;
}

function agentHostContextStripTitle(items: AgentHostContextItem[]): string {
  return `Franja de contexto del turno: ${punctuatedTitleValue(
    items.map((item) => `${item.label} ${item.value}`).join("; "),
  )}`;
}

function agentHostContextLabelTitle(): string {
  return "Etiqueta del contexto del turno.";
}

function agentHostContextItemsTitle(count: number): string {
  return `Elementos del contexto del turno: ${countLabel(count, "elemento", "elementos")}.`;
}

function agentHostContextItemTitle(item: AgentHostContextItem): string {
  return `Contexto del turno, ${item.label.toLowerCase()}: ${punctuatedTitleValue(item.value)}`;
}

function agentHostContextItemLabelTitle(label: string): string {
  return `Etiqueta de elemento del contexto del turno: ${label}.`;
}

function agentHostContextItemValueTitle(item: AgentHostContextItem): string {
  return `Valor de ${item.label.toLowerCase()} del contexto del turno: ${punctuatedTitleValue(item.value)}`;
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
      aria-label="Actividad de Agent"
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
        <span title={agentActivityTurnsFactTitle(overview.turns)}>
          {overview.turns} {turnNoun(overview.turns)}
        </span>
        <span title={agentActivityFilesFactTitle(overview.files)}>
          {overview.files} {overview.files === 1 ? "archivo" : "archivos"}
        </span>
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
        aria-label="Turno seleccionado"
        title={focusedTurnIdleContainerTitle()}
      >
        <div className="agent-panel__turn-focus-head">
          <span title={focusedTurnHeadingLabelTitle()}>Turno seleccionado</span>
          <small title={focusedTurnIdleStatusLabelTitle()}>Inactivo</small>
        </div>
        <strong title={focusedTurnEmptyStateLabelTitle()}>Ningún turno seleccionado</strong>
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
      aria-label="Turno seleccionado"
      title={focusedTurnSelectedContainerTitle(turn.index)}
    >
      <div className="agent-panel__turn-focus-head">
        <span title={focusedTurnHeadingLabelTitle()}>Turno seleccionado</span>
        {timeLabel && (
          <small title={focusedTurnTimeTitle(turn.index, timeLabel)}>{timeLabel}</small>
        )}
      </div>
      <div className="agent-panel__turn-focus-title">
        <strong title={focusedTurnIndexLabelTitle(turn.index)}>Turno {turn.index}</strong>
        <small title={focusedTurnSummaryTitle(turn)}>{turnSummaryLabel(turn)}</small>
      </div>
      <p
        title={
          latest
            ? focusedTurnLatestActivityTitle(turn.index, latest)
            : focusedTurnFallbackTextTitle(turn.index)
        }
      >
        {latest ? compactActivityText(latest) : "No se capturó texto."}
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
          {turn.commandText.length} {turn.commandText.length === 1 ? "comando" : "comandos"}
        </span>
        <span title={focusedTurnFactTitle(turn.index, "files", turn.changes.length)}>
          {turn.changes.length} {turn.changes.length === 1 ? "archivo" : "archivos"}
        </span>
      </div>
      {artifactSummary.length > 0 && (
        <div
          className="agent-panel__turn-artifacts"
          aria-label="Resumen de artefactos del turno seleccionado"
          title={focusedTurnArtifactSummaryContainerTitle(turn.index, artifactSummary.length)}
        >
          {artifactSummary.map((item) => (
            <span key={item.kind} title={turnArtifactSummaryChipTitle(turn.index, item)}>
              {artifactKindLabel(item.kind)} {item.count}
            </span>
          ))}
        </div>
      )}
      {commandSummary && (
        <div
          className="agent-panel__turn-commands"
          aria-label="Resumen de comandos del turno seleccionado"
          title={focusedTurnCommandSummaryContainerTitle(turn.index)}
        >
          <span title={turnCommandSummaryTitle(turn.index, commandSummary)}>
            Comando reciente: {commandSummary}
          </span>
        </div>
      )}
      {visibleChanges.length > 0 && (
        <div
          className="agent-panel__turn-focus-files"
          aria-label="Archivos del turno seleccionado"
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
              {changeKindLabel(change.kind)} {change.path}
            </span>
          ))}
          {hiddenChangeCount > 0 && (
            <span title={focusedTurnHiddenFileOverflowTitle(turn.index, hiddenChangeCount)}>
              +{hiddenChangeCount} más
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
            {isRestoringThisTurn ? "Restaurando" : "Restaurar aquí"}
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
  const searchMatchesLabel = `Coincidencias de búsqueda del turno ${turn.index}`;
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
          <span title={turnIndexLabelTitle(turn.index)}>Turno {turn.index}</span>
          <small title={turnSummaryTitle(turn)}>{turnSummaryLabel(turn)}</small>
        </div>
        <div
          className="agent-panel__chat-turn-meta"
          title={conversationTurnMetadataContainerTitle(turn.index)}
        >
          {timeLabel && <small title={turnTimeTitle(turn.index, timeLabel)}>{timeLabel}</small>}
          {turn.changes.length > 0 && (
            <small title={turnTouchedFilesTitle(turn.index, turn.changes.length)}>
              {turn.changes.length}{" "}
              {turn.changes.length === 1 ? "archivo modificado" : "archivos modificados"}
            </small>
          )}
          <button
            className="agent-panel__turn-copy"
            onClick={() => onCopyTurn(turnTarget, turnTranscriptText(turn, firstTurnAtMs))}
            title={turnCopyButtonTitle(turn.index, turnCopied)}
            type="button"
          >
            <span title={turnCopyLabelTitle(turnCopied ? "Copiado" : "Copiar turno")}>
              {turnCopied ? "Copiado" : "Copiar turno"}
            </span>
          </button>
        </div>
      </div>
      {artifactSummary.length > 0 && (
        <div
          className="agent-panel__turn-artifacts"
          aria-label={`Resumen de artefactos del turno ${turn.index}`}
          title={turnArtifactSummaryContainerTitle(turn.index, artifactSummary.length)}
        >
          {artifactSummary.map((item) => (
            <span key={item.kind} title={turnArtifactSummaryChipTitle(turn.index, item)}>
              {artifactKindLabel(item.kind)} {item.count}
            </span>
          ))}
        </div>
      )}
      {commandSummary && (
        <div
          className="agent-panel__turn-commands"
          aria-label={`Resumen de comandos del turno ${turn.index}`}
          title={turnCommandSummaryContainerTitle(turn.index)}
        >
          <span title={turnCommandSummaryTitle(turn.index, commandSummary)}>
            Comando reciente: {commandSummary}
          </span>
        </div>
      )}
      {searchMatches.length > 0 && (
        <div
          className="agent-panel__turn-search-matches"
          aria-label={searchMatchesLabel}
          title={`${searchMatchesLabel}: explica por qué este turno coincide con la búsqueda.`}
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
          label="Tú"
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
            "Comando",
            copiedTarget === `${turn.id}:command:${index}`,
          )}
          kind="command_output"
          label="Comando"
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
            "Sistema",
            copiedTarget === `${turn.id}:system:${index}`,
          )}
          kind="lifecycle"
          label="Sistema"
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
              {changeKindLabel(change.kind)} {change.path}
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
  const commandSummaryLabel = commandSummary ?? "Salida del comando";
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
          aria-label={`Copiar mensaje de ${label}`}
          onClick={onCopy}
          title={copyTitle}
          type="button"
        >
          <span title={messageCopyLabelTitle(copied ? "Copiado" : "Copiar")}>
            {copied ? "Copiado" : "Copiar"}
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
              <small title={collapsedCommandDisclosureLabelTitle()}>Mostrar salida</small>
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
  if (!firstLine) return "Salida del comando";
  return firstLine.length > 96 ? `${firstLine.slice(0, 93)}...` : firstLine;
}

function agentErrorBannerTitle(error: string): string {
  return `Aviso de error de la sesión de Agent: ${error}`;
}

function emptyChatContainerTitle(hasTranscriptQuery: boolean, readOnly: boolean): string {
  if (hasTranscriptQuery) {
    return "Estado vacío de la conversación con Agent: ningún turno coincide con la búsqueda actual.";
  }
  if (readOnly) {
    return "Estado vacío de la conversación con Agent: la transcripción archivada no contiene turnos guardados.";
  }
  return "Estado vacío de la conversación con Agent: lista para el primer turno.";
}

function emptyChatStateLabelTitle(hasTranscriptQuery: boolean, readOnly: boolean): string {
  if (hasTranscriptQuery) {
    return "Etiqueta del estado vacío de la conversación con Agent: Sin coincidencias.";
  }
  if (readOnly) {
    return "Etiqueta del estado vacío de la conversación con Agent: Transcripción.";
  }
  return "Etiqueta del estado vacío de la conversación con Agent: Listo.";
}

function emptyChatHelperTextTitle(): string {
  return "Estado de la transcripción archivada: no se guardaron turnos.";
}

function emptyChatClearSearchActionTitle(): string {
  return "Acción del estado vacío de la conversación con Agent: borrar la búsqueda, recuperar todos los turnos y devolver el foco al buscador.";
}

function agentHeaderTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Cabecera de ${agentLabel(agentType)} para ${repoLabel}: identidad, estado y controles de sesión.`;
}

function agentPanelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Superficie de la sesión de ${agentLabel(agentType)} para ${repoLabel}: cabecera, estado, conversación, Agent Lens y compositor.`;
}

function agentWorkspaceTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Espacio de trabajo de ${agentLabel(agentType)} para ${repoLabel}: resumen, actividad, conversación, panel de inspección y compositor.`;
}

function agentChatShellTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Contenedor de chat de ${agentLabel(agentType)} para ${repoLabel}: herramientas de transcripción y columna de conversación.`;
}

function agentSideRailTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Panel lateral de ${agentLabel(agentType)} para ${repoLabel}: turno seleccionado y columna de Agent Lens.`;
}

function agentComposerTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Compositor de ${agentLabel(agentType)} para ${repoLabel}: menú de comandos, menciones de skills y entrada de mensajes.`;
}

function agentCommandMenuTitle(
  trigger: { trigger: AgentComposerCommandTrigger; query: string } | null,
  count: number,
): string {
  const prefix = trigger?.trigger ?? "/";
  const query = trigger?.query ?? "";
  const suffix = query
    ? ` ${count === 1 ? "coincidente" : "coincidentes"} con ${prefix}${query}`
    : "";
  const noun = count === 1 ? "comando" : "comandos";
  return `Menú de comandos del compositor: ${count} ${noun}${suffix}.`;
}

function composerCommandOptionId(sessionId: string, commandId: string): string {
  return `composer-command-${sessionId}-${commandId}`;
}

function agentComposerCommandTitle(command: AgentComposerCommand): string {
  const state = command.disabled ? "No disponible" : "Ejecutar";
  const aliases = agentComposerCommandAliasTitle(command);
  return `${state} ${command.trigger}${command.command}: ${command.description}.${aliases}`;
}

function agentComposerCommandCodeTitle(command: AgentComposerCommand): string {
  return `Activador del comando del compositor: ${command.trigger}${command.command}.`;
}

function agentComposerCommandLabelTitle(label: string): string {
  return `Etiqueta del comando del compositor: ${label}.`;
}

function agentComposerCommandDescriptionTitle(command: AgentComposerCommand): string {
  const aliases = agentComposerCommandAliasTitle(command);
  return `Descripción del comando ${command.trigger}${command.command} del compositor, ámbito ${command.scope}: ${command.description}.${aliases}`;
}

function agentComposerCommandAliasText(command: AgentComposerCommand): string {
  const aliases = command.aliases?.slice(0, 3) ?? [];
  if (aliases.length === 0) return "";
  return ` · También ${aliases.map((alias) => `${command.trigger}${alias}`).join(", ")}`;
}

function agentComposerCommandAliasTitle(command: AgentComposerCommand): string {
  const aliases = command.aliases ?? [];
  if (aliases.length === 0) return "";
  return ` Alias disponibles: ${aliases.map((alias) => `${command.trigger}${alias}`).join(", ")}.`;
}

function agentCommandEmptyTitle(
  trigger: { trigger: AgentComposerCommandTrigger; query: string } | null,
): string {
  const prefix = trigger?.trigger ?? "/";
  return `Ningún comando del compositor coincide con ${prefix}${trigger?.query ?? ""}.`;
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
    setters.setNotice(next === "fast" ? "Modo rápido activado." : "Modo rápido desactivado.");
    return true;
  }
  if (command === "model" || command === "modelo") {
    const model = normalizeCodexModel(rawValue);
    setters.setModel(model);
    setters.setNotice(`Modelo cambiado a ${codexModelLabel(model)}.`);
    return true;
  }
  if (command !== "reasoning" && command !== "razonamiento" && command !== "effort") {
    return false;
  }
  const reasoning = normalizeCodexReasoning(rawValue);
  setters.setReasoning(reasoning);
  setters.setNotice(`Razonamiento cambiado a ${codexReasoningLabel(reasoning)}.`);
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
  return CODEX_MODEL_OPTIONS.find((option) => option.value === value)?.label ?? "Automático";
}

function codexModelShortLabel(value: CodexModelSelection): string {
  if (value === "auto") return "Modelo";
  return codexModelLabel(value).replace(/^GPT-/, "");
}

function codexReasoningLabel(value: CodexReasoningSelection): string {
  return CODEX_REASONING_OPTIONS.find((option) => option.value === value)?.label ?? "Automático";
}

function codexReasoningShortLabel(value: CodexReasoningSelection): string {
  if (value === "auto") return "Razonamiento";
  return codexReasoningLabel(value);
}

function codexSpeedLabel(value: CodexSpeedSelection): string {
  return CODEX_SPEED_OPTIONS.find((option) => option.value === value)?.label ?? "Normal";
}

function agentComposerRowTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Fila de entrada del compositor de ${agentLabel(agentType)} para ${repoLabel}: borrador del mensaje y control de envío.`;
}

function agentComposerInputTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  canCompose: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "la sesión de Agent";
  if (readOnly) {
    return `Entrada de mensajes de ${agentLabel(agentType)} para ${repoLabel}: la transcripción archivada es de solo lectura.`;
  }
  if (!canCompose) {
    return `Entrada de mensajes de ${agentLabel(agentType)} para ${repoLabel}: esperando una sesión en la que se pueda escribir.`;
  }
  return `Entrada de mensajes de ${agentLabel(agentType)} para ${repoLabel}: prepara la siguiente instrucción.`;
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
  const repoLabel = repo ? repoName(repo) : "la sesión de Agent";
  if (sending) {
    return `Enviar mensaje a ${agentLabel(agentType)} para ${repoLabel}: enviando el borrador.`;
  }
  if (readOnly) {
    return `Enviar mensaje a ${agentLabel(agentType)} para ${repoLabel}: la transcripción archivada es de solo lectura.`;
  }
  if (!canCompose) {
    return `Enviar mensaje a ${agentLabel(agentType)} para ${repoLabel}: esperando una sesión en la que se pueda escribir.`;
  }
  if (!hasDraft) {
    return `Enviar mensaje a ${agentLabel(agentType)} para ${repoLabel}: el mensaje está vacío.`;
  }
  if (!canSend) {
    return `Enviar mensaje a ${agentLabel(agentType)} para ${repoLabel}: el mensaje está vacío o no está disponible.`;
  }
  return `Enviar el borrador a ${agentLabel(agentType)} para ${repoLabel}.`;
}

function agentComposerSendLabelTitle(sending: boolean): string {
  return `Acción de envío del compositor: ${sending ? "Enviando" : "Enviar"}.`;
}

function agentIdentityTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Identidad de ${agentLabel(agentType)} para ${repoLabel}: nombre de Agent y etiqueta del repositorio.`;
}

function agentHeaderActionsTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Acciones de cabecera de ${agentLabel(agentType)} para ${repoLabel}: controles Detener y Revertir.`;
}

function agentStopControlTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  canStop: boolean,
  stopping: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "la sesión de Agent";
  if (stopping) {
    return `Detener ${agentLabel(agentType)} en ${repoLabel}: deteniendo la sesión.`;
  }
  if (readOnly) {
    return `Detener ${agentLabel(agentType)} en ${repoLabel}: las transcripciones archivadas son de solo lectura.`;
  }
  if (canStop) {
    return `Detener la sesión en ejecución de ${agentLabel(agentType)} en ${repoLabel}.`;
  }
  return `Detener ${agentLabel(agentType)} en ${repoLabel}: la sesión no está en ejecución.`;
}

function agentStopControlLabelTitle(stopping: boolean): string {
  return `Acción para detener Agent: ${stopping ? "Deteniendo" : "Detener"}.`;
}

function agentRevertControlTitle(
  agentType: string,
  repo: string | undefined,
  readOnly: boolean,
  session: AgentSession | null,
  canRevert: boolean,
  reverting: boolean,
): string {
  const repoLabel = repo ? repoName(repo) : "la sesión de Agent";
  if (reverting) {
    return `Revertir cambios de ${agentLabel(agentType)} en ${repoLabel}: revirtiendo.`;
  }
  if (readOnly) {
    return `Revertir cambios de ${agentLabel(agentType)} en ${repoLabel}: la transcripción archivada es de solo lectura.`;
  }
  if (session?.status === "reverted") {
    return `Revertir cambios de ${agentLabel(agentType)} en ${repoLabel}: la sesión ya se revirtió.`;
  }
  if (session && !session.checkpoint) {
    return `Revertir cambios de ${agentLabel(agentType)} en ${repoLabel}: no hay un punto de control reversible.`;
  }
  if (canRevert) {
    return `Revertir los cambios de la sesión de ${agentLabel(agentType)} en ${repoLabel}.`;
  }
  return `Revertir cambios de ${agentLabel(agentType)} en ${repoLabel}: detén la sesión antes de revertir.`;
}

function agentRevertControlLabelTitle(reverting: boolean): string {
  return `Acción para revertir Agent: ${reverting ? "Revirtiendo" : "Revertir"}.`;
}

function agentLogoTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Logotipo de ${agentLabel(agentType)} para ${repoLabel}: marca de Agent.`;
}

function agentRepoLabelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  if (!repo) {
    return `Etiqueta del repositorio de ${agentLabel(agentType)} para ${repoLabel}: sin ruta de repositorio.`;
  }
  return `Etiqueta del repositorio de ${agentLabel(agentType)} para ${repoLabel}: ruta completa ${repo}.`;
}

function agentNameLabelTitle(agentType: string, repo?: string): string {
  const repoLabel = repo ? repoName(repo) : "sesión de Agent";
  return `Etiqueta del nombre visible de ${agentLabel(agentType)} para ${repoLabel}.`;
}

function collapsedCommandSummaryLabelTitle(summary: string): string {
  return `Resumen de la salida contraída del comando: ${summary}.`;
}

function collapsedCommandDisclosureLabelTitle(): string {
  return "Etiqueta del control de salida contraída del comando: Mostrar salida.";
}

function collapsedCommandBlockTitle(turnIndex: number): string {
  return `Contenedor de la salida contraída del comando del turno ${turnIndex}: control de resumen y texto completo de la salida.`;
}

function collapsedCommandSummaryRowTitle(turnIndex: number): string {
  return `Fila de resumen de la salida contraída del comando del turno ${turnIndex}: actívala para mostrar u ocultar la salida completa.`;
}

function messageRoleLabelTitle(label: string): string {
  return `Etiqueta del rol del mensaje de Agent: ${label}.`;
}

function messageBlockContainerTitle(turnIndex: number, label: string): string {
  return `Bloque de mensaje de Agent del turno ${turnIndex}: contenido de ${label} y control para copiar.`;
}

function messageMarkdownContentTitle(turnIndex: number, label: string): string {
  return `Contenido Markdown de ${label} renderizado para el turno ${turnIndex}.`;
}

function messageTerminalContentTitle(turnIndex: number): string {
  return `Texto de salida de comandos del turno ${turnIndex}.`;
}

function messageHeaderTitle(label: string): string {
  return `Cabecera del mensaje de Agent para ${label}: etiqueta de rol y control para copiar.`;
}

function SessionStatus({ session }: { session: AgentSession | undefined }) {
  if (!session) {
    return (
      <div
        className="agent-panel__status-strip"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        title={loadingSessionStatusTitle()}
      >
        <span title={loadingSessionStatusLabelTitle()}>Cargando sesión</span>
      </div>
    );
  }
  return (
    <div
      className="agent-panel__status-strip"
      role="status"
      aria-live="polite"
      aria-atomic="true"
      title={auditTitle(session)}
    >
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
        {session.checkpoint
          ? checkpointLabel(session.checkpoint.checkpoint_type)
          : "Sin punto de control"}
      </span>
      {(session.change_log?.length ?? 0) > 0 && (
        <span title={changeLogStatusFacetTitle(session.change_log?.length ?? 0)}>
          {session.change_log?.length} {session.change_log?.length === 1 ? "cambio" : "cambios"}
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
      ? `Turno ${focusedTurn.index}`
      : `${turns.length} ${turnNoun(turns.length)}`;
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
        aria-label="Ámbito de Agent Lens"
        title={agentLensScopeGroupTitle(activeScope, focusedTurn?.index ?? null)}
      >
        <button
          aria-pressed={activeScope === "focused"}
          disabled={!focusedTurn}
          onClick={() => setScope("focused")}
          title={agentLensScopeTitle("focused", focusedTurn?.index ?? null)}
          type="button"
        >
          Turno
        </button>
        <button
          aria-pressed={activeScope === "session"}
          onClick={() => setScope("session")}
          title={agentLensScopeTitle("session", focusedTurn?.index ?? null)}
          type="button"
        >
          Sesión
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
            Estado del turno
          </small>
        </div>
        <div>
          <span title={agentLensFileMetricValueTitle(activeScope, fileItems.length)}>
            {fileItems.length}
          </span>
          <small title={agentLensFileMetricTitle(activeScope, fileItems.length)}>
            {activeScope === "focused" ? "Archivos del turno" : "Archivos de la sesión"}
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
            Puntos de restauración
          </small>
        </div>
      </div>

      <div
        className="agent-panel__lens-tabs"
        role="tablist"
        aria-label="Vistas de Agent Lens"
        aria-orientation="horizontal"
        title={agentLensTabListTitle(fileItems.length, commandItems.length, timelineItems.length)}
      >
        <AgentLensTabButton
          active={activeTab === "files"}
          controlsId={agentLensPanelId(session.id, "files")}
          count={fileItems.length}
          id={agentLensTabId(session.id, "files")}
          label="Archivos"
          onClick={() => activateTab("files")}
          onKeyDown={(event) => handleTabKeyDown(event, "files")}
        />
        <AgentLensTabButton
          active={activeTab === "commands"}
          controlsId={agentLensPanelId(session.id, "commands")}
          count={commandItems.length}
          id={agentLensTabId(session.id, "commands")}
          label="Comandos"
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
          aria-label="Vista de archivos de Agent Lens"
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
                <span title={agentLensFileFilterLabelTitle()}>Filtrar archivos</span>
                <input
                  aria-describedby={agentLensFileFilterStatusId(session.id)}
                  aria-label="Filtrar archivos modificados"
                  ref={fileFilterRef}
                  value={fileQuery}
                  onChange={(event) => setFileQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasFileQuery) {
                      event.preventDefault();
                      clearFileFilter();
                    }
                  }}
                  placeholder="Ruta o tipo de cambio..."
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
                    ? `${filteredFileItems.length} de ${fileItems.length} archivos`
                    : `${fileItems.length} ${fileItems.length === 1 ? "archivo" : "archivos"}`}
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
                    <span title={agentLensClearFileFilterLabelTitle()}>Borrar</span>
                  </button>
                )}
              </label>
              {filteredFileItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Archivos modificados"
                  title={agentLensTouchedFilesListTitle(
                    activeScope,
                    filteredFileItems.length,
                    hasFileQuery,
                  )}
                >
                  <div
                    className="agent-panel__lens-preview"
                    aria-label="Vista previa del archivo seleccionado"
                    onKeyDown={handlePreviewKeyDown}
                    tabIndex={previewCount > 1 ? 0 : undefined}
                    title={agentLensPreviewContainerTitle(
                      previewItem?.path ?? null,
                      previewPosition,
                      previewCount,
                    )}
                  >
                    <div className="agent-panel__lens-preview-head">
                      <span title={agentLensPreviewLabelTitle()}>Vista previa</span>
                      <small title={agentLensPreviewPositionTitle(previewPosition, previewCount)}>
                        {previewPosition} / {previewCount}
                      </small>
                    </div>
                    <strong title={agentLensPreviewSelectionTitle(previewItem?.path ?? null)}>
                      {previewItem?.path ?? "Ningún archivo seleccionado"}
                    </strong>
                    {previewCount > 1 && (
                      <div
                        aria-label="Navegación por la vista previa"
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
                          <span title={agentLensPreviewNavLabelTitle("Anterior")}>Anterior</span>
                        </button>
                        <button
                          onClick={selectNextPreview}
                          title={agentLensPreviewNavButtonTitle(
                            "next",
                            nextPreviewItem?.path ?? null,
                          )}
                          type="button"
                        >
                          <span title={agentLensPreviewNavLabelTitle("Siguiente")}>Siguiente</span>
                        </button>
                      </div>
                    )}
                    {previewItem && (
                      <div
                        aria-label={`Acciones de vista previa para ${previewItem.path}`}
                        className="agent-panel__lens-preview-actions"
                        title={agentLensPreviewActionsTitle(
                          previewItem.path,
                          Boolean(previewItem.turnCheckpointId),
                        )}
                      >
                        <button
                          aria-label="Abrir el archivo seleccionado"
                          disabled={!repo}
                          onClick={() => onOpenFile(previewItem.path)}
                          title={agentLensOpenActionTitle(previewItem.path, Boolean(repo))}
                          type="button"
                        >
                          <span title={agentLensPreviewActionLabelTitle("Abrir")}>Abrir</span>
                        </button>
                        <button
                          aria-label="Preguntar por el archivo seleccionado"
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
                          <span title={agentLensPreviewActionLabelTitle("Preguntar")}>
                            Preguntar
                          </span>
                        </button>
                        {previewItem.turnCheckpointId && (
                          <button
                            aria-label="Revertir el archivo seleccionado"
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
                                isPreviewReverting ? "Revirtiendo" : "Revertir",
                              )}
                            >
                              {isPreviewReverting ? "Revirtiendo" : "Revertir"}
                            </span>
                          </button>
                        )}
                      </div>
                    )}
                    {previewItem?.context?.preview ? (
                      <div
                        aria-label={`Detalles de la vista previa de ${previewItem.path}`}
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
                        No hay datos en vivo de los fragmentos de este archivo.
                      </p>
                    )}
                  </div>
                  {groupedFileItems.map((group) => (
                    <section
                      aria-label={`Archivos de ${group.kind.toLocaleLowerCase()}`}
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
                              {item.turnIndex ? `Turno ${item.turnIndex}` : "Sesión"}
                              {item.timeLabel ? ` - ${item.timeLabel}` : ""}
                            </span>
                            <strong title={agentLensFilePathMetaTitle(item.path)}>
                              {item.path}
                            </strong>
                            <small title={agentLensFileKindMetaTitle(item.artifactKind, item.kind)}>
                              {changeKindLabel(item.kind)}
                            </small>
                            {item.context && (
                              <div
                                aria-label={`Contexto en vivo de ${item.path}`}
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
                              aria-label={`Acciones para ${item.path}`}
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
                                <span title={agentLensFileActionLabelTitle("Vista previa")}>
                                  Vista previa
                                </span>
                              </button>
                              <button
                                disabled={!repo}
                                onClick={() => onOpenFile(item.path)}
                                title={agentLensOpenActionTitle(item.path, Boolean(repo))}
                                type="button"
                              >
                                <span title={agentLensFileActionLabelTitle("Abrir")}>Abrir</span>
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
                                <span title={agentLensFileActionLabelTitle("Preguntar")}>
                                  Preguntar
                                </span>
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
                                      revertingFile === key ? "Revirtiendo" : "Revertir",
                                    )}
                                  >
                                    {revertingFile === key ? "Revirtiendo" : "Revertir"}
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
                    Ningún archivo coincide con este filtro.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearFileFilter}
                    title={agentLensNoFilesMatchClearTitle(fileQuery)}
                    type="button"
                  >
                    <span title={agentLensClearFileFilterLabelTitle()}>Borrar</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div
              className="agent-panel__empty-lens"
              title={agentLensNoTouchedFilesTitle(activeScope)}
            >
              Aún no hay archivos modificados.
            </div>
          )}
        </div>
      )}

      {activeTab === "commands" && (
        <div
          className="agent-panel__lens-view"
          aria-label="Vista de comandos de Agent Lens"
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
                <span title={agentLensEventFilterLabelTitle("commands")}>Filtrar comandos</span>
                <input
                  aria-describedby={agentLensEventFilterStatusId(session.id, "commands")}
                  aria-label="Filtrar la salida de comandos"
                  ref={commandFilterRef}
                  value={commandQuery}
                  onChange={(event) => setCommandQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasCommandQuery) {
                      event.preventDefault();
                      clearCommandFilter();
                    }
                  }}
                  placeholder="Texto del comando..."
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
                    <span title={agentLensClearEventFilterLabelTitle("commands")}>Borrar</span>
                  </button>
                )}
              </label>
              {filteredCommandItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Salida de comandos"
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
                        Comando del turno {item.turnIndex}
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
                    Ningún comando coincide con este filtro.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearCommandFilter}
                    title={agentLensNoEventsMatchClearTitle("commands", commandQuery)}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("commands")}>Borrar</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="agent-panel__empty-lens" title={agentLensNoCommandsTitle(activeScope)}>
              Aún no se han capturado comandos.
            </div>
          )}
        </div>
      )}

      {activeTab === "timeline" && (
        <div
          className="agent-panel__lens-view"
          aria-label="Vista de Timeline de Agent Lens"
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
                <span title={agentLensEventFilterLabelTitle("timeline")}>Filtrar Timeline</span>
                <input
                  aria-describedby={agentLensEventFilterStatusId(session.id, "timeline")}
                  aria-label="Filtrar eventos de Timeline"
                  ref={timelineFilterRef}
                  value={timelineQuery}
                  onChange={(event) => setTimelineQuery(event.currentTarget.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && hasTimelineQuery) {
                      event.preventDefault();
                      clearTimelineFilter();
                    }
                  }}
                  placeholder="Texto o tipo de evento..."
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
                    <span title={agentLensClearEventFilterLabelTitle("timeline")}>Borrar</span>
                  </button>
                )}
              </label>
              {filteredTimelineItems.length > 0 ? (
                <div
                  className="agent-panel__lens-list"
                  aria-label="Timeline reciente"
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
                        Turno {item.turnIndex} - {item.label}
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
                    Ningún evento de Timeline coincide con este filtro.
                  </span>
                  <button
                    className="agent-panel__empty-lens-action"
                    onClick={clearTimelineFilter}
                    title={agentLensNoEventsMatchClearTitle("timeline", timelineQuery)}
                    type="button"
                  >
                    <span title={agentLensClearEventFilterLabelTitle("timeline")}>Borrar</span>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="agent-panel__empty-lens" title={agentLensNoTimelineTitle(activeScope)}>
              Aún no se han capturado eventos de Timeline.
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
  if (label === "Archivos") {
    return `Mostrar la vista de archivos de Agent Lens con ${countLabel(count, "archivo modificado", "archivos modificados")}.`;
  }
  if (label === "Comandos") {
    return `Mostrar la vista de comandos de Agent Lens con ${countLabel(count, "salida", "salidas")}.`;
  }
  if (label === "Timeline") {
    return `Mostrar la vista de Timeline de Agent Lens con ${countLabel(count, "evento reciente", "eventos recientes")}.`;
  }
  return `Mostrar la vista ${label} de Agent Lens.`;
}

function agentLensTabLabelTitle(label: string): string {
  return `Nombre de la pestaña de Agent Lens: vista ${label}.`;
}

function agentLensTabCountTitle(label: string, count: number): string {
  if (label === "Archivos") {
    return `Cantidad de la pestaña Archivos de Agent Lens: ${countLabel(count, "archivo modificado", "archivos modificados")}.`;
  }
  if (label === "Comandos") {
    return `Cantidad de la pestaña Comandos de Agent Lens: ${countLabel(count, "salida de comando", "salidas de comandos")}.`;
  }
  if (label === "Timeline") {
    return `Cantidad de la pestaña Timeline de Agent Lens: ${countLabel(count, "evento", "eventos")}.`;
  }
  return `Cantidad de la pestaña ${label} de Agent Lens: ${count}.`;
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
  return `Inspector de Agent Lens: ${agentLensScopeSummary(
    activeScope,
    focusedTurnIndex,
    turnCount,
  )}; vista ${agentLensTabName(activeTab)} activa; ${countLabel(fileCount, "archivo", "archivos")}, ${countLabel(commandCount, "salida de comando", "salidas de comandos")}, ${countLabel(timelineCount, "evento de Timeline", "eventos de Timeline")}; estado del turno: ${turnStatusLabel(turnStatus)}.`;
}

function agentLensViewContainerTitle(
  tab: AgentLensTab,
  activeScope: AgentLensScope,
  totalCount: number,
  visibleCount: number,
  hasQuery: boolean,
): string {
  const scope = activeScope === "focused" ? "turno seleccionado" : "sesión actual";
  if (tab === "files") {
    if (hasQuery) {
      return `Vista Archivos de Agent Lens para el ${scope}: se muestran ${visibleCount} de ${countLabel(totalCount, "archivo modificado", "archivos modificados")}.`;
    }
    return `Vista Archivos de Agent Lens para el ${scope}: ${countLabel(totalCount, "archivo modificado", "archivos modificados")}.`;
  }
  if (tab === "commands") {
    if (hasQuery) {
      return `Vista Comandos de Agent Lens para el ${scope}: se muestran ${visibleCount} de ${countLabel(totalCount, "salida de comando", "salidas de comandos")}.`;
    }
    return `Vista Comandos de Agent Lens para el ${scope}: ${countLabel(totalCount, "salida de comando", "salidas de comandos")}.`;
  }
  if (hasQuery) {
    return `Vista Timeline de Agent Lens para el ${scope}: se muestran ${visibleCount} de ${countLabel(totalCount, "evento", "eventos")}.`;
  }
  return `Vista Timeline de Agent Lens para el ${scope}: ${countLabel(totalCount, "evento", "eventos")}.`;
}

function agentLensTabName(tab: AgentLensTab): string {
  switch (tab) {
    case "commands":
      return "Comandos";
    case "timeline":
      return "Timeline";
    case "files":
    default:
      return "Archivos";
  }
}

function agentLensScopeSummary(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused") {
    return focusedTurnIndex
      ? `turno seleccionado ${focusedTurnIndex}`
      : "ámbito seleccionado a la espera de un turno";
  }
  return `sesión actual con ${countLabel(turnCount, "turno", "turnos")}`;
}

function agentLensHeadingTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Inspector de Agent Lens para el turno seleccionado ${focusedTurnIndex}.`;
  }
  return `Inspector de Agent Lens para la sesión completa con ${countLabel(turnCount, "turno", "turnos")}.`;
}

function agentLensHeadingLabelTitle(): string {
  return "Etiqueta de cabecera de Agent Lens.";
}

function agentLensTabListTitle(
  fileCount: number,
  commandCount: number,
  timelineCount: number,
): string {
  return `Pestañas de vistas de Agent Lens: ${countLabel(fileCount, "archivo", "archivos")}, ${countLabel(commandCount, "salida de comando", "salidas de comandos")}, ${countLabel(timelineCount, "evento de Timeline", "eventos de Timeline")}.`;
}

function agentLensScopeTitle(scope: AgentLensScope, focusedTurnIndex: number | null): string {
  if (scope === "focused") {
    return focusedTurnIndex
      ? `Limitar Agent Lens al turno seleccionado ${focusedTurnIndex}.`
      : "Selecciona un turno para limitar Agent Lens a ese turno.";
  }
  return "Mostrar la sesión completa en Agent Lens.";
}

function agentLensScopeLabelTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
  turnCount: number,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Agent Lens muestra el turno seleccionado ${focusedTurnIndex}.`;
  }
  return `Agent Lens muestra la sesión completa con ${countLabel(turnCount, "turno", "turnos")}.`;
}

function agentLensScopeGroupTitle(
  activeScope: AgentLensScope,
  focusedTurnIndex: number | null,
): string {
  if (activeScope === "focused" && focusedTurnIndex) {
    return `Los controles de ámbito de Agent Lens están limitados al turno ${focusedTurnIndex}.`;
  }
  return "Los controles de ámbito de Agent Lens alternan entre el turno seleccionado y la sesión completa.";
}

function agentLensMetricsTitle(
  activeScope: AgentLensScope,
  fileCount: number,
  turnCheckpointCount: number,
  restorableCount: number,
  turnStatus: string,
): string {
  const checkpointLabel = turnCheckpointCount === 1 ? "punto de control" : "puntos de control";
  const scopeLabel = activeScope === "focused" ? "turno seleccionado" : "sesión actual";
  return `Las métricas de Agent Lens resumen el estado ${turnStatusLabel(turnStatus)}, ${countLabel(fileCount, "archivo", "archivos")} y ${restorableCount} ${checkpointLabel} restaurables para el ${scopeLabel}.`;
}

function agentLensTurnStateMetricTitle(turnStatus: string): string {
  return `Estado actual del turno en Agent Lens: ${turnStatusLabel(turnStatus)}.`;
}

function agentLensTurnStateValueTitle(turnStatus: string): string {
  return `Valor del estado del turno en Agent Lens: ${turnStatusLabel(turnStatus)}.`;
}

function agentLensFileMetricTitle(activeScope: AgentLensScope, count: number): string {
  if (activeScope === "focused") {
    return `El ámbito del turno seleccionado de Agent Lens incluye ${countLabel(count, "archivo", "archivos")}.`;
  }
  return `El ámbito de sesión de Agent Lens incluye ${countLabel(count, "archivo", "archivos")}.`;
}

function agentLensFileMetricValueTitle(activeScope: AgentLensScope, count: number): string {
  const scopeLabel = activeScope === "focused" ? "turno seleccionado" : "sesión";
  return `Valor de archivos del ámbito ${scopeLabel} de Agent Lens: ${countLabel(count, "archivo", "archivos")}.`;
}

function agentLensRestoreMetricTitle(
  turnCheckpointCount: number,
  restorableCount: number,
  latestRestorableTurnIndex: number | null,
): string {
  if (turnCheckpointCount === 0) {
    return "Métrica de puntos de restauración de Agent Lens: aún no hay puntos de control de turno completados.";
  }
  const checkpointLabel = turnCheckpointCount === 1 ? "punto de control" : "puntos de control";
  const restorableLabel =
    restorableCount === 1 ? "punto de restauración" : "puntos de restauración";
  const latest = latestRestorableTurnIndex
    ? ` El último turno restaurable es el ${latestRestorableTurnIndex}.`
    : "";
  return `Métrica de puntos de restauración de Agent Lens: ${restorableCount} ${restorableLabel} de ${turnCheckpointCount} ${checkpointLabel}.${latest}`;
}

function agentLensRestoreMetricValueTitle(
  turnCheckpointCount: number,
  restorableCount: number,
  latestRestorableTurnIndex: number | null,
): string {
  if (turnCheckpointCount === 0) {
    return "Valor de puntos de restauración de Agent Lens: 0 de 0 puntos de control son restaurables.";
  }
  const latest = latestRestorableTurnIndex
    ? ` Último turno restaurable: ${latestRestorableTurnIndex}.`
    : "";
  return `Valor de puntos de restauración de Agent Lens: ${restorableCount} de ${turnCheckpointCount} puntos de control son restaurables.${latest}`;
}

function agentLensFileFilterStatusId(sessionId: string): string {
  return `agent-lens-${domIdPart(sessionId)}-file-filter-status`;
}

function agentLensFileFilterTitle(count: number, hasQuery: boolean): string {
  const escapeHint = hasQuery ? " Pulsa Escape para borrar el filtro." : "";
  return `Filtrar ${countLabel(count, "archivo modificado", "archivos modificados")} de Agent Lens por ruta, tipo de cambio, estado o categoría.${escapeHint}`;
}

function agentLensFileFilterLabelTitle(): string {
  return "Etiqueta del filtro de archivos de Agent Lens.";
}

function agentLensFileFilterWrapperTitle(
  totalCount: number,
  visibleCount: number,
  hasQuery: boolean,
): string {
  if (hasQuery) {
    return `El filtro de archivos de Agent Lens muestra ${visibleCount} de ${countLabel(totalCount, "archivo modificado", "archivos modificados")}.`;
  }
  return `El filtro de archivos de Agent Lens controla ${countLabel(totalCount, "archivo modificado", "archivos modificados")}.`;
}

function agentLensFileFilterCountTitle(
  visibleCount: number,
  totalCount: number,
  hasQuery: boolean,
): string {
  if (hasQuery) {
    return `Se muestran ${visibleCount} de ${countLabel(totalCount, "archivo modificado", "archivos modificados")} de Agent Lens después de filtrar.`;
  }
  return `Se muestran los ${countLabel(totalCount, "archivo modificado", "archivos modificados")} de Agent Lens.`;
}

function agentLensClearFileFilterTitle(visibleCount: number, totalCount: number): string {
  return `Borrar el filtro y mostrar los ${totalCount} archivos modificados; ahora se muestran ${visibleCount}.`;
}

function agentLensClearFileFilterLabelTitle(): string {
  return "Etiqueta para borrar el filtro de archivos de Agent Lens: Borrar.";
}

function agentLensPreviewLabelTitle(): string {
  return "Área de vista previa del archivo activo en Agent Lens.";
}

function agentLensPreviewContainerTitle(
  path: string | null,
  position: number,
  count: number,
): string {
  const keyboardHint =
    count > 1 ? " Usa las flechas para recorrer los archivos de la vista previa." : "";
  return path
    ? `Vista previa del archivo seleccionado en Agent Lens: ${path}; elemento ${position} de ${count}.${keyboardHint}`
    : "La vista previa del archivo seleccionado en Agent Lens espera un archivo modificado.";
}

function agentLensPreviewSelectionTitle(path: string | null): string {
  return path
    ? `La vista previa de Agent Lens muestra ${path}.`
    : "Vista previa del archivo seleccionado en Agent Lens: no hay ningún archivo seleccionado.";
}

function agentLensPreviewPositionTitle(position: number, count: number): string {
  return `Posición de la vista previa de Agent Lens: ${position} de ${countLabel(count, "archivo visible", "archivos visibles")}.`;
}

function agentLensPreviewNavigationTitle(path: string | null, count: number): string {
  return path
    ? `Navegación de la vista previa de Agent Lens para ${path}: recorre ${countLabel(count, "archivo visible", "archivos visibles")}.`
    : `Navegación de la vista previa de Agent Lens: recorre ${countLabel(count, "archivo visible", "archivos visibles")}.`;
}

function agentLensPreviewNavButtonTitle(
  direction: "previous" | "next",
  path: string | null,
): string {
  const label = direction === "previous" ? "anterior" : "siguiente";
  return path
    ? `Mostrar el archivo ${label} en la vista previa de Agent Lens: ${path}.`
    : `Mostrar el archivo ${label} en la vista previa de Agent Lens.`;
}

function agentLensPreviewNavLabelTitle(label: string): string {
  return `Etiqueta de navegación de la vista previa de Agent Lens: ${label}.`;
}

function agentLensPreviewActionsTitle(path: string, canShowRevert: boolean): string {
  const controls = canShowRevert
    ? "controles para abrir, preguntar y revertir el archivo seleccionado"
    : "controles para abrir y preguntar por el archivo seleccionado";
  return `Acciones de la vista previa de Agent Lens para ${path}: ${controls}.`;
}

function agentLensPreviewActionLabelTitle(label: string): string {
  return `Etiqueta de acción de la vista previa de Agent Lens: ${label}.`;
}

function agentLensPreviewRevertActionTitle(
  path: string,
  turnIndex: number | null,
  canRevert: boolean,
  isReverting: boolean,
): string {
  return `Archivo seleccionado en la vista previa: ${agentLensRevertActionTitle(
    path,
    turnIndex,
    canRevert,
    isReverting,
  )}`;
}

function agentLensPreviewDetailGroupTitle(path: string): string {
  return `Detalles de la vista previa de Agent Lens para ${path}: resumen y ubicación del primer fragmento.`;
}

function agentLensNoLiveHunkTitle(path: string | null): string {
  return path
    ? `Vista previa del archivo seleccionado ${path}: no hay datos de fragmentos en vivo.`
    : "Vista previa del archivo seleccionado: no hay datos de fragmentos en vivo porque no se ha seleccionado ningún archivo.";
}

function agentLensNoFilesMatchTitle(query: string): string {
  return `Ningún archivo de Agent Lens coincide con "${query.trim()}". Borra o cambia el filtro para mostrar los archivos modificados.`;
}

function agentLensNoFilesMatchLabelTitle(query: string): string {
  return `Resultado vacío del filtro de archivos de Agent Lens para "${query.trim()}".`;
}

function agentLensNoFilesMatchClearTitle(query: string): string {
  return `Borrar el filtro de archivos "${query.trim()}" y volver a mostrar los archivos modificados.`;
}

function agentLensNoTouchedFilesTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens no tiene archivos modificados en el turno seleccionado.";
  }
  return "Agent Lens no tiene archivos modificados en la sesión actual.";
}

function agentLensTouchedFilesListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const scope = activeScope === "focused" ? "turno seleccionado" : "sesión actual";
  const filterPrefix = hasQuery ? "Archivos filtrados" : "Archivos modificados";
  return `${filterPrefix} de Agent Lens para el ${scope}: ${countLabel(count, "archivo", "archivos")}.`;
}

function agentLensCommandListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const scope = activeScope === "focused" ? "turno seleccionado" : "sesión actual";
  const filterPrefix = hasQuery ? "Salidas filtradas" : "Salidas";
  return `${filterPrefix} de comandos de Agent Lens para el ${scope}: ${countLabel(count, "salida", "salidas")}.`;
}

function agentLensTimelineListTitle(
  activeScope: AgentLensScope,
  count: number,
  hasQuery: boolean,
): string {
  const scope = activeScope === "focused" ? "turno seleccionado" : "sesión actual";
  const filterPrefix = hasQuery ? "Timeline reciente filtrado" : "Timeline reciente";
  return `${filterPrefix} de Agent Lens para el ${scope}: ${countLabel(count, "evento", "eventos")}.`;
}

function agentLensEventFilterStatusId(sessionId: string, kind: "commands" | "timeline"): string {
  return `agent-lens-${domIdPart(sessionId)}-${kind}-filter-status`;
}

function agentLensEventFilterTitle(
  kind: "commands" | "timeline",
  count: number,
  hasQuery: boolean,
): string {
  const noun = kind === "commands" ? "salidas de comandos" : "eventos de Timeline";
  const escapeHint = hasQuery ? " Pulsa Escape para borrar el filtro." : "";
  return `Filtrar ${count} ${noun} de Agent Lens por texto${kind === "timeline" ? " o tipo de evento" : ""}.${escapeHint}`;
}

function agentLensEventFilterLabelTitle(kind: "commands" | "timeline"): string {
  return kind === "commands"
    ? "Etiqueta del filtro de comandos de Agent Lens."
    : "Etiqueta del filtro de Timeline de Agent Lens.";
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
        ? "salida de comando"
        : "salidas de comandos"
      : totalCount === 1
        ? "evento de Timeline"
        : "eventos de Timeline";
  if (hasQuery) {
    return `El filtro de ${kind === "commands" ? "comandos" : "Timeline"} de Agent Lens muestra ${visibleCount} de ${totalCount} ${noun}.`;
  }
  return `El filtro de ${kind === "commands" ? "comandos" : "Timeline"} de Agent Lens controla ${totalCount} ${noun}.`;
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
        ? "salida de comando"
        : "salidas de comandos"
      : totalCount === 1
        ? "evento de Timeline"
        : "eventos de Timeline";
  if (hasQuery) {
    return `Se muestran ${visibleCount} de ${totalCount} ${noun} de Agent Lens después de filtrar.`;
  }
  return `Se muestran los ${totalCount} ${noun} de Agent Lens.`;
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
        ? "comando"
        : "comandos"
      : totalCount === 1
        ? "evento"
        : "eventos";
  return hasQuery ? `${visibleCount} de ${totalCount} ${noun}` : `${totalCount} ${noun}`;
}

function agentLensClearEventFilterTitle(
  kind: "commands" | "timeline",
  visibleCount: number,
  totalCount: number,
): string {
  const filterName = kind === "commands" ? "comandos" : "Timeline";
  return `Borrar el filtro de ${filterName} y mostrar los ${totalCount} elementos; ahora se muestran ${visibleCount}.`;
}

function agentLensClearEventFilterLabelTitle(kind: "commands" | "timeline"): string {
  return kind === "commands"
    ? "Etiqueta para borrar el filtro de comandos de Agent Lens: Borrar."
    : "Etiqueta para borrar el filtro de Timeline de Agent Lens: Borrar.";
}

function agentLensNoEventsMatchTitle(kind: "commands" | "timeline", query: string): string {
  const noun = kind === "commands" ? "comando" : "evento de Timeline";
  return `Ningún ${noun} de Agent Lens coincide con "${query.trim()}". Borra o cambia el filtro para mostrar los elementos registrados.`;
}

function agentLensNoEventsMatchLabelTitle(kind: "commands" | "timeline", query: string): string {
  const noun = kind === "commands" ? "salidas de comandos" : "eventos de Timeline";
  return `Resultado vacío del filtro de ${noun} de Agent Lens para "${query.trim()}".`;
}

function agentLensNoEventsMatchClearTitle(kind: "commands" | "timeline", query: string): string {
  const filterName = kind === "commands" ? "comandos" : "Timeline";
  return `Borrar el filtro de ${filterName} "${query.trim()}" y volver a mostrar los elementos capturados.`;
}

function agentLensLiveContextTitle(path: string): string {
  return `Contexto en vivo de Agent Lens para ${path}: estado del repositorio e indicadores del diff.`;
}

function agentLensCommandEventTitle(item: {
  turnIndex: number;
  timeLabel: string | null;
  text: string;
}): string {
  const timing = item.timeLabel ? ` en ${item.timeLabel}` : "";
  return `Salida de comando registrada en Agent Lens para el turno ${item.turnIndex}${timing}: ${item.text}`;
}

function agentLensCommandEventMetaTitle(item: {
  turnIndex: number;
  timeLabel: string | null;
}): string {
  const timing = item.timeLabel ? ` en ${item.timeLabel}` : "";
  return `Metadatos del evento de comando de Agent Lens: comando del turno ${item.turnIndex}${timing}.`;
}

function agentLensCommandEventTextTitle(item: { turnIndex: number; text: string }): string {
  return `Salida de comando registrada en Agent Lens para el turno ${item.turnIndex}: ${item.text}`;
}

function agentLensTimelineEventTitle(item: {
  turnIndex: number;
  label: string;
  timeLabel: string | null;
  text: string;
}): string {
  const timing = item.timeLabel ? ` en ${item.timeLabel}` : "";
  return `Evento ${item.label.toLowerCase()} de Timeline registrado en Agent Lens para el turno ${item.turnIndex}${timing}: ${item.text}`;
}

function agentLensTimelineEventMetaTitle(item: {
  turnIndex: number;
  label: string;
  timeLabel: string | null;
}): string {
  const timing = item.timeLabel ? ` en ${item.timeLabel}` : "";
  return `Metadatos del evento de Timeline de Agent Lens: evento ${item.label} del turno ${item.turnIndex}${timing}.`;
}

function agentLensTimelineEventTextTitle(item: {
  turnIndex: number;
  label: string;
  text: string;
}): string {
  return `Texto de Timeline registrado en Agent Lens para el evento ${item.label} del turno ${item.turnIndex}: ${item.text}`;
}

function agentLensNoCommandsTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens no tiene salidas de comandos en el turno seleccionado.";
  }
  return "Agent Lens no tiene salidas de comandos en la sesión actual.";
}

function agentLensNoTimelineTitle(activeScope: AgentLensScope): string {
  if (activeScope === "focused") {
    return "Agent Lens no tiene eventos de Timeline en el turno seleccionado.";
  }
  return "Agent Lens no tiene eventos de Timeline en la sesión actual.";
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

const AGENT_LENS_ARTIFACT_ORDER = [
  "Código",
  "Pruebas",
  "Documentación",
  "Configuración",
  "Otros",
] as const;

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
    return "Pruebas";
  }
  if (
    normalized.startsWith("docs/") ||
    normalized.endsWith(".md") ||
    normalized.endsWith(".mdx") ||
    normalized.endsWith(".rst")
  ) {
    return "Documentación";
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
    return "Configuración";
  }
  if (
    /\.(ts|tsx|js|jsx|rs|py|go|java|kt|swift|cs|c|cc|cpp|h|hpp|css|scss|html|svelte|vue)$/.test(
      basename,
    )
  ) {
    return "Código";
  }
  return "Otros";
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
    : { label: "Sin diff en vivo", title: `No hay ningún diff en vivo disponible para ${path}.` };
  const renameLabel = diff?.old_path ? `desde ${diff.old_path}` : null;
  return {
    statusChips,
    diffLabel: diffSummary.label,
    diffTitle: diffSummary.title,
    renameLabel,
    renameTitle: diff?.old_path
      ? `Origen del renombrado en el diff en vivo de ${path}: ${diff.old_path}.`
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
  const translated = fileStatusLabel(label);
  return {
    label: translated,
    title: `Estado en vivo del repositorio para ${path}: ${translated}.`,
  };
}

function fileStatusLabel(label: string): string {
  switch (label) {
    case "staged":
      return "preparado";
    case "modified":
      return "modificado";
    case "untracked":
      return "sin seguimiento";
    default:
      return label;
  }
}

function changeKindLabel(kind: string): string {
  switch (kind.toLocaleLowerCase()) {
    case "added":
      return "añadido";
    case "created":
      return "creado";
    case "modified":
      return "modificado";
    case "removed":
    case "deleted":
      return "eliminado";
    case "renamed":
      return "renombrado";
    default:
      return kind;
  }
}

function artifactKindLabel(kind: string): string {
  return kind;
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
    return {
      label: "Diff binario",
      title: `Resumen del diff en vivo de ${path}: archivo binario.`,
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
  return {
    label: `+${totals.added} / -${totals.removed}`,
    title: `Resumen del diff en vivo de ${path}: ${totals.added} añadidas y ${totals.removed} eliminadas.`,
  };
}

function fileDiffPreview(diff: FileDiff): AgentLensFilePreview {
  if (diff.is_binary) {
    return {
      summary: "Diff binario",
      summaryTitle: `Resumen de la vista previa de ${diff.path}: diff binario.`,
      detail: "Archivo binario; no se puede mostrar una vista previa de sus fragmentos.",
      detailTitle: `Detalle de la vista previa de ${diff.path}: archivo binario sin vista previa de fragmentos.`,
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
  const hunkLabel = diff.hunks.length === 1 ? "1 fragmento" : `${diff.hunks.length} fragmentos`;
  const rangeLabel = firstHunk
    ? `Primer fragmento @@ -${firstHunk.old_start} +${firstHunk.new_start}`
    : "Sin fragmentos";
  return {
    summary: `${hunkLabel} - +${totals.added} / -${totals.removed}`,
    summaryTitle: `Resumen de la vista previa del archivo seleccionado ${diff.path}: ${hunkLabel}, ${totals.added} añadidas, ${totals.removed} eliminadas.`,
    detail: diff.old_path ? `${rangeLabel}; renombrado desde ${diff.old_path}.` : `${rangeLabel}.`,
    detailTitle: diff.old_path
      ? `Detalle de la vista previa del archivo seleccionado ${diff.path}: ${rangeLabel}; renombrado desde ${diff.old_path}.`
      : `Detalle de la vista previa del archivo seleccionado ${diff.path}: ${rangeLabel}.`,
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
          label: "Tú",
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
          label: "Comando",
          timeLabel,
          text: compactActivityText(text),
        });
      });
      turn.systemText.forEach((text, index) => {
        items.push({
          id: `${turn.id}:system:${index}`,
          turnIndex: turn.index,
          label: "Sistema",
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
  const parts = [
    `Turno ${turn.index}: ${countLabel(turn.commands, "comando", "comandos")}, ${countLabel(turn.files, "archivo", "archivos")}`,
  ];
  if (turn.commandSummary) parts.push(`Comando reciente: ${turn.commandSummary}`);
  return parts.join(" - ");
}

function turnMapIndexTitle(turnIndex: number): string {
  return `Etiqueta del mapa de turnos del resumen de la sesión de Agent: turno ${turnIndex}.`;
}

function turnMapTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Tiempo del mapa de turnos del resumen de la sesión de Agent para el turno ${turnIndex}: ${timeLabel}.`;
}

function turnMapCommandCountTitle(turnIndex: number, commands: number): string {
  return `Cantidad de comandos del mapa de turnos del resumen de la sesión de Agent para el turno ${turnIndex}: ${overviewMetricCount(
    "Comandos",
    commands,
  )}.`;
}

function turnMapCommandSummaryTitle(turnIndex: number, commandSummary: string): string {
  return `Resumen de comandos del mapa de turnos de la sesión de Agent para el turno ${turnIndex}: ${commandSummary}.`;
}

function turnMapFileCountTitle(turnIndex: number, files: number): string {
  return `Cantidad de archivos del mapa de turnos del resumen de la sesión de Agent para el turno ${turnIndex}: ${overviewMetricCount(
    "Archivos",
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
      title: "Cargando la sesión de Agent",
      detail: "Conectando la vista del espacio de trabajo con la ejecución de Agent.",
      checkpoint: "Punto de control desconocido",
      throughput: "Sin transmisión aún",
      tone: "idle",
    };
  }
  const turnState = turnStatusLabel(session.turn_status ?? "waiting");
  const status = sessionStatusLabel(session.status);
  const changeCount = session.change_log?.length ?? 0;
  const checkpoint = session.checkpoint
    ? checkpointLabel(session.checkpoint.checkpoint_type)
    : "Sin punto de control";
  const throughput =
    session.output_bytes_per_second != null
      ? `${Math.round(session.output_bytes_per_second)} B/s`
      : "Transmisión en reposo";
  if (readOnly) {
    return {
      title: "Transcripción archivada",
      detail: `${countLabel(changeCount, "cambio registrado", "cambios registrados")}. La sesión es de solo lectura.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.status === "failed" || session.status === "error") {
    return {
      title: "Requiere atención",
      detail: session.error
        ? commandMessage(session.error)
        : `${status}. Revisa la transcripción y la salida de comandos reciente.`,
      checkpoint,
      throughput,
      tone: "failed",
    };
  }
  if (session.status === "completed" || session.status === "reverted") {
    return {
      title: session.status === "reverted" ? "Cambios revertidos" : "Sesión completada",
      detail: `${countLabel(changeCount, "cambio registrado", "cambios registrados")}. Estado del turno: ${turnState}.`,
      checkpoint,
      throughput,
      tone: "done",
    };
  }
  if (session.turn_status === "working" || session.status === "running") {
    return {
      title: "Agent está trabajando",
      detail: `${turnState}. ${countLabel(changeCount, "cambio registrado", "cambios registrados")} hasta ahora.`,
      checkpoint,
      throughput,
      tone: "working",
    };
  }
  return {
    title: "Listo para el siguiente turno",
    detail: `${turnState}. ${countLabel(changeCount, "cambio registrado", "cambios registrados")} hasta ahora.`,
    checkpoint,
    throughput,
    tone: "idle",
  };
}

function agentActivityFactsTitle(): string {
  return "Datos de actividad de Agent: turnos, archivos, punto de control y velocidad de transmisión.";
}

function agentActivityStripTitle(
  activity: ReturnType<typeof agentActivitySummary>,
  overview: AgentSessionOverviewView,
): string {
  return `Franja de actividad de Agent: ${activity.title}; ${activity.detail}; ${overviewMetricCount(
    "Turnos",
    overview.turns,
  )}, ${overviewMetricCount("Archivos", overview.files)}.`;
}

function agentActivityMainTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Estado principal de actividad de Agent: ${activity.title}.`;
}

function agentActivityDotTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  const tone = {
    idle: "en reposo",
    working: "trabajando",
    done: "completado",
    failed: "fallido",
  }[activity.tone];
  return `Pulso de actividad de Agent: estado ${tone}.`;
}

function agentActivityTextGroupTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Texto de estado de actividad de Agent: ${activity.title}; ${activity.detail}`;
}

function agentActivityTitleLabelTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Titular de actividad de Agent: ${activity.title}.`;
}

function agentActivityDetailTitle(activity: ReturnType<typeof agentActivitySummary>): string {
  return `Detalle de actividad de Agent: ${activity.detail}`;
}

function agentActivityTurnsFactTitle(turns: number): string {
  return `Cantidad de turnos de la actividad de Agent: ${countLabel(turns, "turno", "turnos")}.`;
}

function agentActivityFilesFactTitle(files: number): string {
  return `Cantidad de archivos modificados por Agent: ${countLabel(files, "archivo", "archivos")}.`;
}

function agentActivityCheckpointFactTitle(checkpoint: string): string {
  return `Punto de control de la actividad de Agent: ${checkpoint}.`;
}

function agentActivityThroughputFactTitle(throughput: string): string {
  return `Velocidad de transmisión de la actividad de Agent: ${throughput}.`;
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
    throw new Error("El portapapeles no está disponible en esta ventana.");
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
  const parts: string[] = [`Turno ${turn.index}${timeLabel ? ` (${timeLabel})` : ""}`];
  if (artifactSummary) parts.push(`Artefactos: ${artifactSummary}.`);
  if (turn.userText) parts.push(`Tú:\n${turn.userText}`);
  turn.agentText.forEach((text) => parts.push(`Agent:\n${text}`));
  turn.commandText.forEach((text) => parts.push(`Comando:\n${text}`));
  turn.systemText.forEach((text) => parts.push(`Sistema:\n${text}`));
  if (turn.changes.length > 0) {
    parts.push(
      `Archivos:\n${turn.changes.map((change) => `- ${changeKindLabel(change.kind)} ${change.path}`).join("\n")}`,
    );
  }
  return parts.join("\n\n");
}

function focusedTurnHeadingLabelTitle(): string {
  return "Etiqueta de cabecera del turno seleccionado: Turno seleccionado.";
}

function focusedTurnIdleContainerTitle(): string {
  return "Contenedor del turno seleccionado: no hay ningún turno seleccionado.";
}

function focusedTurnSelectedContainerTitle(turnIndex: number): string {
  return `Contenedor del turno seleccionado: turno ${turnIndex}.`;
}

function focusedTurnIndexLabelTitle(turnIndex: number): string {
  return `Etiqueta de índice del turno seleccionado: Turno ${turnIndex}.`;
}

function focusedTurnIdleStatusLabelTitle(): string {
  return "Etiqueta de estado del turno seleccionado: Inactivo.";
}

function focusedTurnEmptyStateLabelTitle(): string {
  return "Etiqueta del estado vacío del turno seleccionado: Ningún turno seleccionado.";
}

function focusedTurnFallbackTextTitle(turnIndex: number): string {
  return `Texto alternativo del turno seleccionado: no se registró texto para el turno ${turnIndex}.`;
}

function focusedTurnHiddenFileOverflowTitle(turnIndex: number, count: number): string {
  return `Archivos adicionales ocultos del turno seleccionado: ${countLabel(count, "archivo modificado", "archivos modificados")} en el turno ${turnIndex}.`;
}

function focusedTurnFileRowTitle(
  turnIndex: number,
  change: { kind: string; path: string },
): string {
  return `Fila de archivo modificado del turno seleccionado ${turnIndex}: ${changeKindLabel(change.kind)} ${change.path}.`;
}

function focusedTurnFilesContainerTitle(
  turnIndex: number,
  visibleCount: number,
  hiddenCount: number,
): string {
  const hiddenText =
    hiddenCount > 0
      ? `, además de ${countLabel(hiddenCount, "archivo modificado oculto", "archivos modificados ocultos")}`
      : "";
  return `Contenedor de archivos del turno seleccionado ${turnIndex}: ${countLabel(visibleCount, "archivo modificado visible", "archivos modificados visibles")}${hiddenText}.`;
}

function focusedTurnRestoreContainerTitle(
  turnIndex: number,
  canRestore: boolean,
  restoreReady: boolean,
): string {
  if (!canRestore) {
    return `Contenedor de restauración del turno seleccionado ${turnIndex}: detén la sesión antes de restaurar.`;
  }
  return restoreReady
    ? `Contenedor de restauración del turno seleccionado ${turnIndex}: restaura los archivos y la conversación a este turno.`
    : `Contenedor de restauración del turno seleccionado ${turnIndex}: no hay ningún punto de restauración completado.`;
}

function focusedTurnRestoreButtonTitle(
  turnIndex: number,
  canRestore: boolean,
  restoreReady: boolean,
  restoring: boolean,
): string {
  if (restoring) return `Restaurando los archivos y la conversación al turno ${turnIndex}.`;
  if (!canRestore) return `Restaurar el turno ${turnIndex}: detén la sesión antes.`;
  if (!restoreReady) {
    return `No se puede restaurar el turno ${turnIndex} porque no tiene un punto de control completado.`;
  }
  return `Restaurar los archivos y la conversación al turno ${turnIndex}.`;
}

function focusedTurnRestoreLabelTitle(restoring: boolean): string {
  return restoring ? "Restaurando el turno seleccionado." : "Restaurar en este turno.";
}

function focusedTurnTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Tiempo del turno seleccionado ${turnIndex} respecto al primero: ${timeLabel}.`;
}

function turnTimeTitle(turnIndex: number, timeLabel: string): string {
  return `Tiempo del turno ${turnIndex} respecto al primero: ${timeLabel}.`;
}

function focusedTurnFactTitle(
  turnIndex: number,
  kind: "commands" | "files",
  count: number,
): string {
  const label =
    kind === "commands"
      ? countLabel(count, "comando", "comandos")
      : countLabel(count, "archivo", "archivos");
  return `El turno seleccionado ${turnIndex} tiene ${label}.`;
}

function focusedTurnFactsContainerTitle(
  turnIndex: number,
  commandCount: number,
  fileCount: number,
): string {
  return `Datos del turno seleccionado ${turnIndex}: ${countLabel(commandCount, "comando", "comandos")}, ${countLabel(fileCount, "archivo", "archivos")}.`;
}

function focusedTurnCommandSummaryContainerTitle(turnIndex: number): string {
  return `Contenedor del resumen de comandos del turno seleccionado ${turnIndex}: 1 resumen de comando reciente.`;
}

function focusedTurnArtifactSummaryContainerTitle(
  turnIndex: number,
  categoryCount: number,
): string {
  return `Contenedor del resumen de artefactos del turno seleccionado ${turnIndex}: ${countLabel(categoryCount, "categoría", "categorías")}.`;
}

function focusedTurnLatestActivityTitle(turnIndex: number, latest: string): string {
  return `Actividad registrada más reciente del turno seleccionado ${turnIndex}: ${compactActivityText(latest)}`;
}

function focusedTurnSummaryTitle(turn: AgentTurnView): string {
  return `Cantidad de mensajes, comandos y archivos del turno seleccionado ${turn.index}: ${turnSummaryLabel(turn)}.`;
}

function turnSummaryTitle(turn: AgentTurnView): string {
  return `Cantidad de mensajes, comandos y archivos del turno ${turn.index}: ${turnSummaryLabel(turn)}.`;
}

function conversationTurnContainerTitle(turn: AgentTurnView, focused: boolean): string {
  const focusState = focused ? "seleccionado" : "no seleccionado";
  return `Contenedor del turno ${turn.index} de la conversación: ${focusState}; ${turnSummaryLabel(turn)}.`;
}

function conversationTurnHeaderContainerTitle(turnIndex: number): string {
  return `Cabecera del turno ${turnIndex} de la conversación: título, metadatos y control para copiar.`;
}

function conversationTurnTitleContainerTitle(turnIndex: number): string {
  return `Título del turno ${turnIndex} de la conversación: etiqueta del turno y resumen de la transcripción.`;
}

function conversationTurnMetadataContainerTitle(turnIndex: number): string {
  return `Metadatos del turno ${turnIndex} de la conversación: tiempo, cantidad de archivos modificados y control para copiar.`;
}

function turnIndexLabelTitle(turnIndex: number): string {
  return `Etiqueta de índice del turno de la conversación: Turno ${turnIndex}.`;
}

function turnTouchedFilesTitle(turnIndex: number, count: number): string {
  return `El turno ${turnIndex} modificó ${countLabel(count, "archivo", "archivos")}.`;
}

function conversationTurnTouchedFilesContainerTitle(turnIndex: number, count: number): string {
  return `Archivos modificados del turno ${turnIndex} de la conversación: ${countLabel(count, "indicador", "indicadores")}.`;
}

function turnTouchedFileTitle(turnIndex: number, kind: string, path: string): string {
  return `Archivo modificado en el turno ${turnIndex}: ${changeKindLabel(kind)} ${path}.`;
}

function agentLensFileTitle(turnIndex: number | null, kind: string, path: string): string {
  const scope = turnIndex ? `el turno ${turnIndex}` : "la sesión";
  return `Archivo modificado de Agent Lens para ${scope}: ${changeKindLabel(kind)} ${path}.`;
}

function agentLensFileScopeMetaTitle(turnIndex: number | null, timeLabel: string | null): string {
  const timing = timeLabel ? ` en ${timeLabel}` : "";
  if (turnIndex) {
    return `Tiempo de la fila de archivo de Agent Lens: turno ${turnIndex}${timing}.`;
  }
  return `Ámbito de la fila de archivo de Agent Lens: registro de cambios de la sesión${timing}.`;
}

function agentLensFilePathMetaTitle(path: string): string {
  return `Ruta de la fila de archivo de Agent Lens: ${path}.`;
}

function agentLensFileKindMetaTitle(kind: AgentLensArtifactKind, changeKind: string): string {
  return `Tipo de cambio de la fila de archivo ${kind} de Agent Lens: ${changeKindLabel(changeKind)}.`;
}

function agentLensFileActionsTitle(path: string, canShowRevert: boolean): string {
  const controls = canShowRevert
    ? "vista previa, abrir, preguntar y revertir"
    : "vista previa, abrir y preguntar";
  return `Acciones de Agent Lens para ${path}: ${controls}.`;
}

function agentLensFileActionLabelTitle(label: string): string {
  return `Etiqueta de acción de archivo de Agent Lens: ${label}.`;
}

function agentLensPreviewActionTitle(path: string, isSelected: boolean): string {
  return isSelected
    ? `Se muestran los detalles de ${path} en Agent Lens.`
    : `Mostrar los detalles de ${path} en Agent Lens.`;
}

function agentLensOpenActionTitle(path: string, hasRepo: boolean): string {
  return hasRepo
    ? `Abrir ${path} desde Agent Lens en el espacio de trabajo.`
    : `No se puede abrir ${path} porque el repositorio de la sesión no está disponible.`;
}

function agentLensAskActionTitle(path: string, canPrompt: boolean): string {
  return canPrompt
    ? `Preparar un prompt de seguimiento de Agent Lens para ${path}.`
    : `No se puede preparar un prompt para ${path} porque la sesión está archivada o inactiva.`;
}

function agentLensRevertActionTitle(
  path: string,
  turnIndex: number | null,
  canRevert: boolean,
  isReverting: boolean,
): string {
  const scope = turnIndex ? `el turno ${turnIndex}` : "este turno";
  if (isReverting) return `Revirtiendo ${path} desde ${scope}.`;
  return canRevert
    ? `Revertir ${path} al punto de control de ${scope}.`
    : `Detén la sesión antes de revertir ${path}.`;
}

function agentLensFileGroupCountTitle(kind: AgentLensArtifactKind, count: number): string {
  return `El grupo de artefactos ${kind} contiene ${countLabel(count, "archivo", "archivos")}.`;
}

function agentLensFileGroupTitle(kind: AgentLensArtifactKind, count: number): string {
  return `El grupo de archivos ${kind} de Agent Lens contiene ${countLabel(count, "archivo modificado", "archivos modificados")}.`;
}

function agentLensFileGroupHeaderTitle(kind: AgentLensArtifactKind, count: number): string {
  return `Cabecera del grupo ${kind} de Agent Lens para ${countLabel(count, "archivo modificado", "archivos modificados")}.`;
}

function agentLensFileGroupKindLabelTitle(kind: AgentLensArtifactKind): string {
  return `Etiqueta del tipo de grupo de archivos de Agent Lens: ${kind}.`;
}

function turnCommandSummaryTitle(turnIndex: number, commandSummary: string): string {
  return `Resumen compacto de la salida de comandos reciente del turno ${turnIndex}: ${commandSummary}`;
}

function turnCommandSummaryContainerTitle(turnIndex: number): string {
  return `Resumen de comandos del turno ${turnIndex} de la conversación: 1 resumen de comando reciente.`;
}

function turnArtifactSummaryContainerTitle(turnIndex: number, categoryCount: number): string {
  return `Resumen de artefactos del turno ${turnIndex} de la conversación: ${countLabel(categoryCount, "categoría", "categorías")}.`;
}

function turnArtifactSummaryChipTitle(
  turnIndex: number,
  item: { kind: AgentLensArtifactKind; count: number },
): string {
  return `Artefactos ${item.kind} modificados en el turno ${turnIndex}: ${countLabel(item.count, "archivo", "archivos")}.`;
}

function fileActionPrompt({
  path,
  kind,
  turnIndex,
  artifactKind,
  hunkSummary,
}: AgentLensFilePromptContext): string {
  const scope = turnIndex ? `turno ${turnIndex}` : "esta sesión";
  return [
    `Céntrate en ${path}.`,
    `Este archivo figura como ${changeKindLabel(kind)} en ${scope}.`,
    `Categoría del artefacto: ${artifactKind}.`,
    ...(hunkSummary ? [`Resumen del diff: ${hunkSummary}.`] : []),
    "Revisa el contexto relevante del archivo, explica qué necesita atención todavía y propón el siguiente cambio o paso de verificación concreto.",
  ].join("\n");
}

function turnSummaryLabel(turn: AgentTurnView): string {
  const messages =
    (turn.userText ? 1 : 0) +
    turn.agentText.length +
    turn.systemText.length +
    turn.commandText.length;
  const parts = [countLabel(messages, "mensaje", "mensajes")];
  if (turn.commandText.length > 0) {
    parts.push(countLabel(turn.commandText.length, "comando", "comandos"));
  }
  if (turn.changes.length > 0) {
    parts.push(countLabel(turn.changes.length, "archivo", "archivos"));
  }
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
  return count === 1 ? "turno" : "turnos";
}

function transcriptSearchNavigationTitle(
  direction: "previous" | "next",
  hasQuery: boolean,
  matchCount: number,
): string {
  const label = direction === "previous" ? "anterior" : "siguiente";
  if (!hasQuery) return `Busca en la transcripción para navegar al resultado ${label}.`;
  if (matchCount < 2)
    return `Se necesitan al menos dos turnos coincidentes para ir al resultado ${label}.`;
  return `Ir al resultado ${label}`;
}

function transcriptSearchNavigationLabelTitle(direction: "previous" | "next"): string {
  return `Etiqueta de navegación de la búsqueda en la transcripción: resultado ${direction === "previous" ? "anterior" : "siguiente"}.`;
}

function transcriptCopyButtonTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) return "No hay turnos visibles que copiar.";
  if (hasQuery) {
    return `Copiar ${visibleCount} ${turnNoun(visibleCount)} filtrados de ${totalCount} en total.`;
  }
  return `Copiar los ${totalCount} ${turnNoun(totalCount)} de la transcripción.`;
}

function latestTurnButtonTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) return "No hay turnos visibles a los que ir.";
  if (hasQuery) {
    return `Ir al último turno filtrado de ${totalCount} ${turnNoun(totalCount)} en total.`;
  }
  return `Ir al último de los ${totalCount} ${turnNoun(totalCount)} de la transcripción.`;
}

function transcriptSecondaryActionLabelTitle(
  label: "Último" | "Copiar lo visible" | "Copiado",
): string {
  return `Acción secundaria de la transcripción: ${label}.`;
}

function transcriptClearSearchLabelTitle(): string {
  return "Borrar la búsqueda de la transcripción.";
}

function transcriptToolsContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    if (visibleCount === 0) {
      return `Herramientas de la transcripción de Agent: búsqueda activa sin turnos coincidentes entre ${totalCount} ${turnNoun(totalCount)} en total.`;
    }
    return `Herramientas de la transcripción de Agent: búsqueda activa con ${visibleCount} ${turnNoun(visibleCount)} coincidentes entre ${totalCount} en total.`;
  }
  if (totalCount === 0) {
    return "Herramientas de la transcripción de Agent: búsqueda, navegación por resultados y controles para ir al último turno y copiar, a la espera de turnos.";
  }
  return `Herramientas de la transcripción de Agent: búsqueda, navegación por resultados y controles para ir al último turno y copiar los ${totalCount} ${turnNoun(totalCount)}.`;
}

function transcriptSecondaryActionsTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (visibleCount === 0) {
    if (hasQuery) {
      return `Acciones secundarias de la transcripción: no hay turnos coincidentes a los que ir o que copiar entre ${totalCount} ${turnNoun(totalCount)} en total.`;
    }
    return "Acciones secundarias de la transcripción: controles para ir al último turno y copiar lo visible, a la espera de turnos.";
  }
  if (hasQuery) {
    return `Acciones secundarias de la transcripción: ir al último turno filtrado y copiar ${visibleCount} ${turnNoun(visibleCount)} filtrados de ${totalCount} en total.`;
  }
  return `Acciones secundarias de la transcripción: ir al último turno y copiar los ${totalCount} ${turnNoun(totalCount)}.`;
}

function transcriptSearchContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    if (visibleCount === 0) {
      return `Búsqueda en la transcripción: ningún turno coincide entre ${totalCount} ${turnNoun(totalCount)} en total.`;
    }
    return `Búsqueda en la transcripción: ${visibleCount} ${turnNoun(visibleCount)} coincidentes entre ${totalCount} en total.`;
  }
  return `Búsqueda en la transcripción: encuentra mensajes, comandos y archivos entre ${totalCount} ${turnNoun(totalCount)}.`;
}

function transcriptSearchLabelTitle(): string {
  return "Buscar en la transcripción.";
}

function transcriptSearchInputTitle(): string {
  return "Busca mensajes, comandos o archivos. Pulsa Escape para borrar la búsqueda.";
}

function transcriptSearchCountTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
): string {
  if (hasQuery) {
    return `Cantidad de resultados de la búsqueda en la transcripción: ${visibleCount} ${turnNoun(visibleCount)} coincidentes entre ${totalCount} en total.`;
  }
  return `Cantidad de resultados de la búsqueda en la transcripción: se muestran los ${totalCount} ${turnNoun(totalCount)}.`;
}

function activeSearchResultPositionTitle(activeIndex: number, matchCount: number): string {
  if (activeIndex >= 0) {
    return `Posición activa de la búsqueda en la transcripción: resultado ${activeIndex + 1} de ${matchCount} ${turnNoun(matchCount)} coincidentes.`;
  }
  return `Posición activa de la búsqueda en la transcripción: ningún resultado seleccionado entre ${matchCount} ${turnNoun(matchCount)} coincidentes.`;
}

function conversationContainerTitle(
  hasQuery: boolean,
  visibleCount: number,
  totalCount: number,
  readOnly: boolean,
): string {
  if (visibleCount === 0) {
    if (hasQuery) {
      return `Transcripción de la conversación con Agent: ningún turno coincide entre ${totalCount} ${turnNoun(totalCount)} en total.`;
    }
    return readOnly
      ? "Transcripción de la conversación con Agent: no hay turnos guardados."
      : "Transcripción de la conversación con Agent: lista para el primer turno.";
  }
  if (hasQuery) {
    return `Transcripción de la conversación con Agent: ${visibleCount} ${turnNoun(visibleCount)} coincidentes entre ${totalCount} en total.`;
  }
  return `Transcripción de la conversación con Agent: se muestran los ${totalCount} ${turnNoun(totalCount)}.`;
}

function turnCopyButtonTitle(turnIndex: number, copied: boolean): string {
  return copied
    ? `Transcripción completa del turno ${turnIndex} copiada al portapapeles.`
    : `Copiar la transcripción completa del turno ${turnIndex}, incluidos mensajes, comandos y archivos modificados.`;
}

function turnCopyLabelTitle(label: "Copiar turno" | "Copiado"): string {
  return `Acción para copiar el turno de la conversación: ${label}.`;
}

function messageCopyButtonTitle(turnIndex: number, label: string, copied: boolean): string {
  return copied
    ? `Mensaje de ${label} del turno ${turnIndex} copiado al portapapeles.`
    : `Copiar el mensaje de ${label} del turno ${turnIndex}.`;
}

function messageCopyLabelTitle(label: "Copiar" | "Copiado"): string {
  return `Acción para copiar el mensaje: ${label}.`;
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
      label: "Coincidencia en mensaje",
      title: "La búsqueda coincide con el texto de un mensaje de este turno",
      text: agentTurnMessageSearchText(turn),
    },
    {
      key: "command",
      label: "Coincidencia en comando",
      title: "La búsqueda coincide con la salida de un comando o su resumen",
      text: agentTurnCommandSearchText(turn),
    },
    {
      key: "file",
      label: "Coincidencia en archivo",
      title: "La búsqueda coincide con un archivo modificado, su estado o su categoría",
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
    return String((error as { message?: unknown }).message ?? "Falló el comando de Agent.");
  }
  return String(error || "Falló el comando de Agent.");
}

function turnStatusLabel(status: string): string {
  switch (status) {
    case "working":
      return "Trabajando";
    case "settling":
      return "Finalizando";
    default:
      return "En espera";
  }
}

function sessionStatusLabel(status: string): string {
  switch (status) {
    case "starting":
      return "Iniciando";
    case "running":
      return "En ejecución";
    case "completed":
      return "Completada";
    case "failed":
      return "Fallida";
    case "reverted":
      return "Revertida";
    case "error":
      return "Error";
    default:
      return status;
  }
}

function checkpointLabel(type: string): string {
  return type === "git_ref"
    ? "punto de control de Git"
    : "punto de control del sistema de archivos";
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
    `Estado: ${sessionStatusLabel(session.status)}`,
    session.checkpoint
      ? `Punto de control: ${checkpointLabel(session.checkpoint.checkpoint_type)}`
      : null,
    session.checkpoint?.git_hash ? `Git: ${session.checkpoint.git_hash.slice(0, 12)}` : null,
    `Cambios: ${session.change_log?.length ?? 0}`,
    `Antigüedad: ${Math.round(session.age_ms / 1000)}s`,
    `Sesiones activas: ${session.active_sessions}`,
  ];
  return pieces.filter(Boolean).join(" / ");
}

function sessionStatusFacetTitle(status: AgentSession["status"]): string {
  return `Indicador de estado de la sesión de Agent: ${sessionStatusLabel(status)}.`;
}

function turnStatusFacetTitle(turnStatus: string): string {
  return `Indicador de estado del turno de Agent: ${turnStatusLabel(turnStatus)}.`;
}

function checkpointStatusFacetTitle(checkpointType: string | null | undefined): string {
  return checkpointType
    ? `Indicador de estado del punto de control de Agent: ${checkpointLabel(checkpointType)}.`
    : "Indicador de estado del punto de control de Agent: sin punto de control.";
}

function changeLogStatusFacetTitle(count: number): string {
  return `Indicador del registro de cambios de Agent: ${countLabel(count, "cambio", "cambios")}.`;
}

function loadingSessionStatusTitle(): string {
  return "Franja de estado de la sesión de Agent: cargando la sesión.";
}

function loadingSessionStatusLabelTitle(): string {
  return "Etiqueta de la franja de estado de la sesión de Agent: Cargando sesión.";
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
