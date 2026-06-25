import type { DockviewApi } from "dockview-react";
import type { SerializedDockview } from "dockview-react";
import { PANEL_AGENT_TERMINAL, agentTerminalPanelId } from "./panels";
import type { AgentTerminalOpenParams } from "./openAgentTerminal";

const SAVE_DEBOUNCE_MS = 400;
const STORAGE_KEY = "tinto:console-dock";

function terminalTitle(params: AgentTerminalOpenParams): string {
  const agent = params.agentType ?? "agent";
  const short = params.sessionId.length > 8 ? params.sessionId.slice(0, 8) : params.sessionId;
  return `${agent} ${short}`;
}

function hasPanels(layout: SerializedDockview | null): boolean {
  const panels = (layout as { panels?: object } | null)?.panels;
  return !!panels && Object.keys(panels).length > 0;
}

function loadLayout(): SerializedDockview | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SerializedDockview) : null;
  } catch {
    return null;
  }
}

function saveLayout(layout: SerializedDockview): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
  } catch {
    /* storage unavailable / quota - keep the in-memory layout */
  }
}

function clearLayout(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

class ConsoleDock {
  private api: DockviewApi | null = null;
  private pending = new Map<string, AgentTerminalOpenParams>();
  private disposers: Array<() => void> = [];
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  private scheduleSave() {
    if (!this.api) return;
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      if (this.api) saveLayout(this.api.toJSON());
    }, SAVE_DEBOUNCE_MS);
  }

  register(api: DockviewApi) {
    this.disposers.forEach((dispose) => dispose());
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = null;
    this.api = api;
    const d1 = api.onDidLayoutChange(() => this.scheduleSave());
    this.disposers = [() => d1.dispose()];

    const saved = loadLayout();
    if (hasPanels(saved)) {
      try {
        api.fromJSON(saved as SerializedDockview);
      } catch {
        clearLayout();
      }
    }

    const queued = Array.from(this.pending.values());
    this.pending.clear();
    queued.forEach((params) => this.openTerminal(params));
  }

  unregister(api: DockviewApi) {
    if (this.api === api) {
      if (this.saveTimer) clearTimeout(this.saveTimer);
      this.saveTimer = null;
      try {
        saveLayout(api.toJSON());
      } catch {
        /* api already torn down - debounced saves captured the state */
      }
      this.disposers.forEach((dispose) => dispose());
      this.disposers = [];
      this.api = null;
    }
  }

  openTerminal(params: AgentTerminalOpenParams) {
    const id = agentTerminalPanelId(params.sessionId);
    const api = this.api;
    if (!api) {
      this.pending.set(id, params);
      return;
    }

    const existing = api.getPanel(id);
    if (existing) {
      existing.api.setActive();
      return;
    }

    try {
      api.addPanel({
        id,
        component: PANEL_AGENT_TERMINAL,
        title: terminalTitle(params),
        params,
      });
    } catch {
      api.getPanel(id)?.api.setActive();
    }
  }

  resetForTests() {
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.api = null;
    this.pending.clear();
    this.disposers.forEach((dispose) => dispose());
    this.disposers = [];
    this.saveTimer = null;
    clearLayout();
  }
}

export const consoleDock = new ConsoleDock();
