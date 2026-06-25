import type { TerminalPanelParams } from "./TerminalPanel";

const DETACHED_PARAM = "tintoDetachedTerminal";
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
      void webview.once("tauri://error", () => resolve(false));
    });
  } catch {
    const opened = window.open(detachedTerminalUrl(params), label, "width=980,height=680");
    return !!opened;
  }
}
