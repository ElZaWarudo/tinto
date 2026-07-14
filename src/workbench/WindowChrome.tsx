import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import tintoWordmarkDark from "../assets/brand/tinto-wordmark-dark.png";

function inTauriRuntime(): boolean {
  return Boolean((globalThis as typeof globalThis & { isTauri?: boolean }).isTauri);
}

function runWindowAction(action: () => Promise<void>): void {
  void action().catch(() => {
    // Browser previews do not expose native window commands.
  });
}

export function WindowBrand({ decorative = false }: { decorative?: boolean }) {
  return (
    <span className="menu-bar__brand" data-tauri-drag-region>
      <img
        className="menu-bar__brand-img"
        src={tintoWordmarkDark}
        alt={decorative ? "" : "Tinto"}
        data-tauri-drag-region
        onError={(event) => {
          event.currentTarget.style.display = "none";
        }}
      />
      <span className="menu-bar__brand-fallback" aria-hidden="true" data-tauri-drag-region>
        Tinto
      </span>
    </span>
  );
}

export function WindowDragRegion({ children }: { children?: ReactNode }) {
  return (
    <span
      className="menu-bar__drag-region"
      data-tauri-drag-region
      onDoubleClick={() => {
        if (!inTauriRuntime()) return;
        runWindowAction(() => getCurrentWindow().toggleMaximize());
      }}
    >
      {children}
    </span>
  );
}

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const native = inTauriRuntime();

  useEffect(() => {
    if (!native) return;
    const appWindow = getCurrentWindow();
    let active = true;
    let unlisten: (() => void) | null = null;
    const syncMaximized = () => {
      void appWindow
        .isMaximized()
        .then((value) => active && setMaximized(value))
        .catch(() => {});
    };
    syncMaximized();
    void appWindow
      .onResized(syncMaximized)
      .then((dispose) => {
        if (!active) dispose();
        else unlisten = dispose;
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
    };
  }, [native]);

  if (!native) return null;
  const appWindow = getCurrentWindow();
  const toggleMaximize = () => {
    runWindowAction(async () => {
      await appWindow.toggleMaximize();
      setMaximized(await appWindow.isMaximized());
    });
  };

  return (
    <div className="window-controls" aria-label="Controles de ventana">
      <button
        type="button"
        className="window-controls__button"
        aria-label="Minimizar"
        title="Minimizar"
        onClick={() => runWindowAction(() => appWindow.minimize())}
      >
        <span className="window-controls__minimize" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-controls__button"
        aria-label={maximized ? "Restaurar" : "Maximizar"}
        title={maximized ? "Restaurar" : "Maximizar"}
        onClick={toggleMaximize}
      >
        <span
          className={maximized ? "window-controls__restore" : "window-controls__maximize"}
          aria-hidden="true"
        />
      </button>
      <button
        type="button"
        className="window-controls__button window-controls__button--close"
        aria-label="Cerrar Tinto"
        title="Cerrar"
        onClick={() => runWindowAction(() => appWindow.close())}
      >
        <span className="window-controls__close" aria-hidden="true" />
      </button>
    </div>
  );
}

export function CompactWindowBar() {
  return (
    <header className="menu-bar menu-bar--minimal" aria-label="Barra de ventana">
      <WindowBrand decorative />
      <WindowDragRegion />
      <WindowControls />
    </header>
  );
}
