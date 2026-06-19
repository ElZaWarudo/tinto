// Modal overlay that lists every keyboard shortcut in the app. Opened from the
// "Ayuda" menu. Close on backdrop click, Escape key, or the × button.

import { useEffect, useMemo } from "react";
import { SHORTCUTS } from "../qol/shortcuts";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform);
const mod = isMac ? "⌘" : "Ctrl";

interface ShortcutGroup {
  title: string;
  items: { action: string; keys: string }[];
}

export function KeyboardShortcuts({ onClose }: { onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const groups = useMemo<ShortcutGroup[]>(() => {
    // Group shortcuts by their group property
    const byGroup = new Map<string, { action: string; keys: string }[]>();
    for (const shortcut of SHORTCUTS) {
      const items = byGroup.get(shortcut.group) ?? [];
      items.push({ action: shortcut.action, keys: shortcut.keys });
      byGroup.set(shortcut.group, items);
    }

    // Add zoom shortcuts to "Texto" group
    const zoomItems = [
      { action: "Aumentar tamaño del texto", keys: `${mod} +` },
      { action: "Reducir tamaño del texto", keys: `${mod} −` },
      { action: "Restablecer tamaño del texto", keys: `${mod} 0` },
    ];
    byGroup.set("Texto", zoomItems);

    // Convert to array, ordered by group appearance in SHORTCUTS
    const groupOrder = ["Navegación", "Cerrar", "Vista", "Proyecto", "Texto"];
    return groupOrder
      .filter((g) => byGroup.has(g))
      .map((title) => ({ title, items: byGroup.get(title)! }));
  }, []);

  return (
    <div className="shortcuts-backdrop" data-testid="shortcuts-backdrop" onClick={onClose}>
      <div
        className="shortcuts-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Atajos de teclado"
        data-testid="shortcuts-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="shortcuts-modal__head">
          <h2 className="shortcuts-modal__title">Atajos de teclado</h2>
          <button
            type="button"
            className="shortcuts-modal__close"
            aria-label="Cerrar"
            data-testid="shortcuts-close"
            onClick={onClose}
          >
            ×
          </button>
        </header>

        <div className="shortcuts-modal__body">
          {groups.map((group) => (
            <section key={group.title} className="shortcuts-group">
              <h3 className="shortcuts-group__title">{group.title}</h3>
              <dl className="shortcuts-list">
                {group.items.map((item) => (
                  <div key={item.action} className="shortcuts-row">
                    <dt>{item.action}</dt>
                    <dd>
                      <kbd>{item.keys}</kbd>
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
