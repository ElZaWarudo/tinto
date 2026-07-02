import type { TerminalPanelParams } from "./TerminalPanel";
import type { UnlistenFn } from "@tauri-apps/api/event";

const DETACHED_PARAM = "tintoDetachedTerminal";
const DETACHED_CONSOLES_PARAM = "tintoDetachedConsoles";
const DETACHED_CONSOLES_SESSIONS_PARAM = "sessions";
const DETACHED_CONSOLES_OPEN_TERMINAL_EVENT = "tinto://detached-consoles-open-terminal";
const DETACHED_CONSOLES_REATTACH_EVENT = "tinto://detached-consoles-reattach";
const DETACHED_SKIP_STOP_PREFIX = "tinto:detached-terminal:";

export function detachedTerminalWindowLabel(sessionId: string): string {
  const safe = sessionId.replace(/[^a-zA-Z0-9-_:]/g, "_");
  return `terminal-${safe}`;
}

export function detachedTerminalUrl(params: TerminalPanelParams): string {
  const query = new URLSearchParams();
  query.set(DETACHED_PARAM, "1");
  query.set("sessionId", params.sessionId);
  if (params.repo) query.set("repo", params.repo);
  if (params.agentType) query.set("agentType", params.agentType);
  return `/?${query.toString()}`;
}

export function detachedConsolesUrl(params: TerminalPanelParams[] = []): string {
  const query = new URLSearchParams();
  query.set(DETACHED_CONSOLES_PARAM, "1");
  if (params.length > 0) {
    query.set(DETACHED_CONSOLES_SESSIONS_PARAM, JSON.stringify(params));
  }
  return `/?${query.toString()}`;
}

export function readDetachedTerminalParams(
  search = window.location.search,
): TerminalPanelParams | null {
  const query = new URLSearchParams(search);
  if (query.get(DETACHED_PARAM) !== "1") return null;
  const sessionId = query.get("sessionId");
  if (!sessionId) return null;
  return {
    sessionId,
    repo: query.get("repo") ?? undefined,
    agentType: query.get("agentType") ?? undefined,
  };
}

export function readDetachedConsolesFlag(search = window.location.search): boolean {
  return new URLSearchParams(search).get(DETACHED_CONSOLES_PARAM) === "1";
}

export function readDetachedConsolesParams(search = window.location.search): TerminalPanelParams[] {
  const query = new URLSearchParams(search);
  if (query.get(DETACHED_CONSOLES_PARAM) !== "1") return [];
  const raw = query.get(DETACHED_CONSOLES_SESSIONS_PARAM);
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    const params: TerminalPanelParams[] = [];
    for (const item of parsed) {
      if (!item || typeof item !== "object") continue;
      const candidate = item as Partial<TerminalPanelParams>;
      if (typeof candidate.sessionId !== "string" || !candidate.sessionId) continue;
      params.push(terminalParams(candidate.sessionId, candidate.repo, candidate.agentType));
    }
    return params;
  } catch {
    return [];
  }
}

function terminalParams(sessionId: string, repo: unknown, agentType: unknown): TerminalPanelParams {
  const params: TerminalPanelParams = { sessionId };
  if (typeof repo === "string") params.repo = repo;
  if (typeof agentType === "string") params.agentType = agentType;
  return params;
}

export function markTerminalDetached(sessionId: string): void {
  try {
    localStorage.setItem(`${DETACHED_SKIP_STOP_PREFIX}${sessionId}`, String(Date.now()));
  } catch {
    /* storage unavailable - fall back to normal close behavior */
  }
}

export function consumeTerminalDetachedMarker(sessionId: string): boolean {
  const key = `${DETACHED_SKIP_STOP_PREFIX}${sessionId}`;
  try {
    const value = localStorage.getItem(key);
    if (!value) return false;
    localStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

export async function openDetachedTerminalWindow(params: TerminalPanelParams): Promise<boolean> {
  const label = detachedTerminalWindowLabel(params.sessionId);
  const title = `${params.agentType ?? "agent"} ${params.sessionId.slice(0, 8)}`;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      await existing.setFocus();
      return true;
    }

    const webview = new WebviewWindow(label, {
      url: detachedTerminalUrl(params),
      title,
      width: 980,
      height: 680,
      minWidth: 640,
      minHeight: 420,
      resizable: true,
      focus: true,
    });
    return await new Promise<boolean>((resolve) => {
      void webview.once("tauri://created", () => resolve(true));
      void webview.once("tauri://error", (event) => {
        console.error("Failed to create detached terminal window", event.payload);
        resolve(false);
      });
    });
  } catch (error) {
    console.error("Failed to open detached terminal window", error);
    const opened = window.open(detachedTerminalUrl(params), label, "width=980,height=680");
    return !!opened;
  }
}

export async function openDetachedConsolesWindow(
  params: TerminalPanelParams[] = [],
): Promise<boolean> {
  const label = "consoles";
  const url = detachedConsolesUrl(params);
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      await existing.setFocus();
      await Promise.all(
        params.map((terminal) => existing.emit(DETACHED_CONSOLES_OPEN_TERMINAL_EVENT, terminal)),
      );
      return true;
    }

    const webview = new WebviewWindow(label, {
      url,
      title: "Agents",
      width: 1100,
      height: 720,
      minWidth: 720,
      minHeight: 460,
      resizable: true,
      focus: true,
    });
    return await new Promise<boolean>((resolve) => {
      void webview.once("tauri://created", () => resolve(true));
      void webview.once("tauri://error", (event) => {
        console.error("Failed to create detached consoles window", event.payload);
        resolve(false);
      });
    });
  } catch (error) {
    console.error("Failed to open detached consoles window", error);
    const opened = window.open(url, label, "width=1100,height=720");
    return !!opened;
  }
}

export async function sendTerminalToDetachedConsoles(
  params: TerminalPanelParams,
): Promise<boolean> {
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel("consoles");
    if (!existing) return false;

    await existing.setFocus();
    await existing.emit(DETACHED_CONSOLES_OPEN_TERMINAL_EVENT, params);
    return true;
  } catch (error) {
    console.error("Failed to route terminal to detached consoles window", error);
    return false;
  }
}

export async function onDetachedConsolesOpenTerminal(
  callback: (params: TerminalPanelParams) => void,
): Promise<UnlistenFn> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<TerminalPanelParams>(DETACHED_CONSOLES_OPEN_TERMINAL_EVENT, (event) => {
      callback(event.payload);
    });
  } catch (error) {
    if (!isUnavailableTauriEventBridgeError(error)) {
      throw error;
    }
    return () => {};
  }
}

export async function reattachDetachedConsoles(params: TerminalPanelParams[]): Promise<boolean> {
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit(DETACHED_CONSOLES_REATTACH_EVENT, params);
    return true;
  } catch (error) {
    console.error("Failed to reattach detached consoles window", error);
    return false;
  }
}

export async function onDetachedConsolesReattach(
  callback: (params: TerminalPanelParams[]) => void,
): Promise<UnlistenFn> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<TerminalPanelParams[]>(DETACHED_CONSOLES_REATTACH_EVENT, (event) => {
      callback(event.payload);
    });
  } catch (error) {
    if (!isUnavailableTauriEventBridgeError(error)) {
      throw error;
    }
    return () => {};
  }
}

function isUnavailableTauriEventBridgeError(error: unknown): boolean {
  return error instanceof TypeError && error.message.includes("transformCallback");
}

export async function closeCurrentDetachedWindow(): Promise<void> {
  try {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    await getCurrentWebviewWindow().close();
  } catch (error) {
    console.error("Failed to close detached consoles window", error);
  }
}
