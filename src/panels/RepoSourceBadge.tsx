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

export function RepoSourceBadge({
  repo,
  source,
  distro,
  className,
}: {
  repo: string;
  source?: RepoSource | null;
  distro?: string | null;
  className?: string;
}) {
  if (!isWslRepoSource(repo, source, distro)) return null;
  const classes = ["repo-source-badge", className].filter(Boolean).join(" ");
  return (
    <span
      className={classes}
      data-testid="repo-source-badge"
      title={distro ? `WSL · ${distro}` : "WSL repo"}
    >
      WSL
    </span>
  );
}
