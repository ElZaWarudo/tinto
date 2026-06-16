// Open-file text size, browser-style. Scales ONLY the active file's content
// (code / diff / rendered markdown), not the whole viewer: the level is exposed
// as the CSS variable `--file-zoom`, which the file-content rules multiply into
// their font-size. Persisted to localStorage; bound to Ctrl/Cmd +/-/0 and the
// "Ver" menu.

import { useSyncExternalStore } from "react";

export const ZOOM_MIN = 0.6;
export const ZOOM_MAX = 2.4;
export const ZOOM_STEP = 0.1;
const DEFAULT_ZOOM = 1;
const STORAGE_KEY = "tinto:zoom";

/** Clamp to [MIN, MAX] and round to 1 decimal (avoids float drift on repeats). */
export function clampZoom(z: number): number {
  const clamped = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));
  return Math.round(clamped * 10) / 10;
}

function readPersisted(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const n = raw ? Number(raw) : NaN;
    return Number.isFinite(n) ? clampZoom(n) : DEFAULT_ZOOM;
  } catch {
    return DEFAULT_ZOOM;
  }
}

class ZoomStore {
  private zoom = DEFAULT_ZOOM;
  private listeners = new Set<() => void>();
  private hydrated = false;

  getZoom = (): number => this.zoom;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  /** Load the persisted level and apply it. Call once on app mount. */
  hydrate() {
    if (this.hydrated) return;
    this.hydrated = true;
    this.apply(readPersisted());
  }

  private apply(zoom: number) {
    this.zoom = zoom;
    // Only the file-content rules consume --file-zoom, so this scales the open
    // file's text without touching the rest of the UI.
    if (typeof document !== "undefined") {
      document.documentElement.style.setProperty("--file-zoom", String(zoom));
    }
    try {
      localStorage.setItem(STORAGE_KEY, String(zoom));
    } catch {
      /* private mode / unavailable storage — keep the in-memory level */
    }
    this.listeners.forEach((l) => l());
  }

  set(zoom: number) {
    this.apply(clampZoom(zoom));
  }
  zoomIn() {
    this.set(this.zoom + ZOOM_STEP);
  }
  zoomOut() {
    this.set(this.zoom - ZOOM_STEP);
  }
  reset() {
    this.set(DEFAULT_ZOOM);
  }
}

export const zoomStore = new ZoomStore();

export function useZoom(): number {
  return useSyncExternalStore(zoomStore.subscribe, zoomStore.getZoom);
}

/** Handle a keydown for the zoom accelerators. Exported for unit testing. */
export function handleZoomKey(e: KeyboardEvent): boolean {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return false;
  switch (e.key) {
    case "+":
    case "=": // unshifted '+'
    case "Add":
      zoomStore.zoomIn();
      return true;
    case "-":
    case "_":
    case "Subtract":
      zoomStore.zoomOut();
      return true;
    case "0":
      zoomStore.reset();
      return true;
    default:
      return false;
  }
}

/** Bind the zoom accelerators to the window. Returns an unbind cleanup. */
export function installZoomKeybindings(target: Window = window): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (handleZoomKey(e)) e.preventDefault();
  };
  target.addEventListener("keydown", onKey);
  return () => target.removeEventListener("keydown", onKey);
}
