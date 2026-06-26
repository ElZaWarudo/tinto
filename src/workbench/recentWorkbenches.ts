// Most-recently-used (MRU) workbench ordering. Persists across sessions in
// localStorage so the Recientes submenu and the manage modal can sort the
// workbench list the same way after a restart. Names not seen in this session
// (e.g. created on disk by another Tinto install) are appended at the end in
// the order they were first observed. The list is for ordering only — the
// authoritative set of workbenches still comes from the bus config.

const STORAGE_KEY = "tinto:recent-workbenches:v1";
const MAX_ENTRIES = 64;

function safeRead(): string[] {
  try {
    const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((value): value is string => typeof value === "string");
  } catch {
    return [];
  }
}

function safeWrite(names: string[]): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(names));
  } catch {
    /* localStorage unavailable (private mode, quota) — best-effort, no throw */
  }
}

/** Read the current MRU list (most recent first). */
export function getRecentWorkbenches(): string[] {
  return safeRead();
}

/** Push `name` to the front of the MRU list. De-duplicates and trims the
 *  tail. No-op when the browser does not expose localStorage. */
export function markRecentWorkbench(name: string): void {
  const trimmed = name.trim();
  if (!trimmed) return;
  const next = [trimmed, ...safeRead().filter((existing) => existing !== trimmed)].slice(
    0,
    MAX_ENTRIES,
  );
  safeWrite(next);
}

/** Remove every occurrence of `name` from the MRU list (e.g. on delete or
 *  rename — the caller follows up with markRecentWorkbench for the new name). */
export function forgetRecentWorkbench(name: string): void {
  const next = safeRead().filter((existing) => existing !== name);
  safeWrite(next);
}

/** Sort the given set of workbench names by MRU recency, falling back to the
 *  original order for entries that are not in the MRU list. Pure function:
 *  does not mutate the inputs. */
export function sortByRecency(names: readonly string[]): string[] {
  const mru = safeRead();
  const rank = new Map<string, number>();
  mru.forEach((name, index) => rank.set(name, index));
  const known = names.filter((n) => rank.has(n));
  const unknown = names.filter((n) => !rank.has(n));
  known.sort((a, b) => (rank.get(a) ?? 0) - (rank.get(b) ?? 0));
  return [...known, ...unknown];
}

/** Workbench config is authoritative when complete, but the app can briefly
 * render with a partial config while active/recent names are already known.
 * Keep those names visible so switching is still possible during that gap. */
export function visibleWorkbenchNames(
  configuredNames: readonly string[],
  activeName?: string | null,
): string[] {
  const seen = new Set<string>();
  const merged: string[] = [];
  for (const name of [...configuredNames, ...(activeName ? [activeName] : []), ...safeRead()]) {
    const trimmed = name.trim();
    if (!trimmed || seen.has(trimmed)) continue;
    seen.add(trimmed);
    merged.push(trimmed);
  }
  return sortByRecency(merged);
}
