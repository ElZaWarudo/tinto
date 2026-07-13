import type { BusState } from "../../bus/store";
import type { FsEventKind, RepoDelta } from "../../bus/contract";
import { getFsEvents, getRepoSignals, statusSummary } from "../../bus/store";

export const ORPHAN_QUIET_MS = 30 * 60 * 1000;
export const TIMELINE_COMMIT_LIMIT = 8;

export type TimelineEntryKind = "activity" | "fs-event" | "orphan" | "error" | "degraded";

export interface TimelineEntry {
  id: string;
  kind: TimelineEntryKind;
  repo: string | null;
  repoName: string;
  timestampMs: number;
  title: string;
  detail: string;
  path?: string;
  fsKind?: FsEventKind;
}

function dirty(delta: RepoDelta): boolean {
  const { modified, staged, untracked } = delta.status;
  return modified.length + staged.length + untracked.length > 0;
}

function repoActivityMs(state: BusState, delta: RepoDelta): number {
  return state.activity[delta.repo] ?? delta.last_activity_ms;
}

function fsEventLabel(kind: FsEventKind): string {
  if (kind === "created") return "creado";
  if (kind === "removed") return "eliminado";
  return "modificado";
}

export function buildTimelineEntries(
  state: BusState,
  displayName: (repo: string) => string,
  nowMs: number,
): TimelineEntry[] {
  const entries: TimelineEntry[] = [];

  if (!state.watching.available) {
    entries.push({
      id: "degraded",
      kind: "degraded",
      repo: null,
      repoName: "Workbench",
      timestampMs: nowMs,
      title: "Supervisión degradada",
      detail: state.watching.reason ?? "Los datos siguen disponibles bajo demanda.",
    });
  }

  for (const delta of Object.values(state.repos)) {
    const repoName = displayName(delta.repo);
    const activityMs = repoActivityMs(state, delta);

    if (delta.error) {
      entries.push({
        id: `error:${delta.repo}:${delta.error.category}`,
        kind: "error",
        repo: delta.repo,
        repoName,
        timestampMs: activityMs,
        title: `Error ${delta.error.class} del repo`,
        detail: delta.error.message,
      });
    }

    if (dirty(delta)) {
      const signals = getRepoSignals(delta);
      entries.push({
        id: `activity:${delta.repo}:${delta.revision}`,
        kind: "activity",
        repo: delta.repo,
        repoName,
        timestampMs: activityMs,
        title: "Árbol de trabajo modificado",
        detail:
          signals.length > 0
            ? `${statusSummary(delta.status)} · ${signals.length} ${
                signals.length === 1 ? "señal pasiva" : "señales pasivas"
              }`
            : statusSummary(delta.status),
      });

      if (nowMs - activityMs >= ORPHAN_QUIET_MS) {
        entries.push({
          id: `orphan:${delta.repo}:${delta.revision}`,
          kind: "orphan",
          repo: delta.repo,
          repoName,
          timestampMs: activityMs,
          title: "Repo con cambios sin actividad reciente",
          detail: statusSummary(delta.status),
        });
      }
    }

    for (const event of getFsEvents(state, delta.repo)) {
      entries.push({
        id: `fs:${delta.repo}:${event.timestamp_ms}:${event.path}:${event.kind}`,
        kind: "fs-event",
        repo: delta.repo,
        repoName,
        timestampMs: event.timestamp_ms,
        title: `Archivo observado ${fsEventLabel(event.kind)}`,
        detail: event.path,
        path: event.path,
        fsKind: event.kind,
      });
    }
  }

  return entries.sort((a, b) => b.timestampMs - a.timestampMs || a.id.localeCompare(b.id));
}
