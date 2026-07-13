import type { RepoSource } from "../bus/contract";

export function looksLikeWslRepoPath(path: string): boolean {
  return /^\/(?:home|tmp|mnt|var|opt|workspace|workspaces)\//.test(path) || path === "/";
}

export function isWslRepoSource(
  repo: string,
  source?: RepoSource | null,
  distro?: string | null,
): boolean {
  return source === "wsl" || !!distro || (source !== "local" && looksLikeWslRepoPath(repo));
}
