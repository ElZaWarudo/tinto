import type { RepoSource } from "../bus/contract";
import { isWslRepoSource } from "./repoSource";

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
      title={distro ? `WSL · ${distro}` : "Repositorio WSL"}
    >
      WSL
    </span>
  );
}
