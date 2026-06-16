import type { BusState } from "../bus/store";
import type { FsEvent, PassiveSignal, RepoDelta } from "../bus/contract";
import type { TimelineEntry } from "../panels/timeline/model";
import type { TreeNode } from "../panels/tree/fileTree";
import type { QualityFilters } from "./state";
import { ALL_REPOS } from "./state";

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function normalizePath(value: string): string {
  return value.replace(/\\/g, "/").toLowerCase();
}

function searchNeedle(filters: QualityFilters): string {
  return normalize(filters.search);
}

export function normalizedExtension(extension: string): string {
  const raw = normalize(extension);
  if (!raw) return "";
  return raw.startsWith(".") ? raw : `.${raw}`;
}

export function pathExtension(path: string): string {
  const base = normalizePath(path).split("/").pop() ?? path;
  const dot = base.lastIndexOf(".");
  if (dot === 0 && base.indexOf(".", 1) === -1) return base;
  return dot > 0 ? base.slice(dot) : "";
}

export function matchesExtension(path: string, filters: QualityFilters): boolean {
  const expected = normalizedExtension(filters.extension);
  if (!expected) return true;
  return pathExtension(path) === expected;
}

export function matchesTimeWindow(
  timestampMs: number,
  filters: QualityFilters,
  nowMs: number,
): boolean {
  if (filters.timeWindow === "all") return true;
  if (filters.timeWindow === "15m") return nowMs - timestampMs <= 15 * 60 * 1000;
  if (filters.timeWindow === "1h") return nowMs - timestampMs <= 60 * 60 * 1000;

  const now = new Date(nowMs);
  const event = new Date(timestampMs);
  return (
    now.getFullYear() === event.getFullYear() &&
    now.getMonth() === event.getMonth() &&
    now.getDate() === event.getDate()
  );
}

function signalText(signal: PassiveSignal): string {
  return [signal.kind, signal.severity, signal.message, signal.path ?? ""].join(" ");
}

function changedPaths(delta: RepoDelta): string[] {
  return [...delta.status.modified, ...delta.status.staged, ...delta.status.untracked];
}

function matchesText(values: string[], filters: QualityFilters): boolean {
  const needle = searchNeedle(filters);
  if (!needle) return true;
  return values.some((value) => normalizePath(value).includes(needle));
}

export function matchesRepoFilter(repo: string, filters: QualityFilters): boolean {
  return filters.repo === ALL_REPOS || filters.repo === repo;
}

export function matchesRepoDelta(
  delta: RepoDelta,
  filters: QualityFilters,
  displayName: string,
): boolean {
  if (!matchesRepoFilter(delta.repo, filters)) return false;

  const paths = changedPaths(delta);
  if (normalizedExtension(filters.extension)) {
    const signalPaths = (delta.signals ?? []).map((signal) => signal.path ?? "").filter(Boolean);
    if (![...paths, ...signalPaths].some((path) => matchesExtension(path, filters))) return false;
  }

  return matchesText(
    [
      displayName,
      delta.repo,
      delta.error?.category ?? "",
      delta.error?.message ?? "",
      ...paths,
      ...(delta.signals ?? []).map(signalText),
    ],
    filters,
  );
}

export function filterRepoPaths(
  state: BusState,
  paths: string[],
  filters: QualityFilters,
  displayName: (repo: string) => string,
): string[] {
  return paths.filter((repo) => matchesRepoDelta(state.repos[repo], filters, displayName(repo)));
}

export function filterStatusFiles(
  files: string[],
  filters: QualityFilters,
  signals: PassiveSignal[] = [],
): string[] {
  return files.filter((path) => {
    if (!matchesExtension(path, filters)) return false;
    const pathSignals = signals.filter((signal) => signal.path === path).map(signalText);
    return matchesText([path, ...pathSignals], filters);
  });
}

export function filterFsEvents(
  repo: string,
  events: FsEvent[],
  filters: QualityFilters,
  displayName: string,
  nowMs: number,
): FsEvent[] {
  if (!matchesRepoFilter(repo, filters)) return [];
  return events.filter((event) => {
    if (!matchesExtension(event.path, filters)) return false;
    if (!matchesTimeWindow(event.timestamp_ms, filters, nowMs)) return false;
    return matchesText(
      [displayName, repo, event.path, event.kind, ...(event.signals ?? []).map(signalText)],
      filters,
    );
  });
}

export function filterTimelineEntries(
  entries: TimelineEntry[],
  filters: QualityFilters,
  nowMs: number,
): TimelineEntry[] {
  return entries.filter((entry) => {
    if (entry.repo && !matchesRepoFilter(entry.repo, filters)) return false;
    if (entry.path && !matchesExtension(entry.path, filters)) return false;
    if (!matchesTimeWindow(entry.timestampMs, filters, nowMs)) return false;
    return matchesText(
      [
        entry.repoName,
        entry.repo ?? "",
        entry.title,
        entry.detail,
        entry.path ?? "",
        entry.fsKind ?? "",
      ],
      filters,
    );
  });
}

export function filterTreeNodes(
  nodes: TreeNode[],
  filters: QualityFilters,
  signals: PassiveSignal[] = [],
): TreeNode[] {
  const signalMatches = (path: string) =>
    signals.filter((signal) => signal.path === path).map(signalText);

  return nodes.flatMap((node) => {
    if (node.isDir) {
      const children = filterTreeNodes(node.children, filters, signals);
      const dirMatches =
        !normalizedExtension(filters.extension) && matchesText([node.path, node.name], filters);
      if (children.length > 0 || dirMatches) return [{ ...node, children }];
      return [];
    }

    if (!matchesExtension(node.path, filters)) return [];
    if (!matchesText([node.path, node.name, ...signalMatches(node.path)], filters)) return [];
    return [{ ...node, children: [] }];
  });
}

export function hasActiveFilters(filters: QualityFilters): boolean {
  return (
    normalize(filters.search) !== "" ||
    filters.repo !== ALL_REPOS ||
    normalizedExtension(filters.extension) !== "" ||
    filters.timeWindow !== "all"
  );
}
