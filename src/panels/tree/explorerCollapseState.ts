import { useCallback, useSyncExternalStore } from "react";

const listeners = new Set<() => void>();

const explorerStorageKey = (repo: string) => `tinto:explorer-collapsed:${repo}`;

function notify() {
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getExplorerCollapsed(repo: string): boolean {
  try {
    return localStorage.getItem(explorerStorageKey(repo)) === "1";
  } catch {
    return false;
  }
}

export function setExplorerCollapsed(repo: string, collapsed: boolean): void {
  try {
    localStorage.setItem(explorerStorageKey(repo), collapsed ? "1" : "0");
  } catch {
    /* storage unavailable - keep the runtime subscribers in sync */
  }
  notify();
}

export function useExplorerCollapsed(repo: string): [boolean, () => void] {
  const getSnapshot = useCallback(() => getExplorerCollapsed(repo), [repo]);
  const collapsed = useSyncExternalStore(subscribe, getSnapshot, () => false);
  const toggleCollapsed = useCallback(() => {
    setExplorerCollapsed(repo, !getExplorerCollapsed(repo));
  }, [repo]);

  return [collapsed, toggleCollapsed];
}
