import type { RepoSource } from "../bus/contract";
import { isWindowsHost } from "../workbench/platform";

export function looksLikeWslRepoPath(path: string): boolean {
  return /^\/(?:home|tmp|mnt|var|opt|workspace|workspaces)\//.test(path) || path === "/";
}

export function isWslRepoSource(
  repo: string,
  source?: RepoSource | null,
  distro?: string | null,
): boolean {
  // A Linux path only hints at WSL on a Windows host; on Linux it is a local repo.
  return (
    source === "wsl" ||
    !!distro ||
    (source !== "local" && isWindowsHost() && looksLikeWslRepoPath(repo))
  );
}
