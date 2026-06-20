import { useCallback, useMemo, useSyncExternalStore } from "react";

const listeners = new Set<() => void>();

const explorerStorageKey = (repo: string) => `tinto:explorer-collapsed:${repo}`;
const explorerExpandedStorageKey = (repo: string) => `tinto:explorer-expanded:${repo}`;

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

function readExpandedSnapshot(repo: string): string {
  try {
    const raw = localStorage.getItem(explorerExpandedStorageKey(repo));
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(parsed)) return "[]";
    return JSON.stringify(parsed.filter((path): path is string => typeof path === "string").sort());
  } catch {
    return "[]";
  }
}

export function getExplorerExpanded(repo: string): Set<string> {
  try {
    return new Set(JSON.parse(readExpandedSnapshot(repo)) as string[]);
  } catch {
    return new Set();
  }
}

export function setExplorerExpanded(repo: string, expanded: Set<string>): void {
  try {
    const paths = [...expanded].sort();
    if (paths.length === 0) localStorage.removeItem(explorerExpandedStorageKey(repo));
    else localStorage.setItem(explorerExpandedStorageKey(repo), JSON.stringify(paths));
  } catch {
    /* storage unavailable - keep the runtime subscribers in sync */
  }
  notify();
}

export function useExplorerExpanded(
  repo: string,
): [Set<string>, (update: (current: Set<string>) => Set<string>) => void] {
  const getSnapshot = useCallback(() => readExpandedSnapshot(repo), [repo]);
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, () => "[]");
  const expanded = useMemo(() => new Set(JSON.parse(snapshot) as string[]), [snapshot]);
  const updateExpanded = useCallback(
    (update: (current: Set<string>) => Set<string>) => {
      setExplorerExpanded(repo, update(getExplorerExpanded(repo)));
    },
    [repo],
  );

  return [expanded, updateExpanded];
}
