import type { RepoStatus } from "../bus/contract";

export type RepoChangeKind = "staged" | "modified" | "untracked";

interface RepoStatusMark {
  kind: RepoChangeKind;
  short: string;
  label: string;
  className: string;
  count: (status: RepoStatus) => number;
}

export const REPO_STATUS_MARKS: RepoStatusMark[] = [
  {
    kind: "modified",
    short: "M",
    label: "Modificado",
    className: "modified",
    count: (status) => status.modified.length,
  },
  {
    kind: "staged",
    short: "S",
    label: "Preparado",
    className: "staged",
    count: (status) => status.staged.length,
  },
  {
    kind: "untracked",
    short: "U",
    label: "Sin seguimiento",
    className: "untracked",
    count: (status) => status.untracked.length,
  },
];

const CHANGE_KIND_PRIORITY: RepoChangeKind[] = ["staged", "modified", "untracked"];

export function statusMarkForKind(kind: RepoChangeKind): RepoStatusMark {
  return REPO_STATUS_MARKS.find((mark) => mark.kind === kind)!;
}

export function changeKindForPath(status: RepoStatus, path: string): RepoChangeKind | null {
  for (const kind of CHANGE_KIND_PRIORITY) {
    if (statusPathList(status, kind).includes(path)) return kind;
  }
  return null;
}

function statusPathList(status: RepoStatus, kind: RepoChangeKind): string[] {
  switch (kind) {
    case "staged":
      return status.staged;
    case "modified":
      return status.modified;
    case "untracked":
      return status.untracked;
  }
}
