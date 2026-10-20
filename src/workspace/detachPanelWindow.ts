// A second-level tab in a window of its own: a project's file, or a repo's
// Delivery. The window opens Tinto with the panel in its URL and renders
// only that panel (DetachedPanelApp).

import { openAppWindow } from "./appWindow";

export type DetachedPanel =
  { kind: "file"; repo: string; path: string } | { kind: "delivery"; repo: string };

const PARAM = "tintoDetachedPanel";

export function detachedPanelUrl(panel: DetachedPanel): string {
  const query = new URLSearchParams();
  query.set(PARAM, JSON.stringify(panel));
  return `/?${query.toString()}`;
}

export function readDetachedPanel(search = window.location.search): DetachedPanel | null {
  const raw = new URLSearchParams(search).get(PARAM);
  if (!raw) return null;
  try {
    const value = JSON.parse(raw) as Partial<{ kind: string; repo: unknown; path: unknown }>;
    if (typeof value.repo !== "string" || !value.repo) return null;
    if (value.kind === "file" && typeof value.path === "string" && value.path) {
      return { kind: "file", repo: value.repo, path: value.path };
    }
    if (value.kind === "delivery") return { kind: "delivery", repo: value.repo };
    return null;
  } catch {
    return null;
  }
}

/** The same panel always gets the same label, so detaching it again focuses its window. */
export function detachedPanelLabel(panel: DetachedPanel): string {
  const key =
    panel.kind === "file" ? `file\n${panel.repo}\n${panel.path}` : `delivery\n${panel.repo}`;
  // FNV-1a: window labels only take a few characters, and a path has any.
  let hash = 0x811c9dc5;
  for (let index = 0; index < key.length; index += 1) {
    hash ^= key.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `panel-${panel.kind}-${hash.toString(16)}`;
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export function detachedPanelTitle(panel: DetachedPanel): string {
  return panel.kind === "file"
    ? `${baseName(panel.path)} · ${baseName(panel.repo)}`
    : `Delivery · ${baseName(panel.repo)}`;
}

/** `onClosed` runs when the window closes, to bring the tab back. */
export function openDetachedPanelWindow(
  panel: DetachedPanel,
  onClosed?: () => void,
): Promise<boolean> {
  return openAppWindow({
    label: detachedPanelLabel(panel),
    url: detachedPanelUrl(panel),
    title: detachedPanelTitle(panel),
    width: 980,
    height: 720,
    minWidth: 480,
    minHeight: 360,
    onClosed,
  });
}

/** Whether the panel's window is open, as after the main window reloads with
 *  it still out; `onClosed` then runs when it closes. */
export async function watchDetachedPanelWindow(
  panel: DetachedPanel,
  onClosed: () => void,
): Promise<boolean> {
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(detachedPanelLabel(panel));
    if (!existing) return false;
    void existing.once("tauri://destroyed", onClosed);
    return true;
  } catch {
    return false;
  }
}
