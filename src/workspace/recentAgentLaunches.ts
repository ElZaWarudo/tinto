const STORAGE_KEY = "tinto:recent-agent-launches:v1";
const MAX_RECENT_AGENT_LAUNCHES = 6;

export interface RecentAgentLaunch {
  repo: string;
  agentType: string;
  count: number;
  lastUsedAt: number;
}

interface AgentLaunchParams {
  repo?: string;
  agentType?: string;
}

export function readRecentAgentLaunches(): RecentAgentLaunch[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map(normalizeRecentAgentLaunch)
      .filter((launch): launch is RecentAgentLaunch => !!launch)
      .sort(sortLaunches)
      .slice(0, MAX_RECENT_AGENT_LAUNCHES);
  } catch {
    return [];
  }
}

export function markRecentAgentLaunch(params: AgentLaunchParams): void {
  if (!params.repo || !params.agentType) return;
  const now = Date.now();
  const existing = readRecentAgentLaunches();
  const key = launchKey(params.repo, params.agentType);
  const next = new Map(existing.map((launch) => [launchKey(launch.repo, launch.agentType), launch]));
  const current = next.get(key);
  next.set(key, {
    repo: params.repo,
    agentType: params.agentType,
    count: (current?.count ?? 0) + 1,
    lastUsedAt: now,
  });
  writeRecentAgentLaunches(Array.from(next.values()).sort(sortLaunches));
}

export function forgetRecentAgentLaunch(params: AgentLaunchParams): void {
  if (!params.repo || !params.agentType) return;
  const key = launchKey(params.repo, params.agentType);
  writeRecentAgentLaunches(
    readRecentAgentLaunches().filter(
      (launch) => launchKey(launch.repo, launch.agentType) !== key,
    ),
  );
}

export function clearRecentAgentLaunchesForTests(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

function writeRecentAgentLaunches(launches: RecentAgentLaunch[]): void {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(launches.slice(0, MAX_RECENT_AGENT_LAUNCHES)),
    );
  } catch {
    /* storage unavailable / quota - quick launches are optional */
  }
}

function normalizeRecentAgentLaunch(value: unknown): RecentAgentLaunch | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<RecentAgentLaunch>;
  if (typeof candidate.repo !== "string" || !candidate.repo) return null;
  if (typeof candidate.agentType !== "string" || !candidate.agentType) return null;
  return {
    repo: candidate.repo,
    agentType: candidate.agentType,
    count: typeof candidate.count === "number" && candidate.count > 0 ? candidate.count : 1,
    lastUsedAt:
      typeof candidate.lastUsedAt === "number" && candidate.lastUsedAt > 0
        ? candidate.lastUsedAt
        : 0,
  };
}

function sortLaunches(a: RecentAgentLaunch, b: RecentAgentLaunch): number {
  return b.lastUsedAt - a.lastUsedAt || a.repo.localeCompare(b.repo);
}

function launchKey(repo: string, agentType: string): string {
  return `${repo}\u0000${agentType}`;
}
