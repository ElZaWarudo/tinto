import type { DockviewApi } from "dockview-react";
import type { SerializedDockview } from "dockview-react";
import {
  PANEL_AGENT_TERMINAL,
  TAB_AGENT_CONVERSATION,
  agentTerminalPanelId,
  sessionIdFromAgentTerminalPanelId,
} from "./panels";
import type { AgentTerminalOpenParams } from "./openAgentTerminal";
import { markRecentAgentLaunch } from "./recentAgentLaunches";

const LEGACY_STORAGE_KEY = "tinto:console-dock";
const TRANSFER_STORAGE_KEY = "tinto:console-dock-transfer";

interface RegisterOptions {
  restoreTransferLayout?: boolean;
}

interface EnsureTerminalOptions {
  activate?: boolean;
}

export function agentTerminalTitle(params: AgentTerminalOpenParams, chatNumber = 1): string {
  const agent = agentTitleLabel(params.agentType);
  const project = projectTitleLabel(params.repo);
  return `${agent} · ${project}${chatNumber > 1 ? ` · Chat ${chatNumber}` : ""}`;
}

export function agentConversationTabTitle(
  params: AgentTerminalOpenParams,
  conversationTitle: string,
): string {
  return `${agentTitleLabel(params.agentType)} · ${projectTitleLabel(params.repo)} · ${conversationTitle}`;
}

function agentTitleLabel(agentType: string | undefined): string {
  const normalized = agentType?.trim().toLocaleLowerCase();
  if (normalized === "codex") return "Codex";
  if (normalized === "claude") return "Claude";
  if (normalized === "opencode") return "OpenCode";
  return agentType?.trim() || "Agent";
}

function projectTitleLabel(repo: string | undefined): string {
  const normalized = repo?.replace(/\\/g, "/").replace(/\/+$/, "");
  return normalized?.split("/").pop() || "Proyecto";
}

function terminalGroupKey(params: AgentTerminalOpenParams): string {
  return `${params.agentType?.trim().toLocaleLowerCase() || "agent"}\n${
    params.repo?.replace(/\\/g, "/").replace(/\/+$/, "").toLocaleLowerCase() || ""
  }`;
}

function hasPanels(layout: SerializedDockview | null): boolean {
  const panels = (layout as { panels?: object } | null)?.panels;
  return !!panels && Object.keys(panels).length > 0;
}

function loadTransferLayout(): SerializedDockview | null {
  try {
    const raw = localStorage.getItem(TRANSFER_STORAGE_KEY);
    localStorage.removeItem(TRANSFER_STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SerializedDockview) : null;
  } catch {
    return null;
  }
}

function saveTransferLayout(layout: SerializedDockview): void {
  try {
    localStorage.setItem(TRANSFER_STORAGE_KEY, JSON.stringify(layout));
  } catch {
    /* storage unavailable / quota - skip detached-console restoration */
  }
}

function clearStoredLayouts(): void {
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY);
    localStorage.removeItem(TRANSFER_STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

class ConsoleDock {
  private api: DockviewApi | null = null;
  private pending = new Map<string, AgentTerminalOpenParams>();
  private terminals = new Map<string, AgentTerminalOpenParams>();
  private terminalChatNumbers = new Map<string, number>();
  private nextChatNumberByGroup = new Map<string, number>();
  private forgetTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private disposers: Array<() => void> = [];
  private listeners = new Set<() => void>();

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private emit() {
    this.listeners.forEach((listener) => listener());
  }

  register(api: DockviewApi, options: RegisterOptions = {}) {
    this.disposers.forEach((dispose) => dispose());
    this.api = api;
    clearLegacyLayout();
    const removeDisposable = api.onDidRemovePanel((panel) => {
      this.scheduleForgetTerminal(panel.id);
      this.emit();
    });
    const layoutDisposable = api.onDidLayoutChange(() => this.emit());
    this.disposers = [() => removeDisposable.dispose(), () => layoutDisposable.dispose()];

    const saved = options.restoreTransferLayout ? loadTransferLayout() : null;
    if (hasPanels(saved)) {
      try {
        api.fromJSON(saved as SerializedDockview);
        this.rememberPanelsFromApi(api);
      } catch {
        clearStoredLayouts();
      }
    }

    const queued = Array.from(this.pending.values());
    this.pending.clear();
    queued.forEach((params) => this.rememberTerminal(params));
    Array.from(this.terminals.values()).forEach((params) =>
      this.ensureTerminalPanel(params, { activate: false }),
    );
    this.emit();
  }

  unregister(api: DockviewApi) {
    if (this.api === api) {
      this.disposers.forEach((dispose) => dispose());
      this.disposers = [];
      this.api = null;
      this.emit();
    }
  }

  openTerminal(params: AgentTerminalOpenParams) {
    this.rememberTerminal(params);
    markRecentAgentLaunch(params);
    const id = agentTerminalPanelId(params.sessionId);
    const api = this.api;
    if (!api) {
      this.pending.set(id, params);
      return;
    }

    this.ensureTerminalPanel(params, { activate: true });
  }

  closeTerminal(sessionId: string) {
    const id = agentTerminalPanelId(sessionId);
    this.cancelForgetTerminal(id);
    this.pending.delete(id);
    this.terminals.delete(id);
    this.terminalChatNumbers.delete(id);
    const api = this.api;
    const panel = api?.getPanel(id);
    if (api && panel) {
      api.removePanel(panel);
    }
    this.emit();
  }

  private ensureTerminalPanel(
    params: AgentTerminalOpenParams,
    options: EnsureTerminalOptions = {},
  ) {
    const id = agentTerminalPanelId(params.sessionId);
    const api = this.api;
    if (!api) return;
    const activate = options.activate ?? true;

    const existing = api.getPanel(id);
    if (existing) {
      if (activate) {
        existing.api.setActive();
      }
      this.emit();
      return;
    }

    try {
      api.addPanel({
        id,
        component: PANEL_AGENT_TERMINAL,
        title: agentTerminalTitle(params, this.terminalChatNumbers.get(id) ?? 1),
        tabComponent: TAB_AGENT_CONVERSATION,
        params,
      });
      this.emit();
    } catch {
      if (activate) {
        api.getPanel(id)?.api.setActive();
      }
      this.emit();
    }
  }

  private rememberTerminal(params: AgentTerminalOpenParams) {
    const id = agentTerminalPanelId(params.sessionId);
    this.cancelForgetTerminal(id);
    if (!this.terminalChatNumbers.has(id)) {
      const group = terminalGroupKey(params);
      const next = (this.nextChatNumberByGroup.get(group) ?? 0) + 1;
      this.nextChatNumberByGroup.set(group, next);
      this.terminalChatNumbers.set(id, next);
    }
    this.terminals.set(id, params);
  }

  private rememberPanelsFromApi(api: DockviewApi) {
    for (const panel of api.panels) {
      const sessionId = sessionIdFromAgentTerminalPanelId(panel.id);
      if (!sessionId) continue;
      const params = panel.params as Partial<AgentTerminalOpenParams> | undefined;
      this.rememberTerminal({
        sessionId,
        repo: typeof params?.repo === "string" ? params.repo : undefined,
        agentType: typeof params?.agentType === "string" ? params.agentType : undefined,
      });
    }
  }

  private scheduleForgetTerminal(panelId: string) {
    if (!sessionIdFromAgentTerminalPanelId(panelId)) return;
    this.cancelForgetTerminal(panelId);
    const timer = setTimeout(() => {
      this.forgetTimers.delete(panelId);
      this.terminals.delete(panelId);
      this.terminalChatNumbers.delete(panelId);
      this.pending.delete(panelId);
      this.emit();
    }, 250);
    this.forgetTimers.set(panelId, timer);
  }

  private cancelForgetTerminal(panelId: string) {
    const timer = this.forgetTimers.get(panelId);
    if (timer === undefined) return;
    clearTimeout(timer);
    this.forgetTimers.delete(panelId);
  }

  openTerminalSessionIds(): string[] {
    const api = this.api;
    if (!api) return [];
    return api.panels
      .map((panel) => sessionIdFromAgentTerminalPanelId(panel.id))
      .filter((sessionId): sessionId is string => !!sessionId);
  }

  openTerminalParams(): AgentTerminalOpenParams[] {
    const api = this.api;
    if (!api) return Array.from(this.terminals.values());
    const terminals: AgentTerminalOpenParams[] = [];
    for (const panel of api.panels) {
      const sessionId = sessionIdFromAgentTerminalPanelId(panel.id);
      if (!sessionId) continue;
      const params = panel.params as Partial<AgentTerminalOpenParams> | undefined;
      const terminal: AgentTerminalOpenParams = { sessionId };
      if (typeof params?.repo === "string") terminal.repo = params.repo;
      if (typeof params?.agentType === "string") terminal.agentType = params.agentType;
      terminals.push(terminal);
    }
    return terminals;
  }

  saveNow() {
    if (!this.api) return;
    saveTransferLayout(this.api.toJSON());
  }

  prepareDetachedTransfer() {
    this.saveNow();
  }

  resetForTests() {
    this.api = null;
    this.pending.clear();
    this.terminals.clear();
    this.terminalChatNumbers.clear();
    this.nextChatNumberByGroup.clear();
    this.forgetTimers.forEach((timer) => clearTimeout(timer));
    this.forgetTimers.clear();
    this.disposers.forEach((dispose) => dispose());
    this.disposers = [];
    this.listeners.clear();
    clearStoredLayouts();
  }
}

function clearLegacyLayout(): void {
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

export const consoleDock = new ConsoleDock();
