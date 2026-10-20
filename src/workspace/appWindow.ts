// Opens one of Tinto's own windows (a detached tab), or brings it forward
// when a window with that label is already open.

import type { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface AppWindowOptions {
  label: string;
  url: string;
  title: string;
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
  /** Runs on a window already open with this label, after focusing it. */
  onExisting?: (window: WebviewWindow) => Promise<unknown>;
  /** Runs once the window closes. */
  onClosed?: () => void;
}

/** Resolves to false when the window could not open. */
export async function openAppWindow(options: AppWindowOptions): Promise<boolean> {
  const { label, url, title, width, height, minWidth, minHeight, onExisting, onClosed } = options;
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) {
      await existing.setFocus();
      await onExisting?.(existing);
      if (onClosed) void existing.once("tauri://destroyed", onClosed);
      return true;
    }
    const webview = new WebviewWindow(label, {
      url,
      title,
      width,
      height,
      minWidth,
      minHeight,
      resizable: true,
      focus: true,
    });
    return await new Promise<boolean>((resolve) => {
      void webview.once("tauri://created", () => resolve(true));
      if (onClosed) void webview.once("tauri://destroyed", onClosed);
      void webview.once("tauri://error", (event) => {
        console.error(`Failed to create window ${label}`, event.payload);
        resolve(false);
      });
    });
  } catch (error) {
    console.error(`Failed to open window ${label}`, error);
    const opened = window.open(url, label, `width=${width},height=${height}`);
    return !!opened;
  }
}
