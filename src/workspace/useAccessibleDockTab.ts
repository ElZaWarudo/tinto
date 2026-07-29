import type { IDockviewPanelHeaderProps } from "dockview-react";
import { useEffect, useId, useRef } from "react";

const TAB_ACTION_SELECTOR = "button, .dv-default-tab-action";

function siblingTabs(tab: HTMLElement): HTMLElement[] {
  return Array.from(
    tab.parentElement?.querySelectorAll<HTMLElement>(':scope > [role="tab"]') ?? [],
  );
}

export function useAccessibleDockTab(api: IDockviewPanelHeaderProps["api"]) {
  const contentRef = useRef<HTMLDivElement>(null);
  const reactId = useId().replace(/[^a-zA-Z0-9_-]/g, "");

  useEffect(() => {
    const tab = contentRef.current?.closest<HTMLElement>(".dv-tab");
    if (!tab) return;

    const tablist = tab.closest<HTMLElement>(".dv-tabs-container");
    const group = tab.closest<HTMLElement>(".dv-groupview");
    const panel = group?.querySelector<HTMLElement>(":scope > .dv-content-container");
    tablist?.setAttribute("role", "tablist");
    if (tablist && !tablist.hasAttribute("aria-label")) {
      tablist.setAttribute("aria-label", "Paneles");
    }

    if (!tab.id) tab.id = `tinto-dock-tab-${reactId}`;
    if (panel) {
      if (!panel.id) panel.id = `tinto-dock-panel-${reactId}`;
      panel.setAttribute("role", "tabpanel");
      tab.setAttribute("aria-controls", panel.id);
    }

    const syncTitle = () => tab.setAttribute("aria-label", api.title ?? api.id);
    const syncActive = () => {
      tab.setAttribute("aria-selected", String(api.isActive));
      tab.tabIndex = api.isActive ? 0 : -1;
      if (api.isActive) panel?.setAttribute("aria-labelledby", tab.id);
    };
    const activate = () => {
      api.setActive();
      tab.focus();
    };
    const onClick = (event: MouseEvent) => {
      if (event.button !== 0 || (event.target as Element).closest(TAB_ACTION_SELECTOR)) return;
      activate();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.target as Element).closest(TAB_ACTION_SELECTOR)) return;
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        activate();
        return;
      }

      const tabs = siblingTabs(tab);
      const currentIndex = tabs.indexOf(tab);
      let nextIndex: number | null = null;
      if (event.key === "ArrowRight" || event.key === "ArrowDown") {
        nextIndex = (currentIndex + 1) % tabs.length;
      } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
        nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;
      } else if (event.key === "Home") {
        nextIndex = 0;
      } else if (event.key === "End") {
        nextIndex = tabs.length - 1;
      }
      if (nextIndex == null || currentIndex < 0 || tabs.length === 0) return;

      event.preventDefault();
      tabs[nextIndex].click();
    };

    tab.setAttribute("role", "tab");
    syncTitle();
    syncActive();
    tab.addEventListener("click", onClick);
    tab.addEventListener("keydown", onKeyDown);
    const activeDisposable = api.onDidActiveChange(syncActive);
    const titleDisposable = api.onDidTitleChange(syncTitle);

    return () => {
      tab.removeEventListener("click", onClick);
      tab.removeEventListener("keydown", onKeyDown);
      activeDisposable.dispose();
      titleDisposable.dispose();
    };
  }, [api, reactId]);

  return contentRef;
}
